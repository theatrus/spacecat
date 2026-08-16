//! Machine-readable compatibility metadata for release consumers.

use serde::Serialize;

use crate::{direct::protocol, plugin_runtime, version};

pub const ARTIFACT_CONTRACT_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Serialize)]
pub struct ArtifactContract {
    pub schema_version: u16,
    pub product: &'static str,
    pub runtime_version: &'static str,
    pub git_sha: &'static str,
    pub flavor: &'static str,
    pub protocols: ProtocolContracts,
}

#[derive(Debug, Serialize)]
pub struct ProtocolContracts {
    pub direct: DirectContract,
    pub plugin_runtime: PluginRuntimeContract,
}

#[derive(Debug, Serialize)]
pub struct DirectContract {
    pub version: u16,
    pub websocket_path: &'static str,
    pub local_pipe_prefix: &'static str,
}

#[derive(Debug, Serialize)]
pub struct PluginRuntimeContract {
    pub version: u16,
}

pub fn current() -> ArtifactContract {
    ArtifactContract {
        schema_version: ARTIFACT_CONTRACT_SCHEMA_VERSION,
        product: "chatstronomy",
        runtime_version: version::VERSION,
        git_sha: version::GIT_SHA,
        flavor: if cfg!(feature = "hub") {
            "full"
        } else {
            "plugin"
        },
        protocols: ProtocolContracts {
            direct: DirectContract {
                version: protocol::PROTOCOL_VERSION,
                websocket_path: protocol::DIRECT_WEBSOCKET_PATH,
                local_pipe_prefix: protocol::LOCAL_PIPE_PREFIX,
            },
            plugin_runtime: PluginRuntimeContract {
                version: plugin_runtime::PLUGIN_RUNTIME_PROTOCOL_VERSION,
            },
        },
    }
}

pub fn json() -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&current())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_reports_protocols_from_their_owners() {
        let value: serde_json::Value = serde_json::from_str(&json().unwrap()).unwrap();

        assert_eq!(value["schema_version"], ARTIFACT_CONTRACT_SCHEMA_VERSION);
        assert_eq!(value["runtime_version"], version::VERSION);
        assert_eq!(
            value["protocols"]["direct"]["version"],
            protocol::PROTOCOL_VERSION
        );
        assert_eq!(
            value["protocols"]["plugin_runtime"]["version"],
            plugin_runtime::PLUGIN_RUNTIME_PROTOCOL_VERSION
        );
        assert_eq!(
            value["protocols"]["direct"]["websocket_path"],
            protocol::DIRECT_WEBSOCKET_PATH
        );
    }
}
