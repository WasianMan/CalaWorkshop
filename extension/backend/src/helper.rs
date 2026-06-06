//! Thin HTTP client for the workshop helper sidecar.
//!
//! The extension never runs SteamCMD or touches a server volume itself. It asks
//! the helper to download a Workshop item, then hands the helper's `/files` URL
//! to Wings for placement. See `CONTRACT.md` for the wire format.

use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct DownloadRequest {
    pub app_id: u32,
    pub workshop_id: u64,
    pub account: Option<String>,
    pub archive: bool,
}

#[derive(Deserialize)]
pub struct DownloadResponse {
    pub id: uuid::Uuid,
    pub state: String,
    pub file_token: String,
}

#[derive(Deserialize)]
pub struct JobResponse {
    pub id: uuid::Uuid,
    pub state: String,
    pub app_id: u32,
    pub workshop_id: u64,
    pub file_name: Option<String>,
    pub file_token: String,
    pub size: Option<u64>,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct LoginRequest {
    pub label: String,
    pub username: String,
    pub password: String,
    pub guard_code: Option<String>,
}

/// Borrowed view over the configured helper. Construct per-request from settings.
pub struct HelperClient<'a> {
    client: &'a reqwest::Client,
    base_url: String,
    token: String,
}

impl<'a> HelperClient<'a> {
    /// Returns `None` when the helper has not been configured yet (missing
    /// url/token in the admin settings).
    pub fn new(client: &'a reqwest::Client, helper_url: &str, helper_token: &str) -> Option<Self> {
        if helper_url.trim().is_empty() || helper_token.trim().is_empty() {
            return None;
        }

        Some(Self {
            client,
            base_url: helper_url.trim_end_matches('/').to_string(),
            token: helper_token.to_string(),
        })
    }

    /// Build the public `/files` URL Wings will pull from for a given job.
    pub fn file_url(&self, job_id: uuid::Uuid, file_token: &str) -> String {
        format!(
            "{}/files/{}?token={}",
            self.base_url,
            job_id,
            urlencoding_encode(file_token)
        )
    }

    pub async fn start_download(
        &self,
        body: &DownloadRequest,
    ) -> Result<DownloadResponse, anyhow::Error> {
        Ok(self
            .client
            .post(format!("{}/download", self.base_url))
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    pub async fn get_job(&self, job_id: uuid::Uuid) -> Result<JobResponse, anyhow::Error> {
        Ok(self
            .client
            .get(format!("{}/jobs/{}", self.base_url, job_id))
            .bearer_auth(&self.token)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    pub async fn list_accounts(&self) -> Result<serde_json::Value, anyhow::Error> {
        Ok(self
            .client
            .get(format!("{}/accounts", self.base_url))
            .bearer_auth(&self.token)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    /// Returns the helper's HTTP status and JSON body so the route can forward a
    /// `409 needs_guard` faithfully.
    pub async fn login_account(
        &self,
        body: &LoginRequest,
    ) -> Result<(u16, serde_json::Value), anyhow::Error> {
        let resp = self
            .client
            .post(format!("{}/accounts/login", self.base_url))
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .await?;
        let status = resp.status().as_u16();
        let value = resp.json().await.unwrap_or(serde_json::Value::Null);
        Ok((status, value))
    }

    pub async fn delete_account(&self, label: &str) -> Result<(), anyhow::Error> {
        self.client
            .delete(format!(
                "{}/accounts/{}",
                self.base_url,
                urlencoding_encode(label)
            ))
            .bearer_auth(&self.token)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}

/// Minimal percent-encoding for query/path values (avoids pulling another dep;
/// only used for tokens and account labels which are short ascii-ish strings).
fn urlencoding_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
