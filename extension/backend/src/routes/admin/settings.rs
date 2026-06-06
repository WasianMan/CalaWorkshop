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
        helper_url: String,
        /// Secrets are never returned; only whether they are set.
        helper_token_set: bool,
        steam_api_key_set: bool,
        default_anonymous: bool,
        #[schema(inline)]
        game_presets: Vec<crate::settings::GamePreset>,
    }

    /// Read the extension's admin configuration (secrets masked).
    #[utoipa::path(get, path = "/", responses(
        (status = OK, body = inline(Response)),
    ))]
    pub async fn route(state: GetState, permissions: GetPermissionManager) -> ApiResponseResult {
        permissions.has_admin_permission("calaworkshop.configure")?;

        let settings = state.settings.get().await?;
        let ext: &crate::settings::ExtensionSettingsData = settings.find_extension_settings()?;

        ApiResponse::new_serialized(Response {
            helper_url: ext.helper_url.clone(),
            helper_token_set: !ext.helper_token.trim().is_empty(),
            steam_api_key_set: !ext.steam_api_key.trim().is_empty(),
            default_anonymous: ext.default_anonymous,
            game_presets: ext.game_presets.clone(),
        })
        .ok()
    }
}

mod put {
    use serde::{Deserialize, Serialize};
    use shared::{
        GetState,
        models::user::GetPermissionManager,
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Deserialize)]
    pub struct Payload {
        /// Any field left out is unchanged. An explicit empty string clears a secret.
        helper_url: Option<String>,
        helper_token: Option<String>,
        steam_api_key: Option<String>,
        default_anonymous: Option<bool>,
        #[schema(inline)]
        game_presets: Option<Vec<crate::settings::GamePreset>>,
    }

    #[derive(ToSchema, Serialize)]
    struct Response {
        saved: bool,
    }

    /// Update the extension's admin configuration.
    #[utoipa::path(put, path = "/", responses(
        (status = OK, body = inline(Response)),
    ), request_body = inline(Payload))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        shared::Payload(data): shared::Payload<Payload>,
    ) -> ApiResponseResult {
        permissions.has_admin_permission("calaworkshop.configure")?;

        let mut settings = state.settings.get_mut().await?;
        let ext: &mut crate::settings::ExtensionSettingsData =
            settings.find_mut_extension_settings()?;

        if let Some(helper_url) = data.helper_url {
            ext.helper_url = helper_url;
        }
        if let Some(helper_token) = data.helper_token {
            ext.helper_token = helper_token.into();
        }
        if let Some(steam_api_key) = data.steam_api_key {
            ext.steam_api_key = steam_api_key.into();
        }
        if let Some(default_anonymous) = data.default_anonymous {
            ext.default_anonymous = default_anonymous;
        }
        if let Some(game_presets) = data.game_presets {
            ext.game_presets = game_presets;
        }

        settings.save().await?;

        ApiResponse::new_serialized(Response { saved: true }).ok()
    }
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .routes(routes!(get::route))
        .routes(routes!(put::route))
        .with_state(state.clone())
}
