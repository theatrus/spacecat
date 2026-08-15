//! Database-backed telescope resolution for the hub's Discord bot.
//!
//! Telescope names are unique per guild, so explicit-name lookups are scoped
//! to the guild the command was invoked in. Channel routing is global —
//! Discord channel IDs are unique. Sources come from live `/v1/direct`
//! connections; an offline rig resolves to a clear error instead of a
//! hanging command.

use super::db::Db;
use super::direct_server::RigConnections;
use super::direct_source::DirectRigSource;
use super::tenants::TelescopeRow;
use crate::chat::{CommandContext, RigResolver};
use crate::source::SharedRigSource;
use std::sync::Arc;

pub struct HubRigResolver {
    db: Db,
    connections: Arc<RigConnections>,
}

impl HubRigResolver {
    pub fn new(db: Db, connections: Arc<RigConnections>) -> Self {
        Self { db, connections }
    }

    fn guild_names(&self, guild_id: i64) -> Vec<String> {
        self.db
            .guild_telescopes(guild_id)
            .map(|rows| rows.into_iter().map(|t| t.name).collect())
            .unwrap_or_default()
    }

    fn find_telescope(
        &self,
        invocation: &CommandContext,
        override_name: Option<&str>,
    ) -> Result<TelescopeRow, String> {
        if let Some(name) = override_name {
            let Some(guild_id) = invocation.guild_id else {
                return Err("Commands with a telescope name only work in a server".to_string());
            };
            return match self.db.telescope_by_guild_and_name(guild_id as i64, name) {
                Ok(Some(row)) => Ok(row),
                Ok(None) => Err(format!(
                    "Unknown telescope '{name}' in this server. Known: {:?}",
                    self.guild_names(guild_id as i64)
                )),
                Err(e) => Err(format!("Lookup failed: {e}")),
            };
        }
        match self.db.telescope_by_channel(invocation.channel_id as i64) {
            Ok(Some(row)) => Ok(row),
            Ok(None) => {
                let known = invocation
                    .guild_id
                    .map(|g| self.guild_names(g as i64))
                    .unwrap_or_default();
                Err(format!(
                    "No telescope routed to this channel. Pass `telescope:<name>`. Known: {known:?}"
                ))
            }
            Err(e) => Err(format!("Lookup failed: {e}")),
        }
    }
}

impl RigResolver for HubRigResolver {
    fn resolve(
        &self,
        invocation: &CommandContext,
        override_name: Option<&str>,
    ) -> Result<(String, SharedRigSource), String> {
        let row = self.find_telescope(invocation, override_name)?;
        // Cross-guild access through an explicit channel is impossible
        // (channel routing is set by the guild's own admins), but a name
        // resolved in guild A must never reach guild B's telescope — the
        // guild scope in find_telescope guarantees that.
        let Some(connection) = self.connections.get(row.id) else {
            return Err(format!(
                "Telescope '{}' is not connected to the hub right now.",
                row.name
            ));
        };
        let source: SharedRigSource = Arc::new(DirectRigSource::new(connection));
        Ok((row.name, source))
    }

    /// One lookup: the row that authorizes is the row that actuates. The
    /// default trait impl would re-resolve by name scoped to the invoker's
    /// guild, which can be a different telescope than the channel routed to.
    fn resolve_for_write(
        &self,
        invocation: &CommandContext,
        override_name: Option<&str>,
    ) -> Result<(String, SharedRigSource), String> {
        let row = self.find_telescope(invocation, override_name)?;
        check_write_policy(&row, invocation)?;
        let Some(connection) = self.connections.get(row.id) else {
            return Err(format!(
                "Telescope '{}' is not connected to the hub right now.",
                row.name
            ));
        };
        let source: SharedRigSource = Arc::new(DirectRigSource::new(connection));
        Ok((row.name, source))
    }

    fn write_allowed(&self, invocation: &CommandContext, telescope: &str) -> Result<(), String> {
        let row = self.find_telescope(invocation, Some(telescope))?;
        check_write_policy(&row, invocation)
    }
}

