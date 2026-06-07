//! SteamCMD invocation, output parsing, and artifact selection.
//!
//! This module is deliberately free of any HTTP / axum types so the download
//! pipeline can be unit-reasoned in isolation.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use tokio::process::Command;

use crate::config::Config;

/// Outcome of a steamcmd login attempt (used by `POST /accounts/login`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginOutcome {
    /// Session established / refreshed successfully.
    Ok,
    /// SteamCMD asked for a Steam Guard code — caller must re-try with one.
    NeedsGuard,
    /// Credentials were rejected.
    InvalidCredentials,
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

    // steamcmd +force_install_dir <dir> +login <user> +workshop_download_item <app> <id> +quit
    let mut cmd = Command::new(&config.steamcmd_bin);
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

    let output = cmd
        .output()
        .await
        .with_context(|| format!("spawning steamcmd ({})", config.steamcmd_bin))?;

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

    let output = cmd
        .output()
        .await
        .with_context(|| format!("spawning steamcmd ({})", config.steamcmd_bin))?;

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    tracing::debug!(%combined, "steamcmd login finished");

    // Detection is heuristic — steamcmd's wording varies across versions. We look
    // for the common Steam Guard prompts. Note: in a non-interactive process
    // steamcmd cannot actually *prompt*, so a fresh Guard-protected account will
    // emit one of these and exit; the caller then re-POSTs with `guard_code`.
    let lower = combined.to_lowercase();
    if lower.contains("steam guard")
        || lower.contains("two-factor")
        || lower.contains("guard code")
        || lower.contains("need two factor")
    {
        return Ok(LoginOutcome::NeedsGuard);
    }

    if combined.contains("Logged in OK") || combined.contains("Waiting for user info...OK") {
        return Ok(LoginOutcome::Ok);
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

pub async fn connectivity_check(config: &Config) -> Result<String> {
    let workdir = config.steam_dir("diagnostics");
    tokio::fs::create_dir_all(&workdir)
        .await
        .with_context(|| format!("creating steam workdir {}", workdir.display()))?;

    let output = Command::new(&config.steamcmd_bin)
        .arg("+force_install_dir")
        .arg(&workdir)
        .arg("+login")
        .arg("anonymous")
        .arg("+quit")
        .current_dir(&workdir)
        .kill_on_drop(true)
        .output()
        .await
        .with_context(|| format!("spawning steamcmd ({})", config.steamcmd_bin))?;

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

/// Pick the files a normal install should place into a server. L4D2 workshop
/// items are usually a `.vpk` plus a same-stem preview image.
pub async fn select_install_artifacts(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = regular_files(dir).await?;
    files.sort();

    let vpk = files
        .iter()
        .find(|p| has_ext(p, "vpk"))
        .cloned()
        .or_else(|| {
            files
                .iter()
                .filter_map(|p| std::fs::metadata(p).ok().map(|m| (p, m.len())))
                .max_by_key(|(_, len)| *len)
                .map(|(p, _)| p.clone())
        })
        .ok_or_else(|| anyhow!("no files found in downloaded content"))?;

    let mut selected = vec![vpk.clone()];
    if let Some(stem) = vpk.file_stem().and_then(|s| s.to_str()) {
        if let Some(image) = files.iter().find(|p| {
            p.file_stem().and_then(|s| s.to_str()) == Some(stem)
                && (has_ext(p, "jpg") || has_ext(p, "jpeg") || has_ext(p, "png"))
        }) {
            selected.push(image.clone());
        }
    }

    Ok(selected
        .into_iter()
        .map(|p| p.strip_prefix(dir).unwrap_or(&p).to_path_buf())
        .collect())
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

fn has_ext(path: &Path, ext: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(ext))
        .unwrap_or(false)
}

pub async fn zip_selected_files(src_root: &Path, files: &[PathBuf], dest: &Path) -> Result<()> {
    let src_root = src_root.to_path_buf();
    let files = files.to_vec();
    let dest = dest.to_path_buf();
    tokio::task::spawn_blocking(move || zip_selected_files_blocking(&src_root, &files, &dest))
        .await
        .context("zip task panicked")?
}

fn zip_selected_files_blocking(src_root: &Path, files: &[PathBuf], dest: &Path) -> Result<()> {
    use std::io::{Read, Write};
    use zip::write::SimpleFileOptions;

    let file = std::fs::File::create(dest)
        .with_context(|| format!("creating archive {}", dest.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut buf = Vec::new();

    for rel in files {
        let src = src_root.join(rel);
        let name = rel.to_string_lossy().replace('\\', "/");
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
