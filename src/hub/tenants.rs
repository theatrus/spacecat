//! Tenancy persistence.
//!
//! A telescope belongs to a USER. Every server relationship is an explicit
//! attachment: `can_command` says whether that server may drive the rig
//! (true for attachments the owner made in servers they manage, false for
//! feed-only subscriptions made with a share code), and the attachment
//! carries that server's write policy and role allowlist. Channel routes
//! deliver the feed into channels of attached guilds.

use super::db::{Db, DbError, unix_now};
use rusqlite::OptionalExtension;

/// Pairing tokens expire after an hour; they exist only to move a secret
/// from the web page into the N.I.N.A. plugin settings.
pub const PAIRING_TOKEN_TTL_SECONDS: i64 = 3600;

/// Prefix that makes a leaked pairing token recognizable in scans.
pub const PAIRING_TOKEN_PREFIX: &str = "cspt_";

/// Prefix for durable rig credentials minted by the pairing exchange.
pub const RIG_CREDENTIAL_PREFIX: &str = "csrc_";

/// Prefix for telescope share codes (feed subscriptions).
pub const SHARE_CODE_PREFIX: &str = "cssh_";

/// Share codes live a week: long enough to hand to another server's admin,
/// short enough not to be a standing credential.
pub const SHARE_CODE_TTL_SECONDS: i64 = 7 * 24 * 3600;

#[derive(Debug, Clone)]
pub struct GuildRow {
    pub guild_id: i64,
    pub name: String,
}

/// A user-owned telescope. Guild relationships live in attachments.
#[derive(Debug, Clone)]
pub struct TelescopeRow {
    pub id: i64,
    pub owner_id: i64,
    pub name: String,
    pub image_cooldown_seconds: i64,
}

/// One telescope's relationship with one guild.
#[derive(Debug, Clone)]
pub struct AttachmentRow {
    pub id: i64,
    pub telescope_id: i64,
    pub guild_id: i64,
    /// May this guild's members drive the rig (subject to write_policy)?
    /// False for feed-only subscriptions.
    pub can_command: bool,
    pub write_policy: String,
    pub allowed_role_ids: Vec<i64>,
}

/// Changes applied to an attachment. `None` keeps the current value.
#[derive(Debug, Default, Clone)]
pub struct AttachmentUpdate {
    pub write_policy: Option<String>,
    pub allowed_role_ids: Option<Vec<i64>>,
}

/// An attachment as listed on a guild's management page.
#[derive(Debug, Clone)]
pub struct GuildAttachment {
    pub attachment: AttachmentRow,
    pub telescope: TelescopeRow,
    pub owner_name: String,
}

/// One destination of a telescope's feed: a channel in an attached guild.
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

pub fn generate_pairing_token() -> String {
    format!("{PAIRING_TOKEN_PREFIX}{}", super::auth::random_secret())
}

pub fn generate_rig_credential() -> String {
    format!("{RIG_CREDENTIAL_PREFIX}{}", super::auth::random_secret())
}

