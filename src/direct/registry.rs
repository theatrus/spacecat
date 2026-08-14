//! Multi-instance registration for native N.I.N.A. connections.

use super::protocol::{ClientHello, PROTOCOL_VERSION, RigId};
use std::collections::HashMap;
use thiserror::Error;
use uuid::Uuid;

/// Trusted transport selected by the SpaceCat listener, not claimed by the
/// plugin. Local is the default deployment; Remote is an explicit WSS option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectConnectionMode {
    LocalPipe,
    RemoteWebSocket,
}

/// One active N.I.N.A. plugin connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredRig {
    pub rig_id: RigId,
    pub connection_id: Uuid,
    pub connection_mode: DirectConnectionMode,
    pub hello: ClientHello,
}

/// Result of an accepted registration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registration {
    pub rig: RegisteredRig,
    /// Set when the same N.I.N.A. process reconnected before its old transport
    /// handler observed the disconnect.
    pub previous_connection_id: Option<Uuid>,
    /// Set when a running N.I.N.A. process changed its active profile on the
    /// same connection.
    pub previous_rig_id: Option<RigId>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RegistrationError {
    #[error("Direct protocol {offered} is unsupported; this hub supports {supported}")]
    UnsupportedProtocol { offered: u16, supported: u16 },

    #[error("invalid Direct hello field: {field}")]
    InvalidHello { field: &'static str },

    #[error(
        "N.I.N.A. profile {rig_id} is already active in process {active_process_id} (session {active_session_id})"
    )]
    ProfileAlreadyActive {
        rig_id: RigId,
        active_session_id: Uuid,
        active_process_id: u32,
    },
}

/// All native N.I.N.A. instances connected to one SpaceCat hub.
///
/// A profile has one authoritative process at a time. Different profiles may
/// connect concurrently from the same or different nodes, and reconnecting the
/// same process atomically replaces its stale connection so delayed disconnect
/// notifications are harmless.
#[derive(Debug, Default)]
pub struct DirectRigRegistry {
    by_rig: HashMap<RigId, RegisteredRig>,
    by_connection: HashMap<Uuid, RigId>,
}

impl DirectRigRegistry {
    pub fn new() -> Self {
        Self {
            by_rig: HashMap::new(),
            by_connection: HashMap::new(),
        }
    }

    /// Register or reconnect a plugin instance using the simple local mode.
    pub fn register(
        &mut self,
        connection_id: Uuid,
        hello: ClientHello,
    ) -> Result<Registration, RegistrationError> {
        self.register_with_mode(connection_id, DirectConnectionMode::LocalPipe, hello)
    }

    /// Register or reconnect a plugin instance using an explicit transport.
    ///
    /// Reusing a connection for a new profile performs an atomic profile
    /// switch. A second process claiming an already active profile is rejected
    /// so commands can never be routed ambiguously.
    pub fn register_with_mode(
        &mut self,
        connection_id: Uuid,
        connection_mode: DirectConnectionMode,
        hello: ClientHello,
    ) -> Result<Registration, RegistrationError> {
        self.validate(connection_id, &hello)?;

        let rig_id = RigId {
            node_id: hello.node_id,
            profile_id: hello.profile_id,
        };

        let previous_rig_id = self.by_connection.get(&connection_id).copied();
        let active = self.by_rig.get(&rig_id);

        if let Some(active) = active
            && active.hello.session_id != hello.session_id
        {
            return Err(RegistrationError::ProfileAlreadyActive {
                rig_id,
                active_session_id: active.hello.session_id,
                active_process_id: active.hello.process_id,
            });
        }

        let previous_connection_id = active
            .filter(|active| active.connection_id != connection_id)
            .map(|active| active.connection_id);

        // A profile change on an existing connection must remove the previous
        // rig only after the target profile has passed duplicate checks.
        if let Some(previous_rig_id) = previous_rig_id
            && previous_rig_id != rig_id
        {
            self.by_rig.remove(&previous_rig_id);
        }

        // A reconnect replaces the old connection index before insertion.
        if let Some(previous_connection_id) = previous_connection_id {
            self.by_connection.remove(&previous_connection_id);
        }

        let rig = RegisteredRig {
            rig_id,
            connection_id,
            connection_mode,
            hello,
        };
        self.by_rig.insert(rig_id, rig.clone());
        self.by_connection.insert(connection_id, rig_id);

        Ok(Registration {
            rig,
            previous_connection_id,
            previous_rig_id: previous_rig_id.filter(|previous| *previous != rig_id),
        })
    }

