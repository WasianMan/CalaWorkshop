//! Async Steam login sessions.
//!
//! The old `POST /accounts/login` endpoint runs steamcmd synchronously inside
//! one HTTP request. That breaks down for Steam Guard *mobile confirmations*:
//! steamcmd sits waiting for the user to tap "Approve" on their phone, the
//! request has no way to tell the user that, and the old 120s hard timeout
//! killed logins that only needed a little patience. Sessions fix that: the
//! login runs in a background task, the UI polls its state, and the state
//! machine distinguishes "waiting for your phone" from "type a code" from
//! "failed".
//!
//! Two session kinds share the same lifecycle:
//! - **password**: streaming steamcmd login (`+login user pass [code]`) that
//!   reports `awaiting_mobile_confirmation` the moment steamcmd asks for an
//!   in-app approval, and waits up to 5 minutes for it.
//! - **qr**: a Steam QR auth session (see `steamauth`). After the user approves
//!   the scan, the issued refresh token is handed to steamcmd in place of a
//!   password. That handoff is *verified* — if the local steamcmd build refuses
//!   token sign-ins, the session fails with `error_kind = "qr_unsupported"` and
//!   the UI falls back to the password flow. Nothing is persisted unless the
//!   usual passwordless cached-session verification passes.
//!
//! Sessions are in-memory only and garbage-collected; secrets (password /
//! refresh token) live only inside the spawned task for the duration of the
//! attempt and are never stored on the session object.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::json;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::state::AppState;
use crate::steamauth;
use crate::steamcmd::{self, LoginOutcome};

/// How long a QR session may sit unapproved before it expires. Steam rotates
/// the challenge every ~30s and keeps the underlying session alive well past
/// this; the cap is ours so abandoned sessions don't poll forever.
const QR_SESSION_TIMEOUT: Duration = Duration::from_secs(300);
/// Terminal sessions are kept around this long so the UI can read the outcome.
const TERMINAL_TTL: Duration = Duration::from_secs(600);
/// Absolute cap on any session's lifetime (safety net for stuck tasks).
const SESSION_MAX_AGE: Duration = Duration::from_secs(1200);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Running,
    AwaitingMobileConfirmation,
    AwaitingQr,
    NeedsGuard,
    Verifying,
    Ok,
    Failed,
}

impl Phase {
    fn as_str(self) -> &'static str {
        match self {
            Phase::Running => "running",
            Phase::AwaitingMobileConfirmation => "awaiting_mobile_confirmation",
            Phase::AwaitingQr => "awaiting_qr",
            Phase::NeedsGuard => "needs_guard",
            Phase::Verifying => "verifying",
            Phase::Ok => "ok",
            Phase::Failed => "failed",
        }
    }

    fn terminal(self) -> bool {
        matches!(self, Phase::NeedsGuard | Phase::Ok | Phase::Failed)
    }
}

/// Mutable session state shared between the worker task and the HTTP handlers.
#[derive(Debug)]
struct SessionData {
    method: &'static str, // "password" | "qr"
    phase: Phase,
    username: Option<String>,
    error: Option<String>,
    /// Machine-readable failure class for the UI:
    /// `invalid_credentials`, `rate_limited`, `connectivity`, `timeout`,
    /// `expired`, `qr_unsupported`, `internal`.
    error_kind: Option<String>,
    /// For `needs_guard`: where the code most likely is (`email` | `device`).
    guard_hint: Option<String>,
    challenge_url: Option<String>,
    qr_svg: Option<String>,
}

struct Entry {
    label: String,
    created: Instant,
    data: Arc<Mutex<SessionData>>,
    handle: JoinHandle<()>,
}

/// Registry of in-flight and recently-finished login sessions.
#[derive(Clone, Default)]
pub struct LoginSessions {
    inner: Arc<Mutex<HashMap<Uuid, Entry>>>,
}

impl LoginSessions {
    /// Drop expired sessions and abort any still-running session for `label`
    /// (a new attempt for the same account supersedes the old one — two
    /// steamcmd logins against one session dir would corrupt it).
    fn reap_and_supersede(&self, label: Option<&str>) {
        let mut map = self.inner.lock().unwrap();
        map.retain(|_, entry| {
            let phase = entry.data.lock().unwrap().phase;
            let age = entry.created.elapsed();
            let keep = if phase.terminal() {
                age < TERMINAL_TTL
            } else {
                age < SESSION_MAX_AGE
            };
            if !keep {
                entry.handle.abort();
            }
            keep
        });
        if let Some(label) = label {
            for entry in map.values() {
                if entry.label == label {
                    let mut data = entry.data.lock().unwrap();
                    if !data.phase.terminal() {
                        entry.handle.abort();
                        data.phase = Phase::Failed;
                        data.error = Some("superseded by a newer login attempt".to_string());
                        data.error_kind = Some("superseded".to_string());
                    }
                }
            }
        }
    }