pub fn generate_share_code() -> String {
    format!("{SHARE_CODE_PREFIX}{}", super::auth::random_secret())
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

fn telescope_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<TelescopeRow> {
    Ok(TelescopeRow {
        id: r.get(0)?,
        owner_id: r.get(1)?,
        name: r.get(2)?,
        image_cooldown_seconds: r.get(3)?,
    })
}

fn attachment_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<AttachmentRow> {
    Ok(AttachmentRow {
        id: r.get(0)?,
        telescope_id: r.get(1)?,
        guild_id: r.get(2)?,
        can_command: r.get(3)?,
        write_policy: r.get(4)?,
        allowed_role_ids: roles_from_json(&r.get::<_, String>(5)?),
    })
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

// Qualified so the column lists also work in joins.
const TELESCOPE_COLUMNS: &str = "telescopes.id, telescopes.owner_id, telescopes.name, \
     telescopes.image_cooldown_seconds";
const ATTACHMENT_COLUMNS: &str = "telescope_attachments.id, \
     telescope_attachments.telescope_id, telescope_attachments.guild_id, \
     telescope_attachments.can_command, telescope_attachments.write_policy, \
     telescope_attachments.allowed_role_ids";
const ROUTE_COLUMNS: &str = "telescope_channels.id, telescope_channels.telescope_id, \
     telescope_channels.guild_id, telescope_channels.channel_id, \
     telescope_channels.channel_name, telescope_channels.guild_name";

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

    // ---------- Telescopes (user-owned) ----------

    pub fn create_telescope(&self, owner_id: i64, name: &str) -> Result<TelescopeRow, DbError> {
        let id = self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO telescopes (owner_id, name, created_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![owner_id, name, unix_now()],
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

    pub fn user_telescopes(&self, owner_id: i64) -> Result<Vec<TelescopeRow>, DbError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {TELESCOPE_COLUMNS} FROM telescopes
                 WHERE owner_id = ?1 ORDER BY name"
            ))?;
            let rows = stmt.query_map(rusqlite::params![owner_id], telescope_from_row)?;
            rows.collect()
        })
    }

    pub fn set_telescope_cooldown(&self, id: i64, seconds: i64) -> Result<(), DbError> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE telescopes SET image_cooldown_seconds = ?1 WHERE id = ?2",
                rusqlite::params![seconds, id],
            )
            .map(|_| ())
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

    // ---------- Attachments (telescope x guild) ----------

    /// Attach a telescope to a guild. `can_command: false` is a feed-only
    /// subscription.
    pub fn attach_telescope(
        &self,
        telescope_id: i64,
        guild_id: i64,
        can_command: bool,
        created_by: i64,
    ) -> Result<AttachmentRow, DbError> {
        let id = self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO telescope_attachments
                     (telescope_id, guild_id, can_command, created_by, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![telescope_id, guild_id, can_command, created_by, unix_now()],
            )?;
            Ok(conn.last_insert_rowid())
        })?;
        Ok(self.get_attachment(id)?.expect("attachment just inserted"))
    }

    pub fn get_attachment(&self, id: i64) -> Result<Option<AttachmentRow>, DbError> {
        self.with_conn(|conn| {
            conn.query_row(
                &format!("SELECT {ATTACHMENT_COLUMNS} FROM telescope_attachments WHERE id = ?1"),
                rusqlite::params![id],
                attachment_from_row,
            )
            .optional()
        })
    }

    /// The attachment binding a telescope to one guild, if any.
    pub fn attachment_for(
        &self,
        telescope_id: i64,
        guild_id: i64,
    ) -> Result<Option<AttachmentRow>, DbError> {
        self.with_conn(|conn| {
            conn.query_row(
                &format!(
                    "SELECT {ATTACHMENT_COLUMNS} FROM telescope_attachments
                     WHERE telescope_id = ?1 AND guild_id = ?2"
                ),
                rusqlite::params![telescope_id, guild_id],
                attachment_from_row,
            )
            .optional()
        })
    }

    /// Everything attached to a guild, with the telescope and its owner's
    /// display name. Command-capable attachments first.
    pub fn guild_attachments(&self, guild_id: i64) -> Result<Vec<GuildAttachment>, DbError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {ATTACHMENT_COLUMNS}, {TELESCOPE_COLUMNS}, users.username
                 FROM telescope_attachments
                 JOIN telescopes ON telescopes.id = telescope_attachments.telescope_id
                 JOIN users ON users.discord_user_id = telescopes.owner_id
                 WHERE telescope_attachments.guild_id = ?1
                 ORDER BY telescope_attachments.can_command DESC, telescopes.name"
            ))?;
            let rows = stmt.query_map(rusqlite::params![guild_id], |r| {
                Ok(GuildAttachment {
                    attachment: attachment_from_row(r)?,
                    telescope: TelescopeRow {
                        id: r.get(6)?,
                        owner_id: r.get(7)?,
                        name: r.get(8)?,
                        image_cooldown_seconds: r.get(9)?,
                    },
                    owner_name: r.get(10)?,
                })
            })?;
            rows.collect()
        })
    }

    /// Guilds a telescope is attached to (for the owner's view).
    pub fn telescope_attachments(&self, telescope_id: i64) -> Result<Vec<AttachmentRow>, DbError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {ATTACHMENT_COLUMNS} FROM telescope_attachments
                 WHERE telescope_id = ?1 ORDER BY can_command DESC, guild_id"
            ))?;
            let rows = stmt.query_map(rusqlite::params![telescope_id], attachment_from_row)?;
            rows.collect()
        })
    }

    pub fn update_attachment(&self, id: i64, update: &AttachmentUpdate) -> Result<(), DbError> {
        self.with_conn(|conn| {
            if let Some(policy) = &update.write_policy {
                conn.execute(
                    "UPDATE telescope_attachments SET write_policy = ?1 WHERE id = ?2",
                    rusqlite::params![policy, id],
                )?;
            }
            if let Some(roles) = &update.allowed_role_ids {
                conn.execute(
                    "UPDATE telescope_attachments SET allowed_role_ids = ?1 WHERE id = ?2",
                    rusqlite::params![roles_to_json(roles), id],
                )?;
            }
            Ok(())
        })
    }

    /// Remove an attachment and every route delivering into its guild.
    pub fn detach_telescope(&self, attachment_id: i64) -> Result<(), DbError> {
        let Some(attachment) = self.get_attachment(attachment_id)? else {
            return Ok(());
        };
        self.with_conn(|conn| {
            conn.execute(
                "DELETE FROM telescope_channels
                 WHERE telescope_id = ?1 AND guild_id = ?2",
                rusqlite::params![attachment.telescope_id, attachment.guild_id],
            )?;
            conn.execute(
                "DELETE FROM telescope_attachments WHERE id = ?1",
                rusqlite::params![attachment_id],
            )
            .map(|_| ())
        })
    }

    // ---------- Channel routes ----------

    /// Telescope routed to a Discord channel, if any. Channel IDs are
    /// globally unique on Discord; the unique constraint keeps this
    /// unambiguous.
    pub fn telescope_by_channel(&self, channel_id: i64) -> Result<Option<TelescopeRow>, DbError> {
        self.with_conn(|conn| {
            conn.query_row(
                &format!(
                    "SELECT {TELESCOPE_COLUMNS} FROM telescopes
                     JOIN telescope_channels ON telescope_channels.telescope_id = telescopes.id
                     WHERE telescope_channels.channel_id = ?1"
                ),
                rusqlite::params![channel_id],
                telescope_from_row,
            )
            .optional()
        })
    }

    /// Telescope by name among the guild's attachments. Names are unique per
    /// owner, so two owners can bring same-named scopes into one guild; the
    /// first by owner id wins and the UI shows both names.
    pub fn telescope_by_guild_and_name(
        &self,
        guild_id: i64,
        name: &str,
    ) -> Result<Option<TelescopeRow>, DbError> {
        self.with_conn(|conn| {
            conn.query_row(
                &format!(
                    "SELECT {TELESCOPE_COLUMNS} FROM telescopes
                     JOIN telescope_attachments
                        ON telescope_attachments.telescope_id = telescopes.id
                     WHERE telescope_attachments.guild_id = ?1 AND telescopes.name = ?2
                     ORDER BY telescopes.owner_id LIMIT 1"
                ),
                rusqlite::params![guild_id, name],
                telescope_from_row,
            )
            .optional()
        })
    }

    /// Names of every telescope attached to a guild.
    pub fn guild_telescope_names(&self, guild_id: i64) -> Result<Vec<String>, DbError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT telescopes.name FROM telescopes
                 JOIN telescope_attachments
                    ON telescope_attachments.telescope_id = telescopes.id
                 WHERE telescope_attachments.guild_id = ?1 ORDER BY telescopes.name",
            )?;
            let rows = stmt.query_map(rusqlite::params![guild_id], |r| r.get(0))?;
            rows.collect()
        })
    }

    /// Add a destination. The unique constraint on channel_id rejects a
    /// channel already routed to any telescope.
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
                &format!("SELECT {ROUTE_COLUMNS} FROM telescope_channels WHERE id = ?1"),
                rusqlite::params![route_id],
                route_from_row,
            )
            .optional()
        })
    }

    /// All destinations of one telescope, across every attached guild.
    pub fn telescope_routes(&self, telescope_id: i64) -> Result<Vec<ChannelRoute>, DbError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {ROUTE_COLUMNS} FROM telescope_channels
                 WHERE telescope_id = ?1 ORDER BY guild_id, channel_name"
            ))?;
            let rows = stmt.query_map(rusqlite::params![telescope_id], route_from_row)?;
            rows.collect()
        })
    }

    /// Destinations one attachment delivers into (telescope x guild).
    pub fn attachment_routes(
        &self,
        telescope_id: i64,
        guild_id: i64,
    ) -> Result<Vec<ChannelRoute>, DbError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {ROUTE_COLUMNS} FROM telescope_channels
                 WHERE telescope_id = ?1 AND guild_id = ?2 ORDER BY channel_name"
            ))?;
            let rows = stmt.query_map(rusqlite::params![telescope_id, guild_id], route_from_row)?;
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

    // ---------- Share codes ----------

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

    // ---------- Rig credentials ----------

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

    // ---------- Pairing tokens ----------

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
    /// and expiring; the exchange endpoint calls this.
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub::store::UserRow;

    fn db_with_users() -> Db {
        let db = Db::open_in_memory().unwrap();
        for (id, name) in [(1, "alice"), (2, "bob")] {
            db.upsert_user(&UserRow {
                discord_user_id: id,
                username: name.to_string(),
                email: None,
                email_verified: false,
                avatar_url: None,
            })
            .unwrap();
        }
        db
    }

    #[test]
    fn telescope_ownership_and_naming() {
        let db = db_with_users();
        let t = db.create_telescope(1, "c925").unwrap();
        assert_eq!(t.owner_id, 1);
        assert_eq!(t.image_cooldown_seconds, 60);
        // Names are unique per owner, not globally.
        assert!(db.create_telescope(1, "c925").is_err());
        assert!(db.create_telescope(2, "c925").is_ok());

        db.set_telescope_cooldown(t.id, 120).unwrap();
        assert_eq!(
            db.get_telescope(t.id)
                .unwrap()
                .unwrap()
                .image_cooldown_seconds,
            120
        );
        assert_eq!(db.user_telescopes(1).unwrap().len(), 1);
        db.delete_telescope(t.id).unwrap();
        assert!(db.get_telescope(t.id).unwrap().is_none());
    }

    #[test]
    fn attachment_lifecycle() {
        let db = db_with_users();
        db.register_guild(100, "home", 1).unwrap();
        db.register_guild(200, "club", 2).unwrap();
        let t = db.create_telescope(1, "c925").unwrap();

        // Owner attaches at home with command capability; the club gets a
        // feed-only subscription.
        let home = db.attach_telescope(t.id, 100, true, 1).unwrap();
        let club = db.attach_telescope(t.id, 200, false, 2).unwrap();
        assert!(home.can_command);
        assert!(!club.can_command);
        assert_eq!(home.write_policy, "admins");
        // One attachment per (telescope, guild).
        assert!(db.attach_telescope(t.id, 100, true, 1).is_err());

        // The guild page shows attachments with owner names, command-capable
        // first.
        let listed = db.guild_attachments(200).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].owner_name, "alice");
        assert!(!listed[0].attachment.can_command);

        // Attachment carries the per-guild policy.
        db.update_attachment(
            home.id,
            &AttachmentUpdate {
                write_policy: Some("roles".to_string()),
                allowed_role_ids: Some(vec![7]),
            },
        )
        .unwrap();
        let updated = db.get_attachment(home.id).unwrap().unwrap();
        assert_eq!(updated.write_policy, "roles");
        assert_eq!(updated.allowed_role_ids, vec![7]);

        // Detaching removes the attachment and its guild's routes only.
        db.add_channel_route(t.id, 100, 42, "obs", "home", 1)
            .unwrap();
        db.add_channel_route(t.id, 200, 900, "feed", "club", 2)
            .unwrap();
        db.detach_telescope(club.id).unwrap();
        assert!(db.attachment_for(t.id, 200).unwrap().is_none());
        assert!(db.telescope_by_channel(900).unwrap().is_none());
        assert!(db.telescope_by_channel(42).unwrap().is_some());
    }

    #[test]
    fn channel_routing_and_name_resolution() {
        let db = db_with_users();
        db.register_guild(100, "home", 1).unwrap();
        let t = db.create_telescope(1, "c925").unwrap();
        db.attach_telescope(t.id, 100, true, 1).unwrap();
        db.add_channel_route(t.id, 100, 42, "obs", "home", 1)
            .unwrap();

        assert_eq!(db.telescope_by_channel(42).unwrap().unwrap().id, t.id);
        assert_eq!(
            db.telescope_by_guild_and_name(100, "c925")
                .unwrap()
                .unwrap()
                .id,
            t.id
        );
        assert!(
            db.telescope_by_guild_and_name(999, "c925")
                .unwrap()
                .is_none()
        );
        assert_eq!(db.guild_telescope_names(100).unwrap(), vec!["c925"]);
        // A channel routes once, hub-wide.
        assert!(
            db.add_channel_route(t.id, 100, 42, "dupe", "home", 1)
                .is_err()
        );
    }

    #[test]
    fn share_code_single_use() {
        let db = db_with_users();
        db.register_guild(100, "home", 1).unwrap();
        let t = db.create_telescope(1, "c925").unwrap();
        let code = db.create_share_code(t.id, 1).unwrap();
        assert!(code.starts_with(SHARE_CODE_PREFIX));
        assert_eq!(db.consume_share_code(&code).unwrap(), Some(t.id));
        assert_eq!(db.consume_share_code(&code).unwrap(), None);
        assert_eq!(db.consume_share_code("cssh_bogus").unwrap(), None);
    }

    #[test]
    fn pairing_token_single_use_and_hashed() {
        let db = db_with_users();
        let t = db.create_telescope(1, "c925").unwrap();
        let token = db.issue_pairing_token(t.id, 1).unwrap();
        assert!(token.starts_with(PAIRING_TOKEN_PREFIX));

        let stored: String = db
            .with_conn(|conn| {
                conn.query_row("SELECT token_hash FROM pairing_tokens", [], |r| r.get(0))
            })
            .unwrap();
        assert_ne!(stored, token);
        assert_eq!(stored, hash_token(&token));

        assert_eq!(db.consume_pairing_token(&token).unwrap(), Some(t.id));
        assert_eq!(db.consume_pairing_token(&token).unwrap(), None);
    }

    #[test]
    fn rig_credential_lifecycle() {
        let db = db_with_users();
        let t = db.create_telescope(1, "c925").unwrap();
        let credential = db
            .create_rig_credential(t.id, "node-1", "profile-1")
            .unwrap();
        assert!(credential.starts_with(RIG_CREDENTIAL_PREFIX));
        let row = db.lookup_rig_credential(&credential).unwrap().unwrap();
        assert_eq!(row.telescope_id, t.id);
        assert_eq!(db.revoke_rig_credentials(t.id).unwrap(), 1);
        assert!(db.lookup_rig_credential(&credential).unwrap().is_none());
    }

    #[test]
    fn deleting_telescope_cascades_everything() {
        let db = db_with_users();
        db.register_guild(100, "home", 1).unwrap();
        let t = db.create_telescope(1, "c925").unwrap();
        db.attach_telescope(t.id, 100, true, 1).unwrap();
        db.add_channel_route(t.id, 100, 42, "obs", "home", 1)
            .unwrap();
        db.issue_pairing_token(t.id, 1).unwrap();
        db.create_rig_credential(t.id, "n", "p").unwrap();
        db.create_share_code(t.id, 1).unwrap();

        db.delete_telescope(t.id).unwrap();
        for table in [
            "telescope_attachments",
            "telescope_channels",
            "pairing_tokens",
            "rig_credentials",
            "telescope_shares",
        ] {
            let count: i64 = db
                .with_conn(|conn| {
                    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                })
                .unwrap();
            assert_eq!(count, 0, "{table} not cascaded");
        }
    }
}
