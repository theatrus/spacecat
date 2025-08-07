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

SpaceCat is a Rust-based astronomical observation system that interfaces with SpaceCat API for telescope automation and monitoring. The system provides real-time event tracking, image history management, and sequence automation.

### Core Modules

- **`src/main.rs`**: Main application demonstrating sequence parsing, event monitoring, image analysis, and real-time polling
- **`src/config.rs`**: JSON-based configuration system with validation and error handling
- **`src/api.rs`**: HTTP client with generic retry logic for SpaceCat API endpoints
- **`src/events.rs`**: Event history structures and analysis methods
- **`src/images.rs`**: Image metadata structures and session statistics
- **`src/sequence.rs`**: Sequence management and container parsing
- **`src/poller.rs`**: Real-time event polling with deduplication

### API Integration

The system connects to SpaceCat API at `http://192.168.0.82:1888` with endpoints:
- `/v2/api/version` - API health check and version info
- `/v2/api/event-history` - Equipment event monitoring  
- `/v2/api/image-history?all=true` - Image metadata and session statistics

### Configuration

Uses `config.json` for API settings:
```json
{
  "api": {
    "base_url": "http://192.168.0.82:1888",
    "timeout_seconds": 30,
    "retry_attempts": 3
  },
  "logging": {
    "level": "info"
  }
}
```

### Key Features

- **Equipment Events**: Real-time monitoring of astronomical equipment:
  - Camera operations (connect/disconnect, image capture)
  - Filterwheel management (filter changes between HA, OIII, SII, R, G, B, L)
  - Mount control (parking/unparking, positioning)
  - Focuser, rotator, and guider operations
  - Weather monitoring and safety systems

- **Image History**: Comprehensive image metadata tracking:
  - Session statistics (exposure times, filter counts, temperature ranges)
  - Image type classification (LIGHT, DARK, FLAT, BIAS frames)
  - Filter analysis (broadband vs narrowband)
  - Calibration frame management

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

### Data Structures

- **Events**: Timestamped equipment state changes with optional details
- **Images**: Metadata including exposure times, filters, temperatures, statistics
- **Sequences**: Container-based automation with triggers and conditions
- **Configuration**: JSON-based settings with validation

The system successfully demonstrates live telescope operation monitoring with 90 calibration images across 7 filters and real-time event tracking.