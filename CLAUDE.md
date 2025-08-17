# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

- **Build**: `cargo build`
- **Run**: `cargo run`
- **Test**: `cargo test`
- **Check**: `cargo check`
- **Format**: `cargo fmt`
- **Lint**: `cargo clippy`

## Architecture

SpaceCat is a Rust-based astronomical observation system that interfaces with
NINA Advanced API for monitoring and posting to multiple chat services (Discord,
Matrix). The system provides real-time event tracking, image history management,
and sequence automation.

### Core Modules

- **`src/main.rs`**: Main application with CLI commands for API-only operations: sequence parsing, event monitoring, image analysis, mount information, autofocus data, and real-time polling
- **`src/config.rs`**: JSON-based configuration system with validation and error handling
- **`src/api.rs`**: HTTP client with generic retry logic for SpaceCat API endpoints
- **`src/events.rs`**: Event history structures and analysis methods
- **`src/images.rs`**: Image metadata structures and session statistics
- **`src/mount.rs`**: Mount information structures, status parsing, and position tracking
- **`src/sequence.rs`**: Sequence management, container parsing, and target extraction utilities
- **`src/autofocus.rs`**: Autofocus data structures, parsing, and analysis methods
- **`src/poller.rs`**: Real-time event polling with deduplication
- **`src/chat_updater.rs`**: Combined event and image polling with multi-chat service integration and autofocus detection
- **`src/chat/mod.rs`**: Chat service abstraction layer supporting multiple chat platforms
- **`src/chat/discord_service.rs`**: Discord webhook implementation for chat notifications
- **`src/chat/matrix_service.rs`**: Matrix client implementation for chat notifications
- **`src/service_wrapper.rs`**: Service abstraction layer for running as CLI or background service
- **`src/windows_service.rs`**: Windows service integration (Windows-only, conditionally compiled)

### API Integration

The system connects to NINA Advanced API at `http://192.168.0.82:1888` with endpoints:
- `/v2/api/version` - API health check and version info
- `/v2/api/event-history` - Equipment event monitoring  
- `/v2/api/image-history?all=true` - Image metadata and session statistics
- `/v2/api/sequence/json` - Current sequence status and target information
- `/v2/api/equipment/focuser/last-af` - Last autofocus session data
- `/v2/api/equipment/mount` - Mount status, position, and capabilities
- `/v2/api/image/{index}` - Individual image data with base64 encoding
- `/v2/api/image/thumbnail/{index}` - Thumbnail images for previews

### Configuration

Uses `config.json` for API and chat service settings:
```json
{
  "api": {
    "base_url": "http://192.168.0.82:1888",
    "timeout_seconds": 30,
    "retry_attempts": 3
  },
  "logging": {
    "level": "info",
    "enable_file_logging": false,
    "log_file": "spacecat.log"
  },
  "chat": {
    "discord": {
      "webhook_url": "https://discord.com/api/webhooks/YOUR_WEBHOOK_ID/YOUR_WEBHOOK_TOKEN",
      "enabled": true
    },
    "matrix": {
      "homeserver_url": "https://matrix.example.com",
      "username": "@spacecat:matrix.example.com",
      "password": "your_matrix_password",
      "room_id": "!roomid:matrix.example.com",
      "enabled": false
    }
  },
  "image_cooldown_seconds": 60
}
```

### Key Features

- **Equipment Events**: Real-time monitoring of astronomical equipment:
  - Camera operations (connect/disconnect, image capture)
  - Filterwheel management (filter changes between HA, OIII, SII, R, G, B, L)
  - Mount control (parking/unparking, positioning, slewing, meridian flips)
  - Focuser, rotator, and guider operations (start, sync, move, dither)
  - Autofocus completion with detailed results
  - Sequence management (start/stop/pause/resume/finished, advanced sequence stop)
  - Weather monitoring and safety systems
  - TS-TARGETSTART events for actual target tracking (overrides sequence targets)

- **Image History**: Comprehensive image metadata tracking:
  - Session statistics (exposure times, filter counts, temperature ranges)
  - Image type classification (LIGHT, DARK, FLAT, BIAS frames)
  - Filter analysis (broadband vs narrowband)
  - Calibration frame management
  - Target tracking and identification

- **Mount Information**: Comprehensive mount status and control monitoring:
  - Real-time position tracking (RA/Dec, Alt/Az coordinates)
  - Connection and operational status (connected, tracking, parked, slewing)
  - Meridian flip timing and pier side information
  - Site location data (latitude, longitude, elevation)
  - Mount capabilities and supported operations
  - Tracking modes and guide rates
  - Integration with sequence monitoring for target coordination

- **Autofocus System**: Advanced autofocus analysis and monitoring:
  - Real-time autofocus completion detection via AUTOFOCUS-FINISHED events
  - Comprehensive focus data parsing (positions, HFR values, temperatures)
  - Multiple curve fitting analysis (quadratic, hyperbolic, trend lines)
  - Focus quality assessment with R-squared values
  - Success criteria evaluation and position change tracking
  - Integration with Discord updater for automatic result display

- **Event Polling**: Real-time event monitoring with:
  - Event deduplication using timestamp+event+details keys
  - Configurable poll intervals (default 5 seconds)
  - Rate limiting and error handling
  - 231+ events tracked in production

- **Retry Logic**: Robust network handling with:
  - Exponential backoff for failed requests
  - Generic retry patterns across all API endpoints
  - Comprehensive error types (Network, Parse, HTTP)
  - Configurable timeout and retry attempts