    fn insert(&self, id: Uuid, entry: Entry) {
        self.inner.lock().unwrap().insert(id, entry);
    }

    /// JSON view of a session (used for both the 202 create response and polls).
    pub fn view(&self, id: &Uuid) -> Option<serde_json::Value> {
        let map = self.inner.lock().unwrap();
        let entry = map.get(id)?;
        let data = entry.data.lock().unwrap();
        Some(json!({
            "id": id,
            "label": entry.label,
            "method": data.method,
            "state": data.phase.as_str(),
            "username": data.username,
            "error": data.error,
            "error_kind": data.error_kind,
            "guard_hint": data.guard_hint,
            "challenge_url": data.challenge_url,
            "qr_svg": data.qr_svg,
            "verified": data.phase == Phase::Ok,
        }))
    }

    /// Abort and remove a session. Returns false when the id is unknown.
    pub fn cancel(&self, id: &Uuid) -> bool {
        let mut map = self.inner.lock().unwrap();
        match map.remove(id) {
            Some(entry) => {
                entry.handle.abort();
                true
            }
            None => false,
        }
    }

    /// Start a password login session. Returns the new session id.
    pub fn begin_password(
        &self,
        state: &AppState,
        label: String,
        username: String,
        password: String,
        guard_code: Option<String>,
    ) -> Uuid {
        self.reap_and_supersede(Some(&label));

        let id = Uuid::new_v4();
        let data = Arc::new(Mutex::new(SessionData {
            method: "password",
            phase: Phase::Running,
            username: Some(username.clone()),
            error: None,
            error_kind: None,
            guard_hint: None,
            challenge_url: None,
            qr_svg: None,
        }));

        let task_data = data.clone();
        let task_state = state.clone();
        let task_label = label.clone();
        let handle = tokio::spawn(async move {
            run_password_session(
                task_state, task_label, username, password, guard_code, task_data,
            )
            .await;
        });

        self.insert(
            id,
            Entry {
                label,
                created: Instant::now(),
                data,
                handle,
            },
        );
        id
    }

    /// Start a QR login session. The Begin call runs inline so the response to
    /// the create request already carries a scannable QR code.
    pub async fn begin_qr(&self, state: &AppState, label: String) -> anyhow::Result<Uuid> {
        self.reap_and_supersede(Some(&label));

        let qr = steamauth::begin_qr(&state.http, "CalaWorkshop (Calagopus panel)").await?;
        let svg = steamauth::challenge_qr_svg(&qr.challenge_url)?;

        let id = Uuid::new_v4();
        let data = Arc::new(Mutex::new(SessionData {
            method: "qr",
            phase: Phase::AwaitingQr,
            username: None,
            error: None,
            error_kind: None,
            guard_hint: None,
            challenge_url: Some(qr.challenge_url.clone()),
            qr_svg: Some(svg),
        }));

        let task_data = data.clone();
        let task_state = state.clone();
        let task_label = label.clone();
        let handle = tokio::spawn(async move {
            run_qr_session(task_state, task_label, qr, task_data).await;
        });

        self.insert(
            id,
            Entry {
                label,
                created: Instant::now(),
                data,
                handle,
            },
        );
        Ok(id)
    }
}

fn set_phase(data: &Arc<Mutex<SessionData>>, phase: Phase) {
    data.lock().unwrap().phase = phase;
}

fn fail(data: &Arc<Mutex<SessionData>>, kind: &str, message: impl Into<String>) {
    let mut d = data.lock().unwrap();
    d.phase = Phase::Failed;
    d.error_kind = Some(kind.to_string());
    d.error = Some(message.into());
}

/// Shared tail of both flows: steamcmd reported a successful login, so prove
/// the cached session actually works without credentials, then persist the
/// username so downloads can reuse it.
async fn verify_and_persist(
    state: &AppState,
    label: &str,
    username: &str,
    data: &Arc<Mutex<SessionData>>,
) {
    set_phase(data, Phase::Verifying);
    let workdir = state.config.steam_dir(label);
    if let Err(e) = steamcmd::verify_cached_login(&state.config, &workdir, username).await {
        fail(
            data,
            "internal",
            format!("Steam accepted the login, but the cached session did not verify: {e:#}"),
        );
        return;
    }
    if let Err(e) = steamcmd::write_account_meta(&workdir, username).await {
        fail(
            data,
            "internal",
            format!("could not persist account metadata: {e:#}"),
        );
        return;
    }
    set_phase(data, Phase::Ok);
}

