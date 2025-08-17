//! Error types for SpaceCat
//!
//! This module defines all custom error types used throughout the application,
//! providing better error handling and debugging information.

use thiserror::Error;

/// Main error type for SpaceCat operations
#[derive(Error, Debug)]
pub enum SpaceCatError {
    /// API-related errors
    #[error("API error: {0}")]
    Api(#[from] crate::api::ApiError),

    /// Configuration errors
    #[error("Configuration error: {0}")]
    Config(#[from] crate::config::ConfigError),

    /// Chat service errors
    #[error("Chat service error: {0}")]
    Chat(#[from] ChatError),

    /// Service wrapper errors
    #[error("Service error: {0}")]
    Service(#[from] ServiceError),

    /// IO errors
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON parsing errors
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Network/HTTP errors
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// URL parsing errors
    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),

    /// Base64 decoding errors
    #[error("Base64 decode error: {0}")]
    Base64(#[from] base64::DecodeError),

    /// Generic error with context
    #[error("Error: {message}")]
    Generic { message: String },
}

/// Chat service specific errors
#[derive(Error, Debug)]
pub enum ChatError {
    /// Discord webhook errors
    #[error("Discord error: {message}")]
    Discord { message: String },

    /// Matrix client errors
    #[error("Matrix error: {0}")]
    Matrix(#[from] matrix_sdk::Error),

    /// Chat service initialization error
    #[error("Failed to initialize chat service: {service_name}: {reason}")]
    Initialization {
        service_name: String,
        reason: String,
    },

    /// Chat message sending error
    #[error("Failed to send message to {service_name}: {reason}")]
    MessageSend {
        service_name: String,
        reason: String,
    },

    /// No chat services configured
    #[error("No chat services are configured")]
    NoServicesConfigured,
}

/// Service wrapper and Windows service errors
#[derive(Error, Debug)]
pub enum ServiceError {
    /// Service initialization failed
    #[error("Service initialization failed: {reason}")]
    Initialization { reason: String },

    /// Service runtime error
    #[error("Service runtime error: {reason}")]
    Runtime { reason: String },

    /// Service shutdown error
    #[error("Service shutdown error: {reason}")]
    Shutdown { reason: String },

    /// Windows service specific errors
    #[cfg(windows)]
    #[error("Windows service error: {0}")]
    Windows(#[from] windows_service::Error),

    /// Tokio runtime errors
    #[error("Tokio runtime error: {0}")]
    TokioRuntime(#[from] tokio::io::Error),
}

/// Result type alias for SpaceCat operations
pub type Result<T> = std::result::Result<T, SpaceCatError>;

/// Result type alias for Chat operations
pub type ChatResult<T> = std::result::Result<T, ChatError>;

/// Result type alias for Service operations
pub type ServiceResult<T> = std::result::Result<T, ServiceError>;

impl From<String> for SpaceCatError {
    fn from(message: String) -> Self {
        Self::Generic { message }
    }
}

impl From<&str> for SpaceCatError {
    fn from(message: &str) -> Self {
        Self::Generic {
            message: message.to_string(),
        }
    }
}

impl From<String> for ChatError {
    fn from(message: String) -> Self {
        Self::Discord { message }
    }
}

impl From<&str> for ChatError {
    fn from(message: &str) -> Self {
        Self::Discord {
            message: message.to_string(),
        }
    }
}

impl From<String> for ServiceError {
    fn from(reason: String) -> Self {
        Self::Runtime { reason }
    }
}

impl From<&str> for ServiceError {
    fn from(reason: &str) -> Self {
        Self::Runtime {
            reason: reason.to_string(),
        }
    }
}
