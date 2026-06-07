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
            crate::validation::validate_game_presets(&game_presets)?;
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

pub mod diagnostics {
    use serde::Serialize;
    use shared::{
        GetState,
        models::user::GetPermissionManager,
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Serialize)]
    struct Check {
        ok: bool,
        message: Option<String>,
        error: Option<String>,
    }

    #[derive(ToSchema, Serialize)]
    struct Response {
        helper: Check,
        steamcmd: Check,
    }

    #[utoipa::path(get, path = "/", responses(
        (status = OK, body = inline(Response)),
    ))]
    pub async fn route(state: GetState, permissions: GetPermissionManager) -> ApiResponseResult {
        permissions.has_admin_permission("calaworkshop.configure")?;
        // Snapshot settings and drop the read guard before the helper calls.
        let ext = {
            let settings = state.settings.get().await?;
            settings
                .find_extension_settings::<crate::settings::ExtensionSettingsData>()?
                .clone()
        };
        let Some(helper) =
            crate::helper::HelperClient::new(&state.client, &ext.helper_url, &ext.helper_token)
        else {
            return ApiResponse::new_serialized(Response {
                helper: Check {
                    ok: false,
                    message: None,
                    error: Some("workshop helper is not configured".to_string()),
                },
                steamcmd: Check {
                    ok: false,
                    message: None,
                    error: Some("workshop helper is not configured".to_string()),
                },
            })
            .ok();
        };

        let helper_check = match helper.health().await {
            Ok(_) => Check {
                ok: true,
                message: Some("helper reachable".to_string()),
                error: None,
            },
            Err(err) => Check {
                ok: false,
                message: None,
                error: Some(format!("{err:#}")),
            },
        };

        let steamcmd_check = match helper.steamcmd_check().await {
            Ok(value) => Check {
                ok: value.get("ok").and_then(|v| v.as_bool()).unwrap_or(false),
                message: value
                    .get("message")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                error: value
                    .get("error")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
            },
            Err(err) => Check {
                ok: false,
                message: None,
                error: Some(format!("{err:#}")),
            },
        };

        ApiResponse::new_serialized(Response {
            helper: helper_check,
            steamcmd: steamcmd_check,
        })
        .ok()
    }
}

pub fn diagnostics_router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .routes(routes!(diagnostics::route))
        .with_state(state.clone())
}
