use crate::chat::{ChatConfig, TelescopeChatOverrides};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use url::Url;

/// In-memory configuration for a plugin-owned local Direct runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub chat: ChatConfig,
    pub telescopes: Vec<TelescopeConfig>,
}

/// Per-profile routing and updater behavior. N.I.N.A. data always arrives
/// through an explicit Direct source supplied by the plugin or Hub.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelescopeConfig {
    pub name: String,
    #[serde(default)]
    pub chat: TelescopeChatOverrides,
    #[serde(default = "default_image_cooldown_seconds")]
    pub image_cooldown_seconds: u64,
    #[serde(default)]
    pub reconnect: ReconnectConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconnectConfig {
    #[serde(default = "default_reconnect_initial_seconds")]
    pub initial_seconds: u64,
    #[serde(default = "default_reconnect_max_seconds")]
    pub max_seconds: u64,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            initial_seconds: default_reconnect_initial_seconds(),
            max_seconds: default_reconnect_max_seconds(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub enable_file_logging: bool,
    pub log_file: String,
}

fn default_image_cooldown_seconds() -> u64 {
    60
}

fn default_reconnect_initial_seconds() -> u64 {
    60
}

fn default_reconnect_max_seconds() -> u64 {
    600
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            enable_file_logging: false,
            log_file: "chatstronomy.log".to_string(),
        }
    }
}

impl Default for TelescopeConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            chat: TelescopeChatOverrides::default(),
            image_cooldown_seconds: default_image_cooldown_seconds(),
            reconnect: ReconnectConfig::default(),
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

#[derive(Debug)]
pub enum ConfigError {
    FileNotFound(String),
    ParseError(serde_json::Error),
    IoError(std::io::Error),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FileNotFound(path) => write!(formatter, "Configuration file not found: {path}"),
            Self::ParseError(error) => write!(formatter, "Failed to parse configuration: {error}"),
            Self::IoError(error) => write!(formatter, "IO error reading configuration: {error}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<serde_json::Error> for ConfigError {
    fn from(error: serde_json::Error) -> Self {
        Self::ParseError(error)
    }
}

impl From<std::io::Error> for ConfigError {
    fn from(error: std::io::Error) -> Self {
        Self::IoError(error)
    }
}

/// Load any JSON-decodable configuration file, with a distinct missing-file
/// error. The Hub configuration also uses this helper.
pub fn load_json_file<T, P>(path: P) -> Result<T, ConfigError>
where
    T: serde::de::DeserializeOwned,
    P: AsRef<Path>,
{
    let path = path.as_ref();
    if !path.exists() {
        return Err(ConfigError::FileNotFound(
            path.to_string_lossy().to_string(),
        ));
    }
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        let valid_levels = ["error", "warn", "info", "debug", "trace"];
        if !valid_levels.contains(&self.logging.level.as_str()) {
            return Err(format!("Invalid logging level '{}'", self.logging.level));
        }
        if self.telescopes.is_empty() {
            return Err("At least one telescope must be configured.".to_string());
        }

        if let Some(matrix) = &self.chat.matrix
            && matrix.enabled
        {
            if !is_valid_https_url(&matrix.homeserver_url) {
                return Err("Matrix homeserver URL must be an absolute https:// URL".to_string());
            }
            if matrix.username.is_empty() || matrix.password.is_empty() {
                return Err("Matrix username and password are required".to_string());
            }
        }
        if let Some(discord) = &self.chat.discord
            && discord.enabled
            && let Some(url) = &discord.default_webhook_url
            && !is_valid_discord_webhook_url(url)
        {
            return Err("Default Discord webhook URL is invalid".to_string());
        }
        if let Some(bot) = &self.chat.discord_bot
            && bot.enabled
            && bot.token.is_empty()
        {
            return Err("Discord bot token cannot be empty".to_string());
        }

        let mut names = std::collections::HashSet::new();
        for telescope in &self.telescopes {
            if !names.insert(telescope.name.clone()) {
                return Err(format!("Duplicate telescope name '{}'", telescope.name));
            }
            telescope.validate(&self.chat)?;
        }
        Ok(())
    }
}

impl TelescopeConfig {
    pub fn validate(&self, shared_chat: &ChatConfig) -> Result<(), String> {
        let context = |message: String| format!("telescope '{}': {message}", self.name);
        if self.name.trim().is_empty() {
            return Err("Telescope name cannot be empty".to_string());
        }
        if let Some(url) = &self.chat.discord_webhook_url {
            if !is_valid_discord_webhook_url(url) {
                return Err(context("Discord webhook URL is invalid".to_string()));
            }
            if shared_chat
                .discord
                .as_ref()
                .is_none_or(|config| !config.enabled)
            {
                return Err(context(
                    "Discord webhook service is not enabled".to_string(),
                ));
            }
        }
        if self.chat.matrix_room_id.is_some()
            && shared_chat
                .matrix
                .as_ref()
                .is_none_or(|config| !config.enabled)
        {
            return Err(context("Matrix service is not enabled".to_string()));
        }
        if self.chat.discord_channel_id.is_some()
            && shared_chat
                .discord_bot
                .as_ref()
                .is_none_or(|config| !config.enabled)
        {
            return Err(context("Discord bot is not enabled".to_string()));
        }
        Ok(())
    }
}

pub(crate) fn is_valid_https_url(value: &str) -> bool {
    Url::parse(value).is_ok_and(|url| url.scheme() == "https" && url.host_str().is_some())
}

pub(crate) fn is_valid_discord_webhook_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    let valid_host = matches!(url.host_str(), Some("discord.com" | "discordapp.com"))
        || url.host_str().is_some_and(|host| {
            host.ends_with(".discord.com") || host.ends_with(".discordapp.com")
        });
    let segments: Vec<_> = url
        .path_segments()
        .map(Iterator::collect)
        .unwrap_or_default();
    let (id, token) = match segments.as_slice() {
        [api, webhooks, id, token]
            if api.eq_ignore_ascii_case("api") && webhooks.eq_ignore_ascii_case("webhooks") =>
        {
            (*id, *token)
        }
        [api, version, webhooks, id, token]
            if api.eq_ignore_ascii_case("api")
                && is_discord_api_version(version)
                && webhooks.eq_ignore_ascii_case("webhooks") =>
        {
            (*id, *token)
        }
        _ => ("", ""),
    };
    url.scheme() == "https"
        && valid_host
        && url.port_or_known_default() == Some(443)
        && id.parse::<u64>().is_ok_and(|id| id != 0)
        && !token.is_empty()
}

fn is_discord_api_version(value: &str) -> bool {
    value
        .strip_prefix('v')
        .or_else(|| value.strip_prefix('V'))
        .is_some_and(|version| !version.is_empty() && version.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_direct_runtime_config_is_valid() {
        assert!(Config::default().validate().is_ok());
    }

    #[test]
    fn duplicate_telescope_names_are_rejected() {
        let mut config = Config::default();
        config.telescopes.push(TelescopeConfig::default());
        assert!(config.validate().unwrap_err().contains("Duplicate"));
    }

    #[test]
    fn matrix_requires_https() {
        assert!(is_valid_https_url("https://matrix.example.test"));
        assert!(!is_valid_https_url("http://matrix.example.test"));
    }

    #[test]
    fn discord_webhook_validation_accepts_versioned_paths_only() {
        assert!(is_valid_discord_webhook_url(
            "https://discord.com/api/v10/webhooks/123/token"
        ));
        assert!(!is_valid_discord_webhook_url(
            "https://discord.com/api/webhooks/0/token"
        ));
    }
}
