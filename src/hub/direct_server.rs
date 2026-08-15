//! The hub's `/v1/direct` WebSocket endpoint.
//!
//! N.I.N.A. plugins and relay agents connect outward to this endpoint. The
//! first frame authenticates: a one-time pairing token (first connect) or the
//! durable rig credential minted by that pairing. After the handshake the hub
//! sends [`QueryRequest`] frames and the rig answers with [`QueryResult`];
//! heartbeats keep NATs open. One telescope has one active connection — a
//! newer authenticated connection replaces the older one.

use super::server::HubState;
use crate::direct::protocol::{
    AgentHello, AuthRequest, ClientHello, DirectMessage, PROTOCOL_VERSION, PairRequest, QueryKind,
    QueryRequest, QueryResult, RigId,
};
use crate::source::RigCapabilities;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

/// How long the hub waits for the authentication frame.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Default wait for a rig to answer a query.
pub const QUERY_TIMEOUT: Duration = Duration::from_secs(20);

/// One live, authenticated rig connection.
pub struct RigConnection {
    pub telescope_id: i64,
    pub session_id: Uuid,
    pub capabilities: RigCapabilities,
    pub profile_name: String,
    outgoing: mpsc::UnboundedSender<DirectMessage>,
    pending: Mutex<HashMap<Uuid, oneshot::Sender<QueryResult>>>,
    /// Set when the hub wants this socket closed (e.g. credentials
    /// revoked). The write loop checks it after every frame.
    close_requested: std::sync::atomic::AtomicBool,
}

impl RigConnection {
    /// Send a query and await its result. Errors are strings so callers can
    /// wrap them in their own error type.
    pub async fn query(&self, kind: QueryKind, timeout: Duration) -> Result<QueryResult, String> {
        let id = Uuid::new_v4();
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().map_err(|_| "connection poisoned")?;
            pending.insert(id, tx);
        }
        // The rig must not execute this after the hub has stopped waiting.
        let expires_at = Some(crate::hub::db::unix_now() + timeout.as_secs() as i64);
        let sent = self.outgoing.send(DirectMessage::Query(QueryRequest {
            id,
            expires_at,
            kind,
        }));
        if sent.is_err() {
            self.remove_pending(&id);
            return Err("rig connection is closed".to_string());
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(_)) => Err("rig connection closed while waiting".to_string()),
            Err(_) => {
                self.remove_pending(&id);
                Err("rig did not answer in time".to_string())
            }
        }
    }

    fn remove_pending(&self, id: &Uuid) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(id);
        }
    }

    fn resolve(&self, result: QueryResult) {
        if let Ok(mut pending) = self.pending.lock()
            && let Some(tx) = pending.remove(&result.id)
        {
            let _ = tx.send(result);
        }
    }

    fn send(&self, message: DirectMessage) {
        let _ = self.outgoing.send(message);
    }

    /// Ask the write loop to send a final error frame and close the socket.
    /// `retryable` tells the client whether reconnecting can help.
    pub(crate) fn request_close(&self, reason: &str, retryable: bool) {
        self.close_requested
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.send(DirectMessage::Error {
            message: reason.to_string(),
            retryable,
        });
    }

    fn close_requested(&self) -> bool {
        self.close_requested
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Test-only connection with no live socket behind it. The receiver end
    /// of the outgoing channel is returned so tests can observe (or ignore)
    /// query traffic.
    #[cfg(test)]
    pub(crate) fn stub(
        telescope_id: i64,
        session_id: Uuid,
    ) -> (Arc<RigConnection>, mpsc::UnboundedReceiver<DirectMessage>) {
        let (outgoing, rx) = mpsc::unbounded_channel();
        (
            Arc::new(RigConnection {
                telescope_id,
                session_id,
                capabilities: crate::source::RigCapabilities::advanced_api(),
                profile_name: format!("stub-{telescope_id}"),
                outgoing,
                pending: Mutex::new(HashMap::new()),
                close_requested: std::sync::atomic::AtomicBool::new(false),
            }),
            rx,
        )
    }
}

/// Live connections by telescope. Shared between the WebSocket handler and
/// whatever consumes rigs (chat updaters, bot commands, tests).
#[derive(Default)]
pub struct RigConnections {
    inner: Mutex<HashMap<i64, Arc<RigConnection>>>,
}

