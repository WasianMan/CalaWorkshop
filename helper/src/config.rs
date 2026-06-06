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

        Ok(Self {
            token,
            bind,
            data_dir,
            steamcmd_bin,
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
