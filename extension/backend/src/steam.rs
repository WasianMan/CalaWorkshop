use serde::Deserialize;

use crate::registry::WorkshopMetadata;

#[derive(Deserialize)]
struct DetailsEnvelope {
    response: DetailsResponse,
}

#[derive(Deserialize)]
struct DetailsResponse {
    #[serde(default)]
    publishedfiledetails: Vec<PublishedFileDetails>,
}

#[derive(Deserialize)]
struct PublishedFileDetails {
    title: Option<String>,
    preview_url: Option<String>,
}

pub async fn get_published_file_details(
    client: &reqwest::Client,
    api_key: &str,
    workshop_id: u64,
) -> Option<WorkshopMetadata> {
    if api_key.trim().is_empty() {
        return None;
    }

    let mut form = vec![
        ("itemcount".to_string(), "1".to_string()),
        ("publishedfileids[0]".to_string(), workshop_id.to_string()),
        ("key".to_string(), api_key.to_string()),
    ];
    form.push(("format".to_string(), "json".to_string()));

    let details = client
        .post("https://api.steampowered.com/ISteamRemoteStorage/GetPublishedFileDetails/v1/")
        .form(&form)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json::<DetailsEnvelope>()
        .await
        .ok()?
        .response
        .publishedfiledetails
        .into_iter()
        .next()?;

    Some(WorkshopMetadata {
        title: details.title.filter(|s| !s.trim().is_empty()),
        preview_url: details.preview_url.filter(|s| !s.trim().is_empty()),
    })
}
