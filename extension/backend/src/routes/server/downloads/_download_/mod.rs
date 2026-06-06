use super::State;
use utoipa_axum::{router::OpenApiRouter, routes};

mod install;

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
        id: uuid::Uuid,
        state: String,
        app_id: u32,
        workshop_id: u64,
        file_name: Option<String>,
        size: Option<u64>,
        error: Option<String>,
    }

    /// Poll a helper download job. The helper's internal `file_token` is never
    /// exposed to the client.
    #[utoipa::path(get, path = "/", responses(
        (status = OK, body = inline(Response)),
    ), params(
        ("server" = uuid::Uuid, description = "The server ID"),
        ("download" = uuid::Uuid, description = "The download job ID"),
    ))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        Path((_server, download)): Path<(uuid::Uuid, uuid::Uuid)>,
    ) -> ApiResponseResult {
        permissions.has_server_permission("workshop.read")?;

        let settings = state.settings.get().await?;
        let ext: &crate::settings::ExtensionSettingsData = settings.find_extension_settings()?;

        let helper = crate::helper::HelperClient::new(
            &state.client,
            &ext.helper_url,
            &ext.helper_token,
        )
        .ok_or_else(|| ApiResponse::error("workshop helper is not configured"))?;

        let job = helper.get_job(download).await?;

        ApiResponse::new_serialized(Response {
            id: job.id,
            state: job.state,
            app_id: job.app_id,
            workshop_id: job.workshop_id,
            file_name: job.file_name,
            size: job.size,
            error: job.error,
        })
        .ok()
    }
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .routes(routes!(get::route))
        .nest("/install", install::router(state))
        .with_state(state.clone())
}
