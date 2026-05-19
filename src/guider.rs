use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GuiderInfoResponse {
    pub response: GuiderInfo,
    pub error: String,
    pub status_code: i32,
    pub success: bool,
    #[serde(rename = "Type")]
    pub response_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GuiderInfo {
    pub connected: bool,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub pixel_scale: f64,
    #[serde(rename = "RMSError", default)]
    pub rms_error: Option<GuiderRmsError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiderRmsError {
    #[serde(rename = "RA")]
    pub ra: GuiderAxisError,
    #[serde(rename = "Dec")]
    pub dec: GuiderAxisError,
    #[serde(rename = "Total")]
    pub total: GuiderAxisError,
    #[serde(rename = "PeakRA", default)]
    pub peak_ra: Option<GuiderAxisError>,
    #[serde(rename = "PeakDec", default)]
    pub peak_dec: Option<GuiderAxisError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GuiderAxisError {
    pub pixel: f64,
    pub arcseconds: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_live_guider_info() {
        let json = r#"{"Response":{"Connected":true,"Name":"PHD2","DisplayName":"PHD2","Description":"PHD2 Guider","DriverInfo":"PHD2 Guider","DriverVersion":"1.0","DeviceId":"PHD2_Single","CanClearCalibration":true,"CanSetShiftRate":true,"CanGetLockPosition":true,"SupportedActions":[],"RMSError":{"RA":{"Pixel":0,"Arcseconds":0},"Dec":{"Pixel":0,"Arcseconds":0},"Total":{"Pixel":0,"Arcseconds":0},"PeakRA":{"Pixel":0,"Arcseconds":0},"PeakDec":{"Pixel":0,"Arcseconds":0}},"PixelScale":0.351089,"State":"Stopped"},"Error":"","StatusCode":200,"Success":true,"Type":"API"}"#;
        let parsed: GuiderInfoResponse = serde_json::from_str(json).unwrap();
        assert!(parsed.success);
        assert!(parsed.response.connected);
        assert_eq!(parsed.response.state, "Stopped");
        assert!((parsed.response.pixel_scale - 0.351089).abs() < 1e-6);
        let rms = parsed.response.rms_error.unwrap();
        assert_eq!(rms.total.arcseconds, 0.0);
    }
}
