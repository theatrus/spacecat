use crate::serde_helpers::de_f64_tolerant;
use serde::{Deserialize, Serialize};

/// Advanced API-compatible camera snapshot used by both HTTP and Direct rigs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CameraInfoResponse {
    pub response: CameraInfo,
    pub error: String,
    pub status_code: i32,
    pub success: bool,
    #[serde(rename = "Type")]
    pub response_type: String,
}

/// The cooling fields Chatstronomy needs for live operation progress.
/// Unknown Advanced API fields are intentionally ignored.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CameraInfo {
    pub connected: bool,
    #[serde(default)]
    pub can_set_temperature: bool,
    #[serde(default)]
    pub cooler_on: bool,
    #[serde(default, deserialize_with = "de_f64_tolerant")]
    pub cooler_power: f64,
    #[serde(default, deserialize_with = "de_f64_tolerant")]
    pub temperature: f64,
    #[serde(default, deserialize_with = "de_f64_tolerant")]
    pub temperature_set_point: f64,
    #[serde(default)]
    pub at_target_temp: bool,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub display_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_advanced_api_camera_cooling_snapshot() {
        let json = r#"{"Response":{"Connected":true,"CanSetTemperature":true,"CoolerOn":true,"CoolerPower":72.5,"Temperature":-6.4,"TemperatureSetPoint":-10.0,"AtTargetTemp":false,"Name":"ASI2600MM","DisplayName":"ASI2600MM"},"Error":"","StatusCode":200,"Success":true,"Type":"API"}"#;
        let parsed: CameraInfoResponse = serde_json::from_str(json).unwrap();

        assert!(parsed.response.connected);
        assert!(parsed.response.cooler_on);
        assert!((parsed.response.temperature - -6.4).abs() < 1e-6);
        assert!((parsed.response.temperature_set_point - -10.0).abs() < 1e-6);
        assert!(!parsed.response.at_target_temp);
    }

    #[test]
    fn tolerates_unknown_camera_temperatures() {
        let json = r#"{"Response":{"Connected":false,"Temperature":"NaN","TemperatureSetPoint":[],"CoolerPower":null},"Error":"","StatusCode":200,"Success":true,"Type":"API"}"#;
        let parsed: CameraInfoResponse = serde_json::from_str(json).unwrap();

        assert!(parsed.response.temperature.is_nan());
        assert!(parsed.response.temperature_set_point.is_nan());
        assert!(parsed.response.cooler_power.is_nan());
    }
}
