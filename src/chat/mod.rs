mod discord_service;
mod matrix_service;

pub use discord_service::DiscordChatService;
pub use matrix_service::MatrixChatService;

use crate::api::SpaceCatApiClient;
use crate::error::ChatError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Represents a field in a chat message
#[derive(Debug, Clone)]
pub struct ChatField {
    pub name: String,
    pub value: String,
    pub inline: bool,
}

/// Represents a chat message to be sent
#[derive(Debug, Clone, Default)]
pub struct ChatMessage {
    pub title: String,
    pub color: Option<u32>,
    pub fields: Vec<ChatField>,
    pub footer: Option<String>,
    pub timestamp: Option<String>,
}

impl ChatMessage {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            color: None,
            fields: Vec::new(),
            footer: None,
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
        }
    }

    pub fn color(mut self, color: u32) -> Self {
        self.color = Some(color);
        self
    }

    pub fn field(mut self, name: &str, value: &str, inline: bool) -> Self {
        self.fields.push(ChatField {
            name: name.to_string(),
            value: value.to_string(),
            inline,
        });
        self
    }

    pub fn footer(mut self, text: &str) -> Self {
        self.footer = Some(text.to_string());
        self
    }
}

/// Configuration for Discord chat service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordConfig {
    pub webhook_url: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// Configuration for Matrix chat service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatrixConfig {
    pub homeserver_url: String,
    pub username: String,
    pub password: String,
    pub room_id: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

/// Configuration for all chat services
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChatConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discord: Option<DiscordConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matrix: Option<MatrixConfig>,
}

/// Trait for chat service implementations
#[async_trait]
pub trait ChatService: Send + Sync {
    /// Send a message to the chat service
    async fn send_message(&self, message: &ChatMessage) -> Result<(), ChatError>;

    /// Send a message with an image attachment
    async fn send_message_with_image(
        &self,
        message: &ChatMessage,
        image_data: &[u8],
        filename: &str,
    ) -> Result<(), ChatError>;

    /// Get the service name for logging
    fn service_name(&self) -> &'static str;
}

/// Chat service manager that can broadcast to multiple services
pub struct ChatServiceManager {
    services: Vec<Box<dyn ChatService>>,
}

impl ChatServiceManager {
    pub fn new() -> Self {
        Self {
            services: Vec::new(),
        }
    }

    pub fn add_service(&mut self, service: Box<dyn ChatService>) {
        self.services.push(service);
    }

    pub async fn send_message(&self, message: &ChatMessage) {
        for service in &self.services {
            if let Err(e) = service.send_message(message).await {
                eprintln!(
                    "Failed to send message to {}: {}",
                    service.service_name(),
                    e
                );
            }
        }
    }

    pub async fn send_message_with_image(
        &self,
        message: &ChatMessage,
        client: &SpaceCatApiClient,
        image_index: u32,
    ) {
        // Try to download thumbnail
        match client.get_thumbnail(image_index).await {
            Ok(thumbnail_data) => {
                let filename = format!("thumbnail_{}.jpg", image_index);
                for service in &self.services {
                    if let Err(e) = service
                        .send_message_with_image(message, &thumbnail_data.data, &filename)
                        .await
                    {
                        eprintln!(
                            "Failed to send image message to {}: {}",
                            service.service_name(),
                            e
                        );
                    }
                }
            }
            Err(e) => {
                eprintln!(
                    "Failed to download thumbnail for image {}: {}",
                    image_index, e
                );
                // Fallback to sending without image
                self.send_message(message).await;
            }
        }
    }

    pub fn service_count(&self) -> usize {
        self.services.len()
    }
}

impl Default for ChatServiceManager {
    fn default() -> Self {
        Self::new()
    }
}
