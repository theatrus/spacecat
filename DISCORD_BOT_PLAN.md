# Discord Bot — Feature Plan

Add a process-wide Serenity/Poise Discord bot alongside the existing webhook
posting model. Each telescope can map to a Discord channel; slash commands
invoked in that channel act on the telescope's NINA API. Webhook posting
stays untouched for setups that don't enable the bot.

## Goals

- **One bot, many telescopes** — single Discord application token shared
  across all configured telescopes, mirroring the shared-Matrix model.
- **Channel-driven defaults** — `/status` invoked in `#c925` automatically
  targets the c925 telescope. Falls back to an explicit `--telescope` arg
  when run from an unmapped channel.
- **Coexist with webhooks** — bot is opt-in per telescope. If a telescope
  configures `discord_channel_id`, the bot takes precedence; otherwise the
  existing webhook path is used unchanged.
- **Phased delivery** — read-only commands first; live status; write
  commands; cosmetic polish.

## Architecture

### Service layer

A new `DiscordBotService` implements the existing `ChatService` trait so it
slots into `ChatServiceManager` next to `DiscordChatService` and
`MatrixChatService`. Routing precedence is enforced via each service's
`can_route(&ChatTarget)`:

```rust
// Webhook service: defers when a channel_id is configured.
impl ChatService for DiscordChatService {
    fn can_route(&self, target: &ChatTarget) -> bool {
        if target.discord_channel_id.is_some() {
            return false;
        }
        self.resolve_url(target).is_some()
    }
}

// Bot service: takes routes that have a channel (override or default).
impl ChatService for DiscordBotService {
    fn can_route(&self, target: &ChatTarget) -> bool {
        target.discord_channel_id.is_some() || self.default_channel_id.is_some()
    }
}
```

This keeps the routing decision local to each service — no cross-service
knowledge.

### Bot internals

A single `serenity::Client` runs in a dedicated tokio task, constructed in
`service_wrapper::build_shared_chat_manager`. The gateway connection handles
slash commands; outbound posts (from the `ChatService::send_message` path)
use a cloned `Arc<Http>` directly, with no mpsc bridge needed.

The Poise `Data` struct carries:

```rust
pub struct BotData {
    pub api_clients: HashMap<String, SpaceCatApiClient>,  // telescope -> client
    pub channel_to_telescope: HashMap<ChannelId, String>,
    pub write_acl: HashSet<UserId>,                       // Phase 3
    pub status_state: Arc<Mutex<StatusState>>,            // Phase 2
}
```

Gateway intents required: `GUILDS` only. We don't need `MESSAGE_CONTENT` —
slash commands deliver parameters directly to the bot without parsing
message bodies.

## Config shape

```jsonc
{
  "chat": {
    "discord_bot": {
      "enabled": true,
      "token": "BOT_TOKEN_HERE",
      "default_channel_id": "111111111111111111",  // optional fallback
      "state_file": "./spacecat-state.json",       // CWD by default
      "write_acl": ["123456789012345678"]          // Phase 3
    },
    "discord": { "enabled": true, "default_webhook_url": "..." },  // unchanged
    "matrix":  { ... }  // unchanged
  },
  "telescopes": [
    {
      "name": "c925",
      "api": { "base_url": "http://192.168.0.81:1888" },
      "chat": {
        "discord_channel_id": "222222222222222222"
        // discord_webhook_url ignored when channel_id is set
      }
    }
  ]
}
```

Validation rules added to `Config::validate`:

- If any telescope sets `discord_channel_id`, `chat.discord_bot.enabled`
  must be true.
- If `chat.discord_bot.enabled`, `token` must be non-empty.
- Telescopes with both `discord_channel_id` and `discord_webhook_url` get a
  startup warning ("webhook ignored — bot takes precedence") but are not
  an error.

## File / module changes

| File | Change |
|------|--------|
| `Cargo.toml` | add `serenity = "0.12"`, `poise = "0.6"` |
| `src/chat/discord_bot.rs` (new) | `DiscordBotService`, command framework, channel routing |
| `src/chat/mod.rs` | export `DiscordBotService`; `ChatTarget` gains `discord_channel_id` field |
| `src/chat/discord_service.rs` | `can_route` updated to defer when channel_id present |
| `src/config.rs` | new `DiscordBotConfig` struct; `TelescopeChatOverrides` gains `discord_channel_id`; validation rules |
| `src/service_wrapper.rs` | in `build_shared_chat_manager`, instantiate bot if enabled; pass `BotData` derived from telescope list |
| `src/chat_updater.rs` | no change — already routes through `ChatTarget` |

## Phasing

### Phase 1 — bot infrastructure + read commands

Ship the bot connected, channel-routed, registering and serving the
following read-only slash commands. All commands accept an optional
`--telescope <name>` parameter; when omitted, the bot resolves the
telescope from the channel the command was invoked in.

| Command | NINA endpoint(s) | Description |
|---------|------------------|-------------|
| `/status` | mount/info + sequence/json + event-history + filterwheel/info | one-page summary embed |
| `/sequence` | `/sequence/json` | containers, current target, meridian flip |
| `/target` | `/sequence/json` + latest `TS-TARGETSTART` | name, coords, rotation, time remaining |
| `/mount` | `/equipment/mount/info` | RA/Dec, Alt/Az, pier side, tracking, flip ETA |
| `/filter` | `/equipment/filterwheel/info` | current + available filters |
| `/focus` | `/equipment/focuser/last-af` | last AF: HFR, R², position, temperature |
| `/guider` | `/equipment/guider/info` | RMS, pixel scale, state |
| `/events [count]` | `/event-history` | last N events |
| `/last-image` | `/image/{idx}` + `/image/thumbnail/{idx}` | embed + thumbnail attachment |

