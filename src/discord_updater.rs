use crate::api::SpaceCatApiClient;
use crate::autofocus::AutofocusResponse;
use crate::discord::{DiscordWebhook, Embed, colors};
use crate::events::{Event, EventDetails, TargetCoordinates, event_types};
use crate::images::ImageMetadata;
use crate::sequence::{
    SequenceResponse, extract_current_target, extract_meridian_flip_time,
    meridian_flip_time_formatted_with_clock,
};
use std::collections::HashSet;
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// Information about the current observation target
#[derive(Debug, Clone)]
struct TargetInfo {
    name: String,
    source: TargetSource,
    coordinates: Option<TargetCoordinates>,
    project: Option<String>,
    rotation: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
enum TargetSource {
    Sequence,
    TsTargetStart,
}

/// State management for the Discord updater
struct UpdaterState {
    events_seen: HashSet<String>,
    images_seen: HashSet<String>,
    current_target: Option<TargetInfo>,
    meridian_flip_time: Option<f64>,
    sequence: Option<SequenceResponse>,
    last_discord_image_time: Option<Instant>,
    skipped_images_count: u32,
}

impl UpdaterState {
    fn new() -> Self {
        Self {
            events_seen: HashSet::new(),
            images_seen: HashSet::new(),
            current_target: None,
            meridian_flip_time: None,
            sequence: None,
            last_discord_image_time: None,
            skipped_images_count: 0,
        }
    }

    fn event_key(event: &Event) -> String {
        format!("{}|{}|{:?}", event.time, event.event, event.details)
    }

    fn image_key(image: &ImageMetadata) -> String {
        format!("{}|{}", image.date, image.camera_name)
    }

    fn has_seen_event(&mut self, event: &Event) -> bool {
        !self.events_seen.insert(Self::event_key(event))
    }

