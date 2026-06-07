//! HTTP routing, auth, and request handlers.

use std::path::Path;

use axum::{
    body::Body,
    extract::{Path as AxumPath, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::state::{AppState, Job, JobState};
use crate::steamcmd::{self, LoginOutcome};

/// Build the application router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/download", post(post_download))
        .route("/health", get(get_health))
        .route("/diagnostics/steamcmd", get(get_steamcmd_check))
        .route("/jobs/:id", get(get_job))
        .route("/files/:id", get(get_file))
        .route("/accounts", get(list_accounts))
        .route("/accounts/login", post(post_login))
        .route("/accounts/:label", delete(delete_account))
        .with_state(state)
}

async fn get_health(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Response> {
    check_bearer(&headers, &state.config.token)?;
    Ok(Json(json!({ "ok": true })).into_response())
}

async fn get_steamcmd_check(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    check_bearer(&headers, &state.config.token)?;
    match steamcmd::connectivity_check(&state.config).await {
        Ok(message) => Ok(Json(json!({ "ok": true, "message": message })).into_response()),
        Err(err) => Ok((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "ok": false, "error": format!("{err:#}") })),
        )
            .into_response()),
    }
}

// ---------------------------------------------------------------------------
// Error helper
// ---------------------------------------------------------------------------

/// A JSON error body `{ "error": "..." }` plus a status code.
struct ApiError(StatusCode, String);

impl ApiError {
    fn new(status: StatusCode, msg: impl Into<String>) -> Self {
        Self(status, msg.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

type ApiResult<T> = Result<T, ApiError>;

/// Validate the `Authorization: Bearer <token>` header against the configured token.
fn check_bearer(headers: &HeaderMap, expected: &str) -> ApiResult<()> {
    let provided = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim);

    match provided {
        Some(t) if t == expected => Ok(()),
        _ => Err(ApiError::new(StatusCode::UNAUTHORIZED, "invalid token")),
    }
}

// ---------------------------------------------------------------------------
// POST /download
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct DownloadRequest {
    app_id: u64,
    workshop_id: u64,
    #[serde(default)]
    account: Option<String>,
    #[serde(default)]
    archive: bool,
    #[serde(default)]
    title_slug: Option<String>,
    /// Data-driven file selection/rename rule resolved by the extension from the
    /// game preset. Absent or empty `match` ⇒ mirror every downloaded file.
    #[serde(default)]
    install_rule: crate::steamcmd::InstallRule,
}

#[derive(Debug, Serialize)]
struct DownloadResponse {
    id: Uuid,
    state: JobState,
    file_token: String,
}

async fn post_download(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<DownloadRequest>,
) -> ApiResult<Response> {
    check_bearer(&headers, &state.config.token)?;

    // Sanity: positive integers (serde already rejects negatives for u64, but a
    // zero id is never valid for a Steam app/workshop item).
    if req.app_id == 0 {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "app_id must be a positive integer",
        ));
    }
    if req.workshop_id == 0 {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "workshop_id must be a positive integer",
        ));
    }

    let id = Uuid::new_v4();
    let file_token = random_token();

    let job = Job {
        id,
        state: JobState::Queued,
        app_id: req.app_id,
        workshop_id: req.workshop_id,
        file_name: None,
        files: Vec::new(),
        file_token: file_token.clone(),
        size: None,
        error: None,
    };
    state.insert_job(job).await;

    // Spawn the actual download off the request path.
    let bg_state = state.clone();
    let account = req.account.clone();
    let archive = req.archive;
    let title_slug = req.title_slug.clone();
    let app_id = req.app_id;
    let workshop_id = req.workshop_id;
    let install_rule = req.install_rule.clone();
    tokio::spawn(async move {
        run_download(
            bg_state,
            id,
            account,
            archive,
            title_slug,
            app_id,
            workshop_id,
            install_rule,
        )
        .await;
    });

    let body = DownloadResponse {
        id,
        state: JobState::Queued,
        file_token,
    };
    Ok((StatusCode::ACCEPTED, Json(body)).into_response())
}