    /// Remove the rig owned by a closed connection.
    ///
    /// If the session already reconnected, its old connection ID is no longer
    /// indexed and this becomes a no-op.
    pub fn disconnect(&mut self, connection_id: Uuid) -> Option<RegisteredRig> {
        let rig_id = self.by_connection.remove(&connection_id)?;
        let active = self.by_rig.get(&rig_id)?;
        if active.connection_id != connection_id {
            return None;
        }
        self.by_rig.remove(&rig_id)
    }

    pub fn get(&self, rig_id: &RigId) -> Option<&RegisteredRig> {
        self.by_rig.get(rig_id)
    }

    pub fn rig_for_connection(&self, connection_id: Uuid) -> Option<&RegisteredRig> {
        self.by_connection
            .get(&connection_id)
            .and_then(|rig_id| self.by_rig.get(rig_id))
    }

    pub fn rigs(&self) -> impl Iterator<Item = &RegisteredRig> {
        self.by_rig.values()
    }

    pub fn len(&self) -> usize {
        self.by_rig.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_rig.is_empty()
    }

    fn validate(&self, connection_id: Uuid, hello: &ClientHello) -> Result<(), RegistrationError> {
        if hello.protocol_version != PROTOCOL_VERSION {
            return Err(RegistrationError::UnsupportedProtocol {
                offered: hello.protocol_version,
                supported: PROTOCOL_VERSION,
            });
        }
        if hello.node_id.is_nil() {
            return Err(RegistrationError::InvalidHello { field: "node_id" });
        }
        if connection_id.is_nil() {
            return Err(RegistrationError::InvalidHello {
                field: "connection_id",
            });
        }
        if hello.session_id.is_nil() {
            return Err(RegistrationError::InvalidHello {
                field: "session_id",
            });
        }
        if hello.profile_id.is_nil() {
            return Err(RegistrationError::InvalidHello {
                field: "profile_id",
            });
        }
        if hello.process_id == 0 {
            return Err(RegistrationError::InvalidHello {
                field: "process_id",
            });
        }
        if hello.profile_name.trim().is_empty() {
            return Err(RegistrationError::InvalidHello {
                field: "profile_name",
            });
        }
        if hello.plugin_version.trim().is_empty() {
            return Err(RegistrationError::InvalidHello {
                field: "plugin_version",
            });
        }
        if hello.nina_version.trim().is_empty() {
            return Err(RegistrationError::InvalidHello {
                field: "nina_version",
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::RigCapabilities;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn hello(node_id: u128, profile_id: u128, session_id: u128, process_id: u32) -> ClientHello {
        ClientHello {
            protocol_version: PROTOCOL_VERSION,
            node_id: id(node_id),
            session_id: id(session_id),
            process_id,
            profile_id: id(profile_id),
            profile_name: format!("Rig {profile_id}"),
            plugin_version: "0.1.0.0".to_string(),
            nina_version: "3.2.0.9001".to_string(),
            capabilities: RigCapabilities::none(),
        }
    }

    #[test]
    fn registers_multiple_profiles_under_one_hub() {
        let mut registry = DirectRigRegistry::new();

        let north = registry.register(id(101), hello(1, 11, 21, 1001)).unwrap();
        let south = registry.register(id(102), hello(1, 12, 22, 1002)).unwrap();

        assert_eq!(registry.len(), 2);
        assert_ne!(north.rig.rig_id, south.rig.rig_id);
        assert_eq!(north.rig.rig_id.node_id, id(1));
        assert_eq!(south.rig.rig_id.node_id, id(1));
        assert_eq!(north.rig.connection_mode, DirectConnectionMode::LocalPipe);
    }

    #[test]
    fn same_profile_guid_on_different_systems_is_not_a_collision() {
        let mut registry = DirectRigRegistry::new();

        let observatory_a = registry
            .register_with_mode(
                id(101),
                DirectConnectionMode::RemoteWebSocket,
                hello(1, 11, 21, 1001),
            )
            .unwrap();
        let observatory_b = registry
            .register_with_mode(
                id(102),
                DirectConnectionMode::RemoteWebSocket,
                hello(2, 11, 22, 1002),
            )
            .unwrap();

        assert_eq!(observatory_a.rig.rig_id.profile_id, id(11));
        assert_eq!(observatory_b.rig.rig_id.profile_id, id(11));
        assert_ne!(observatory_a.rig.rig_id, observatory_b.rig.rig_id);
        assert_eq!(
            observatory_a.rig.connection_mode,
            DirectConnectionMode::RemoteWebSocket
        );
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn rejects_two_processes_using_the_same_profile() {
        let mut registry = DirectRigRegistry::new();
        registry.register(id(101), hello(1, 11, 21, 1001)).unwrap();

        let error = registry
            .register(id(102), hello(1, 11, 22, 1002))
            .unwrap_err();

        assert_eq!(
            error,
            RegistrationError::ProfileAlreadyActive {
                rig_id: RigId {
                    node_id: id(1),
                    profile_id: id(11),
                },
                active_session_id: id(21),
                active_process_id: 1001,
            }
        );
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn reconnect_replaces_stale_connection_for_the_same_session() {
        let mut registry = DirectRigRegistry::new();
        registry
            .register_with_mode(
                id(101),
                DirectConnectionMode::RemoteWebSocket,
                hello(1, 11, 21, 1001),
            )
            .unwrap();

        let registration = registry
            .register_with_mode(
                id(102),
                DirectConnectionMode::RemoteWebSocket,
                hello(1, 11, 21, 1001),
            )
            .unwrap();

        assert_eq!(registration.previous_connection_id, Some(id(101)));
        assert_eq!(registration.rig.connection_id, id(102));
        assert_eq!(
            registration.rig.connection_mode,
            DirectConnectionMode::RemoteWebSocket
        );
        assert!(registry.rig_for_connection(id(101)).is_none());
        assert!(registry.rig_for_connection(id(102)).is_some());
        assert!(registry.disconnect(id(101)).is_none());
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn profile_change_moves_an_existing_connection_atomically() {
        let mut registry = DirectRigRegistry::new();
        let first = registry.register(id(101), hello(1, 11, 21, 1001)).unwrap();

        let switched = registry.register(id(101), hello(1, 12, 21, 1001)).unwrap();

        assert_eq!(switched.previous_rig_id, Some(first.rig.rig_id));
        assert!(registry.get(&first.rig.rig_id).is_none());
        assert_eq!(registry.rig_for_connection(id(101)), Some(&switched.rig));
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn failed_profile_change_preserves_the_original_registration() {
        let mut registry = DirectRigRegistry::new();
        let north = registry.register(id(101), hello(1, 11, 21, 1001)).unwrap();
        registry.register(id(102), hello(1, 12, 22, 1002)).unwrap();

        let error = registry
            .register(id(101), hello(1, 12, 21, 1001))
            .unwrap_err();

        assert!(matches!(
            error,
            RegistrationError::ProfileAlreadyActive { .. }
        ));
        assert_eq!(registry.rig_for_connection(id(101)), Some(&north.rig));
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn disconnected_profile_can_be_claimed_by_a_new_process() {
        let mut registry = DirectRigRegistry::new();
        registry.register(id(101), hello(1, 11, 21, 1001)).unwrap();
        registry.disconnect(id(101)).unwrap();

        let new_process = registry.register(id(102), hello(1, 11, 22, 1002)).unwrap();

        assert_eq!(new_process.rig.hello.process_id, 1002);
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn rejects_incompatible_protocol_versions() {
        let mut registry = DirectRigRegistry::new();
        let mut incompatible = hello(1, 11, 21, 1001);
        incompatible.protocol_version = PROTOCOL_VERSION + 1;

        assert_eq!(
            registry.register(id(101), incompatible),
            Err(RegistrationError::UnsupportedProtocol {
                offered: PROTOCOL_VERSION + 1,
                supported: PROTOCOL_VERSION,
            })
        );
    }
}
