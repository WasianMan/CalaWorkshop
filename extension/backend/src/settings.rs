//! Global, panel-wide settings for the Calagopus Workshop extension.
//!
//! Settings in Calagopus are global per-extension (not per-user). Secrets
//! (`helper_token`, `steam_api_key`) are encrypted at rest via
//! `database.encrypt` / `database.decrypt`; the game-preset list is stored as a
//! single serde blob. Per-user data (e.g. which Steam account a user linked)
//! lives in the helper and/or the extension migration tables, not here.

use base32::Alphabet;
use serde::{Deserialize, Serialize};
use shared::extensions::settings::{
    ExtensionSettings, SettingsDeserializeExt, SettingsDeserializer, SettingsSerializeExt,
    SettingsSerializer,
};
use utoipa::ToSchema;

/// A game preset: the Steam app id plus the default place workshop content is
/// installed for that game. Seeded with Left 4 Dead 2; admins can add more.
#[derive(ToSchema, Serialize, Deserialize, Clone)]
pub struct GamePreset {
    /// Steam application id (e.g. 550 for Left 4 Dead 2).
    pub app_id: u32,
    /// Human-readable name shown in the picker.
    pub name: String,
    /// Default install path inside the server volume, relative to its root.
    pub install_path: String,
}

impl GamePreset {
    fn defaults() -> Vec<GamePreset> {
        vec![GamePreset {
            app_id: 550,
            name: "Left 4 Dead 2".to_string(),
            install_path: "left4dead2/addons/workshop".to_string(),
        }]
    }
}

#[derive(ToSchema, Serialize, Deserialize, Clone)]
pub struct ExtensionSettingsData {
    /// Base URL of the workshop helper, reachable by the panel (and by Wings for
    /// file pulls). On AIO this is the compose service name.
    pub helper_url: String,
    /// Shared bearer token for talking to the helper. Secret, encrypted at rest.
    pub helper_token: compact_str::CompactString,
    /// Steam Web API key used for search/metadata (never for downloads). Secret.
    pub steam_api_key: compact_str::CompactString,
    /// Whether downloads default to anonymous SteamCMD login.
    pub default_anonymous: bool,
    /// Configured game presets.
    pub game_presets: Vec<GamePreset>,
}

impl Default for ExtensionSettingsData {
    fn default() -> Self {
        Self {
            helper_url: "http://calagopus-workshop-helper:8090".to_string(),
            helper_token: "".into(),
            steam_api_key: "".into(),
            default_anonymous: true,
            game_presets: GamePreset::defaults(),
        }
    }
}

#[async_trait::async_trait]
impl SettingsSerializeExt for ExtensionSettingsData {
    async fn serialize(
        &self,
        serializer: SettingsSerializer,
    ) -> Result<SettingsSerializer, anyhow::Error> {
        let database = serializer.database.clone();

        Ok(serializer
            .write_raw_setting("helper_url", self.helper_url.clone())
            .write_raw_setting(
                "helper_token",
                base32::encode(
                    Alphabet::Z,
                    database
                        .encrypt(self.helper_token.clone())
                        .await?
                        .as_slice(),
                ),
            )
            .write_raw_setting(
                "steam_api_key",
                base32::encode(
                    Alphabet::Z,
                    database
                        .encrypt(self.steam_api_key.clone())
                        .await?
                        .as_slice(),
                ),
            )
            .write_raw_setting("default_anonymous", self.default_anonymous.to_string())
            .write_serde_setting("game_presets", &self.game_presets)?)
    }
}

pub struct ExtensionSettingsDataDeserializer;

#[async_trait::async_trait]
impl SettingsDeserializeExt for ExtensionSettingsDataDeserializer {
    async fn deserialize_boxed(
        &self,
        mut deserializer: SettingsDeserializer<'_>,
    ) -> Result<ExtensionSettings, anyhow::Error> {
        let defaults = ExtensionSettingsData::default();

        let helper_url = deserializer
            .take_raw_setting("helper_url")
            .map(|s| s.to_string())
            .unwrap_or(defaults.helper_url);

        let helper_token = match deserializer.take_raw_setting("helper_token") {
            Some(encoded) if !encoded.is_empty() => {
                let decoded = base32::decode(Alphabet::Z, &encoded)
                    .ok_or_else(|| anyhow::anyhow!("failed to decode helper_token from base32"))?;
                deserializer.database.decrypt(decoded).await?
            }
            _ => "".into(),
        };

        let steam_api_key = match deserializer.take_raw_setting("steam_api_key") {
            Some(encoded) if !encoded.is_empty() => {
                let decoded = base32::decode(Alphabet::Z, &encoded)
                    .ok_or_else(|| anyhow::anyhow!("failed to decode steam_api_key from base32"))?;
                deserializer.database.decrypt(decoded).await?
            }
            _ => "".into(),
        };

        let default_anonymous = deserializer
            .take_raw_setting("default_anonymous")
            .and_then(|s| s.parse().ok())
            .unwrap_or(defaults.default_anonymous);

        let game_presets = deserializer
            .read_serde_setting("game_presets")
            .unwrap_or_else(|_| GamePreset::defaults());

        Ok(Box::new(ExtensionSettingsData {
            helper_url,
            helper_token,
            steam_api_key,
            default_anonymous,
            game_presets,
        }))
    }
}
