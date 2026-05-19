use crate::chat::{ChatConfig, SharedDiscordConfig, TelescopeChatOverrides};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Top-level configuration. Holds the shared chat infrastructure (one Matrix
/// login process-wide, default Discord webhook) and the list of telescopes
/// being monitored.
#[derive(Debug, Clone, Serialize)]
pub struct Config {
    pub logging: LoggingConfig,
    #[serde(default)]
    pub chat: ChatConfig,
    pub telescopes: Vec<TelescopeConfig>,
}

/// Per-telescope configuration: each telescope has its own NINA API endpoint
/// and chat destination overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelescopeConfig {
    /// Human-readable identifier (e.g. "c925", "esprit"). Used as a prefix in
    /// chat notifications and as the --telescope selector for CLI commands.
    pub name: String,
    pub api: ApiConfig,
    /// Overrides for which Discord webhook / Matrix room this telescope posts
    /// to. Each `None` field falls back to the shared default in `Config.chat`.
    #[serde(default)]
    pub chat: TelescopeChatOverrides,
    #[serde(default = "default_image_cooldown_seconds")]
    pub image_cooldown_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    pub base_url: String,
    pub timeout_seconds: u64,
    pub retry_attempts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub enable_file_logging: bool,
    pub log_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordConfig {
    pub webhook_url: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_image_cooldown_seconds")]
    pub image_cooldown_seconds: u64,
}

fn default_enabled() -> bool {
    true
}

fn default_image_cooldown_seconds() -> u64 {
    60
}

fn default_telescope_name() -> String {
    "default".to_string()
}

#[derive(Debug)]
pub enum ConfigError {
    FileNotFound(String),
    ParseError(serde_json::Error),
    IoError(std::io::Error),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::FileNotFound(path) => write!(f, "Configuration file not found: {path}"),
            ConfigError::ParseError(e) => write!(f, "Failed to parse configuration: {e}"),
            ConfigError::IoError(e) => write!(f, "IO error reading configuration: {e}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<serde_json::Error> for ConfigError {
    fn from(err: serde_json::Error) -> Self {
        ConfigError::ParseError(err)
    }
}

impl From<std::io::Error> for ConfigError {
    fn from(err: std::io::Error) -> Self {
        ConfigError::IoError(err)
    }
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            base_url: "http://192.168.0.82:1888".to_string(),
            timeout_seconds: 30,
            retry_attempts: 3,
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            enable_file_logging: false,
            log_file: "spacecat.log".to_string(),
        }
    }
}

impl Default for TelescopeConfig {
    fn default() -> Self {
        Self {
            name: default_telescope_name(),
            api: ApiConfig::default(),
            chat: TelescopeChatOverrides::default(),
            image_cooldown_seconds: default_image_cooldown_seconds(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            logging: LoggingConfig::default(),
            chat: ChatConfig::default(),
            telescopes: vec![TelescopeConfig::default()],
        }
    }
}

/// Wire formats accepted by `Config`'s deserializer.
///
/// - `New`: `{ logging, chat, telescopes: [...] }` — the multi-telescope shape.
/// - `Legacy`: `{ logging, api, chat, image_cooldown_seconds, [discord] }` —
///   the original single-telescope shape. The legacy top-level `chat` block
///   (with `webhook_url` / `room_id` keys) maps to the new shared `chat`
///   thanks to the `#[serde(alias = ...)]` on `SharedDiscordConfig` and
///   `SharedMatrixConfig`. Legacy is normalized into a one-element
///   `telescopes` list with name "default" and empty per-telescope overrides
///   — every post uses the shared defaults.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ConfigEnvelope {
    New {
        #[serde(default)]
        logging: LoggingConfig,
        #[serde(default)]
        chat: ChatConfig,
        telescopes: Vec<TelescopeConfig>,
    },
    Legacy {
        #[serde(default)]
        logging: LoggingConfig,
        api: ApiConfig,
        #[serde(default)]
        chat: ChatConfig,
        #[serde(default = "default_image_cooldown_seconds")]
        image_cooldown_seconds: u64,
        /// Older configs put a Discord webhook at the top level (before the
        /// `chat` block existed). Honored when `chat.discord` is absent.
        #[serde(default)]
        discord: Option<DiscordConfig>,
    },
}

