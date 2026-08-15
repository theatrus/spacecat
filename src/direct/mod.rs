//! Native N.I.N.A. Direct-mode contracts.
//!
//! The transport is intentionally separate from these cross-platform types so
//! identity, registration, and JSON compatibility can be tested on every CI
//! platform before the Windows named-pipe listener is introduced.

#[cfg(windows)]
pub mod pipe_source;
pub mod protocol;
pub mod registry;

pub use protocol::{
    AgentHello, ClientHello, DIRECT_WEBSOCKET_PATH, DirectMessage, LOCAL_PIPE_PREFIX,
    PROTOCOL_VERSION, RigId, local_pipe_name,
};
pub use registry::{
    DirectConnectionMode, DirectRigRegistry, RegisteredRig, Registration, RegistrationError,
};
