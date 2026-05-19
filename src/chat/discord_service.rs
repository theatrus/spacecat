use super::{ChatMessage, ChatService, ChatTarget};
use crate::discord::{DiscordWebhook, Embed, colors};
use crate::error::ChatError;
use async_trait::async_trait;

/// Discord chat service. Holds an optional default webhook URL; per-telescope
/// `ChatTarget::discord_webhook_url` overrides it. A new `DiscordWebhook` is
/// constructed per send so each telescope can route to a different channel.
pub struct DiscordChatService {
    default_webhook_url: Option<String>,
}

impl DiscordChatService {
    pub fn new(default_webhook_url: Option<String>) -> Self {
        Self {
            default_webhook_url,
        }
    }

    fn resolve_url<'a>(&'a self, target: &'a ChatTarget) -> Option<&'a str> {
        target
            .discord_webhook_url
            .as_deref()
            .or(self.default_webhook_url.as_deref())
    }

    fn build_webhook(&self, target: &ChatTarget) -> Result<DiscordWebhook, ChatError> {
        let url = self.resolve_url(target).ok_or_else(|| ChatError::Discord {
            message: "No Discord webhook URL available (no default and no telescope override)"
                .to_string(),
        })?;
        DiscordWebhook::new(url.to_string()).map_err(|e| ChatError::Discord {
            message: e.to_string(),
        })
    }

    fn build_embed(message: &ChatMessage) -> Embed {
        let mut embed = Embed::new().title(&message.title);
        embed = embed.color(message.color.unwrap_or(colors::GRAY));
        if let Some(timestamp) = &message.timestamp {
            embed = embed.timestamp(timestamp);
        }
        for field in &message.fields {
            embed = embed.field(&field.name, &field.value, field.inline);
        }
        if let Some(footer_text) = &message.footer {
            embed = embed.footer(footer_text, None);
        }
        embed
    }
}

#[async_trait]
impl ChatService for DiscordChatService {
    async fn send_message(
        &self,
        message: &ChatMessage,
        target: &ChatTarget,
    ) -> Result<(), ChatError> {
        let webhook = self.build_webhook(target)?;
        let embed = Self::build_embed(message);
        webhook
            .execute_with_embed(None, embed)
            .await
            .map_err(|e| ChatError::Discord {
                message: e.to_string(),
            })?;
        Ok(())
    }

    async fn send_message_with_image(
        &self,
        message: &ChatMessage,
        target: &ChatTarget,
        image_data: &[u8],
        filename: &str,
    ) -> Result<(), ChatError> {
        let webhook = self.build_webhook(target)?;
        let embed = Self::build_embed(message);
        webhook
            .execute_with_file(None, Some(embed), image_data, filename)
            .await
            .map_err(|e| ChatError::Discord {
                message: e.to_string(),
            })?;
        Ok(())
    }

    fn service_name(&self) -> &'static str {
        "Discord"
    }

    fn can_route(&self, target: &ChatTarget) -> bool {
        self.resolve_url(target).is_some()
    }
}
