use crate::api::SpaceCatClient;
use crate::discord::{DiscordWebhook, Embed, colors};
use crate::events::Event;
use crate::images::ImageMetadata;
use std::collections::HashSet;
use std::time::Duration;
use tokio::time::sleep;

pub struct DualPoller {
    client: SpaceCatClient,
    event_seen: HashSet<String>,
    image_seen: HashSet<String>,
    discord_webhook: Option<DiscordWebhook>,
}

impl DualPoller {
    pub fn new(client: SpaceCatClient) -> Self {
        Self {
            client,
            event_seen: HashSet::new(),
            image_seen: HashSet::new(),
            discord_webhook: None,
        }
    }

    pub fn with_discord_webhook(mut self, webhook_url: &str) -> Result<Self, Box<dyn std::error::Error>> {
        self.discord_webhook = Some(DiscordWebhook::new(webhook_url.to_string())?);
        Ok(self)
    }

    pub async fn start_polling(&mut self, poll_interval: Duration) {
        println!("Starting dual polling loop (events and images)...");
        println!("Polling interval: {:?}", poll_interval);
        println!("Press Ctrl+C to stop\n");

        // Initial fetch to establish baseline
        if let Err(e) = self.initialize_baseline().await {
            eprintln!("Failed to initialize baseline: {}", e);
            return;
        }

        loop {
            // Poll events
            match self.client.get_event_history().await {
                Ok(events) => {
                    for event in events.response {
                        // Skip filterwheel changed events where the filter didn't actually change
                        // This can happen when the filterwheel reports its position without actually moving
                        if event.event == "FILTERWHEEL-CHANGED" {
                            if let Some(crate::events::EventDetails::FilterWheelChange { ref new, ref previous }) = event.details {
                                if new.name == previous.name {
                                    continue; // Skip this redundant event
                                }
                            }
                        }
                        
                        let key = self.event_key(&event);
                        if self.event_seen.insert(key) {
                            self.print_new_event(&event);
                            if let Some(webhook) = &self.discord_webhook {
                                self.send_event_to_discord(webhook, &event).await;
                            }
                        }
                    }
                }
                Err(e) => eprintln!("Error fetching events: {}", e),
            }

            // Poll images
            match self.client.get_all_image_history().await {
                Ok(images) => {
                    for (index, image) in images.response.iter().enumerate() {
                        let key = self.image_key(&image);
                        if self.image_seen.insert(key) {
                            self.print_new_image(&image);
                            if let Some(webhook) = &self.discord_webhook {
                                self.send_image_to_discord(webhook, &image, index).await;
                            }
                        }
                    }
                }
                Err(e) => eprintln!("Error fetching images: {}", e),
            }

            sleep(poll_interval).await;
        }
    }

    async fn initialize_baseline(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Fetching initial baseline...");

        // Get initial events
        let events = self.client.get_event_history().await?;
        for event in events.response {
            // Skip filterwheel changed events where the filter didn't actually change
            // This can happen when the filterwheel reports its position without actually moving
            if event.event == "FILTERWHEEL-CHANGED" {
                if let Some(crate::events::EventDetails::FilterWheelChange { ref new, ref previous }) = event.details {
                    if new.name == previous.name {
                        continue; // Skip this redundant event
                    }
                }
            }
            self.event_seen.insert(self.event_key(&event));
        }
        println!("Baseline: {} events", self.event_seen.len());

        // Get initial images
        let images = self.client.get_all_image_history().await?;
        for image in images.response {
            self.image_seen.insert(self.image_key(&image));
        }
        println!("Baseline: {} images", self.image_seen.len());
        println!("Now monitoring for new events and images...\n");

        Ok(())
    }

    fn event_key(&self, event: &Event) -> String {
        format!(
            "{}|{}|{:?}",
            event.time,
            event.event,
            event.details
        )
    }

    fn image_key(&self, image: &ImageMetadata) -> String {
        format!("{}|{}", image.date, image.camera_name)
    }

    fn print_new_event(&self, event: &Event) {
        println!("[NEW EVENT] {}", event.time);
        println!("  Type: {}", event.event);
        if let Some(details) = &event.details {
            println!("  Details: {:?}", details);
        }
        println!();
    }

    fn print_new_image(&self, image: &ImageMetadata) {
        println!("[NEW IMAGE] {}", image.date);
        println!("  Camera: {}", image.camera_name);
        println!("  Type: {}", image.image_type);
        println!("  Filter: {}", image.filter);
        println!("  Exposure: {}s", image.exposure_time);
        println!("  Temperature: {:.1}°C", image.temperature);
        println!("  Stars: {}, HFR: {:.2}", image.stars, image.hfr);
        println!();
    }

    async fn send_event_to_discord(&self, webhook: &DiscordWebhook, event: &Event) {
        let color = match event.event.as_str() {
            "IMAGE-SAVE" => colors::GREEN,
            "FILTERWHEEL-CHANGED" => colors::BLUE,
            "SEQUENCE-START" => colors::CYAN,
            "SEQUENCE-STOP" => colors::ORANGE,
            "MOUNT-PARKED" => colors::YELLOW,
            _ if event.event.contains("ERROR") => colors::RED,
            _ if event.event.contains("WARNING") => colors::ORANGE,
            _ => colors::GRAY,
        };

        let mut embed = Embed::new()
            .title(&format!("📡 {}", event.event))
            .color(color)
            .field("Time", &event.time, false)
            .timestamp(&chrono::Utc::now().to_rfc3339());

        if let Some(details) = &event.details {
            embed = embed.field("Details", &format!("{:?}", details), false);
        }

        if let Err(e) = webhook.execute_with_embed(None, embed).await {
            eprintln!("Failed to send event to Discord: {}", e);
        }
    }

    async fn send_image_to_discord(&self, webhook: &DiscordWebhook, image: &ImageMetadata, index: usize) {
        let color = match image.image_type.as_str() {
            "LIGHT" => colors::GREEN,
            "DARK" => colors::GRAY,
            "FLAT" => colors::BLUE,
            "BIAS" => colors::PURPLE,
            _ => colors::CYAN,
        };

        let embed = Embed::new()
            .title(&format!("📸 New {} Frame Captured", image.image_type))
            .color(color)
            .field("Camera", &image.camera_name, true)
            .field("Filter", &image.filter, true)
            .field("Exposure", &format!("{}s", image.exposure_time), true)
            .field("Temperature", &format!("{:.1}°C", image.temperature), true)
            .field("Stars", &image.stars.to_string(), true)
            .field("HFR", &format!("{:.2}", image.hfr), true)
            .field("Mean", &format!("{:.1}", image.mean), true)
            .field("Median", &format!("{:.1}", image.median), true)
            .field("StDev", &format!("{:.1}", image.st_dev), true)
            .footer(&format!("Telescope: {}", image.telescope_name), None)
            .timestamp(&chrono::Utc::now().to_rfc3339());

        // Try to download and attach the thumbnail
        match self.client.get_thumbnail(index as u32).await {
            Ok(thumbnail_data) => {
                let filename = format!("thumbnail_{}_{}.jpg", image.filter, image.date.replace(':', "-").replace(' ', "_"));
                if let Err(e) = webhook.execute_with_file(None, Some(embed), &thumbnail_data.data, &filename).await {
                    eprintln!("Failed to send image with thumbnail to Discord: {}", e);
                }
            }
            Err(e) => {
                eprintln!("Failed to download thumbnail for image {}: {}", index, e);
                // Send without thumbnail
                if let Err(e) = webhook.execute_with_embed(None, embed).await {
                    eprintln!("Failed to send image to Discord: {}", e);
                }
            }
        }
    }
}