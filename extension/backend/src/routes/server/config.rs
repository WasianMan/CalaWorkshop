use super::State;
use utoipa_axum::{router::OpenApiRouter, routes};

mod get {
    use serde::Serialize;
    use shared::{
        GetState,
        models::user::GetPermissionManager,
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Serialize)]
    struct Response {
        #[schema(inline)]
        presets: Vec<crate::settings::GamePreset>,
        default_anonymous: bool,
        helper_configured: bool,
        steam_search_available: bool,
    }

    /// Returns the game presets and feature flags the Workshop tab needs to render.
    #[utoipa::path(get, path = "/", responses(
        (status = OK, body = inline(Response)),
    ), params(
        ("server" = uuid::Uuid, description = "The server ID"),
    ))]
    pub async fn route(state: GetState, permissions: GetPermissionManager) -> ApiResponseResult {
        permissions.has_server_permission("workshop.read")?;

        let settings = state.settings.get().await?;
        let ext: &crate::settings::ExtensionSettingsData = settings.find_extension_settings()?;

        ApiResponse::new_serialized(Response {
            presets: ext.game_presets.clone(),
            default_anonymous: ext.default_anonymous,
            helper_configured: !ext.helper_url.trim().is_empty()
                && !ext.helper_token.trim().is_empty(),
            steam_search_available: !ext.steam_api_key.trim().is_empty(),
        })
        .ok()
    }
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .routes(routes!(get::route))
        .with_state(state.clone())
}
