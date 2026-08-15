//! Tenancy persistence: registered guilds, their telescopes, and one-time
//! pairing tokens.

use super::db::{Db, DbError, unix_now};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Pairing tokens expire after an hour; they exist only to move a secret
/// from the web page into the N.I.N.A. plugin settings.
pub const PAIRING_TOKEN_TTL_SECONDS: i64 = 3600;

/// Prefix that makes a leaked pairing token recognizable in scans.
pub const PAIRING_TOKEN_PREFIX: &str = "cspt_";

/// Prefix for durable rig credentials minted by the pairing exchange.
pub const RIG_CREDENTIAL_PREFIX: &str = "csrc_";

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
    pub discord_channel_id: Option<i64>,
    pub image_cooldown_seconds: i64,
    pub write_policy: String,
    pub allowed_role_ids: Vec<i64>,
}

/// Changes applied to a telescope. `None` keeps the current value.
#[derive(Debug, Default, Clone)]
pub struct TelescopeUpdate {
    pub discord_channel_id: Option<Option<i64>>,
    pub image_cooldown_seconds: Option<i64>,
    pub write_policy: Option<String>,
    pub allowed_role_ids: Option<Vec<i64>>,
}

/// Generate a fresh pairing token. Returned once, in plaintext; only its
/// hash is stored.
pub fn generate_pairing_token() -> String {
    format!(
        "{PAIRING_TOKEN_PREFIX}{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

/// Generate a fresh rig credential. Sent to the rig once at pairing; only
/// its hash is stored.
pub fn generate_rig_credential() -> String {
    format!(
        "{RIG_CREDENTIAL_PREFIX}{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
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
    URL_SAFE_NO_PAD.encode(Sha256::digest(token.as_bytes()))
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
        guild_id: r.get(1)?,
        name: r.get(2)?,
        discord_channel_id: r.get(3)?,
        image_cooldown_seconds: r.get(4)?,
        write_policy: r.get(5)?,
        allowed_role_ids: roles_from_json(&r.get::<_, String>(6)?),
    })
}

const TELESCOPE_COLUMNS: &str = "id, guild_id, name, discord_channel_id, \
     image_cooldown_seconds, write_policy, allowed_role_ids";

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
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
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
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
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
            if let Some(channel) = &update.discord_channel_id {
                conn.execute(
                    "UPDATE telescopes SET discord_channel_id = ?1 WHERE id = ?2",
                    rusqlite::params![channel, id],
                )?;
            }
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
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })?;
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
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })?;
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
        assert_eq!(t.write_policy, "disabled");
        assert_eq!(t.image_cooldown_seconds, 60);
        assert_eq!(t.discord_channel_id, None);

        db.update_telescope(
            t.id,
            &TelescopeUpdate {
                discord_channel_id: Some(Some(555)),
                image_cooldown_seconds: Some(120),
                write_policy: Some("roles".to_string()),
                allowed_role_ids: Some(vec![7, 8]),
            },
        )
        .unwrap();
        let updated = db.get_telescope(t.id).unwrap().unwrap();
        assert_eq!(updated.discord_channel_id, Some(555));
        assert_eq!(updated.image_cooldown_seconds, 120);
        assert_eq!(updated.write_policy, "roles");
        assert_eq!(updated.allowed_role_ids, vec![7, 8]);

        // Clearing the channel with Some(None).
        db.update_telescope(
            t.id,
            &TelescopeUpdate {
                discord_channel_id: Some(None),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            db.get_telescope(t.id).unwrap().unwrap().discord_channel_id,
            None
        );

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
