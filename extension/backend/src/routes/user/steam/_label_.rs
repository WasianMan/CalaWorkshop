use super::State;
use axum::{extract::Path, http::StatusCode, routing};
use shared::{
    GetState,
    models::user::{GetPermissionManager, GetUser},
    response::{ApiResponse, ApiResponseResult},
};
use utoipa_axum::router::OpenApiRouter;

/// `DELETE /steam/accounts/{label}` — unlink one of the calling user's accounts
/// and drop the helper's cached session. Scoped to the user, so a label only
/// resolves to a session they own.
async fn remove(
    state: GetState,
    permissions: GetPermissionManager,
    user: GetUser,
    Path(label): Path<String>,
) -> ApiResponseResult {
    permissions.has_user_permission("calaworkshop.link-steam")?;
    crate::validation::validate_account_label(&label)?;

    // Delete the ownership row first (authoritative), returning the opaque label.
    let helper_label = crate::steam_links::delete(state.database.write(), user.uuid, &label)
        .await?
        .ok_or_else(|| ApiResponse::error("unknown account").with_status(StatusCode::NOT_FOUND))?;

    // Best-effort cleanup of the helper's cached session. Snapshot settings and
    // drop the read guard before the network call.
    let ext = {
        let settings = state.settings.get().await?;
        settings
            .find_extension_settings::<crate::settings::ExtensionSettingsData>()?
            .clone()
    };
    if let Some(helper) =
        crate::helper::HelperClient::new(&state.client, &ext.helper_url, &ext.helper_token)
    {
        let _ = helper.delete_account(&helper_label).await;
    }

    ApiResponse::new_serialized(serde_json::json!({ "removed": true })).ok()
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .route("/", routing::delete(remove))
        .with_state(state.clone())
}
