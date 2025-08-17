use super::{ChatMessage, ChatService};
use crate::discord::{DiscordWebhook, Embed, colors};
use crate::error::ChatError;
use async_trait::async_trait;

pub struct DiscordChatService {
    webhook: DiscordWebhook,
}

impl DiscordChatService {
    pub fn new(webhook_url: &str) -> Result<Self, ChatError> {
        let webhook = DiscordWebhook::new(webhook_url.to_string())
            .map_err(|e| ChatError::Discord { message: e.to_string() })?;
        Ok(Self { webhook })
    }
}

#[async_trait]
impl ChatService for DiscordChatService {
    async fn send_message(
        &self,
        message: &ChatMessage,
    ) -> Result<(), ChatError> {
        let mut embed = Embed::new().title(&message.title);

        if let Some(color) = message.color {
            embed = embed.color(color);
        } else {
            embed = embed.color(colors::GRAY);
        }

        if let Some(timestamp) = &message.timestamp {
            embed = embed.timestamp(timestamp);
        }

        for field in &message.fields {
            embed = embed.field(&field.name, &field.value, field.inline);
        }

        if let Some(footer_text) = &message.footer {
            embed = embed.footer(footer_text, None);
        }

        self.webhook.execute_with_embed(None, embed).await
            .map_err(|e| ChatError::Discord { message: e.to_string() })?;
        Ok(())
    }

    async fn send_message_with_image(
        &self,
        message: &ChatMessage,
        image_data: &[u8],
        filename: &str,
    ) -> Result<(), ChatError> {
        let mut embed = Embed::new().title(&message.title);

        if let Some(color) = message.color {
            embed = embed.color(color);
        } else {
            embed = embed.color(colors::GRAY);
        }

        if let Some(timestamp) = &message.timestamp {
            embed = embed.timestamp(timestamp);
        }

        for field in &message.fields {
            embed = embed.field(&field.name, &field.value, field.inline);
        }

        if let Some(footer_text) = &message.footer {
            embed = embed.footer(footer_text, None);
        }

        self.webhook
            .execute_with_file(None, Some(embed), image_data, filename)
            .await
            .map_err(|e| ChatError::Discord { message: e.to_string() })?;
        Ok(())
    }

    fn service_name(&self) -> &'static str {
        "Discord"
    }
}
