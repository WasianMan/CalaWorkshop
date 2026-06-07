//! Normalized installed Workshop content registry plus Wings directory scanning.

use std::collections::HashSet;

use super::State;
use axum::{extract::Path, http::StatusCode, routing};
use serde::{Deserialize, Serialize};
use shared::{
    GetState,
    models::{server::GetServer, user::GetPermissionManager},
    response::{ApiResponse, ApiResponseResult},
};
use utoipa_axum::router::OpenApiRouter;

/// Max Steam metadata lookups performed inline per installed-list request.
const MAX_ENRICH_PER_REQUEST: usize = 24;

#[derive(Serialize)]
struct WorkshopItem {
    id: Option<uuid::Uuid>,
    title: String,
    app_id: u32,
    workshop_id: Option<u64>,
    install_path: String,
    vpk_file: Option<String>,
    image_file: Option<String>,
    files: Vec<String>,
    source: String,
}

async fn list(
    state: GetState,
    permissions: GetPermissionManager,
    server: GetServer,
    Path(server_uuid): Path<uuid::Uuid>,
) -> ApiResponseResult {
    permissions.has_server_permission("workshop.read")?;

    let mut items = Vec::new();
    let tracked = crate::registry::list_installed(state.database.read(), server_uuid).await?;
    let mut seen = HashSet::new();

    // Snapshot settings and drop the read guard before any Steam call below.
    let ext = {
        let settings = state.settings.get().await?;
        settings
            .find_extension_settings::<crate::settings::ExtensionSettingsData>()?
            .clone()
    };
    // Cap how many numeric/unnamed items we enrich per request so a slow Steam
    // API can never make listing crawl. Resolved titles persist, so repeated
    // loads converge; unresolved ones (private/deleted) retry a few at a time.
    let mut enrich_budget = MAX_ENRICH_PER_REQUEST;

    for mut item in tracked {
        for file in &item.files {
            seen.insert((item.install_path.clone(), file.clone()));
            if let Some((scan_path, name)) = scan_key_for_installed_file(&item.install_path, file) {
                seen.insert((scan_path, name));
            }
        }
        if let Some(workshop_id) = item.workshop_id.map(|id| id as u64) {
            if enrich_budget > 0
                && should_refresh_title(
                    item.title.as_deref(),
                    workshop_id,
                    item.vpk_file.as_deref(),
                )
            {
                enrich_budget -= 1;
                if let Some(metadata) = crate::steam::get_published_file_details(
                    &state.client,
                    ext.steam_api_key.as_str(),
                    workshop_id,
                )
                .await
                {
                    if let Some(title) = metadata.title {
                        crate::registry::update_installed_title(
                            state.database.write(),
                            item.id,
                            &title,
                        )
                        .await?;
                        item.title = Some(title);
                    }
                }
            }
        }
        items.push(WorkshopItem {
            id: Some(item.id),
            title: item.title.clone().unwrap_or_else(|| {
                item.workshop_id
                    .map(|id| id.to_string())
                    .or_else(|| item.vpk_file.clone())
                    .unwrap_or_else(|| "Workshop item".to_string())
            }),
            app_id: item.app_id as u32,
            workshop_id: item.workshop_id.map(|id| id as u64),
            install_path: item.install_path,
            vpk_file: item.vpk_file,
            image_file: item.image_file,
            files: item.files,
            source: item.source,
        });
    }

    for item in scan_unmanaged(&state, &server, &ext.game_presets, &seen).await? {
        items.push(item);
    }

    ApiResponse::new_serialized(serde_json::json!({ "items": items })).ok()
}

#[derive(Deserialize)]
struct ImportPayload {
    app_id: Option<u32>,
    workshop_id: Option<u64>,
    title: Option<String>,
    install_path: String,
    files: Vec<String>,
}

