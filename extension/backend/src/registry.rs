use serde::{Deserialize, Serialize};
use sqlx::Row;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DownloadJob {
    pub id: uuid::Uuid,
    pub server_uuid: uuid::Uuid,
    pub app_id: i32,
    pub workshop_id: i64,
    pub helper_job_id: Option<uuid::Uuid>,
    pub state: String,
    pub title: Option<String>,
    pub preview_url: Option<String>,
    pub install_path: Option<String>,
    pub file_name: Option<String>,
    pub files: Vec<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct InstalledItem {
    pub id: uuid::Uuid,
    pub server_uuid: uuid::Uuid,
    pub app_id: i32,
    pub workshop_id: Option<i64>,
    pub title: Option<String>,
    pub install_path: String,
    pub vpk_file: Option<String>,
    pub image_file: Option<String>,
    pub files: Vec<String>,
    pub source: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkshopMetadata {
    pub title: Option<String>,
    pub preview_url: Option<String>,
}

fn parse_files(raw: String) -> Vec<String> {
    serde_json::from_str(&raw).unwrap_or_default()
}

fn download_from_row(row: sqlx::postgres::PgRow) -> DownloadJob {
    DownloadJob {
        id: row.get("id"),
        server_uuid: row.get("server_uuid"),
        app_id: row.get("app_id"),
        workshop_id: row.get("workshop_id"),
        helper_job_id: row.get("helper_job_id"),
        state: row.get("state"),
        title: row.get("title"),
        preview_url: row.get("preview_url"),
        install_path: row.get("install_path"),
        file_name: row.get("file_name"),
        files: parse_files(row.get("files_json")),
        error: row.get("error"),
        created_at: row.get("created_at_str"),
        updated_at: row.get("updated_at_str"),
    }
}

fn installed_from_row(row: sqlx::postgres::PgRow) -> InstalledItem {
    InstalledItem {
        id: row.get("id"),
        server_uuid: row.get("server_uuid"),
        app_id: row.get("app_id"),
        workshop_id: row.get("workshop_id"),
        title: row.get("title"),
        install_path: row.get("install_path"),
        vpk_file: row.get("vpk_file"),
        image_file: row.get("image_file"),
        files: parse_files(row.get("files_json")),
        source: row.get("source"),
        created_at: row.get("created_at_str"),
        updated_at: row.get("updated_at_str"),
    }
}

pub async fn create_download(
    db: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    server_uuid: uuid::Uuid,
    app_id: u32,
    workshop_id: u64,
    metadata: WorkshopMetadata,
) -> Result<DownloadJob, sqlx::Error> {
    let row = sqlx::query(
        r#"
        INSERT INTO dev_wasian_calaworkshop_download_jobs
            (server_uuid, app_id, workshop_id, state, title, preview_url)
        VALUES ($1, $2, $3, 'queued', $4, $5)
        RETURNING *, files::text AS files_json, created_at::text AS created_at_str, updated_at::text AS updated_at_str
        "#,
    )
    .bind(server_uuid)
    .bind(app_id as i32)
    .bind(workshop_id as i64)
    .bind(metadata.title)
    .bind(metadata.preview_url)
    .fetch_one(db)
    .await?;
    Ok(download_from_row(row))
}

pub async fn recent_downloads(
    db: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    server_uuid: uuid::Uuid,
) -> Result<Vec<DownloadJob>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT *, files::text AS files_json, created_at::text AS created_at_str, updated_at::text AS updated_at_str
        FROM dev_wasian_calaworkshop_download_jobs
        WHERE server_uuid = $1
        ORDER BY
            CASE WHEN state IN ('queued', 'downloading', 'ready') THEN 0 ELSE 1 END,
            updated_at DESC
        LIMIT 50
        "#,
    )
    .bind(server_uuid)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(download_from_row).collect())
}

pub async fn get_download(
    db: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    server_uuid: uuid::Uuid,
    id: uuid::Uuid,
) -> Result<Option<DownloadJob>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        SELECT *, files::text AS files_json, created_at::text AS created_at_str, updated_at::text AS updated_at_str
        FROM dev_wasian_calaworkshop_download_jobs
        WHERE server_uuid = $1 AND id = $2
        "#,
    )
    .bind(server_uuid)
    .bind(id)
    .fetch_optional(db)
    .await?;
    Ok(row.map(download_from_row))
}

