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
        models::user::GetPermissionManager,
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
        Path(server): Path<uuid::Uuid>,
        shared::Payload(data): shared::Payload<Payload>,
    ) -> ApiResponseResult {
        permissions.has_server_permission("workshop.install")?;

        if data.app_id == 0 || data.workshop_id == 0 {
            return Err(ApiResponse::error(
                "app_id and workshop_id must be positive",
            ));
        }

        let settings = state.settings.get().await?;
        let ext: &crate::settings::ExtensionSettingsData = settings.find_extension_settings()?;

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
        let job = crate::registry::create_download(
            state.database.write(),
            server,
            data.app_id,
            data.workshop_id,
            metadata,
        )
        .await?;

        let helper =
            crate::helper::HelperClient::new(&state.client, &ext.helper_url, &ext.helper_token)
                .ok_or_else(|| ApiResponse::error("workshop helper is not configured"))?;

        // Anonymous by default unless the caller picked a linked account. Linked
        // helper sessions are currently global, so account-backed downloads stay
        // admin-only until the steam_links ownership table is enforced.
        let account = data.account.and_then(|a| {
            let label = a.trim().to_string();
            (!label.is_empty()).then_some(label)
        });
        if let Some(label) = &account {
            permissions.has_admin_permission("calaworkshop.configure")?;
            crate::validation::validate_account_label(label)?;
        }

        let resp = match helper
            .start_download(&crate::helper::DownloadRequest {
                app_id: data.app_id,
                workshop_id: data.workshop_id,
                account,
                archive: data.archive,
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
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .routes(routes!(get::route))
        .routes(routes!(post::route))
        .nest("/{download}", _download_::router(state))
        .with_state(state.clone())
}
