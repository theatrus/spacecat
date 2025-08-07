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

This is a Rust project called "spacecat" that appears to be related to astronomical observation and equipment control. The codebase is currently minimal with just a basic main.rs file.

### Key Components

- **Equipment Events**: The project handles various astronomical equipment events including:
  - Camera operations (connect/disconnect, image capture)
  - Filterwheel management (filter changes between HA, OIII, SII, R, G, B, L)
  - Mount control (parking/unparking, positioning)
  - Focuser, rotator, and guider operations
  - Weather monitoring and safety systems

- **Sequence Management**: Based on the example JSON files, the system manages:
  - Imaging sequences with multiple filters and exposure times
  - Automated equipment startup/shutdown procedures
  - Trigger-based actions (dithering, autofocus, meridian flips)
  - Condition monitoring (altitude limits, temperature changes)

- **Data Structure**: Events are timestamped JSON objects containing:
  - Equipment status changes
  - Filter wheel position updates
  - Image save notifications
  - System connection/disconnection events

The project appears to be in early development stage with the core Rust structure established but minimal implementation in place.