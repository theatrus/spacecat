# SpaceCat 🔭

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust](https://img.shields.io/badge/rust-1.93+-orange.svg)](https://www.rust-lang.org)
[![Build Status](https://github.com/theatrus/spacecat/workflows/CI/badge.svg)](https://github.com/theatrus/spacecat/actions)

**SpaceCat** is a Rust-based monitor that watches one or more [NINA](https://nighttime-imaging.eu) imaging
rigs over the [Advanced API](https://github.com/christian-photo/ninaAPI) extension and posts events,
images, autofocus results, and live status to Discord (via webhook or a full bot) and/or Matrix.

A single SpaceCat instance can drive **multiple telescopes** concurrently — one process, one Discord bot
identity, one Matrix login, with per-telescope channels, webhooks, and rooms.

## On Vibe Coding

This code base was a test of extensively vibe-coding this integration. It was
fast to iterate on, but has a lot of misc code smells. Not the worst result
really.

## 🚀 Quick Start

### Installation

#### Option 1: RPM Package (Recommended for Fedora/RHEL/CentOS)

```bash
# Download the latest release
wget https://github.com/theatrus/spacecat/releases/latest/download/spacecat-*.rpm

# Install the package
sudo dnf install ./spacecat-*.rpm

# Configure the system
sudo vim /etc/spacecat/config.json

# Enable and start the service
sudo systemctl enable --now spacecat.service

# Check status
sudo systemctl status spacecat.service
```

#### Option 2: Build from Source

```bash
# Clone the repository
git clone https://github.com/theatrus/spacecat.git
cd spacecat

# Build the project (Windows service support included automatically on Windows)
cargo build --release

# Copy the binary to your PATH
sudo cp target/release/spacecat /usr/local/bin/

# Start from the example config
cp config.example.json ~/.config/spacecat/config.json
```

### Configuration

SpaceCat uses a single `config.json` describing **shared chat infrastructure**
(one Matrix login, one Discord bot, default webhook) plus a **list of telescopes**,
each with its own NINA API URL and optional per-telescope chat overrides.

A minimal single-telescope config:

```json
{
  "logging": { "level": "info", "enable_file_logging": false, "log_file": "spacecat.log" },
  "chat": {
    "discord": {
      "enabled": true,
      "default_webhook_url": "https://discord.com/api/webhooks/YOUR_WEBHOOK_URL"
    }
  },
  "telescopes": [
    {
      "name": "primary",
      "api": {
        "base_url": "http://192.168.0.82:1888",
        "timeout_seconds": 30,
        "retry_attempts": 3
      },
      "image_cooldown_seconds": 60
    }
  ]
}
```

A multi-telescope, multi-channel config:

```json
{
  "logging": { "level": "info", "enable_file_logging": false, "log_file": "spacecat.log" },
  "chat": {
    "discord_bot": {
      "enabled": true,
      "token": "BOT_TOKEN_FROM_DISCORD_DEVELOPER_PORTAL",
      "application_id": 123456789012345678,
      "live_status": true,
      "write_acl": [987654321098765432]
    },
    "matrix": {
      "enabled": true,
      "homeserver_url": "https://matrix.example.com",
      "username": "@spacecat:matrix.example.com",
      "password": "your_password",
      "default_room_id": "!default:matrix.example.com"
    }
  },
  "telescopes": [
    {
      "name": "c925",
      "api": { "base_url": "http://192.168.0.81:1888", "timeout_seconds": 30, "retry_attempts": 3 },
      "chat": { "discord_channel_id": 111111111111111111 },
      "image_cooldown_seconds": 60
    },
    {
      "name": "esprit",
      "api": { "base_url": "http://192.168.0.83:1888", "timeout_seconds": 30, "retry_attempts": 3 },
      "chat": { "discord_channel_id": 222222222222222222 }
    }
  ]
}
```

The **legacy single-telescope config shape** (top-level `api` + `chat`) still loads — it's
normalized into a one-element `telescopes` list with name `"default"`.

See [Configuration Reference](#-configuration-reference) below for every field.

## 📖 Usage

### Command Line Interface

```bash
# Show help
spacecat --help

# --- Read-only commands (single-telescope, --telescope required when more than one) ---

# Current sequence (containers, target, meridian flip ETA)
spacecat sequence
spacecat --telescope c925 sequence

# Event history
spacecat events
spacecat last-events --count 20

# Image history + session statistics
spacecat images

# Download a specific image
spacecat get-image 0 --params "autoPrepare=true"

# Image thumbnail
spacecat get-thumbnail 5 --output image_5.jpg --image-type LIGHT

# Last autofocus run details
spacecat last-autofocus

# Mount status
spacecat mount-info

# Real-time event polling (no chat posting)
spacecat poll --interval 2 --count 5

# --- Chat updater: fans out across every configured telescope ---
spacecat chat-updater --interval 5
```

The `--telescope <name>` flag picks which telescope a single-scope command targets. If you
have only one telescope configured, it's optional. With multiple telescopes and no flag,
single-scope commands print the known names and exit. `chat-updater` ignores `--telescope`
and always fans out across every configured telescope concurrently.

### Custom Configuration File

```bash
# Use a custom configuration file (use the long form — there is no short -c flag)
spacecat --config /path/to/custom-config.json sequence
spacecat --config ~/observatory/config.json --telescope c925 mount-info
```

### Service Mode

#### Linux (systemd)

```bash
# Check service logs
journalctl -u spacecat.service -f

# Restart after configuration changes
sudo systemctl restart spacecat.service

# Monitor service status
sudo systemctl status spacecat.service
```

#### Windows Service

```powershell
# Install the service (run as Administrator)
spacecat.exe windows-service install

# Configure the service
# Edit C:\ProgramData\SpaceCat\config.json

# Start the service
spacecat.exe windows-service start

# Check service status / Stop / Uninstall
spacecat.exe windows-service status
spacecat.exe windows-service stop
spacecat.exe windows-service uninstall
```

Windows service support is automatically compiled in on Windows builds. The service polls
every configured telescope concurrently using the same shared Discord bot and Matrix client.

## ✨ Features

- **Multi-telescope monitoring** — one process drives multiple NINA rigs in parallel, each
  with its own channel/webhook/room. One shared Discord bot and one shared Matrix login.
- **Discord webhooks OR full bot** — choose per telescope. Webhook for stateless posts,
  bot for slash commands + per-channel routing + live status messages. Bot takes precedence
  when both are configured for a telescope.
- **`/spacecat` slash commands** — read-only inspection (`status`, `sequence`, `target`,
  `mount`, `filter`, `focus`, `guider`, `events`, `last-image`) plus ACL-gated write commands
  (`park`, `unpark`, `home`, `change-filter`, `cool`, `warm`, `autofocus`, `guider-start`,
  `guider-stop`, `abort-capture`, `start-sequence`, `stop-sequence`). Destructive actions
  require a button confirmation.
- **Channel-driven defaults** — `/spacecat status` invoked in a telescope's channel
  automatically targets that telescope.
- **Live status embed** *(opt-in via `chat.discord_bot.live_status`)* — one pinned status
  message per telescope channel, edited in place on state changes (target switches,
  filter changes, mount events, etc.). Message IDs are persisted in `spacecat-state.json`
  so the same embed survives restarts.
- **Smart target tracking** — TS-TARGETSTART / TS-NEWTARGETSTART events override generic
  sequence target names; tolerant parsing handles NINA's empty-array coord payloads.
- **Rich event notifications** — events are enriched on the fly:
  - `AUTOFOCUS-FINISHED` → fetches `/equipment/focuser/last-af` for HFR, R², position
  - `FILTERWHEEL-CHANGED` → fetches `/equipment/filterwheel/info` when payload is incomplete
  - `GUIDER-START` / `-DITHER` → fetches `/equipment/guider/info` for RMS error
  - `ROTATOR-MOVED` → parses From/To/Δ from payload
  - `ROTATOR-SYNCED` → fetches `/equipment/rotator/info`
  - `FOCUSER-USER-FOCUSED` → fetches `/equipment/focuser/info`
  - `MOUNT-*` → fetches `/equipment/mount/info` for live position
- **Image notifications** — automatic thumbnail downloads with configurable per-telescope
  cooldown (`image_cooldown_seconds`) to limit channel spam.
- **Matrix support** — one shared login serves every telescope; per-telescope `matrix_room_id`
  override or fall back to the shared `default_room_id`.
- **Startup state inference** — walks recent event history on startup to compute the current
  observatory state ("Waiting until 22:02 -07:00 — 26 min remaining; Mount unparked; Guiding")
  and posts it as a welcome message.

## 🤖 Chat Services

You can mix and match: per telescope, choose webhook or bot for Discord; optionally also
post to a Matrix room.

### Option A — Discord Webhook (simplest)

1. In Discord: **Server Settings → Integrations → Webhooks → New Webhook**, copy the URL.
2. Add to config:
   ```json
   "chat": {
     "discord": {
       "enabled": true,
       "default_webhook_url": "https://discord.com/api/webhooks/..."
     }
   }
   ```
3. Per telescope, optionally override:
   ```json
   "telescopes": [
     {
       "name": "c925",
       "api": { "base_url": "..." },
       "chat": { "discord_webhook_url": "https://discord.com/api/webhooks/OTHER" }
     }
   ]
   ```

### Option B — Discord Bot (slash commands + per-channel routing)

1. Create an application + bot in the [Discord Developer Portal](https://discord.com/developers/applications).
   Copy the **bot token** and **application ID**.
2. Add to config:
   ```json
   "chat": {
     "discord_bot": {
       "enabled": true,
       "token": "...",
       "application_id": 123456789012345678,
       "live_status": true,
       "write_acl": [your_discord_user_id]
     }
   }
   ```
3. Invite the bot to your server with this URL (replace `<APP_ID>`):
   ```
   https://discord.com/oauth2/authorize?client_id=<APP_ID>&permissions=52224&scope=bot+applications.commands
   ```
   The `52224` permission set covers: View Channel + Send Messages + Embed Links + Attach Files.
4. Map each telescope to a channel:
   ```json
   "telescopes": [
     {
       "name": "c925",
       "api": { "base_url": "..." },
       "chat": { "discord_channel_id": 111111111111111111 }
     }
   ]
   ```
   The bot takes precedence over `discord_webhook_url` for telescopes with `discord_channel_id`.
5. Slash command global registration takes up to an hour on a new bot. After that, type `/spacecat`
   in any channel the bot can see.

### Matrix

```json
"chat": {
  "matrix": {
    "enabled": true,
    "homeserver_url": "https://matrix.org",
    "username": "@spacecat:matrix.org",
    "password": "your_password",
    "default_room_id": "!your_room_id:matrix.org"
  }
}
```

Per-telescope override `matrix_room_id` if you want each rig to post in its own room;
otherwise everything goes to `default_room_id` with telescope-name title prefixes (`[c925]`, etc.).
SpaceCat auto-joins any pending room invitations on startup.

## 🎛️ Configuration Reference

### Top-level

| Field | Type | Description |
|---|---|---|
| `logging.level` | string | `error` / `warn` / `info` / `debug` / `trace` |
| `logging.enable_file_logging` | bool | (reserved) |
| `logging.log_file` | string | (reserved) |
| `chat` | object | Shared chat infrastructure (see below) |
| `telescopes` | array | One entry per NINA rig (see below). Must contain at least one |

### `chat.discord` (webhook posting)

| Field | Type | Description |
|---|---|---|
| `enabled` | bool | Enable webhook posting |
| `default_webhook_url` | string | Fallback webhook for telescopes that don't override (legacy `webhook_url` alias accepted) |

### `chat.discord_bot` (full bot)

| Field | Type | Description |
|---|---|---|
| `enabled` | bool | Connect a Discord bot to the gateway and register slash commands |
| `token` | string | Bot token from the Discord Developer Portal |
| `application_id` | u64 | Optional — reserved for HTTP-interaction tooling |
| `public_key` | string | Optional — reserved for HTTP interactions |
| `default_channel_id` | u64 | Optional fallback channel ID |
| `live_status` | bool | Enable per-telescope pinned live-status embed (default `false`) |
| `state_file` | string | Where to persist live-status message IDs (default `./spacecat-state.json`) |
| `write_acl` | array of u64 | Discord user IDs allowed to invoke write commands |

### `chat.matrix`

| Field | Type | Description |
|---|---|---|
| `enabled` | bool | Connect to Matrix |
| `homeserver_url` | string | e.g. `https://matrix.org` |
| `username` | string | e.g. `@spacecat:matrix.org` |
| `password` | string | Plain password (use a dedicated bot account) |
| `default_room_id` | string | Fallback room for telescopes that don't override (legacy `room_id` alias accepted) |

### `telescopes[]`

| Field | Type | Description |
|---|---|---|
| `name` | string | Identifier used in CLI `--telescope` and as a `[name]` prefix in chat |
| `api.base_url` | string | NINA Advanced API URL, e.g. `http://192.168.0.82:1888` |
| `api.timeout_seconds` | u64 | HTTP timeout |
| `api.retry_attempts` | u32 | Retry count for transient failures |
| `image_cooldown_seconds` | u64 | Seconds between image notifications per telescope (default 60) |
| `chat.discord_webhook_url` | string | Override the shared default webhook |
| `chat.discord_channel_id` | u64 | Route this telescope through the bot to this channel (takes precedence over webhook) |
| `chat.matrix_room_id` | string | Override the shared default Matrix room |

### Validation

SpaceCat validates the config on startup. Common errors:

- `discord_channel_id` set on a telescope but `chat.discord_bot.enabled` is false
- `chat.discord_bot.enabled` true but `token` empty
- Two telescopes share the same `name`
- Matrix enabled with missing `homeserver_url`/`username`/`password`

## 🔌 NINA API Endpoints Used

SpaceCat talks to a number of NINA Advanced API endpoints. All are under `/v2/api/`:

| Endpoint | Used for |
|---|---|
| `/version` | API health check |
| `/event-history` | Event monitoring (the main poll) |
| `/image-history?all=true` | Image metadata and session statistics |
| `/image/{index}` | Image download |
| `/image/thumbnail/{index}` | Thumbnail download |
| `/sequence/json` | Current sequence + meridian-flip ETA |
| `/equipment/mount/info` | Mount status, position, capabilities |
| `/equipment/filterwheel/info` | Current filter + available filters |
| `/equipment/focuser/info` | Focuser position + temperature |
| `/equipment/focuser/last-af` | Last autofocus run details |
| `/equipment/guider/info` | Guider RMS, state, pixel scale |
| `/equipment/rotator/info` | Rotator angle + sync state |

Write endpoints used by the `/spacecat ...` Discord slash commands:

| Endpoint | Triggered by |
|---|---|
| `/equipment/mount/park` | `/spacecat park` |
| `/equipment/mount/unpark` | `/spacecat unpark` |
| `/equipment/mount/home` | `/spacecat home` |
| `/equipment/filterwheel/change-filter?filterId=N` | `/spacecat change-filter <name>` |
| `/equipment/focuser/auto-focus?cancel=<bool>` | `/spacecat autofocus [cancel:true]` |
| `/equipment/guider/start?calibrate=<bool>` | `/spacecat guider-start` |
| `/equipment/guider/stop` | `/spacecat guider-stop` |
| `/equipment/camera/cool?temperature=T&minutes=M` | `/spacecat cool <temp>` |
| `/equipment/camera/warm?minutes=M` | `/spacecat warm` |
| `/equipment/camera/abort-exposure` | `/spacecat abort-capture` |
| `/sequence/start?skipValidation=<bool>` | `/spacecat start-sequence` |
| `/sequence/stop` | `/spacecat stop-sequence` |

## 🏗️ Development

### Prerequisites

- **Rust 1.93+**: Install from [rustup.rs](https://rustup.rs/)
- **Git**: For version control
- **OpenSSL development headers**: For HTTPS support

SQLite is bundled statically — no external SQLite installation required.

```bash
# Fedora/RHEL/CentOS
sudo dnf install rust cargo openssl-devel git

# Ubuntu/Debian
sudo apt install cargo rustc libssl-dev git pkg-config

# macOS
brew install rust openssl git
```

### Building

```bash
# Clone and build
git clone https://github.com/theatrus/spacecat.git
cd spacecat

# Development build
cargo build

# Release build
cargo build --release

# Run tests
cargo test

# Lint
cargo clippy --all-targets --all-features -- -D warnings

# Format
cargo fmt
```

### Project layout

```
src/
├── main.rs              # CLI entry point
├── lib.rs               # Module declarations
├── config.rs            # Config parsing + multi-telescope validation
├── api.rs               # SpaceCatApiClient — read + write helpers
├── chat/
│   ├── mod.rs           # ChatService trait + ChatServiceManager + ChatTarget
│   ├── discord_service.rs  # Webhook-based posting
│   ├── discord_bot.rs   # Serenity/Poise bot with /spacecat commands
│   ├── matrix_service.rs   # Matrix client (one shared login)
│   └── status_state.rs  # Live-status message persistence
├── chat_updater.rs      # Per-telescope poll loop
├── service_wrapper.rs   # Spawns one ChatUpdater per telescope
├── events.rs            # Event types + EventDetails (per-event payload variants)
├── filterwheel.rs       # /equipment/filterwheel/info
├── focuser.rs           # /equipment/focuser/info
├── guider.rs            # /equipment/guider/info
├── rotator.rs           # /equipment/rotator/info
├── mount.rs             # /equipment/mount/info
├── images.rs            # /image-history + thumbnail
├── autofocus.rs         # /equipment/focuser/last-af
├── sequence.rs          # /sequence/json
└── windows_service.rs   # Windows service integration (cfg(windows))
```

## 🤝 Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

### Development Workflow

1. **Fork** the repository
2. **Clone** your fork: `git clone https://github.com/YOUR-USERNAME/spacecat.git`
3. **Create** a feature branch: `git checkout -b feature/amazing-feature`
4. **Make** your changes and add tests
5. **Test** your changes: `cargo test && cargo clippy --all-targets --all-features -- -D warnings`
6. **Commit** your changes: `git commit -m 'Add amazing feature'`
7. **Push** to your branch: `git push origin feature/amazing-feature`
8. **Create** a Pull Request

### Code Style

- Follow Rust standard formatting: `cargo fmt`
- Ensure clippy passes: `cargo clippy --all-targets --all-features -- -D warnings`
- Add tests for new functionality
- Update documentation as needed

## 📄 License

This project is licensed under the **Apache License 2.0** — see the [LICENSE](LICENSE) file for details.


---

**Made with ❤️ for the astronomy community**

*SpaceCat helps astronomers automate and monitor their observations, bringing the universe closer to everyone.*
