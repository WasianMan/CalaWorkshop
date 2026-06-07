//! SteamCMD invocation, output parsing, and artifact selection.
//!
//! This module is deliberately free of any HTTP / axum types so the download
//! pipeline can be unit-reasoned in isolation.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use globset::{Glob, GlobSetBuilder};
use serde::Deserialize;
use tokio::process::Command;

use crate::config::Config;

/// Safety ceilings for an `InstallRule` so a malicious or buggy preset can't make
/// the helper select an unbounded number of files or emit huge paths.
const MAX_RULES: usize = 64;
const MAX_MATCHED_FILES: usize = 4096;
const MAX_DEST_LEN: usize = 512;

/// Hard ceilings so a stuck steamcmd (e.g. a blocked socket retrying, or a Steam
/// Guard prompt it can never satisfy non-interactively) can't hang a request or
/// a worker forever. On elapse the child is dropped and killed (`kill_on_drop`).
const CONNECTIVITY_TIMEOUT: Duration = Duration::from_secs(90);
const LOGIN_TIMEOUT: Duration = Duration::from_secs(120);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(3600);

/// Global steamcmd flags that keep it from ever blocking on an interactive
/// prompt: never ask for a password at the TTY, and bail on the first failed
/// command instead of dropping into the interactive shell.
fn apply_noninteractive_flags(cmd: &mut Command) {
    cmd.arg("+@ShutdownOnFailedCommand")
        .arg("1")
        .arg("+@NoPromptForPassword")
        .arg("1");
}

/// Keep SteamCMD's auth/sentry/config files inside the account workdir. SteamCMD
/// consults the process home for parts of its login cache, so `current_dir` and
/// `+force_install_dir` alone are not enough for reliable per-account reuse.
fn apply_steam_home(cmd: &mut Command, workdir: &Path) {
    cmd.env("HOME", workdir)
        .env("USER", "steam")
        .env("LOGNAME", "steam")
        .env("XDG_DATA_HOME", workdir.join(".local").join("share"))
        .env("XDG_CONFIG_HOME", workdir.join(".config"))
        .env("XDG_CACHE_HOME", workdir.join(".cache"));
}

/// Run a prepared steamcmd command with a timeout, returning its captured output.
async fn run_steamcmd(
    mut cmd: Command,
    timeout: Duration,
    bin: &str,
) -> Result<std::process::Output> {
    match tokio::time::timeout(timeout, cmd.output()).await {
        Ok(result) => result.with_context(|| format!("spawning steamcmd ({bin})")),
        Err(_) => bail!(
            "steamcmd timed out after {}s (no response — check the SteamCMD connectivity diagnostic)",
            timeout.as_secs()
        ),
    }
}

/// Outcome of a steamcmd login attempt (used by `POST /accounts/login`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginOutcome {
    /// Session established / refreshed successfully.
    Ok,
    /// SteamCMD asked for a Steam Guard code — caller must re-try with one.
    NeedsGuard,
    /// Credentials were rejected.
    InvalidCredentials,
    /// SteamCMD could not reach Steam, so credentials were not tested.
    ConnectivityFailed(String),
}

/// Persisted, non-secret metadata for a linked account label.
///
/// We store ONLY the username (never the password). SteamCMD keeps its own
/// session/sentry cache inside the account's working dir; on a later download we
/// re-run steamcmd in that same dir with `+login <username>` and rely on that
/// cached session. If the session has expired, steamcmd will fail and the user
/// must `POST /accounts/login` again. See README "Steam Guard caveat".
const ACCOUNT_META_FILE: &str = "account.json";

