//! User-scoped Steam account linking.
//!
//! Each linked account is owned by the calling user. We store the ownership and
//! a friendly label in `dev_wasian_calaworkshop_steam_links` and address the
//! helper's cached SteamCMD session through an opaque per-link `helper_label`, so
//! no user (admin or otherwise) can see or use another user's linked account.
//! Requires the `calaworkshop.link-steam` user permission.

use super::State;
use axum::{http::StatusCode, routing};
use serde::Deserialize;
use shared::{
    GetState,
    models::user::{GetPermissionManager, GetUser},
    response::{ApiResponse, ApiResponseResult},
};
use std::collections::HashSet;
use utoipa_axum::router::OpenApiRouter;

mod _label_;

/// Snapshot the extension settings, dropping the settings read guard before any
/// network call (holding it across helper I/O can stall the whole panel).
async fn snapshot_settings(
    state: &GetState,
) -> Result<crate::settings::ExtensionSettingsData, anyhow::Error> {
    let settings = state.settings.get().await?;
    Ok(settings
        .find_extension_settings::<crate::settings::ExtensionSettingsData>()?
        .clone())
}

/// `GET /steam/accounts` — list the calling user's linked accounts.
async fn list(
    state: GetState,
    permissions: GetPermissionManager,
    user: GetUser,
) -> ApiResponseResult {
    permissions.has_user_permission("calaworkshop.link-steam")?;

    let links = crate::steam_links::list_by_user(state.database.read(), user.uuid).await?;

    // Best-effort: one helper call tells us which opaque sessions look live.
    let ext = snapshot_settings(&state).await?;
    let valid: HashSet<String> =
        match crate::helper::HelperClient::new(&state.client, &ext.helper_url, &ext.helper_token) {
            Some(helper) => match helper.list_accounts().await {
                Ok(value) => value
                    .get("accounts")
                    .and_then(|a| a.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter(|a| a.get("valid").and_then(|b| b.as_bool()).unwrap_or(false))
                            .filter_map(|a| {
                                a.get("label").and_then(|l| l.as_str()).map(str::to_string)
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                Err(_) => HashSet::new(),
            },
            None => HashSet::new(),
        };

    let accounts: Vec<serde_json::Value> = links
        .into_iter()
        .map(|link| {
            serde_json::json!({
                "label": link.label,
                "username": link.steam_username,
                "valid": valid.contains(&link.helper_label),
            })
        })
        .collect();

    ApiResponse::new_serialized(serde_json::json!({ "accounts": accounts })).ok()
}

#[derive(Deserialize)]
struct LoginPayload {
    label: String,
    username: String,
    password: String,
    #[serde(default)]
    guard_code: Option<String>,
}

/// `POST /steam/accounts` — establish/refresh a cached SteamCMD session for the
/// calling user. Forwards the helper's status, including `409 needs_guard`.
async fn login(
    state: GetState,
    permissions: GetPermissionManager,
    user: GetUser,
    shared::Payload(data): shared::Payload<LoginPayload>,
) -> ApiResponseResult {
    permissions.has_user_permission("calaworkshop.link-steam")?;

    let label = data.label.trim().to_string();
    let username = data.username.trim().to_string();
    if label.is_empty() || username.is_empty() {
        return Err(ApiResponse::error("label and username are required"));
    }
    crate::validation::validate_account_label(&label)?;

    let ext = snapshot_settings(&state).await?;
    let helper =
        crate::helper::HelperClient::new(&state.client, &ext.helper_url, &ext.helper_token)
            .ok_or_else(|| ApiResponse::error("workshop helper is not configured"))?;

    // Reserve (or reuse) the opaque helper label up front so a Steam Guard retry
    // keeps targeting the same cached session directory on the helper.
    let link =
        crate::steam_links::upsert(state.database.write(), user.uuid, &label, Some(&username))
            .await?;

    let (status, body) = helper
        .login_account(&crate::helper::LoginRequest {
            label: link.helper_label,
            username,
            password: data.password,
            guard_code: data.guard_code,
        })
        .await?;

    ApiResponse::new_serialized(body)
        .with_status(StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY))
        .ok()
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .route("/", routing::get(list).post(login))
        .nest("/{label}", _label_::router(state))
        .with_state(state.clone())
}
