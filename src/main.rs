mod api;
mod config;
mod discord;
mod dual_poller;
mod events;
mod images;
mod poller;
mod sequence;

use api::SpaceCatApiClient;
use base64::Engine;
use clap::{Parser, Subcommand};
use config::Config;
use dual_poller::DualPoller;
use events::EventHistoryResponse;
use images::ImageHistoryResponse;
use poller::EventPoller;
use sequence::{SequenceResponse, extract_current_target};
use std::fs;
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
    /// Parse and display sequence information
    Sequence {
        /// Path to the sequence JSON file
        #[arg(short, long, default_value = "example_sequence.json")]
        file: String,
    },
    /// Get event history from API or file
    Events {
        /// Use local file instead of API
        #[arg(short, long)]
        local: bool,
        /// Path to the event history JSON file (when using --local)
        #[arg(short, long, default_value = "example_event-history.json")]
        file: String,
    },
    /// List the last N events with details
    LastEvents {
        /// Number of last events to display
        #[arg(short, long, default_value = "10")]
        count: usize,
        /// Use local file instead of API
        #[arg(long)]
        local: bool,
        /// Path to the event history JSON file (when using --local)
        #[arg(short, long, default_value = "example_event-history.json")]
        file: String,
    },
    /// Get image history from API or file  
    Images {
        /// Use local file instead of API
        #[arg(short, long)]
        local: bool,
        /// Path to the image history JSON file (when using --local)
        #[arg(short, long, default_value = "example_image-history.json")]
        file: String,
    },
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
    /// Test base64 image processing
    TestBase64,
    /// Run all demos (original behavior)
    All,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Sequence { file } => {
            if let Err(e) = cmd_sequence(&file).await {
                eprintln!("Sequence command failed: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Events { local, file } => {
            if let Err(e) = cmd_events(local, &file).await {
                eprintln!("Events command failed: {}", e);
                std::process::exit(1);
            }
        }
        Commands::LastEvents { count, local, file } => {
            if let Err(e) = cmd_last_events(count, local, &file).await {
                eprintln!("LastEvents command failed: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Images { local, file } => {
            if let Err(e) = cmd_images(local, &file).await {
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
        Commands::GetThumbnail { index, output, image_type } => {
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
        Commands::TestBase64 => {
            cmd_test_base64();
        }
        Commands::All => {
            if let Err(e) = cmd_all().await {
                eprintln!("All command failed: {}", e);
                std::process::exit(1);
            }
        }
    }
}

async fn cmd_sequence(file: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("Loading sequence from: {}", file);
    
    match load_sequence(file) {
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
        }
        Err(e) => {
            return Err(format!("Failed to load sequence: {}", e).into());
        }
    }
    
    Ok(())
}

async fn cmd_events(local: bool, file: &str) -> Result<(), Box<dyn std::error::Error>> {
    if local {
        println!("Loading events from local file: {}", file);
        match load_event_history(file) {
            Ok(events) => {
                println!(
                    "Successfully loaded {} events from file",
                    events.response.len()
                );
                display_event_statistics(&events);
            }
            Err(e) => {
                return Err(format!("Failed to load event history from file: {}", e).into());
            }
        }
    } else {
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
    }
    
    Ok(())
}

async fn cmd_last_events(count: usize, local: bool, file: &str) -> Result<(), Box<dyn std::error::Error>> {
    let events = if local {
        println!("Loading events from local file: {}", file);
        match load_event_history(file) {
            Ok(events) => {
                println!(
                    "Successfully loaded {} events from file",
                    events.response.len()
                );
                events
            }
            Err(e) => {
                return Err(format!("Failed to load event history from file: {}", e).into());
            }
        }
    } else {
        println!("Loading events from API...");
        match load_event_history_from_api().await {
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
        }
    };

    display_last_events(&events, count);
    Ok(())
}

async fn cmd_images(local: bool, file: &str) -> Result<(), Box<dyn std::error::Error>> {
    if local {
        println!("Loading images from local file: {}", file);
        match load_image_history(file) {
            Ok(images) => {
                println!(
                    "Successfully loaded {} images from file",
                    images.response.len()
                );
                display_image_statistics(&images);
                display_last_images(&images, 3);
            }
            Err(e) => {
                return Err(format!("Failed to load image history from file: {}", e).into());
            }
        }
    } else {
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
            eprintln!("Warning: Invalid parameter format '{}', expected 'key=value'", param);
        }
    }
    
    let config = Config::load_or_default();
    let client = SpaceCatApiClient::new(config.api)?;
    
    match client.get_image_with_params(index, &param_pairs).await {
        Ok(image_response) => {
            println!("Successfully retrieved image:");
            println!("  Status: {}, Success: {}", image_response.status_code, image_response.success);
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
                            } else if decoded.starts_with(b"RIFF") && decoded.len() > 8 && &decoded[8..12] == b"WEBP" {
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
    image_type: Option<&str>
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
                    return Err(format!("Failed to save thumbnail to {}: {}", output_path, e).into());
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

fn cmd_test_base64() {
    println!("Testing base64 decoding with a known PNG image...");
    
    // Small 1x1 PNG image encoded as base64 (transparent pixel)
    let test_png_base64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChAI9AAAAAElFTkSuQmCC";
    
    println!("  Test base64: {}", test_png_base64);
    
    match base64::engine::general_purpose::STANDARD.decode(test_png_base64) {
        Ok(decoded) => {
            println!("  Successfully decoded {} bytes", decoded.len());
            
            if decoded.len() > 10 {
                let header = &decoded[0..std::cmp::min(10, decoded.len())];
                println!("  Image header (hex): {:02x?}", header);
                
                if decoded.starts_with(b"\x89PNG\r\n\x1a\n") {
                    println!("  ✓ Confirmed PNG format");
                    println!("  This demonstrates our base64 processing works correctly!");
                } else {
                    println!("  Unexpected format for test PNG");
                }
            }
        }
        Err(e) => {
            println!("  Failed to decode test base64: {}", e);
        }
    }
}

async fn cmd_all() -> Result<(), Box<dyn std::error::Error>> {
    println!("SpaceCat - Astronomical Observation System");
    println!("Running all demos...\n");

    println!("=== Sequence Demo ===");
    cmd_sequence("example_sequence.json").await?;

    println!("\n=== Events Demo ===");
    cmd_events(false, "").await?;

    println!("\n=== Last Events Demo ===");
    cmd_last_events(5, false, "").await?;

    println!("\n=== Images Demo ===");
    cmd_images(false, "").await?;

    println!("\n=== Get Image Demo ===");
    cmd_get_image(0, &[]).await?;

    println!("\n=== Get Thumbnail Demo ===");
    cmd_get_thumbnail(0, "demo_thumbnail.jpg", None).await?;

    println!("\n=== Polling Demo ===");
    cmd_poll(2, 5).await?;

    println!("\n=== Base64 Test Demo ===");
    cmd_test_base64();

    println!("\nAll demos completed!");
    Ok(())
}

// Helper functions

fn load_sequence(filename: &str) -> Result<SequenceResponse, Box<dyn std::error::Error>> {
    let json_content = fs::read_to_string(filename)?;
    let sequence: SequenceResponse = serde_json::from_str(&json_content)?;
    Ok(sequence)
}

fn load_event_history(filename: &str) -> Result<EventHistoryResponse, Box<dyn std::error::Error>> {
    let json_content = fs::read_to_string(filename)?;
    let events: EventHistoryResponse = serde_json::from_str(&json_content)?;
    Ok(events)
}

fn load_image_history(filename: &str) -> Result<ImageHistoryResponse, Box<dyn std::error::Error>> {
    let json_content = fs::read_to_string(filename)?;
    let images: ImageHistoryResponse = serde_json::from_str(&json_content)?;
    Ok(images)
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
        let max_temp = temperatures.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
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
    let last_images: Vec<_> = images.response.iter()
        .enumerate()
        .rev()  // Reverse to get most recent first
        .take(count)
        .collect();
    
    if last_images.is_empty() {
        println!("No images to display");
        return;
    }
    
    for (index, image) in last_images.iter().rev() {  // Reverse again to show in chronological order
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
        println!("  Mean: {:.1}, Median: {:.1}, StDev: {:.1}", 
                image.mean, image.median, image.st_dev);
    }
}

fn display_last_events(events: &EventHistoryResponse, count: usize) {
    println!("\n=== Last {} Events ===", count);
    
    if events.response.is_empty() {
        println!("No events available");
        return;
    }
    
    // Get the last N events (events are typically in chronological order)
    let last_events: Vec<_> = events.response.iter()
        .enumerate()
        .rev()  // Reverse to get most recent first
        .take(count)
        .collect();
    
    if last_events.is_empty() {
        println!("No events to display");
        return;
    }
    
    for (index, event) in last_events.iter().rev() {  // Reverse again to show in chronological order
        println!("\nEvent Index {}: ", index);
        println!("  Time: {}", event.time);
        println!("  Event: {}", event.event);
        
        // Display details if available
        if let Some(ref details) = event.details {
            match details {
                crate::events::EventDetails::FilterWheelChange { new, previous } => {
                    println!("  Details: Filter changed from {} to {}", previous.name, new.name);
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