async fn run_password_session(
    state: AppState,
    label: String,
    username: String,
    password: String,
    guard_code: Option<String>,
    data: Arc<Mutex<SessionData>>,
) {
    let workdir = state.config.steam_dir(&label);
    let waiting_data = data.clone();
    let result = steamcmd::login_interactive(
        &state.config,
        &workdir,
        &username,
        &password,
        guard_code.as_deref(),
        move || set_phase(&waiting_data, Phase::AwaitingMobileConfirmation),
    )
    .await;

    match result {
        Ok((LoginOutcome::Ok, _)) => verify_and_persist(&state, &label, &username, &data).await,
        Ok((LoginOutcome::NeedsGuard, output)) => {
            let hint = if output.to_lowercase().contains("email") {
                "email"
            } else {
                "device"
            };
            let mut d = data.lock().unwrap();
            d.phase = Phase::NeedsGuard;
            d.guard_hint = Some(hint.to_string());
        }
        Ok((LoginOutcome::RateLimited, _)) => fail(
            &data,
            "rate_limited",
            "Steam is rate-limiting login attempts from this server. Wait a few minutes before trying again.",
        ),
        Ok((LoginOutcome::InvalidCredentials, _)) => {
            fail(&data, "invalid_credentials", "Steam rejected the credentials.")
        }
        Ok((LoginOutcome::ConnectivityFailed(message), _)) => fail(&data, "connectivity", message),
        Err(e) => {
            let waiting = data.lock().unwrap().phase == Phase::AwaitingMobileConfirmation;
            let kind = if waiting { "timeout" } else { "internal" };
            let msg = if waiting {
                "The Steam Mobile app approval was not received in time. Start the login again and approve promptly, or enter the Steam Guard code instead.".to_string()
            } else {
                format!("{e:#}")
            };
            fail(&data, kind, msg);
        }
    }
}

async fn run_qr_session(
    state: AppState,
    label: String,
    mut qr: steamauth::QrSession,
    data: Arc<Mutex<SessionData>>,
) {
    let deadline = Instant::now() + QR_SESSION_TIMEOUT;
    let approved = loop {
        if Instant::now() >= deadline {
            fail(
                &data,
                "expired",
                "The QR code was not scanned and approved in time. Start again to get a fresh code.",
            );
            return;
        }
        tokio::time::sleep(Duration::from_secs(qr.interval.max(2))).await;

        match steamauth::poll_qr(&state.http, qr.client_id, &qr.request_id).await {
            Ok(steamauth::QrPoll::Pending) => continue,
            Ok(steamauth::QrPoll::NewChallenge {
                client_id,
                challenge_url,
            }) => {
                qr.client_id = client_id;
                match steamauth::challenge_qr_svg(&challenge_url) {
                    Ok(svg) => {
                        let mut d = data.lock().unwrap();
                        d.challenge_url = Some(challenge_url);
                        d.qr_svg = Some(svg);
                    }
                    Err(e) => tracing::warn!(error = %format!("{e:#}"), "QR re-render failed"),
                }
            }
            Ok(steamauth::QrPoll::Approved {
                account_name,
                refresh_token,
            }) => break (account_name, refresh_token),
            Ok(steamauth::QrPoll::Gone(message)) => {
                fail(&data, "expired", message);
                return;
            }
            Err(e) => {
                // Transient poll error: log and keep trying until the deadline.
                tracing::warn!(error = %format!("{e:#}"), "QR poll failed; retrying");
            }
        }
    };

    let (account_name, refresh_token) = approved;
    {
        let mut d = data.lock().unwrap();
        d.username = Some(account_name.clone());
        d.challenge_url = None;
        d.qr_svg = None;
        d.phase = Phase::Verifying;
    }

    // The handoff experiment: modern steamcmd accepts a Steam-issued refresh
    // token in place of the password. This is verified, not assumed — if this
    // build of steamcmd refuses it, we say so and the UI falls back to the
    // password flow. No mobile confirmation can occur here (the approval
    // already happened on the phone), so the waiting callback is a no-op.
    let workdir = state.config.steam_dir(&label);
    let result = steamcmd::login_interactive(
        &state.config,
        &workdir,
        &account_name,
        &refresh_token,
        None,
        || {},
    )
    .await;

    match result {
        Ok((LoginOutcome::Ok, _)) => verify_and_persist(&state, &label, &account_name, &data).await,
        Ok((LoginOutcome::ConnectivityFailed(message), _)) => fail(&data, "connectivity", message),
        Ok((LoginOutcome::RateLimited, _)) => fail(
            &data,
            "rate_limited",
            "Steam is rate-limiting login attempts from this server. Wait a few minutes and try again.",
        ),
        Ok((LoginOutcome::InvalidCredentials | LoginOutcome::NeedsGuard, _)) => fail(
            &data,
            "qr_unsupported",
            "The QR sign-in was approved, but this SteamCMD build did not accept the Steam-issued \
             sign-in token. Use the password method instead — the QR approval has not harmed anything.",
        ),
        Err(e) => fail(&data, "internal", format!("{e:#}")),
    }
}
