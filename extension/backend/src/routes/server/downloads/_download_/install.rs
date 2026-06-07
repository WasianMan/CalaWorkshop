use super::State;
use utoipa_axum::{router::OpenApiRouter, routes};

mod post {
    use axum::{extract::Path, http::StatusCode};
    use serde::{Deserialize, Serialize};
    use shared::{
        GetState,
        models::{
            server::{GetServer, GetServerActivityLogger},
            user::GetPermissionManager,
        },
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Deserialize)]
    pub struct Payload {
        /// Destination directory inside the server volume (relative to the
        /// server root), e.g. `left4dead2/addons`.
        install_path: String,
        /// Whether the helper artifact is a zip that must be decompressed in place.
        #[serde(default)]
        archive: bool,
    }

    #[derive(ToSchema, Serialize)]
    struct Response {
        installed: bool,
        file_name: String,
        #[schema(value_type = Vec<String>)]
        files: Vec<String>,
    }

    /// Place a finished download into the server via Wings: pull the helper's
    /// `/files` URL into `install_path`, then (for archives) decompress + clean up.
    #[utoipa::path(post, path = "/", responses(
        (status = OK, body = inline(Response)),
    ), params(
        ("server" = uuid::Uuid, description = "The server ID"),
        ("download" = uuid::Uuid, description = "The download job ID"),
    ), request_body = inline(Payload))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        server: GetServer,
        activity_logger: GetServerActivityLogger,
        Path((_server, download)): Path<(uuid::Uuid, uuid::Uuid)>,
        shared::Payload(data): shared::Payload<Payload>,
    ) -> ApiResponseResult {
        permissions.has_server_permission("workshop.install")?;

        let install_path = crate::validation::normalize_server_path(&data.install_path)?;

        let db_job = crate::registry::get_download(&state.database, _server, download)
            .await?
            .ok_or_else(|| ApiResponse::error("unknown download"))?;
        let helper_job_id = db_job
            .helper_job_id
            .ok_or_else(|| ApiResponse::error("download has no helper job"))?;

        let settings = state.settings.get().await?;
        let ext: &crate::settings::ExtensionSettingsData = settings.find_extension_settings()?;
        let helper =
            crate::helper::HelperClient::new(&state.client, &ext.helper_url, &ext.helper_token)
                .ok_or_else(|| ApiResponse::error("workshop helper is not configured"))?;

        let job = helper.get_job(helper_job_id).await?;
        if job.state != "ready" {
            return ApiResponse::error(format!("download is not ready (state: {})", job.state))
                .with_status(StatusCode::CONFLICT)
                .ok();
        }

        let file_name = job
            .file_name
            .clone()
            .ok_or_else(|| ApiResponse::error("helper job is ready but has no file"))?;

        let file_url = helper.file_url(job.id, &job.file_token);

        // Hand the helper URL to Wings so the node fetches it straight into the
        // server volume — works on AIO and remote nodes alike. `foreground` so we
        // can decompress/clean up synchronously afterwards.
        let node = server.node.fetch_cached(&state.database).await?;
        let api = node.api_client(&state.database).await?;

        api.post_servers_server_files_pull(
            server.uuid,
            &wings_api::servers_server_files_pull::post::RequestBody {
                root: install_path.clone().into(),
                url: file_url.into(),
                file_name: Some(file_name.clone().into()),
                use_header: false,
                foreground: true,
            },
        )
        .await?;

        let should_decompress =
            data.archive || file_name.ends_with(".zip") || !job.files.is_empty();
        if should_decompress {
            api.post_servers_server_files_decompress(
                server.uuid,
                &wings_api::servers_server_files_decompress::post::RequestBody {
                    root: install_path.clone().into(),
                    file: file_name.clone().into(),
                    foreground: true,
                },
            )
            .await?;

            // Remove the transfer archive once extracted; ignore failures (best-effort cleanup).
            let _ = api
                .post_servers_server_files_delete(
                    server.uuid,
                    &wings_api::servers_server_files_delete::post::RequestBody {
                        root: install_path.clone().into(),
                        files: vec![file_name.clone().into()],
                    },
                )
                .await;
        }

        let installed_files = if job.files.is_empty() {
            vec![file_name.clone()]
        } else {
            job.files.clone()
        };

        let installed = crate::registry::create_installed(
            &state.database,
            server.uuid,
            job.app_id as u32,
            Some(job.workshop_id),
            db_job.title.clone(),
            &install_path,
            installed_files.clone(),
            "managed",
        )
        .await?;
        crate::registry::mark_download_installed(&state.database, download, &install_path).await?;

        activity_logger
            .log(
                "calaworkshop:install",
                serde_json::json!({
                    "app_id": job.app_id,
                    "workshop_id": job.workshop_id,
                    "directory": install_path,
                    "file": file_name,
                    "installed_id": installed.id,
                    "files": installed_files.clone(),
                }),
            )
            .await;

        ApiResponse::new_serialized(Response {
            installed: true,
            file_name,
            files: installed_files,
        })
        .ok()
    }
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .routes(routes!(post::route))
        .with_state(state.clone())
}