    fn has_seen_image(&mut self, image: &ImageMetadata) -> bool {
        !self.images_seen.insert(Self::image_key(image))
    }
}

/// Simplified Discord updater
pub struct DiscordUpdater {
    client: SpaceCatApiClient,
    state: UpdaterState,
    discord_webhook: Option<DiscordWebhook>,
    discord_image_cooldown: Duration,
}

impl DiscordUpdater {
    pub fn new(client: SpaceCatApiClient) -> Self {
        Self {
            client,
            state: UpdaterState::new(),
            discord_webhook: None,
            discord_image_cooldown: Duration::from_secs(60),
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
        println!("Starting Discord updater loop (events and images)...");
        println!("Polling interval: {poll_interval:?}");
        println!("Press Ctrl+C to stop\n");

        if let Err(e) = self.initialize_baseline().await {
            eprintln!("Failed to initialize baseline: {e}");
            return;
        }

        loop {
            self.poll_sequence().await;
            self.poll_events().await;
            self.poll_images().await;
            sleep(poll_interval).await;
        }
    }

    async fn initialize_baseline(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Fetching initial baseline...");

        // Load events and find latest TS-TARGETSTART
        let events = self.client.get_event_history().await?;
        self.process_baseline_events(&events.response);

        // Load images
        let images = self.client.get_all_image_history().await?;
        for image in &images.response {
            self.state
                .images_seen
                .insert(UpdaterState::image_key(image));
        }

        println!(
            "Baseline: {} events, {} images",
            self.state.events_seen.len(),
            self.state.images_seen.len()
        );

        if let Some(target) = &self.state.current_target {
            println!("Current target: {} (from {:?})", target.name, target.source);
        }

        println!("Now monitoring for new events and images...\n");
        Ok(())
    }

    fn process_baseline_events(&mut self, events: &[Event]) {
        let mut latest_ts_target: Option<(String, TargetInfo)> = None;

        for event in events {
            // Skip redundant filterwheel events
            if event.event == event_types::FILTERWHEEL_CHANGED
                && let Some(EventDetails::FilterWheelChange { new, previous }) = &event.details
                && new.name == previous.name
            {
                continue;
            }

            // Track TS-TARGETSTART events
            if event.event == event_types::TS_TARGETSTART
                && let Some(EventDetails::TargetStart {
                    target_name,
                    coordinates,
                    project_name,
                    rotation,
                    ..
                }) = &event.details
                && target_name != "Sequential Instruction Set"
            {
                let target_info = TargetInfo {
                    name: target_name.clone(),
                    source: TargetSource::TsTargetStart,
                    coordinates: Some(coordinates.clone()),
                    project: Some(project_name.clone()),
                    rotation: Some(*rotation),
                };

                if latest_ts_target.is_none()
                    || latest_ts_target
                        .as_ref()
                        .map(|(time, _)| time < &event.time)
                        .unwrap_or(false)
                {
                    latest_ts_target = Some((event.time.clone(), target_info));
                }
            }

            self.state
                .events_seen
                .insert(UpdaterState::event_key(event));
        }

        // Set the latest TS target if found
        if let Some((_, target)) = latest_ts_target {
            self.state.current_target = Some(target);
        }
    }

    async fn poll_events(&mut self) {
        match self.client.get_event_history().await {
            Ok(events) => {
                for event in events.response {
                    if !self.should_process_event(&event) {
                        continue;
                    }

                    if !self.state.has_seen_event(&event) {
                        self.print_new_event(&event);
                        self.handle_event(&event).await;
                    }
                }
            }
            Err(e) => eprintln!("Error fetching events: {e}"),
        }
    }

    fn should_process_event(&self, event: &Event) -> bool {
        // Skip redundant filterwheel events
        if event.event == event_types::FILTERWHEEL_CHANGED
            && let Some(EventDetails::FilterWheelChange { new, previous }) = &event.details
        {
            return new.name != previous.name;
        }
        true
    }

    async fn handle_event(&mut self, event: &Event) {
        match event.event.as_str() {
            event_types::TS_TARGETSTART => self.handle_ts_targetstart(event).await,
            event_types::AUTOFOCUS_FINISHED => self.handle_autofocus_finished(event).await,
            event_types::MOUNT_BEFORE_FLIP
            | event_types::MOUNT_AFTER_FLIP
            | event_types::MOUNT_PARKED => self.handle_mount_event(event).await,
            event_types::IMAGE_SAVE => {} // Handled in image polling
            _ => self.handle_generic_event(event).await,
        }
    }

    async fn handle_ts_targetstart(&mut self, event: &Event) {
        if let Some(EventDetails::TargetStart {
            target_name,
            coordinates,
            project_name,
            rotation,
            ..
        }) = &event.details
        {
            if target_name == "Sequential Instruction Set" {
                return;
            }

            let new_target = TargetInfo {
                name: target_name.clone(),
                source: TargetSource::TsTargetStart,
                coordinates: Some(coordinates.clone()),
                project: Some(project_name.clone()),
                rotation: Some(*rotation),
            };

            let old_target = self.state.current_target.clone();
            let target_changed = old_target
                .as_ref()
                .map(|t| t.name != new_target.name)
                .unwrap_or(true);

            if target_changed {
                self.state.current_target = Some(new_target.clone());
                println!("[TS-TARGETSTART] Target: {}", target_name);

                if let Some(webhook) = &self.discord_webhook {
                    if let Some(old) = old_target {
                        self.send_target_change_notification(webhook, &old, &new_target)
                            .await;
                    } else {
                        self.send_target_start_notification(webhook, &new_target)
                            .await;
                    }
                }
            }
        }
    }

    async fn handle_autofocus_finished(&self, event: &Event) {
        println!("[AUTOFOCUS FINISHED] {}", event.time);
        println!("Fetching autofocus results...");

        match self.client.get_last_autofocus().await {
            Ok(autofocus_data) => {
                self.display_autofocus_results(&autofocus_data);

                if let Some(webhook) = &self.discord_webhook {
                    self.send_autofocus_notification(webhook, &autofocus_data)
                        .await;
                }
            }
            Err(e) => eprintln!("Failed to fetch autofocus data: {e}"),
        }
    }

    async fn handle_mount_event(&self, event: &Event) {
        if let Some(webhook) = &self.discord_webhook {
            self.send_mount_event_notification(webhook, event).await;
        }
    }

    async fn handle_generic_event(&self, event: &Event) {
        if let Some(webhook) = &self.discord_webhook {
            self.send_generic_event_notification(webhook, event).await;
        }
    }

    async fn poll_sequence(&mut self) {
        match self.client.get_sequence().await {
            Ok(sequence) => {
                let new_sequence_target = extract_current_target(&sequence);
                let new_meridian_flip_time = extract_meridian_flip_time(&sequence);

                self.state.meridian_flip_time = new_meridian_flip_time;
                self.state.sequence = Some(sequence);

                // Only update target if we don't have a TS-TARGETSTART override
                if self
                    .state
                    .current_target
                    .as_ref()
                    .map(|t| t.source != TargetSource::TsTargetStart)
                    .unwrap_or(true)
                    && let Some(target_name) = new_sequence_target
                {
                    let new_target = TargetInfo {
                        name: target_name.clone(),
                        source: TargetSource::Sequence,
                        coordinates: None,
                        project: None,
                        rotation: None,
                    };

                    let old_target = self.state.current_target.clone();
                    let target_changed = old_target
                        .as_ref()
                        .map(|t| t.name != new_target.name)
                        .unwrap_or(true);

                    if target_changed {
                        self.state.current_target = Some(new_target.clone());
                        println!("[SEQUENCE TARGET] {}", target_name);

                        if let Some(webhook) = &self.discord_webhook {
                            if let Some(old) = old_target {
                                self.send_target_change_notification(webhook, &old, &new_target)
                                    .await;
                            } else {
                                self.send_target_start_notification(webhook, &new_target)
                                    .await;
                            }
                        }
                    }
                }
            }
            Err(e) => {
                if self.state.sequence.is_none() {
                    eprintln!("Error fetching sequence (will retry silently): {e}");
                }
            }
        }
    }

    async fn poll_images(&mut self) {
        match self.client.get_all_image_history().await {
            Ok(images) => {
                for (index, image) in images.response.iter().enumerate() {
                    if !self.state.has_seen_image(image) {
                        self.print_new_image(image);

                        let webhook_url = self.discord_webhook.clone();
                        if let Some(webhook) = webhook_url {
                            self.handle_new_image(&webhook, image, index).await;
                        }
                    }
                }
            }
            Err(e) => eprintln!("Error fetching images: {e}"),
        }
    }

    async fn handle_new_image(
        &mut self,
        webhook: &DiscordWebhook,
        image: &ImageMetadata,
        index: usize,
    ) {
        let should_send = match self.state.last_discord_image_time {
            None => true,
            Some(last_time) => last_time.elapsed() >= self.discord_image_cooldown,
        };

        if should_send {
            self.send_image_notification(webhook, image, index, self.state.skipped_images_count)
                .await;
            self.state.last_discord_image_time = Some(Instant::now());
            if self.state.skipped_images_count > 0 {
                println!(
                    "  Sent image to Discord (including {} skipped images)",
                    self.state.skipped_images_count
                );
            }
            self.state.skipped_images_count = 0;
        } else {
            self.state.skipped_images_count += 1;
            let remaining =
                self.discord_image_cooldown - self.state.last_discord_image_time.unwrap().elapsed();
            println!(
                "  Skipping Discord notification (cooldown: {:.0}s remaining)",
                remaining.as_secs_f32()
            );
        }
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
        if let Some(target) = &self.state.current_target {
            println!("  Target: {}", target.name);
        }
        if let Some(meridian_flip_hours) = self.state.meridian_flip_time {
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
        println!(
            "  Position Change: {}",
            af_data.calculated_focus_point.position - af_data.initial_focus_point.position
        );
        println!("  Best R-squared: {:.4}", af.get_best_r_squared());
    }

    // Discord notification methods
    async fn send_target_change_notification(
        &self,
        webhook: &DiscordWebhook,
        old_target: &TargetInfo,
        new_target: &TargetInfo,
    ) {
        let mut notification = DiscordNotification::new("🎯 Target Change")
            .color(colors::CYAN)
            .field("Previous Target", &old_target.name, true)
            .field("New Target", &new_target.name, true);

        if let Some(project) = &new_target.project {
            notification = notification.field("Project", project, true);
        }

        if let Some(coords) = &new_target.coordinates {
            notification = notification.field(
                "Coordinates",
                &format!("RA: {}\nDec: {}", coords.ra_string, coords.dec_string),
                false,
            );
        }

        if let Some(rotation) = &new_target.rotation {
            notification = notification.field("Rotation", &format!("{}°", rotation), true);
        }

        let notification = notification
            .with_meridian_flip(&self.state.meridian_flip_time)
            .with_mount_info(&self.client)
            .await;
        notification.send(webhook).await;
    }

    async fn send_target_start_notification(&self, webhook: &DiscordWebhook, target: &TargetInfo) {
        let mut notification = DiscordNotification::new("🎯 Target Started")
            .color(colors::GREEN)
            .field("Target", &target.name, false);

        if let Some(project) = &target.project {
            notification = notification.field("Project", project, true);
        }

        if let Some(coords) = &target.coordinates {
            notification = notification.field(
                "Coordinates",
                &format!("RA: {}\nDec: {}", coords.ra_string, coords.dec_string),
                false,
            );
        }

        if let Some(rotation) = &target.rotation {
            notification = notification.field("Rotation", &format!("{}°", rotation), true);
        }

        let notification = notification
            .with_meridian_flip(&self.state.meridian_flip_time)
            .with_mount_info(&self.client)
            .await;
        notification.send(webhook).await;
    }

    async fn send_autofocus_notification(&self, webhook: &DiscordWebhook, af: &AutofocusResponse) {
        if !af.success {
            return;
        }

        let af_data = &af.response;
        let color = if af.is_successful() {
            colors::GREEN
        } else {
            colors::ORANGE
        };
        let success_indicator = if af.is_successful() { "✅" } else { "⚠️" };

        let position_change =
            af_data.calculated_focus_point.position - af_data.initial_focus_point.position;
        let position_change_text = if position_change > 0 {
            format!("+{position_change}")
        } else {
            position_change.to_string()
        };

        DiscordNotification::new(&format!("{success_indicator} Autofocus Completed"))
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
            .footer(&format!("Focuser: {}", af_data.auto_focuser_name))
            .send(webhook)
            .await;
    }

    async fn send_mount_event_notification(&self, webhook: &DiscordWebhook, event: &Event) {
        let (title, color) = match event.event.as_str() {
            event_types::MOUNT_BEFORE_FLIP => {
                ("🔄 Mount Preparing for Meridian Flip", colors::ORANGE)
            }
            event_types::MOUNT_AFTER_FLIP => ("✅ Mount Meridian Flip Completed", colors::GREEN),
            event_types::MOUNT_PARKED => ("🅿️ Mount Parked", colors::YELLOW),
            _ => ("🔭 Mount Event", colors::GRAY),
        };

        let mut notification = DiscordNotification::new(title)
            .color(color)
            .field("Event", &event.event, true)
            .field("Time", &event.time, true);

        if let Some(target) = &self.state.current_target {
            notification = notification.field("Current Target", &target.name, true);
        }

        let notification = notification.with_mount_info(&self.client).await;
        notification.send(webhook).await;
    }

    async fn send_generic_event_notification(&self, webhook: &DiscordWebhook, event: &Event) {
        let color = get_event_color(&event.event);
        let title = get_event_title(&event.event);

        let mut notification =
            DiscordNotification::new(&title)
                .color(color)
                .field("Time", &event.time, false);

        // Add event-specific details
        if let Some(details) = &event.details {
            match details {
                EventDetails::FilterWheelChange { new, previous } => {
                    notification = notification
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
                EventDetails::TargetStart { .. } => {
                    // Already handled in handle_ts_targetstart
                    return;
                }
            }
        }

        notification.send(webhook).await;
    }

    async fn send_image_notification(
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

        let mut notification = DiscordNotification::new(&title).color(color);

        if let Some(target) = &self.state.current_target {
            notification = notification.field("Target", &target.name, true);
        }

        if skipped_count > 0 {
            notification = notification.field(
                "Images Since Last Post",
                &format!("{} images", skipped_count + 1),
                true,
            );
        }

        notification = notification
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
            .footer(&format!("Telescope: {}", image.telescope_name));

        if self
            .state
            .meridian_flip_time
            .as_ref()
            .map(|&h| h <= 1.0)
            .unwrap_or(false)
        {
            notification = notification.with_meridian_flip(&self.state.meridian_flip_time);
        }

        // Try to attach thumbnail
        notification
            .send_with_thumbnail(webhook, &self.client, index as u32)
            .await;
    }
}

// Discord notification builder
struct DiscordNotification {
    title: String,
    color: u32,
    fields: Vec<(String, String, bool)>,
    footer: Option<String>,
}

impl DiscordNotification {
    fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            color: colors::GRAY,
            fields: Vec::new(),
            footer: None,
        }
    }

    fn color(mut self, color: u32) -> Self {
        self.color = color;
        self
    }

    fn field(mut self, name: &str, value: &str, inline: bool) -> Self {
        self.fields
            .push((name.to_string(), value.to_string(), inline));
        self
    }

    fn footer(mut self, text: &str) -> Self {
        self.footer = Some(text.to_string());
        self
    }

    fn with_meridian_flip(mut self, meridian_flip_time: &Option<f64>) -> Self {
        if let Some(hours) = meridian_flip_time {
            let formatted = meridian_flip_time_formatted_with_clock(*hours);
            self.fields
                .push(("Meridian Flip In".to_string(), formatted, true));
        }
        self
    }

    async fn with_mount_info(mut self, client: &SpaceCatApiClient) -> Self {
        if let Ok(mount_info) = client.get_mount_info().await
            && mount_info.is_connected()
        {
            let (ra, dec) = mount_info.get_coordinates();
            let (alt, az) = mount_info.get_alt_az();

            self.fields.push((
                "Mount Position".to_string(),
                format!("RA: {ra}\nDec: {dec}"),
                true,
            ));
            self.fields
                .push(("Alt/Az".to_string(), format!("Alt: {alt}\nAz: {az}"), true));
            self.fields.push((
                "Pier Side".to_string(),
                mount_info.get_side_of_pier().to_string(),
                true,
            ));

            let tracking_status = if mount_info.response.tracking_enabled {
                "✅ Enabled"
            } else {
                "❌ Disabled"
            };
            self.fields
                .push(("Tracking".to_string(), tracking_status.to_string(), true));
        }
        self
    }

    async fn send(self, webhook: &DiscordWebhook) {
        let mut embed = Embed::new()
            .title(&self.title)
            .color(self.color)
            .timestamp(&chrono::Utc::now().to_rfc3339());

        for (name, value, inline) in self.fields {
            embed = embed.field(&name, &value, inline);
        }

        if let Some(footer_text) = self.footer {
            embed = embed.footer(&footer_text, None);
        }

        if let Err(e) = webhook.execute_with_embed(None, embed).await {
            eprintln!("Failed to send Discord notification: {e}");
        }
    }

    async fn send_with_thumbnail(
        self,
        webhook: &DiscordWebhook,
        client: &SpaceCatApiClient,
        image_index: u32,
    ) {
        let mut embed = Embed::new()
            .title(&self.title)
            .color(self.color)
            .timestamp(&chrono::Utc::now().to_rfc3339());

        for (name, value, inline) in self.fields {
            embed = embed.field(&name, &value, inline);
        }

        if let Some(footer_text) = self.footer {
            embed = embed.footer(&footer_text, None);
        }

        // Try to download and attach thumbnail
        match client.get_thumbnail(image_index).await {
            Ok(thumbnail_data) => {
                let filename = format!("thumbnail_{}.jpg", image_index);
                if let Err(e) = webhook
                    .execute_with_file(None, Some(embed), &thumbnail_data.data, &filename)
                    .await
                {
                    eprintln!("Failed to send image with thumbnail to Discord: {e}");
                }
            }
            Err(e) => {
                eprintln!("Failed to download thumbnail for image {image_index}: {e}");
                if let Err(e) = webhook.execute_with_embed(None, embed).await {
                    eprintln!("Failed to send image to Discord: {e}");
                }
            }
        }
    }
}

fn get_event_color(event: &str) -> u32 {
    match event {
        // Camera events
        event_types::CAMERA_CONNECTED => colors::GREEN,
        event_types::CAMERA_DISCONNECTED => colors::RED,

        // Filterwheel events
        event_types::FILTERWHEEL_CONNECTED => colors::BLUE,
        event_types::FILTERWHEEL_DISCONNECTED => colors::RED,
        event_types::FILTERWHEEL_CHANGED => colors::BLUE,

        // Mount events
        event_types::MOUNT_CONNECTED => colors::GREEN,
        event_types::MOUNT_DISCONNECTED => colors::RED,
        event_types::MOUNT_PARKED => colors::YELLOW,
        event_types::MOUNT_UNPARKED => colors::YELLOW,
        event_types::MOUNT_SLEW => colors::ORANGE,

        // Focuser events
        event_types::FOCUSER_CONNECTED => colors::GREEN,
        event_types::FOCUSER_DISCONNECTED => colors::RED,
        event_types::FOCUS_START => colors::PURPLE,
        event_types::FOCUS_END => colors::PURPLE,
        event_types::AUTOFOCUS_FINISHED => colors::PURPLE,

        // Rotator events
        event_types::ROTATOR_CONNECTED => colors::GREEN,
        event_types::ROTATOR_DISCONNECTED => colors::RED,
        event_types::ROTATOR_MOVED => colors::CYAN,
        event_types::ROTATOR_SYNCED => colors::CYAN,

        // Guider events
        event_types::GUIDER_CONNECTED => colors::GREEN,
        event_types::GUIDER_DISCONNECTED => colors::RED,
        event_types::GUIDER_START => colors::BLUE,
        event_types::GUIDER_DITHER => colors::CYAN,

        // Sequence events
        event_types::SEQUENCE_START => colors::CYAN,
        event_types::SEQUENCE_STOP => colors::ORANGE,
        event_types::SEQUENCE_PAUSE => colors::YELLOW,
        event_types::SEQUENCE_RESUME => colors::CYAN,
        event_types::SEQUENCE_FINISHED => colors::GREEN,
        event_types::ADV_SEQ_STOP => colors::ORANGE,

        // Exposure events
        event_types::EXPOSURE_START => colors::YELLOW,
        event_types::EXPOSURE_END => colors::GREEN,

        // System events
        event_types::FLAT_DISCONNECTED
        | event_types::WEATHER_DISCONNECTED
        | event_types::SWITCH_DISCONNECTED
        | event_types::DOME_DISCONNECTED
        | event_types::SAFETY_DISCONNECTED => colors::RED,

        // Target events
        event_types::TS_TARGETSTART => colors::CYAN,

        // Fallback patterns
        _ if event.contains("ERROR") => colors::RED,
        _ if event.contains("WARNING") => colors::ORANGE,
        _ => colors::GRAY,
    }
}

fn get_event_title(event: &str) -> String {
    match event {
        event_types::FILTERWHEEL_CHANGED => "🔄 Filter Changed".to_string(),
        event_types::TS_TARGETSTART => "🎯 Target Started".to_string(),
        _ => format!("📡 {}", event),
    }
}