- **Multi-Chat Integration**: Real-time notifications via multiple chat services:
  - **Discord**: Via webhooks with rich embeds and image attachments
  - **Matrix**: Via Matrix SDK with room auto-join and markdown formatting
  - Event notifications with comprehensive color-coded embeds organized by equipment type
  - Image capture alerts with detailed metadata and thumbnails
  - Configurable image cooldown to prevent Discord spam (default 60 seconds)
  - Skipped image counting with summary in next notification
  - Autofocus completion notifications with quality metrics
  - Enhanced target change notifications with real-time mount position data
  - Mount meridian flip events (MOUNT-BEFORE-FLIP, MOUNT-AFTER-FLIP) with detailed position info
  - Mount park events (MOUNT-PARKED) with position, site location, and tracking status
  - Mount information includes RA/Dec, Alt/Az, pier side, tracking status, and sidereal time
  - TS-TARGETSTART event handling:
    - Automatically detects and uses actual observation targets from TS-TARGETSTART events
    - Overrides sequence targets (which may show "Sequential Instruction Set")
    - Tracks the most recent target from event history on startup
    - Updates target display in real-time as new TS-TARGETSTART events arrive
  - Configurable via config.json
  - Non-blocking operation that won't interrupt observations

- **Windows Service Support**: Production-ready Windows service integration (optional):
  - Full Windows service lifecycle management (install/uninstall/start/stop)
  - Automatic startup with Windows boot
  - Background execution without user login required
  - Windows Event Log integration for centralized logging
  - System-wide configuration storage (`C:\ProgramData\SpaceCat\config.json`)
  - Graceful shutdown handling with proper service status reporting
  - Platform-specific compilation (automatically available on Windows)
  - Compatible with Windows 10/11 and Windows Server 2016+

- **Testing**: Comprehensive unit test coverage with file-based testing:
  - All modules include unit tests for data parsing and analysis
  - File-based testing using example JSON files (e.g., example_sequence.json)
  - Base64 image processing validation tests
  - API response structure validation

### Data Structures

- **Events**: Timestamped equipment state changes with optional details (including AUTOFOCUS-FINISHED and TS-TARGETSTART events)
  - All event types are defined as constants in `event_types` module for type-safe matching
  - No string literals in event matching - all comparisons use predefined constants
  - TS-TARGETSTART events include target name, coordinates, rotation, and project information
- **Images**: Metadata including exposure times, filters, temperatures, statistics
- **Sequences**: Container-based automation with triggers, conditions, and target extraction
- **Autofocus**: Comprehensive focus session data with measurement points, curve fitting results, and quality metrics
- **Configuration**: JSON-based settings with validation

### CLI Commands

The system provides comprehensive CLI commands for all functionality. All commands now connect directly to the API for real-time data:

- `cargo run -- sequence` - Get current sequence information and extract active targets
- `cargo run -- events` - Load and analyze event history from API
- `cargo run -- last-events --count 10` - Display the most recent events with details
- `cargo run -- images` - Load and analyze image history with session statistics
- `cargo run -- get-image <index>` - Retrieve specific images with base64 decoding
- `cargo run -- get-thumbnail <index>` - Download image thumbnails
- `cargo run -- poll` - Real-time event polling with configurable intervals
- `cargo run -- chat-updater` - Combined event/image monitoring with multi-chat service integration
- `cargo run -- last-autofocus` - Display detailed autofocus analysis and quality metrics
- `cargo run -- mount-info` - Display comprehensive mount status, position, and capabilities

The system successfully demonstrates live telescope operation monitoring with 90 calibration images across 7 filters and real-time event tracking.

## GitHub Automation

### Continuous Integration
- **CI Pipeline**: Automated testing on Linux, Windows, and macOS with stable and beta Rust
- **Code Quality**: Formatting checks with `rustfmt`, linting with `clippy`
- **Security**: Dependency vulnerability scanning with `cargo audit`
- **Coverage**: Code coverage reporting with `tarpaulin` (main branch only)

### Release Automation
Automated binary building on GitHub tag creation (`v*.*.*`):

- **Linux x86_64**: `spacecat-linux-x86_64` - Standard Linux systems
- **Linux aarch64**: `spacecat-linux-aarch64` - ARM64 systems (Raspberry Pi 4, etc.)
- **Windows x64**: `spacecat-windows-x64.exe` - 64-bit Windows systems

Release process:
1. Update version in `Cargo.toml`
2. Create git tag: `git tag v1.0.0 && git push origin v1.0.0`
3. GitHub Actions automatically builds cross-platform binaries
4. Creates GitHub release with all artifacts

### Dependency Management
- **Dependabot**: Weekly dependency updates for Cargo and GitHub Actions
- **Security**: Automated vulnerability scanning and alerts
- **Current Dependencies**:
  - tokio = "1" (latest: 1.47)
  - clap = "4" (latest: 4.5)
  - matrix-sdk = "0.13" (upgraded from 0.8)
  - serde = "1.0", reqwest = "0.12", base64 = "0.22"

## Recent Updates

### Multi-Chat Service Support
- Added abstraction layer for multiple chat services
- Implemented Matrix support alongside Discord
- Auto-join Matrix room invitations
- Welcome message on startup showing current observatory status
- Backward compatibility with legacy Discord configuration

### Target Tracking Enhancement
- TS-TARGETSTART event support for accurate target identification
- Automatic target override from actual observation events
- Prevents duplicate target change notifications
- Target precedence: TS-TARGETSTART > Sequence targets

### Dependency Upgrades
- **matrix-sdk**: 0.8 → 0.13 (API compatibility updates)
- **tokio**: 1.0 → 1.47 (latest async runtime)
- **clap**: 4.0 → 4.5 (latest CLI framework)
- Updated integration tests for new command names
