mod api;
mod autofocus;
mod config;
mod discord;
mod dual_poller;
mod events;
mod images;
mod poller;
mod sequence;

use api::SpaceCatApiClient;
use autofocus::AutofocusResponse;
use base64::Engine;
use clap::{Parser, Subcommand};
use config::Config;
use dual_poller::DualPoller;
use events::EventHistoryResponse;
use images::ImageHistoryResponse;
use poller::EventPoller;
use sequence::{
    SequenceResponse, extract_current_target, extract_meridian_flip_time,
    meridian_flip_time_formatted_with_clock,
};
use std::time::Duration;

#[derive(Parser)]
#[command(name = "spacecat")]
#[command(about = "SpaceCat - Astronomical Observation System", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Get current sequence information from API
    Sequence,
    /// Get event history from API
    Events,
    /// List the last N events with details
    LastEvents {
        /// Number of last events to display
        #[arg(short, long, default_value = "10")]
        count: usize,
    },
    /// Get image history from API
    Images,
    /// Get a specific image by index
    GetImage {
        /// Image index to retrieve
        #[arg(default_value = "0")]
        index: u32,
        /// Additional parameters as key=value pairs
        #[arg(short, long)]
        params: Vec<String>,
    },
    /// Get a thumbnail for a specific image by index
    GetThumbnail {
        /// Image index to retrieve thumbnail for
        #[arg(default_value = "0")]
        index: u32,
        /// Output file path for the thumbnail
        #[arg(short, long, default_value = "thumbnail.jpg")]
        output: String,
        /// Image type filter (LIGHT, FLAT, DARK, BIAS, SNAPSHOT)
        #[arg(long)]
        image_type: Option<String>,
    },
    /// Poll for new events in real-time
    Poll {
        /// Poll interval in seconds
        #[arg(short, long, default_value = "2")]
        interval: u64,
        /// Number of poll cycles to run
        #[arg(short, long, default_value = "5")]
        count: u32,
    },
    /// Poll for both new events and images in real-time
    DualPoll {
        /// Poll interval in seconds
        #[arg(short, long, default_value = "5")]
        interval: u64,
    },
    /// Get the last autofocus data from API
    LastAutofocus,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Sequence => {
            if let Err(e) = cmd_sequence().await {
                eprintln!("Sequence command failed: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Events => {
            if let Err(e) = cmd_events().await {
                eprintln!("Events command failed: {}", e);
                std::process::exit(1);
            }
        }
        Commands::LastEvents { count } => {
            if let Err(e) = cmd_last_events(count).await {
                eprintln!("LastEvents command failed: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Images => {
            if let Err(e) = cmd_images().await {
                eprintln!("Images command failed: {}", e);
                std::process::exit(1);
            }
        }
        Commands::GetImage { index, params } => {
            if let Err(e) = cmd_get_image(index, &params).await {
                eprintln!("GetImage command failed: {}", e);
                std::process::exit(1);
            }
        }
        Commands::GetThumbnail {
            index,
            output,
            image_type,
        } => {
            if let Err(e) = cmd_get_thumbnail(index, &output, image_type.as_deref()).await {
                eprintln!("GetThumbnail command failed: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Poll { interval, count } => {
            if let Err(e) = cmd_poll(interval, count).await {
                eprintln!("Poll command failed: {}", e);
                std::process::exit(1);
            }
        }
        Commands::DualPoll { interval } => {
            if let Err(e) = cmd_dual_poll(interval).await {
                eprintln!("DualPoll command failed: {}", e);
                std::process::exit(1);
            }
        }
        Commands::LastAutofocus => {
            if let Err(e) = cmd_last_autofocus().await {
                eprintln!("LastAutofocus command failed: {}", e);
                std::process::exit(1);
            }
        }
    }
}

async fn cmd_sequence() -> Result<(), Box<dyn std::error::Error>> {
    println!("Loading sequence from API...");

    match load_sequence_from_api().await {
        Ok(seq) => {
            println!(
                "Successfully loaded sequence with {} items",
                seq.response.len()
            );
            println!("Status: {}, Success: {}", seq.status_code, seq.success);

            // Get global triggers
            if let Some(triggers) = seq.get_global_triggers() {
                println!("Found {} global triggers", triggers.global_triggers.len());
            }

            // Get all containers
            let containers = seq.get_containers();
            println!("Found {} containers:", containers.len());
            for container in &containers {
                println!(
                    "  - {} (status: {}, {} items)",
                    container.name,
                    container.status,
                    container.items.len()
                );
            }

            // Test the new extract_current_target utility function
            if let Some(target) = extract_current_target(&seq) {
                println!("Current active target: {}", target);
            } else {
                println!("No active target found");
            }

            // Extract meridian flip information
            if let Some(meridian_flip_hours) = extract_meridian_flip_time(&seq) {
                let formatted_time = meridian_flip_time_formatted_with_clock(meridian_flip_hours);
                println!(
                    "Meridian flip in: {:.3} hours ({})",
                    meridian_flip_hours, formatted_time
                );
            } else {
                println!("No meridian flip information available");
            }
        }
        Err(e) => {
            return Err(format!("Failed to load sequence from API: {}", e).into());
        }
    }

    Ok(())
}

async fn cmd_events() -> Result<(), Box<dyn std::error::Error>> {
    println!("Loading events from API...");
    match load_event_history_from_api().await {
        Ok(events) => {
            println!(
                "Successfully loaded {} events from API",
                events.response.len()
            );
            display_event_statistics(&events);
        }
        Err(e) => {
            return Err(format!("Failed to load events from API: {}", e).into());
        }
    }

    Ok(())
}

async fn cmd_last_events(count: usize) -> Result<(), Box<dyn std::error::Error>> {
    println!("Loading events from API...");
    let events = match load_event_history_from_api().await {
        Ok(events) => {
            println!(
                "Successfully loaded {} events from API",
                events.response.len()
            );
            events
        }
        Err(e) => {
            return Err(format!("Failed to load events from API: {}", e).into());
        }
    };

    display_last_events(&events, count);
    Ok(())
}

async fn cmd_images() -> Result<(), Box<dyn std::error::Error>> {
    println!("Loading images from API...");
    match load_image_history_from_api().await {
        Ok(images) => {
            println!(
                "Successfully loaded {} images from API",
                images.response.len()
            );
            display_image_statistics(&images);
            display_last_images(&images, 3);
        }
        Err(e) => {
            return Err(format!("Failed to load images from API: {}", e).into());
        }
    }

    Ok(())
}

async fn cmd_get_image(index: u32, params: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    println!("Getting image at index {} from API...", index);

    // Parse additional parameters
    let mut param_pairs = vec![("autoPrepare", "true")]; // Default parameter
    for param in params {
        if let Some((key, value)) = param.split_once('=') {
            param_pairs.push((key, value));
        } else {
            eprintln!(
                "Warning: Invalid parameter format '{}', expected 'key=value'",
                param
            );
        }
    }

    let config = Config::load_or_default();
    let client = SpaceCatApiClient::new(config.api)?;

    match client.get_image_with_params(index, &param_pairs).await {
        Ok(image_response) => {
            println!("Successfully retrieved image:");
            println!(
                "  Status: {}, Success: {}",
                image_response.status_code, image_response.success
            );
            println!("  Response Type: {}", image_response.response_type);

            if !image_response.error.is_empty() {
                println!("  Error: {}", image_response.error);
            }

            // Check if we got image data
            if image_response.success && !image_response.response.is_empty() {
                let data_size = image_response.response.len();
                println!("  Image data size: {} characters (base64)", data_size);

                // Show first few characters of base64 data as a sample
                let preview = if data_size > 50 {
                    format!("{}...", &image_response.response[0..50])
                } else {
                    image_response.response.clone()
                };
                println!("  Base64 preview: {}", preview);

                // Try to decode base64 to get actual image size
                match base64::engine::general_purpose::STANDARD.decode(&image_response.response) {
                    Ok(decoded) => {
                        println!("  Decoded image size: {} bytes", decoded.len());

                        // Check if this looks like a valid image by examining the header
                        if decoded.len() > 10 {
                            let header = &decoded[0..std::cmp::min(10, decoded.len())];
                            println!("  Image header (hex): {:02x?}", header);

                            // Check for common image formats
                            if decoded.starts_with(b"\x89PNG\r\n\x1a\n") {
                                println!("  Image format: PNG");
                            } else if decoded.starts_with(&[0xFF, 0xD8, 0xFF]) {
                                println!("  Image format: JPEG");
                            } else if decoded.starts_with(b"GIF8") {
                                println!("  Image format: GIF");
                            } else if decoded.starts_with(b"BM") {
                                println!("  Image format: BMP");
                            } else if decoded.starts_with(b"RIFF")
                                && decoded.len() > 8
                                && &decoded[8..12] == b"WEBP"
                            {
                                println!("  Image format: WebP");
                            } else {
                                println!("  Image format: Unknown or custom format");
                            }
                        }
                    }
                    Err(e) => {
                        println!("  Failed to decode base64: {}", e);
                    }
                }
            } else {
                println!("  No image data received");
            }
        }
        Err(e) => {
            return Err(format!("Failed to get image: {}", e).into());
        }
    }

    Ok(())
}

async fn cmd_get_thumbnail(
    index: u32,
    output_path: &str,
    image_type: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Getting thumbnail for image at index {} from API...", index);

    let config = Config::load_or_default();
    let client = SpaceCatApiClient::new(config.api)?;

    // Build parameters
    let mut params = vec![];
    if let Some(img_type) = image_type {
        params.push(("imageType", img_type));
    }

    match client.get_thumbnail_with_params(index, &params).await {
        Ok(thumbnail_response) => {
            println!("Successfully retrieved thumbnail:");
            println!("  Status Code: {}", thumbnail_response.status_code);
            println!("  Content Type: {}", thumbnail_response.content_type);
            println!("  Data Size: {} bytes", thumbnail_response.data.len());

            // Save the thumbnail to disk
            match std::fs::write(output_path, &thumbnail_response.data) {
                Ok(()) => {
                    println!("  Thumbnail saved to: {}", output_path);

                    // Try to detect image format from first few bytes
                    if thumbnail_response.data.len() >= 4 {
                        let header = &thumbnail_response.data[0..4];
                        if header.starts_with(&[0xFF, 0xD8, 0xFF]) {
                            println!("  Format: JPEG");
                        } else if header.starts_with(b"\x89PNG") {
                            println!("  Format: PNG");
                        } else {
                            println!("  Format: Unknown (header: {:02x?})", header);
                        }
                    }
                }
                Err(e) => {
                    return Err(
                        format!("Failed to save thumbnail to {}: {}", output_path, e).into(),
                    );
                }
            }
        }
        Err(e) => {
            return Err(format!("Failed to get thumbnail: {}", e).into());
        }
    }

    Ok(())
}

async fn cmd_poll(interval: u64, count: u32) -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting event polling...");
    println!("Poll interval: {}s, Poll cycles: {}", interval, count);

    let config = Config::load_or_default();
    let client = SpaceCatApiClient::new(config.api)?;
    let mut poller = EventPoller::new(client, Duration::from_secs(interval));

    for i in 1..=count {
        println!("\nPoll #{}", i);

        match poller.poll_new_events().await {
            Ok(result) => {
                println!("  {}", result.summary());

                if result.has_new_events() {
                    println!("  New events found:");
                    for event in &result.new_events {
                        println!("    {} at {}", event.event, event.time);
                    }

                    // Show specific event types
                    let image_saves = result.get_events_by_type("IMAGE-SAVE");
                    if !image_saves.is_empty() {
                        println!("    → {} image saves in this batch", image_saves.len());
                    }

                    let filter_changes = result.get_events_by_type("FILTERWHEEL-CHANGED");
                    if !filter_changes.is_empty() {
                        println!(
                            "    → {} filter changes in this batch",
                            filter_changes.len()
                        );
                    }
                } else {
                    println!("  No new events since last poll");
                }

                println!("  Total events seen: {}", poller.seen_event_count());
            }
            Err(e) => {
                println!("  Poll failed: {}", e);
            }
        }
    }

    Ok(())
}

async fn cmd_dual_poll(interval: u64) -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting dual polling (events and images)...");
    println!("Poll interval: {}s", interval);
    println!("Press Ctrl+C to stop\n");

    let config = Config::load_or_default();
    let client = SpaceCatApiClient::new(config.api.clone())?;
    let mut poller = DualPoller::new(client);

    // Check for Discord webhook configuration
    if let Some(discord_config) = &config.discord {
        if discord_config.enabled && !discord_config.webhook_url.is_empty() {
            println!("Discord webhook configured, events will be sent to Discord");
            poller = poller.with_discord_webhook(&discord_config.webhook_url)?;
        } else if !discord_config.enabled {
            println!("Discord webhook disabled in configuration");
        }
    } else {
        println!("No Discord webhook configured (add 'discord' section to config.json to enable)");
    }

    poller.start_polling(Duration::from_secs(interval)).await;

    Ok(())
}

async fn cmd_last_autofocus() -> Result<(), Box<dyn std::error::Error>> {
    println!("Loading autofocus data from API...");
    match load_autofocus_from_api().await {
        Ok(autofocus) => {
            println!("Successfully loaded autofocus data from API");
            display_autofocus_data(&autofocus);
        }
        Err(e) => {
            return Err(format!("Failed to load autofocus data from API: {}", e).into());
        }
    }

    Ok(())
}

// Helper functions

async fn load_autofocus_from_api() -> Result<AutofocusResponse, Box<dyn std::error::Error>> {
    // Load configuration from config.json or use default
    let config = Config::load_or_default();

    // Validate configuration
    if let Err(e) = config.validate() {
        return Err(e.into());
    }

    // Create API client
    let client = SpaceCatApiClient::new(config.api)?;

    // Check API version and health
    match client.get_version().await {
        Ok(_) => {} // Version check successful
        Err(e) => {
            return Err(format!("Could not get API version: {}", e).into());
        }
    }

    // Fetch autofocus data
    let autofocus = client.get_last_autofocus().await?;
    Ok(autofocus)
}

fn display_autofocus_data(autofocus: &AutofocusResponse) {
    println!(
        "Status: {}, Success: {}",
        autofocus.status_code, autofocus.success
    );

    if !autofocus.error.is_empty() {
        println!("Error: {}", autofocus.error);
        return;
    }

    let af_data = &autofocus.response;

    println!("\n=== Autofocus Summary ===");
    println!("Filter: {}", af_data.filter);
    println!("Focuser: {}", af_data.auto_focuser_name);
    println!("Star Detector: {}", af_data.star_detector_name);
    println!("Method: {}", af_data.method);
    println!("Fitting: {}", af_data.fitting);
    println!("Temperature: {:.1}°C", af_data.temperature);
    println!("Duration: {}", af_data.duration);
    println!("Timestamp: {}", af_data.timestamp);

    println!("\n=== Focus Results ===");
    println!("Initial Position: {}", af_data.initial_focus_point.position);
    println!(
        "Calculated Position: {}",
        af_data.calculated_focus_point.position
    );
    println!(
        "Calculated HFR: {:.3}",
        af_data.calculated_focus_point.value
    );
    println!(
        "Previous Position: {}",
        af_data.previous_focus_point.position
    );

    let (min_pos, max_pos) = af_data.get_focus_range();
    println!("Focus Range: {} - {}", min_pos, max_pos);

    if let Some(best_hfr) = af_data.get_best_measured_hfr() {
        println!("Best Measured HFR: {:.3}", best_hfr);
    }

    println!(
        "\n=== Measurement Points ({}) ===",
        af_data.measure_points.len()
    );
    for (i, point) in af_data.measure_points.iter().enumerate() {
        println!(
            "  {:2}: Position {}, HFR {:.3}, Error {:.3}",
            i + 1,
            point.position,
            point.value,
            point.error
        );
    }

    println!("\n=== Curve Fitting Results ===");
    println!("R-squared values:");
    println!("  Quadratic: {:.4}", af_data.r_squares.quadratic);
    println!("  Hyperbolic: {:.4}", af_data.r_squares.hyperbolic);
    println!("  Left Trend: {:.4}", af_data.r_squares.left_trend);
    println!("  Right Trend: {:.4}", af_data.r_squares.right_trend);
    println!("Best R-squared: {:.4}", autofocus.get_best_r_squared());

    println!("\n=== Intersections ===");
    let intersections = &af_data.intersections;
    println!(
        "Trend Line Intersection: Position {}, Value {:.3}",
        intersections.trend_line_intersection.position, intersections.trend_line_intersection.value
    );
    println!(
        "Hyperbolic Minimum: Position {}, Value {:.3}",
        intersections.hyperbolic_minimum.position, intersections.hyperbolic_minimum.value
    );
    println!(
        "Quadratic Minimum: Position {}, Value {:.3}",
        intersections.quadratic_minimum.position, intersections.quadratic_minimum.value
    );
    println!(
        "Gaussian Maximum: Position {}, Value {:.3}",
        intersections.gaussian_maximum.position, intersections.gaussian_maximum.value
    );

    println!("\n=== Backlash Compensation ===");
    let backlash = &af_data.backlash_compensation;
    println!("Model: {}", backlash.backlash_compensation_model);
    println!("Backlash IN: {}", backlash.backlash_in);
    println!("Backlash OUT: {}", backlash.backlash_out);

    println!("\n=== Analysis ===");
    if autofocus.is_successful() {
        println!("✅ Autofocus appears successful");
    } else {
        println!("❌ Autofocus may have issues");
    }

    let position_change =
        af_data.calculated_focus_point.position - af_data.previous_focus_point.position;
    if position_change != 0 {
        println!("Focus position changed by {} steps", position_change);
    } else {
        println!("Focus position unchanged from previous run");
    }
}

async fn load_sequence_from_api() -> Result<SequenceResponse, Box<dyn std::error::Error>> {
    // Load configuration from config.json or use default
    let config = Config::load_or_default();

    // Validate configuration
    if let Err(e) = config.validate() {
        return Err(e.into());
    }

    // Create API client
    let client = SpaceCatApiClient::new(config.api)?;

    // Check API version and health
    match client.get_version().await {
        Ok(_) => {} // Version check successful
        Err(e) => {
            return Err(format!("Could not get API version: {}", e).into());
        }
    }

    // Fetch sequence data
    let sequence = client.get_sequence().await?;
    Ok(sequence)
}

async fn load_event_history_from_api() -> Result<EventHistoryResponse, Box<dyn std::error::Error>> {
    // Load configuration from config.json or use default
    let config = Config::load_or_default();

    // Validate configuration
    if let Err(e) = config.validate() {
        return Err(e.into());
    }

    // Create API client
    let client = SpaceCatApiClient::new(config.api)?;

    // Check API version and health
    match client.get_version().await {
        Ok(_) => {} // Version check successful
        Err(e) => {
            return Err(format!("Could not get API version: {}", e).into());
        }
    }

    // Fetch event history
    let events = client.get_event_history().await?;
    Ok(events)
}

async fn load_image_history_from_api() -> Result<ImageHistoryResponse, Box<dyn std::error::Error>> {
    // Load configuration from config.json or use default
    let config = Config::load_or_default();

    // Validate configuration
    if let Err(e) = config.validate() {
        return Err(e.into());
    }

    // Create API client
    let client = SpaceCatApiClient::new(config.api)?;

    // Check API version and health
    match client.get_version().await {
        Ok(_) => {} // Version check successful
        Err(e) => {
            return Err(format!("Could not get API version: {}", e).into());
        }
    }

    // Fetch all image history
    let images = client.get_all_image_history().await?;
    Ok(images)
}

fn display_event_statistics(events: &EventHistoryResponse) {
    println!(
        "Status: {}, Success: {}",
        events.status_code, events.success
    );

    // Show event statistics
    let counts = events.count_events_by_type();
    println!("Event type counts:");
    for (event_type, count) in counts.iter() {
        println!("  {}: {}", event_type, count);
    }

    // Show filter wheel changes
    let filter_changes = events.get_filterwheel_changes();
    println!("\nFound {} filter wheel changes", filter_changes.len());

    // Show image saves
    let image_saves = events.get_image_saves();
    println!("Found {} image saves", image_saves.len());

    // Show connection events
    let connections = events.get_connection_events();
    println!("Found {} connection events", connections.len());
}

fn display_image_statistics(images: &ImageHistoryResponse) {
    println!(
        "Status: {}, Success: {}",
        images.status_code, images.success
    );

    // Show session statistics
    let stats = images.get_session_stats();
    println!("{}", stats);

    // Show image type counts
    let type_counts = images.count_images_by_type();
    println!("\nImage type counts:");
    for (image_type, count) in type_counts.iter() {
        println!("  {}: {}", image_type, count);
    }

    // Show filter counts
    let filter_counts = images.count_images_by_filter();
    println!("\nFilter counts:");
    for (filter, count) in filter_counts.iter() {
        println!("  {}: {}", filter, count);
    }

    // Show light frames by filter
    let light_frames = images.get_light_frames();
    if !light_frames.is_empty() {
        println!("\nLight frames by filter:");
        let mut filter_lights = std::collections::HashMap::new();
        for frame in light_frames {
            *filter_lights.entry(&frame.filter).or_insert(0) += 1;
        }
        for (filter, count) in filter_lights.iter() {
            println!("  {}: {} light frames", filter, count);
        }
    }

    // Show calibration breakdown
    let calibration = images.get_calibration_frames();
    println!("\nFound {} calibration frames", calibration.len());

    // Temperature range
    if !images.response.is_empty() {
        let temperatures: Vec<f64> = images.response.iter().map(|img| img.temperature).collect();
        let min_temp = temperatures.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        let max_temp = temperatures
            .iter()
            .fold(f64::NEG_INFINITY, |a, &b| a.max(b));
        println!("Temperature range: {:.1}°C to {:.1}°C", min_temp, max_temp);
    }
}

fn display_last_images(images: &ImageHistoryResponse, count: usize) {
    println!("\n=== Last {} Images ===", count);

    if images.response.is_empty() {
        println!("No images available");
        return;
    }

    // Get the last N images (images are typically in chronological order)
    let last_images: Vec<_> = images
        .response
        .iter()
        .enumerate()
        .rev() // Reverse to get most recent first
        .take(count)
        .collect();

    if last_images.is_empty() {
        println!("No images to display");
        return;
    }

    for (index, image) in last_images.iter().rev() {
        // Reverse again to show in chronological order
        println!("\nImage Index {}: ", index);
        println!("  Date: {}", image.date);
        println!("  Type: {}", image.image_type);
        println!("  Filter: {}", image.filter);
        println!("  Exposure: {:.1}s", image.exposure_time);
        println!("  Temperature: {:.1}°C", image.temperature);
        println!("  Camera: {}", image.camera_name);
        println!("  Telescope: {}", image.telescope_name);
        println!("  Gain: {}, Offset: {}", image.gain, image.offset);
        println!("  Stars: {}, HFR: {:.2}", image.stars, image.hfr);
        println!(
            "  Mean: {:.1}, Median: {:.1}, StDev: {:.1}",
            image.mean, image.median, image.st_dev
        );
    }
}

fn display_last_events(events: &EventHistoryResponse, count: usize) {
    println!("\n=== Last {} Events ===", count);

    if events.response.is_empty() {
        println!("No events available");
        return;
    }

    // Get the last N events (events are typically in chronological order)
    let last_events: Vec<_> = events
        .response
        .iter()
        .enumerate()
        .rev() // Reverse to get most recent first
        .take(count)
        .collect();

    if last_events.is_empty() {
        println!("No events to display");
        return;
    }

    for (index, event) in last_events.iter().rev() {
        // Reverse again to show in chronological order
        println!("\nEvent Index {}: ", index);
        println!("  Time: {}", event.time);
        println!("  Event: {}", event.event);

        // Display details if available
        if let Some(ref details) = event.details {
            match details {
                crate::events::EventDetails::FilterWheelChange { new, previous } => {
                    println!(
                        "  Details: Filter changed from {} to {}",
                        previous.name, new.name
                    );
                }
            }
        }

        // Display event type with emoji and description
        let (emoji, description) = get_event_type_info(&event.event);
        println!("  Type: {} {}", emoji, description);
    }
}

fn get_event_type_info(event_name: &str) -> (&'static str, &'static str) {
    if is_connection_event(event_name) {
        return get_connection_event_info(event_name);
    }

    match event_name {
        "IMAGE-SAVE" => ("📸", "Image captured and saved"),
        "FILTERWHEEL-CHANGED" => ("🔄", "Filter wheel position changed"),
        "GUIDER-DITHER" => ("🎯", "Dithering for drizzling"),
        "SEQUENCE-START" => ("▶️", "Sequence started"),
        "SEQUENCE-STOP" => ("⏹️", "Sequence stopped"),
        "SEQUENCE-PAUSE" => ("⏸️", "Sequence paused"),
        "SEQUENCE-RESUME" => ("▶️", "Sequence resumed"),
        "EXPOSURE-START" => ("🌟", "Exposure started"),
        "EXPOSURE-END" => ("✨", "Exposure completed"),
        "MOUNT-SLEW" => ("🔭", "Mount slewing to target"),
        "FOCUS-START" => ("🔍", "Auto-focus started"),
        "FOCUS-END" => ("✅", "Auto-focus completed"),
        _ => ("📋", "System event"),
    }
}

fn is_connection_event(event_name: &str) -> bool {
    event_name.contains("CONNECTED") || event_name.contains("DISCONNECTED")
}

fn get_connection_event_info(event_name: &str) -> (&'static str, &'static str) {
    if event_name.contains("DISCONNECTED") {
        ("🔴", "Equipment disconnected")
    } else if event_name.contains("CONNECTED") {
        ("🟢", "Equipment connected")
    } else {
        ("📋", "Connection event")
    }
}