/// The background worker that drives a single job to `ready` or `failed`.
#[allow(clippy::too_many_arguments)]
async fn run_download(
    state: AppState,
    id: Uuid,
    account: Option<String>,
    archive: bool,
    title_slug: Option<String>,
    app_id: u64,
    workshop_id: u64,
    install_rule: crate::steamcmd::InstallRule,
) {
    state
        .update_job(&id, |j| j.state = JobState::Downloading)
        .await;

    let result = do_download(
        &state,
        id,
        account,
        archive,
        title_slug,
        app_id,
        workshop_id,
        install_rule,
    )
    .await;

    match result {
        Ok((file_name, files, size)) => {
            state
                .update_job(&id, |j| {
                    j.state = JobState::Ready;
                    j.file_name = Some(file_name);
                    j.files = files;
                    j.size = Some(size);
                })
                .await;
            tracing::info!(%id, "job ready");
        }
        Err(e) => {
            let msg = format!("{e:#}");
            tracing::warn!(%id, error = %msg, "job failed");
            state
                .update_job(&id, |j| {
                    j.state = JobState::Failed;
                    j.error = Some(msg);
                })
                .await;
        }
    }
}

/// Core download → artifact-selection pipeline. Returns `(file_name, size)`.
#[allow(clippy::too_many_arguments)]
async fn do_download(
    state: &AppState,
    id: Uuid,
    account: Option<String>,
    archive: bool,
    title_slug: Option<String>,
    app_id: u64,
    workshop_id: u64,
    install_rule: crate::steamcmd::InstallRule,
) -> anyhow::Result<(String, Vec<String>, u64)> {
    let config = &state.config;

    let label = account.as_deref().unwrap_or("anonymous");
    let workdir = config.steam_dir(label);

    // For an account label, reuse the stored username (+ cached session). For
    // anonymous, pass no username.
    let username: Option<String> = if account.is_some() {
        steamcmd::read_account_username(&workdir).await
    } else {
        None
    };
    if account.is_some() && username.is_none() {
        anyhow::bail!("no cached session for account '{label}' — call POST /accounts/login first");
    }

    let content =
        steamcmd::download_item(config, &workdir, username.as_deref(), app_id, workshop_id).await?;

    // Prepare the job artifact dir.
    let job_dir = config.job_dir(&id);
    tokio::fs::create_dir_all(&job_dir).await?;

    if archive {
        // Mode 1: zip the whole downloaded folder verbatim.
        let file_name = "archive.zip".to_string();
        let dest = job_dir.join(&file_name);
        let size = steamcmd::zip_folder(content, dest).await?;
        Ok((file_name.clone(), vec![file_name], size))
    } else {
        // Mode 2/3: select + map files per the resolved install rule (empty rule
        // mirrors everything). The transfer is always a zip the extension then
        // pulls + decompresses into the volume.
        let mapped = steamcmd::apply_install_rule(
            &content,
            app_id,
            workshop_id,
            title_slug.as_deref(),
            &install_rule,
        )
        .await?;
        if mapped.is_empty() {
            anyhow::bail!("the install rule matched no files in the downloaded content");
        }
        let file_name = format!("workshop_{workshop_id}.zip");
        let dest = job_dir.join(&file_name);
        steamcmd::zip_selected_files(&content, &mapped, &dest).await?;
        let size = tokio::fs::metadata(&dest).await?.len();
        let files = mapped
            .iter()
            .map(|entry| entry.install_name().to_string())
            .collect();
        Ok((file_name, files, size))
    }
}

// ---------------------------------------------------------------------------
// GET /jobs/{id}
// ---------------------------------------------------------------------------

async fn get_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<Uuid>,
) -> ApiResult<Response> {
    check_bearer(&headers, &state.config.token)?;
    match state.get_job(&id).await {
        Some(job) => Ok(Json(job).into_response()),
        None => Err(ApiError::new(StatusCode::NOT_FOUND, "unknown job")),
    }
}

// ---------------------------------------------------------------------------
// GET /files/{id}?token=...   (no bearer; token query instead)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct FileQuery {
    #[serde(default)]
    token: String,
}

async fn get_file(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
    Query(q): Query<FileQuery>,
) -> ApiResult<Response> {
    let job = state
        .get_job(&id)
        .await
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown job"))?;

    // Constant-ish comparison; tokens are random 32-byte url-safe strings.
    if q.token.is_empty() || q.token != job.file_token {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "invalid file token"));
    }

    if job.state != JobState::Ready {
        return Err(ApiError::new(StatusCode::CONFLICT, "job is not ready"));
    }

    let file_name = job
        .file_name
        .clone()
        .ok_or_else(|| ApiError::new(StatusCode::CONFLICT, "job has no artifact"))?;
    let path = state.config.job_dir(&id).join(&file_name);

    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|_| ApiError::new(StatusCode::NOT_FOUND, "artifact missing on disk"))?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let disposition = format!("attachment; filename=\"{}\"", sanitize_filename(&file_name));
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_DISPOSITION, disposition)
        .body(body)
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(response)
}

