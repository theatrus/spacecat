//! Tenancy persistence: registered guilds, their telescopes, and one-time
//! pairing tokens.

use super::db::{Db, DbError, unix_now};
use rusqlite::OptionalExtension;

/// Pairing tokens expire after an hour; they exist only to move a secret
/// from the web page into the N.I.N.A. plugin settings.
pub const PAIRING_TOKEN_TTL_SECONDS: i64 = 3600;

/// Prefix that makes a leaked pairing token recognizable in scans.
pub const PAIRING_TOKEN_PREFIX: &str = "cspt_";

/// Prefix for durable rig credentials minted by the pairing exchange.
pub const RIG_CREDENTIAL_PREFIX: &str = "csrc_";

/// Prefix for telescope share codes (cross-server subscribing).
pub const SHARE_CODE_PREFIX: &str = "cssh_";

/// Share codes live a week: long enough to hand to another server's admin,
/// short enough not to be a standing credential.
pub const SHARE_CODE_TTL_SECONDS: i64 = 7 * 24 * 3600;

#[derive(Debug, Clone)]
pub struct GuildRow {
    pub guild_id: i64,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct TelescopeRow {
    pub id: i64,
    pub guild_id: i64,
    pub name: String,
    pub image_cooldown_seconds: i64,
    pub write_policy: String,
    pub allowed_role_ids: Vec<i64>,
}

/// Changes applied to a telescope. `None` keeps the current value.
#[derive(Debug, Default, Clone)]
pub struct TelescopeUpdate {
    pub image_cooldown_seconds: Option<i64>,
    pub write_policy: Option<String>,
    pub allowed_role_ids: Option<Vec<i64>>,
}

/// Generate a fresh pairing token. Returned once, in plaintext; only its
/// hash is stored.
pub fn generate_pairing_token() -> String {
    format!("{PAIRING_TOKEN_PREFIX}{}", super::auth::random_secret())
}

/// Generate a fresh rig credential. Sent to the rig once at pairing; only
/// its hash is stored.
pub fn generate_rig_credential() -> String {
    format!("{RIG_CREDENTIAL_PREFIX}{}", super::auth::random_secret())
}

/// Generate a share code for subscribing another server to a telescope.
pub fn generate_share_code() -> String {
    format!("{SHARE_CODE_PREFIX}{}", super::auth::random_secret())
}

/// One destination of a telescope's feed: a channel in some guild — the
/// owning guild or any guild whose manager subscribed with a share code.
#[derive(Debug, Clone)]
pub struct ChannelRoute {
    pub id: i64,
    pub telescope_id: i64,
    pub guild_id: i64,
    pub channel_id: i64,
    /// Display snapshots taken when the route was created.
    pub channel_name: String,
    pub guild_name: String,
}

/// A rig credential row as needed by the connection handshake.
#[derive(Debug, Clone)]
pub struct RigCredentialRow {
    pub id: i64,
    pub telescope_id: i64,
    pub node_id: String,
    pub profile_id: String,
}

/// Hash a token for storage or lookup.
pub fn hash_token(token: &str) -> String {
    super::auth::sha256_b64url(token)
}

fn roles_to_json(roles: &[i64]) -> String {
    serde_json::to_string(roles).unwrap_or_else(|_| "[]".to_string())
}

fn roles_from_json(json: &str) -> Vec<i64> {
    serde_json::from_str(json).unwrap_or_default()
}

/// A feed shared into a guild from another guild's telescope.
#[derive(Debug, Clone)]
pub struct InboundShare {
    pub route: ChannelRoute,
    pub telescope_name: String,
    pub owning_guild_name: String,
}

fn route_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ChannelRoute> {
    Ok(ChannelRoute {
        id: r.get(0)?,
        telescope_id: r.get(1)?,
        guild_id: r.get(2)?,
        channel_id: r.get(3)?,
        channel_name: r.get(4)?,
        guild_name: r.get(5)?,
    })
}

fn telescope_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<TelescopeRow> {
    Ok(TelescopeRow {
        id: r.get(0)?,
        guild_id: r.get(1)?,
        name: r.get(2)?,
        image_cooldown_seconds: r.get(3)?,
        write_policy: r.get(4)?,
        allowed_role_ids: roles_from_json(&r.get::<_, String>(5)?),
    })
}

// Qualified so the column list also works in joins against
// telescope_channels (which has its own id/guild_id).
const TELESCOPE_COLUMNS: &str = "telescopes.id, telescopes.guild_id, telescopes.name, \
     telescopes.image_cooldown_seconds, telescopes.write_policy, telescopes.allowed_role_ids";

impl Db {
    /// Register a guild (or refresh its name) as a tenant.
    pub fn register_guild(
        &self,
        guild_id: i64,
        name: &str,
        registered_by: i64,
    ) -> Result<(), DbError> {
        let now = unix_now();
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO guilds (guild_id, name, registered_by, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)
                 ON CONFLICT(guild_id) DO UPDATE SET name = ?2, updated_at = ?4",
                rusqlite::params![guild_id, name, registered_by, now],
            )
            .map(|_| ())
        })
    }

    /// Which of these guilds are registered tenants, in one query.
    pub fn registered_guild_ids(
        &self,
        ids: &[i64],
    ) -> Result<std::collections::HashSet<i64>, DbError> {
        if ids.is_empty() {
            return Ok(std::collections::HashSet::new());
        }
        let placeholders = vec!["?"; ids.len()].join(",");
        let sql = format!("SELECT guild_id FROM guilds WHERE guild_id IN ({placeholders})");
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(ids.iter()), |r| r.get(0))?;
            rows.collect()
        })
    }

    pub fn get_guild(&self, guild_id: i64) -> Result<Option<GuildRow>, DbError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT guild_id, name FROM guilds WHERE guild_id = ?1",
                rusqlite::params![guild_id],
                |r| {
                    Ok(GuildRow {
                        guild_id: r.get(0)?,
                        name: r.get(1)?,
                    })
                },
            )
            .optional()
        })
    }

    pub fn create_telescope(
        &self,
        guild_id: i64,
        name: &str,
        created_by: i64,
    ) -> Result<TelescopeRow, DbError> {
        let id = self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO telescopes (guild_id, name, created_by, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![guild_id, name, created_by, unix_now()],
            )?;
            Ok(conn.last_insert_rowid())
        })?;
        Ok(self
            .get_telescope(id)?
            .expect("telescope row just inserted"))
    }

    pub fn get_telescope(&self, id: i64) -> Result<Option<TelescopeRow>, DbError> {
        self.with_conn(|conn| {
            conn.query_row(
                &format!("SELECT {TELESCOPE_COLUMNS} FROM telescopes WHERE id = ?1"),
                rusqlite::params![id],
                telescope_from_row,
            )
            .optional()
        })
    }

    /// Telescope routed to a Discord channel, if any. Channel IDs are
    /// globally unique on Discord, so no guild scope is needed; the unique
    /// constraint on telescope_channels keeps this unambiguous.
    pub fn telescope_by_channel(&self, channel_id: i64) -> Result<Option<TelescopeRow>, DbError> {
        self.with_conn(|conn| {
            conn.query_row(
                &format!(
                    "SELECT {TELESCOPE_COLUMNS} FROM telescopes
                     JOIN telescope_channels tc ON tc.telescope_id = telescopes.id
                     WHERE tc.channel_id = ?1"
                ),
                rusqlite::params![channel_id],
                telescope_from_row,
            )
            .optional()
        })
    }

    /// Telescope by name within one guild. Names are only unique per guild.
    pub fn telescope_by_guild_and_name(
        &self,
        guild_id: i64,
        name: &str,
    ) -> Result<Option<TelescopeRow>, DbError> {
        self.with_conn(|conn| {
            conn.query_row(
                &format!(
                    "SELECT {TELESCOPE_COLUMNS} FROM telescopes
                     WHERE guild_id = ?1 AND name = ?2"
                ),
                rusqlite::params![guild_id, name],
                telescope_from_row,
            )
            .optional()
        })
    }

    pub fn guild_telescopes(&self, guild_id: i64) -> Result<Vec<TelescopeRow>, DbError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {TELESCOPE_COLUMNS} FROM telescopes
                 WHERE guild_id = ?1 ORDER BY name"
            ))?;
            let rows = stmt.query_map(rusqlite::params![guild_id], telescope_from_row)?;
            rows.collect()
        })
    }

    pub fn update_telescope(&self, id: i64, update: &TelescopeUpdate) -> Result<(), DbError> {
        self.with_conn(|conn| {
            if let Some(cooldown) = update.image_cooldown_seconds {
                conn.execute(
                    "UPDATE telescopes SET image_cooldown_seconds = ?1 WHERE id = ?2",
                    rusqlite::params![cooldown, id],
                )?;
            }
            if let Some(policy) = &update.write_policy {
                conn.execute(
                    "UPDATE telescopes SET write_policy = ?1 WHERE id = ?2",
                    rusqlite::params![policy, id],
                )?;
            }
            if let Some(roles) = &update.allowed_role_ids {
                conn.execute(
                    "UPDATE telescopes SET allowed_role_ids = ?1 WHERE id = ?2",
                    rusqlite::params![roles_to_json(roles), id],
                )?;
            }
            Ok(())
        })
    }

    pub fn delete_telescope(&self, id: i64) -> Result<(), DbError> {
        self.with_conn(|conn| {
            conn.execute(
                "DELETE FROM telescopes WHERE id = ?1",
                rusqlite::params![id],
            )
            .map(|_| ())
        })
    }

    /// Issue a pairing token for a telescope. Returns the plaintext token —
    /// the only time it exists outside the caller's hands.
    pub fn issue_pairing_token(
        &self,
        telescope_id: i64,
        created_by: i64,
    ) -> Result<String, DbError> {
        let token = generate_pairing_token();
        let now = unix_now();
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO pairing_tokens
                     (token_hash, telescope_id, created_by, issued_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    hash_token(&token),
                    telescope_id,
                    created_by,
                    now,
                    now + PAIRING_TOKEN_TTL_SECONDS
                ],
            )
            .map(|_| ())
        })?;
        Ok(token)
    }

    /// Consume a pairing token, returning the telescope it pairs. Single-use
    /// and expiring; the exchange endpoint (later phase) calls this.
    pub fn consume_pairing_token(&self, token: &str) -> Result<Option<i64>, DbError> {
        let now = unix_now();
        let hash = hash_token(token);
        self.with_conn(|conn| {
            let row: Option<(i64, i64, Option<i64>)> = conn
                .query_row(
                    "SELECT telescope_id, expires_at, consumed_at
                     FROM pairing_tokens WHERE token_hash = ?1",
                    rusqlite::params![hash],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            let Some((telescope_id, expires_at, consumed_at)) = row else {
                return Ok(None);
            };
            if consumed_at.is_some() || expires_at <= now {
                return Ok(None);
            }
            conn.execute(
                "UPDATE pairing_tokens SET consumed_at = ?1 WHERE token_hash = ?2",
                rusqlite::params![now, hash],
            )?;
            Ok(Some(telescope_id))
        })
    }

    /// Revoke any unconsumed pairing tokens for a telescope.
    pub fn revoke_pairing_tokens(&self, telescope_id: i64) -> Result<usize, DbError> {
        self.with_conn(|conn| {
            conn.execute(
                "DELETE FROM pairing_tokens
                 WHERE telescope_id = ?1 AND consumed_at IS NULL",
                rusqlite::params![telescope_id],
            )
        })
    }

    /// Add a destination for a telescope's feed. The unique constraint on
    /// channel_id rejects a channel already routed to any telescope.
    pub fn add_channel_route(
        &self,
        telescope_id: i64,
        guild_id: i64,
        channel_id: i64,
        channel_name: &str,
        guild_name: &str,
        created_by: i64,
    ) -> Result<ChannelRoute, DbError> {
        let id = self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO telescope_channels
                     (telescope_id, guild_id, channel_id, channel_name, guild_name,
                      created_by, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    telescope_id,
                    guild_id,
                    channel_id,
                    channel_name,
                    guild_name,
                    created_by,
                    unix_now()
                ],
            )?;
            Ok(conn.last_insert_rowid())
        })?;
        Ok(self.get_route(id)?.expect("route row just inserted"))
    }

    pub fn get_route(&self, route_id: i64) -> Result<Option<ChannelRoute>, DbError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, telescope_id, guild_id, channel_id, channel_name, guild_name
                 FROM telescope_channels WHERE id = ?1",
                rusqlite::params![route_id],
                route_from_row,
            )
            .optional()
        })
    }

    /// All destinations of one telescope, owning guild's channels first.
    pub fn telescope_routes(&self, telescope_id: i64) -> Result<Vec<ChannelRoute>, DbError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT tc.id, tc.telescope_id, tc.guild_id, tc.channel_id,
                        tc.channel_name, tc.guild_name
                 FROM telescope_channels tc
                 JOIN telescopes t ON t.id = tc.telescope_id
                 WHERE tc.telescope_id = ?1
                 ORDER BY (tc.guild_id != t.guild_id), tc.channel_name",
            )?;
            let rows = stmt.query_map(rusqlite::params![telescope_id], route_from_row)?;
            rows.collect()
        })
    }

    /// Feeds shared INTO a guild: routes landing in this guild's channels
    /// whose telescope belongs to another guild.
    pub fn routes_into_guild(&self, guild_id: i64) -> Result<Vec<InboundShare>, DbError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT tc.id, tc.telescope_id, tc.guild_id, tc.channel_id,
                        tc.channel_name, tc.guild_name, t.name, og.name
                 FROM telescope_channels tc
                 JOIN telescopes t ON t.id = tc.telescope_id
                 JOIN guilds og ON og.guild_id = t.guild_id
                 WHERE tc.guild_id = ?1 AND t.guild_id != tc.guild_id
                 ORDER BY t.name",
            )?;
            let rows = stmt.query_map(rusqlite::params![guild_id], |r| {
                Ok(InboundShare {
                    route: route_from_row(r)?,
                    telescope_name: r.get(6)?,
                    owning_guild_name: r.get(7)?,
                })
            })?;
            rows.collect()
        })
    }

    pub fn delete_route(&self, route_id: i64) -> Result<(), DbError> {
        self.with_conn(|conn| {
            conn.execute(
                "DELETE FROM telescope_channels WHERE id = ?1",
                rusqlite::params![route_id],
            )
            .map(|_| ())
        })
    }

    /// Mint a share code for a telescope. One outstanding code at a time;
    /// only the hash is stored.
    pub fn create_share_code(&self, telescope_id: i64, created_by: i64) -> Result<String, DbError> {
        let code = generate_share_code();
        let now = unix_now();
        self.with_conn(|conn| {
            conn.execute(
                "DELETE FROM telescope_shares
                 WHERE telescope_id = ?1 AND consumed_at IS NULL",
                rusqlite::params![telescope_id],
            )?;
            conn.execute(
                "INSERT INTO telescope_shares
                     (code_hash, telescope_id, created_by, issued_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    hash_token(&code),
                    telescope_id,
                    created_by,
                    now,
                    now + SHARE_CODE_TTL_SECONDS
                ],
            )
            .map(|_| ())
        })?;
        Ok(code)
    }

    /// Redeem a share code: single use, expiring. Returns the telescope it
    /// shares.
    pub fn consume_share_code(&self, code: &str) -> Result<Option<i64>, DbError> {
        let now = unix_now();
        let hash = hash_token(code);
        self.with_conn(|conn| {
            let row: Option<(i64, i64, Option<i64>)> = conn
                .query_row(
                    "SELECT telescope_id, expires_at, consumed_at
                     FROM telescope_shares WHERE code_hash = ?1",
                    rusqlite::params![hash],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            let Some((telescope_id, expires_at, consumed_at)) = row else {
                return Ok(None);
            };
            if consumed_at.is_some() || expires_at <= now {
                return Ok(None);
            }
            conn.execute(
                "UPDATE telescope_shares SET consumed_at = ?1 WHERE code_hash = ?2",
                rusqlite::params![now, hash],
            )?;
            Ok(Some(telescope_id))
        })
    }

    /// Mint and store a rig credential bound to a node and profile. Returns
    /// the plaintext credential — the only time it exists on the hub.
    pub fn create_rig_credential(
        &self,
        telescope_id: i64,
        node_id: &str,
        profile_id: &str,
    ) -> Result<String, DbError> {
        let credential = generate_rig_credential();
        let now = unix_now();
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO rig_credentials
                     (telescope_id, credential_hash, node_id, profile_id,
                      paired_at, last_seen_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                rusqlite::params![
                    telescope_id,
                    hash_token(&credential),
                    node_id,
                    profile_id,
                    now
                ],
            )
            .map(|_| ())
        })?;
        Ok(credential)
    }

    /// Look up an unrevoked rig credential by plaintext and stamp its last
    /// use.
    pub fn lookup_rig_credential(
        &self,
        credential: &str,
    ) -> Result<Option<RigCredentialRow>, DbError> {
        let hash = hash_token(credential);
        let now = unix_now();
        self.with_conn(|conn| {
            let row: Option<RigCredentialRow> = conn
                .query_row(
                    "SELECT id, telescope_id, node_id, profile_id
                     FROM rig_credentials
                     WHERE credential_hash = ?1 AND revoked_at IS NULL",
                    rusqlite::params![hash],
                    |r| {
                        Ok(RigCredentialRow {
                            id: r.get(0)?,
                            telescope_id: r.get(1)?,
                            node_id: r.get(2)?,
                            profile_id: r.get(3)?,
                        })
                    },
                )
                .optional()?;
            if let Some(found) = &row {
                conn.execute(
                    "UPDATE rig_credentials SET last_seen_at = ?1 WHERE id = ?2",
                    rusqlite::params![now, found.id],
                )?;
            }
            Ok(row)
        })
    }

    /// Revoke every credential for a telescope. Live connections are handled
    /// by the caller.
    pub fn revoke_rig_credentials(&self, telescope_id: i64) -> Result<usize, DbError> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE rig_credentials SET revoked_at = ?1
                 WHERE telescope_id = ?2 AND revoked_at IS NULL",
                rusqlite::params![unix_now(), telescope_id],
            )
        })
    }

    /// Delete a credential by plaintext. Used to roll back a pairing whose
    /// reply never reached the client.
    pub fn delete_rig_credential(&self, credential: &str) -> Result<(), DbError> {
        self.with_conn(|conn| {
            conn.execute(
                "DELETE FROM rig_credentials WHERE credential_hash = ?1",
                rusqlite::params![hash_token(credential)],
            )
            .map(|_| ())
        })
    }

    /// Un-consume a pairing token so the client's retry with the same token
    /// works. Only meaningful right after a failed pairing-reply delivery.
    pub fn restore_pairing_token(&self, token: &str) -> Result<(), DbError> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE pairing_tokens SET consumed_at = NULL WHERE token_hash = ?1",
                rusqlite::params![hash_token(token)],
            )
            .map(|_| ())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub::store::UserRow;

    fn db_with_user() -> Db {
        let db = Db::open_in_memory().unwrap();
        db.upsert_user(&UserRow {
            discord_user_id: 1,
            username: "admin".to_string(),
            email: None,
            email_verified: false,
            avatar_url: None,
        })
        .unwrap();
        db
    }

    #[test]
    fn guild_register_upserts() {
        let db = db_with_user();
        db.register_guild(100, "Observatory", 1).unwrap();
        db.register_guild(100, "Observatory Renamed", 1).unwrap();
        assert_eq!(
            db.get_guild(100).unwrap().unwrap().name,
            "Observatory Renamed"
        );
        assert!(db.get_guild(999).unwrap().is_none());
    }

    #[test]
    fn telescope_crud() {
        let db = db_with_user();
        db.register_guild(100, "g", 1).unwrap();
        let t = db.create_telescope(100, "c925", 1).unwrap();
        // Managers control new scopes by default.
        assert_eq!(t.write_policy, "admins");
        assert_eq!(t.image_cooldown_seconds, 60);

        db.update_telescope(
            t.id,
            &TelescopeUpdate {
                image_cooldown_seconds: Some(120),
                write_policy: Some("roles".to_string()),
                allowed_role_ids: Some(vec![7, 8]),
            },
        )
        .unwrap();
        let updated = db.get_telescope(t.id).unwrap().unwrap();
        assert_eq!(updated.image_cooldown_seconds, 120);
        assert_eq!(updated.write_policy, "roles");
        assert_eq!(updated.allowed_role_ids, vec![7, 8]);

        db.delete_telescope(t.id).unwrap();
        assert!(db.get_telescope(t.id).unwrap().is_none());
    }

    #[test]
    fn duplicate_telescope_name_in_guild_rejected() {
        let db = db_with_user();
        db.register_guild(100, "g", 1).unwrap();
        db.create_telescope(100, "c925", 1).unwrap();
        assert!(db.create_telescope(100, "c925", 1).is_err());
        // Same name in another guild is fine.
        db.register_guild(200, "g2", 1).unwrap();
        assert!(db.create_telescope(200, "c925", 1).is_ok());
    }

    #[test]
    fn invalid_write_policy_rejected_by_check() {
        let db = db_with_user();
        db.register_guild(100, "g", 1).unwrap();
        let t = db.create_telescope(100, "c925", 1).unwrap();
        let result = db.update_telescope(
            t.id,
            &TelescopeUpdate {
                write_policy: Some("everyone".to_string()),
                ..Default::default()
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn routes_and_shares_lifecycle() {
        let db = db_with_user();
        db.register_guild(100, "g", 1).unwrap();
        db.register_guild(200, "partner", 1).unwrap();
        let t = db.create_telescope(100, "c925", 1).unwrap();

        // Two destinations in the owning guild, one shared out.
        let own = db.add_channel_route(t.id, 100, 42, "obs", "g", 1).unwrap();
        db.add_channel_route(t.id, 100, 43, "alerts", "g", 1)
            .unwrap();
        let code = db.create_share_code(t.id, 1).unwrap();
        assert!(code.starts_with(SHARE_CODE_PREFIX));
        assert_eq!(db.consume_share_code(&code).unwrap(), Some(t.id));
        assert_eq!(db.consume_share_code(&code).unwrap(), None);
        let external = db
            .add_channel_route(t.id, 200, 900, "feed", "partner", 1)
            .unwrap();

        // Channel routing resolves through routes and stays unique.
        assert_eq!(db.telescope_by_channel(42).unwrap().unwrap().id, t.id);
        assert_eq!(db.telescope_by_channel(900).unwrap().unwrap().id, t.id);
        assert!(db.add_channel_route(t.id, 100, 42, "dupe", "g", 1).is_err());

        // Own-guild routes list first; the partner sees the inbound share.
        let routes = db.telescope_routes(t.id).unwrap();
        assert_eq!(routes.len(), 3);
        assert!(routes[..2].iter().all(|r| r.guild_id == 100));
        let inbound = db.routes_into_guild(200).unwrap();
        assert_eq!(inbound.len(), 1);
        assert_eq!(inbound[0].telescope_name, "c925");
        assert_eq!(inbound[0].owning_guild_name, "g");

        // Severing routes works from either row; deleting the telescope
        // cascades the rest.
        db.delete_route(external.id).unwrap();
        assert!(db.telescope_by_channel(900).unwrap().is_none());
        db.delete_route(own.id).unwrap();
        db.delete_telescope(t.id).unwrap();
        let remaining: i64 = db
            .with_conn(|conn| {
                conn.query_row("SELECT COUNT(*) FROM telescope_channels", [], |r| r.get(0))
            })
            .unwrap();
        assert_eq!(remaining, 0);
    }

    #[test]
    fn pairing_token_single_use_and_hashed() {
        let db = db_with_user();
        db.register_guild(100, "g", 1).unwrap();
        let t = db.create_telescope(100, "c925", 1).unwrap();
        let token = db.issue_pairing_token(t.id, 1).unwrap();
        assert!(token.starts_with(PAIRING_TOKEN_PREFIX));

        // Plaintext never stored.
        let stored: String = db
            .with_conn(|conn| {
                conn.query_row("SELECT token_hash FROM pairing_tokens", [], |r| r.get(0))
            })
            .unwrap();
        assert_ne!(stored, token);
        assert_eq!(stored, hash_token(&token));

        assert_eq!(db.consume_pairing_token(&token).unwrap(), Some(t.id));
        assert_eq!(db.consume_pairing_token(&token).unwrap(), None);
        assert_eq!(db.consume_pairing_token("cspt_bogus").unwrap(), None);
    }

    #[test]
    fn expired_pairing_token_rejected() {
        let db = db_with_user();
        db.register_guild(100, "g", 1).unwrap();
        let t = db.create_telescope(100, "c925", 1).unwrap();
        let token = db.issue_pairing_token(t.id, 1).unwrap();
        db.with_conn(|conn| {
            conn.execute("UPDATE pairing_tokens SET expires_at = 1", [])
                .map(|_| ())
        })
        .unwrap();
        assert_eq!(db.consume_pairing_token(&token).unwrap(), None);
    }

    #[test]
    fn revoke_unconsumed_tokens() {
        let db = db_with_user();
        db.register_guild(100, "g", 1).unwrap();
        let t = db.create_telescope(100, "c925", 1).unwrap();
        let consumed = db.issue_pairing_token(t.id, 1).unwrap();
        db.consume_pairing_token(&consumed).unwrap();
        let live = db.issue_pairing_token(t.id, 1).unwrap();
        assert_eq!(db.revoke_pairing_tokens(t.id).unwrap(), 1);
        assert_eq!(db.consume_pairing_token(&live).unwrap(), None);
    }

    #[test]
    fn rig_credential_lifecycle() {
        let db = db_with_user();
        db.register_guild(100, "g", 1).unwrap();
        let t = db.create_telescope(100, "c925", 1).unwrap();
        let credential = db
            .create_rig_credential(t.id, "node-1", "profile-1")
            .unwrap();
        assert!(credential.starts_with(RIG_CREDENTIAL_PREFIX));

        let row = db.lookup_rig_credential(&credential).unwrap().unwrap();
        assert_eq!(row.telescope_id, t.id);
        assert_eq!(row.node_id, "node-1");
        assert_eq!(row.profile_id, "profile-1");
        assert!(db.lookup_rig_credential("csrc_wrong").unwrap().is_none());

        assert_eq!(db.revoke_rig_credentials(t.id).unwrap(), 1);
        assert!(db.lookup_rig_credential(&credential).unwrap().is_none());
    }

    #[test]
    fn deleting_guild_cascades_to_telescopes_and_tokens() {
        let db = db_with_user();
        db.register_guild(100, "g", 1).unwrap();
        let t = db.create_telescope(100, "c925", 1).unwrap();
        db.issue_pairing_token(t.id, 1).unwrap();
        db.with_conn(|conn| {
            conn.execute("DELETE FROM guilds WHERE guild_id = 100", [])
                .map(|_| ())
        })
        .unwrap();
        let telescopes: i64 = db
            .with_conn(|conn| conn.query_row("SELECT COUNT(*) FROM telescopes", [], |r| r.get(0)))
            .unwrap();
        let tokens: i64 = db
            .with_conn(|conn| {
                conn.query_row("SELECT COUNT(*) FROM pairing_tokens", [], |r| r.get(0))
            })
            .unwrap();
        assert_eq!((telescopes, tokens), (0, 0));
    }
}
