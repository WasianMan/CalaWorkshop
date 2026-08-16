//! Runtime configuration, loaded from environment variables.

use anyhow::{anyhow, Result};
use std::path::PathBuf;

/// Process-wide configuration. Built once at startup and shared via `AppState`.
#[derive(Debug, Clone)]
pub struct Config {
    /// Bearer token required on every authenticated endpoint.
    pub token: String,
    /// Socket address to bind the HTTP server to.
    pub bind: String,
    /// Root data directory. Holds `jobs/<id>/` artifacts and `steam/<label>/` workdirs.
    pub data_dir: PathBuf,
    /// Path to the `steamcmd` executable (or `.sh` wrapper).
    pub steamcmd_bin: String,
    /// Maximum number of `steamcmd` downloads allowed to run concurrently. Jobs
    /// beyond this stay `queued` until a slot frees up. Paces big collections so
    /// we don't spawn dozens of steamcmd processes at once and trip Steam's rate
    /// limiter. Defaults to 3; override with `WORKSHOP_MAX_CONCURRENT`.
    pub max_concurrent: usize,
}

impl Config {
    /// Load configuration from the environment, applying documented defaults.
    ///
    /// Returns an error (so the process can refuse to start) when the required
    /// `WORKSHOP_HELPER_TOKEN` is missing or empty.
    pub fn from_env() -> Result<Self> {
        let token = std::env::var("WORKSHOP_HELPER_TOKEN")
            .ok()
            .filter(|t| !t.trim().is_empty())
            .ok_or_else(|| {
                anyhow!("WORKSHOP_HELPER_TOKEN is required but unset/empty — refusing to start")
            })?;

        let bind =
            std::env::var("WORKSHOP_HELPER_BIND").unwrap_or_else(|_| "0.0.0.0:8090".to_string());

        let data_dir = std::env::var("WORKSHOP_DATA_DIR")
            .unwrap_or_else(|_| "/data".to_string())
            .into();

        let steamcmd_bin = std::env::var("STEAMCMD_BIN").unwrap_or_else(|_| "steamcmd".to_string());

        // At least 1; ignore unparsable/zero values and fall back to the default.
        let max_concurrent = std::env::var("WORKSHOP_MAX_CONCURRENT")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|n| *n >= 1)
            .unwrap_or(3);

        Ok(Self {
            token,
            bind,
            data_dir,
            steamcmd_bin,
            max_concurrent,
        })
    }

    /// Directory holding a job's finished artifact: `<data_dir>/jobs/<id>/`.
    pub fn job_dir(&self, id: &uuid::Uuid) -> PathBuf {
        self.data_dir.join("jobs").join(id.to_string())
    }

    /// SteamCMD working dir for an account label (`anonymous` when null).
    pub fn steam_dir(&self, label: &str) -> PathBuf {
        self.data_dir.join("steam").join(label)
    }
}
