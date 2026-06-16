use super::State;
use utoipa_axum::{router::OpenApiRouter, routes};

mod _download_;

mod get {
    use axum::extract::{Path, Query};
    use serde::{Deserialize, Serialize};
    use shared::{
        GetState,
        models::user::GetPermissionManager,
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    const DEFAULT_PER_PAGE: i64 = 25;
    const MAX_PER_PAGE: i64 = 100;

    #[derive(Deserialize, utoipa::IntoParams)]
    #[into_params(parameter_in = Query)]
    pub struct ListQuery {
        /// 1-based history page. Active jobs are always returned in full.
        #[serde(default)]
        page: Option<i64>,
        /// History rows per page (clamped 1..=100).
        #[serde(default)]
        per_page: Option<i64>,
    }

    #[derive(ToSchema, Serialize)]
    struct Response {
        /// In-flight jobs (queued / downloading / ready), always returned in full.
        #[schema(value_type = Vec<Object>)]
        active: Vec<crate::registry::DownloadJob>,
        /// One page of terminal jobs (installed / failed), newest first.
        #[schema(value_type = Vec<Object>)]
        history: Vec<crate::registry::DownloadJob>,
        /// Total terminal jobs, for pagination.
        history_total: i64,
        page: i64,
        per_page: i64,
        /// Back-compat alias of `active` for older clients. Deprecated.
        #[schema(value_type = Vec<Object>)]
        jobs: Vec<crate::registry::DownloadJob>,
    }

    /// List Workshop download jobs for this server: all active jobs plus a page
    /// of terminal history. Active jobs are never paged out so a large batch
    /// stays fully visible.
    #[utoipa::path(get, path = "/", responses(
        (status = OK, body = inline(Response)),
    ), params(
        ListQuery,
        ("server" = uuid::Uuid, description = "The server ID"),
    ))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        Path(server): Path<uuid::Uuid>,
        Query(q): Query<ListQuery>,
    ) -> ApiResponseResult {
        permissions.has_server_permission("workshop.read")?;

        let page = q.page.unwrap_or(1).max(1);
        let per_page = q.per_page.unwrap_or(DEFAULT_PER_PAGE).clamp(1, MAX_PER_PAGE);
        let offset = (page - 1) * per_page;

        let active = crate::registry::active_downloads(state.database.read(), server).await?;
        let history =
            crate::registry::history_downloads(state.database.read(), server, per_page, offset)
                .await?;
        let history_total =
            crate::registry::count_history_downloads(state.database.read(), server).await?;

        ApiResponse::new_serialized(Response {
            jobs: active.clone(),
            active,
            history,
            history_total,
            page,
            per_page,
        })
        .ok()
    }
}

