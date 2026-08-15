//! Relay agent: bridges a local N.I.N.A. Advanced API to a central
//! Chatstronomy hub.
//!
//! The relay opens an outbound WebSocket to the hub's `/v1/direct` endpoint,
//! authenticates with a pairing token (first run) or the credential minted by
//! that pairing, then answers the hub's queries by calling the local Advanced
//! API. The observatory needs no inbound port and runs no chat clients.

use crate::api::ChatstronomyApiClient;
use crate::config::TelescopeConfig;
use crate::direct::protocol::{
    AuthRequest, ClientHello, DIRECT_WEBSOCKET_PATH, DirectMessage, PROTOCOL_VERSION, PairRequest,
    QueryKind, QueryRequest, QueryResult,
};
use crate::source::RigCapabilities;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

/// Per-telescope relay settings, under `telescopes[].relay` in the rig
/// configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayConfig {
    /// Hub origin, e.g. "wss://hub.example.com" (http/https accepted and
    /// mapped to ws/wss).
    pub hub_url: String,
    /// One-time pairing token from the hub web app. Used only until a
    /// credential is minted and saved to the state file.
    #[serde(default)]
    pub pairing_token: Option<String>,
    /// Where the minted credential and stable identity live. Defaults to
    /// `chatstronomy-relay-<telescope>.json`.
    #[serde(default)]
    pub state_file: Option<String>,
}

impl RelayConfig {
    pub fn validate(&self) -> Result<(), String> {
        let ok = ["ws://", "wss://", "http://", "https://"]
            .iter()
            .any(|scheme| self.hub_url.starts_with(scheme));
        if !ok {
            return Err(
                "relay.hub_url must start with ws://, wss://, http://, or https://".to_string(),
            );
        }
        Ok(())
    }

    pub fn state_file_for(&self, telescope_name: &str) -> String {
        self.state_file
            .clone()
            .unwrap_or_else(|| format!("chatstronomy-relay-{telescope_name}.json"))
    }
}

/// Durable relay identity: generated once, persisted beside the config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayState {
    pub node_id: Uuid,
    pub profile_id: Uuid,
    #[serde(default)]
    pub credential: Option<String>,
}

