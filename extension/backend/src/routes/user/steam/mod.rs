//! User-scoped Steam account linking. These proxy the helper's `/accounts` API.
//!
//! v1 note: this is a thin proxy suitable for a single-admin panel — any user
//! with the `calaworkshop.link-steam` permission can manage helper accounts.
//! Per-user ownership scoping (which Calagopus user owns which label) is a
//! Phase 5 item and would use the `*_steam_links` migration table.

use super::State;
use axum::{extract::Path, http::StatusCode, routing};
use serde::Deserialize;
use shared::{
    GetState,
    models::user::GetPermissionManager,
    response::{ApiResponse, ApiResponseResult},
};
use utoipa_axum::router::OpenApiRouter;

mod _label_;

fn helper_from<'a>(
    state: &'a GetState,
    ext: &'a crate::settings::ExtensionSettingsData,
) -> Result<crate::helper::HelperClient<'a>, ApiResponse> {
    crate::helper::HelperClient::new(&state.client, &ext.helper_url, &ext.helper_token)
        .ok_or_else(|| ApiResponse::error("workshop helper is not configured"))
}

/// `GET /steam/accounts` — list linked accounts known to the helper.
async fn list(state: GetState, permissions: GetPermissionManager) -> ApiResponseResult {
    permissions.has_user_permission("calaworkshop.link-steam")?;

    let settings = state.settings.get().await?;
    let ext: &crate::settings::ExtensionSettingsData = settings.find_extension_settings()?;
    let helper = helper_from(&state, ext)?;

    let accounts = helper.list_accounts().await?;
    ApiResponse::new_serialized(accounts).ok()
}

#[derive(Deserialize)]
struct LoginPayload {
    label: String,
    username: String,
    password: String,
    #[serde(default)]
    guard_code: Option<String>,
}

/// `POST /steam/accounts` — establish/refresh a cached SteamCMD session. Forwards
/// the helper's status, including `409 needs_guard`.
async fn login(
    state: GetState,
    permissions: GetPermissionManager,
    shared::Payload(data): shared::Payload<LoginPayload>,
) -> ApiResponseResult {
    permissions.has_user_permission("calaworkshop.link-steam")?;

    if data.label.trim().is_empty() || data.username.trim().is_empty() {
        return Err(ApiResponse::error("label and username are required"));
    }

    let settings = state.settings.get().await?;
    let ext: &crate::settings::ExtensionSettingsData = settings.find_extension_settings()?;
    let helper = helper_from(&state, ext)?;

    let (status, body) = helper
        .login_account(&crate::helper::LoginRequest {
            label: data.label,
            username: data.username,
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