async fn import(
    state: GetState,
    permissions: GetPermissionManager,
    Path(server_uuid): Path<uuid::Uuid>,
    shared::Payload(data): shared::Payload<ImportPayload>,
) -> ApiResponseResult {
    permissions.has_server_permission("workshop.install")?;
    let install_path = crate::validation::normalize_server_path(&data.install_path)?;
    if data.files.is_empty() {
        return Err(ApiResponse::error("no files specified"));
    }
    let app_id = data
        .app_id
        .filter(|id| *id > 0)
        .ok_or_else(|| ApiResponse::error("app_id is required"))?;
    for file in &data.files {
        crate::validation::validate_file_name(file)?;
    }

    let mut title = data.title;
    if let Some(workshop_id) = data.workshop_id {
        if should_refresh_title(title.as_deref(), workshop_id, None) {
            // Snapshot settings and drop the read guard before the Steam call.
            let ext = {
                let settings = state.settings.get().await?;
                settings
                    .find_extension_settings::<crate::settings::ExtensionSettingsData>()?
                    .clone()
            };
            if let Some(metadata) = crate::steam::get_published_file_details(
                &state.client,
                ext.steam_api_key.as_str(),
                workshop_id,
            )
            .await
            {
                if metadata.title.is_some() {
                    title = metadata.title;
                }
            }
        }
    }

    let item = crate::registry::create_installed(
        state.database.write(),
        server_uuid,
        app_id,
        data.workshop_id,
        title,
        &install_path,
        data.files,
        "imported",
    )
    .await?;

    ApiResponse::new_serialized(item).ok()
}

async fn remove(
    state: GetState,
    permissions: GetPermissionManager,
    server: GetServer,
    Path((server_uuid, installed_id)): Path<(uuid::Uuid, uuid::Uuid)>,
) -> ApiResponseResult {
    permissions.has_server_permission("workshop.remove")?;
    let item = crate::registry::get_installed(state.database.read(), server_uuid, installed_id)
        .await?
        .ok_or_else(|| ApiResponse::error("unknown installed item"))?;

    let node = server.node.fetch_cached(&state.database).await?;
    let api = node.api_client(&state.database).await?;
    let result = api
        .post_servers_server_files_delete(
            server.uuid,
            &wings_api::servers_server_files_delete::post::RequestBody {
                root: item.install_path.clone().into(),
                files: item.files.iter().cloned().map(Into::into).collect(),
            },
        )
        .await?;

    crate::registry::delete_installed(state.database.write(), server_uuid, installed_id).await?;
    ApiResponse::new_serialized(serde_json::json!({ "deleted": result.deleted })).ok()
}

async fn preview(
    state: GetState,
    permissions: GetPermissionManager,
    Path((server_uuid, installed_id)): Path<(uuid::Uuid, uuid::Uuid)>,
) -> ApiResponseResult {
    permissions.has_server_permission("workshop.read")?;
    let item = crate::registry::get_installed(state.database.read(), server_uuid, installed_id)
        .await?
        .ok_or_else(|| ApiResponse::error("unknown installed item"))?;
    if item.image_file.is_none() {
        return ApiResponse::error("installed item has no local preview")
            .with_status(StatusCode::NOT_FOUND)
            .ok();
    }

    ApiResponse::error("local preview streaming requires a Wings file-contents API binding")
        .with_status(StatusCode::NOT_IMPLEMENTED)
        .ok()
}