impl From<ConfigEnvelope> for Config {
    fn from(env: ConfigEnvelope) -> Self {
        match env {
            ConfigEnvelope::New {
                logging,
                chat,
                telescopes,
            } => Self {
                logging,
                chat,
                telescopes,
            },
            ConfigEnvelope::Legacy {
                logging,
                api,
                mut chat,
                image_cooldown_seconds,
                discord,
            } => {
                // Honor the legacy top-level `discord` block when no
                // `chat.discord` was provided.
                if chat.discord.is_none()
                    && let Some(d) = discord
                {
                    chat.discord = Some(SharedDiscordConfig {
                        enabled: d.enabled,
                        default_webhook_url: Some(d.webhook_url),
                    });
                }
                Self {
                    logging,
                    chat,
                    telescopes: vec![TelescopeConfig {
                        name: default_telescope_name(),
                        api,
                        chat: TelescopeChatOverrides::default(),
                        image_cooldown_seconds,
                    }],
                }
            }
        }
    }
}

impl<'de> Deserialize<'de> for Config {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        ConfigEnvelope::deserialize(deserializer).map(Config::from)
    }
}

impl Config {
    /// Load configuration from a JSON file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let path_ref = path.as_ref();

        if !path_ref.exists() {
            return Err(ConfigError::FileNotFound(
                path_ref.to_string_lossy().to_string(),
            ));
        }

        let content = fs::read_to_string(path_ref)?;
        let config: Config = serde_json::from_str(&content)?;
        Ok(config)
    }

    /// Load configuration from default file (config.json)
    pub fn load_default() -> Result<Self, ConfigError> {
        Self::load_from_file("config.json")
    }

    /// Load configuration with fallback to default
    pub fn load_or_default() -> Self {
        match Self::load_default() {
            Ok(config) => {
                println!("Loaded configuration from config.json");
                config
            }
            Err(e) => {
                println!("Failed to load config.json ({e}), using defaults");
                Self::default()
            }
        }
    }

    /// Load configuration from specified file with fallback to default
    pub fn load_or_default_from<P: AsRef<Path>>(path: P) -> Self {
        let path_ref = path.as_ref();
        match Self::load_from_file(path_ref) {
            Ok(config) => {
                println!("Loaded configuration from {}", path_ref.display());
                config
            }
            Err(e) => {
                println!(
                    "Failed to load {} ({}), using defaults",
                    path_ref.display(),
                    e
                );
                Self::default()
            }
        }
    }

    /// Save configuration to a JSON file
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), ConfigError> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Save configuration to default file (config.json)
    pub fn save_default(&self) -> Result<(), ConfigError> {
        self.save_to_file("config.json")
    }

    /// Create a sample configuration file
    pub fn create_sample_config<P: AsRef<Path>>(path: P) -> Result<(), ConfigError> {
        let sample_config = Self::default();
        sample_config.save_to_file(path)
    }

    /// Look up a telescope by name. If `name` is None and there's exactly one
    /// telescope, returns it. Otherwise returns an error listing the names.
    pub fn pick_telescope(&self, name: Option<&str>) -> Result<&TelescopeConfig, String> {
        match (name, self.telescopes.as_slice()) {
            (Some(n), _) => self
                .telescopes
                .iter()
                .find(|t| t.name == n)
                .ok_or_else(|| {
                    let known: Vec<&str> =
                        self.telescopes.iter().map(|t| t.name.as_str()).collect();
                    format!("Telescope '{n}' not found. Known telescopes: {known:?}")
                }),
            (None, [only]) => Ok(only),
            (None, []) => Err("No telescopes configured.".to_string()),
            (None, many) => {
                let names: Vec<&str> = many.iter().map(|t| t.name.as_str()).collect();
                Err(format!(
                    "Multiple telescopes configured; pass --telescope <name>. Known: {names:?}"
                ))
            }
        }
    }

    /// Validate every telescope configuration
    pub fn validate(&self) -> Result<(), String> {
        let valid_levels = ["error", "warn", "info", "debug", "trace"];
        if !valid_levels.contains(&self.logging.level.as_str()) {
            return Err(format!(
                "Invalid logging level '{}'. Valid levels: {:?}",
                self.logging.level, valid_levels
            ));
        }

        if self.telescopes.is_empty() {
            return Err("At least one telescope must be configured.".to_string());
        }

        // Validate shared Matrix config (presence + URL shape)
        if let Some(matrix) = &self.chat.matrix
            && matrix.enabled
        {
            if matrix.homeserver_url.is_empty() {
                return Err(
                    "Matrix homeserver URL cannot be empty when Matrix is enabled".to_string(),
                );
            }
            if !matrix.homeserver_url.starts_with("https://")
                && !matrix.homeserver_url.starts_with("http://")
            {
                return Err("Matrix homeserver URL must start with http:// or https://".to_string());
            }
            if matrix.username.is_empty() {
                return Err("Matrix username cannot be empty when Matrix is enabled".to_string());
            }
            if matrix.password.is_empty() {
                return Err("Matrix password cannot be empty when Matrix is enabled".to_string());
            }
        }

        if let Some(discord) = &self.chat.discord
            && discord.enabled
            && let Some(url) = &discord.default_webhook_url
            && !url.starts_with("https://discord.com/api/webhooks/")
            && !url.starts_with("https://discordapp.com/api/webhooks/")
        {
            return Err(
                "Default Discord webhook URL must be a valid Discord webhook URL".to_string(),
            );
        }

        let mut seen = std::collections::HashSet::new();
        for t in &self.telescopes {
            if !seen.insert(t.name.clone()) {
                return Err(format!("Duplicate telescope name '{}'", t.name));
            }
            t.validate(&self.chat)?;
        }
        Ok(())
    }
}