// ---------------------------------------------------------------------------
// Accounts
// ---------------------------------------------------------------------------

async fn list_accounts(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Response> {
    check_bearer(&headers, &state.config.token)?;

    let steam_root = state.config.data_dir.join("steam");
    let mut accounts: Vec<serde_json::Value> = Vec::new();

    if let Ok(mut rd) = tokio::fs::read_dir(&steam_root).await {
        while let Ok(Some(entry)) = rd.next_entry().await {
            let Ok(ft) = entry.file_type().await else {
                continue;
            };
            if !ft.is_dir() {
                continue;
            }
            let label = entry.file_name().to_string_lossy().to_string();
            if label == "anonymous" {
                continue;
            }
            // "valid" is best-effort: we have a stored username (i.e. a login was
            // attempted) — we cannot cheaply verify session freshness without
            // invoking steamcmd, so this reflects "linked", not "session live".
            let valid = steamcmd::read_account_username(&entry.path())
                .await
                .is_some();
            accounts.push(json!({ "label": label, "valid": valid }));
        }
    }

    Ok(Json(json!({ "accounts": accounts })).into_response())
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    label: String,
    username: String,
    password: String,
    #[serde(default)]
    guard_code: Option<String>,
}

async fn post_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> ApiResult<Response> {
    check_bearer(&headers, &state.config.token)?;

    if req.label.trim().is_empty() || !is_safe_label(&req.label) {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "invalid label"));
    }

    let workdir = state.config.steam_dir(&req.label);
    let outcome = steamcmd::login(
        &state.config,
        &workdir,
        &req.username,
        &req.password,
        req.guard_code.as_deref(),
    )
    .await
    .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")))?;

    match outcome {
        LoginOutcome::Ok => {
            steamcmd::verify_cached_login(&state.config, &workdir, &req.username)
                .await
                .map_err(|e| {
                    ApiError::new(
                        StatusCode::SERVICE_UNAVAILABLE,
                        format!(
                            "Steam login succeeded, but cached-session verification failed: {e:#}"
                        ),
                    )
                })?;
            // Persist the username (never the password) so account downloads can
            // reuse the cached session later.
            steamcmd::write_account_meta(&workdir, &req.username)
                .await
                .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")))?;
            Ok(Json(json!({ "state": "ok", "verified": true })).into_response())
        }
        LoginOutcome::NeedsGuard => Ok((
            StatusCode::CONFLICT,
            Json(json!({ "state": "needs_guard" })),
        )
            .into_response()),
        LoginOutcome::InvalidCredentials => Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid credentials",
        )),
        LoginOutcome::ConnectivityFailed(message) => {
            Err(ApiError::new(StatusCode::SERVICE_UNAVAILABLE, message))
        }
    }
}

async fn delete_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(label): AxumPath<String>,
) -> ApiResult<Response> {
    check_bearer(&headers, &state.config.token)?;

    if !is_safe_label(&label) {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "invalid label"));
    }

    let workdir = state.config.steam_dir(&label);
    if !workdir.exists() {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "unknown account"));
    }
    tokio::fs::remove_dir_all(&workdir)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")))?;

    Ok(StatusCode::NO_CONTENT.into_response())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Generate a 32-byte url-safe random token (base64url, no padding).
fn random_token() -> String {
    use rand::RngCore;
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes
        .iter()
        .map(|b| ALPHABET[(*b as usize) % ALPHABET.len()] as char)
        .collect()
}

/// Reject labels that could escape the steam dir or contain quotes/control chars.
fn is_safe_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 64
        && label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        && label != "."
        && label != ".."
}

/// Strip anything that would break a quoted `Content-Disposition` filename.
fn sanitize_filename(name: &str) -> String {
    let base = Path::new(name)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| name.to_string());
    base.chars()
        .filter(|c| *c != '"' && *c != '\\' && !c.is_control())
        .collect()
}
