use super::State;
use utoipa_axum::{router::OpenApiRouter, routes};

mod _download_;

mod get {
    use axum::extract::Path;
    use serde::Serialize;
    use shared::{
        GetState,
        models::user::GetPermissionManager,
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Serialize)]
    struct Response {
        #[schema(value_type = Vec<Object>)]
        jobs: Vec<crate::registry::DownloadJob>,
    }

    /// List active and recent persisted Workshop download jobs for this server.
    #[utoipa::path(get, path = "/", responses(
        (status = OK, body = inline(Response)),
    ), params(
        ("server" = uuid::Uuid, description = "The server ID"),
    ))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        Path(server): Path<uuid::Uuid>,
    ) -> ApiResponseResult {
        permissions.has_server_permission("workshop.read")?;
        let jobs = crate::registry::recent_downloads(state.database.read(), server).await?;
        ApiResponse::new_serialized(Response { jobs }).ok()
    }
}

mod post {
    use axum::extract::Path;
    use serde::{Deserialize, Serialize};
    use shared::{
        GetState,
        models::user::{GetPermissionManager, GetUser},
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Deserialize)]
    pub struct Payload {
        /// Steam app id. Defaults to the resolved preset on the frontend, but the
        /// backend requires it explicitly.
        app_id: u32,
        /// Workshop item id.
        workshop_id: u64,
        /// Linked Steam account label to download as, or null for anonymous.
        #[serde(default)]
        account: Option<String>,
        /// Zip the whole item folder instead of serving a single file.
        #[serde(default)]
        archive: bool,
    }

    #[derive(ToSchema, Serialize)]
    struct Response {
        job_id: uuid::Uuid,
        state: String,
    }

    /// Kick off a workshop download on the helper. Returns a job id to poll.
    #[utoipa::path(post, path = "/", responses(
        (status = OK, body = inline(Response)),
    ), params(
        ("server" = uuid::Uuid, description = "The server ID"),
    ), request_body = inline(Payload))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        user: GetUser,
        Path(server): Path<uuid::Uuid>,
        shared::Payload(data): shared::Payload<Payload>,
    ) -> ApiResponseResult {
        permissions.has_server_permission("workshop.install")?;

        if data.app_id == 0 || data.workshop_id == 0 {
            return Err(ApiResponse::error(
                "app_id and workshop_id must be positive",
            ));
        }

        // Snapshot settings into an owned value and drop the read guard before any
        // network I/O — holding it across helper/Steam calls can stall the panel.
        let ext = {
            let settings = state.settings.get().await?;
            settings
                .find_extension_settings::<crate::settings::ExtensionSettingsData>()?
                .clone()
        };

        // Resolve the game preset for this app id (if any). It drives the install
        // rule sent to the helper and the post-install behavior we persist. An
        // unconfigured app id falls back to "mirror every file" / no post-install.
        let preset = ext.game_presets.iter().find(|p| p.app_id == data.app_id);
        let install_rule = crate::helper::InstallRulePayload {
            matchers: preset.map(|p| p.r#match.clone()).unwrap_or_default(),
            generated_files: preset
                .map(|p| p.generated_files.clone())
                .unwrap_or_default(),
        };
        let post_install = match preset.map(|p| p.post_install) {
            Some(crate::settings::PostInstall::Extract) => "extract",
            _ => "none",
        };
        let requires_account = match preset.map(|p| p.auth).unwrap_or_default() {
            crate::settings::AuthRequirement::Account => true,
            crate::settings::AuthRequirement::Anonymous => false,
            crate::settings::AuthRequirement::Default => !ext.default_anonymous,
        };

        // Resolve the (optional) linked account to its opaque helper label,
        // scoped to the calling user. A user can only download as an account they
        // personally linked; anonymous otherwise.
        let account = match data
            .account
            .as_deref()
            .map(str::trim)
            .filter(|a| !a.is_empty())
        {
            Some(label) => {
                permissions.has_user_permission("calaworkshop.link-steam")?;
                crate::validation::validate_account_label(label)?;
                let link =
                    crate::steam_links::get_by_label(state.database.read(), user.uuid, label)
                        .await?
                        .ok_or_else(|| {
                            ApiResponse::error(
                                "you have not linked a Steam account with that label",
                            )
                        })?;
                Some(link.helper_label)
            }
            None if requires_account => {
                return Err(ApiResponse::error(
                    "this game requires a linked Steam account; select one before downloading",
                ));
            }
            None => None,
        };

        let metadata = crate::steam::get_published_file_details(
            &state.client,
            ext.steam_api_key.as_str(),
            data.workshop_id,
        )
        .await
        .unwrap_or(crate::registry::WorkshopMetadata {
            title: None,
            preview_url: None,
        });
        let title_slug = metadata
            .title
            .as_deref()
            .map(slugify_title)
            .filter(|slug| !slug.is_empty());
        let job = crate::registry::create_download(
            state.database.write(),
            server,
            data.app_id,
            data.workshop_id,
            metadata,
            post_install,
        )
        .await?;

        let helper =
            crate::helper::HelperClient::new(&state.client, &ext.helper_url, &ext.helper_token)
                .ok_or_else(|| ApiResponse::error("workshop helper is not configured"))?;

        let resp = match helper
            .start_download(&crate::helper::DownloadRequest {
                app_id: data.app_id,
                workshop_id: data.workshop_id,
                account,
                archive: data.archive,
                title_slug,
                install_rule,
            })
            .await
        {
            Ok(resp) => resp,
            Err(err) => {
                crate::registry::update_download_helper(
                    state.database.write(),
                    job.id,
                    None,
                    "failed",
                    Some(format!("{err:#}")),
                )
                .await?;
                return Err(ApiResponse::error(format!("{err:#}")));
            }
        };

        crate::registry::update_download_helper(
            state.database.write(),
            job.id,
            Some(resp.id),
            &resp.state,
            None,
        )
        .await?;

        ApiResponse::new_serialized(Response {
            job_id: job.id,
            state: resp.state,
        })
        .ok()
    }

    fn slugify_title(title: &str) -> String {
        let mut out = String::new();
        let mut last_was_sep = false;
        for ch in title.chars().flat_map(char::to_lowercase) {
            if ch.is_ascii_alphanumeric() {
                out.push(ch);
                last_was_sep = false;
            } else if !last_was_sep && !out.is_empty() {
                out.push('_');
                last_was_sep = true;
            }
            if out.len() >= 80 {
                break;
            }
        }
        out.trim_matches('_').to_string()
    }
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .routes(routes!(get::route))
        .routes(routes!(post::route))
        .nest("/{download}", _download_::router(state))
        .with_state(state.clone())
}
