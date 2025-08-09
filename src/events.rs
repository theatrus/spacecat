use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct EventHistoryResponse {
    pub response: Vec<Event>,
    pub error: String,
    pub status_code: i32,
    pub success: bool,
    #[serde(rename = "Type")]
    pub response_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Event {
    pub time: String,
    pub event: String,
    #[serde(flatten)]
    pub details: Option<EventDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EventDetails {
    FilterWheelChange {
        #[serde(rename = "New")]
        new: FilterInfo,
        #[serde(rename = "Previous")]
        previous: FilterInfo,
    },
    // Add other event details as needed
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct FilterInfo {
    pub name: String,
    pub id: i32,
}

// Event type constants for easier matching
pub mod event_types {
    pub const CAMERA_DISCONNECTED: &str = "CAMERA-DISCONNECTED";
    pub const CAMERA_CONNECTED: &str = "CAMERA-CONNECTED";
    pub const FILTERWHEEL_DISCONNECTED: &str = "FILTERWHEEL-DISCONNECTED";
    pub const FILTERWHEEL_CONNECTED: &str = "FILTERWHEEL-CONNECTED";
    pub const FILTERWHEEL_CHANGED: &str = "FILTERWHEEL-CHANGED";
    pub const MOUNT_DISCONNECTED: &str = "MOUNT-DISCONNECTED";
    pub const MOUNT_CONNECTED: &str = "MOUNT-CONNECTED";
    pub const MOUNT_UNPARKED: &str = "MOUNT-UNPARKED";
    pub const MOUNT_PARKED: &str = "MOUNT-PARKED";
    pub const MOUNT_BEFORE_FLIP: &str = "MOUNT-BEFORE-FLIP";
    pub const MOUNT_AFTER_FLIP: &str = "MOUNT-AFTER-FLIP";
    pub const FOCUSER_DISCONNECTED: &str = "FOCUSER-DISCONNECTED";
    pub const FOCUSER_CONNECTED: &str = "FOCUSER-CONNECTED";
    pub const ROTATOR_DISCONNECTED: &str = "ROTATOR-DISCONNECTED";
    pub const ROTATOR_CONNECTED: &str = "ROTATOR-CONNECTED";
    pub const GUIDER_CONNECTED: &str = "GUIDER-CONNECTED";
    pub const GUIDER_DISCONNECTED: &str = "GUIDER-DISCONNECTED";
    pub const FLAT_DISCONNECTED: &str = "FLAT-DISCONNECTED";
    pub const WEATHER_DISCONNECTED: &str = "WEATHER-DISCONNECTED";
    pub const SWITCH_DISCONNECTED: &str = "SWITCH-DISCONNECTED";
    pub const DOME_DISCONNECTED: &str = "DOME-DISCONNECTED";
    pub const SAFETY_DISCONNECTED: &str = "SAFETY-DISCONNECTED";
    pub const IMAGE_SAVE: &str = "IMAGE-SAVE";
    pub const AUTOFOCUS_FINISHED: &str = "AUTOFOCUS-FINISHED";
}

impl EventHistoryResponse {
    /// Get all events of a specific type
    pub fn get_events_by_type(&self, event_type: &str) -> Vec<&Event> {
        self.response
            .iter()
            .filter(|event| event.event == event_type)
            .collect()
    }

    /// Get all filter wheel change events
    pub fn get_filterwheel_changes(&self) -> Vec<&Event> {
        self.get_events_by_type(event_types::FILTERWHEEL_CHANGED)
    }

    /// Get all image save events
    pub fn get_image_saves(&self) -> Vec<&Event> {
        self.get_events_by_type(event_types::IMAGE_SAVE)
    }

    /// Get connection events (connected/disconnected)
    pub fn get_connection_events(&self) -> Vec<&Event> {
        self.response
            .iter()
            .filter(|event| {
                event.event.ends_with("-CONNECTED") || event.event.ends_with("-DISCONNECTED")
            })
            .collect()
    }

    /// Get events in a time range (assuming ISO 8601 timestamps)
    pub fn get_events_in_range(&self, start: &str, end: &str) -> Vec<&Event> {
        self.response
            .iter()
            .filter(|event| event.time.as_str() >= start && event.time.as_str() <= end)
            .collect()
    }

    /// Count events by type
    pub fn count_events_by_type(&self) -> std::collections::HashMap<String, usize> {
        let mut counts = std::collections::HashMap::new();
        for event in &self.response {
            *counts.entry(event.event.clone()).or_insert(0) += 1;
        }
        counts
    }
}

impl Event {
    /// Check if this is a connection event
    pub fn is_connection_event(&self) -> bool {
        self.event.ends_with("-CONNECTED") || self.event.ends_with("-DISCONNECTED")
    }

    /// Check if this is a disconnection event
    pub fn is_disconnection(&self) -> bool {
        self.event.ends_with("-DISCONNECTED")
    }

    /// Check if this is a connection event
    pub fn is_connection(&self) -> bool {
        self.event.ends_with("-CONNECTED")
    }

    /// Get the equipment name from the event (e.g., "CAMERA" from "CAMERA-CONNECTED")
    pub fn get_equipment_name(&self) -> Option<&str> {
        if self.is_connection_event() {
            if let Some(pos) = self.event.rfind('-') {
                return Some(&self.event[..pos]);
            }
        }
        None
    }

    /// Parse timestamp as std::time::SystemTime
    pub fn parse_timestamp(&self) -> Result<std::time::SystemTime, Box<dyn std::error::Error>> {
        // For now, just return current time - full parsing would need a date parsing library
        Ok(std::time::SystemTime::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_event_parsing() {
        let event_json = r#"{
            "Time": "2025-08-06T21:50:56.545923-07:00",
            "Event": "AUTOFOCUS-FINISHED"
        }"#;

        let event: Event = serde_json::from_str(event_json).unwrap();
        assert_eq!(event.time, "2025-08-06T21:50:56.545923-07:00");
        assert_eq!(event.event, "AUTOFOCUS-FINISHED");
        assert!(event.details.is_none());
    }

    #[test]
    fn test_filterwheel_change_event() {
        let event_json = r#"{
            "Time": "2025-08-06T19:18:09.4045633-07:00",
            "New": {"Name": "HA", "Id": 0},
            "Previous": {"Name": "OIII", "Id": 1},
            "Event": "FILTERWHEEL-CHANGED"
        }"#;

        let event: Event = serde_json::from_str(event_json).unwrap();
        assert_eq!(event.event, "FILTERWHEEL-CHANGED");

        if let Some(EventDetails::FilterWheelChange { new, previous }) = event.details {
            assert_eq!(new.name, "HA");
            assert_eq!(new.id, 0);
            assert_eq!(previous.name, "OIII");
            assert_eq!(previous.id, 1);
        } else {
            panic!("Expected FilterWheelChange details");
        }
    }

    #[test]
    fn test_event_methods() {
        let camera_connected = Event {
            time: "2025-08-06T18:45:40.1430956-07:00".to_string(),
            event: "CAMERA-CONNECTED".to_string(),
            details: None,
        };

        assert!(camera_connected.is_connection_event());
        assert!(camera_connected.is_connection());
        assert!(!camera_connected.is_disconnection());
        assert_eq!(camera_connected.get_equipment_name(), Some("CAMERA"));

        let mount_disconnected = Event {
            time: "2025-08-06T19:20:35.2068582-07:00".to_string(),
            event: "MOUNT-DISCONNECTED".to_string(),
            details: None,
        };

        assert!(mount_disconnected.is_connection_event());
        assert!(!mount_disconnected.is_connection());
        assert!(mount_disconnected.is_disconnection());
        assert_eq!(mount_disconnected.get_equipment_name(), Some("MOUNT"));
    }

    #[test]
    fn test_event_history_methods() {
        let events_json = r#"{
            "Response": [
                {
                    "Time": "2025-08-06T19:18:39.2067156-07:00",
                    "Event": "IMAGE-SAVE"
                },
                {
                    "Time": "2025-08-06T19:18:09.4045633-07:00",
                    "New": {"Name": "HA", "Id": 0},
                    "Previous": {"Name": "OIII", "Id": 1},
                    "Event": "FILTERWHEEL-CHANGED"
                },
                {
                    "Time": "2025-08-06T21:50:56.545923-07:00",
                    "Event": "AUTOFOCUS-FINISHED"
                },
                {
                    "Time": "2025-08-06T18:45:40.1430956-07:00",
                    "Event": "CAMERA-CONNECTED"
                }
            ],
            "Error": "",
            "StatusCode": 200,
            "Success": true,
            "Type": "API"
        }"#;

        let events: EventHistoryResponse = serde_json::from_str(events_json).unwrap();

        // Test filtering by type
        let image_saves = events.get_image_saves();
        assert_eq!(image_saves.len(), 1);
        assert_eq!(image_saves[0].event, "IMAGE-SAVE");

        let filter_changes = events.get_filterwheel_changes();
        assert_eq!(filter_changes.len(), 1);
        assert_eq!(filter_changes[0].event, "FILTERWHEEL-CHANGED");

        let connection_events = events.get_connection_events();
        assert_eq!(connection_events.len(), 1);
        assert_eq!(connection_events[0].event, "CAMERA-CONNECTED");

        // Test autofocus events
        let autofocus_events = events.get_events_by_type(event_types::AUTOFOCUS_FINISHED);
        assert_eq!(autofocus_events.len(), 1);
        assert_eq!(autofocus_events[0].event, "AUTOFOCUS-FINISHED");

        // Test counting
        let counts = events.count_events_by_type();
        assert_eq!(counts.get("IMAGE-SAVE"), Some(&1));
        assert_eq!(counts.get("FILTERWHEEL-CHANGED"), Some(&1));
        assert_eq!(counts.get("AUTOFOCUS-FINISHED"), Some(&1));
        assert_eq!(counts.get("CAMERA-CONNECTED"), Some(&1));
    }

    #[test]
    fn test_load_event_history_from_file() {
        // Test loading the example event history file if it exists
        if let Ok(json_content) = std::fs::read_to_string("example_event-history.json") {
            let events: Result<EventHistoryResponse, _> = serde_json::from_str(&json_content);
            assert!(
                events.is_ok(),
                "Should be able to parse example_event-history.json"
            );

            let events = events.unwrap();
            assert!(events.success, "Events should indicate success");
            assert_eq!(events.status_code, 200, "Should have status code 200");
            assert!(!events.response.is_empty(), "Should have events");

            println!("Found {} events in example file", events.response.len());

            // Test event analysis
            let counts = events.count_events_by_type();
            println!("Event type counts: {:?}", counts);

            let filter_changes = events.get_filterwheel_changes();
            println!("Found {} filter wheel changes", filter_changes.len());

            let image_saves = events.get_image_saves();
            println!("Found {} image saves", image_saves.len());

            let autofocus_events = events.get_events_by_type(event_types::AUTOFOCUS_FINISHED);
            println!("Found {} autofocus events", autofocus_events.len());

            // Test time range filtering (get first and last event times)
            if events.response.len() > 1 {
                let first_time = &events.response[0].time;
                let last_time = &events.response[events.response.len() - 1].time;
                let range_events = events.get_events_in_range(first_time, last_time);
                assert_eq!(range_events.len(), events.response.len());
            }
        } else {
            println!("example_event-history.json not found, skipping file test");
        }
    }
}