impl RelayState {
    pub fn load_or_create<P: AsRef<Path>>(path: P) -> Result<Self, RelayError> {
        let path_ref = path.as_ref();
        if path_ref.exists() {
            let content = std::fs::read_to_string(path_ref)?;
            return serde_json::from_str(&content)
                .map_err(|e| RelayError::State(format!("invalid state file: {e}")));
        }
        let state = Self {
            node_id: Uuid::new_v4(),
            profile_id: Uuid::new_v4(),
            credential: None,
        };
        state.save(path_ref)?;
        Ok(state)
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), RelayError> {
        let json =
            serde_json::to_string_pretty(self).map_err(|e| RelayError::State(e.to_string()))?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RelayError {
    #[error("relay configuration error: {0}")]
    Config(String),
    #[error("relay state error: {0}")]
    State(String),
    #[error("relay I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("hub connection error: {0}")]
    Connect(#[from] Box<tokio_tungstenite::tungstenite::Error>),
    #[error("hub rejected the connection: {0}")]
    Rejected(String),
    #[error("hub connection closed")]
    Disconnected,
}

/// Build the WebSocket URL for a hub origin.
pub fn direct_url(hub_url: &str) -> String {
    let trimmed = hub_url.trim_end_matches('/');
    let mapped = if let Some(rest) = trimmed.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = trimmed.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        trimmed.to_string()
    };
    format!("{mapped}{DIRECT_WEBSOCKET_PATH}")
}

/// Run the relay until the process is stopped, reconnecting with the
/// telescope's backoff schedule. Authentication rejections are fatal —
/// retrying a bad credential forever helps nobody.
pub async fn run_relay(telescope: &TelescopeConfig) -> Result<(), RelayError> {
    let relay = telescope
        .relay
        .as_ref()
        .ok_or_else(|| RelayError::Config("telescope has no relay configuration".to_string()))?;
    relay.validate().map_err(RelayError::Config)?;
    let api = ChatstronomyApiClient::new(telescope.api.clone())
        .map_err(|e| RelayError::Config(e.to_string()))?;
    let state_path = relay.state_file_for(&telescope.name);
    let mut state = RelayState::load_or_create(&state_path)?;

    let initial = Duration::from_secs(telescope.reconnect.initial_seconds);
    let max = Duration::from_secs(telescope.reconnect.max_seconds);
    let mut backoff = initial;
    loop {
        match run_connection(relay, telescope, &api, &mut state, &state_path).await {
            Err(RelayError::Rejected(message)) => {
                return Err(RelayError::Rejected(message));
            }
            Err(e) => {
                eprintln!("Relay connection lost: {e}; reconnecting in {backoff:?}");
            }
            Ok(()) => {
                eprintln!("Relay connection closed; reconnecting in {backoff:?}");
            }
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(max);
    }
}

/// One connection lifetime: authenticate, then answer queries until the
/// socket closes. Public so tests can drive a single connection.
pub async fn run_connection(
    relay: &RelayConfig,
    telescope: &TelescopeConfig,
    api: &ChatstronomyApiClient,
    state: &mut RelayState,
    state_path: &str,
) -> Result<(), RelayError> {
    let url = direct_url(&relay.hub_url);
    let (socket, _) = connect_async(&url).await.map_err(Box::new)?;
    let (mut sink, mut stream) = socket.split();

    let hello = ClientHello {
        protocol_version: PROTOCOL_VERSION,
        node_id: state.node_id,
        session_id: Uuid::new_v4(),
        process_id: std::process::id(),
        profile_id: state.profile_id,
        profile_name: telescope.name.clone(),
        plugin_version: crate::version::VERSION_STRING.to_string(),
        nina_version: "advanced-api-relay".to_string(),
        capabilities: RigCapabilities::advanced_api(),
    };

    let first = match (&state.credential, &relay.pairing_token) {
        (Some(credential), _) => DirectMessage::Auth(AuthRequest {
            credential: credential.clone(),
            hello,
        }),
        (None, Some(token)) => DirectMessage::Pair(PairRequest {
            pairing_token: token.clone(),
            hello,
        }),
        (None, None) => {
            return Err(RelayError::Config(
                "no credential yet and no relay.pairing_token configured".to_string(),
            ));
        }
    };
    send(&mut sink, &first).await?;

    match recv(&mut stream).await? {
        DirectMessage::PairResult(result) => {
            state.credential = Some(result.credential);
            state.save(state_path)?;
            println!("Paired with hub; credential saved to {state_path}");
        }
        DirectMessage::AgentHello(_) => {}
        DirectMessage::Error { message } => return Err(RelayError::Rejected(message)),
        _ => {
            return Err(RelayError::Rejected(
                "unexpected handshake reply".to_string(),
            ));
        }
    }
    println!("Relay connected to {url}");

    // Query answers come from concurrent tasks; a channel funnels them to
    // the single writer.
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<DirectMessage>();
    let mut heartbeat = tokio::time::interval(Duration::from_secs(30));
    let mut heartbeat_seq: u64 = 0;

    loop {
        tokio::select! {
            outbound = out_rx.recv() => {
                // The channel can't close while out_tx lives in this scope.
                if let Some(message) = outbound {
                    send(&mut sink, &message).await?;
                }
            }
            _ = heartbeat.tick() => {
                heartbeat_seq += 1;
                send(&mut sink, &DirectMessage::Heartbeat { seq: heartbeat_seq }).await?;
            }
            inbound = stream.next() => {
                let Some(Ok(frame)) = inbound else {
                    return Err(RelayError::Disconnected);
                };
                match frame {
                    Message::Text(text) => {
                        match serde_json::from_str::<DirectMessage>(&text) {
                            Ok(DirectMessage::Query(query)) => {
                                let now = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_secs() as i64)
                                    .unwrap_or(0);
                                if query.expired_at(now) {
                                    // Never run stale work — especially
                                    // commands — after a hang or reconnect.
                                    let _ = out_tx.send(DirectMessage::QueryResult(QueryResult {
                                        id: query.id,
                                        ok: false,
                                        payload: serde_json::Value::Null,
                                        error: Some("query expired before execution".to_string()),
                                    }));
                                    continue;
                                }
                                let api = api.clone();
                                let out = out_tx.clone();
                                tokio::spawn(async move {
                                    let result = answer_query(&api, query).await;
                                    let _ = out.send(DirectMessage::QueryResult(result));
                                });
                            }
                            Ok(DirectMessage::HeartbeatAck { .. }) => {}
                            Ok(DirectMessage::Error { message }) => {
                                return Err(RelayError::Rejected(message));
                            }
                            Ok(_) | Err(_) => {}
                        }
                    }
                    Message::Close(_) => return Err(RelayError::Disconnected),
                    _ => {}
                }
            }
        }
    }
}

async fn send<S>(sink: &mut S, message: &DirectMessage) -> Result<(), RelayError>
where
    S: SinkExt<Message> + Unpin,
    S::Error: Into<tokio_tungstenite::tungstenite::Error>,
{
    let json = serde_json::to_string(message).map_err(|e| RelayError::State(e.to_string()))?;
    sink.send(Message::Text(json))
        .await
        .map_err(|e| RelayError::Connect(Box::new(e.into())))
}

async fn recv<S>(stream: &mut S) -> Result<DirectMessage, RelayError>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    while let Some(frame) = stream.next().await {
        match frame.map_err(Box::new)? {
            Message::Text(text) => {
                return serde_json::from_str(&text)
                    .map_err(|e| RelayError::Rejected(format!("invalid frame: {e}")));
            }
            Message::Ping(_) | Message::Pong(_) => continue,
            _ => return Err(RelayError::Disconnected),
        }
    }
    Err(RelayError::Disconnected)
}

fn to_result<T: serde::Serialize>(
    id: Uuid,
    outcome: Result<T, crate::api::ApiError>,
) -> QueryResult {
    match outcome {
        Ok(value) => match serde_json::to_value(&value) {
            Ok(payload) => QueryResult {
                id,
                ok: true,
                payload,
                error: None,
            },
            Err(e) => QueryResult {
                id,
                ok: false,
                payload: serde_json::Value::Null,
                error: Some(format!("serialize failed: {e}")),
            },
        },
        Err(e) => QueryResult {
            id,
            ok: false,
            payload: serde_json::Value::Null,
            error: Some(e.to_string()),
        },
    }
}

/// Answer a hub query with the local Advanced API.
pub async fn answer_query(api: &ChatstronomyApiClient, query: QueryRequest) -> QueryResult {
    let id = query.id;
    match query.kind {
        QueryKind::EventHistory => to_result(id, api.get_event_history().await),
        QueryKind::ImageHistory => to_result(id, api.get_all_image_history().await),
        QueryKind::Sequence => to_result(id, api.get_sequence().await),
        QueryKind::Thumbnail { index } => to_result(id, api.get_thumbnail(index).await),
        QueryKind::LastAutofocus => to_result(id, api.get_last_autofocus().await),
        QueryKind::MountInfo => to_result(id, api.get_mount_info().await),
        QueryKind::FilterwheelInfo => to_result(id, api.get_filterwheel_info().await),
        QueryKind::GuiderInfo => to_result(id, api.get_guider_info().await),
        QueryKind::GuiderGraph => to_result(id, api.get_guider_graph().await),
        QueryKind::RotatorInfo => to_result(id, api.get_rotator_info().await),
        QueryKind::FocuserInfo => to_result(id, api.get_focuser_info().await),
        QueryKind::Command { endpoint, params } => {
            let borrowed: Vec<(&str, &str)> = params
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            to_result(id, api.execute_command(&endpoint, &borrowed).await)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_url_maps_schemes() {
        assert_eq!(
            direct_url("https://hub.example.com"),
            "wss://hub.example.com/v1/direct"
        );
        assert_eq!(
            direct_url("http://127.0.0.1:8092/"),
            "ws://127.0.0.1:8092/v1/direct"
        );
        assert_eq!(
            direct_url("wss://hub.example.com"),
            "wss://hub.example.com/v1/direct"
        );
        assert_eq!(direct_url("ws://localhost:1"), "ws://localhost:1/v1/direct");
    }

    #[test]
    fn relay_config_validation() {
        let ok = RelayConfig {
            hub_url: "wss://hub.example.com".to_string(),
            pairing_token: None,
            state_file: None,
        };
        assert!(ok.validate().is_ok());
        let bad = RelayConfig {
            hub_url: "ftp://hub".to_string(),
            pairing_token: None,
            state_file: None,
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn state_file_roundtrip_and_identity_stability() {
        let dir =
            std::env::temp_dir().join(format!("chatstronomy-relay-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        let _ = std::fs::remove_file(&path);

        let first = RelayState::load_or_create(&path).unwrap();
        assert!(first.credential.is_none());
        let mut updated = first.clone();
        updated.credential = Some("csrc_test".to_string());
        updated.save(&path).unwrap();

        let reloaded = RelayState::load_or_create(&path).unwrap();
        assert_eq!(reloaded.node_id, first.node_id);
        assert_eq!(reloaded.profile_id, first.profile_id);
        assert_eq!(reloaded.credential.as_deref(), Some("csrc_test"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_state_file_is_per_telescope() {
        let relay = RelayConfig {
            hub_url: "wss://h".to_string(),
            pairing_token: None,
            state_file: None,
        };
        assert_eq!(relay.state_file_for("c925"), "chatstronomy-relay-c925.json");
    }
}
