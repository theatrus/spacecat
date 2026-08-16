use base64::Engine;
use chrono::{DateTime as ChronoDateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SequenceResponse {
    pub response: Vec<Value>,
    pub error: String,
    pub status_code: i32,
    pub success: bool,
    #[serde(rename = "Type")]
    pub response_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Container {
    pub status: String,
    pub items: Vec<Item>,
    pub triggers: Vec<Trigger>,
    pub conditions: Vec<Condition>,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Item {
    pub status: String,
    pub name: String,
    #[serde(flatten)]
    pub extra: Value,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Coordinates {
    #[serde(rename = "RA")]
    pub ra: f64,
    #[serde(rename = "RAString")]
    pub ra_string: String,
    #[serde(rename = "RADegrees")]
    pub ra_degrees: f64,
    pub dec: f64,
    pub dec_string: String,
    pub epoch: String,
    #[serde(rename = "DateTime")]
    pub date_time: DateTime,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DateTime {
    pub now: String,
    pub utc_now: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Binning {
    pub name: String,
    #[serde(rename = "X")]
    pub x: i32,
    #[serde(rename = "Y")]
    pub y: i32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Trigger {
    pub status: String,
    pub name: String,
    #[serde(flatten)]
    pub extra: Value,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Condition {
    pub status: String,
    pub name: String,
    #[serde(flatten)]
    pub extra: Value,
}

// More specific types for when you need them
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GlobalTriggers {
    pub global_triggers: Vec<Trigger>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SmartExposure {
    pub status: String,
    pub iterations: i32,
    #[serde(rename = "Type")]
    pub exposure_type: String,
    pub exposure_time: i32,
    pub dither_progress_exposures: i32,
    pub dither_target_exposures: i32,
    pub gain: i32,
    pub exposure_count: i32,
    pub binning: Binning,
    pub offset: i32,
    pub filter: String,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CoolCamera {
    pub status: String,
    pub min_cooling_time: i32,
    pub temperature: i32,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SlewCenterRotate {
    pub status: String,
    pub coordinates: Coordinates,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StartGuiding {
    pub status: String,
    pub force_calibration: bool,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Annotation {
    pub status: String,
    pub text: String,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct WarmCamera {
    pub status: String,
    pub min_warming_time: i32,
    pub name: String,
}

// Additional trigger types seen in the updated JSON
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DitherTrigger {
    pub status: String,
    pub target_exposures: i32,
    pub exposures: i32,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AltitudeCondition {
    pub status: String,
    pub current_altitude: f64,
    pub altitude: f64,
    pub expected_time: String,
    pub name: String,
}

/// A long-running sequence operation that benefits from chat progress updates.
#[derive(Debug, Clone, PartialEq)]
pub struct SequenceOperation {
    /// Stable position within the sequence tree for correlating adjacent polls.
    pub key: String,
    pub name: String,
    pub status: String,
    pub kind: SequenceOperationKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SequenceOperationKind {
    CameraCooling {
        target_temperature: f64,
        minimum_duration: Option<chrono::Duration>,
    },
    TimeWait {
        target_time: Option<ChronoDateTime<FixedOffset>>,
        configured_duration: Option<chrono::Duration>,
    },
    MountSlew {
        coordinates: Option<OperationCoordinates>,
        /// Older Advanced API payloads expose coordinates but not the
        /// concrete sequence-item type. A nearby MOUNT-CENTER event can
        /// safely promote one of these otherwise-slew operations.
        may_be_center: bool,
    },
    MountCenter {
        coordinates: Option<OperationCoordinates>,
        rotation: Option<f64>,
        output: Option<Box<PlateSolveOutput>>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct OperationCoordinates {
    pub ra_hours: Option<f64>,
    pub ra_string: Option<String>,
    pub dec_degrees: Option<f64>,
    pub dec_string: Option<String>,
    pub epoch: Option<String>,
    pub altitude_degrees: Option<f64>,
    pub azimuth_degrees: Option<f64>,
}

impl OperationCoordinates {
    pub fn display(&self) -> String {
        if self.altitude_degrees.is_some() || self.azimuth_degrees.is_some() {
            return format!(
                "Alt {} · Az {}",
                self.altitude_degrees
                    .map(|value| format!("{value:.2}°"))
                    .unwrap_or_else(|| "--".to_string()),
                self.azimuth_degrees
                    .map(|value| format!("{value:.2}°"))
                    .unwrap_or_else(|| "--".to_string())
            );
        }

        let ra = self.ra_string.clone().unwrap_or_else(|| {
            self.ra_hours
                .map(|value| format!("{value:.5} h"))
                .unwrap_or_else(|| "--".to_string())
        });
        let dec = self.dec_string.clone().unwrap_or_else(|| {
            self.dec_degrees
                .map(|value| format!("{value:.5}°"))
                .unwrap_or_else(|| "--".to_string())
        });
        match self.epoch.as_deref() {
            Some(epoch) => format!("RA {ra} · Dec {dec} ({epoch})"),
            None => format!("RA {ra} · Dec {dec}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlateSolveOutput {
    pub solve_time: Option<String>,
    pub success: Option<bool>,
    pub coordinates: Option<OperationCoordinates>,
    pub position_angle: Option<f64>,
    pub pixel_scale: Option<f64>,
    pub radius_degrees: Option<f64>,
    pub separation_arcseconds: Option<f64>,
    pub ra_error: Option<String>,
    pub dec_error: Option<String>,
    pub ra_pixel_error: Option<f64>,
    pub dec_pixel_error: Option<f64>,
    pub flipped: Option<bool>,
    pub thumbnail: Option<Vec<u8>>,
    pub thumbnail_media_type: Option<String>,
}

impl SequenceOperation {
    pub fn is_active(&self) -> bool {
        matches!(
            self.status.to_ascii_uppercase().as_str(),
            "RUNNING" | "ACTIVE"
        )
    }

    pub fn is_failed(&self) -> bool {
        matches!(
            self.status.to_ascii_uppercase().as_str(),
            "FAILED" | "ABORTED" | "CANCELLED" | "CANCELED"
        )
    }
}

/// Find chat-visible, long-running sequence items without depending on
/// localized display names. Direct snapshots supply `OperationKind`; older
/// plugin and Advanced API payloads are recognized from their stable fields.
pub fn extract_sequence_operations(sequence: &SequenceResponse) -> Vec<SequenceOperation> {
    fn visit_items(values: &[Value], parent: &str, output: &mut Vec<SequenceOperation>) {
        for (index, value) in values.iter().enumerate() {
            let Some(object) = value.as_object() else {
                continue;
            };
            let key = if parent.is_empty() {
                index.to_string()
            } else {
                format!("{parent}/{index}")
            };
            let name = object
                .get("Name")
                .and_then(Value::as_str)
                .unwrap_or("Sequence operation")
                .to_string();
            let status = object
                .get("Status")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let explicit_kind = object.get("OperationKind").and_then(Value::as_str);

            let is_cooling = explicit_kind == Some("camera_cooling")
                || (object.contains_key("Temperature") && object.contains_key("MinCoolingTime"));
            let is_wait = explicit_kind == Some("time_wait")
                || object.contains_key("CalculatedWaitDuration")
                || object.contains_key("Delay");
            let has_coordinates =
                object.contains_key("Coordinates") && !name.ends_with("_Container");
            let lower_name = name.to_ascii_lowercase();
            let name_is_center = lower_name.contains("center")
                || lower_name.contains("centr")
                || lower_name.contains("zentrier");
            let is_center = explicit_kind == Some("mount_center")
                || (has_coordinates && (object.contains_key("Rotation") || name_is_center));
            let is_slew = explicit_kind == Some("mount_slew") || (has_coordinates && !is_center);

            if is_cooling {
                if let Some(target_temperature) = object.get("Temperature").and_then(value_as_f64) {
                    output.push(SequenceOperation {
                        key: key.clone(),
                        name: name.clone(),
                        status: status.clone(),
                        kind: SequenceOperationKind::CameraCooling {
                            target_temperature,
                            minimum_duration: object
                                .get("MinCoolingTime")
                                .and_then(parse_minutes_or_timespan),
                        },
                    });
                }
            } else if is_wait {
                let configured_duration = object
                    .get("CalculatedWaitDuration")
                    .and_then(parse_duration_value)
                    .or_else(|| object.get("Delay").and_then(parse_duration_value));
                let target_time = object
                    .get("TargetTime")
                    .and_then(Value::as_str)
                    .and_then(parse_target_time);
                if configured_duration.is_some() || target_time.is_some() {
                    output.push(SequenceOperation {
                        key: key.clone(),
                        name: name.clone(),
                        status: status.clone(),
                        kind: SequenceOperationKind::TimeWait {
                            target_time,
                            configured_duration,
                        },
                    });
                }
            } else if is_center {
                output.push(SequenceOperation {
                    key: key.clone(),
                    name: name.clone(),
                    status: status.clone(),
                    kind: SequenceOperationKind::MountCenter {
                        coordinates: object
                            .get("Coordinates")
                            .and_then(parse_operation_coordinates),
                        rotation: object.get("Rotation").and_then(value_as_f64),
                        output: object
                            .get("PlateSolveOutput")
                            .and_then(parse_plate_solve_output)
                            .map(Box::new),
                    },
                });
            } else if is_slew {
                output.push(SequenceOperation {
                    key: key.clone(),
                    name: name.clone(),
                    status: status.clone(),
                    kind: SequenceOperationKind::MountSlew {
                        coordinates: object
                            .get("Coordinates")
                            .and_then(parse_operation_coordinates),
                        may_be_center: explicit_kind.is_none(),
                    },
                });
            }

            if let Some(items) = object.get("Items").and_then(Value::as_array) {
                visit_items(items, &key, output);
            }
        }
    }

    let mut operations = Vec::new();
    visit_items(&sequence.response, "", &mut operations);
    operations
}

fn parse_operation_coordinates(value: &Value) -> Option<OperationCoordinates> {
    let object = value.as_object()?;
    let angle = |name: &str| {
        object.get(name).and_then(|value| {
            value_as_f64(value).or_else(|| value.get("Degree").and_then(value_as_f64))
        })
    };
    let coordinates = OperationCoordinates {
        ra_hours: object.get("RA").and_then(value_as_f64),
        ra_string: object
            .get("RAString")
            .and_then(Value::as_str)
            .map(str::to_string),
        dec_degrees: object.get("Dec").and_then(value_as_f64),
        dec_string: object
            .get("DecString")
            .and_then(Value::as_str)
            .map(str::to_string),
        epoch: object.get("Epoch").map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| value.to_string())
        }),
        altitude_degrees: angle("Altitude"),
        azimuth_degrees: angle("Azimuth"),
    };
    (coordinates.ra_hours.is_some()
        || coordinates.dec_degrees.is_some()
        || coordinates.altitude_degrees.is_some()
        || coordinates.azimuth_degrees.is_some()
        || coordinates.ra_string.is_some()
        || coordinates.dec_string.is_some())
    .then_some(coordinates)
}

fn parse_plate_solve_output(value: &Value) -> Option<PlateSolveOutput> {
    const MAX_THUMBNAIL_BASE64_LEN: usize = 8 * 1024 * 1024;
    let object = value.as_object()?;
    let thumbnail = object
        .get("ThumbnailBase64")
        .and_then(Value::as_str)
        .filter(|encoded| encoded.len() <= MAX_THUMBNAIL_BASE64_LEN)
        .and_then(|encoded| {
            base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .ok()
        });
    let output = PlateSolveOutput {
        solve_time: object
            .get("SolveTime")
            .and_then(Value::as_str)
            .map(str::to_string),
        success: object.get("Success").and_then(Value::as_bool),
        coordinates: object
            .get("Coordinates")
            .and_then(parse_operation_coordinates),
        position_angle: object.get("PositionAngle").and_then(value_as_f64),
        pixel_scale: object.get("PixelScale").and_then(value_as_f64),
        radius_degrees: object.get("RadiusDegrees").and_then(value_as_f64),
        separation_arcseconds: object.get("SeparationArcseconds").and_then(value_as_f64),
        ra_error: object
            .get("RaError")
            .and_then(Value::as_str)
            .map(str::to_string),
        dec_error: object
            .get("DecError")
            .and_then(Value::as_str)
            .map(str::to_string),
        ra_pixel_error: object.get("RaPixelError").and_then(value_as_f64),
        dec_pixel_error: object.get("DecPixelError").and_then(value_as_f64),
        flipped: object.get("Flipped").and_then(Value::as_bool),
        thumbnail,
        thumbnail_media_type: object
            .get("ThumbnailMediaType")
            .and_then(Value::as_str)
            .map(str::to_string),
    };
    (output.solve_time.is_some()
        || output.success.is_some()
        || output.coordinates.is_some()
        || output.thumbnail.is_some())
    .then_some(output)
}

fn value_as_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.parse::<f64>().ok())
        .filter(|value| value.is_finite())
}

fn parse_target_time(value: &str) -> Option<ChronoDateTime<FixedOffset>> {
    ChronoDateTime::parse_from_rfc3339(value).ok()
}

fn parse_duration_value(value: &Value) -> Option<chrono::Duration> {
    if let Some(seconds) = value_as_f64(value) {
        return duration_from_seconds(seconds);
    }
    parse_timespan(value.as_str()?)
}

fn parse_minutes_or_timespan(value: &Value) -> Option<chrono::Duration> {
    if let Some(minutes) = value_as_f64(value) {
        return duration_from_seconds(minutes * 60.0);
    }
    parse_timespan(value.as_str()?)
}

fn duration_from_seconds(seconds: f64) -> Option<chrono::Duration> {
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    let milliseconds = (seconds * 1000.0).round();
    if milliseconds > i64::MAX as f64 {
        return None;
    }
    chrono::Duration::try_milliseconds(milliseconds as i64)
}

/// Parse the invariant TimeSpan strings emitted by System.Text.Json and
/// Newtonsoft.Json: `[d.]hh:mm:ss[.fffffff]`.
fn parse_timespan(value: &str) -> Option<chrono::Duration> {
    let (days, clock) = match value.split_once('.') {
        Some((head, tail)) if !head.contains(':') => (head.parse::<i64>().ok()?, tail),
        _ => (0, value),
    };
    let mut fields = clock.split(':');
    let hours = fields.next()?.parse::<i64>().ok()?;
    let minutes = fields.next()?.parse::<i64>().ok()?;
    let seconds = fields.next()?.parse::<f64>().ok()?;
    if fields.next().is_some()
        || hours < 0
        || !(0..60).contains(&minutes)
        || !(0.0..60.0).contains(&seconds)
    {
        return None;
    }
    let whole_seconds = days
        .checked_mul(24)?
        .checked_add(hours)?
        .checked_mul(60)?
        .checked_add(minutes)?
        .checked_mul(60)?;
    duration_from_seconds(whole_seconds as f64 + seconds)
}

impl SequenceResponse {
    /// Get global triggers from the first item if it exists
    pub fn get_global_triggers(&self) -> Option<GlobalTriggers> {
        self.response
            .first()?
            .as_object()?
            .get("GlobalTriggers")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .map(|triggers| GlobalTriggers {
                global_triggers: triggers,
            })
    }

    /// Get all containers from the response
    pub fn get_containers(&self) -> Vec<Container> {
        self.response
            .iter()
            .skip(1) // Skip global triggers
            .filter_map(|item| serde_json::from_value(item.clone()).ok())
            .collect()
    }
}

impl Container {
    /// Get items of a specific type from this container
    pub fn get_items_by_name(&self, name: &str) -> Vec<&Item> {
        self.items
            .iter()
            .filter(|item| item.name.contains(name))
            .collect()
    }

    /// Try to parse an item as a specific type
    pub fn parse_item<T>(&self, item: &Item) -> Option<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let mut value = serde_json::to_value(item).ok()?;
        if let Some(obj) = value.as_object_mut() {
            // Merge the extra fields
            if let Some(extra_obj) = item.extra.as_object() {
                for (k, v) in extra_obj {
                    obj.insert(k.clone(), v.clone());
                }
            }
        }
        serde_json::from_value(value).ok()
    }
}

/// Extract the current target name from a sequence response
///
/// This function looks for active or running containers that represent observation targets.
/// Target containers are identified by having "_Container" suffix in their names.
/// The suffix is removed from the returned target name.
///
/// # Arguments
/// * `sequence` - The sequence response to analyze
///
/// # Returns
/// * `Some(String)` - The current target name without "_Container" suffix
/// * `None` - If no active target is found
pub fn extract_current_target(sequence: &SequenceResponse) -> Option<String> {
    // Recursively search through all JSON objects for active target containers
    fn search_containers(values: &[Value]) -> Option<String> {
        for value in values {
            if let Some(obj) = value.as_object() {
                // Try to extract data directly from the JSON object
                if let (Some(name), Some(status)) = (
                    obj.get("Name").and_then(|v| v.as_str()),
                    obj.get("Status").and_then(|v| v.as_str()),
                ) {
                    if (status == "RUNNING" || status == "Active")
                        && name.ends_with("_Container")
                        && !is_system_container(name)
                    {
                        // Remove the "_Container" suffix to get the target name
                        let target_name = name.strip_suffix("_Container").unwrap_or(name);

                        if !target_name.is_empty() {
                            return Some(target_name.to_string());
                        }
                    }

                    // Also search nested items
                    if let Some(items) = obj.get("Items").and_then(|v| v.as_array())
                        && let Some(nested_target) = search_containers(items)
                    {
                        return Some(nested_target);
                    }
                }
            }
        }
        None
    }

    search_containers(&sequence.response)
}

/// Extract the meridian flip time from a sequence response
///
/// This function looks for the "Meridian Flip_Trigger" in the GlobalTriggers section
/// and extracts the TimeToFlip value, which represents the time until meridian flip in hours.
///
/// # Arguments
/// * `sequence` - The sequence response to analyze
///
/// # Returns
/// * `Some(f64)` - The time to meridian flip in hours
/// * `None` - If no meridian flip trigger is found or TimeToFlip is not available
pub fn extract_meridian_flip_time(sequence: &SequenceResponse) -> Option<f64> {
    // Get global triggers from the first item
    let global_triggers_item = sequence.response.first()?;
    let global_triggers_array = global_triggers_item
        .as_object()?
        .get("GlobalTriggers")?
        .as_array()?;

    // Search for the Meridian Flip trigger
    for trigger in global_triggers_array {
        if let Some(trigger_obj) = trigger.as_object()
            && let Some(name) = trigger_obj.get("Name").and_then(|v| v.as_str())
            && name == "Meridian Flip_Trigger"
        {
            // Extract TimeToFlip value
            if let Some(time_to_flip) = trigger_obj.get("TimeToFlip").and_then(|v| v.as_f64()) {
                return Some(time_to_flip);
            }
        }
    }

    None
}

/// Convert meridian flip time from hours to minutes
pub fn meridian_flip_time_minutes(hours: f64) -> f64 {
    hours * 60.0
}

/// Convert meridian flip time from hours to hours:minutes format string
pub fn meridian_flip_time_formatted(hours: f64) -> String {
    let total_minutes = (hours * 60.0) as i32;
    let hrs = total_minutes / 60;
    let mins = total_minutes % 60;
    format!("{hrs:02}:{mins:02}")
}

/// Convert meridian flip time from hours to a detailed format string with wall-clock time
pub fn meridian_flip_time_formatted_with_clock(hours: f64) -> String {
    let total_minutes = (hours * 60.0) as i32;
    let hrs = total_minutes / 60;
    let mins = total_minutes % 60;
    let duration_str = format!("{hrs:02}:{mins:02}");

    // Calculate wall-clock time when meridian flip will occur
    let now = chrono::Utc::now();
    let meridian_flip_time = now + chrono::Duration::seconds((hours * 3600.0) as i64);

    // Format in local timezone for better readability
    let local_flip_time = meridian_flip_time.with_timezone(&chrono::Local);
    let clock_time = local_flip_time.format("%H:%M:%S").to_string();

    format!("{duration_str} (at {clock_time})")
}

/// Check if a container name represents a system container rather than a target
fn is_system_container(name: &str) -> bool {
    let system_containers = [
        "Start_Container",
        "End_Container",
        "Targets_Container",
        "Basic Sequence Startup_Container",
        "Basic Sequence End_Container",
        "Target Imaging Instructions_Container",
        "Parallel End of Sequence Instructions_Container",
    ];

    system_containers
        .iter()
        .any(|&sys_name| name.contains(sys_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_current_target() {
        // Test data representing the example sequence structure
        let sequence_json = r#"{
            "Response": [
                {
                    "GlobalTriggers": []
                },
                {
                    "Name": "Start_Container",
                    "Status": "FINISHED",
                    "Items": [],
                    "Triggers": [],
                    "Conditions": []
                },
                {
                    "Name": "Targets_Container", 
                    "Status": "RUNNING",
                    "Items": [
                        {
                            "Name": "Sh2 101_Container",
                            "Status": "RUNNING",
                            "Items": [],
                            "Triggers": [],
                            "Conditions": []
                        },
                        {
                            "Name": "Triangulum Pinwheel_Container",
                            "Status": "CREATED",
                            "Items": [],
                            "Triggers": [],
                            "Conditions": []
                        }
                    ],
                    "Triggers": [],
                    "Conditions": []
                },
                {
                    "Name": "End_Container",
                    "Status": "CREATED", 
                    "Items": [],
                    "Triggers": [],
                    "Conditions": []
                }
            ],
            "Error": "",
            "StatusCode": 200,
            "Success": true,
            "Type": "API"
        }"#;

        let sequence: SequenceResponse = serde_json::from_str(sequence_json).unwrap();

        // The function should extract "Sh2 101" from "Sh2 101_Container" since it has RUNNING status
        let target = extract_current_target(&sequence);
        assert_eq!(target, Some("Sh2 101".to_string()));
    }

    #[test]
    fn test_extract_current_target_no_active_target() {
        let sequence_json = r#"{
            "Response": [
                {
                    "GlobalTriggers": []
                },
                {
                    "Name": "Start_Container",
                    "Status": "FINISHED",
                    "Items": [],
                    "Triggers": [],
                    "Conditions": []
                },
                {
                    "Name": "Targets_Container", 
                    "Status": "CREATED",
                    "Items": [],
                    "Triggers": [],
                    "Conditions": []
                }
            ],
            "Error": "",
            "StatusCode": 200,
            "Success": true,
            "Type": "API"
        }"#;

        let sequence: SequenceResponse = serde_json::from_str(sequence_json).unwrap();
        let target = extract_current_target(&sequence);
        assert_eq!(target, None);
    }

    #[test]
    fn test_extract_current_target_triangulum_pinwheel() {
        let sequence_json = r#"{
            "Response": [
                {
                    "GlobalTriggers": []
                },
                {
                    "Name": "Targets_Container", 
                    "Status": "RUNNING",
                    "Items": [
                        {
                            "Name": "Sh2 101_Container",
                            "Status": "FINISHED",
                            "Items": [],
                            "Triggers": [],
                            "Conditions": []
                        },
                        {
                            "Name": "Triangulum Pinwheel_Container",
                            "Status": "RUNNING",
                            "Items": [],
                            "Triggers": [],
                            "Conditions": []
                        }
                    ],
                    "Triggers": [],
                    "Conditions": []
                }
            ],
            "Error": "",
            "StatusCode": 200,
            "Success": true,
            "Type": "API"
        }"#;

        let sequence: SequenceResponse = serde_json::from_str(sequence_json).unwrap();
        let target = extract_current_target(&sequence);
        assert_eq!(target, Some("Triangulum Pinwheel".to_string()));
    }

    #[test]
    fn test_is_system_container() {
        assert!(is_system_container("Start_Container"));
        assert!(is_system_container("End_Container"));
        assert!(is_system_container("Targets_Container"));
        assert!(is_system_container("Basic Sequence Startup_Container"));
        assert!(is_system_container("Target Imaging Instructions_Container"));

        assert!(!is_system_container("Sh2 101_Container"));
        assert!(!is_system_container("Triangulum Pinwheel_Container"));
        assert!(!is_system_container("M31_Container"));
    }

    #[test]
    fn test_load_sequence_from_file() {
        // Test loading the example sequence file if it exists
        if let Ok(json_content) = std::fs::read_to_string("example_sequence.json") {
            let sequence: Result<SequenceResponse, _> = serde_json::from_str(&json_content);
            assert!(
                sequence.is_ok(),
                "Should be able to parse example_sequence.json"
            );

            let sequence = sequence.unwrap();
            assert!(sequence.success, "Sequence should indicate success");
            assert_eq!(sequence.status_code, 200, "Should have status code 200");
            assert!(!sequence.response.is_empty(), "Should have response items");

            // Test target extraction from real file
            let target = extract_current_target(&sequence);
            println!("Found target in example file: {target:?}");

            // Test container extraction
            let containers = sequence.get_containers();
            println!("Found {} containers in example file", containers.len());

            // Test global triggers
            if let Some(triggers) = sequence.get_global_triggers() {
                println!("Found {} global triggers", triggers.global_triggers.len());
            }

            // Test meridian flip time extraction from real file
            let meridian_flip_time = extract_meridian_flip_time(&sequence);
            println!("Found meridian flip time in example file: {meridian_flip_time:?}");

            if let Some(time_hours) = meridian_flip_time {
                let time_minutes = meridian_flip_time_minutes(time_hours);
                let time_formatted = meridian_flip_time_formatted(time_hours);
                println!(
                    "Meridian flip in {time_hours:.3} hours ({time_minutes:.1} minutes, {time_formatted})"
                );
            }
        } else {
            println!("example_sequence.json not found, skipping file test");
        }
    }

    #[test]
    fn test_extract_meridian_flip_time() {
        // Test data with meridian flip trigger
        let sequence_json = r#"{
            "Response": [
                {
                    "GlobalTriggers": [
                        {
                            "Name": "Meridian Flip_Trigger",
                            "TimeToFlip": 1.3464521451944444,
                            "Status": "CREATED"
                        },
                        {
                            "Name": "AF After HFR Increase_Trigger",
                            "Status": "CREATED",
                            "DeltaHFR": 10
                        }
                    ]
                },
                {
                    "Name": "Start_Container",
                    "Status": "FINISHED",
                    "Items": [],
                    "Triggers": [],
                    "Conditions": []
                }
            ],
            "Error": "",
            "StatusCode": 200,
            "Success": true,
            "Type": "API"
        }"#;

        let sequence: SequenceResponse = serde_json::from_str(sequence_json).unwrap();
        let flip_time = extract_meridian_flip_time(&sequence);

        assert!(flip_time.is_some());
        let time_hours = flip_time.unwrap();
        assert!((time_hours - 1.3464521451944444).abs() < 0.0001);
    }

    #[test]
    fn test_extract_meridian_flip_time_no_trigger() {
        // Test data without meridian flip trigger
        let sequence_json = r#"{
            "Response": [
                {
                    "GlobalTriggers": [
                        {
                            "Name": "AF After HFR Increase_Trigger",
                            "Status": "CREATED",
                            "DeltaHFR": 10
                        }
                    ]
                }
            ],
            "Error": "",
            "StatusCode": 200,
            "Success": true,
            "Type": "API"
        }"#;

        let sequence: SequenceResponse = serde_json::from_str(sequence_json).unwrap();
        let flip_time = extract_meridian_flip_time(&sequence);

        assert!(flip_time.is_none());
    }

    #[test]
    fn extracts_direct_cooling_and_wait_operations_recursively() {
        let sequence: SequenceResponse = serde_json::from_value(serde_json::json!({
            "Response": [
                {"GlobalTriggers": []},
                {
                    "Name": "Startup_Container",
                    "Status": "RUNNING",
                    "Items": [
                        {
                            "Name": "Cool camera",
                            "Status": "RUNNING",
                            "OperationKind": "camera_cooling",
                            "Temperature": -10.0,
                            "MinCoolingTime": "00:15:00"
                        },
                        {
                            "Name": "Wait for time span",
                            "Status": "RUNNING",
                            "OperationKind": "time_wait",
                            "Delay": 300,
                            "CalculatedWaitDuration": "00:05:00"
                        }
                    ]
                }
            ],
            "Error": "",
            "StatusCode": 200,
            "Success": true,
            "Type": "API"
        }))
        .unwrap();

        let operations = extract_sequence_operations(&sequence);
        assert_eq!(operations.len(), 2);
        assert!(operations.iter().all(SequenceOperation::is_active));
        assert!(matches!(
            operations[0].kind,
            SequenceOperationKind::CameraCooling {
                target_temperature: -10.0,
                minimum_duration: Some(duration)
            } if duration == chrono::Duration::minutes(15)
        ));
        assert!(matches!(
            operations[1].kind,
            SequenceOperationKind::TimeWait {
                target_time: None,
                configured_duration: Some(duration)
            } if duration == chrono::Duration::minutes(5)
        ));
    }

    #[test]
    fn recognizes_advanced_api_fields_but_ignores_timed_conditions() {
        let sequence: SequenceResponse = serde_json::from_value(serde_json::json!({
            "Response": [{
                "Name": "Target_Container",
                "Status": "RUNNING",
                "Items": [{
                    "Name": "CoolCamera",
                    "Status": "RUNNING",
                    "Temperature": "-15",
                    "MinCoolingTime": 10
                }, {
                    "Name": "WaitForTime",
                    "Status": "RUNNING",
                    "CalculatedWaitDuration": "00:01:30.5000000",
                    "TargetTime": "2026-08-16T01:02:03-07:00"
                }, {
                    "Name": "Attendre une durée",
                    "Status": "RUNNING",
                    "Delay": 45
                }],
                "Conditions": [{
                    "Name": "Time span condition",
                    "Status": "RUNNING",
                    "RemainingTime": "00:30:00",
                    "TargetTime": "2026-08-16T02:00:00-07:00"
                }]
            }],
            "Error": "",
            "StatusCode": 200,
            "Success": true,
            "Type": "API"
        }))
        .unwrap();

        let operations = extract_sequence_operations(&sequence);
        assert_eq!(operations.len(), 3);
        assert!(matches!(
            operations[0].kind,
            SequenceOperationKind::CameraCooling {
                minimum_duration: Some(duration),
                ..
            } if duration == chrono::Duration::minutes(10)
        ));
        assert!(matches!(
            operations[1].kind,
            SequenceOperationKind::TimeWait {
                target_time: Some(_),
                configured_duration: Some(duration)
            } if duration == chrono::Duration::milliseconds(90_500)
        ));
        assert!(matches!(
            operations[2].kind,
            SequenceOperationKind::TimeWait {
                target_time: None,
                configured_duration: Some(duration)
            } if duration == chrono::Duration::seconds(45)
        ));
    }

    #[test]
    fn extracts_slew_center_and_direct_plate_solve_output() {
        let sequence: SequenceResponse = serde_json::from_value(serde_json::json!({
            "Response": [{
                "Name": "Target_Container",
                "Status": "RUNNING",
                "Items": [{
                    "Name": "Slew to target",
                    "Status": "RUNNING",
                    "OperationKind": "mount_slew",
                    "Coordinates": {
                        "RA": 12.5,
                        "RAString": "12:30:00",
                        "Dec": 42.25,
                        "DecString": "+42:15:00",
                        "Epoch": "J2000"
                    }
                }, {
                    "Name": "Center and rotate",
                    "Status": "RUNNING",
                    "OperationKind": "mount_center",
                    "Coordinates": { "RA": 12.5, "Dec": 42.25 },
                    "Rotation": 91.5,
                    "PlateSolveOutput": {
                        "SolveTime": "2026-08-16T06:45:00Z",
                        "Success": true,
                        "Coordinates": {
                            "RA": 12.5001,
                            "RAString": "12:30:00.4",
                            "Dec": 42.2502,
                            "DecString": "+42:15:00.7",
                            "Epoch": "J2000"
                        },
                        "PositionAngle": 91.45,
                        "PixelScale": 1.25,
                        "SeparationArcseconds": 2.4,
                        "ThumbnailBase64": "AQID",
                        "ThumbnailMediaType": "image/jpeg"
                    }
                }]
            }],
            "Error": "",
            "StatusCode": 200,
            "Success": true,
            "Type": "API"
        }))
        .unwrap();

        let operations = extract_sequence_operations(&sequence);
        assert_eq!(operations.len(), 2);
        assert!(matches!(
            &operations[0].kind,
            SequenceOperationKind::MountSlew {
                coordinates: Some(coordinates),
                may_be_center: false,
            } if coordinates.ra_string.as_deref() == Some("12:30:00")
        ));
        assert!(matches!(
            &operations[1].kind,
            SequenceOperationKind::MountCenter {
                rotation: Some(rotation),
                output: Some(output),
                ..
            } if (*rotation - 91.5).abs() < f64::EPSILON
                && output.success == Some(true)
                && output.separation_arcseconds == Some(2.4)
                && output.thumbnail.as_deref() == Some(&[1, 2, 3][..])
        ));
    }

    #[test]
    fn advanced_api_coordinate_items_remain_center_promotable() {
        let sequence: SequenceResponse = serde_json::from_value(serde_json::json!({
            "Response": [{
                "Name": "Custom renamed operation",
                "Status": "RUNNING",
                "Coordinates": {
                    "Altitude": { "Degree": 50.5 },
                    "Azimuth": { "Degree": 182.25 }
                }
            }],
            "Error": "",
            "StatusCode": 200,
            "Success": true,
            "Type": "API"
        }))
        .unwrap();

        let operations = extract_sequence_operations(&sequence);
        assert!(matches!(
            &operations[0].kind,
            SequenceOperationKind::MountSlew {
                coordinates: Some(coordinates),
                may_be_center: true,
            } if coordinates.altitude_degrees == Some(50.5)
                && coordinates.azimuth_degrees == Some(182.25)
        ));
    }

    #[test]
    fn test_extract_meridian_flip_time_no_global_triggers() {
        // Test data with empty global triggers
        let sequence_json = r#"{
            "Response": [
                {
                    "GlobalTriggers": []
                }
            ],
            "Error": "",
            "StatusCode": 200,
            "Success": true,
            "Type": "API"
        }"#;

        let sequence: SequenceResponse = serde_json::from_str(sequence_json).unwrap();
        let flip_time = extract_meridian_flip_time(&sequence);

        assert!(flip_time.is_none());
    }

    #[test]
    fn test_meridian_flip_time_conversions() {
        let time_hours = 1.3464521451944444;

        // Test conversion to minutes
        let time_minutes = meridian_flip_time_minutes(time_hours);
        let expected_minutes = 1.3464521451944444 * 60.0;
        assert!((time_minutes - expected_minutes).abs() < 0.001);

        // Test formatted time string
        let formatted = meridian_flip_time_formatted(time_hours);
        assert_eq!(formatted, "01:20"); // 1.346... hours = 1 hour 20 minutes (approximately)

        // Test edge cases
        assert_eq!(meridian_flip_time_formatted(0.0), "00:00");
        assert_eq!(meridian_flip_time_formatted(2.5), "02:30");
        assert_eq!(meridian_flip_time_formatted(0.25), "00:15");

        // Test formatted time string with wall-clock time
        let formatted_with_clock = meridian_flip_time_formatted_with_clock(time_hours);
        // Should contain the duration and "at" followed by time
        assert!(formatted_with_clock.contains("01:20"));
        assert!(formatted_with_clock.contains("at "));
        assert!(formatted_with_clock.matches(':').count() >= 3); // Should have HH:MM:SS format

        // Test short duration with wall-clock time
        let short_duration = meridian_flip_time_formatted_with_clock(0.25);
        assert!(short_duration.contains("00:15"));
        assert!(short_duration.contains("at "));
    }

    #[test]
    fn test_load_sequence_2_from_file() {
        // Test loading the second example sequence file if it exists
        if let Ok(json_content) = std::fs::read_to_string("example_sequence_2.json") {
            let sequence: Result<SequenceResponse, _> = serde_json::from_str(&json_content);
            assert!(
                sequence.is_ok(),
                "Should be able to parse example_sequence_2.json"
            );

            let sequence = sequence.unwrap();
            assert!(sequence.success, "Sequence should indicate success");
            assert_eq!(sequence.status_code, 200, "Should have status code 200");
            assert!(!sequence.response.is_empty(), "Should have response items");

            // Test target extraction from real file - should be "North American" (currently running)
            let target = extract_current_target(&sequence);
            println!("Found target in example_sequence_2.json: {target:?}");
            assert_eq!(target, Some("North American".to_string()));

            // Test container extraction
            let containers = sequence.get_containers();
            println!(
                "Found {} containers in example_sequence_2.json",
                containers.len()
            );
            assert!(!containers.is_empty());

            // Test global triggers
            if let Some(triggers) = sequence.get_global_triggers() {
                println!("Found {} global triggers", triggers.global_triggers.len());
                assert_eq!(triggers.global_triggers.len(), 4); // Should have 4 global triggers
            }

            // Test meridian flip time extraction from real file
            let meridian_flip_time = extract_meridian_flip_time(&sequence);
            println!("Found meridian flip time in example_sequence_2.json: {meridian_flip_time:?}");

            // Should have meridian flip time
            assert!(meridian_flip_time.is_some());
            let time_hours = meridian_flip_time.unwrap();
            // Should be around 1.169 hours based on the file content
            assert!((time_hours - 1.1690078382777778).abs() < 0.0001);

            if let Some(time_hours) = meridian_flip_time {
                let time_minutes = meridian_flip_time_minutes(time_hours);
                let time_formatted = meridian_flip_time_formatted(time_hours);
                println!(
                    "Meridian flip in {time_hours:.3} hours ({time_minutes:.1} minutes, {time_formatted})"
                );
            }
        } else {
            println!("example_sequence_2.json not found, skipping file test");
        }
    }
}