/// Run steamcmd to download a workshop item and return the absolute path to the
/// content folder `<workdir>/steamapps/workshop/content/<app_id>/<workshop_id>`.
///
/// `username` is `None` for anonymous downloads. For account downloads we pass
/// `+login <username>` and depend on the cached session in `workdir`.
pub async fn download_item(
    config: &Config,
    workdir: &Path,
    username: Option<&str>,
    app_id: u64,
    workshop_id: u64,
) -> Result<PathBuf> {
    tokio::fs::create_dir_all(workdir)
        .await
        .with_context(|| format!("creating steam workdir {}", workdir.display()))?;

    let login_user = username.unwrap_or("anonymous");

    // steamcmd +@... +force_install_dir <dir> +login <user> +workshop_download_item <app> <id> +quit
    let mut cmd = Command::new(&config.steamcmd_bin);
    apply_noninteractive_flags(&mut cmd);
    apply_steam_home(&mut cmd, workdir);
    cmd.arg("+force_install_dir")
        .arg(workdir)
        .arg("+login")
        .arg(login_user)
        .arg("+workshop_download_item")
        .arg(app_id.to_string())
        .arg(workshop_id.to_string())
        .arg("+quit")
        .current_dir(workdir)
        .kill_on_drop(true);

    let output = run_steamcmd(cmd, DOWNLOAD_TIMEOUT, &config.steamcmd_bin).await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    tracing::debug!(%stdout, %stderr, "steamcmd finished");

    let content = content_folder(workdir, app_id, workshop_id);

    // Success = explicit success line OR a populated content folder. SteamCMD's
    // exit code is unreliable, so we treat the filesystem as the source of truth.
    let success_line =
        stdout.contains("Success. Downloaded item") || stderr.contains("Success. Downloaded item");
    let has_content = folder_has_file(&content).await;

    if success_line || has_content {
        if !has_content {
            // steamcmd claimed success but nothing landed — surface that.
            bail!(
                "steamcmd reported success but content folder {} is empty",
                content.display()
            );
        }
        return Ok(content);
    }

    // Failed: extract the most relevant line for a concise error.
    let reason = extract_error_line(&stdout)
        .or_else(|| extract_error_line(&stderr))
        .unwrap_or_else(|| "steamcmd did not download the item".to_string());
    Err(anyhow!(reason))
}

/// Attempt a login to establish/refresh a cached session in `workdir`.
///
/// Steam Guard is best-effort: we feed `guard_code` as the optional 3rd `+login`
/// arg when provided and parse stdout for the "needs code" signal otherwise.
pub async fn login(
    config: &Config,
    workdir: &Path,
    username: &str,
    password: &str,
    guard_code: Option<&str>,
) -> Result<LoginOutcome> {
    tokio::fs::create_dir_all(workdir)
        .await
        .with_context(|| format!("creating steam workdir {}", workdir.display()))?;

    let mut cmd = Command::new(&config.steamcmd_bin);
    apply_noninteractive_flags(&mut cmd);
    apply_steam_home(&mut cmd, workdir);
    cmd.arg("+force_install_dir")
        .arg(workdir)
        .arg("+login")
        .arg(username)
        .arg(password);
    if let Some(code) = guard_code {
        // SteamCMD accepts the Guard code as the 3rd positional arg to +login.
        cmd.arg(code);
    }
    cmd.arg("+quit").current_dir(workdir).kill_on_drop(true);

    let output = run_steamcmd(cmd, LOGIN_TIMEOUT, &config.steamcmd_bin).await?;

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    tracing::debug!(%combined, "steamcmd login finished");

    parse_login_outcome(&combined)
}

/// Verify that the just-created cached session is usable without resending the
/// password or Steam Guard code.
pub async fn verify_cached_login(config: &Config, workdir: &Path, username: &str) -> Result<()> {
    tokio::fs::create_dir_all(workdir)
        .await
        .with_context(|| format!("creating steam workdir {}", workdir.display()))?;

    let mut cmd = Command::new(&config.steamcmd_bin);
    apply_noninteractive_flags(&mut cmd);
    apply_steam_home(&mut cmd, workdir);
    cmd.arg("+force_install_dir")
        .arg(workdir)
        .arg("+login")
        .arg(username)
        .arg("+quit")
        .current_dir(workdir)
        .kill_on_drop(true);

    let output = run_steamcmd(cmd, LOGIN_TIMEOUT, &config.steamcmd_bin).await?;
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    tracing::debug!(%combined, "steamcmd cached-session verification finished");

    match parse_login_outcome(&combined)? {
        LoginOutcome::Ok => Ok(()),
        LoginOutcome::NeedsGuard => bail!("cached SteamCMD session still requires Steam Guard"),
        LoginOutcome::InvalidCredentials => bail!("cached SteamCMD session was not accepted"),
        LoginOutcome::ConnectivityFailed(message) => bail!(message),
    }
}

