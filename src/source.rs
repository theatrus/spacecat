//! Observatory data-source abstraction.
//!
//! Chatstronomy supports two ways to communicate with N.I.N.A.:
//! [`RigSourceKind::AdvancedApi`] uses the existing Advanced API HTTP plugin,
//! while [`RigSourceKind::NinaDirect`] is reserved for the native Chatstronomy
//! N.I.N.A. plugin transport.  Consumers such as the chat updater and Discord
//! command handlers depend on [`RigSource`] rather than either transport.

use crate::api::{ApiError, ChatstronomyApiClient, CommandResponse};
use crate::autofocus::AutofocusResponse;
use crate::config::ApiConfig;
use crate::events::EventHistoryResponse;
use crate::filterwheel::FilterWheelInfoResponse;
use crate::focuser::FocuserInfoResponse;
use crate::guider::{GuiderGraphResponse, GuiderInfoResponse};
use crate::images::{ImageHistoryResponse, ThumbnailResponse};
use crate::mount::MountInfoResponse;
use crate::rotator::RotatorInfoResponse;
use crate::sequence::SequenceResponse;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;

/// How a rig supplies N.I.N.A. data to Chatstronomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RigSourceKind {
    /// Poll the separately installed N.I.N.A. Advanced API plugin over HTTP.
    AdvancedApi,
    /// Receive events and execute commands through the Chatstronomy N.I.N.A. plugin.
    NinaDirect,
}

/// Features a source can expose to the source-neutral Chatstronomy runtime.
///
/// Direct mode will negotiate these flags with the N.I.N.A. plugin. Keeping
/// them explicit lets chat commands report an unsupported feature instead of
/// guessing or silently falling back to a second source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RigCapabilities {
    pub event_history: bool,
    pub image_history: bool,
    pub thumbnails: bool,
    pub sequence: bool,
    pub equipment_snapshots: bool,
    pub autofocus_details: bool,
    pub guider_graph: bool,
    pub commands: bool,
}

impl RigCapabilities {
    pub const fn none() -> Self {
        Self {
            event_history: false,
            image_history: false,
            thumbnails: false,
            sequence: false,
            equipment_snapshots: false,
            autofocus_details: false,
            guider_graph: false,
            commands: false,
        }
    }

    pub const fn advanced_api() -> Self {
        Self {
            event_history: true,
            image_history: true,
            thumbnails: true,
            sequence: true,
            equipment_snapshots: true,
            autofocus_details: true,
            guider_graph: true,
            commands: true,
        }
    }
}

#[derive(Debug, Error)]
pub enum RigSourceError {
    #[error("Advanced API error: {0}")]
    AdvancedApi(#[from] ApiError),

    #[error("{kind:?} source does not support {capability}")]
    Unsupported {
        kind: RigSourceKind,
        capability: &'static str,
    },

    #[error("{kind:?} source is unavailable: {reason}")]
    Unavailable { kind: RigSourceKind, reason: String },
}

pub type RigSourceResult<T> = Result<T, RigSourceError>;
pub type SharedRigSource = Arc<dyn RigSource>;

/// Source-neutral read and command surface used by Chatstronomy's runtime.
///
/// This intentionally describes the capabilities Chatstronomy consumes rather
/// than mirroring a transport. The direct N.I.N.A. implementation can satisfy
/// the same operations from cached snapshots and request/response messages.
#[async_trait]
pub trait RigSource: Send + Sync {
    fn kind(&self) -> RigSourceKind;
    fn capabilities(&self) -> RigCapabilities;

