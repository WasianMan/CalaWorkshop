use serde::{Deserialize, Deserializer, Serialize};

use crate::registry::WorkshopMetadata;

const DETAILS_URL: &str =
    "https://api.steampowered.com/ISteamRemoteStorage/GetPublishedFileDetails/v1/";
const COLLECTION_URL: &str =
    "https://api.steampowered.com/ISteamRemoteStorage/GetCollectionDetails/v1/";
const QUERY_FILES_URL: &str = "https://api.steampowered.com/IPublishedFileService/QueryFiles/v1/";
const STEAM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkshopSearchResult {
    pub published_file_id: u64,
    pub title: String,
    pub preview_url: Option<String>,
    pub short_description: Option<String>,
    pub file_size: Option<u64>,
    pub subscriptions: Option<u64>,
    pub time_created: Option<u64>,
    pub time_updated: Option<u64>,
    pub vote_score: Option<f64>,
    pub vote_count: Option<u64>,
    pub stars: Option<f64>,
    pub file_type: Option<u32>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub items: Vec<WorkshopSearchResult>,
    pub next_cursor: Option<String>,
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
pub enum SearchSort {
    Relevance,
    Popular,
    Trending,
    Newest,
    Updated,
    Subscribed,
}

impl SearchSort {
    pub fn from_param(value: Option<&str>, has_query: bool) -> Self {
        match value.unwrap_or("").to_ascii_lowercase().as_str() {
            "popular" => Self::Popular,
            "trending" => Self::Trending,
            "newest" => Self::Newest,
            "updated" => Self::Updated,
            "subscribed" => Self::Subscribed,
            "relevance" => Self::Relevance,
            _ if has_query => Self::Relevance,
            _ => Self::Popular,
        }
    }

    fn query_type(self) -> u32 {
        match self {
            Self::Popular => 0,    // RankedByVote
            Self::Newest => 1,     // RankedByPublicationDate
            Self::Trending => 3,   // RankedByTrend
            Self::Subscribed => 9, // RankedByTotalUniqueSubscriptions
            Self::Relevance => 12, // RankedByTextSearch
            Self::Updated => 21,   // RankedByLastUpdatedDate
        }
    }

    pub fn cache_key(self) -> &'static str {
        match self {
            Self::Relevance => "relevance",
            Self::Popular => "popular",
            Self::Trending => "trending",
            Self::Newest => "newest",
            Self::Updated => "updated",
            Self::Subscribed => "subscribed",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MatchingFileType {
    Items,
    Collections,
}

impl MatchingFileType {
    pub fn from_param(value: Option<&str>) -> Self {
        match value.unwrap_or("").to_ascii_lowercase().as_str() {
            "collection" | "collections" => Self::Collections,
            _ => Self::Items,
        }
    }

    fn value(self) -> u32 {
        match self {
            Self::Items => 0,
            Self::Collections => 1,
        }
    }

    pub fn cache_key(self) -> &'static str {
        match self {
            Self::Items => "items",
            Self::Collections => "collections",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionPreview {
    pub collection: Option<WorkshopSearchResult>,
    pub children: Vec<WorkshopSearchResult>,
    pub skipped: Vec<CollectionSkippedItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSkippedItem {
    pub published_file_id: u64,
    pub reason: String,
}

#[derive(Deserialize)]
struct DetailsEnvelope {
    response: DetailsResponse,
}

#[derive(Deserialize)]
struct DetailsResponse {
    #[serde(default)]
    publishedfiledetails: Vec<PublishedFileDetails>,
}

#[derive(Debug, Clone, Deserialize)]
struct PublishedFileDetails {
    #[serde(default, deserialize_with = "de_u64_any_default")]
    publishedfileid: u64,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    preview_url: Option<String>,
    #[serde(default, alias = "file_description", alias = "description")]
    short_description: Option<String>,
    #[serde(default, deserialize_with = "de_opt_u64_any")]
    file_size: Option<u64>,
    #[serde(default, deserialize_with = "de_opt_u64_any")]
    subscriptions: Option<u64>,
    #[serde(default, deserialize_with = "de_opt_u64_any")]
    time_created: Option<u64>,
    #[serde(default, deserialize_with = "de_opt_u64_any")]
    time_updated: Option<u64>,
    #[serde(default)]
    file_type: Option<u32>,
    #[serde(default)]
    tags: Vec<SteamTag>,
    #[serde(default)]
    vote_data: Option<VoteData>,
}

#[derive(Debug, Clone, Deserialize)]
struct SteamTag {
    tag: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct VoteData {
    #[serde(default, deserialize_with = "de_opt_u64_any")]
    votes_up: Option<u64>,
    #[serde(default, deserialize_with = "de_opt_u64_any")]
    votes_down: Option<u64>,
    #[serde(default)]
    score: Option<f64>,
}

#[derive(Deserialize)]
struct QueryEnvelope {
    response: QueryResponse,
}

#[derive(Deserialize)]
struct QueryResponse {
    #[serde(default)]
    publishedfiledetails: Vec<PublishedFileDetails>,
    #[serde(default, deserialize_with = "de_opt_u64_any")]
    total: Option<u64>,
    #[serde(default)]
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
struct CollectionEnvelope {
    response: CollectionResponse,
}

#[derive(Deserialize)]
struct CollectionResponse {
    #[serde(default)]
    collectiondetails: Vec<CollectionDetails>,
}

#[derive(Deserialize)]
struct CollectionDetails {
    #[serde(default, deserialize_with = "de_u64_any_default")]
    publishedfileid: u64,
    #[serde(default)]
    children: Vec<CollectionChild>,
}

#[derive(Deserialize)]
struct CollectionChild {
    #[serde(default, deserialize_with = "de_u64_any_default")]
    publishedfileid: u64,
}

pub async fn get_published_file_details(
    client: &reqwest::Client,
    api_key: &str,
    workshop_id: u64,
) -> Option<WorkshopMetadata> {
    let details = get_published_file_details_many(client, api_key, &[workshop_id])
        .await
        .ok()?;
    let item = details.into_iter().next()?;

    Some(WorkshopMetadata {
        title: nonempty(item.title),
        preview_url: item.preview_url.and_then(nonempty),
    })
}

pub async fn get_published_file_details_many(
    client: &reqwest::Client,
    api_key: &str,
    workshop_ids: &[u64],
) -> Result<Vec<WorkshopSearchResult>, anyhow::Error> {
    if workshop_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut form = vec![("itemcount".to_string(), workshop_ids.len().to_string())];
    for (idx, id) in workshop_ids.iter().enumerate() {
        form.push((format!("publishedfileids[{idx}]"), id.to_string()));
    }
    if !api_key.trim().is_empty() {
        form.push(("key".to_string(), api_key.to_string()));
    }
    form.push(("format".to_string(), "json".to_string()));

    let response = client
        .post(DETAILS_URL)
        .form(&form)
        .timeout(STEAM_TIMEOUT)
        .send()
        .await?
        .error_for_status()?
        .json::<DetailsEnvelope>()
        .await?
        .response
        .publishedfiledetails
        .into_iter()
        .filter_map(to_search_result)
        .collect();

    Ok(response)
}

pub async fn query_files(
    client: &reqwest::Client,
    api_key: &str,
    app_id: u32,
    query: Option<&str>,
    sort: SearchSort,
    cursor: Option<&str>,
    file_type: MatchingFileType,
    tags: &[String],
) -> Result<SearchResponse, anyhow::Error> {
    let mut input = serde_json::json!({
        "query_type": sort.query_type(),
        "cursor": cursor.map(str::trim).filter(|cursor| !cursor.is_empty()).unwrap_or("*"),
        "creator_appid": app_id,
        "appid": app_id,
        "numperpage": 24,
        "filetype": file_type.value(),
        "return_vote_data": true,
        "return_tags": true,
        "return_previews": true,
        "return_children": true,
        "return_short_description": true,
    });
    if let Some(q) = query.map(str::trim).filter(|q| !q.is_empty()) {
        input["search_text"] = serde_json::Value::String(q.to_string());
    }
    if !tags.is_empty() {
        input["requiredtags"] = serde_json::Value::String(tags.join(","));
        input["match_all_tags"] = serde_json::Value::Bool(false);
    }
    if matches!(sort, SearchSort::Trending) {
        input["days"] = serde_json::Value::Number(7.into());
        input["include_recent_votes_only"] = serde_json::Value::Bool(false);
    }

    let input_json = input.to_string();
    let response = client
        .get(QUERY_FILES_URL)
        .query(&[
            ("key", api_key),
            ("format", "json"),
            ("input_json", input_json.as_str()),
        ])
        .timeout(STEAM_TIMEOUT)
        .send()
        .await?
        .error_for_status()?
        .json::<QueryEnvelope>()
        .await?
        .response;

    Ok(SearchResponse {
        items: response
            .publishedfiledetails
            .into_iter()
            .filter_map(to_search_result)
            .collect(),
        next_cursor: response.next_cursor.filter(|s| !s.trim().is_empty()),
        total: response.total,
    })
}

pub async fn get_collection_preview(
    client: &reqwest::Client,
    api_key: &str,
    collection_id: u64,
) -> Result<CollectionPreview, anyhow::Error> {
    let mut form = vec![
        ("collectioncount".to_string(), "1".to_string()),
        ("publishedfileids[0]".to_string(), collection_id.to_string()),
        ("format".to_string(), "json".to_string()),
    ];
    if !api_key.trim().is_empty() {
        form.push(("key".to_string(), api_key.to_string()));
    }

    let collection = client
        .post(COLLECTION_URL)
        .form(&form)
        .timeout(STEAM_TIMEOUT)
        .send()
        .await?
        .error_for_status()?
        .json::<CollectionEnvelope>()
        .await?
        .response
        .collectiondetails
        .into_iter()
        .find(|details| details.publishedfileid == collection_id)
        .ok_or_else(|| anyhow::anyhow!("Steam did not return that collection"))?;

    let mut child_ids = Vec::new();
    for child in collection.children {
        if child.publishedfileid != 0 && !child_ids.contains(&child.publishedfileid) {
            child_ids.push(child.publishedfileid);
        }
    }

    let mut detail_ids = vec![collection_id];
    detail_ids.extend(child_ids.iter().copied());
    let details = get_published_file_details_many(client, api_key, &detail_ids).await?;
    let collection_item = details
        .iter()
        .find(|item| item.published_file_id == collection_id)
        .cloned();

    let mut children = Vec::new();
    let mut skipped = Vec::new();
    for child_id in child_ids {
        if let Some(item) = details
            .iter()
            .find(|item| item.published_file_id == child_id)
            .cloned()
        {
            children.push(item);
        } else {
            skipped.push(CollectionSkippedItem {
                published_file_id: child_id,
                reason: "Steam did not return item metadata".to_string(),
            });
        }
    }

    Ok(CollectionPreview {
        collection: collection_item,
        children,
        skipped,
    })
}

fn to_search_result(details: PublishedFileDetails) -> Option<WorkshopSearchResult> {
    if details.publishedfileid == 0 {
        return None;
    }
    let title = details
        .title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Workshop item")
        .to_string();
    let (vote_score, vote_count, stars) = vote_summary(details.vote_data.as_ref());

    Some(WorkshopSearchResult {
        published_file_id: details.publishedfileid,
        title,
        preview_url: details.preview_url.and_then(nonempty),
        short_description: details.short_description.and_then(nonempty),
        file_size: details.file_size,
        subscriptions: details.subscriptions,
        time_created: details.time_created,
        time_updated: details.time_updated,
        vote_score,
        vote_count,
        stars,
        file_type: details.file_type,
        tags: details
            .tags
            .into_iter()
            .filter_map(|tag| tag.tag.and_then(nonempty))
            .collect(),
    })
}

fn vote_summary(vote_data: Option<&VoteData>) -> (Option<f64>, Option<u64>, Option<f64>) {
    let Some(vote_data) = vote_data else {
        return (None, None, None);
    };
    let up = vote_data.votes_up.unwrap_or(0);
    let down = vote_data.votes_down.unwrap_or(0);
    let count = up + down;
    let score = vote_data
        .score
        .or_else(|| (count > 0).then_some((up as f64 / count as f64).clamp(0.0, 1.0)));
    let stars = score.map(|score| (score * 5.0 * 2.0).round() / 2.0);
    (score, Some(count), (count > 0).then_some(stars).flatten())
}

fn nonempty(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn de_u64_any_default<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(de_opt_u64_any(deserializer)?.unwrap_or_default())
}

fn de_opt_u64_any<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(match value {
        Some(serde_json::Value::Number(n)) => n.as_u64(),
        Some(serde_json::Value::String(s)) => s.trim().parse().ok(),
        _ => None,
    })
}
