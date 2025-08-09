use crate::api::SpaceCatClient;
use crate::events::Event;
use crate::images::ImageMetadata;
use std::collections::HashSet;
use std::time::Duration;
use tokio::time::sleep;

pub struct DualPoller {
    client: SpaceCatClient,
    event_seen: HashSet<String>,
    image_seen: HashSet<String>,
}

impl DualPoller {
    pub fn new(client: SpaceCatClient) -> Self {
        Self {
            client,
            event_seen: HashSet::new(),
            image_seen: HashSet::new(),
        }
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
                        let key = self.event_key(&event);
                        if self.event_seen.insert(key) {
                            self.print_new_event(&event);
                        }
                    }
                }
                Err(e) => eprintln!("Error fetching events: {}", e),
            }

            // Poll images
            match self.client.get_all_image_history().await {
                Ok(images) => {
                    for image in images.response {
                        let key = self.image_key(&image);
                        if self.image_seen.insert(key) {
                            self.print_new_image(&image);
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
}