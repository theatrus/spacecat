use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct FocuserInfoResponse {
    pub response: FocuserInfo,
    pub error: String,
    pub status_code: i32,
    pub success: bool,
    #[serde(rename = "Type")]
    pub response_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct FocuserInfo {
    pub connected: bool,
    #[serde(default)]
    pub position: i32,
    #[serde(default)]
    pub step_size: f64,
    /// NINA returns this as a JSON *string* `"NaN"` when the focuser has
    /// no temperature sensor (or is disconnected). Accept "NaN"/number/null.
    #[serde(default, deserialize_with = "de_f64_or_nan_string")]
    pub temperature: f64,
    #[serde(default)]
    pub is_moving: bool,
    #[serde(default)]
    pub is_settling: bool,
    #[serde(default)]
    pub temp_comp: bool,
    #[serde(default)]
    pub temp_comp_available: bool,
}

fn de_f64_or_nan_string<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    use serde::de::Error;
    let v = serde_json::Value::deserialize(d)?;
    match v {
        serde_json::Value::Number(n) => n
            .as_f64()
            .ok_or_else(|| D::Error::custom("temperature not f64")),
        serde_json::Value::String(s) => match s.as_str() {
            "NaN" => Ok(f64::NAN),
            "Infinity" => Ok(f64::INFINITY),
            "-Infinity" => Ok(f64::NEG_INFINITY),
            _ => s.parse::<f64>().map_err(D::Error::custom),
        },
        serde_json::Value::Null => Ok(f64::NAN),
        other => Err(D::Error::custom(format!(
            "expected number, NaN string, or null for temperature, got {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_live_focuser_info_nan_string() {
        // c925 returns "NaN" as a string when the focuser temperature sensor
        // is unavailable.
        let json = r#"{"Response":{"Position":3325,"StepSize":1,"Temperature":"NaN","IsMoving":false,"IsSettling":false,"TempComp":false,"TempCompAvailable":false,"Connected":true},"Error":"","StatusCode":200,"Success":true,"Type":"API"}"#;
        let parsed: FocuserInfoResponse = serde_json::from_str(json).unwrap();
        assert!(parsed.response.connected);
        assert_eq!(parsed.response.position, 3325);
        assert!(parsed.response.temperature.is_nan());
    }

    #[test]
    fn test_parse_focuser_with_real_temperature() {
        let json = r#"{"Response":{"Position":3325,"StepSize":1,"Temperature":14.7,"IsMoving":false,"IsSettling":false,"TempComp":false,"TempCompAvailable":true,"Connected":true},"Error":"","StatusCode":200,"Success":true,"Type":"API"}"#;
        let parsed: FocuserInfoResponse = serde_json::from_str(json).unwrap();
        assert!((parsed.response.temperature - 14.7).abs() < 1e-6);
        assert!(parsed.response.temp_comp_available);
    }
}
