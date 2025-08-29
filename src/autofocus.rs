use serde::{Deserialize, Deserializer, Serialize};

/// Custom deserializer for f64 that handles "NaN" strings
fn deserialize_f64_or_nan<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    use std::fmt;

    struct F64OrNanVisitor;

    impl<'de> Visitor<'de> for F64OrNanVisitor {
        type Value = f64;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a number or \"NaN\"")
        }

        fn visit_f64<E>(self, value: f64) -> Result<f64, E>
        where
            E: de::Error,
        {
            Ok(value)
        }

        fn visit_str<E>(self, value: &str) -> Result<f64, E>
        where
            E: de::Error,
        {
            if value == "NaN" {
                Ok(f64::NAN)
            } else {
                value.parse().map_err(de::Error::custom)
            }
        }
    }

    deserializer.deserialize_any(F64OrNanVisitor)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct AutofocusResponse {
    pub response: AutofocusData,
    pub error: String,
    pub status_code: i32,
    pub success: bool,
    #[serde(rename = "Type")]
    pub response_type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct AutofocusData {
    pub version: i32,
    pub filter: String,
    pub auto_focuser_name: String,
    pub star_detector_name: String,
    pub timestamp: String,
    pub temperature: f64,
    pub method: String,
    pub fitting: String,
    pub initial_focus_point: FocusPoint,
    pub calculated_focus_point: FocusPoint,
    pub previous_focus_point: FocusPoint,
    pub measure_points: Vec<FocusPoint>,
    pub intersections: Intersections,
    pub fittings: Fittings,
    #[serde(rename = "RSquares")]
    pub r_squares: RSquares,
    pub backlash_compensation: BacklashCompensation,
    pub duration: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct FocusPoint {
    pub position: i32,
    #[serde(deserialize_with = "deserialize_f64_or_nan")]
    pub value: f64,
    pub error: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct IntersectionPoint {
    pub position: f64,
    #[serde(deserialize_with = "deserialize_f64_or_nan")]
    pub value: f64,
    pub error: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Intersections {
    pub trend_line_intersection: Option<IntersectionPoint>,
    pub hyperbolic_minimum: Option<IntersectionPoint>,
    pub quadratic_minimum: Option<IntersectionPoint>,
    pub gaussian_maximum: Option<IntersectionPoint>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Fittings {
    pub quadratic: String,
    pub hyperbolic: String,
    pub gaussian: String,
    pub left_trend: String,
    pub right_trend: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct RSquares {
    #[serde(deserialize_with = "deserialize_f64_or_nan")]
    pub quadratic: f64,
    #[serde(deserialize_with = "deserialize_f64_or_nan")]
    pub hyperbolic: f64,
    #[serde(deserialize_with = "deserialize_f64_or_nan")]
    pub left_trend: f64,
    #[serde(deserialize_with = "deserialize_f64_or_nan")]
    pub right_trend: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct BacklashCompensation {
    pub backlash_compensation_model: String,
    #[serde(rename = "BacklashIN")]
    pub backlash_in: i32,
    #[serde(rename = "BacklashOUT")]
    pub backlash_out: i32,
}

impl AutofocusResponse {
    /// Get the calculated focus position
    pub fn get_calculated_position(&self) -> i32 {
        self.response.calculated_focus_point.position
    }

    /// Get the HFR (Half Flux Radius) value at the calculated focus position
    pub fn get_calculated_hfr(&self) -> f64 {
        self.response.calculated_focus_point.value
    }

    /// Get the filter used during autofocus
    pub fn get_filter(&self) -> &str {
        &self.response.filter
    }

    /// Get the temperature during autofocus
    pub fn get_temperature(&self) -> f64 {
        self.response.temperature
    }

    /// Get the autofocus duration
    pub fn get_duration(&self) -> &str {
        &self.response.duration
    }

    /// Get the method used for autofocus
    pub fn get_method(&self) -> &str {
        &self.response.method
    }

    /// Get the fitting method used
    pub fn get_fitting(&self) -> &str {
        &self.response.fitting
    }

    /// Get the number of measurement points taken
    pub fn get_measurement_count(&self) -> usize {
        self.response.measure_points.len()
    }

    /// Get the best R-squared value among all fitting methods
    pub fn get_best_r_squared(&self) -> f64 {
        let r_squares = &self.response.r_squares;
        [
            r_squares.quadratic,
            r_squares.hyperbolic,
            r_squares.left_trend,
            r_squares.right_trend,
        ]
        .iter()
        .fold(f64::NEG_INFINITY, |acc, &x| acc.max(x))
    }

    /// Check if the autofocus was successful based on criteria
    pub fn is_successful(&self) -> bool {
        self.success
            && self.response.calculated_focus_point.error == 0.0
            && self.get_best_r_squared() > 0.8
    }
}

impl AutofocusData {
    /// Get focus positions in ascending order
    pub fn get_focus_positions(&self) -> Vec<i32> {
        let mut positions: Vec<i32> = self.measure_points.iter().map(|p| p.position).collect();
        positions.sort();
        positions
    }

    /// Get the focus range (min to max position tested)
    pub fn get_focus_range(&self) -> (i32, i32) {
        let positions = self.get_focus_positions();
        (
            *positions.first().unwrap_or(&0),
            *positions.last().unwrap_or(&0),
        )
    }

    /// Get HFR values corresponding to focus positions
    pub fn get_hfr_values(&self) -> Vec<f64> {
        self.measure_points.iter().map(|p| p.value).collect()
    }

    /// Get the best HFR (lowest value) from all measurement points
    pub fn get_best_measured_hfr(&self) -> Option<f64> {
        self.measure_points
            .iter()
            .map(|p| p.value)
            .min_by(|a, b| a.partial_cmp(b).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_autofocus_response() {
        let json_content = std::fs::read_to_string("example_last_af.json").unwrap();
        let response: AutofocusResponse = serde_json::from_str(&json_content).unwrap();

        // Test basic response structure
        assert_eq!(response.status_code, 200);
        assert!(response.success);
        assert_eq!(response.response_type, "API");
        assert!(response.error.is_empty());

        // Test autofocus data
        let af_data = &response.response;
        assert_eq!(af_data.version, 2);
        assert_eq!(af_data.filter, "OIII");
        assert_eq!(af_data.auto_focuser_name, "NINA");
        assert_eq!(af_data.star_detector_name, "NINA");
        assert_eq!(af_data.temperature, 21.3);
        assert_eq!(af_data.method, "STARHFR");
        assert_eq!(af_data.fitting, "TRENDHYPERBOLIC");

        // Test focus points
        assert_eq!(af_data.calculated_focus_point.position, 4068);
        assert_eq!(af_data.calculated_focus_point.value, 2.90813054456021);
        assert_eq!(af_data.calculated_focus_point.error, 0.0);

        // Test that initial focus point has NaN value (properly parsed)
        assert_eq!(af_data.initial_focus_point.position, 4092);
        assert!(af_data.initial_focus_point.value.is_nan());
        assert_eq!(af_data.initial_focus_point.error, 0.0);

        // Test measure points
        assert_eq!(af_data.measure_points.len(), 10);
        assert_eq!(af_data.measure_points[0].position, 3992);
        assert!((af_data.measure_points[0].value - 3.9320351318958195).abs() < 1e-10);

        // Test R-squared values
        assert!((af_data.r_squares.hyperbolic - 0.9894178774335628).abs() < 1e-10);
        assert!((af_data.r_squares.quadratic - 0.9810757827720883).abs() < 1e-10);

        // Test backlash compensation
        assert_eq!(
            af_data.backlash_compensation.backlash_compensation_model,
            "OVERSHOOT"
        );
        assert_eq!(af_data.backlash_compensation.backlash_in, 0);
        assert_eq!(af_data.backlash_compensation.backlash_out, 20);
    }

    #[test]
    fn test_autofocus_response_methods() {
        let json_content = std::fs::read_to_string("example_last_af.json").unwrap();
        let response: AutofocusResponse = serde_json::from_str(&json_content).unwrap();

        // Test convenience methods
        assert_eq!(response.get_calculated_position(), 4068);
        assert_eq!(response.get_calculated_hfr(), 2.90813054456021);
        assert_eq!(response.get_filter(), "OIII");
        assert_eq!(response.get_temperature(), 21.3);
        assert_eq!(response.get_method(), "STARHFR");
        assert_eq!(response.get_fitting(), "TRENDHYPERBOLIC");
        assert_eq!(response.get_measurement_count(), 10);

        // Test R-squared analysis
        let best_r_squared = response.get_best_r_squared();
        assert!(best_r_squared > 0.98); // Should be the hyperbolic fit (0.9894)

        // Test success criteria
        assert!(response.is_successful());
    }

    #[test]
    fn test_autofocus_data_analysis() {
        let json_content = std::fs::read_to_string("example_last_af.json").unwrap();
        let response: AutofocusResponse = serde_json::from_str(&json_content).unwrap();
        let af_data = &response.response;

        // Test focus range analysis
        let (min_pos, max_pos) = af_data.get_focus_range();
        assert_eq!(min_pos, 3992);
        assert_eq!(max_pos, 4172);

        // Test position ordering
        let positions = af_data.get_focus_positions();
        assert_eq!(positions.len(), 10);
        assert!(positions.windows(2).all(|w| w[0] <= w[1])); // Check sorted

        // Test HFR analysis
        let hfr_values = af_data.get_hfr_values();
        assert_eq!(hfr_values.len(), 10);

        let best_hfr = af_data.get_best_measured_hfr().unwrap();
        assert!(best_hfr < 3.1); // Should find the minimum HFR (around 3.009)
    }

    #[test]
    fn test_parse_autofocus_response_2() {
        let json_content = std::fs::read_to_string("example_last_af_2.json").unwrap();
        let response: AutofocusResponse = serde_json::from_str(&json_content).unwrap();

        // Test basic response structure
        assert_eq!(response.status_code, 200);
        assert!(response.success);
        assert_eq!(response.response_type, "API");
        assert!(response.error.is_empty());

        // Test autofocus data
        let af_data = &response.response;
        assert_eq!(af_data.version, 2);
        assert_eq!(af_data.filter, "SII");
        assert_eq!(af_data.auto_focuser_name, "Hocus Focus");
        assert_eq!(af_data.star_detector_name, "NINA");
        assert_eq!(af_data.temperature, 24.6);
        assert_eq!(af_data.method, "STARHFR");
        assert_eq!(af_data.fitting, "TRENDHYPERBOLIC");

        // Test focus points
        assert_eq!(af_data.calculated_focus_point.position, 4186);
        assert!((af_data.calculated_focus_point.value - 2.6632989580477844).abs() < 1e-10);
        assert_eq!(af_data.calculated_focus_point.error, 0.0);

        // Test initial focus point
        assert_eq!(af_data.initial_focus_point.position, 4076);
        assert!((af_data.initial_focus_point.value - 5.024422888144213).abs() < 1e-10);

        // Test measure points
        assert_eq!(af_data.measure_points.len(), 11);
        assert_eq!(af_data.measure_points[0].position, 4056);
        assert_eq!(af_data.measure_points[10].position, 4256);

        // Test R-squared values
        assert!((af_data.r_squares.hyperbolic - 0.991774159363382).abs() < 1e-10);
        assert!(af_data.r_squares.quadratic.is_nan());

        // Test intersections (only some are present in this example)
        assert!(af_data.intersections.trend_line_intersection.is_some());
        assert!(af_data.intersections.hyperbolic_minimum.is_some());
        assert!(af_data.intersections.quadratic_minimum.is_none());
        assert!(af_data.intersections.gaussian_maximum.is_none());

        // Check the hyperbolic minimum has fractional position
        if let Some(hyperbolic) = &af_data.intersections.hyperbolic_minimum {
            assert!((hyperbolic.position - 4188.955065493704).abs() < 1e-10);
        }

        // Test that it's a successful autofocus
        assert!(response.is_successful());

        // Test focus change
        assert_eq!(af_data.initial_focus_point.position, 4076);
        assert_eq!(af_data.calculated_focus_point.position, 4186);
        let position_change =
            af_data.calculated_focus_point.position - af_data.initial_focus_point.position;
        assert_eq!(position_change, 110);
    }
}