fn parse_login_outcome(combined: &str) -> Result<LoginOutcome> {
    // Detection is heuristic — steamcmd's wording varies across versions. We look
    // for the common Steam Guard prompts. Note: in a non-interactive process
    // steamcmd cannot actually *prompt*, so a fresh Guard-protected account will
    // emit one of these and exit; the caller then re-POSTs with `guard_code`.
    let lower = combined.to_lowercase();
    if combined.contains("CreateBoundSocket") || lower.contains("no connection") {
        return Ok(LoginOutcome::ConnectivityFailed(
            extract_error_line(combined)
                .unwrap_or_else(|| "steamcmd connectivity failed".to_string()),
        ));
    }

    // Success must be checked BEFORE the Steam Guard prompt: a Steam Guard mobile
    // authenticator login that the user confirms on their phone prints both the
    // "protected by a Steam Guard mobile authenticator" notice AND the success
    // lines, so checking guard first would misreport a completed login as
    // needing a code (and the caller would never persist the session).
    if login_output_indicates_success(combined) {
        return Ok(LoginOutcome::Ok);
    }

    if lower.contains("steam guard")
        || lower.contains("two-factor")
        || lower.contains("guard code")
        || lower.contains("need two factor")
    {
        return Ok(LoginOutcome::NeedsGuard);
    }

    if lower.contains("invalid password")
        || lower.contains("login failure")
        || lower.contains("account logon denied")
        || lower.contains("invalid login")
    {
        return Ok(LoginOutcome::InvalidCredentials);
    }

    // Ambiguous output: be conservative and treat as invalid credentials so the
    // caller doesn't believe a session exists when it may not.
    Ok(LoginOutcome::InvalidCredentials)
}

fn login_output_indicates_success(combined: &str) -> bool {
    combined.contains("Logged in OK")
        || combined.contains("Waiting for user info...OK")
        || combined.lines().any(|line| {
            let lower = line.to_lowercase();
            lower.contains("logging in user")
                && lower.contains("to steam public")
                && lower.trim_end().ends_with("ok")
        })
        || combined.lines().any(|line| {
            let lower = line.to_lowercase();
            lower.starts_with("waiting for user info") && lower.trim_end().ends_with("ok")
        })
}

pub async fn connectivity_check(config: &Config) -> Result<String> {
    let workdir = config.steam_dir("diagnostics");
    tokio::fs::create_dir_all(&workdir)
        .await
        .with_context(|| format!("creating steam workdir {}", workdir.display()))?;

    let mut cmd = Command::new(&config.steamcmd_bin);
    apply_noninteractive_flags(&mut cmd);
    apply_steam_home(&mut cmd, &workdir);
    cmd.arg("+force_install_dir")
        .arg(&workdir)
        .arg("+login")
        .arg("anonymous")
        .arg("+quit")
        .current_dir(&workdir)
        .kill_on_drop(true);
    let output = run_steamcmd(cmd, CONNECTIVITY_TIMEOUT, &config.steamcmd_bin).await?;

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let lower = combined.to_lowercase();
    if combined.contains("CreateBoundSocket") || lower.contains("no connection") {
        bail!(extract_error_line(&combined)
            .unwrap_or_else(|| "steamcmd connectivity failed".to_string()));
    }
    if combined.contains("Logged in OK") || combined.contains("Waiting for user info...OK") {
        return Ok("anonymous login ok".to_string());
    }
    if output.status.success() {
        return Ok("steamcmd exited successfully".to_string());
    }
    bail!(
        extract_error_line(&combined).unwrap_or_else(|| "steamcmd connectivity failed".to_string())
    )
}

/// Path to a workshop item's content folder inside a steam workdir.
pub fn content_folder(workdir: &Path, app_id: u64, workshop_id: u64) -> PathBuf {
    workdir
        .join("steamapps")
        .join("workshop")
        .join("content")
        .join(app_id.to_string())
        .join(workshop_id.to_string())
}

/// Write the account's non-secret metadata file (username only).
pub async fn write_account_meta(workdir: &Path, username: &str) -> Result<()> {
    tokio::fs::create_dir_all(workdir).await.ok();
    let meta = serde_json::json!({ "username": username });
    tokio::fs::write(
        workdir.join(ACCOUNT_META_FILE),
        serde_json::to_vec_pretty(&meta)?,
    )
    .await
    .with_context(|| "writing account metadata")?;
    Ok(())
}

/// Read the stored username for an account label, if present.
pub async fn read_account_username(workdir: &Path) -> Option<String> {
    let bytes = tokio::fs::read(workdir.join(ACCOUNT_META_FILE))
        .await
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    v.get("username")?.as_str().map(str::to_string)
}