impl TelescopeConfig {
    /// Validate per-telescope settings, including that every chat override
    /// has a corresponding enabled service in the shared config to fall back
    /// to (otherwise the override would be a dead reference).
    pub fn validate(&self, shared_chat: &ChatConfig) -> Result<(), String> {
        let ctx = |msg: String| format!("telescope '{}': {msg}", self.name);

        if self.name.is_empty() {
            return Err("Telescope name cannot be empty".to_string());
        }

        if self.api.base_url.is_empty() {
            return Err(ctx("API base URL cannot be empty".to_string()));
        }

        if !self.api.base_url.starts_with("http://") && !self.api.base_url.starts_with("https://") {
            return Err(ctx(
                "API base URL must start with http:// or https://".to_string(),
            ));
        }

        if self.api.timeout_seconds == 0 {
            return Err(ctx("Timeout seconds must be greater than 0".to_string()));
        }

        if self.api.timeout_seconds > 300 {
            return Err(ctx(
                "Timeout seconds should not exceed 300 (5 minutes)".to_string(),
            ));
        }

        if self.api.retry_attempts > 10 {
            return Err(ctx("Retry attempts should not exceed 10".to_string()));
        }

        // Discord override: must look like a webhook URL, and shared Discord
        // must be enabled (otherwise the service won't exist at runtime).
        if let Some(url) = &self.chat.discord_webhook_url {
            if !url.starts_with("https://discord.com/api/webhooks/")
                && !url.starts_with("https://discordapp.com/api/webhooks/")
            {
                return Err(ctx(
                    "Discord webhook URL must be a valid Discord webhook URL".to_string(),
                ));
            }
            if shared_chat.discord.as_ref().is_none_or(|d| !d.enabled) {
                return Err(ctx(
                    "discord_webhook_url override set but shared chat.discord is not enabled"
                        .to_string(),
                ));
            }
        }

        if let Some(_room) = &self.chat.matrix_room_id
            && shared_chat.matrix.as_ref().is_none_or(|m| !m.enabled)
        {
            return Err(ctx(
                "matrix_room_id override set but shared chat.matrix is not enabled".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.telescopes.len(), 1);
        assert_eq!(config.telescopes[0].name, "default");
        assert_eq!(config.telescopes[0].api.timeout_seconds, 30);
        assert_eq!(config.logging.level, "info");
    }

    #[test]
    fn test_config_validation() {
        let mut config = Config::default();
        assert!(config.validate().is_ok());

        // Invalid URL
        config.telescopes[0].api.base_url = "invalid-url".to_string();
        assert!(config.validate().is_err());

        // Empty URL
        config.telescopes[0].api.base_url = "".to_string();
        assert!(config.validate().is_err());

        // Fix URL, break logging level
        config.telescopes[0].api.base_url = "http://localhost:8080".to_string();
        config.logging.level = "invalid".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_legacy_config_loads() {
        // Old single-telescope shape — top-level api/chat/image_cooldown_seconds.
        // The legacy chat block uses `webhook_url` (no `default_` prefix); the
        // SharedDiscordConfig alias handles this.
        let json = r#"{
            "api": {
                "base_url": "http://192.168.0.81:1888",
                "timeout_seconds": 30,
                "retry_attempts": 3
            },
            "logging": {
                "level": "info",
                "enable_file_logging": false,
                "log_file": "spacecat.log"
            },
            "chat": {
                "discord": {
                    "enabled": true,
                    "webhook_url": "https://discord.com/api/webhooks/123/abc"
                },
                "matrix": {
                    "enabled": false,
                    "homeserver_url": "https://m.example.com",
                    "username": "@bot:example.com",
                    "password": "secret",
                    "room_id": "!legacy:example.com"
                }
            },
            "image_cooldown_seconds": 60
        }"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.telescopes.len(), 1);
        assert_eq!(config.telescopes[0].name, "default");
        assert_eq!(config.telescopes[0].api.base_url, "http://192.168.0.81:1888");
        // Legacy `chat.discord.webhook_url` is aliased into the new shared
        // default location.
        let discord = config.chat.discord.as_ref().unwrap();
        assert_eq!(
            discord.default_webhook_url.as_deref(),
            Some("https://discord.com/api/webhooks/123/abc")
        );
        let matrix = config.chat.matrix.as_ref().unwrap();
        assert_eq!(matrix.default_room_id.as_deref(), Some("!legacy:example.com"));
        // No per-telescope overrides set.
        assert!(config.telescopes[0].chat.discord_webhook_url.is_none());
        assert!(config.telescopes[0].chat.matrix_room_id.is_none());
    }

    #[test]
    fn test_new_multi_telescope_config_loads() {
        let json = r#"{
            "logging": {
                "level": "info",
                "enable_file_logging": false,
                "log_file": "spacecat.log"
            },
            "chat": {
                "discord": {
                    "enabled": true,
                    "default_webhook_url": "https://discord.com/api/webhooks/0/default"
                }
            },
            "telescopes": [
                {
                    "name": "c925",
                    "api": { "base_url": "http://192.168.0.81:1888", "timeout_seconds": 30, "retry_attempts": 3 },
                    "chat": {
                        "discord_webhook_url": "https://discord.com/api/webhooks/0/c925"
                    }
                },
                {
                    "name": "esprit",
                    "api": { "base_url": "http://192.168.0.82:1888", "timeout_seconds": 30, "retry_attempts": 3 },
                    "image_cooldown_seconds": 120
                }
            ]
        }"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.telescopes.len(), 2);
        assert_eq!(config.telescopes[0].name, "c925");
        assert_eq!(
            config.telescopes[0].chat.discord_webhook_url.as_deref(),
            Some("https://discord.com/api/webhooks/0/c925")
        );
        assert!(config.telescopes[1].chat.discord_webhook_url.is_none());
        assert_eq!(config.telescopes[1].image_cooldown_seconds, 120);
        let shared = config.chat.discord.as_ref().unwrap();
        assert_eq!(
            shared.default_webhook_url.as_deref(),
            Some("https://discord.com/api/webhooks/0/default")
        );
    }

    #[test]
    fn test_pick_telescope() {
        let config: Config = serde_json::from_str(
            r#"{
                "telescopes": [
                    { "name": "c925", "api": { "base_url": "http://a", "timeout_seconds": 30, "retry_attempts": 3 } },
                    { "name": "esprit", "api": { "base_url": "http://b", "timeout_seconds": 30, "retry_attempts": 3 } }
                ]
            }"#,
        )
        .unwrap();

        assert_eq!(config.pick_telescope(Some("c925")).unwrap().name, "c925");
        assert!(config.pick_telescope(Some("unknown")).is_err());
        // Two telescopes, no name -> error
        assert!(config.pick_telescope(None).is_err());
    }

    #[test]
    fn test_pick_telescope_single_no_name() {
        let config = Config::default();
        // Single telescope, no name -> ok
        assert_eq!(config.pick_telescope(None).unwrap().name, "default");
    }

    #[test]
    fn test_duplicate_telescope_names_rejected() {
        let config: Config = serde_json::from_str(
            r#"{
                "telescopes": [
                    { "name": "scope", "api": { "base_url": "http://a", "timeout_seconds": 30, "retry_attempts": 3 } },
                    { "name": "scope", "api": { "base_url": "http://b", "timeout_seconds": 30, "retry_attempts": 3 } }
                ]
            }"#,
        )
        .unwrap();
        assert!(config.validate().is_err());
    }
}
