//! Versioned messages exchanged between N.I.N.A. plugins and a Chatstronomy hub.

use crate::source::RigCapabilities;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The first Chatstronomy Direct protocol version.
pub const PROTOCOL_VERSION: u16 = 1;

/// Prefix for the per-installation Windows named pipe used by local plugins.
///
/// Windows APIs expect the pipe name without the `\\.\pipe\` prefix when
/// opening a `NamedPipeClientStream`.
pub const LOCAL_PIPE_PREFIX: &str = "chatstronomy-agent-v1";

/// Default route exposed by a remote Chatstronomy hub for Direct plugin clients.
pub const DIRECT_WEBSOCKET_PATH: &str = "/v1/direct";

/// Derive the local pipe name shared by Chatstronomy and every N.I.N.A. instance
/// in the same plugin installation. Including the node ID prevents collisions
/// between different Windows users or installations on one system.
pub fn local_pipe_name(node_id: Uuid) -> String {
    format!("{LOCAL_PIPE_PREFIX}-{}", node_id.simple())
}

/// Stable identity assigned to one N.I.N.A. profile on one host.
///
/// Profile GUIDs are only assumed to be unique within a node. Combining the
/// plugin's persistent node ID with the profile ID allows one Chatstronomy hub to
/// distinguish rigs on different systems without using addresses or ports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RigId {
    pub node_id: Uuid,
    pub profile_id: Uuid,
}

impl std::fmt::Display for RigId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.node_id, self.profile_id)
    }
}

/// First message sent by each loaded Chatstronomy N.I.N.A. plugin instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientHello {
    pub protocol_version: u16,
    pub node_id: Uuid,
    pub session_id: Uuid,
    pub process_id: u32,
    pub profile_id: Uuid,
    pub profile_name: String,
    pub plugin_version: String,
    pub nina_version: String,
    pub capabilities: RigCapabilities,
}

/// Registration response returned by the Chatstronomy hub.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentHello {
    pub protocol_version: u16,
    pub connection_id: Uuid,
    pub rig_id: RigId,
}

/// Initial set of Direct messages. Snapshot, event, query, and command payloads
/// will be added without changing the identity handshake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum DirectMessage {
    ClientHello(ClientHello),
    AgentHello(AgentHello),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_client_hello() -> ClientHello {
        ClientHello {
            protocol_version: PROTOCOL_VERSION,
            node_id: Uuid::parse_str("363db028-9d79-4fdc-8940-1b1ff52b9e8d").unwrap(),
            session_id: Uuid::parse_str("7afcde18-b5a8-46fd-ad1f-ed54cf3bbc4e").unwrap(),
            process_id: 4242,
            profile_id: Uuid::parse_str("460a8c62-28ce-4781-92e5-ab2440982175").unwrap(),
            profile_name: "North Rig".to_string(),
            plugin_version: "0.1.0.0".to_string(),
            nina_version: "3.2.0.9001".to_string(),
            capabilities: RigCapabilities::none(),
        }
    }

    #[test]
    fn client_hello_has_stable_json_contract() {
        let message = DirectMessage::ClientHello(sample_client_hello());
        let value = serde_json::to_value(message).unwrap();

        assert_eq!(value["type"], "client_hello");
        assert_eq!(value["payload"]["protocol_version"], PROTOCOL_VERSION);
        assert_eq!(
            value["payload"]["node_id"],
            "363db028-9d79-4fdc-8940-1b1ff52b9e8d"
        );
        assert_eq!(value["payload"]["process_id"], 4242);
        assert_eq!(
            value["payload"]["profile_id"],
            "460a8c62-28ce-4781-92e5-ab2440982175"
        );
        assert_eq!(value["payload"]["profile_name"], "North Rig");
        assert_eq!(
            value["payload"]["capabilities"],
            json!({
                "event_history": false,
                "image_history": false,
                "thumbnails": false,
                "sequence": false,
                "equipment_snapshots": false,
                "autofocus_details": false,
                "guider_graph": false,
                "commands": false
            })
        );
    }

    #[test]
    fn rig_id_display_is_stable_and_unambiguous() {
        let rig_id = RigId {
            node_id: Uuid::parse_str("363db028-9d79-4fdc-8940-1b1ff52b9e8d").unwrap(),
            profile_id: Uuid::parse_str("460a8c62-28ce-4781-92e5-ab2440982175").unwrap(),
        };

        assert_eq!(
            rig_id.to_string(),
            "363db028-9d79-4fdc-8940-1b1ff52b9e8d/460a8c62-28ce-4781-92e5-ab2440982175"
        );
    }

    #[test]
    fn local_pipe_name_is_scoped_to_the_node() {
        let node_id = Uuid::parse_str("363db028-9d79-4fdc-8940-1b1ff52b9e8d").unwrap();

        assert_eq!(
            local_pipe_name(node_id),
            "chatstronomy-agent-v1-363db0289d794fdc89401b1ff52b9e8d"
        );
    }
}
