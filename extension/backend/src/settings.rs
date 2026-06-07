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

/// Whether downloads for a game should authenticate.
///
/// `Default` defers to the global `default_anonymous` switch; `Anonymous` and
/// `Account` are explicit per-game overrides surfaced to the Workshop tab so it
/// can nudge the user toward (or away from) a linked Steam account.
#[derive(ToSchema, Serialize, Deserialize, Clone, Copy, Default, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum AuthRequirement {
    /// Use the global `default_anonymous` setting.
    #[default]
    Default,
    /// This game can be downloaded anonymously.
    Anonymous,
    /// This game requires a linked Steam account that owns it (e.g. L4D2).
    Account,
}

/// What to do after the helper artifact has been pulled into the volume.
///
/// `None` just places the files; `Extract` additionally decompresses any archive
/// among the installed files in place (for games that ship a mod as a nested
/// archive). Symlinking is intentionally unsupported — placement goes through
/// Wings and the helper never touches a volume.
#[derive(ToSchema, Serialize, Deserialize, Clone, Copy, Default, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum PostInstall {
    #[default]
    None,
    Extract,
}

/// A single file-selection rule. `glob` may contain `|`-separated alternatives
/// (e.g. `*.vpk|*.bin`); brace alternation (`*.{jpg,jpeg,png}`) is also
/// supported. The optional `rename` template maps each matched file to its
/// install destination; tokens: `{workshop_id}`, `{app_id}`, `{ext}` (lowercased
/// original extension), `{basename}` (original stem), `{title_slug}`. It may
/// include `/` subdirectories and is validated as a safe relative path by the
/// helper.
#[derive(ToSchema, Serialize, Deserialize, Clone, Debug)]
pub struct MatchRule {
    pub glob: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rename: Option<String>,
}

/// A file synthesized by the helper and included in the install transfer.
///
/// This is intentionally admin-only configuration: generated files can include
/// game scripts (e.g. Garry's Mod server Lua).
#[derive(ToSchema, Serialize, Deserialize, Clone, Debug)]
pub struct GeneratedFileRule {
    pub path: String,
    pub content: String,
}

/// A directory scan used to discover unmanaged Workshop content for a preset.
#[derive(ToSchema, Serialize, Deserialize, Clone, Debug)]
pub struct ScanRule {
    pub path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glob: Option<String>,
}

/// A game preset: the Steam app id, the default install path, and an optional
/// data-driven install rule. Seeded with Left 4 Dead 2; admins can add more
/// games (and richer rules) entirely through settings — no code change needed.
#[derive(ToSchema, Serialize, Deserialize, Clone)]
pub struct GamePreset {
    /// Steam application id (e.g. 550 for Left 4 Dead 2).
    pub app_id: u32,
    /// Human-readable name shown in the picker.
    pub name: String,
    /// Default install path inside the server volume, relative to its root.
    pub install_path: String,
    /// Per-game auth requirement (advisory; defers to `default_anonymous`).
    #[serde(default)]
    pub auth: AuthRequirement,
    /// File-selection/rename rules. Empty = mirror every downloaded file as-is.
    #[serde(default, rename = "match")]
    pub r#match: Vec<MatchRule>,
    /// Generated files to bundle into the install artifact.
    #[serde(default)]
    pub generated_files: Vec<GeneratedFileRule>,
    /// Scan rules for unmanaged content discovery.
    #[serde(default)]
    pub scan: Vec<ScanRule>,
    /// Post-install behavior once files land in the volume.
    #[serde(default)]
    pub post_install: PostInstall,
}

impl GamePreset {
    fn defaults() -> Vec<GamePreset> {
        vec![GamePreset::l4d2_default(), GamePreset::gmod_default()]
    }

    fn l4d2_default() -> GamePreset {
        // L4D2's special-casing now lives here as data instead of in helper code:
        // SteamCMD delivers app-550 items as `<handle>_legacy.bin` (the raw VPK)
        // and the dedicated server only loads addons named `<workshop_id>.vpk`.
        GamePreset {
            app_id: 550,
            name: "Left 4 Dead 2".to_string(),
            install_path: "left4dead2/addons".to_string(),
            auth: AuthRequirement::Account,
            r#match: vec![
                MatchRule {
                    glob: "*.vpk|*_legacy.bin".to_string(),
                    rename: Some("{workshop_id}.vpk".to_string()),
                },
                MatchRule {
                    glob: "*_legacy.{jpg,jpeg,png}".to_string(),
                    rename: Some("{workshop_id}.{ext}".to_string()),
                },
            ],
            generated_files: Vec::new(),
            scan: vec![
                ScanRule {
                    path: "left4dead2/addons".to_string(),
                    extensions: vec![
                        "vpk".to_string(),
                        "jpg".to_string(),
                        "jpeg".to_string(),
                        "png".to_string(),
                    ],
                    glob: None,
                },
                ScanRule {
                    path: "left4dead2/addons/workshop".to_string(),
                    extensions: vec![
                        "vpk".to_string(),
                        "jpg".to_string(),
                        "jpeg".to_string(),
                        "png".to_string(),
                    ],
                    glob: None,
                },
            ],
            post_install: PostInstall::None,
        }
    }

    fn gmod_default() -> GamePreset {
        GamePreset {
            app_id: 4000,
            name: "Garry's Mod".to_string(),
            install_path: "garrysmod".to_string(),
            auth: AuthRequirement::Anonymous,
            r#match: vec![MatchRule {
                glob: "*.gma|*_legacy.bin".to_string(),
                rename: Some("addons/{title_slug}_{workshop_id}.gma".to_string()),
            }],
            generated_files: vec![GeneratedFileRule {
                path: "lua/autorun/server/cala_workshop_{workshop_id}.lua".to_string(),
                content: "if SERVER then resource.AddWorkshop(\"{workshop_id}\") end\n".to_string(),
            }],
            scan: vec![ScanRule {
                path: "garrysmod/addons".to_string(),
                extensions: vec!["gma".to_string()],
                glob: None,
            }],
            post_install: PostInstall::None,
        }
    }

    fn hydrate_legacy_defaults(&mut self) {
        let default = match self.app_id {
            550 => GamePreset::l4d2_default(),
            _ => return,
        };

        if self.auth == AuthRequirement::Default {
            self.auth = default.auth;
        }
        if self.r#match.is_empty() {
            self.r#match = default.r#match;
        }
        if self.post_install == PostInstall::None {
            self.post_install = default.post_install;
        }
        if self.scan.is_empty() {
            self.scan = default.scan;
        }
    }

    fn seed_missing_defaults(presets: &mut Vec<GamePreset>) {
        for default in GamePreset::defaults() {
            if !presets.iter().any(|preset| preset.app_id == default.app_id) {
                presets.push(default);
            }
        }
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

        let mut game_presets = deserializer
            .read_serde_setting("game_presets")
            .unwrap_or_else(|_| GamePreset::defaults());
        for preset in &mut game_presets {
            preset.hydrate_legacy_defaults();
        }
        GamePreset::seed_missing_defaults(&mut game_presets);

        Ok(Box::new(ExtensionSettingsData {
            helper_url,
            helper_token,
            steam_api_key,
            default_anonymous,
            game_presets,
        }))
    }
}
