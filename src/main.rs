mod api;
mod config;
mod events;
mod poller;
mod sequence;

use api::SpaceCatApiClient;
use config::Config;
use events::EventHistoryResponse;
use poller::EventPoller;
use sequence::SequenceResponse;
use std::fs;
use std::time::Duration;

#[tokio::main]
async fn main() {
    println!("SpaceCat - Astronomical Sequence Manager");

    // Load and parse the example sequence JSON
    match load_sequence("example_sequence.json") {
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
        }
        Err(e) => {
            eprintln!("Failed to load sequence: {}", e);
        }
    }

    println!("\n--- Event History ---");

    // Try to load from API first, then fall back to file
    match load_event_history_from_api().await {
        Ok(events) => {
            println!(
                "Successfully loaded {} events from API",
                events.response.len()
            );
            display_event_statistics(&events);
        }
        Err(e) => {
            println!("Failed to load from API: {}", e);
            println!("Falling back to local file...");

            // Fall back to local file
            match load_event_history("example_event-history.json") {
                Ok(events) => {
                    println!(
                        "Successfully loaded {} events from file",
                        events.response.len()
                    );
                    display_event_statistics(&events);
                }
                Err(e) => {
                    eprintln!("Failed to load event history from file: {}", e);
                }
            }
        }
    }

    println!("\n--- Event Polling Demo ---");

    // Demonstrate the event poller
    match demo_event_polling().await {
        Ok(_) => println!("Polling demo completed"),
        Err(e) => println!("Polling demo failed: {}", e),
    }
}

async fn demo_event_polling() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration and create client
    let config = Config::load_or_default();
    let client = SpaceCatApiClient::new(config.api)?;

    // Create event poller with 2-second interval
    let mut poller = EventPoller::new(client, Duration::from_secs(2));

    println!("Starting event polling demo (will run for 10 seconds)...");
    println!("Poll interval: {:?}", poller.poll_interval());

    // Poll for new events a few times
    for i in 1..=5 {
        println!("Poll #{}", i);

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

        println!();
    }

    Ok(())
}

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

async fn load_event_history_from_api() -> Result<EventHistoryResponse, Box<dyn std::error::Error>> {
    // Load configuration from config.json or use default
    let config = Config::load_or_default();

    // Validate configuration
    if let Err(e) = config.validate() {
        println!("Configuration validation failed: {}", e);
        return Err(e.into());
    }

    println!("Connecting to API at: {}", config.api.base_url);

    // Create API client
    let client = SpaceCatApiClient::new(config.api)?;

    // Check API version and health
    match client.get_version().await {
        Ok(version) => {
            println!(
                "API version: {} (success: {})",
                version.response, version.success
            );
            if !version.success {
                println!("API warning: {}", version.error);
            }
        }
        Err(e) => {
            println!("Could not get API version: {}", e);
        }
    }

    // Fetch event history
    let events = client.get_event_history().await?;
    Ok(events)
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