    async fn get_event_history(&self) -> RigSourceResult<EventHistoryResponse>;
    async fn get_all_image_history(&self) -> RigSourceResult<ImageHistoryResponse>;
    async fn get_sequence(&self) -> RigSourceResult<SequenceResponse>;
    async fn get_thumbnail(&self, index: u32) -> RigSourceResult<ThumbnailResponse>;
    async fn get_last_autofocus(&self) -> RigSourceResult<AutofocusResponse>;
    async fn get_mount_info(&self) -> RigSourceResult<MountInfoResponse>;
    async fn get_filterwheel_info(&self) -> RigSourceResult<FilterWheelInfoResponse>;
    async fn get_guider_info(&self) -> RigSourceResult<GuiderInfoResponse>;
    async fn get_guider_graph(&self) -> RigSourceResult<GuiderGraphResponse>;
    async fn get_rotator_info(&self) -> RigSourceResult<RotatorInfoResponse>;
    async fn get_focuser_info(&self) -> RigSourceResult<FocuserInfoResponse>;
    async fn execute_command(
        &self,
        endpoint: &str,
        params: &[(&str, &str)],
    ) -> RigSourceResult<CommandResponse>;
}

/// Existing Advanced API behavior exposed through [`RigSource`].
#[derive(Debug, Clone)]
pub struct AdvancedApiSource {
    client: ChatstronomyApiClient,
}

impl AdvancedApiSource {
    pub fn new(config: ApiConfig) -> Result<Self, ApiError> {
        Ok(Self {
            client: ChatstronomyApiClient::new(config)?,
        })
    }

    pub fn from_client(client: ChatstronomyApiClient) -> Self {
        Self { client }
    }

    pub fn client(&self) -> &ChatstronomyApiClient {
        &self.client
    }
}

#[async_trait]
impl RigSource for AdvancedApiSource {
    fn kind(&self) -> RigSourceKind {
        RigSourceKind::AdvancedApi
    }

    fn capabilities(&self) -> RigCapabilities {
        RigCapabilities::advanced_api()
    }

    async fn get_event_history(&self) -> RigSourceResult<EventHistoryResponse> {
        Ok(self.client.get_event_history().await?)
    }

    async fn get_all_image_history(&self) -> RigSourceResult<ImageHistoryResponse> {
        Ok(self.client.get_all_image_history().await?)
    }

    async fn get_sequence(&self) -> RigSourceResult<SequenceResponse> {
        Ok(self.client.get_sequence().await?)
    }

    async fn get_thumbnail(&self, index: u32) -> RigSourceResult<ThumbnailResponse> {
        Ok(self.client.get_thumbnail(index).await?)
    }

    async fn get_last_autofocus(&self) -> RigSourceResult<AutofocusResponse> {
        Ok(self.client.get_last_autofocus().await?)
    }

    async fn get_mount_info(&self) -> RigSourceResult<MountInfoResponse> {
        Ok(self.client.get_mount_info().await?)
    }

    async fn get_filterwheel_info(&self) -> RigSourceResult<FilterWheelInfoResponse> {
        Ok(self.client.get_filterwheel_info().await?)
    }

    async fn get_guider_info(&self) -> RigSourceResult<GuiderInfoResponse> {
        Ok(self.client.get_guider_info().await?)
    }

    async fn get_guider_graph(&self) -> RigSourceResult<GuiderGraphResponse> {
        Ok(self.client.get_guider_graph().await?)
    }

    async fn get_rotator_info(&self) -> RigSourceResult<RotatorInfoResponse> {
        Ok(self.client.get_rotator_info().await?)
    }

    async fn get_focuser_info(&self) -> RigSourceResult<FocuserInfoResponse> {
        Ok(self.client.get_focuser_info().await?)
    }

    async fn execute_command(
        &self,
        endpoint: &str,
        params: &[(&str, &str)],
    ) -> RigSourceResult<CommandResponse> {
        Ok(self.client.execute_command(endpoint, params).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advanced_api_source_advertises_existing_features() {
        let source = AdvancedApiSource::new(ApiConfig {
            base_url: "http://127.0.0.1:1888".to_string(),
            timeout_seconds: 30,
            retry_attempts: 3,
        })
        .unwrap();

        assert_eq!(source.kind(), RigSourceKind::AdvancedApi);
        assert_eq!(source.capabilities(), RigCapabilities::advanced_api());
    }

    #[test]
    fn source_kind_has_stable_wire_names() {
        assert_eq!(
            serde_json::to_string(&RigSourceKind::AdvancedApi).unwrap(),
            r#""advanced_api""#
        );
        assert_eq!(
            serde_json::to_string(&RigSourceKind::NinaDirect).unwrap(),
            r#""nina_direct""#
        );
    }
}
