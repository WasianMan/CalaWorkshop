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
        /// server root), e.g. `left4dead2/addons/workshop`.
        install_path: String,
        /// Whether the helper artifact is a zip that must be decompressed in place.
        #[serde(default)]
        archive: bool,
    }

    #[derive(ToSchema, Serialize)]
    struct Response {
        installed: bool,
        file_name: String,
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
        mut server: GetServer,
        activity_logger: GetServerActivityLogger,
        Path((_server, download)): Path<(uuid::Uuid, uuid::Uuid)>,
        shared::Payload(data): shared::Payload<Payload>,
    ) -> ApiResponseResult {
        permissions.has_server_permission("workshop.install")?;

        let install_path = data.install_path.trim().trim_start_matches('/').to_string();
        if install_path.is_empty() || install_path.split('/').any(|seg| seg == "..") {
            return Err(ApiResponse::error("invalid install path"));
        }

        let settings = state.settings.get().await?;
        let ext: &crate::settings::ExtensionSettingsData = settings.find_extension_settings()?;

        let helper = crate::helper::HelperClient::new(
            &state.client,
            &ext.helper_url,
            &ext.helper_token,
        )
        .ok_or_else(|| ApiResponse::error("workshop helper is not configured"))?;

        let job = helper.get_job(download).await?;
        if job.state != "ready" {
            return ApiResponse::error(format!(
                "download is not ready (state: {})",
                job.state
            ))
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

        if data.archive {
            api.post_servers_server_files_decompress(
                server.uuid,
                &wings_api::servers_server_files_decompress::post::RequestBody {
                    root: install_path.clone().into(),
                    file: file_name.clone().into(),
                    foreground: true,
                },
            )
            .await?;

            // Remove the archive once extracted; ignore failures (best-effort cleanup).
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

        activity_logger
            .log(
                "calaworkshop:install",
                serde_json::json!({
                    "app_id": job.app_id,
                    "workshop_id": job.workshop_id,
                    "directory": install_path,
                    "file": file_name,
                }),
            )
            .await;

        ApiResponse::new_serialized(Response {
            installed: true,
            file_name,
        })
        .ok()
    }
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .routes(routes!(post::route))
        .with_state(state.clone())
}
