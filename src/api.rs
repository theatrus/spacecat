use crate::config::ApiConfig;
use crate::events::EventHistoryResponse;
use crate::images::{ImageHistoryResponse, ImageResponse};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct SpaceCatApiClient {
    client: Client,
    base_url: String,
    retry_attempts: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct VersionResponse {
    pub response: String,
    pub error: String,
    pub status_code: i32,
    pub success: bool,
    #[serde(rename = "Type")]
    pub response_type: String,
}

#[derive(Debug)]
pub enum ApiError {
    Network(reqwest::Error),
    Parse(serde_json::Error),
    Http { status: u16, message: String },
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Network(e) => write!(f, "Network error: {}", e),
            ApiError::Parse(e) => write!(f, "Parse error: {}", e),
            ApiError::Http { status, message } => write!(f, "HTTP error {}: {}", status, message),
        }
    }
}

impl std::error::Error for ApiError {}

impl From<reqwest::Error> for ApiError {
    fn from(err: reqwest::Error) -> Self {
        ApiError::Network(err)
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        ApiError::Parse(err)
    }
}

impl SpaceCatApiClient {
    /// Create a new API client with the given configuration
    pub fn new(config: ApiConfig) -> Result<Self, ApiError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()?;

        Ok(Self {
            client,
            base_url: config.base_url,
            retry_attempts: config.retry_attempts,
        })
    }

    /// Create a new API client with default configuration
    pub fn default() -> Result<Self, ApiError> {
        Self::new(ApiConfig::default())
    }

    /// Create a new API client with a custom base URL
    pub fn with_url(base_url: &str) -> Result<Self, ApiError> {
        let config = ApiConfig {
            base_url: base_url.to_string(),
            ..ApiConfig::default()
        };
        Self::new(config)
    }

    /// Fetch event history from the /event-history endpoint
    pub async fn get_event_history(&self) -> Result<EventHistoryResponse, ApiError> {
        self.get_event_history_with_params(&[]).await
    }

    /// Fetch event history with custom query parameters
    pub async fn get_event_history_with_params(
        &self,
        params: &[(&str, &str)],
    ) -> Result<EventHistoryResponse, ApiError> {
        self.request_with_retry("/event-history", params).await
    }

    /// Generic request method with retry logic
    async fn request_with_retry(
        &self,
        endpoint: &str,
        params: &[(&str, &str)],
    ) -> Result<EventHistoryResponse, ApiError> {
        self.generic_request_with_retry(endpoint, params).await
    }

    /// Generic retry handler for any JSON response type
    async fn generic_request_with_retry<T>(
        &self,
        endpoint: &str,
        params: &[(&str, &str)],
    ) -> Result<T, ApiError>
    where
        T: serde::de::DeserializeOwned,
    {
        let url = format!("{}/v2/api{}", self.base_url, endpoint);
        let mut last_error = None;

        for attempt in 0..=self.retry_attempts {
            if attempt > 0 {
                println!(
                    "Retrying API request (attempt {} of {})",
                    attempt + 1,
                    self.retry_attempts + 1
                );
                tokio::time::sleep(Duration::from_millis(1000 * attempt as u64)).await;
            }

            let mut request = self.client.get(&url);
            if !params.is_empty() {
                request = request.query(params);
            }

            match request.send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        match response.json().await {
                            Ok(parsed_response) => return Ok(parsed_response),
                            Err(e) => {
                                last_error = Some(ApiError::Network(e));
                                continue;
                            }
                        }
                    } else {
                        let status = response.status().as_u16();
                        let message = response
                            .text()
                            .await
                            .unwrap_or_else(|_| "Unknown error".to_string());
                        return Err(ApiError::Http { status, message });
                    }
                }
                Err(e) => {
                    last_error = Some(ApiError::Network(e));
                    continue;
                }
            }
        }

        Err(last_error.unwrap_or_else(|| ApiError::Http {
            status: 500,
            message: "Max retries exceeded".to_string(),
        }))
    }

    /// Get the base URL for this client
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Get API version (health check)
    pub async fn get_version(&self) -> Result<VersionResponse, ApiError> {
        self.version_request_with_retry().await
    }

    /// Health check endpoint - returns true if API is available
    pub async fn health_check(&self) -> Result<bool, ApiError> {
        match self.get_version().await {
            Ok(version_response) => Ok(version_response.success),
            Err(_) => Ok(false),
        }
    }

    /// Version request with retry logic
    async fn version_request_with_retry(&self) -> Result<VersionResponse, ApiError> {
        self.generic_request_with_retry("/version", &[]).await
    }

    /// Fetch image history from the /image-history endpoint
    pub async fn get_image_history(&self) -> Result<ImageHistoryResponse, ApiError> {
        self.get_image_history_with_params(&[]).await
    }

    /// Fetch image history with custom query parameters
    pub async fn get_image_history_with_params(
        &self,
        params: &[(&str, &str)],
    ) -> Result<ImageHistoryResponse, ApiError> {
        self.generic_request_with_retry("/image-history", params).await
    }

    /// Fetch all image history (equivalent to ?all=true parameter)
    pub async fn get_all_image_history(&self) -> Result<ImageHistoryResponse, ApiError> {
        self.get_image_history_with_params(&[("all", "true")]).await
    }

    /// Fetch a specific image by index with autoPrepare=true by default
    pub async fn get_image(&self, index: u32) -> Result<ImageResponse, ApiError> {
        self.get_image_with_params(index, &[("autoPrepare", "true")]).await
    }

    /// Fetch a specific image by index with custom parameters
    pub async fn get_image_with_params(
        &self,
        index: u32,
        params: &[(&str, &str)],
    ) -> Result<ImageResponse, ApiError> {
        let endpoint = format!("/image/{}", index);
        self.generic_request_with_retry(&endpoint, params).await
    }
}