/// True if `dir` exists and contains at least one regular file (recursively).
async fn folder_has_file(dir: &Path) -> bool {
    largest_file(dir)
        .await
        .map(|(_, sz)| sz > 0)
        .unwrap_or(false)
        || first_regular_file_exists(dir).await
}

/// Cheap existence check independent of size (some items contain only tiny files).
async fn first_regular_file_exists(dir: &Path) -> bool {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let mut rd = match tokio::fs::read_dir(&d).await {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        while let Ok(Some(entry)) = rd.next_entry().await {
            let ft = match entry.file_type().await {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            if ft.is_dir() {
                stack.push(entry.path());
            } else if ft.is_file() {
                return true;
            }
        }
    }
    false
}

/// Recursively find the single largest regular file under `dir`.
/// Returns `(path, size_bytes)`.
pub async fn largest_file(dir: &Path) -> Option<(PathBuf, u64)> {
    let mut best: Option<(PathBuf, u64)> = None;
    let mut stack = vec![dir.to_path_buf()];

    while let Some(d) = stack.pop() {
        let mut rd = tokio::fs::read_dir(&d).await.ok()?;
        while let Ok(Some(entry)) = rd.next_entry().await {
            let ft = match entry.file_type().await {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            if ft.is_dir() {
                stack.push(entry.path());
            } else if ft.is_file() {
                if let Ok(meta) = entry.metadata().await {
                    let size = meta.len();
                    if best.as_ref().map(|(_, b)| size > *b).unwrap_or(true) {
                        best = Some((entry.path(), size));
                    }
                }
            }
        }
    }
    best
}

/// A single file-selection rule received from the extension. `glob` may carry
/// `|`-separated alternatives (e.g. `*.vpk|*.bin`); brace alternation
/// (`*.{jpg,jpeg,png}`) is handled natively by `globset`. `rename` is an optional
/// destination template (see [`render_template`]).
#[derive(Debug, Clone, Deserialize)]
pub struct MatchRule {
    pub glob: String,
    #[serde(default)]
    pub rename: Option<String>,
}

/// The resolved install rule for a download. An empty `matchers` list means
/// "mirror every downloaded file under its original relative path".
#[derive(Debug, Clone, Default, Deserialize)]
pub struct InstallRule {
    #[serde(default, rename = "match")]
    pub matchers: Vec<MatchRule>,
}

/// Decide which downloaded files to install and under what destination path.
///
/// Returns `(source_relative_to_content_dir, install_destination)` pairs. With no
/// matchers, every regular file is mirrored as-is; with matchers, the first
/// matching rule (lowest index) wins and its `rename` template (if any) produces
/// the destination. Every destination is validated as a safe relative path and
/// duplicates are rejected.
pub async fn apply_install_rule(
    content_dir: &Path,
    app_id: u64,
    workshop_id: u64,
    rule: &InstallRule,
) -> Result<Vec<(PathBuf, String)>> {
    if rule.matchers.len() > MAX_RULES {
        bail!(
            "install rule has too many match entries ({} > {MAX_RULES})",
            rule.matchers.len()
        );
    }

    // Files relative to the content root, sorted for deterministic output.
    let mut rels: Vec<PathBuf> = regular_files(content_dir)
        .await?
        .into_iter()
        .map(|abs| abs.strip_prefix(content_dir).unwrap_or(&abs).to_path_buf())
        .collect();
    rels.sort();

    // Mode 3: mirror everything under its original relative name.
    if rule.matchers.is_empty() {
        let mut out = Vec::with_capacity(rels.len());
        for rel in &rels {
            let dest = rel_to_dest(rel);
            validate_install_dest(&dest)?;
            out.push((rel.clone(), dest));
        }
        return finalize_selection(out);
    }

    // Mode 2: compile a GlobSet; remember which matcher each compiled glob owns.
    let mut builder = GlobSetBuilder::new();
    let mut owner: Vec<usize> = Vec::new();
    for (i, m) in rule.matchers.iter().enumerate() {
        for pat in m.glob.split('|') {
            let pat = pat.trim();
            if pat.is_empty() {
                continue;
            }
            let glob = Glob::new(pat).with_context(|| format!("invalid glob '{pat}'"))?;
            builder.add(glob);
            owner.push(i);
        }
    }
    let set = builder.build().context("building glob set")?;

    let mut out = Vec::new();
    for rel in &rels {
        let rel_str = rel_to_dest(rel);
        let Some(rule_idx) = set
            .matches(&rel_str)
            .into_iter()
            .map(|builder_idx| owner[builder_idx])
            .min()
        else {
            continue; // unmatched files are simply not installed
        };
        let dest = match &rule.matchers[rule_idx].rename {
            Some(tpl) => render_template(tpl, app_id, workshop_id, rel)?,
            None => rel_str,
        };
        validate_install_dest(&dest)?;
        out.push((rel.clone(), dest));
    }
    finalize_selection(out)
}

/// Enforce the matched-file ceiling and reject duplicate destinations.
fn finalize_selection(out: Vec<(PathBuf, String)>) -> Result<Vec<(PathBuf, String)>> {
    if out.len() > MAX_MATCHED_FILES {
        bail!(
            "install rule matched too many files ({} > {MAX_MATCHED_FILES})",
            out.len()
        );
    }
    let mut seen = HashSet::new();
    for (_, dest) in &out {
        if !seen.insert(dest.as_str()) {
            bail!("install rule produces duplicate destination '{dest}'");
        }
    }
    Ok(out)
}

/// Render a `rename` template. Supported tokens: `{workshop_id}`, `{app_id}`,
/// `{ext}` (lowercased original extension), `{basename}` (original stem). Any
/// leftover `{`/`}` means an unknown token, which is rejected rather than written
/// literally to disk.
fn render_template(tpl: &str, app_id: u64, workshop_id: u64, rel: &Path) -> Result<String> {
    let ext = rel
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let basename = rel
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();

    let out = tpl
        .replace("{workshop_id}", &workshop_id.to_string())
        .replace("{app_id}", &app_id.to_string())
        .replace("{ext}", &ext)
        .replace("{basename}", &basename);

    if out.len() > MAX_DEST_LEN {
        bail!("rendered install path '{out}' is too long");
    }
    if out.contains('{') || out.contains('}') {
        bail!("rename template '{tpl}' contains an unknown token");
    }
    Ok(out)
}

/// Relative path as a forward-slash string suitable for matching and zip names.
fn rel_to_dest(rel: &Path) -> String {
    rel.to_string_lossy().replace('\\', "/")
}

/// Reject anything that could escape the install root or is otherwise unsafe.
fn validate_install_dest(dest: &str) -> Result<()> {
    let norm = dest.replace('\\', "/");
    if norm.is_empty() || norm.len() > MAX_DEST_LEN {
        bail!("invalid install destination '{dest}'");
    }
    if norm.starts_with('/') {
        bail!("install destination '{dest}' must be relative");
    }
    for seg in norm.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            bail!("install destination '{dest}' has an invalid path segment");
        }
        if seg.contains(':') {
            bail!("install destination '{dest}' contains a drive/colon segment");
        }
        if seg.chars().any(char::is_control) {
            bail!("install destination '{dest}' contains control characters");
        }
    }
    Ok(())
}

