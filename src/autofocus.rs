use serde::{Deserialize, Serialize};

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
    pub value: f64,
    pub error: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Intersections {
    pub trend_line_intersection: FocusPoint,
    pub hyperbolic_minimum: FocusPoint,
    pub quadratic_minimum: FocusPoint,
    pub gaussian_maximum: FocusPoint,
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
    pub quadratic: f64,
    pub hyperbolic: f64,
    pub left_trend: f64,
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
        self.success && 
        self.response.calculated_focus_point.error == 0.0 &&
        self.get_best_r_squared() > 0.8
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
        (*positions.first().unwrap_or(&0), *positions.last().unwrap_or(&0))
    }
    
    /// Get HFR values corresponding to focus positions
    pub fn get_hfr_values(&self) -> Vec<f64> {
        self.measure_points.iter().map(|p| p.value).collect()
    }
    
    /// Get the best HFR (lowest value) from all measurement points
    pub fn get_best_measured_hfr(&self) -> Option<f64> {
        self.measure_points.iter().map(|p| p.value).min_by(|a, b| a.partial_cmp(b).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

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
        assert_eq!(af_data.temperature, 26.8);
        assert_eq!(af_data.method, "STARHFR");
        assert_eq!(af_data.fitting, "TRENDHYPERBOLIC");
        
        // Test focus points
        assert_eq!(af_data.calculated_focus_point.position, 4146);
        assert_eq!(af_data.calculated_focus_point.value, 2.9420331693319604);
        assert_eq!(af_data.calculated_focus_point.error, 0.0);
        
        // Test measure points
        assert_eq!(af_data.measure_points.len(), 10);
        assert_eq!(af_data.measure_points[0].position, 4058);
        assert_eq!(af_data.measure_points[0].value, 4.650582372113996);
        
        // Test R-squared values
        assert_eq!(af_data.r_squares.hyperbolic, 0.970086228706008);
        assert_eq!(af_data.r_squares.quadratic, 0.944310012215433);
        
        // Test backlash compensation
        assert_eq!(af_data.backlash_compensation.backlash_compensation_model, "OVERSHOOT");
        assert_eq!(af_data.backlash_compensation.backlash_in, 0);
        assert_eq!(af_data.backlash_compensation.backlash_out, 20);
    }
    
    #[test]
    fn test_autofocus_response_methods() {
        let json_content = std::fs::read_to_string("example_last_af.json").unwrap();
        let response: AutofocusResponse = serde_json::from_str(&json_content).unwrap();
        
        // Test convenience methods
        assert_eq!(response.get_calculated_position(), 4146);
        assert_eq!(response.get_calculated_hfr(), 2.9420331693319604);
        assert_eq!(response.get_filter(), "OIII");
        assert_eq!(response.get_temperature(), 26.8);
        assert_eq!(response.get_method(), "STARHFR");
        assert_eq!(response.get_fitting(), "TRENDHYPERBOLIC");
        assert_eq!(response.get_measurement_count(), 10);
        
        // Test R-squared analysis
        let best_r_squared = response.get_best_r_squared();
        assert!(best_r_squared > 0.97); // Should be the hyperbolic fit
        
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
        assert_eq!(min_pos, 4058);
        assert_eq!(max_pos, 4238);
        
        // Test position ordering
        let positions = af_data.get_focus_positions();
        assert_eq!(positions.len(), 10);
        assert!(positions.windows(2).all(|w| w[0] <= w[1])); // Check sorted
        
        // Test HFR analysis
        let hfr_values = af_data.get_hfr_values();
        assert_eq!(hfr_values.len(), 10);
        
        let best_hfr = af_data.get_best_measured_hfr().unwrap();
        assert!(best_hfr < 3.1); // Should find the minimum HFR
    }
}