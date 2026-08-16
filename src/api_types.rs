use serde::{Deserialize, Serialize};

/// Transport-neutral response returned by a semantic N.I.N.A. command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CommandResponse {
    pub response: serde_json::Value,
    pub error: String,
    pub status_code: i32,
    pub success: bool,
    #[serde(rename = "Type")]
    pub response_type: String,
}

impl CommandResponse {
    /// Best-effort human-readable summary of the response body.
    pub fn summary(&self) -> String {
        if !self.success {
            return if self.error.is_empty() {
                "failed".to_string()
            } else {
                format!("failed: {}", self.error)
            };
        }
        match &self.response {
            serde_json::Value::String(value) if !value.is_empty() => value.clone(),
            serde_json::Value::Bool(value) => value.to_string(),
            serde_json::Value::Null => "ok".to_string(),
            other => other.to_string(),
        }
    }
}
