//! Per-user Steam account ownership.
//!
//! Each row ties a Calagopus `user_uuid` + user-facing `label` to an opaque
//! `helper_label`. The helper keys its cached SteamCMD session by the opaque
//! label, so the friendly label a user types is never used to address a session
//! directly — that is what keeps one user from listing, using, or deleting
//! another user's linked account, even between two admins.

use sqlx::Row;

#[derive(Debug, Clone)]
pub struct SteamLink {
    pub label: String,
    /// Opaque, per-link identifier the helper uses as its session directory.
    pub helper_label: String,
    pub steam_username: Option<String>,
}

fn from_row(row: sqlx::postgres::PgRow) -> SteamLink {
    SteamLink {
        label: row.get("label"),
        helper_label: row.get("helper_label"),
        steam_username: row.get("steam_username"),
    }
}

pub async fn list_by_user(
    db: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    user_uuid: uuid::Uuid,
) -> Result<Vec<SteamLink>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT label, helper_label, steam_username
        FROM dev_wasian_calaworkshop_steam_links
        WHERE user_uuid = $1 AND helper_label IS NOT NULL
        ORDER BY label
        "#,
    )
    .bind(user_uuid)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(from_row).collect())
}

pub async fn get_by_label(
    db: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    user_uuid: uuid::Uuid,
    label: &str,
) -> Result<Option<SteamLink>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        SELECT label, helper_label, steam_username
        FROM dev_wasian_calaworkshop_steam_links
        WHERE user_uuid = $1 AND label = $2 AND helper_label IS NOT NULL
        "#,
    )
    .bind(user_uuid)
    .bind(label)
    .fetch_optional(db)
    .await?;
    Ok(row.map(from_row))
}

/// Reserve (or reuse) the opaque helper label for `(user, label)` and record the
/// Steam username. A new link gets a fresh random `helper_label`; re-linking an
/// existing label keeps the original one so the helper's cached session and any
/// pending Steam Guard retry stay addressable.
pub async fn upsert(
    db: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    user_uuid: uuid::Uuid,
    label: &str,
    steam_username: Option<&str>,
) -> Result<SteamLink, sqlx::Error> {
    let helper_label = uuid::Uuid::new_v4().simple().to_string();
    let row = sqlx::query(
        r#"
        INSERT INTO dev_wasian_calaworkshop_steam_links
            (user_uuid, label, helper_label, steam_username, created_at, updated_at)
        VALUES ($1, $2, $3, $4, now(), now())
        ON CONFLICT (user_uuid, label)
        DO UPDATE SET steam_username = EXCLUDED.steam_username, updated_at = now()
        RETURNING label, helper_label, steam_username
        "#,
    )
    .bind(user_uuid)
    .bind(label)
    .bind(&helper_label)
    .bind(steam_username)
    .fetch_one(db)
    .await?;
    Ok(from_row(row))
}

/// Delete a user's link, returning the opaque helper label so the caller can
/// clean up the helper's cached session. `None` when the user owns no such label.
pub async fn delete(
    db: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    user_uuid: uuid::Uuid,
    label: &str,
) -> Result<Option<String>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        DELETE FROM dev_wasian_calaworkshop_steam_links
        WHERE user_uuid = $1 AND label = $2
        RETURNING helper_label
        "#,
    )
    .bind(user_uuid)
    .bind(label)
    .fetch_optional(db)
    .await?;
    Ok(row.and_then(|r| r.get::<Option<String>, _>("helper_label")))
}
