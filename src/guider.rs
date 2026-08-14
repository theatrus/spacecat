use crate::serde_helpers::de_f64_tolerant;
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

/// NINA guider scale: 0 = pixels, 1 = arcseconds (NINA `GuiderScaleEnum`).
pub const GUIDER_SCALE_ARCSECONDS: i32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GuiderGraphResponse {
    pub response: GuideStepsHistory,
    pub error: String,
    pub status_code: i32,
    pub success: bool,
    #[serde(rename = "Type")]
    pub response_type: String,
}

/// The guide graph data behind NINA's guiding chart, as returned by
/// `/equipment/guider/graph`: the last n guide steps plus RMS statistics
/// and the axis ranges NINA itself uses to draw the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GuideStepsHistory {
    #[serde(rename = "RMS", default)]
    pub rms: Option<GuideGraphRms>,
    #[serde(default)]
    pub interval: i32,
    #[serde(default)]
    pub max_y: f64,
    #[serde(default)]
    pub min_y: f64,
    #[serde(default)]
    pub max_duration_y: f64,
    #[serde(default)]
    pub min_duration_y: f64,
    #[serde(default)]
    pub guide_steps: Vec<GuideGraphStep>,
    #[serde(default)]
    pub history_size: i32,
    #[serde(default, deserialize_with = "de_f64_tolerant")]
    pub pixel_scale: f64,
    /// 0 = pixels, 1 = arcseconds (`GUIDER_SCALE_ARCSECONDS`).
    #[serde(default)]
    pub scale: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GuideGraphRms {
    #[serde(rename = "RA", default, deserialize_with = "de_f64_tolerant")]
    pub ra: f64,
    #[serde(default, deserialize_with = "de_f64_tolerant")]
    pub dec: f64,
    #[serde(default, deserialize_with = "de_f64_tolerant")]
    pub total: f64,
    #[serde(rename = "RAText", default)]
    pub ra_text: String,
    #[serde(default)]
    pub dec_text: String,
    #[serde(default)]
    pub total_text: String,
    #[serde(rename = "PeakRAText", default)]
    pub peak_ra_text: String,
    #[serde(default)]
    pub peak_dec_text: String,
    #[serde(default, deserialize_with = "de_f64_tolerant")]
    pub scale: f64,
    #[serde(rename = "PeakRA", default, deserialize_with = "de_f64_tolerant")]
    pub peak_ra: f64,
    #[serde(default, deserialize_with = "de_f64_tolerant")]
    pub peak_dec: f64,
    #[serde(default)]
    pub data_points: i32,
}

/// One guide exposure: the measured RA/Dec error plus the correction
/// pulse durations issued in response. Durations are signed by direction
/// (NINA negates East/one Dec direction), so bars plot around zero.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GuideGraphStep {
    #[serde(default)]
    pub id: i64,
    #[serde(default, deserialize_with = "de_f64_tolerant")]
    pub id_offset_left: f64,
    #[serde(default, deserialize_with = "de_f64_tolerant")]
    pub id_offset_right: f64,
    #[serde(
        rename = "RADistanceRaw",
        default,
        deserialize_with = "de_f64_tolerant"
    )]
    pub ra_distance_raw: f64,
    #[serde(
        rename = "RADistanceRawDisplay",
        default,
        deserialize_with = "de_f64_tolerant"
    )]
    pub ra_distance_raw_display: f64,
    #[serde(rename = "RADuration", default, deserialize_with = "de_f64_tolerant")]
    pub ra_duration: f64,
    #[serde(
        rename = "DECDistanceRaw",
        default,
        deserialize_with = "de_f64_tolerant"
    )]
    pub dec_distance_raw: f64,
    #[serde(
        rename = "DECDistanceRawDisplay",
        default,
        deserialize_with = "de_f64_tolerant"
    )]
    pub dec_distance_raw_display: f64,
    #[serde(rename = "DECDuration", default, deserialize_with = "de_f64_tolerant")]
    pub dec_duration: f64,
    #[serde(default)]
    pub dither: String,
}

impl GuideStepsHistory {
    /// True when there are enough steps to draw a meaningful graph.
    pub fn has_graph_data(&self) -> bool {
        self.guide_steps.len() >= 2
    }

    /// Unit label for the error axis.
    pub fn scale_unit(&self) -> &'static str {
        if self.scale == GUIDER_SCALE_ARCSECONDS {
            "arcsec"
        } else {
            "px"
        }
    }

    /// One-line RMS summary built from NINA's preformatted text fields,
    /// e.g. `RA: 0.31 (0.65") Dec: 0.27 (0.57") Tot: 0.41 (0.86")`.
    pub fn rms_summary(&self) -> Option<String> {
        let rms = self.rms.as_ref()?;
        let parts: Vec<&str> = [
            rms.ra_text.as_str(),
            rms.dec_text.as_str(),
            rms.total_text.as_str(),
        ]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("  "))
        }
    }

    /// True when this step marks a dither (NINA sets `Dither` to a
    /// non-`"NO"` value on dither steps).
    pub fn is_dither_step(step: &GuideGraphStep) -> bool {
        !step.dither.is_empty() && step.dither != "NO"
    }
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

    #[test]
    fn test_parse_guider_graph_file() {
        let json = std::fs::read_to_string("example_guider_graph.json").unwrap();
        let parsed: GuiderGraphResponse = serde_json::from_str(&json).unwrap();
        assert!(parsed.success);
        let history = &parsed.response;
        assert!(history.has_graph_data());
        assert_eq!(history.guide_steps.len(), 8);
        assert_eq!(history.scale, GUIDER_SCALE_ARCSECONDS);
        assert_eq!(history.scale_unit(), "arcsec");
        assert!((history.pixel_scale - 2.1).abs() < 1e-9);

        // "NaN" string sentinels parse to NaN instead of failing
        assert!(history.guide_steps[5].ra_distance_raw.is_nan());

        // Signed correction durations survive round-tripping
        assert!((history.guide_steps[0].ra_duration - -120.0).abs() < 1e-9);

        // Dither step detection
        assert!(GuideStepsHistory::is_dither_step(&history.guide_steps[2]));
        assert!(!GuideStepsHistory::is_dither_step(&history.guide_steps[0]));

        let summary = history.rms_summary().unwrap();
        assert!(summary.contains("RA: 0.34"));
        assert!(summary.contains("Tot: 0.44"));
    }

    #[test]
    fn test_guider_graph_empty_history() {
        let json = r#"{"Response":{"RMS":null,"Interval":0,"MaxY":0,"MinY":0,"MaxDurationY":0,"MinDurationY":0,"GuideSteps":[],"HistorySize":100,"PixelScale":0,"Scale":0},"Error":"","StatusCode":200,"Success":true,"Type":"API"}"#;
        let parsed: GuiderGraphResponse = serde_json::from_str(json).unwrap();
        assert!(!parsed.response.has_graph_data());
        assert_eq!(parsed.response.scale_unit(), "px");
        assert!(parsed.response.rms_summary().is_none());
    }
}
