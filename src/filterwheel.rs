use crate::events::FilterInfo;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct FilterWheelInfoResponse {
    pub response: FilterWheelInfo,
    pub error: String,
    pub status_code: i32,
    pub success: bool,
    #[serde(rename = "Type")]
    pub response_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct FilterWheelInfo {
    pub connected: bool,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub is_moving: bool,
    pub selected_filter: Option<FilterInfo>,
    #[serde(default)]
    pub available_filters: Vec<FilterInfo>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_live_filterwheel_info() {
        let json = r#"{"Response":{"Connected":true,"Name":"EFW","DisplayName":"EFW","Description":"Native driver for ZWOptical filter wheels","DriverInfo":"SDK: 1, 7; FW: 4.0.4","DriverVersion":"1.0","DeviceId":"ZWOptical_EFW_","SupportedActions":[],"IsMoving":false,"SelectedFilter":{"Name":"B","Id":5},"AvailableFilters":[{"Name":"HA","Id":0},{"Name":"OIII","Id":1},{"Name":"SII","Id":2},{"Name":"R","Id":3},{"Name":"G","Id":4},{"Name":"B","Id":5},{"Name":"L","Id":6}]},"Error":"","StatusCode":200,"Success":true,"Type":"API"}"#;
        let parsed: FilterWheelInfoResponse = serde_json::from_str(json).unwrap();
        assert!(parsed.success);
        assert!(parsed.response.connected);
        let selected = parsed.response.selected_filter.unwrap();
        assert_eq!(selected.name, "B");
        assert_eq!(selected.id, 5);
        assert_eq!(parsed.response.available_filters.len(), 7);
        assert_eq!(parsed.response.available_filters[0].name, "HA");
    }

    #[test]
    fn test_parse_disconnected_filterwheel() {
        let json = r#"{"Response":{"Connected":false,"Name":"","DisplayName":"","IsMoving":false,"SelectedFilter":null,"AvailableFilters":[]},"Error":"","StatusCode":200,"Success":true,"Type":"API"}"#;
        let parsed: FilterWheelInfoResponse = serde_json::from_str(json).unwrap();
        assert!(!parsed.response.connected);
        assert!(parsed.response.selected_filter.is_none());
    }
}
