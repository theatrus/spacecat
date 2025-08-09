use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DiscordWebhook {
    client: Client,
    webhook_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WebhookMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tts: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embeds: Option<Vec<Embed>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_mentions: Option<AllowedMentions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_json: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Embed {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footer: Option<EmbedFooter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<EmbedImage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<EmbedThumbnail>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<EmbedVideo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<EmbedProvider>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<EmbedAuthor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<EmbedField>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EmbedFooter {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_icon_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EmbedImage {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EmbedThumbnail {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EmbedVideo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EmbedProvider {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EmbedAuthor {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_icon_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EmbedField {
    pub name: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AllowedMentions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub users: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replied_user: Option<bool>,
}

#[derive(Debug)]
pub enum DiscordError {
    Network(reqwest::Error),
    Parse(serde_json::Error),
    Http { status: u16, message: String },
    InvalidWebhookUrl,
}

impl std::fmt::Display for DiscordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiscordError::Network(e) => write!(f, "Network error: {}", e),
            DiscordError::Parse(e) => write!(f, "Parse error: {}", e),
            DiscordError::Http { status, message } => write!(f, "HTTP error {}: {}", status, message),
            DiscordError::InvalidWebhookUrl => write!(f, "Invalid webhook URL"),
        }
    }
}

impl std::error::Error for DiscordError {}

impl From<reqwest::Error> for DiscordError {
    fn from(err: reqwest::Error) -> Self {
        DiscordError::Network(err)
    }
}

impl From<serde_json::Error> for DiscordError {
    fn from(err: serde_json::Error) -> Self {
        DiscordError::Parse(err)
    }
}

impl DiscordWebhook {
    pub fn new(webhook_url: String) -> Result<Self, DiscordError> {
        // Basic validation of webhook URL
        if !webhook_url.starts_with("https://discord.com/api/webhooks/") 
            && !webhook_url.starts_with("https://discordapp.com/api/webhooks/") {
            return Err(DiscordError::InvalidWebhookUrl);
        }

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self {
            client,
            webhook_url,
        })
    }

    pub async fn execute(&self, message: &WebhookMessage) -> Result<(), DiscordError> {
        self.execute_with_params(message, None).await
    }

    pub async fn execute_with_params(
        &self,
        message: &WebhookMessage,
        params: Option<HashMap<&str, &str>>,
    ) -> Result<(), DiscordError> {
        let mut url = self.webhook_url.clone();
        
        // Add query parameters if provided
        if let Some(params) = params {
            let query_string = params
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&");
            url = format!("{}?{}", url, query_string);
        }

        let response = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(message)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(DiscordError::Http {
                status,
                message: error_text,
            });
        }

        Ok(())
    }

    pub async fn execute_simple(&self, content: &str) -> Result<(), DiscordError> {
        let message = WebhookMessage {
            content: Some(content.to_string()),
            username: None,
            avatar_url: None,
            tts: None,
            embeds: None,
            allowed_mentions: None,
            components: None,
            files: None,
            payload_json: None,
            attachments: None,
            flags: None,
        };
        
        self.execute(&message).await
    }

    pub async fn execute_with_embed(&self, content: Option<&str>, embed: Embed) -> Result<(), DiscordError> {
        let message = WebhookMessage {
            content: content.map(|s| s.to_string()),
            username: None,
            avatar_url: None,
            tts: None,
            embeds: Some(vec![embed]),
            allowed_mentions: None,
            components: None,
            files: None,
            payload_json: None,
            attachments: None,
            flags: None,
        };
        
        self.execute(&message).await
    }

    pub async fn execute_with_file(
        &self,
        content: Option<&str>,
        embed: Option<Embed>,
        file_data: &[u8],
        filename: &str,
    ) -> Result<(), DiscordError> {
        let mut form = reqwest::multipart::Form::new();
        
        // Add the file
        let file_part = reqwest::multipart::Part::bytes(file_data.to_vec())
            .file_name(filename.to_string());
        form = form.part("file", file_part);
        
        // Create the payload
        let message = WebhookMessage {
            content: content.map(|s| s.to_string()),
            username: None,
            avatar_url: None,
            tts: None,
            embeds: embed.map(|e| vec![e]),
            allowed_mentions: None,
            components: None,
            files: None,
            payload_json: None,
            attachments: None,
            flags: None,
        };
        
        // Add the payload as JSON
        let payload_json = serde_json::to_string(&message)?;
        form = form.text("payload_json", payload_json);
        
        let response = self.client
            .post(&self.webhook_url)
            .multipart(form)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(DiscordError::Http {
                status,
                message: error_text,
            });
        }

        Ok(())
    }
}

// Helper functions for creating embeds
impl Embed {
    pub fn new() -> Self {
        Self {
            title: None,
            description: None,
            url: None,
            timestamp: None,
            color: None,
            footer: None,
            image: None,
            thumbnail: None,
            video: None,
            provider: None,
            author: None,
            fields: None,
        }
    }

    pub fn title(mut self, title: &str) -> Self {
        self.title = Some(title.to_string());
        self
    }

    pub fn description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }

    pub fn color(mut self, color: u32) -> Self {
        self.color = Some(color);
        self
    }

    pub fn field(mut self, name: &str, value: &str, inline: bool) -> Self {
        let field = EmbedField {
            name: name.to_string(),
            value: value.to_string(),
            inline: Some(inline),
        };
        
        match &mut self.fields {
            Some(fields) => fields.push(field),
            None => self.fields = Some(vec![field]),
        }
        
        self
    }

    pub fn footer(mut self, text: &str, icon_url: Option<&str>) -> Self {
        self.footer = Some(EmbedFooter {
            text: text.to_string(),
            icon_url: icon_url.map(|s| s.to_string()),
            proxy_icon_url: None,
        });
        self
    }

    pub fn author(mut self, name: &str, url: Option<&str>, icon_url: Option<&str>) -> Self {
        self.author = Some(EmbedAuthor {
            name: name.to_string(),
            url: url.map(|s| s.to_string()),
            icon_url: icon_url.map(|s| s.to_string()),
            proxy_icon_url: None,
        });
        self
    }

    pub fn timestamp(mut self, timestamp: &str) -> Self {
        self.timestamp = Some(timestamp.to_string());
        self
    }

    pub fn image(mut self, url: &str) -> Self {
        self.image = Some(EmbedImage {
            url: url.to_string(),
            proxy_url: None,
            height: None,
            width: None,
        });
        self
    }

    pub fn thumbnail(mut self, url: &str) -> Self {
        self.thumbnail = Some(EmbedThumbnail {
            url: url.to_string(),
            proxy_url: None,
            height: None,
            width: None,
        });
        self
    }
}

// Color constants for common embed colors
pub mod colors {
    pub const RED: u32 = 0xFF0000;
    pub const GREEN: u32 = 0x00FF00;
    pub const BLUE: u32 = 0x0000FF;
    pub const YELLOW: u32 = 0xFFFF00;
    pub const PURPLE: u32 = 0x800080;
    pub const ORANGE: u32 = 0xFFA500;
    pub const CYAN: u32 = 0x00FFFF;
    pub const PINK: u32 = 0xFFC0CB;
    pub const WHITE: u32 = 0xFFFFFF;
    pub const BLACK: u32 = 0x000000;
    pub const GRAY: u32 = 0x808080;
}