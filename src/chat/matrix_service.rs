use super::{ChatMessage, ChatService};
use async_trait::async_trait;
use matrix_sdk::{
    Client, Room,
    config::SyncSettings,
    ruma::{OwnedRoomId, events::room::message::RoomMessageEventContent},
};
use std::error::Error;
use url::Url;

pub struct MatrixChatService {
    client: Client,
    room_id: OwnedRoomId,
}

impl MatrixChatService {
    pub async fn new(
        homeserver_url: &str,
        username: &str,
        password: &str,
        room_id: &str,
    ) -> Result<Self, Box<dyn Error>> {
        let homeserver_url = Url::parse(homeserver_url)?;
        let client = Client::new(homeserver_url).await?;

        // Login to Matrix
        client
            .matrix_auth()
            .login_username(username, password)
            .initial_device_display_name("SpaceCat")
            .await?;

        // Start sync in the background
        tokio::spawn({
            let client = client.clone();
            async move { client.sync(SyncSettings::default()).await }
        });

        let room_id: OwnedRoomId = room_id.try_into()?;

        Ok(Self { client, room_id })
    }

    fn format_message(&self, message: &ChatMessage) -> String {
        let mut formatted = format!("**{}**\n\n", message.title);

        if !message.fields.is_empty() {
            for field in &message.fields {
                formatted.push_str(&format!("**{}**: {}\n", field.name, field.value));
            }
            formatted.push('\n');
        }

        if let Some(footer) = &message.footer {
            formatted.push_str(&format!("_{}_", footer));
        }

        formatted
    }

    async fn get_room(&self) -> Result<Room, Box<dyn Error + Send + Sync>> {
        self.client
            .get_room(&self.room_id)
            .ok_or_else(|| format!("Room {} not found", self.room_id).into())
    }
}

#[async_trait]
impl ChatService for MatrixChatService {
    async fn send_message(
        &self,
        message: &ChatMessage,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let room = self.get_room().await?;
        let formatted_message = self.format_message(message);

        let content =
            RoomMessageEventContent::text_html(formatted_message.clone(), formatted_message);
        room.send(content).await?;

        Ok(())
    }

    async fn send_message_with_image(
        &self,
        message: &ChatMessage,
        image_data: &[u8],
        filename: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let room = self.get_room().await?;

        // Send the text message first
        let formatted_message = self.format_message(message);
        let content =
            RoomMessageEventContent::text_html(formatted_message.clone(), formatted_message);
        room.send(content).await?;

        // Then send the image
        let mime_type = if filename.ends_with(".jpg") || filename.ends_with(".jpeg") {
            "image/jpeg"
        } else if filename.ends_with(".png") {
            "image/png"
        } else {
            "image/jpeg" // Default fallback
        };

        let mime = mime_type.parse::<mime::Mime>()?;
        room.send_attachment(filename, &mime, image_data.to_vec(), Default::default())
            .await?;

        Ok(())
    }

    fn service_name(&self) -> &'static str {
        "Matrix"
    }
}
