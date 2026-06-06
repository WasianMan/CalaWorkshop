use super::State;
use axum::{extract::Path, routing};
use shared::{
    GetState,
    models::user::GetPermissionManager,
    response::{ApiResponse, ApiResponseResult},
};
use utoipa_axum::router::OpenApiRouter;

/// `DELETE /steam/accounts/{label}` — drop a cached helper session.
async fn remove(
    state: GetState,
    permissions: GetPermissionManager,
    Path(label): Path<String>,
) -> ApiResponseResult {
    permissions.has_admin_permission("calaworkshop.configure")?;
    crate::validation::validate_account_label(&label)?;

    let settings = state.settings.get().await?;
    let ext: &crate::settings::ExtensionSettingsData = settings.find_extension_settings()?;
    let helper =
        crate::helper::HelperClient::new(&state.client, &ext.helper_url, &ext.helper_token)
            .ok_or_else(|| ApiResponse::error("workshop helper is not configured"))?;

    helper.delete_account(&label).await?;

    ApiResponse::new_serialized(serde_json::json!({ "removed": true })).ok()
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .route("/", routing::delete(remove))
        .with_state(state.clone())
}
