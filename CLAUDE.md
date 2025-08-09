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
NINA Advanced API for monitoring and posting to Discord (or other services). The
system provides real-time event tracking, image history management, and sequence
automation.

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
- **`src/dual_poller.rs`**: Combined event and image polling with Discord integration and autofocus detection

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

Uses `config.json` for API and Discord settings:
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
  "discord": {
    "webhook_url": "https://discord.com/api/webhooks/YOUR_WEBHOOK_ID/YOUR_WEBHOOK_TOKEN",
    "enabled": true,
    "image_cooldown_seconds": 60
  }
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

- **Discord Integration**: Real-time notifications via Discord webhooks:
  - Event notifications with comprehensive color-coded embeds organized by equipment type
  - Image capture alerts with detailed metadata and thumbnails
  - Configurable image cooldown to prevent Discord spam (default 60 seconds)
  - Skipped image counting with summary in next notification
  - Autofocus completion notifications with quality metrics
  - Enhanced target change notifications with real-time mount position data
  - Mount meridian flip events (MOUNT-BEFORE-FLIP, MOUNT-AFTER-FLIP) with detailed position info
  - Mount park events (MOUNT-PARKED) with position, site location, and tracking status
  - Mount information includes RA/Dec, Alt/Az, pier side, tracking status, and sidereal time
  - Configurable via config.json
  - Non-blocking operation that won't interrupt observations

- **Testing**: Comprehensive unit test coverage with file-based testing:
  - All modules include unit tests for data parsing and analysis
  - File-based testing using example JSON files (e.g., example_sequence.json)
  - Base64 image processing validation tests
  - API response structure validation

### Data Structures

- **Events**: Timestamped equipment state changes with optional details (including AUTOFOCUS-FINISHED events)
  - All event types are defined as constants in `event_types` module for type-safe matching
  - No string literals in event matching - all comparisons use predefined constants
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
- `cargo run -- discord-updater` - Combined event/image monitoring with Discord integration
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
