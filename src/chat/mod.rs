mod discord_bot;
mod discord_service;
mod matrix_service;
mod status_state;

pub use discord_bot::{DiscordBotService, run_bot};
pub use discord_service::DiscordChatService;
pub use matrix_service::MatrixChatService;
pub use status_state::{StatusMessage, StatusState};

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

/// Per-telescope routing overrides. Each field, when `Some`, redirects this
/// telescope's posts away from the shared default destination configured on
/// the corresponding `ChatService`.
///
/// When `discord_channel_id` is set, the Discord bot service takes precedence
/// over webhook posting for this telescope — the webhook service defers via
/// `can_route`, and the bot routes the message to the channel.
#[derive(Debug, Clone, Default)]
pub struct ChatTarget {
    pub discord_webhook_url: Option<String>,
    pub matrix_room_id: Option<String>,
    pub discord_channel_id: Option<u64>,
}

/// Shared Discord configuration. The webhook here is the fallback destination
/// used when a telescope doesn't supply its own override.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SharedDiscordConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Default webhook used by telescopes that don't override it. Accepts
    /// either `default_webhook_url` (new) or `webhook_url` (legacy) on the
    /// wire — see the manual Deserialize impl in serde.
    #[serde(default, alias = "webhook_url")]
    pub default_webhook_url: Option<String>,
}

/// Shared Matrix configuration. The login is held once per process and reused
/// across every telescope (each telescope can post to a different room via
/// `ChatTarget::matrix_room_id`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedMatrixConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub homeserver_url: String,
    pub username: String,
    pub password: String,
    /// Default room used by telescopes that don't override it. Accepts either
    /// `default_room_id` (new) or `room_id` (legacy).
    #[serde(default, alias = "room_id")]
    pub default_room_id: Option<String>,
}

fn default_enabled() -> bool {
    true
}

/// Shared chat configuration at the top of the config file. Persistent
/// connections (Matrix login, Discord bot gateway) live here and are reused
/// across telescopes.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChatConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discord: Option<SharedDiscordConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matrix: Option<SharedMatrixConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discord_bot: Option<DiscordBotConfig>,
}

/// Shared Discord bot configuration. One bot identity / token serves every
/// telescope; each telescope can map to a different channel via
/// `TelescopeChatOverrides::discord_channel_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordBotConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Bot token from the Discord Developer Portal.
    pub token: String,
    /// Discord application ID. Not required for gateway-based slash commands
    /// (Serenity infers it from the token), but useful to keep alongside the
    /// token for HTTP interaction endpoints and tooling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application_id: Option<u64>,
    /// Discord public key, used to verify interaction payloads when running
    /// command handlers over HTTP webhooks. Unused in the gateway path
    /// (Phase 1), reserved for future use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    /// Optional fallback channel for telescopes that don't override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_channel_id: Option<u64>,
    /// Where to persist the live-status message IDs (Phase 2).
    #[serde(default = "default_state_file")]
    pub state_file: String,
    /// Discord user IDs allowed to invoke write commands (Phase 3).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub write_acl: Vec<u64>,
}

fn default_state_file() -> String {
    "./spacecat-state.json".to_string()
}

/// Per-telescope chat routing overrides. Either field, when present, replaces
/// the shared default for that service for this telescope only. Setting
/// `discord_channel_id` switches that telescope's Discord posts from the
/// webhook path to the bot path.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TelescopeChatOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discord_webhook_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matrix_room_id: Option<String>,
    /// When set, this telescope's Discord posts go through the bot to this
    /// channel; the webhook path is ignored for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discord_channel_id: Option<u64>,
}

impl TelescopeChatOverrides {
    pub fn to_chat_target(&self) -> ChatTarget {
        ChatTarget {
            discord_webhook_url: self.discord_webhook_url.clone(),
            matrix_room_id: self.matrix_room_id.clone(),
            discord_channel_id: self.discord_channel_id,
        }
    }
}

/// Trait for chat service implementations
#[async_trait]
pub trait ChatService: Send + Sync {
    async fn send_message(
        &self,
        message: &ChatMessage,
        target: &ChatTarget,
    ) -> Result<(), ChatError>;

    async fn send_message_with_image(
        &self,
        message: &ChatMessage,
        target: &ChatTarget,
        image_data: &[u8],
        filename: &str,
    ) -> Result<(), ChatError>;

    fn service_name(&self) -> &'static str;

    /// True if this service has a destination for the given target. Lets the
    /// manager skip services that would have no valid destination (e.g. a
    /// telescope without a webhook override on a Discord service with no
    /// default).
    fn can_route(&self, target: &ChatTarget) -> bool;

    /// Upsert a "live status" message: edit the previously-posted message
    /// for this telescope in place if one exists, otherwise post a new
    /// one and remember its ID. Default implementation is a no-op for
    /// services that don't support editing (webhooks, Matrix).
    async fn upsert_status(
        &self,
        _telescope: &str,
        _target: &ChatTarget,
        _message: &ChatMessage,
    ) -> Result<(), ChatError> {
        Ok(())
    }

    /// True if this service knows how to edit a previously-posted status
    /// message. Used to decide whether to bother building the embed.
    fn supports_status_upsert(&self) -> bool {
        false
    }
}

/// Chat service manager. One instance is shared across all telescopes; the
/// `ChatTarget` passed to each send selects the per-telescope destination.
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

    /// Refresh the live status message for a telescope across every service
    /// that supports editing. Currently only the Discord bot acts on this.
    pub async fn upsert_status(&self, telescope: &str, target: &ChatTarget, message: &ChatMessage) {
        for service in &self.services {
            if !service.supports_status_upsert() || !service.can_route(target) {
                continue;
            }
            if let Err(e) = service.upsert_status(telescope, target, message).await {
                eprintln!(
                    "Failed to upsert status on {} for {telescope}: {}",
                    service.service_name(),
                    e
                );
            }
        }
    }

    /// True when at least one service in the manager can edit live status
    /// messages for this target. Lets callers skip building the embed
    /// entirely when nothing would consume it.
    pub fn has_status_upsert(&self, target: &ChatTarget) -> bool {
        self.services
            .iter()
            .any(|s| s.supports_status_upsert() && s.can_route(target))
    }

    pub async fn send_message(&self, message: &ChatMessage, target: &ChatTarget) {
        for service in &self.services {
            if !service.can_route(target) {
                continue;
            }
            if let Err(e) = service.send_message(message, target).await {
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
        target: &ChatTarget,
        client: &SpaceCatApiClient,
        image_index: u32,
    ) {
        match client.get_thumbnail(image_index).await {
            Ok(thumbnail_data) => {
                let filename = format!("thumbnail_{}.jpg", image_index);
                for service in &self.services {
                    if !service.can_route(target) {
                        continue;
                    }
                    if let Err(e) = service
                        .send_message_with_image(message, target, &thumbnail_data.data, &filename)
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
                self.send_message(message, target).await;
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
