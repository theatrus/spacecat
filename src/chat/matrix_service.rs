use super::{ChatMessage, ChatService};
use async_trait::async_trait;
use matrix_sdk::{
    Client, EncryptionState, Room,
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

        println!("Successfully logged into Matrix as {}", username);

        // Initial sync to get room states
        println!("Syncing with Matrix server...");
        client.sync_once(SyncSettings::default()).await?;

        // Check for invited rooms and join them
        let invited_rooms = client.invited_rooms();
        if !invited_rooms.is_empty() {
            println!("Found {} room invitation(s):", invited_rooms.len());
            for room in &invited_rooms {
                let room_name = room.name().unwrap_or_else(|| room.room_id().to_string());
                println!("  - Joining room: {} ({})", room_name, room.room_id());

                match room.join().await {
                    Ok(_) => println!("    ✅ Successfully joined"),
                    Err(e) => println!("    ❌ Failed to join: {}", e),
                }
            }

            // Sync again to update room states after joining
            client.sync_once(SyncSettings::default()).await?;
        } else {
            println!("No pending room invitations");
        }

        // List all joined rooms
        let joined_rooms = client.joined_rooms();
        println!("Currently joined to {} room(s):", joined_rooms.len());
        for room in &joined_rooms {
            let room_name = room.name().unwrap_or("Unnamed room".to_string());
            let member_count = room.active_members_count();
            let encryption_state = room.encryption_state();
            let encryption_status = match encryption_state {
                EncryptionState::Encrypted => "🔒",
                _ => "🔓",
            };

            println!(
                "  - {} {} ({}) - {} members",
                encryption_status,
                room_name,
                room.room_id(),
                member_count
            );
        }

        // Start background sync
        tokio::spawn({
            let client = client.clone();
            async move {
                if let Err(e) = client.sync(SyncSettings::default()).await {
                    eprintln!("Matrix sync error: {}", e);
                }
            }
        });

        let room_id: OwnedRoomId = room_id.try_into()?;

        // Verify the target room is accessible
        if let Some(target_room) = client.get_room(&room_id) {
            let room_name = target_room.name().unwrap_or_else(|| room_id.to_string());
            println!("✅ Target room found: {} ({})", room_name, room_id);
        } else {
            println!(
                "⚠️  Warning: Target room {} not found in joined rooms",
                room_id
            );
            println!("   Make sure the bot is invited to this room or check the room ID");
        }

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
