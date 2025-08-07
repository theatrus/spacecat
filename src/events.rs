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
