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
        title: Option<String>,
        preview_url: Option<String>,
        file_name: Option<String>,
        #[schema(value_type = Vec<String>)]
        files: Vec<String>,
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
        Path((server_uuid, download)): Path<(uuid::Uuid, uuid::Uuid)>,
    ) -> ApiResponseResult {
        permissions.has_server_permission("workshop.read")?;

        let mut job = crate::registry::get_download(state.database.read(), server_uuid, download)
            .await?
            .ok_or_else(|| ApiResponse::error("unknown download"))?;

        if let Some(helper_job_id) = job.helper_job_id {
            let settings = state.settings.get().await?;
            let ext: &crate::settings::ExtensionSettingsData =
                settings.find_extension_settings()?;
            if let Some(helper) =
                crate::helper::HelperClient::new(&state.client, &ext.helper_url, &ext.helper_token)
            {
                if matches!(job.state.as_str(), "queued" | "downloading" | "ready") {
                    if let Ok(helper_job) = helper.get_job(helper_job_id).await {
                        crate::registry::update_download_from_helper(
                            state.database.write(),
                            job.id,
                            &helper_job.state,
                            helper_job.file_name.clone(),
                            helper_job.files.clone(),
                            helper_job.error.clone(),
                        )
                        .await?;
                        job.state = helper_job.state;
                        job.file_name = helper_job.file_name;
                        job.files = helper_job.files;
                        job.error = helper_job.error;
                    }
                }
            }
        }

        ApiResponse::new_serialized(Response {
            id: job.id,
            state: job.state,
            app_id: job.app_id as u32,
            workshop_id: job.workshop_id as u64,
            title: job.title,
            preview_url: job.preview_url,
            file_name: job.file_name,
            files: job.files,
            size: None,
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
