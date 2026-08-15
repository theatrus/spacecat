# Hosted Chatstronomy service (hub)

The hub is the third run mode, next to Advanced API polling and N.I.N.A.
Direct. One hosted process serves a web app, runs the central Discord
application, and accepts rig connections. Users install the Chatstronomy
Discord app into their own server, pair their telescopes through the web app,
and get chat notifications and slash commands without running their own bot.

This document extends `NINA_PLUGIN_ARCHITECTURE.md`, which defines the Direct
protocol, rig identity, and pairing rules the hub relies on.

## Scope decisions

- **Discord only at first.** Matrix support in the hosted service is deferred.
  Self-hosted Advanced API mode keeps Matrix.
- **Relay connections are accepted.** A rig can reach the hub two ways: the
  N.I.N.A. plugin's Direct WebSocket, or an existing Chatstronomy agent that
  polls the Advanced API locally and relays outbound. Both use the same
  authenticated channel and message envelopes.
- **One SQLite database.** All durable hub state — users, sessions, guilds,
  telescopes, credentials, updater state — lives in one SQLite file accessed
  through `rusqlite`. No second database system.

## Process shape

`chatstronomy hub --hub-config hub.json` starts one process containing:

- an axum web server (login, guild and telescope management, health);
- the central Discord bot (serenity/poise, the existing command set);
- the `/v1/direct` WebSocket listener for plugins and relay agents;
- one `ChatUpdater` per connected rig, feeding guild channels.

TLS terminates at a reverse proxy in front of the web server. The hub has its
own configuration file because it has no telescope list in config — telescopes
live in the database.

## Tenancy

A Discord guild is a tenant.

1. A guild admin installs the Chatstronomy Discord app.
2. They log into the hub web app with Discord OAuth using the scopes
   `identify email guilds`. The hub stores their Discord ID, email, and a
   snapshot of their guilds with permission bits.
3. Management of a guild requires `MANAGE_GUILD` there, checked from the
   OAuth snapshot and re-verified live through the bot before changes.
4. Under a guild, an admin creates telescope records: name, notification
   channel, image cooldown, and a write-command policy (off by default, or
   limited to chosen roles).
5. Creating a telescope issues a one-time pairing token. The user pastes the
   hub URL and token into the N.I.N.A. plugin (or relay agent) settings. On
   first connect the hub exchanges it for a credential bound to that rig's
   `(node_id, profile_id)` and stores only a hash. Tokens are revocable from
   the web app.

## Authentication details

Patterns follow hotbot's web layer, with its gaps closed:

- Server-side sessions in SQLite. The cookie holds a session ID plus an
  HMAC-SHA256 signature; compare with a constant-time primitive. No JWT.
- OAuth state rows are single-use and expire in 15 minutes; `next` redirect
  paths are sanitized. PKCE is used.
- Per-session CSRF token required on mutating requests.
- Guild authorization = OAuth-time snapshot AND a live membership/permission
  check via the bot, with short positive-only caching.
- Discord access tokens are used during login and discarded. Rig credentials
  and pairing tokens are stored hashed.

## Data plane

Rig connections land in the `DirectRigRegistry`. A `DirectRigSource`
implements `RigSource` over the connection: reads come from a
freshness-stamped snapshot cache or a bounded round trip; commands carry
correlation IDs and expiry and never queue for an offline rig. The existing
poise commands and `ChatUpdater` consume the source unchanged. Relay agents
push the same envelopes from Advanced API polling instead of native N.I.N.A.
mediators.

### N.I.N.A. plugin client

Hosted delivery is implemented directly in the N.I.N.A. plugin. The user pastes
the hub's HTTPS origin (or WSS Direct endpoint) and a one-time telescope pairing
code. The plugin connects only over TLS in production, exchanges the code for a
node-bound credential, stores both secrets in Windows Credential Manager scoped
to the profile and hub origin, and clears the code after a successful exchange.

After authentication, every hub query is answered by the same native provider
used by local Direct mode, including thumbnails, autofocus details, guider graph
data, equipment snapshots, sequence state, and the typed command allowlist.
Commands that arrive after their expiry plus the protocol's clock-skew grace are
rejected before reaching N.I.N.A. The connection sends heartbeats, reconnects
with bounded exponential backoff after transient failures, and stops for fatal
authentication errors so the N.I.N.A. options page can direct the user to repair
or forget the credential.

## Schema sketch

```
hub_settings(key, value, updated_at)                    -- V1 (shipped)
users(discord_user_id PK, email, email_verified, username, avatar_url, last_auth_at)
sessions / oauth_states                                  -- hotbot shapes
guilds(guild_id PK, name, installed_at, write_policy, allowed_role_ids)
user_guilds(discord_user_id, guild_id, permissions, last_seen_at)
telescopes(id PK, guild_id FK, name, discord_channel_id, image_cooldown, created_by)
rig_credentials(id, telescope_id FK, token_hash, node_id, profile_id, paired_at, revoked_at)
pairing_tokens(token_hash, telescope_id, expires_at, consumed_at)
updater_state(telescope_id, event_cursor, status_message_id, ...)
```

Migrations are an append-only list in `src/hub/db.rs` applied through
`PRAGMA user_version`, each atomic with its version bump. Discord snowflakes
are stored as INTEGER and serialized to JSON as strings.

## Delivery order

1. Hub skeleton: `hub` feature and subcommand, config, SQLite + migrations,
   axum server with health endpoint. *(this PR)*
2. Discord OAuth login with email capture, sessions, CSRF.
3. Guild and telescope management, pairing tokens.
4. `/v1/direct` WebSocket transport, pairing exchange, `DirectRigSource`,
   relay ingestion.
5. Multi-tenant bot routing and per-rig `ChatUpdater` with state in SQLite.
6. Hardening: rate limits, revocation, command expiry, audit log.