pub(crate) mod post {
    use axum::extract::Path;
    use serde::{Deserialize, Serialize};
    use shared::{
        GetState,
        models::user::{GetPermissionManager, GetUser},
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
    pub struct Response {
        pub job_id: uuid::Uuid,
        pub state: String,
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
        user: GetUser,
        Path(server): Path<uuid::Uuid>,
        shared::Payload(data): shared::Payload<Payload>,
    ) -> ApiResponseResult {
        permissions.has_server_permission("workshop.install")?;

        if data.app_id == 0 || data.workshop_id == 0 {
            return Err(ApiResponse::error(
                "app_id and workshop_id must be positive",
            ));
        }

        let resp = start_download_for_item(
            &state,
            &permissions,
            user.uuid,
            server,
            data.app_id,
            data.workshop_id,
            data.account.as_deref(),
            data.archive,
        )
        .await?;

        ApiResponse::new_serialized(resp).ok()
    }

    /// Everything needed to dispatch a download, resolved from settings + the
    /// requested account. Shared by the first download and the retry path so the
    /// account/install-rule resolution lives in exactly one place.
    struct ResolvedDispatch {
        install_rule: crate::helper::InstallRulePayload,
        account: Option<String>,
        post_install: &'static str,
        metadata: crate::registry::WorkshopMetadata,
        title_slug: Option<String>,
        helper_url: String,
        helper_token: String,
    }

    async fn resolve_dispatch(
        state: &GetState,
        permissions: &GetPermissionManager,
        user_uuid: uuid::Uuid,
        app_id: u32,
        workshop_id: u64,
        account_label: Option<&str>,
    ) -> Result<ResolvedDispatch, shared::response::ApiResponse> {
        let ext = {
            let settings = state.settings.get().await?;
            settings
                .find_extension_settings::<crate::settings::ExtensionSettingsData>()?
                .clone()
        };

        let preset = ext.game_presets.iter().find(|p| p.app_id == app_id);
        let install_rule = crate::helper::InstallRulePayload {
            matchers: preset.map(|p| p.r#match.clone()).unwrap_or_default(),
            generated_files: preset
                .map(|p| p.generated_files.clone())
                .unwrap_or_default(),
            extract_files: preset.map(|p| p.extract_files.clone()).unwrap_or_default(),
        };
        let post_install = match preset.map(|p| p.post_install) {
            Some(crate::settings::PostInstall::Extract) => "extract",
            _ => "none",
        };
        let requires_account = match preset.map(|p| p.auth).unwrap_or_default() {
            crate::settings::AuthRequirement::Account => true,
            crate::settings::AuthRequirement::Anonymous => false,
            crate::settings::AuthRequirement::Default => !ext.default_anonymous,
        };

        let account = match account_label.map(str::trim).filter(|a| !a.is_empty()) {
            Some(label) => {
                permissions.has_user_permission("calaworkshop.link-steam")?;
                crate::validation::validate_account_label(label)?;
                let link =
                    crate::steam_links::get_by_label(state.database.read(), user_uuid, label)
                        .await?
                        .ok_or_else(|| {
                            ApiResponse::error(
                                "you have not linked a Steam account with that label",
                            )
                        })?;
                Some(link.helper_label)
            }
            None if requires_account => {
                return Err(ApiResponse::error(
                    "this game requires a linked Steam account; select one before downloading",
                ));
            }
            None => None,
        };

        let metadata = get_metadata_cached(state, ext.steam_api_key.as_str(), workshop_id).await;
        let title_slug = metadata
            .title
            .as_deref()
            .map(slugify_title)
            .filter(|slug| !slug.is_empty());

        Ok(ResolvedDispatch {
            install_rule,
            account,
            post_install,
            metadata,
            title_slug,
            helper_url: ext.helper_url,
            helper_token: ext.helper_token,
        })
    }

    /// POST the resolved request to the helper and reconcile the given (already
    /// persisted) job row with the outcome.
    async fn dispatch_to_helper(
        state: &GetState,
        resolved: &ResolvedDispatch,
        job_id: uuid::Uuid,
        app_id: u32,
        workshop_id: u64,
        archive: bool,
    ) -> Result<Response, shared::response::ApiResponse> {
        let helper = crate::helper::HelperClient::new(
            &state.client,
            &resolved.helper_url,
            &resolved.helper_token,
        )
        .ok_or_else(|| ApiResponse::error("workshop helper is not configured"))?;

        let resp = match helper
            .start_download(&crate::helper::DownloadRequest {
                app_id,
                workshop_id,
                account: resolved.account.clone(),
                archive,
                title_slug: resolved.title_slug.clone(),
                install_rule: resolved.install_rule.clone(),
            })
            .await
        {
            Ok(resp) => resp,
            Err(err) => {
                crate::registry::update_download_helper(
                    state.database.write(),
                    job_id,
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
            job_id,
            Some(resp.id),
            &resp.state,
            None,
        )
        .await?;

        Ok(Response {
            job_id,
            state: resp.state,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn start_download_for_item(
        state: &GetState,
        permissions: &GetPermissionManager,
        user_uuid: uuid::Uuid,
        server_uuid: uuid::Uuid,
        app_id: u32,
        workshop_id: u64,
        account_label: Option<&str>,
        archive: bool,
    ) -> Result<Response, shared::response::ApiResponse> {
        let resolved =
            resolve_dispatch(state, permissions, user_uuid, app_id, workshop_id, account_label)
                .await?;

        let job = crate::registry::create_download(
            state.database.write(),
            server_uuid,
            app_id,
            workshop_id,
            resolved.metadata.clone(),
            resolved.post_install,
        )
        .await?;

        dispatch_to_helper(state, &resolved, job.id, app_id, workshop_id, archive).await
    }

    /// Re-dispatch an existing (failed) job row instead of creating a new one,
    /// resetting it back to `queued` first. Used by "retry all failed".
    #[allow(clippy::too_many_arguments)]
    pub async fn retry_existing_job(
        state: &GetState,
        permissions: &GetPermissionManager,
        user_uuid: uuid::Uuid,
        job_id: uuid::Uuid,
        app_id: u32,
        workshop_id: u64,
        account_label: Option<&str>,
        archive: bool,
    ) -> Result<Response, shared::response::ApiResponse> {
        let resolved =
            resolve_dispatch(state, permissions, user_uuid, app_id, workshop_id, account_label)
                .await?;

        // Clear the prior failure so it shows as active again immediately.
        crate::registry::update_download_helper(
            state.database.write(),
            job_id,
            None,
            "queued",
            None,
        )
        .await?;

        dispatch_to_helper(state, &resolved, job_id, app_id, workshop_id, archive).await
    }

    pub fn slugify_title(title: &str) -> String {
        let mut out = String::new();
        let mut last_was_sep = false;
        for ch in title.chars().flat_map(char::to_lowercase) {
            if ch.is_ascii_alphanumeric() {
                out.push(ch);
                last_was_sep = false;
            } else if !last_was_sep && !out.is_empty() {
                out.push('_');
                last_was_sep = true;
            }
            if out.len() >= 80 {
                break;
            }
        }
        out.trim_matches('_').to_string()
    }

    async fn get_metadata_cached(
        state: &GetState,
        api_key: &str,
        workshop_id: u64,
    ) -> crate::registry::WorkshopMetadata {
        let cache_key = workshop_id.to_string();
        if let Ok(Some(cached)) =
            crate::registry::get_cache_json(state.database.read(), "details", &cache_key).await
        {
            if let Ok(metadata) = serde_json::from_value(cached) {
                return metadata;
            }
        }

        let metadata =
            crate::steam::get_published_file_details(&state.client, api_key, workshop_id)
                .await
                .unwrap_or(crate::registry::WorkshopMetadata {
                    title: None,
                    preview_url: None,
                });
        if let Ok(value) = serde_json::to_value(&metadata) {
            let _ = crate::registry::put_cache_json(
                state.database.write(),
                "details",
                &cache_key,
                &value,
                1800,
            )
            .await;
        }
        metadata
    }
}

mod retry {
    use axum::extract::Path;
    use serde::{Deserialize, Serialize};
    use shared::{
        GetState,
        models::user::{GetPermissionManager, GetUser},
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Deserialize, Default)]
    pub struct Payload {
        /// Linked Steam account label to retry as, or null for anonymous. Applies
        /// to every retried item (the original per-item account isn't persisted).
        #[serde(default)]
        account: Option<String>,
    }

    #[derive(ToSchema, Serialize)]
    struct Response {
        /// How many failed jobs were successfully re-queued on the helper.
        retried: usize,
        /// How many could not be re-dispatched (left as failed).
        still_failed: usize,
    }

    /// Re-dispatch every failed download job for this server. Each job's existing
    /// row is reused (reset to `queued`), so retrying does not pile up duplicates.
    #[utoipa::path(post, path = "/retry", responses(
        (status = OK, body = inline(Response)),
    ), params(
        ("server" = uuid::Uuid, description = "The server ID"),
    ), request_body = inline(Payload))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        user: GetUser,
        Path(server): Path<uuid::Uuid>,
        shared::Payload(data): shared::Payload<Payload>,
    ) -> ApiResponseResult {
        permissions.has_server_permission("workshop.install")?;

        let failed = crate::registry::list_failed_downloads(state.database.read(), server).await?;
        let mut retried = 0usize;
        let mut still_failed = 0usize;
        for job in failed {
            match super::post::retry_existing_job(
                &state,
                &permissions,
                user.uuid,
                job.id,
                job.app_id as u32,
                job.workshop_id as u64,
                data.account.as_deref(),
                false,
            )
            .await
            {
                Ok(_) => retried += 1,
                Err(_) => still_failed += 1,
            }
        }

        ApiResponse::new_serialized(Response {
            retried,
            still_failed,
        })
        .ok()
    }
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .routes(routes!(get::route))
        .routes(routes!(post::route))
        .routes(routes!(retry::route))
        .nest("/{download}", _download_::router(state))
        .with_state(state.clone())
}
