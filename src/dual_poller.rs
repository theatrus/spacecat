use crate::api::SpaceCatClient;
use crate::autofocus::AutofocusResponse;
use crate::discord::{DiscordWebhook, Embed, colors};
use crate::events::{Event, event_types};
use crate::images::ImageMetadata;
use crate::mount::MountInfoResponse;
use crate::sequence::{
    SequenceResponse, extract_current_target, extract_meridian_flip_time,
    meridian_flip_time_formatted_with_clock,
};
use std::collections::HashSet;
use std::time::{Duration, Instant};
use tokio::time::sleep;

pub struct DualPoller {
    client: SpaceCatClient,
    event_seen: HashSet<String>,
    image_seen: HashSet<String>,
    discord_webhook: Option<DiscordWebhook>,
    current_target: Option<String>,
    current_meridian_flip_time: Option<f64>,
    current_sequence: Option<SequenceResponse>,
    last_discord_image_time: Option<Instant>,
    discord_image_cooldown: Duration,
    skipped_images_count: u32,
}

impl DualPoller {
    pub fn new(client: SpaceCatClient) -> Self {
        Self {
            client,
            event_seen: HashSet::new(),
            image_seen: HashSet::new(),
            discord_webhook: None,
            current_target: None,
            current_meridian_flip_time: None,
            current_sequence: None,
            last_discord_image_time: None,
            discord_image_cooldown: Duration::from_secs(60), // Default 60 seconds
            skipped_images_count: 0,
        }
    }

    pub fn with_discord_webhook(
        mut self,
        webhook_url: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        self.discord_webhook = Some(DiscordWebhook::new(webhook_url.to_string())?);
        Ok(self)
    }

    pub fn with_discord_image_cooldown(mut self, cooldown_seconds: u64) -> Self {
        self.discord_image_cooldown = Duration::from_secs(cooldown_seconds);
        self
    }