async fn regular_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let mut rd = tokio::fs::read_dir(&d).await?;
        while let Some(entry) = rd.next_entry().await? {
            let ft = entry.file_type().await?;
            if ft.is_dir() {
                stack.push(entry.path());
            } else if ft.is_file() {
                out.push(entry.path());
            }
        }
    }
    Ok(out)
}

/// Zip the selected source files into `dest`, each stored under its mapped
/// install name (`(relative_source, install_name)` pairs).
pub async fn zip_selected_files(
    src_root: &Path,
    files: &[(PathBuf, String)],
    dest: &Path,
) -> Result<()> {
    let src_root = src_root.to_path_buf();
    let files = files.to_vec();
    let dest = dest.to_path_buf();
    tokio::task::spawn_blocking(move || zip_selected_files_blocking(&src_root, &files, &dest))
        .await
        .context("zip task panicked")?
}

fn zip_selected_files_blocking(
    src_root: &Path,
    files: &[(PathBuf, String)],
    dest: &Path,
) -> Result<()> {
    use std::io::{Read, Write};
    use zip::write::SimpleFileOptions;

    let file = std::fs::File::create(dest)
        .with_context(|| format!("creating archive {}", dest.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut buf = Vec::new();

    for (rel, install_name) in files {
        let src = src_root.join(rel);
        let name = install_name.replace('\\', "/");
        zip.start_file(name, options)?;
        let mut f = std::fs::File::open(&src)?;
        buf.clear();
        f.read_to_end(&mut buf)?;
        zip.write_all(&buf)?;
    }

    zip.finish()?;
    Ok(())
}

/// Zip an entire content folder into `dest` (`.zip`). Runs the (blocking) zip
/// work on a blocking thread. Returns the byte size of the resulting archive.
pub async fn zip_folder(src: PathBuf, dest: PathBuf) -> Result<u64> {
    tokio::task::spawn_blocking(move || zip_folder_blocking(&src, &dest))
        .await
        .context("zip task panicked")?
}

fn zip_folder_blocking(src: &Path, dest: &Path) -> Result<u64> {
    use std::io::{Read, Write};
    use zip::write::SimpleFileOptions;

    let file = std::fs::File::create(dest)
        .with_context(|| format!("creating archive {}", dest.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // Iterative directory walk to keep relative paths inside the archive.
    let mut stack = vec![src.to_path_buf()];
    let mut buf = Vec::new();
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d)? {
            let entry = entry?;
            let path = entry.path();
            let rel = path
                .strip_prefix(src)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if path.is_dir() {
                zip.add_directory(format!("{rel}/"), options)?;
                stack.push(path);
            } else {
                zip.start_file(rel, options)?;
                let mut f = std::fs::File::open(&path)?;
                buf.clear();
                f.read_to_end(&mut buf)?;
                zip.write_all(&buf)?;
            }
        }
    }
    zip.finish()?;

    let size = std::fs::metadata(dest)?.len();
    Ok(size)
}

/// Pull out a likely-relevant error line from steamcmd output.
fn extract_error_line(out: &str) -> Option<String> {
    for line in out.lines().rev() {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        let lower = l.to_lowercase();
        if lower.contains("error")
            || lower.contains("failed")
            || lower.contains("failure")
            || lower.contains("invalid")
            || lower.contains("no connection")
            || lower.contains("no subscription")
            || lower.contains("timeout")
        {
            // Trim to a sane length to keep job errors concise.
            let trimmed: String = l.chars().take(300).collect();
            return Some(trimmed);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn login_no_connection_is_not_invalid_credentials() {
        let outcome = parse_login_outcome(
            "Loading Steam API...CreateBoundSocket: failed to create socket, error [no name available] (38)\n\
             Logging in user 'example' [U:1:2345678] to Steam Public...Retrying...\n\
             ERROR (No Connection)\n",
        )
        .expect("parse login output");

        assert_eq!(
            outcome,
            LoginOutcome::ConnectivityFailed("ERROR (No Connection)".to_string())
        );
    }

    #[test]
    fn login_guard_prompt_is_detected() {
        let outcome = parse_login_outcome("Steam Guard code required").expect("parse login output");
        assert_eq!(outcome, LoginOutcome::NeedsGuard);
    }

    #[test]
    fn mobile_authenticator_confirmation_is_success_not_guard() {
        // A confirmed mobile-authenticator login mentions "Steam Guard" but also
        // completes — it must be treated as Ok, not NeedsGuard.
        let outcome = parse_login_outcome(
            "Logging in user 'example' to Steam Public...This account is protected by a Steam Guard mobile authenticator.\n\
             Please confirm the login in the Steam Mobile app on your phone.\n\
             Waiting for confirmation...OK\n\
             Waiting for client config...OK\n\
             Waiting for user info...OK\n",
        )
        .expect("parse login output");
        assert_eq!(outcome, LoginOutcome::Ok);
    }

    #[test]
    fn mobile_authenticator_success_with_compat_text_is_ok() {
        let outcome = parse_login_outcome(
            "Logging in user 'example' [U:1:2345678] to Steam Public...This account is protected by a Steam Guard mobile authenticator.\n\
             Please confirm the login in the Steam Mobile app on your phone.\n\
             Waiting for confirmation...OK\n\
             Waiting for client config...OK\n\
             Waiting for user info...Waiting for compat in post-logon took: 0.152688sOK\n",
        )
        .expect("parse login output");
        assert_eq!(outcome, LoginOutcome::Ok);
    }

    #[test]
    fn guard_code_success_with_public_ok_is_ok() {
        let outcome = parse_login_outcome(
            "Logging in using username/password.\n\
             Steam Guard code provided.\n\
             Logging in user 'example' [U:1:2345678] to Steam Public...OK\n\
             Waiting for client config...OK\n\
             Waiting for user info...Waiting for compat in post-logon took: 0.098278sOK\n",
        )
        .expect("parse login output");
        assert_eq!(outcome, LoginOutcome::Ok);
    }

    // --- install-rule evaluator ---------------------------------------------

    /// Create a throwaway content dir populated with empty files.
    async fn make_content(files: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("calaworkshop-test-{}", uuid::Uuid::new_v4()));
        for f in files {
            let path = dir.join(f);
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await.unwrap();
            }
            tokio::fs::write(&path, b"x").await.unwrap();
        }
        dir
    }

    /// Destinations only, sorted, for order-independent assertions.
    fn dests(mut v: Vec<(PathBuf, String)>) -> Vec<String> {
        v.sort_by(|a, b| a.1.cmp(&b.1));
        v.into_iter().map(|(_, d)| d).collect()
    }

    fn l4d2_rule() -> InstallRule {
        InstallRule {
            matchers: vec![
                MatchRule {
                    glob: "*.vpk|*_legacy.bin".into(),
                    rename: Some("{workshop_id}.vpk".into()),
                },
                MatchRule {
                    glob: "*_legacy.{jpg,jpeg,png}".into(),
                    rename: Some("{workshop_id}.{ext}".into()),
                },
            ],
        }
    }

    #[tokio::test]
    async fn l4d2_rule_renames_bin_and_pairs_image() {
        let dir = make_content(&[
            "9372098838249685383_legacy.bin",
            "9372098838249685383_legacy.jpg",
        ])
        .await;
        let mapped = apply_install_rule(&dir, 550, 2888803926, &l4d2_rule())
            .await
            .unwrap();
        assert_eq!(
            dests(mapped),
            vec!["2888803926.jpg".to_string(), "2888803926.vpk".to_string()]
        );
        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn empty_rule_mirrors_all_files_preserving_structure() {
        let dir = make_content(&["a.txt", "sub/b.dat"]).await;
        let mapped = apply_install_rule(&dir, 4000, 1, &InstallRule::default())
            .await
            .unwrap();
        assert_eq!(
            dests(mapped),
            vec!["a.txt".to_string(), "sub/b.dat".to_string()]
        );
        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn unmatched_files_are_skipped() {
        let dir = make_content(&["keep.pak", "ignore.log"]).await;
        let rule = InstallRule {
            matchers: vec![MatchRule {
                glob: "*.pak".into(),
                rename: None,
            }],
        };
        let mapped = apply_install_rule(&dir, 1, 1, &rule).await.unwrap();
        assert_eq!(dests(mapped), vec!["keep.pak".to_string()]);
        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn subpath_rename_is_allowed() {
        let dir = make_content(&["mod.pak"]).await;
        let rule = InstallRule {
            matchers: vec![MatchRule {
                glob: "*.pak".into(),
                rename: Some("Mods/{basename}.{ext}".into()),
            }],
        };
        let mapped = apply_install_rule(&dir, 1, 1, &rule).await.unwrap();
        assert_eq!(dests(mapped), vec!["Mods/mod.pak".to_string()]);
        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn duplicate_destinations_are_rejected() {
        let dir = make_content(&["a.vpk", "b.vpk"]).await;
        let rule = InstallRule {
            matchers: vec![MatchRule {
                glob: "*.vpk".into(),
                rename: Some("{workshop_id}.vpk".into()),
            }],
        };
        let err = apply_install_rule(&dir, 550, 99, &rule).await.unwrap_err();
        assert!(err.to_string().contains("duplicate destination"));
        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[test]
    fn template_rejects_unknown_token() {
        let err = render_template("{nope}.vpk", 1, 2, Path::new("a.bin")).unwrap_err();
        assert!(err.to_string().contains("unknown token"));
    }

    #[test]
    fn template_expands_known_tokens() {
        let out =
            render_template("{app_id}/{basename}.{ext}", 550, 7, Path::new("Foo.BIN")).unwrap();
        assert_eq!(out, "550/Foo.bin");
    }

    #[test]
    fn dest_rejects_path_escape() {
        assert!(validate_install_dest("../evil").is_err());
        assert!(validate_install_dest("/abs").is_err());
        assert!(validate_install_dest("c:/win").is_err());
        assert!(validate_install_dest("ok/sub.vpk").is_ok());
    }
}
