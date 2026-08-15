//! Telescope resolution and write authorization for bot commands.
//!
//! The Discord bot resolves "which telescope does this command mean" and
//! "may this user run write commands" through this trait, so the same
//! command set serves both a self-hosted bot (static config maps) and the
//! hub (database-backed, per-guild tenancy, live rig connections).

use crate::source::SharedRigSource;
use std::collections::{HashMap, HashSet};

/// Facts about a slash-command invocation that resolution and authorization
/// may use.
#[derive(Debug, Clone, Default)]
pub struct CommandContext {
    pub guild_id: Option<u64>,
    pub channel_id: u64,
    pub user_id: u64,
    /// The invoking member's role IDs (empty in DMs).
    pub role_ids: Vec<u64>,
}

pub trait RigResolver: Send + Sync {
    /// Resolve a telescope from an explicit override or the invocation's
    /// channel. The error is a user-facing message.
    fn resolve(
        &self,
        invocation: &CommandContext,
        override_name: Option<&str>,
    ) -> Result<(String, SharedRigSource), String>;

    /// May this user run write commands against this telescope? The error is
    /// a user-facing message.
    fn write_allowed(&self, invocation: &CommandContext, telescope: &str) -> Result<(), String>;
}

/// Config-file-backed resolver used by the self-hosted bot: fixed telescope
/// maps and a flat user-ID allowlist.
pub struct StaticRigResolver {
    /// One source-neutral rig connection per telescope, keyed by name.
    pub rig_sources: HashMap<String, SharedRigSource>,
    /// Discord channel ID -> telescope name.
    pub channel_to_telescope: HashMap<u64, String>,
    /// Discord user IDs allowed to invoke write commands.
    pub write_acl: HashSet<u64>,
}

impl StaticRigResolver {
    fn known_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.rig_sources.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }
}

impl RigResolver for StaticRigResolver {
    fn resolve(
        &self,
        invocation: &CommandContext,
        override_name: Option<&str>,
    ) -> Result<(String, SharedRigSource), String> {
        if let Some(name) = override_name {
            return self
                .rig_sources
                .get(name)
                .cloned()
                .map(|source| (name.to_string(), source))
                .ok_or_else(|| {
                    format!(
                        "Unknown telescope '{name}'. Known: {:?}",
                        self.known_names()
                    )
                });
        }
        if let Some(name) = self.channel_to_telescope.get(&invocation.channel_id) {
            let source = self
                .rig_sources
                .get(name)
                .cloned()
                .expect("channel_to_telescope -> rig_sources invariant");
            return Ok((name.clone(), source));
        }
        Err(format!(
            "No telescope mapped to this channel. Pass `telescope:<name>`. Known: {:?}",
            self.known_names()
        ))
    }

    fn write_allowed(&self, invocation: &CommandContext, _telescope: &str) -> Result<(), String> {
        if self.write_acl.contains(&invocation.user_id) {
            return Ok(());
        }
        Err(format!(
            "You are not authorized to run write commands. \
             Your user ID `{}` is not in `chat.discord_bot.write_acl`.",
            invocation.user_id
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ApiConfig;
    use crate::source::AdvancedApiSource;
    use std::sync::Arc;

    fn resolver() -> StaticRigResolver {
        let source: SharedRigSource = Arc::new(
            AdvancedApiSource::new(ApiConfig {
                base_url: "http://127.0.0.1:1888".to_string(),
                timeout_seconds: 5,
                retry_attempts: 0,
            })
            .unwrap(),
        );
        StaticRigResolver {
            rig_sources: HashMap::from([("c925".to_string(), source)]),
            channel_to_telescope: HashMap::from([(42, "c925".to_string())]),
            write_acl: HashSet::from([7]),
        }
    }

    fn invocation(channel_id: u64, user_id: u64) -> CommandContext {
        CommandContext {
            guild_id: Some(1),
            channel_id,
            user_id,
            role_ids: Vec::new(),
        }
    }

    #[test]
    fn resolves_by_name_and_channel() {
        let r = resolver();
        assert_eq!(r.resolve(&invocation(42, 7), None).unwrap().0, "c925");
        assert_eq!(
            r.resolve(&invocation(0, 7), Some("c925")).unwrap().0,
            "c925"
        );
    }

    #[test]
    fn unknown_name_and_unmapped_channel_error() {
        let r = resolver();
        assert!(
            r.resolve(&invocation(0, 7), Some("nope"))
                .err()
                .unwrap()
                .contains("Unknown")
        );
        assert!(
            r.resolve(&invocation(0, 7), None)
                .err()
                .unwrap()
                .contains("No telescope mapped")
        );
    }

    #[test]
    fn write_acl_gates_by_user_id() {
        let r = resolver();
        assert!(r.write_allowed(&invocation(42, 7), "c925").is_ok());
        assert!(r.write_allowed(&invocation(42, 8), "c925").is_err());
    }
}