impl RigConnections {
    pub fn get(&self, telescope_id: i64) -> Option<Arc<RigConnection>> {
        self.inner.lock().ok()?.get(&telescope_id).cloned()
    }

    pub fn connected_telescopes(&self) -> Vec<i64> {
        self.inner
            .lock()
            .map(|map| map.keys().copied().collect())
            .unwrap_or_default()
    }

    /// Insert a connection, returning the one it replaced (if any).
    pub(crate) fn insert(&self, connection: Arc<RigConnection>) -> Option<Arc<RigConnection>> {
        self.inner
            .lock()
            .ok()?
            .insert(connection.telescope_id, connection)
    }

    /// Force-remove a telescope's connection (e.g. credentials revoked) and
    /// return it so the caller can close the socket.
    pub(crate) fn remove(&self, telescope_id: i64) -> Option<Arc<RigConnection>> {
        self.inner.lock().ok()?.remove(&telescope_id)
    }

    /// Remove a connection, but only if this exact session still owns the
    /// slot — a replaced connection must not evict its replacement.
    pub(crate) fn remove_if_current(&self, telescope_id: i64, session_id: Uuid) {
        if let Ok(mut map) = self.inner.lock()
            && map
                .get(&telescope_id)
                .is_some_and(|c| c.session_id == session_id)
        {
            map.remove(&telescope_id);
        }
    }
}

pub async fn direct_ws(
    State(state): State<HubState>,
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    let ip = crate::hub::server::client_ip(&headers, peer, state.config.trust_x_forwarded_for);
    upgrade.on_upgrade(move |socket| handle_socket(state, socket, ip))
}

async fn recv_message(socket: &mut WebSocket) -> Option<DirectMessage> {
    while let Some(frame) = socket.recv().await {
        match frame {
            Ok(Message::Text(text)) => match serde_json::from_str(&text) {
                Ok(message) => return Some(message),
                Err(_) => return None,
            },
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => continue,
            _ => return None,
        }
    }
    None
}

async fn send_message(socket: &mut WebSocket, message: &DirectMessage) -> bool {
    let Ok(json) = serde_json::to_string(message) else {
        return false;
    };
    socket.send(Message::Text(json.into())).await.is_ok()
}

async fn reject(mut socket: WebSocket, message: &str, retryable: bool) {
    let _ = send_message(
        &mut socket,
        &DirectMessage::Error {
            message: message.to_string(),
            retryable,
        },
    )
    .await;
    let _ = socket.send(Message::Close(None)).await;
}

/// What to undo if the PairResult carrying the freshly minted credential
/// never reaches the client: restore the token, drop the orphan credential.
struct PairRollback {
    token: String,
    credential: String,
}

/// Authenticate the first frame: either a pairing exchange or a credential
/// presentation. Returns the telescope, the hello, the response to send,
/// and — for pairing — the rollback data for a failed delivery.
fn authenticate(
    state: &HubState,
    first: &DirectMessage,
) -> Result<(i64, ClientHello, DirectMessage, Option<PairRollback>), String> {
    match first {
        DirectMessage::Pair(PairRequest {
            pairing_token,
            hello,
        }) => {
            check_hello(hello)?;
            let telescope_id = state
                .db
                .consume_pairing_token(pairing_token)
                .map_err(|e| format!("database error: {e}"))?
                .ok_or("pairing token is unknown, expired, or already used")?;
            // Pairing rotates: earlier credentials die so a retired install
            // can never reconnect and displace the new rig.
            if let Ok(revoked) = state.db.revoke_rig_credentials(telescope_id)
                && revoked > 0
            {
                println!(
                    "Pairing for telescope {telescope_id} revoked {revoked} earlier credential(s)"
                );
            }
            let credential = state
                .db
                .create_rig_credential(
                    telescope_id,
                    &hello.node_id.to_string(),
                    &hello.profile_id.to_string(),
                )
                .map_err(|e| format!("database error: {e}"))?;
            let response = DirectMessage::PairResult(crate::direct::protocol::PairResult {
                credential: credential.clone(),
                agent_hello: agent_hello(hello),
            });
            Ok((
                telescope_id,
                hello.clone(),
                response,
                Some(PairRollback {
                    token: pairing_token.clone(),
                    credential,
                }),
            ))
        }
        DirectMessage::Auth(AuthRequest { credential, hello }) => {
            check_hello(hello)?;
            let row = state
                .db
                .lookup_rig_credential(credential)
                .map_err(|e| format!("database error: {e}"))?
                .ok_or("credential is unknown or revoked")?;
            // The credential is bound to the installation that paired it.
            if row.node_id != hello.node_id.to_string() {
                return Err("credential is bound to a different node".to_string());
            }
            let response = DirectMessage::AgentHello(agent_hello(hello));
            Ok((row.telescope_id, hello.clone(), response, None))
        }
        _ => Err("first frame must be pair or auth".to_string()),
    }
}

