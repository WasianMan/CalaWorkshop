use axum::{extract::Query, routing};
use serde::{Deserialize, Serialize};
use shared::{
    GetState,
    models::{server::GetServer, user::GetPermissionManager},
    response::{ApiResponse, ApiResponseResult},
};
use utoipa_axum::router::OpenApiRouter;

use super::State;

const SEARCH_TTL_SECONDS: i64 = 300;

#[derive(Deserialize)]
struct SearchQuery {
    app_id: u32,
    q: Option<String>,
    sort: Option<String>,
    cursor: Option<String>,
    file_type: Option<String>,
    tags: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchPayload {
    items: Vec<crate::steam::WorkshopSearchResult>,
    next_cursor: Option<String>,
    total: Option<u64>,
    cached: bool,
}

async fn search(
    state: GetState,
    permissions: GetPermissionManager,
    _server: GetServer,
    Query(query): Query<SearchQuery>,
) -> ApiResponseResult {
    permissions.has_server_permission("workshop.read")?;
    if query.app_id == 0 {
        return Err(ApiResponse::error("app_id is required"));
    }

    let ext = {
        let settings = state.settings.get().await?;
        settings
            .find_extension_settings::<crate::settings::ExtensionSettingsData>()?
            .clone()
    };
    if ext.steam_api_key.trim().is_empty() {
        return Err(ApiResponse::error(
            "Steam Web API key is required for search",
        ));
    }
    if !ext.game_presets.iter().any(|p| p.app_id == query.app_id) {
        return Err(ApiResponse::error(
            "select a configured game before searching",
        ));
    }

    let trimmed_query = query.q.as_deref().map(str::trim).filter(|q| !q.is_empty());
    let sort = crate::steam::SearchSort::from_param(query.sort.as_deref(), trimmed_query.is_some());
    let file_type = crate::steam::MatchingFileType::from_param(query.file_type.as_deref());
    let tags: Vec<String> = query
        .tags
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .take(8)
        .map(str::to_string)
        .collect();
    let cache_key = format!(
        "app={};q={};sort={};cursor={};type={};tags={}",
        query.app_id,
        trimmed_query.unwrap_or(""),
        sort.cache_key(),
        query.cursor.as_deref().unwrap_or("").trim(),
        file_type.cache_key(),
        tags.join("|")
    );

    if let Some(cached) =
        crate::registry::get_cache_json(state.database.read(), "search", &cache_key).await?
    {
        if let Ok(mut payload) = serde_json::from_value::<SearchPayload>(cached) {
            payload.cached = true;
            return ApiResponse::new_serialized(payload).ok();
        }
    }

    let response = crate::steam::query_files(
        &state.client,
        ext.steam_api_key.as_str(),
        query.app_id,
        trimmed_query,
        sort,
        query.cursor.as_deref(),
        file_type,
        &tags,
    )
    .await
    .map_err(|err| ApiResponse::error(format!("Steam search failed: {err:#}")))?;

    let payload = SearchPayload {
        items: response.items,
        next_cursor: response.next_cursor,
        total: response.total,
        cached: false,
    };
    let cache_value = serde_json::to_value(&payload).unwrap_or_default();
    crate::registry::put_cache_json(
        state.database.write(),
        "search",
        &cache_key,
        &cache_value,
        SEARCH_TTL_SECONDS,
    )
    .await?;

    ApiResponse::new_serialized(payload).ok()
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .route("/", routing::get(search))
        .with_state(state.clone())
}
