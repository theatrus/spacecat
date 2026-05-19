use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RotatorInfoResponse {
    pub response: RotatorInfo,
    pub error: String,
    pub status_code: i32,
    pub success: bool,
    #[serde(rename = "Type")]
    pub response_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RotatorInfo {
    pub connected: bool,
    #[serde(default)]
    pub can_reverse: bool,
    #[serde(default)]
    pub reverse: bool,
    #[serde(default)]
    pub position: f64,
    #[serde(default)]
    pub mechanical_position: f64,
    #[serde(default)]
    pub step_size: f64,
    #[serde(default)]
    pub is_moving: bool,
    #[serde(default)]
    pub synced: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_live_rotator_info() {
        let json = r#"{"Response":{"CanReverse":false,"Reverse":false,"MechanicalPosition":0,"Position":104.04,"StepSize":0.5,"IsMoving":false,"Synced":true,"Connected":true},"Error":"","StatusCode":200,"Success":true,"Type":"API"}"#;
        let parsed: RotatorInfoResponse = serde_json::from_str(json).unwrap();
        assert!(parsed.success);
        assert!(parsed.response.connected);
        assert!((parsed.response.position - 104.04).abs() < 1e-6);
        assert!(parsed.response.synced);
    }

    #[test]
    fn test_parse_disconnected_rotator() {
        let json = r#"{"Response":{"CanReverse":false,"Reverse":false,"MechanicalPosition":0,"Position":0,"StepSize":0,"IsMoving":false,"Synced":false,"Connected":false},"Error":"","StatusCode":200,"Success":true,"Type":"API"}"#;
        let parsed: RotatorInfoResponse = serde_json::from_str(json).unwrap();
        assert!(!parsed.response.connected);
    }
}