fn check_hello(hello: &ClientHello) -> Result<(), String> {
    if hello.protocol_version != PROTOCOL_VERSION {
        return Err(format!(
            "unsupported protocol version {} (hub speaks {PROTOCOL_VERSION})",
            hello.protocol_version
        ));
    }
    Ok(())
}

fn agent_hello(hello: &ClientHello) -> AgentHello {
    AgentHello {
        protocol_version: PROTOCOL_VERSION,
        connection_id: Uuid::new_v4(),
        rig_id: RigId {
            node_id: hello.node_id,
            profile_id: hello.profile_id,
        },
    }
}

async fn handle_socket(state: HubState, mut socket: WebSocket, client_ip: String) {
    // Token/credential guessing protection: an IP with too many recent
    // failures is refused before any database work.
    if state.limits.direct_auth.blocked(&client_ip) {
        reject(
            socket,
            "too many failed attempts; try again in a minute",
            true,
        )
        .await;
        return;
    }

    let first = match tokio::time::timeout(HANDSHAKE_TIMEOUT, recv_message(&mut socket)).await {
        Ok(Some(message)) => message,
        _ => {
            reject(socket, "expected an authentication frame", true).await;
            return;
        }
    };

    let (telescope_id, hello, response, pair_rollback) = match authenticate(&state, &first) {
        Ok(ok) => ok,
        Err(message) => {
            state.limits.direct_auth.check(&client_ip);
            reject(socket, &message, false).await;
            return;
        }
    };
    if !send_message(&mut socket, &response).await {
        // A pairing reply that never arrived means the client still has no
        // credential: give the token back and drop the orphan credential so
        // the client's retry with the same token works.
        if let Some(rollback) = pair_rollback {
            let _ = state.db.delete_rig_credential(&rollback.credential);
            let _ = state.db.restore_pairing_token(&rollback.token);
            println!("Pairing reply for telescope {telescope_id} was not delivered; rolled back");
        }
        return;
    }

    let (outgoing_tx, mut outgoing_rx) = mpsc::unbounded_channel();
    let connection = Arc::new(RigConnection {
        telescope_id,
        session_id: hello.session_id,
        capabilities: hello.capabilities,
        profile_name: hello.profile_name.clone(),
        outgoing: outgoing_tx,
        pending: Mutex::new(HashMap::new()),
        close_requested: std::sync::atomic::AtomicBool::new(false),
    });
    // A newer connection for the same telescope replaces the older one. The
    // old session is told to close — its own task holds an Arc of its
    // connection, so only an explicit close ends its write loop.
    if let Some(replaced) = state.rig_connections.insert(connection.clone()) {
        replaced.request_close("replaced by a newer connection for this telescope", true);
        println!("Rig for telescope {telescope_id} reconnected; replacing previous session");
    } else {
        println!(
            "Rig connected for telescope {telescope_id} ({})",
            hello.profile_name
        );
    }

    let session_id = connection.session_id;
    loop {
        tokio::select! {
            outbound = outgoing_rx.recv() => {
                match outbound {
                    Some(message) => {
                        if !send_message(&mut socket, &message).await {
                            break;
                        }
                        if connection.close_requested() {
                            let _ = socket.send(Message::Close(None)).await;
                            break;
                        }
                    }
                    // Sender dropped: this connection was replaced.
                    None => break,
                }
            }
            inbound = socket.recv() => {
                let Some(Ok(frame)) = inbound else { break };
                match frame {
                    Message::Text(text) => {
                        match serde_json::from_str::<DirectMessage>(&text) {
                            Ok(DirectMessage::QueryResult(result)) => connection.resolve(result),
                            Ok(DirectMessage::Heartbeat { seq }) => {
                                connection.send(DirectMessage::HeartbeatAck { seq });
                            }
                            Ok(_) | Err(_) => {}
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
        }
    }

    state
        .rig_connections
        .remove_if_current(telescope_id, session_id);
    println!("Rig disconnected for telescope {telescope_id}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ApiConfig, TelescopeConfig};
    use crate::hub::config::HubConfig;
    use crate::hub::db::Db;
    use crate::hub::direct_source::DirectRigSource;
    use crate::hub::store::UserRow;
    use crate::relay::{RelayConfig, RelayState, run_connection};
    use crate::source::RigSource;
    use axum::routing::get;
    use axum::{Json, Router};

    /// Stub of the two NINA Advanced API endpoints the tests exercise.
    async fn spawn_stub_nina() -> String {
        async fn event_history() -> Json<serde_json::Value> {
            Json(serde_json::json!({
                "Response": [
                    {"Time": "2026-08-14T01:02:03", "Event": "IMAGE-SAVE"},
                    {"Time": "2026-08-14T01:05:00", "Event": "SEQUENCE-FINISHED"}
                ],
                "Error": "",
                "StatusCode": 200,
                "Success": true,
                "Type": "API"
            }))
        }
        async fn unpark() -> Json<serde_json::Value> {
            Json(serde_json::json!({
                "Response": "Mount unparked",
                "Error": "",
                "StatusCode": 200,
                "Success": true,
                "Type": "API"
            }))
        }
        let app = Router::new()
            .route("/v2/api/event-history", get(event_history))
            .route("/v2/api/equipment/mount/unpark", get(unpark));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    /// Hub with a telescope, an issued pairing token, and the direct route.
    async fn spawn_hub() -> (String, Db, HubState, i64, String) {
        let db = Db::open_in_memory().unwrap();
        db.upsert_user(&UserRow {
            discord_user_id: 1,
            username: "admin".to_string(),
            email: None,
            email_verified: false,
            avatar_url: None,
        })
        .unwrap();
        db.register_guild(100, "g", 1).unwrap();
        let telescope = db.create_telescope(1, "c925").unwrap();
        let pairing_token = db.issue_pairing_token(telescope.id, 1).unwrap();

        let state = HubState::build(HubConfig::default(), db.clone(), None).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = crate::hub::server::router(state.clone());
        tokio::spawn(async move {
            axum::serve(
                listener,
                router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap()
        });
        (
            format!("http://{addr}"),
            db,
            state,
            telescope.id,
            pairing_token,
        )
    }

    fn relay_setup(
        nina_base: &str,
        hub_base: &str,
        pairing_token: Option<String>,
        state_dir: &std::path::Path,
    ) -> (RelayConfig, TelescopeConfig, String) {
        let state_path = state_dir.join("relay-state.json");
        let relay = RelayConfig {
            hub_url: hub_base.to_string(),
            pairing_token,
            state_file: Some(state_path.to_string_lossy().to_string()),
        };
        let telescope = TelescopeConfig {
            name: "c925".to_string(),
            api: ApiConfig {
                base_url: nina_base.to_string(),
                timeout_seconds: 5,
                retry_attempts: 0,
            },
            relay: Some(relay.clone()),
            ..Default::default()
        };
        (relay, telescope, state_path.to_string_lossy().to_string())
    }

    async fn wait_for_connection(state: &HubState, telescope_id: i64) -> Arc<RigConnection> {
        for _ in 0..100 {
            if let Some(connection) = state.rig_connections.get(telescope_id) {
                return connection;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("rig never connected");
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "chatstronomy-direct-test-{tag}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn pair_query_and_command_roundtrip() {
        let nina = spawn_stub_nina().await;
        let (hub_base, _db, state, telescope_id, pairing_token) = spawn_hub().await;
        let dir = temp_dir("roundtrip");
        let (relay, telescope, state_path) =
            relay_setup(&nina, &hub_base, Some(pairing_token), &dir);

        let api = crate::api::ChatstronomyApiClient::new(telescope.api.clone()).unwrap();
        let mut relay_state = RelayState::load_or_create(&state_path).unwrap();
        let relay_task = {
            let relay = relay.clone();
            let telescope = telescope.clone();
            let api = api.clone();
            let state_path = state_path.clone();
            tokio::spawn(async move {
                let _ =
                    run_connection(&relay, &telescope, &api, &mut relay_state, &state_path).await;
            })
        };

        let connection = wait_for_connection(&state, telescope_id).await;
        assert_eq!(connection.profile_name, "c925");
        let source = DirectRigSource::new(connection);

        // Read path: event history proxied from the stub NINA API.
        let events = source.get_event_history().await.unwrap();
        assert_eq!(events.response.len(), 2);
        assert_eq!(
            events.response[0].event,
            crate::events::event_types::IMAGE_SAVE
        );

        // Command path.
        let result = source
            .execute_command(crate::source::RigCommand::UnparkMount)
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.response, serde_json::json!("Mount unparked"));

        // Pairing minted and saved a credential.
        let saved = RelayState::load_or_create(&state_path).unwrap();
        assert!(
            saved
                .credential
                .as_deref()
                .is_some_and(|c| c.starts_with(crate::hub::tenants::RIG_CREDENTIAL_PREFIX))
        );

        relay_task.abort();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn reconnect_with_credential_and_node_binding() {
        let nina = spawn_stub_nina().await;
        let (hub_base, db, state, telescope_id, pairing_token) = spawn_hub().await;
        let dir = temp_dir("reconnect");
        let (relay, telescope, state_path) =
            relay_setup(&nina, &hub_base, Some(pairing_token), &dir);
        let api = crate::api::ChatstronomyApiClient::new(telescope.api.clone()).unwrap();

        // First connection pairs.
        let mut relay_state = RelayState::load_or_create(&state_path).unwrap();
        let first = {
            let (relay, telescope, api, state_path) = (
                relay.clone(),
                telescope.clone(),
                api.clone(),
                state_path.clone(),
            );
            tokio::spawn(async move {
                let _ =
                    run_connection(&relay, &telescope, &api, &mut relay_state, &state_path).await;
            })
        };
        wait_for_connection(&state, telescope_id).await;
        first.abort();

        // Second connection authenticates with the stored credential (the
        // pairing token is already consumed).
        let mut relay_state = RelayState::load_or_create(&state_path).unwrap();
        assert!(relay_state.credential.is_some());
        let second = {
            let (relay, telescope, api, state_path) = (
                relay.clone(),
                telescope.clone(),
                api.clone(),
                state_path.clone(),
            );
            tokio::spawn(async move {
                let _ =
                    run_connection(&relay, &telescope, &api, &mut relay_state, &state_path).await;
            })
        };
        // The first task was aborted, so the slot may still hold the stale
        // connection; wait until a live query works.
        let mut ok = false;
        for _ in 0..100 {
            if let Some(connection) = state.rig_connections.get(telescope_id) {
                let source = DirectRigSource::new(connection);
                if source.get_event_history().await.is_ok() {
                    ok = true;
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(ok, "credential reconnect never became queryable");
        second.abort();

        // A credential bound to another node is rejected.
        let credential_row = db
            .with_conn(|conn| {
                conn.query_row("SELECT credential_hash FROM rig_credentials", [], |r| {
                    r.get::<_, String>(0)
                })
            })
            .unwrap();
        assert!(!credential_row.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn request_close_sends_final_error_frame() {
        let connections = RigConnections::default();
        let (connection, mut rx) = RigConnection::stub(7, Uuid::new_v4());
        connections.insert(connection);

        let removed = connections.remove(7).expect("connection present");
        removed.request_close("credentials revoked", false);
        assert!(connections.get(7).is_none());
        assert!(removed.close_requested());

        let frame = rx.recv().await.expect("close error frame");
        let DirectMessage::Error { message, .. } = frame else {
            panic!("expected an error frame, got {frame:?}");
        };
        assert!(message.contains("revoked"));
    }

    #[tokio::test]
    async fn bad_pairing_token_rejected() {
        let nina = spawn_stub_nina().await;
        let (hub_base, _db, _state, _telescope_id, _token) = spawn_hub().await;
        let dir = temp_dir("badtoken");
        let (relay, telescope, state_path) =
            relay_setup(&nina, &hub_base, Some("cspt_bogus".to_string()), &dir);
        let api = crate::api::ChatstronomyApiClient::new(telescope.api.clone()).unwrap();
        let mut relay_state = RelayState::load_or_create(&state_path).unwrap();

        let result = run_connection(&relay, &telescope, &api, &mut relay_state, &state_path).await;
        assert!(matches!(result, Err(crate::relay::RelayError::Rejected(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
