use super::State;
use utoipa_axum::{router::OpenApiRouter, routes};

mod _download_;

mod post {
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
        shared::Payload(data): shared::Payload<Payload>,
    ) -> ApiResponseResult {
        permissions.has_server_permission("workshop.install")?;

        if data.app_id == 0 || data.workshop_id == 0 {
            return Err(ApiResponse::error("app_id and workshop_id must be positive"));
        }

        let settings = state.settings.get().await?;
        let ext: &crate::settings::ExtensionSettingsData = settings.find_extension_settings()?;

        let helper = crate::helper::HelperClient::new(
            &state.client,
            &ext.helper_url,
            &ext.helper_token,
        )
        .ok_or_else(|| ApiResponse::error("workshop helper is not configured"))?;

        // Anonymous by default unless the caller picked a linked account.
        let account = data.account.filter(|a| !a.trim().is_empty());

        let resp = helper
            .start_download(&crate::helper::DownloadRequest {
                app_id: data.app_id,
                workshop_id: data.workshop_id,
                account,
                archive: data.archive,
            })
            .await?;

        ApiResponse::new_serialized(Response {
            job_id: resp.id,
            state: resp.state,
        })
        .ok()
    }
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .routes(routes!(post::route))
        .nest("/{download}", _download_::router(state))
        .with_state(state.clone())
}
