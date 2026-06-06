//! List / remove installed workshop content via Wings file APIs.
//!
//! These talk to the node directly and return Wings' own `DirectoryEntry`
//! shapes, so they're registered with plain axum routing (no `routes!`) to keep
//! them out of the OpenAPI surface and avoid re-deriving `ToSchema` for Wings types.

use super::State;
use axum::{
    extract::{Path, Query},
    routing,
};
use serde::Deserialize;
use shared::{
    GetState,
    models::{server::GetServer, user::GetPermissionManager},
    response::{ApiResponse, ApiResponseResult},
};
use utoipa_axum::router::OpenApiRouter;

#[derive(Deserialize)]
struct ListQuery {
    /// Directory inside the server volume to list.
    path: String,
}

/// `GET /installed?path=left4dead2/addons/workshop` — list a directory.
async fn list(
    state: GetState,
    permissions: GetPermissionManager,
    server: GetServer,
    Path(_server): Path<uuid::Uuid>,
    Query(query): Query<ListQuery>,
) -> ApiResponseResult {
    permissions.has_server_permission("workshop.read")?;
    let path = crate::validation::normalize_server_path(&query.path)?;

    let node = server.node.fetch_cached(&state.database).await?;
    let api = node.api_client(&state.database).await?;

    let entries = api
        .get_servers_server_files_list_directory(server.uuid, path.as_str())
        .await?;

    ApiResponse::new_serialized(serde_json::json!({ "entries": entries })).ok()
}

#[derive(Deserialize)]
struct DeletePayload {
    /// Directory the files live in.
    path: String,
    /// File/directory names within `path` to delete.
    files: Vec<compact_str::CompactString>,
}

/// `DELETE /installed` — remove installed files.
async fn remove(
    state: GetState,
    permissions: GetPermissionManager,
    server: GetServer,
    Path(_server): Path<uuid::Uuid>,
    shared::Payload(data): shared::Payload<DeletePayload>,
) -> ApiResponseResult {
    permissions.has_server_permission("workshop.remove")?;

    if data.files.is_empty() {
        return Err(ApiResponse::error("no files specified"));
    }
    let path = crate::validation::normalize_server_path(&data.path)?;
    for file in &data.files {
        crate::validation::validate_file_name(file)?;
    }

    let node = server.node.fetch_cached(&state.database).await?;
    let api = node.api_client(&state.database).await?;

    let result = api
        .post_servers_server_files_delete(
            server.uuid,
            &wings_api::servers_server_files_delete::post::RequestBody {
                root: path.into(),
                files: data.files,
            },
        )
        .await?;

    ApiResponse::new_serialized(serde_json::json!({ "deleted": result.deleted })).ok()
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .route("/", routing::get(list).delete(remove))
        .with_state(state.clone())
}
