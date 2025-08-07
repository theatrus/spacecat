use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub api: ApiConfig,
    pub logging: LoggingConfig,
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

#[derive(Debug)]
pub enum ConfigError {
    FileNotFound(String),
    ParseError(serde_json::Error),
    IoError(std::io::Error),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::FileNotFound(path) => write!(f, "Configuration file not found: {}", path),
            ConfigError::ParseError(e) => write!(f, "Failed to parse configuration: {}", e),
            ConfigError::IoError(e) => write!(f, "IO error reading configuration: {}", e),
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

impl Default for Config {
    fn default() -> Self {
        Self {
            api: ApiConfig::default(),
            logging: LoggingConfig::default(),
        }
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
                println!("Failed to load config.json ({}), using defaults", e);
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

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), String> {
        // Validate API URL
        if self.api.base_url.is_empty() {
            return Err("API base URL cannot be empty".to_string());
        }

        if !self.api.base_url.starts_with("http://") && !self.api.base_url.starts_with("https://") {
            return Err("API base URL must start with http:// or https://".to_string());
        }

        // Validate timeout
        if self.api.timeout_seconds == 0 {
            return Err("Timeout seconds must be greater than 0".to_string());
        }

        if self.api.timeout_seconds > 300 {
            return Err("Timeout seconds should not exceed 300 (5 minutes)".to_string());
        }

        // Validate retry attempts
        if self.api.retry_attempts > 10 {
            return Err("Retry attempts should not exceed 10".to_string());
        }

        // Validate logging level
        let valid_levels = ["error", "warn", "info", "debug", "trace"];
        if !valid_levels.contains(&self.logging.level.as_str()) {
            return Err(format!(
                "Invalid logging level '{}'. Valid levels: {:?}",
                self.logging.level, valid_levels
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.api.base_url, "http://192.168.0.82:1888");
        assert_eq!(config.api.timeout_seconds, 30);
        assert_eq!(config.logging.level, "info");
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let json = serde_json::to_string_pretty(&config).unwrap();
        let deserialized: Config = serde_json::from_str(&json).unwrap();

        assert_eq!(config.api.base_url, deserialized.api.base_url);
        assert_eq!(config.api.timeout_seconds, deserialized.api.timeout_seconds);
    }

    #[test]
    fn test_config_validation() {
        let mut config = Config::default();
        assert!(config.validate().is_ok());

        // Test invalid URL
        config.api.base_url = "invalid-url".to_string();
        assert!(config.validate().is_err());

        // Test empty URL
        config.api.base_url = "".to_string();
        assert!(config.validate().is_err());

        // Fix URL and test invalid timeout
        config.api.base_url = "http://localhost:8080".to_string();
        config.api.timeout_seconds = 0;
        assert!(config.validate().is_err());

        // Test invalid logging level
        config.api.timeout_seconds = 30;
        config.logging.level = "invalid".to_string();
        assert!(config.validate().is_err());
    }
}
