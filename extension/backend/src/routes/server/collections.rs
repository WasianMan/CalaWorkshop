use axum::{extract::Path, routing};
use serde::{Deserialize, Serialize};
use shared::{
    GetState,
    models::{server::GetServer, user::GetPermissionManager, user::GetUser},
    response::{ApiResponse, ApiResponseResult},
};
use utoipa_axum::router::OpenApiRouter;

use super::State;

const COLLECTION_TTL_SECONDS: i64 = 600;
const MAX_COLLECTION_ITEMS: usize = 500;

#[derive(Deserialize)]
struct CollectionPayload {
    app_id: u32,
    collection_id: u64,
    #[serde(default)]
    account: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewResponse {
    collection: Option<crate::steam::WorkshopSearchResult>,
    children: Vec<crate::steam::WorkshopSearchResult>,
    skipped: Vec<crate::steam::CollectionSkippedItem>,
    cached: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallResponse {
    collection_id: u64,
    jobs: Vec<super::downloads::post::Response>,
    skipped: Vec<crate::steam::CollectionSkippedItem>,
}

async fn preview(
    state: GetState,
    permissions: GetPermissionManager,
    _server: GetServer,
    shared::Payload(data): shared::Payload<CollectionPayload>,
) -> ApiResponseResult {
    permissions.has_server_permission("workshop.read")?;
    let payload = load_collection_preview(&state, data.app_id, data.collection_id).await?;
    ApiResponse::new_serialized(payload).ok()
}

async fn install(
    state: GetState,
    permissions: GetPermissionManager,
    user: GetUser,
    _server: GetServer,
    Path(server_uuid): Path<uuid::Uuid>,
    shared::Payload(data): shared::Payload<CollectionPayload>,
) -> ApiResponseResult {
    permissions.has_server_permission("workshop.install")?;
    let payload = load_collection_preview(&state, data.app_id, data.collection_id).await?;
    if payload.children.is_empty() {
        return Err(ApiResponse::error("collection has no installable children"));
    }

    // Re-adding a collection should only fetch what's missing. Items already in
    // the installed registry for this server are skipped, so a half-finished
    // 100+ item install can be resumed by simply installing the collection again
    // instead of re-downloading everything.
    let already_installed: std::collections::HashSet<i64> =
        crate::registry::list_installed(state.database.read(), server_uuid)
            .await?
            .into_iter()
            .filter_map(|item| item.workshop_id)
            .collect();

    let mut jobs = Vec::new();
    let mut skipped = payload.skipped;
    for item in payload.children.iter().take(MAX_COLLECTION_ITEMS) {
        if already_installed.contains(&(item.published_file_id as i64)) {
            skipped.push(crate::steam::CollectionSkippedItem {
                published_file_id: item.published_file_id,
                reason: "already installed".to_string(),
            });
            continue;
        }
        // Reuse the title/preview already returned by the collection preview so we
        // don't make a Steam Web API metadata call per child — that storm is what
        // rate-limited big collections.
        let known = crate::registry::WorkshopMetadata {
            title: Some(item.title.clone()),
            preview_url: item.preview_url.clone(),
        };
        let job = super::downloads::post::start_download_for_item(
            &state,
            &permissions,
            user.uuid,
            server_uuid,
            data.app_id,
            item.published_file_id,
            data.account.as_deref(),
            false,
            Some(known),
        )
        .await?;
        jobs.push(job);
    }

    ApiResponse::new_serialized(InstallResponse {
        collection_id: data.collection_id,
        jobs,
        skipped,
    })
    .ok()
}

async fn load_collection_preview(
    state: &GetState,
    app_id: u32,
    collection_id: u64,
) -> Result<PreviewResponse, ApiResponse> {
    if app_id == 0 || collection_id == 0 {
        return Err(ApiResponse::error("app_id and collection_id are required"));
    }
    let ext = {
        let settings = state.settings.get().await?;
        settings
            .find_extension_settings::<crate::settings::ExtensionSettingsData>()?
            .clone()
    };
    if !ext.game_presets.iter().any(|p| p.app_id == app_id) {
        return Err(ApiResponse::error(
            "select a configured game before loading a collection",
        ));
    }

    let cache_key = format!("app={app_id};collection={collection_id}");
    if let Some(cached) =
        crate::registry::get_cache_json(state.database.read(), "collection", &cache_key).await?
    {
        if let Ok(mut payload) = serde_json::from_value::<PreviewResponse>(cached) {
            payload.cached = true;
            return Ok(payload);
        }
    }

    let preview = crate::steam::get_collection_preview(
        &state.client,
        ext.steam_api_key.as_str(),
        collection_id,
    )
    .await
    .map_err(|err| ApiResponse::error(format!("Steam collection lookup failed: {err:#}")))?;

    let mut children = preview.children;
    let mut skipped = preview.skipped;
    if children.len() > MAX_COLLECTION_ITEMS {
        for item in children.drain(MAX_COLLECTION_ITEMS..) {
            skipped.push(crate::steam::CollectionSkippedItem {
                published_file_id: item.published_file_id,
                reason: format!("collection install is capped at {MAX_COLLECTION_ITEMS} items"),
            });
        }
    }

    let payload = PreviewResponse {
        collection: preview.collection,
        children,
        skipped,
        cached: false,
    };
    let cache_value = serde_json::to_value(&payload).unwrap_or_default();
    crate::registry::put_cache_json(
        state.database.write(),
        "collection",
        &cache_key,
        &cache_value,
        COLLECTION_TTL_SECONDS,
    )
    .await?;
    Ok(payload)
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .route("/preview", routing::post(preview))
        .route("/install", routing::post(install))
        .with_state(state.clone())
}
