use super::State;
use utoipa_axum::{router::OpenApiRouter, routes};

mod get {
    use serde::Serialize;
    use shared::{
        GetState,
        models::{server::GetServer, server_variable::ServerVariable, user::GetPermissionManager},
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
        can_configure: bool,
        can_link_steam: bool,
        /// Best-effort Steam app id detected from this server's egg variables /
        /// startup, matched against a configured preset. `null` if unsure.
        detected_app_id: Option<u32>,
        /// `high` | `medium` | `low` when `detected_app_id` is set, else `null`.
        detected_app_id_confidence: Option<String>,
    }

    /// Returns the game presets and feature flags the Workshop tab needs to render.
    #[utoipa::path(get, path = "/", responses(
        (status = OK, body = inline(Response)),
    ), params(
        ("server" = uuid::Uuid, description = "The server ID"),
    ))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        server: GetServer,
    ) -> ApiResponseResult {
        permissions.has_server_permission("workshop.read")?;

        // Snapshot the bits we need and drop the settings read guard before any
        // further I/O (the variable lookup hits the database).
        let (presets, default_anonymous, helper_configured, steam_search_available) = {
            let settings = state.settings.get().await?;
            let ext: &crate::settings::ExtensionSettingsData = settings.find_extension_settings()?;
            (
                ext.game_presets.clone(),
                ext.default_anonymous,
                !ext.helper_url.trim().is_empty() && !ext.helper_token.trim().is_empty(),
                !ext.steam_api_key.trim().is_empty(),
            )
        };

        let (detected_app_id, detected_app_id_confidence) =
            detect_app_id(&state, &server, &presets).await;

        ApiResponse::new_serialized(Response {
            presets,
            default_anonymous,
            helper_configured,
            steam_search_available,
            can_configure: permissions
                .has_admin_permission("calaworkshop.configure")
                .is_ok(),
            can_link_steam: permissions
                .has_user_permission("calaworkshop.link-steam")
                .is_ok(),
            detected_app_id,
            detected_app_id_confidence: detected_app_id_confidence.map(|c| c.to_string()),
        })
        .ok()
    }

    /// Heuristically detect this server's Steam app id and how confident we are.
    ///
    /// Pterodactyl-style eggs don't carry an app id as a first-class field, so we
    /// read the server's egg variables + startup string and match candidate
    /// numbers against configured presets, scored:
    /// - **high:** a variable whose name looks like an app-id var (`*APP_ID*` etc.)
    /// - **medium:** a number next to `app_update`/`workshop_download_item`/`+app_id`
    ///   in the startup command
    /// - **low:** any numeric variable value that matches a preset
    ///
    /// The highest non-empty tier wins; an ambiguous tier (more than one distinct
    /// matching app id) yields `None` rather than a wrong guess.
    async fn detect_app_id(
        state: &shared::State,
        server: &shared::models::server::Server,
        presets: &[crate::settings::GamePreset],
    ) -> (Option<u32>, Option<&'static str>) {
        use std::collections::BTreeSet;

        if presets.is_empty() {
            return (None, None);
        }
        let known: BTreeSet<u32> = presets.iter().map(|p| p.app_id).collect();

        let mut high: BTreeSet<u32> = BTreeSet::new();
        let mut low: BTreeSet<u32> = BTreeSet::new();

        if let Ok(vars) =
            ServerVariable::all_by_server_uuid_egg_uuid(&state.database, server.uuid, server.egg.uuid)
                .await
        {
            for var in &vars {
                let Ok(value) = var.value.trim().parse::<u32>() else {
                    continue;
                };
                if !known.contains(&value) {
                    continue;
                }
                let name = var.variable.env_variable.to_ascii_uppercase();
                if ["APP_ID", "APPID", "STEAMCMD_APPID", "SRCDS_APPID", "STEAM_APP_ID"]
                    .iter()
                    .any(|needle| name.contains(needle))
                {
                    high.insert(value);
                } else {
                    low.insert(value);
                }
            }
        }

        let medium = scan_startup_app_ids(&server.startup, &known);

        for (tier, set) in [("high", &high), ("medium", &medium), ("low", &low)] {
            if set.len() == 1 {
                return (set.iter().next().copied(), Some(tier));
            }
            if !set.is_empty() {
                // Ambiguous at this tier — don't guess.
                return (None, None);
            }
        }
        (None, None)
    }

    /// Pull preset-matching numbers that appear right after a Steam app-id token.
    fn scan_startup_app_ids(
        startup: &str,
        known: &std::collections::BTreeSet<u32>,
    ) -> std::collections::BTreeSet<u32> {
        const TOKENS: [&str; 3] = ["app_update", "workshop_download_item", "+app_id"];
        let lower = startup.to_ascii_lowercase();
        let mut found = std::collections::BTreeSet::new();

        for token in TOKENS {
            let mut from = 0;
            while let Some(pos) = lower[from..].find(token) {
                let after = from + pos + token.len();
                // First run of digits following the token (skipping separators).
                let rest = &startup[after..];
                let digits: String = rest
                    .trim_start_matches(|c: char| c == ' ' || c == '=' || c == ':')
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                if let Ok(value) = digits.parse::<u32>() {
                    if known.contains(&value) {
                        found.insert(value);
                    }
                }
                from = after;
            }
        }
        found
    }
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .routes(routes!(get::route))
        .with_state(state.clone())
}