/// Per-telescope write policy.
///
/// - `disabled`: nobody, not even admins — a deliberate off switch.
/// - `admins` (the default): whoever manages the guild on Discord — its
///   owner or members holding ADMINISTRATOR/MANAGE_GUILD. The integration's
///   owner controls their scopes out of the box, straight from Discord.
/// - `roles`: guild managers plus members holding an allowlisted role.
fn check_write_policy(row: &TelescopeRow, invocation: &CommandContext) -> Result<(), String> {
    // `manages_guild` and `role_ids` describe the invoker's standing in the
    // guild the command came from. Discord's channel model already binds an
    // interaction to one guild, but this guard makes the invariant local:
    // an invoker's standing never authorizes another guild's telescope.
    if invocation.guild_id != Some(row.guild_id as u64) {
        return Err("This telescope belongs to a different server.".to_string());
    }
    match row.write_policy.as_str() {
        "admins" if invocation.manages_guild => Ok(()),
        "admins" => Err(
            "Write commands on this telescope are limited to server managers. \
             Ask an admin to add your role in the hub settings."
                .to_string(),
        ),
        "roles" => {
            let allowed = invocation.manages_guild
                || row
                    .allowed_role_ids
                    .iter()
                    .any(|role| invocation.role_ids.contains(&(*role as u64)));
            if allowed {
                Ok(())
            } else {
                Err(
                    "You are not authorized to run write commands for this telescope. \
                     Ask a server admin to grant your role in the hub settings."
                        .to_string(),
                )
            }
        }
        _ => Err(
            "Write commands are disabled for this telescope. A server admin can \
             enable them in the hub settings."
                .to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub::store::UserRow;
    use crate::hub::tenants::TelescopeUpdate;
    use uuid::Uuid;

    fn setup() -> (Db, Arc<RigConnections>, HubRigResolver, i64) {
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
        let telescope = db.create_telescope(100, "c925", 1).unwrap();
        db.update_telescope(
            telescope.id,
            &TelescopeUpdate {
                discord_channel_id: Some(Some(42)),
                ..Default::default()
            },
        )
        .unwrap();
        let connections = Arc::new(RigConnections::default());
        let resolver = HubRigResolver::new(db.clone(), connections.clone());
        (db, connections, resolver, telescope.id)
    }

    fn invocation(guild_id: u64, channel_id: u64, roles: Vec<u64>) -> CommandContext {
        CommandContext {
            guild_id: Some(guild_id),
            channel_id,
            user_id: 7,
            role_ids: roles,
            manages_guild: false,
        }
    }

    fn manager_invocation(guild_id: u64, channel_id: u64) -> CommandContext {
        CommandContext {
            manages_guild: true,
            ..invocation(guild_id, channel_id, Vec::new())
        }
    }

    fn connect(connections: &RigConnections, telescope_id: i64) {
        let (connection, _rx) =
            crate::hub::direct_server::RigConnection::stub(telescope_id, Uuid::new_v4());
        // Leak the receiver so the channel stays open for the test.
        std::mem::forget(_rx);
        connections.insert(connection);
    }

    #[test]
    fn resolves_by_channel_and_scoped_name() {
        let (_db, connections, resolver, id) = setup();
        connect(&connections, id);

        let by_channel = resolver
            .resolve(&invocation(100, 42, vec![]), None)
            .unwrap();
        assert_eq!(by_channel.0, "c925");

        let by_name = resolver
            .resolve(&invocation(100, 0, vec![]), Some("c925"))
            .unwrap();
        assert_eq!(by_name.0, "c925");

        // Same name from a different guild does not resolve.
        let cross_guild = resolver.resolve(&invocation(999, 0, vec![]), Some("c925"));
        assert!(cross_guild.err().unwrap().contains("Unknown telescope"));
    }

    #[test]
    fn offline_rig_reports_clearly() {
        let (_db, _connections, resolver, _id) = setup();
        let err = resolver
            .resolve(&invocation(100, 42, vec![]), None)
            .err()
            .unwrap();
        assert!(err.contains("not connected"));
    }

    #[test]
    fn unmapped_channel_lists_guild_telescopes() {
        let (_db, connections, resolver, id) = setup();
        connect(&connections, id);
        let err = resolver
            .resolve(&invocation(100, 555, vec![]), None)
            .err()
            .unwrap();
        assert!(err.contains("c925"));
    }

    #[test]
    fn default_policy_lets_managers_and_only_managers_write() {
        // New telescopes default to 'admins': the integration's owner and
        // other server managers control their scopes out of the box.
        let (_db, connections, resolver, id) = setup();
        connect(&connections, id);
        assert!(
            resolver
                .resolve_for_write(&manager_invocation(100, 42), None)
                .is_ok()
        );
        let err = resolver
            .resolve_for_write(&invocation(100, 42, vec![1111]), None)
            .err()
            .unwrap();
        assert!(err.contains("server managers"), "got: {err}");
    }

    #[test]
    fn disabled_policy_blocks_even_managers() {
        let (db, connections, resolver, id) = setup();
        connect(&connections, id);
        db.update_telescope(
            id,
            &TelescopeUpdate {
                write_policy: Some("disabled".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        let err = resolver
            .resolve_for_write(&manager_invocation(100, 42), None)
            .err()
            .unwrap();
        assert!(err.contains("disabled"), "got: {err}");
    }

    #[test]
    fn write_auth_applies_to_the_channel_routed_telescope() {
        // Guild 200 has a same-named telescope with a permissive role
        // policy. A guild-200 invocation reaching guild 100's channel-routed
        // rig (impossible on real Discord, but defended anyway) is denied by
        // the guild-match guard, whatever the invoker's standing.
        let (db, connections, resolver, id) = setup();
        connect(&connections, id);
        db.register_guild(200, "other", 1).unwrap();
        let other = db.create_telescope(200, "c925", 1).unwrap();
        db.update_telescope(
            other.id,
            &TelescopeUpdate {
                write_policy: Some("roles".to_string()),
                allowed_role_ids: Some(vec![9]),
                ..Default::default()
            },
        )
        .unwrap();

        let err = resolver
            .resolve_for_write(&invocation(200, 42, vec![9]), None)
            .err()
            .unwrap();
        assert!(err.contains("different server"), "got: {err}");
        let err = resolver
            .resolve_for_write(&manager_invocation(200, 42), None)
            .err()
            .unwrap();
        assert!(err.contains("different server"), "got: {err}");
    }

    #[test]
    fn write_policy_roles_matches_member_roles() {
        let (db, connections, resolver, id) = setup();
        connect(&connections, id);
        db.update_telescope(
            id,
            &TelescopeUpdate {
                write_policy: Some("roles".to_string()),
                allowed_role_ids: Some(vec![1111]),
                ..Default::default()
            },
        )
        .unwrap();

        assert!(
            resolver
                .write_allowed(&invocation(100, 42, vec![1111, 2222]), "c925")
                .is_ok()
        );
        assert!(
            resolver
                .write_allowed(&invocation(100, 42, vec![3333]), "c925")
                .is_err()
        );
        assert!(
            resolver
                .write_allowed(&invocation(100, 42, vec![]), "c925")
                .is_err()
        );
    }
}