pub async fn delete_download(
    db: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    server_uuid: uuid::Uuid,
    id: uuid::Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "DELETE FROM dev_wasian_calaworkshop_download_jobs WHERE server_uuid = $1 AND id = $2",
    )
    .bind(server_uuid)
    .bind(id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn update_download_helper(
    db: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: uuid::Uuid,
    helper_job_id: Option<uuid::Uuid>,
    state: &str,
    error: Option<String>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE dev_wasian_calaworkshop_download_jobs
        SET helper_job_id = $2, state = $3, error = $4, updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(helper_job_id)
    .bind(state)
    .bind(error)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn update_download_from_helper(
    db: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: uuid::Uuid,
    state: &str,
    file_name: Option<String>,
    files: Vec<String>,
    error: Option<String>,
) -> Result<(), sqlx::Error> {
    let files = serde_json::to_string(&files).unwrap_or_else(|_| "[]".to_string());
    sqlx::query(
        r#"
        UPDATE dev_wasian_calaworkshop_download_jobs
        SET state = $2, file_name = $3, files = $4::jsonb, error = $5, updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(state)
    .bind(file_name)
    .bind(files)
    .bind(error)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn mark_download_installed(
    db: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: uuid::Uuid,
    install_path: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE dev_wasian_calaworkshop_download_jobs SET state = 'installed', install_path = $2, updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(install_path)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn create_installed(
    db: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    server_uuid: uuid::Uuid,
    app_id: u32,
    workshop_id: Option<u64>,
    title: Option<String>,
    install_path: &str,
    files: Vec<String>,
    source: &str,
) -> Result<InstalledItem, sqlx::Error> {
    let vpk = files.iter().find(|f| ext_is(f, "vpk")).cloned();
    let image = files
        .iter()
        .find(|f| ext_is(f, "jpg") || ext_is(f, "jpeg") || ext_is(f, "png"))
        .cloned();
    let files_json = serde_json::to_string(&files).unwrap_or_else(|_| "[]".to_string());
    let row = sqlx::query(
        r#"
        INSERT INTO dev_wasian_calaworkshop_installed_items
            (server_uuid, app_id, workshop_id, title, install_path, vpk_file, image_file, files, source)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8::jsonb, $9)
        RETURNING *, files::text AS files_json, created_at::text AS created_at_str, updated_at::text AS updated_at_str
        "#,
    )
    .bind(server_uuid)
    .bind(app_id as i32)
    .bind(workshop_id.map(|id| id as i64))
    .bind(title)
    .bind(install_path)
    .bind(vpk)
    .bind(image)
    .bind(files_json)
    .bind(source)
    .fetch_one(db)
    .await?;
    Ok(installed_from_row(row))
}

pub async fn list_installed(
    db: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    server_uuid: uuid::Uuid,
) -> Result<Vec<InstalledItem>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT *, files::text AS files_json, created_at::text AS created_at_str, updated_at::text AS updated_at_str
        FROM dev_wasian_calaworkshop_installed_items
        WHERE server_uuid = $1
        ORDER BY updated_at DESC
        "#,
    )
    .bind(server_uuid)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(installed_from_row).collect())
}

pub async fn get_installed(
    db: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    server_uuid: uuid::Uuid,
    id: uuid::Uuid,
) -> Result<Option<InstalledItem>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        SELECT *, files::text AS files_json, created_at::text AS created_at_str, updated_at::text AS updated_at_str
        FROM dev_wasian_calaworkshop_installed_items
        WHERE server_uuid = $1 AND id = $2
        "#,
    )
    .bind(server_uuid)
    .bind(id)
    .fetch_optional(db)
    .await?;
    Ok(row.map(installed_from_row))
}

pub async fn delete_installed(
    db: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    server_uuid: uuid::Uuid,
    id: uuid::Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "DELETE FROM dev_wasian_calaworkshop_installed_items WHERE server_uuid = $1 AND id = $2",
    )
    .bind(server_uuid)
    .bind(id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn update_installed_title(
    db: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: uuid::Uuid,
    title: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE dev_wasian_calaworkshop_installed_items SET title = $2, updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(title)
    .execute(db)
    .await?;
    Ok(())
}

fn ext_is(file: &str, ext: &str) -> bool {
    file.rsplit('.')
        .next()
        .map(|actual| actual.eq_ignore_ascii_case(ext))
        .unwrap_or(false)
}
