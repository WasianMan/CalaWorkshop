//! Async login sessions (password with mobile-confirmation waiting, and QR).
//!
//! These routes are thin pass-throughs to the helper's login-session API. The
//! extension's job is ownership: it maps the caller's friendly label to their
//! opaque `helper_label` before talking to the helper, verifies on every poll
//! that the session belongs to that link, and records the Steam username once
//! a QR login completes. The helper-internal opaque label is stripped from
//! responses before they reach the browser.

use super::{snapshot_settings, State};
use axum::{extract::Path, http::StatusCode, routing};
use serde::Deserialize;
use shared::{
    GetState,
    models::user::{GetPermissionManager, GetUser},
    response::{ApiResponse, ApiResponseResult},
};
use utoipa_axum::router::OpenApiRouter;

/// Snapshot extension settings, mapping failure to a uniform error.
async fn ext_settings(
    state: &GetState,
) -> Result<crate::settings::ExtensionSettingsData, ApiResponse> {
    snapshot_settings(state)
        .await
        .map_err(|_| ApiResponse::error("extension settings unavailable"))
}

/// Forward a helper `(status, body)` pair to the browser, stripping the opaque
/// helper label from the session view.
fn forward(status: u16, mut body: serde_json::Value) -> ApiResponseResult {
    if let Some(obj) = body.as_object_mut() {
        obj.remove("label");
    }
    ApiResponse::new_serialized(body)
        .with_status(StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY))
        .ok()
}

#[derive(Deserialize)]
struct BeginPayload {
    label: String,
    username: String,
    password: String,
    #[serde(default)]
    guard_code: Option<String>,
}

/// `POST /steam/login-sessions` — start an async password login.
async fn begin(
    state: GetState,
    permissions: GetPermissionManager,
    user: GetUser,
    shared::Payload(data): shared::Payload<BeginPayload>,
) -> ApiResponseResult {
    permissions.has_user_permission("calaworkshop.link-steam")?;

    let label = data.label.trim().to_string();
    let username = data.username.trim().to_string();
    if label.is_empty() || username.is_empty() {
        return Err(ApiResponse::error("label and username are required"));
    }
    crate::validation::validate_account_label(&label)?;

    let ext = ext_settings(&state).await?;
    let helper =
        crate::helper::HelperClient::new(&state.client, &ext.helper_url, &ext.helper_token)
            .ok_or_else(|| ApiResponse::error("workshop helper is not configured"))?;

    let link =
        crate::steam_links::upsert(state.database.write(), user.uuid, &label, Some(&username))
            .await?;

    let (status, body) = helper
        .begin_login_session(&crate::helper::LoginRequest {
            label: link.helper_label,
            username,
            password: data.password,
            guard_code: data.guard_code,
        })
        .await?;
    forward(status, body)
}

#[derive(Deserialize)]
struct BeginQrPayload {
    label: String,
}

/// `POST /steam/login-sessions/qr` — start a QR login (no credentials).
async fn begin_qr(
    state: GetState,
    permissions: GetPermissionManager,
    user: GetUser,
    shared::Payload(data): shared::Payload<BeginQrPayload>,
) -> ApiResponseResult {
    permissions.has_user_permission("calaworkshop.link-steam")?;

    let label = data.label.trim().to_string();
    if label.is_empty() {
        return Err(ApiResponse::error("label is required"));
    }
    crate::validation::validate_account_label(&label)?;

    let ext = ext_settings(&state).await?;
    let helper =
        crate::helper::HelperClient::new(&state.client, &ext.helper_url, &ext.helper_token)
            .ok_or_else(|| ApiResponse::error("workshop helper is not configured"))?;

    // Username is unknown until the QR approval; keep any previous one.
    let link = crate::steam_links::upsert(state.database.write(), user.uuid, &label, None).await?;

    let (status, body) = helper.begin_login_session_qr(&link.helper_label).await?;
    forward(status, body)
}

#[derive(Deserialize)]
struct SessionQuery {
    label: String,
}

/// Fetch a session from the helper and verify it belongs to the caller's link.
/// Returns the link alongside the raw session body.
async fn owned_session(
    state: &GetState,
    user_uuid: uuid::Uuid,
    label: &str,
    id: uuid::Uuid,
) -> Result<(crate::steam_links::SteamLink, u16, serde_json::Value), ApiResponse> {
    crate::validation::validate_account_label(label)?;
    let link = crate::steam_links::get_by_label(state.database.read(), user_uuid, label)
        .await?
        .ok_or_else(|| ApiResponse::error("unknown account label").with_status(StatusCode::NOT_FOUND))?;

    let ext = ext_settings(state).await?;
    let helper =
        crate::helper::HelperClient::new(&state.client, &ext.helper_url, &ext.helper_token)
            .ok_or_else(|| ApiResponse::error("workshop helper is not configured"))?;

    let (status, body) = helper.get_login_session(id).await?;
    if status == 200
        && body.get("label").and_then(|l| l.as_str()) != Some(link.helper_label.as_str())
    {
        return Err(ApiResponse::error("unknown login session").with_status(StatusCode::NOT_FOUND));
    }
    Ok((link, status, body))
}

/// `GET /steam/login-sessions/{id}?label=...` — poll a login session. When the
/// session has completed, the link's recorded Steam username is refreshed (QR
/// logins only learn it at approval time).
async fn poll(
    state: GetState,
    permissions: GetPermissionManager,
    user: GetUser,
    Path(id): Path<uuid::Uuid>,
    axum::extract::Query(query): axum::extract::Query<SessionQuery>,
) -> ApiResponseResult {
    permissions.has_user_permission("calaworkshop.link-steam")?;

    let (_link, status, body) = owned_session(&state, user.uuid, &query.label, id).await?;

    if status == 200
        && body.get("state").and_then(|s| s.as_str()) == Some("ok")
    {
        if let Some(username) = body.get("username").and_then(|u| u.as_str()) {
            crate::steam_links::set_username(state.database.write(), user.uuid, &query.label, username)
                .await?;
        }
    }
    forward(status, body)
}

/// `DELETE /steam/login-sessions/{id}?label=...` — cancel a login session.
async fn cancel(
    state: GetState,
    permissions: GetPermissionManager,
    user: GetUser,
    Path(id): Path<uuid::Uuid>,
    axum::extract::Query(query): axum::extract::Query<SessionQuery>,
) -> ApiResponseResult {
    permissions.has_user_permission("calaworkshop.link-steam")?;

    let (_link, status, _body) = owned_session(&state, user.uuid, &query.label, id).await?;
    if status == 200 {
        let ext = ext_settings(&state).await?;
        if let Some(helper) =
            crate::helper::HelperClient::new(&state.client, &ext.helper_url, &ext.helper_token)
        {
            helper.cancel_login_session(id).await.ok();
        }
    }
    ApiResponse::new_serialized(serde_json::json!({ "cancelled": true })).ok()
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .route("/", routing::post(begin))
        .route("/qr", routing::post(begin_qr))
        .route("/{id}", routing::get(poll).delete(cancel))
        .with_state(state.clone())
}