async fn scan_unmanaged(
    state: &GetState,
    server: &GetServer,
    presets: &[crate::settings::GamePreset],
    seen: &HashSet<(String, String)>,
) -> Result<Vec<WorkshopItem>, anyhow::Error> {
    let node = server.node.fetch_cached(&state.database).await?;
    let api = node.api_client(&state.database).await?;
    let mut out = Vec::new();

    for preset in presets {
        for scan in &preset.scan {
            let path = scan.path.as_str();
            let entries = match api
                .get_servers_server_files_list_directory(server.uuid, path)
                .await
            {
                Ok(entries) => entries,
                Err(_) => continue,
            };
            let names = entry_names(serde_json::to_value(entries)?);
            let mut used_images = HashSet::new();
            let image_exts = ["jpg", "jpeg", "png"];
            let primary_names: Vec<String> = names
                .iter()
                .filter(|name| {
                    scan_matches(name, scan) && !image_exts.iter().any(|ext| ext_is(name, ext))
                })
                .cloned()
                .collect();

            for file in primary_names {
                if seen.contains(&(path.to_string(), file.clone())) {
                    continue;
                }
                let stem = file_stem(&file);
                let image = names
                    .iter()
                    .find(|name| {
                        file_stem(name) == stem
                            && (ext_is(name, "jpg") || ext_is(name, "jpeg") || ext_is(name, "png"))
                    })
                    .cloned();
                if let Some(image) = &image {
                    used_images.insert(image.clone());
                }
                let mut files = vec![file.clone()];
                if let Some(image) = &image {
                    files.push(image.clone());
                }
                out.push(WorkshopItem {
                    id: None,
                    title: workshop_id_from_name(&file).unwrap_or_else(|| file.clone()),
                    app_id: preset.app_id,
                    workshop_id: workshop_id_from_name(&file).and_then(|id| id.parse().ok()),
                    install_path: path.to_string(),
                    vpk_file: ext_is(&file, "vpk").then_some(file.clone()),
                    image_file: image,
                    files,
                    source: "unmanaged".to_string(),
                });
            }

            for image in names.iter().filter(|name| {
                scan_matches(name, scan)
                    && (ext_is(name, "jpg") || ext_is(name, "jpeg") || ext_is(name, "png"))
                    && !used_images.contains(*name)
                    && !seen.contains(&(path.to_string(), (*name).clone()))
            }) {
                out.push(WorkshopItem {
                    id: None,
                    title: image.clone(),
                    app_id: preset.app_id,
                    workshop_id: workshop_id_from_name(image).and_then(|id| id.parse().ok()),
                    install_path: path.to_string(),
                    vpk_file: None,
                    image_file: Some(image.clone()),
                    files: vec![image.clone()],
                    source: "unmanaged".to_string(),
                });
            }
        }
    }

    Ok(out)
}

fn scan_key_for_installed_file(install_path: &str, file: &str) -> Option<(String, String)> {
    let normalized_file = file.replace('\\', "/");
    let (dir, name) = normalized_file.rsplit_once('/')?;
    Some((
        format!("{}/{}", install_path.trim_end_matches('/'), dir),
        name.to_string(),
    ))
}

fn scan_matches(name: &str, scan: &crate::settings::ScanRule) -> bool {
    let extension_match =
        scan.extensions.is_empty() || scan.extensions.iter().any(|ext| ext_is(name, ext));
    let glob_match = scan
        .glob
        .as_deref()
        .map(|pattern| simple_glob_match(pattern, name))
        .unwrap_or(true);
    extension_match && glob_match
}

fn simple_glob_match(pattern: &str, name: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix('*') {
        return name.ends_with(suffix);
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return name.starts_with(prefix);
    }
    pattern.eq_ignore_ascii_case(name)
}

fn entry_names(value: serde_json::Value) -> Vec<String> {
    let entries = value
        .get("entries")
        .and_then(|v| v.as_array())
        .or_else(|| value.as_array());

    entries
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            entry
                .get("name")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .collect()
}

fn ext_is(file: &str, ext: &str) -> bool {
    file.rsplit('.')
        .next()
        .map(|actual| actual.eq_ignore_ascii_case(ext))
        .unwrap_or(false)
}

fn file_stem(file: &str) -> &str {
    file.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(file)
}

fn workshop_id_from_name(file: &str) -> Option<String> {
    let stem = file_stem(file);
    let id: String = stem
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    (!id.is_empty()).then_some(id)
}

fn should_refresh_title(title: Option<&str>, workshop_id: u64, vpk_file: Option<&str>) -> bool {
    let Some(title) = title.map(str::trim).filter(|title| !title.is_empty()) else {
        return true;
    };
    let id = workshop_id.to_string();
    if title == id {
        return true;
    }
    if let Some(vpk_file) = vpk_file {
        return title == vpk_file || title == file_stem(vpk_file);
    }
    false
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .route("/", routing::get(list))
        .route("/import", routing::post(import))
        .route("/{installed_id}", routing::delete(remove))
        .route("/{installed_id}/preview", routing::get(preview))
        .with_state(state.clone())
}
