use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ImageResponse {
    pub response: String, // Base64 encoded image data
    pub error: String,
    pub status_code: i32,
    pub success: bool,
    #[serde(rename = "Type")]
    pub response_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ImageHistoryResponse {
    pub response: Vec<ImageMetadata>,
    pub error: String,
    pub status_code: i32,
    pub success: bool,
    #[serde(rename = "Type")]
    pub response_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ImageMetadata {
    pub exposure_time: f64,
    pub image_type: String,
    pub filter: String,
    pub rms_text: String,
    pub temperature: f64,
    pub camera_name: String,
    pub gain: i32,
    pub offset: i32,
    pub date: String,
    pub telescope_name: String,
    pub focal_length: i32,
    pub st_dev: f64,
    pub mean: f64,
    pub median: f64,
    pub stars: i32,
    #[serde(rename = "HFR")]
    pub hfr: f64,
    pub is_bayered: bool,
}

// Image type constants for easier matching
pub mod image_types {
    pub const LIGHT: &str = "LIGHT";
    pub const DARK: &str = "DARK";
    pub const FLAT: &str = "FLAT";
    pub const BIAS: &str = "BIAS";
}

// Common filter constants
pub mod filters {
    pub const LUMINANCE: &str = "L";
    pub const RED: &str = "R";
    pub const GREEN: &str = "G";
    pub const BLUE: &str = "B";
    pub const HYDROGEN_ALPHA: &str = "HA";
    pub const OXYGEN_III: &str = "OIII";
    pub const SULFUR_II: &str = "SII";
}

impl ImageHistoryResponse {
    /// Get all images of a specific type (LIGHT, DARK, FLAT, BIAS)
    pub fn get_images_by_type(&self, image_type: &str) -> Vec<&ImageMetadata> {
        self.response
            .iter()
            .filter(|image| image.image_type == image_type)
            .collect()
    }

    /// Get all images using a specific filter
    pub fn get_images_by_filter(&self, filter: &str) -> Vec<&ImageMetadata> {
        self.response
            .iter()
            .filter(|image| image.filter == filter)
            .collect()
    }

    /// Get all light frames (science images)
    pub fn get_light_frames(&self) -> Vec<&ImageMetadata> {
        self.get_images_by_type(image_types::LIGHT)
    }

    /// Get all calibration frames (darks, flats, bias)
    pub fn get_calibration_frames(&self) -> Vec<&ImageMetadata> {
        self.response
            .iter()
            .filter(|image| {
                image.image_type == image_types::DARK
                    || image.image_type == image_types::FLAT
                    || image.image_type == image_types::BIAS
            })
            .collect()
    }

    /// Get images in a temperature range
    pub fn get_images_in_temperature_range(&self, min_temp: f64, max_temp: f64) -> Vec<&ImageMetadata> {
        self.response
            .iter()
            .filter(|image| image.temperature >= min_temp && image.temperature <= max_temp)
            .collect()
    }

    /// Get images with specific exposure time
    pub fn get_images_by_exposure_time(&self, exposure_time: f64) -> Vec<&ImageMetadata> {
        self.response
            .iter()
            .filter(|image| (image.exposure_time - exposure_time).abs() < 0.001)
            .collect()
    }

    /// Count images by type
    pub fn count_images_by_type(&self) -> std::collections::HashMap<String, usize> {
        let mut counts = std::collections::HashMap::new();
        for image in &self.response {
            *counts.entry(image.image_type.clone()).or_insert(0) += 1;
        }
        counts
    }

    /// Count images by filter
    pub fn count_images_by_filter(&self) -> std::collections::HashMap<String, usize> {
        let mut counts = std::collections::HashMap::new();
        for image in &self.response {
            *counts.entry(image.filter.clone()).or_insert(0) += 1;
        }
        counts
    }

    /// Get basic statistics about the image session
    pub fn get_session_stats(&self) -> SessionStats {
        let total_images = self.response.len();
        let light_frames = self.get_light_frames().len();
        let calibration_frames = self.get_calibration_frames().len();
        
        let total_exposure_time: f64 = self.get_light_frames()
            .iter()
            .map(|img| img.exposure_time)
            .sum();
        
        let unique_filters: std::collections::HashSet<_> = self.response
            .iter()
            .map(|img| img.filter.as_str())
            .collect();

        SessionStats {
            total_images,
            light_frames,
            calibration_frames,
            total_exposure_time,
            unique_filters: unique_filters.len(),
        }
    }
}

#[derive(Debug)]
pub struct SessionStats {
    pub total_images: usize,
    pub light_frames: usize,
    pub calibration_frames: usize,
    pub total_exposure_time: f64,
    pub unique_filters: usize,
}

impl std::fmt::Display for SessionStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Session Stats: {} total images ({} light frames, {} calibration frames), {:.1}s total exposure, {} unique filters",
            self.total_images,
            self.light_frames,
            self.calibration_frames,
            self.total_exposure_time,
            self.unique_filters
        )
    }
}

impl ImageMetadata {
    /// Check if this is a light frame (science image)
    pub fn is_light_frame(&self) -> bool {
        self.image_type == image_types::LIGHT
    }

    /// Check if this is a calibration frame
    pub fn is_calibration_frame(&self) -> bool {
        self.image_type == image_types::DARK
            || self.image_type == image_types::FLAT
            || self.image_type == image_types::BIAS
    }

    /// Check if this is a broadband filter (LRGB)
    pub fn is_broadband_filter(&self) -> bool {
        matches!(
            self.filter.as_str(),
            filters::LUMINANCE | filters::RED | filters::GREEN | filters::BLUE
        )
    }

    /// Check if this is a narrowband filter
    pub fn is_narrowband_filter(&self) -> bool {
        matches!(
            self.filter.as_str(),
            filters::HYDROGEN_ALPHA | filters::OXYGEN_III | filters::SULFUR_II
        )
    }

    /// Parse timestamp as std::time::SystemTime
    pub fn parse_timestamp(&self) -> Result<std::time::SystemTime, Box<dyn std::error::Error>> {
        // For now, just return current time - full parsing would need a date parsing library
        Ok(std::time::SystemTime::now())
    }

    /// Get exposure time in minutes for easier reading
    pub fn exposure_time_minutes(&self) -> f64 {
        self.exposure_time / 60.0
    }
}