    pub async fn start_polling(&mut self, poll_interval: Duration) {
        println!("Starting dual polling loop (events and images)...");
        println!("Polling interval: {poll_interval:?}");
        println!("Press Ctrl+C to stop\n");

        // Initial fetch to establish baseline
        if let Err(e) = self.initialize_baseline().await {
            eprintln!("Failed to initialize baseline: {e}");
            return;
        }

        loop {
            // Poll sequence for target information
            self.poll_sequence().await;

            // Poll events
            match self.client.get_event_history().await {
                Ok(events) => {
                    for event in events.response {
                        // Skip filterwheel changed events where the filter didn't actually change
                        // This can happen when the filterwheel reports its position without actually moving
                        if event.event == event_types::FILTERWHEEL_CHANGED
                            && let Some(crate::events::EventDetails::FilterWheelChange {
                                ref new,
                                ref previous,
                            }) = event.details
                            && new.name == previous.name
                        {
                            continue; // Skip this redundant event
                        }

                        let key = self.event_key(&event);
                        if self.event_seen.insert(key) {
                            self.print_new_event(&event);

                            // Handle autofocus-finished events
                            if event.event == event_types::AUTOFOCUS_FINISHED {
                                self.handle_autofocus_finished(&event).await;
                            }

                            // Handle mount events with enhanced info
                            if event.event == event_types::MOUNT_BEFORE_FLIP
                                || event.event == event_types::MOUNT_AFTER_FLIP
                                || event.event == event_types::MOUNT_PARKED
                            {
                                if let Some(webhook) = &self.discord_webhook {
                                    self.send_mount_event_to_discord(webhook, &event).await;
                                }
                            } else if let Some(webhook) = &self.discord_webhook {
                                self.send_event_to_discord(webhook, &event).await;
                            }
                        }
                    }
                }
                Err(e) => eprintln!("Error fetching events: {e}"),
            }

            // Poll images
            match self.client.get_all_image_history().await {
                Ok(images) => {
                    for (index, image) in images.response.iter().enumerate() {
                        let key = self.image_key(image);
                        if self.image_seen.insert(key) {
                            self.print_new_image(image);
                            if let Some(webhook) = &self.discord_webhook {
                                // Check if we should send this image to Discord based on cooldown
                                let should_send = match self.last_discord_image_time {
                                    None => true, // First image, always send
                                    Some(last_time) => {
                                        last_time.elapsed() >= self.discord_image_cooldown
                                    }
                                };

                                if should_send {
                                    self.send_image_to_discord(
                                        webhook,
                                        image,
                                        index,
                                        self.skipped_images_count,
                                    )
                                    .await;
                                    self.last_discord_image_time = Some(Instant::now());
                                    // Reset skipped count after sending
                                    if self.skipped_images_count > 0 {
                                        println!(
                                            "  Sent image to Discord (including {} skipped images)",
                                            self.skipped_images_count
                                        );
                                    }
                                    self.skipped_images_count = 0;
                                } else {
                                    self.skipped_images_count += 1;
                                    let remaining = self.discord_image_cooldown
                                        - self.last_discord_image_time.unwrap().elapsed();
                                    println!(
                                        "  Skipping Discord notification (cooldown: {:.0}s remaining)",
                                        remaining.as_secs_f32()
                                    );
                                }
                            }
                        }
                    }
                }
                Err(e) => eprintln!("Error fetching images: {e}"),
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
            if event.event == event_types::FILTERWHEEL_CHANGED
                && let Some(crate::events::EventDetails::FilterWheelChange {
                    ref new,
                    ref previous,
                }) = event.details
                && new.name == previous.name
            {
                continue; // Skip this redundant event
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
        format!("{}|{}|{:?}", event.time, event.event, event.details)
    }

    fn image_key(&self, image: &ImageMetadata) -> String {
        format!("{}|{}", image.date, image.camera_name)
    }

    fn print_new_event(&self, event: &Event) {
        println!("[NEW EVENT] {}", event.time);
        println!("  Type: {}", event.event);
        if let Some(details) = &event.details {
            println!("  Details: {details:?}");
        }
        println!();
    }

    fn print_new_image(&self, image: &ImageMetadata) {
        println!("[NEW IMAGE] {}", image.date);
        if let Some(target) = &self.current_target {
            println!("  Target: {target}");
        }
        if let Some(meridian_flip_hours) = self.current_meridian_flip_time {
            let formatted_time = meridian_flip_time_formatted_with_clock(meridian_flip_hours);
            println!("  Meridian flip in: {formatted_time}");
        }
        println!("  Camera: {}", image.camera_name);
        println!("  Type: {}", image.image_type);
        println!("  Filter: {}", image.filter);
        println!("  Exposure: {}s", image.exposure_time);
        println!("  Temperature: {:.1}°C", image.temperature);
        println!("  Stars: {}, HFR: {:.2}", image.stars, image.hfr);
        println!("  RMS: {}", image.rms_text);
        println!(
            "  Mean: {:.1}, Median: {:.1}, StDev: {:.1}",
            image.mean, image.median, image.st_dev
        );
        println!("  Telescope: {}", image.telescope_name);

        println!();
    }

    async fn send_event_to_discord(&self, webhook: &DiscordWebhook, event: &Event) {
        if event.event == event_types::IMAGE_SAVE {
            // Skip IMAGE-SAVE events, they will be handled in the image section
            return;
        }

        let color = match event.event.as_str() {
            // Camera events - Green tones for positive, Red for negative
            event_types::CAMERA_CONNECTED => colors::GREEN,
            event_types::CAMERA_DISCONNECTED => colors::RED,
            event_types::IMAGE_SAVE => colors::GREEN,

            // Filterwheel events - Blue tones
            event_types::FILTERWHEEL_CONNECTED => colors::BLUE,
            event_types::FILTERWHEEL_DISCONNECTED => colors::RED,
            event_types::FILTERWHEEL_CHANGED => colors::BLUE,

            // Mount events - Yellow/Orange tones for operations, Green for completion
            event_types::MOUNT_CONNECTED => colors::GREEN,
            event_types::MOUNT_DISCONNECTED => colors::RED,
            event_types::MOUNT_PARKED => colors::YELLOW,
            event_types::MOUNT_UNPARKED => colors::YELLOW,
            event_types::MOUNT_SLEW => colors::ORANGE,
            event_types::MOUNT_BEFORE_FLIP => colors::ORANGE,
            event_types::MOUNT_AFTER_FLIP => colors::GREEN,

            // Focuser events - Purple tones
            event_types::FOCUSER_CONNECTED => colors::GREEN,
            event_types::FOCUSER_DISCONNECTED => colors::RED,
            event_types::FOCUS_START => colors::PURPLE,
            event_types::FOCUS_END => colors::PURPLE,
            event_types::AUTOFOCUS_FINISHED => colors::PURPLE,

            // Rotator events - Cyan tones
            event_types::ROTATOR_CONNECTED => colors::GREEN,
            event_types::ROTATOR_DISCONNECTED => colors::RED,
            event_types::ROTATOR_MOVED => colors::CYAN,
            event_types::ROTATOR_SYNCED => colors::CYAN,

            // Guider events - Blue/Cyan tones
            event_types::GUIDER_CONNECTED => colors::GREEN,
            event_types::GUIDER_DISCONNECTED => colors::RED,
            event_types::GUIDER_START => colors::BLUE,
            event_types::GUIDER_DITHER => colors::CYAN,

            // Sequence events - Bright colors for visibility
            event_types::SEQUENCE_START => colors::CYAN,
            event_types::SEQUENCE_STOP => colors::ORANGE,
            event_types::SEQUENCE_PAUSE => colors::YELLOW,
            event_types::SEQUENCE_RESUME => colors::CYAN,
            event_types::SEQUENCE_FINISHED => colors::GREEN,
            event_types::ADV_SEQ_STOP => colors::ORANGE,

            // Exposure events - Star-like colors
            event_types::EXPOSURE_START => colors::YELLOW,
            event_types::EXPOSURE_END => colors::GREEN,

            // System/Safety events - Red for disconnections
            event_types::FLAT_DISCONNECTED => colors::RED,
            event_types::WEATHER_DISCONNECTED => colors::RED,
            event_types::SWITCH_DISCONNECTED => colors::RED,
            event_types::DOME_DISCONNECTED => colors::RED,
            event_types::SAFETY_DISCONNECTED => colors::RED,

            // Fallback patterns
            _ if event.event.contains("ERROR") => colors::RED,
            _ if event.event.contains("WARNING") => colors::ORANGE,
            _ => colors::GRAY,
        };

        let title = match event.event.as_str() {
            event_types::FILTERWHEEL_CHANGED => "🔄 Filter Changed".to_string(),
            _ => format!("📡 {}", event.event),
        };

        let mut embed = Embed::new()
            .title(&title)
            .color(color)
            .field("Time", &event.time, false)
            .timestamp(&chrono::Utc::now().to_rfc3339());

        if let Some(details) = &event.details {
            match details {
                crate::events::EventDetails::FilterWheelChange { new, previous } => {
                    embed = embed
                        .field(
                            "Filter Change",
                            &format!("{} → {}", previous.name, new.name),
                            false,
                        )
                        .field(
                            "Previous",
                            &format!("{} (ID: {})", previous.name, previous.id),
                            true,
                        )
                        .field("New", &format!("{} (ID: {})", new.name, new.id), true);
                }
            }
        }

        if let Err(e) = webhook.execute_with_embed(None, embed).await {
            eprintln!("Failed to send event to Discord: {e}");
        }
    }

    async fn send_image_to_discord(
        &self,
        webhook: &DiscordWebhook,
        image: &ImageMetadata,
        index: usize,
        skipped_count: u32,
    ) {
        let color = match image.image_type.as_str() {
            "LIGHT" => colors::GREEN,
            "DARK" => colors::GRAY,
            "FLAT" => colors::BLUE,
            "BIAS" => colors::PURPLE,
            _ => colors::CYAN,
        };

        let title = if skipped_count > 0 {
            format!(
                "📸 New {} Frame Captured (+{} skipped)",
                image.image_type, skipped_count
            )
        } else {
            format!("📸 New {} Frame Captured", image.image_type)
        };

        let mut embed = Embed::new().title(&title).color(color);

        // Add target information if available
        if let Some(target) = &self.current_target {
            embed = embed.field("Target", target, true);
        }

        // Add skipped images summary if any
        if skipped_count > 0 {
            embed = embed.field(
                "Images Since Last Post",
                &format!("{} images", skipped_count + 1),
                true,
            );
        }

        // Add meridian flip time if available and within the next hour
        if let Some(meridian_flip_hours) = self.current_meridian_flip_time
            && meridian_flip_hours <= 1.0
        {
            let formatted_time = meridian_flip_time_formatted_with_clock(meridian_flip_hours);
            embed = embed.field("Meridian Flip In", &formatted_time, true);
        }

        embed = embed
            .field("Camera", &image.camera_name, true)
            .field("Tracking RMS", &image.rms_text, true)
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
                let filename = format!(
                    "thumbnail_{}_{}.jpg",
                    image.filter,
                    image.date.replace(':', "-").replace(' ', "_")
                );
                if let Err(e) = webhook
                    .execute_with_file(None, Some(embed), &thumbnail_data.data, &filename)
                    .await
                {
                    eprintln!("Failed to send image with thumbnail to Discord: {e}");
                }
            }
            Err(e) => {
                eprintln!("Failed to download thumbnail for image {index}: {e}");
                // Send without thumbnail
                if let Err(e) = webhook.execute_with_embed(None, embed).await {
                    eprintln!("Failed to send image to Discord: {e}");
                }
            }
        }
    }

    async fn poll_sequence(&mut self) {
        match self.client.get_sequence().await {
            Ok(sequence) => {
                let new_target = extract_current_target(&sequence);
                let new_meridian_flip_time = extract_meridian_flip_time(&sequence);

                // Check if target changed
                if new_target != self.current_target {
                    if let (Some(old_target), Some(new_target_name)) =
                        (&self.current_target, &new_target)
                    {
                        println!("[TARGET CHANGE] {old_target} -> {new_target_name}");
                        if let Some(meridian_flip_hours) = new_meridian_flip_time {
                            let formatted_time =
                                meridian_flip_time_formatted_with_clock(meridian_flip_hours);
                            println!("  Meridian flip in: {formatted_time}");
                        }
                        if let Some(webhook) = &self.discord_webhook {
                            self.send_target_change_to_discord(
                                webhook,
                                old_target,
                                new_target_name,
                                new_meridian_flip_time,
                            )
                            .await;
                        }
                    } else if let Some(new_target_name) = &new_target {
                        println!("[TARGET START] {new_target_name}");
                        if let Some(meridian_flip_hours) = new_meridian_flip_time {
                            let formatted_time =
                                meridian_flip_time_formatted_with_clock(meridian_flip_hours);
                            println!("  Meridian flip in: {formatted_time}");
                        }
                        if let Some(webhook) = &self.discord_webhook {
                            self.send_target_start_to_discord(
                                webhook,
                                new_target_name,
                                new_meridian_flip_time,
                            )
                            .await;
                        }
                    }

                    self.current_target = new_target;
                    self.current_meridian_flip_time = new_meridian_flip_time;
                } else {
                    // Even if target didn't change, update meridian flip time
                    self.current_meridian_flip_time = new_meridian_flip_time;
                }

                self.current_sequence = Some(sequence);
            }
            Err(e) => {
                // Only log sequence errors occasionally to avoid spam
                if self.current_sequence.is_none() {
                    eprintln!("Error fetching sequence (will retry silently): {e}");
                }
            }
        }
    }

    async fn send_target_change_to_discord(
        &self,
        webhook: &DiscordWebhook,
        old_target: &str,
        new_target: &str,
        meridian_flip_time: Option<f64>,
    ) {
        let mut embed = Embed::new()
            .title("🎯 Target Change")
            .color(colors::CYAN)
            .field("Previous Target", old_target, true)
            .field("New Target", new_target, true);

        if let Some(meridian_flip_hours) = meridian_flip_time {
            let formatted_time = meridian_flip_time_formatted_with_clock(meridian_flip_hours);
            embed = embed.field("Meridian Flip In", &formatted_time, true);
        }

        // Try to fetch mount info
        if let Ok(mount_info) = self.client.get_mount_info().await {
            embed = self.add_mount_info_to_embed(embed, &mount_info);
        }

        let embed = embed.timestamp(&chrono::Utc::now().to_rfc3339());

        if let Err(e) = webhook.execute_with_embed(None, embed).await {
            eprintln!("Failed to send target change to Discord: {e}");
        }
    }

    async fn send_target_start_to_discord(
        &self,
        webhook: &DiscordWebhook,
        target: &str,
        meridian_flip_time: Option<f64>,
    ) {
        let mut embed = Embed::new()
            .title("🎯 Target Started")
            .color(colors::GREEN)
            .field("Target", target, false);

        if let Some(meridian_flip_hours) = meridian_flip_time {
            let formatted_time = meridian_flip_time_formatted_with_clock(meridian_flip_hours);
            embed = embed.field("Meridian Flip In", &formatted_time, true);
        }

        // Try to fetch mount info
        if let Ok(mount_info) = self.client.get_mount_info().await {
            embed = self.add_mount_info_to_embed(embed, &mount_info);
        }

        let embed = embed.timestamp(&chrono::Utc::now().to_rfc3339());

        if let Err(e) = webhook.execute_with_embed(None, embed).await {
            eprintln!("Failed to send target start to Discord: {e}");
        }
    }

    async fn send_mount_event_to_discord(&self, webhook: &DiscordWebhook, event: &Event) {
        let (title, color) = match event.event.as_str() {
            event_types::MOUNT_BEFORE_FLIP => {
                ("🔄 Mount Preparing for Meridian Flip", colors::ORANGE)
            }
            event_types::MOUNT_AFTER_FLIP => ("✅ Mount Meridian Flip Completed", colors::GREEN),
            event_types::MOUNT_PARKED => ("🅿️ Mount Parked", colors::YELLOW),
            _ => ("🔭 Mount Event", colors::GRAY),
        };

        let mut embed = Embed::new()
            .title(title)
            .color(color)
            .field("Event", &event.event, true)
            .field("Time", &event.time, true);

        // Add current target if available
        if let Some(target) = &self.current_target {
            embed = embed.field("Current Target", target, true);
        }

        // Try to fetch mount info for detailed position data
        if let Ok(mount_info) = self.client.get_mount_info().await
            && mount_info.is_connected()
        {
            let (ra, dec) = mount_info.get_coordinates();
            let (alt, az) = mount_info.get_alt_az();

            embed = embed
                .field("Mount Position", &format!("RA: {ra}\nDec: {dec}"), true)
                .field("Alt/Az", &format!("Alt: {alt}\nAz: {az}"), true)
                .field("Pier Side", mount_info.get_side_of_pier(), true)
                .field(
                    "Sidereal Time",
                    &mount_info.response.sidereal_time_string,
                    true,
                );

            // For after-flip, show the new meridian flip time
            if event.event == event_types::MOUNT_AFTER_FLIP {
                let flip_time = mount_info.get_time_to_meridian_flip_hours();
                let formatted_time = meridian_flip_time_formatted_with_clock(flip_time);
                embed = embed.field("Next Meridian Flip", &formatted_time, true);
            }

            // For parked event, show park details
            if event.event == event_types::MOUNT_PARKED {
                let (lat, lon, elev) = mount_info.get_site_info();
                embed = embed.field(
                    "Site Location",
                    &format!("Lat: {lat:.3}°\nLon: {lon:.3}°\nElev: {elev}m"),
                    true,
                );
            }

            if mount_info.response.tracking_enabled {
                embed = embed.field("Tracking Status", "✅ Enabled", true);
            } else {
                embed = embed.field("Tracking Status", "❌ Disabled", true);
            }

            // Add slewing status if relevant
            if mount_info.response.slewing {
                embed = embed.field("Mount Status", "🔄 Slewing", true);
            } else if mount_info.response.at_park {
                embed = embed.field("Mount Status", "🅿️ Parked", true);
            } else {
                embed = embed.field("Mount Status", "✅ Tracking", true);
            }
        }

        let embed = embed.timestamp(&chrono::Utc::now().to_rfc3339());

        if let Err(e) = webhook.execute_with_embed(None, embed).await {
            eprintln!("Failed to send mount event to Discord: {e}");
        }
    }

    async fn handle_autofocus_finished(&self, event: &Event) {
        println!("[AUTOFOCUS FINISHED] {}", event.time);
        println!("Fetching autofocus results...");

        match self.client.get_last_autofocus().await {
            Ok(autofocus_data) => {
                self.display_autofocus_results(&autofocus_data);

                // Send to Discord if configured
                if let Some(webhook) = &self.discord_webhook {
                    self.send_autofocus_to_discord(webhook, &autofocus_data)
                        .await;
                }
            }
            Err(e) => {
                eprintln!("Failed to fetch autofocus data: {e}");
            }
        }
        println!();
    }

    fn display_autofocus_results(&self, af: &AutofocusResponse) {
        if !af.success {
            println!("❌ Autofocus failed: {}", af.error);
            return;
        }

        let af_data = &af.response;
        let success_indicator = if af.is_successful() { "✅" } else { "⚠️" };

        println!("{success_indicator} Autofocus Summary");
        println!("  Filter: {}", af_data.filter);
        println!("  Method: {}", af_data.method);
        println!("  Temperature: {:.1}°C", af_data.temperature);
        println!("  Duration: {}", af_data.duration);

        println!("Focus Results:");
        println!(
            "  Initial Position: {}",
            af_data.initial_focus_point.position
        );
        println!(
            "  Calculated Position: {} (HFR: {:.3})",
            af_data.calculated_focus_point.position, af_data.calculated_focus_point.value
        );
        println!(
            "  Position Change: {}",
            af_data.calculated_focus_point.position - af_data.initial_focus_point.position
        );

        println!("Quality Metrics:");
        println!("  Measurement Points: {}", af_data.measure_points.len());
        println!("  Best R-squared: {:.4}", af.get_best_r_squared());

        let (min_pos, max_pos) = af_data.get_focus_range();
        println!("  Focus Range: {min_pos} - {max_pos}");

        if let Some(best_hfr) = af_data.get_best_measured_hfr() {
            println!("  Best Measured HFR: {best_hfr:.3}");
        }
    }

    async fn send_autofocus_to_discord(&self, webhook: &DiscordWebhook, af: &AutofocusResponse) {
        let color = if af.is_successful() {
            colors::GREEN
        } else {
            colors::ORANGE
        };

        let af_data = &af.response;
        let success_indicator = if af.is_successful() { "✅" } else { "⚠️" };

        let position_change =
            af_data.calculated_focus_point.position - af_data.initial_focus_point.position;
        let position_change_text = if position_change > 0 {
            format!("+{position_change}")
        } else {
            position_change.to_string()
        };

        let embed = Embed::new()
            .title(&format!("{success_indicator} Autofocus Completed"))
            .color(color)
            .field("Filter", &af_data.filter, true)
            .field("Method", &af_data.method, true)
            .field("Duration", &af_data.duration, true)
            .field(
                "Temperature",
                &format!("{:.1}°C", af_data.temperature),
                true,
            )
            .field(
                "Focus Position",
                &af_data.calculated_focus_point.position.to_string(),
                true,
            )
            .field("Position Change", &position_change_text, true)
            .field(
                "HFR",
                &format!("{:.3}", af_data.calculated_focus_point.value),
                true,
            )
            .field(
                "R-squared",
                &format!("{:.4}", af.get_best_r_squared()),
                true,
            )
            .field(
                "Measurements",
                &af_data.measure_points.len().to_string(),
                true,
            )
            .footer(&format!("Focuser: {}", af_data.auto_focuser_name), None)
            .timestamp(&chrono::Utc::now().to_rfc3339());

        if let Err(e) = webhook.execute_with_embed(None, embed).await {
            eprintln!("Failed to send autofocus results to Discord: {e}");
        }
    }

    // Helper method to add mount info fields to an embed
    fn add_mount_info_to_embed(&self, mut embed: Embed, mount_info: &MountInfoResponse) -> Embed {
        if mount_info.is_connected() {
            let (ra, dec) = mount_info.get_coordinates();
            let (alt, az) = mount_info.get_alt_az();

            embed = embed
                .field("Mount Position", &format!("RA: {ra}\nDec: {dec}"), true)
                .field("Alt/Az", &format!("Alt: {alt}\nAz: {az}"), true)
                .field("Pier Side", mount_info.get_side_of_pier(), true);

            if mount_info.response.tracking_enabled {
                embed = embed.field("Tracking", "✅ Enabled", true);
            } else {
                embed = embed.field("Tracking", "❌ Disabled", true);
            }
        }
        embed
    }
}