Outbound message routing (event/image notifications) flows through the new
bot service for any telescope with `discord_channel_id` set. No visible
change for existing webhook-only telescopes.

### Phase 2 — live status message

Each bot-routed telescope gets a pinned embed in its channel, edited in
place on every poll cycle. Posted on first run; the message ID is
persisted to `./spacecat-state.json` (CWD by default; configurable via
`chat.discord_bot.state_file`).

State file schema:

```json
{
  "status_messages": {
    "c925":     { "channel_id": "111", "message_id": "999" },
    "askar107": { "channel_id": "222", "message_id": "888" }
  }
}
```

Operational flow per poll cycle:

1. Poll sequence / events / images (unchanged).
2. Post new event messages and new image messages (unchanged — the status
   message **supplements**, it does not replace, the event stream).
3. Rebuild the status embed and `edit_message(channel_id, message_id, embed)`.
4. If the edit returns 404 (message deleted), post a fresh status message
   and update the state file entry.

Atomic file writes via tempfile + rename. Reads on startup.

Live status embed fields:

- Current target (name, project, coords)
- Sequence progress (running / paused / finished, container count)
- Mount state (parked / tracking / slewing, RA/Dec, pier side)
- Last filter (name + ID)
- Guider RMS (total arcsec)
- Last image HFR + stars
- Meridian flip ETA
- Last autofocus R² + position change

### Phase 3 — write commands (ACL-gated)

Every write command:

1. Checks invoker's Discord user ID against `chat.discord_bot.write_acl`.
   Reject with an ephemeral error if not authorized.
2. For destructive actions (park, abort, stop-sequence), requires a button
   confirmation via `poise::CreateReply::components` before issuing the
   API call.

| Command | NINA endpoint | Destructive |
|---------|---------------|-------------|
| `/park` | `POST /equipment/mount/park` | ✓ (confirm) |
| `/unpark` | `POST /equipment/mount/unpark` | |
| `/home` | `POST /equipment/mount/home` | |
| `/change-filter <name>` | `POST /equipment/filterwheel/change-filter/{id}` | |
| `/move-focuser <pos>` | `POST /equipment/focuser/move-by-position/{n}` | |
| `/rotate <angle>` | `POST /equipment/rotator/move-mechanical/{n}` | |
| `/dither` | `POST /equipment/guider/dither` | |
| `/abort-capture` | `POST /equipment/camera/abort` | ✓ |
| `/pause-sequence` | `POST /sequence/pause` | |
| `/resume-sequence` | `POST /sequence/resume` | |
| `/start-sequence` | `POST /sequence/start` | ✓ (confirm) |
| `/stop-sequence` | `POST /sequence/stop` | ✓ (confirm) |
| `/cool <temp>` | `POST /equipment/camera/cool/{temp}` | |

The exact NINA POST endpoints must be verified against the ninaAPI source
before implementation — the earlier survey focused on events, not write
endpoints.

### Phase 4 — bot identity polish

- Set bot presence: `Watching 4 telescopes`.
- Optional rotation through per-telescope status messages
  (`Watching c925 — guiding NGC 7000`).
- Avatar / display name configurable.

Pure cosmetics; no functional change.

## Slash command channel-default resolution

```rust
async fn resolve_telescope<'a>(
    ctx: Context<'a>,
    override_name: Option<String>,
) -> Result<&'a SpaceCatApiClient, Error> {
    let data = ctx.data();
    if let Some(name) = override_name {
        return data
            .api_clients
            .get(&name)
            .ok_or_else(|| format!("Unknown telescope '{name}'. Known: {:?}", data.known_names()).into());
    }
    if let Some(name) = data.channel_to_telescope.get(&ctx.channel_id()) {
        return Ok(data.api_clients.get(name).expect("channel map invariant"));
    }
    Err(format!(
        "No telescope mapped to this channel. Pass --telescope <name>. Known: {:?}",
        data.known_names()
    )
    .into())
}
```

This lets a single ops/admin channel drive every rig by explicit name,
while per-telescope channels Just Work without specifying which one.

## Routing decisions recap

- **Bot scope:** single shared bot, one token, all telescopes.
- **Precedence when both webhook + channel configured:** bot wins; webhook
  ignored for that telescope.
- **ACL granularity (Phase 3):** Discord user IDs to start. Role-based
  ACLs can be added later if needed.
- **State file path:** CWD, default `./spacecat-state.json`.
- **Live status:** Phase 2 (supplements the event stream, doesn't replace it).

## Open implementation questions (to resolve when coding starts)

- Do slash commands need to be registered globally (slow, but works in
  every guild) or per-guild (fast, requires knowing the guild ID)?
  Per-guild is faster to iterate on during development; we can flip to
  global before release.
- Should the bot validate that every `discord_channel_id` is a channel it
  can see at startup? Probably yes, with a warning per missing channel.
- Thumbnail caching for `/last-image` — not required for correctness, but
  avoids re-fetching the same thumbnail if the command is run repeatedly.
