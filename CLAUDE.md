# Chatstronomy developer guide

Chatstronomy is the Rust runtime and hosted Hub for the standalone N.I.N.A.
plugin. N.I.N.A. data enters through the versioned Direct protocol. The plugin
owns all interaction with N.I.N.A.; the Rust runtime does not query N.I.N.A.
over HTTP.

## Commands

- Build all features: `cargo build --all-features`
- Build the lean plugin runtime: `cargo build --no-default-features`
- Test: `cargo test --all-features`
- Test the lean runtime: `cargo test --no-default-features`
- Format: `cargo fmt --all`
- Lint: `cargo clippy --all-targets --all-features -- -D warnings`

## Architecture

- `src/direct/`: local named-pipe transport and Direct protocol source.
- `src/hub/`: authenticated WebSocket Hub and remote Direct source.
- `src/plugin_runtime.rs`: plugin-owned local runtime bootstrap.
- `src/service_wrapper.rs`: starts one or more explicit Direct sources.
- `src/chat/`: Discord and Matrix delivery, formatting, and commands.
- `src/chat_updater.rs`: consumes normalized Direct events, state, images,
  autofocus data, and guiding samples.
- `src/charts.rs`: renders autofocus and guider graphs for chat attachments.
- `src/config.rs`: runtime chat and telescope configuration.
- `contracts/direct/v1/`: versioned protocol schema and compatibility fixtures.

There are two supported deployment paths:

1. Local mode: the N.I.N.A. plugin starts the lean Rust runtime and streams
   Direct messages over a named pipe. The runtime sends to locally configured
   Discord and Matrix destinations.
2. Hub mode: the plugin sends the same Direct messages to
   `wss://hub.chatstronomy.com`; the Hub owns chat credentials and commands.

Every configured telescope must have an explicit Direct source. Older Direct
payloads remain compatible through serde defaults and protocol fixtures.

## Testing expectations

Changes to the Direct envelope or payloads must update the schema, fixtures,
compatibility tests, and plugin in the standalone
`chatstronomy-nina-plugin` repository. Verify both the all-features Hub build
and the no-default-features plugin runtime. Never commit pairing tokens,
Discord credentials, Matrix credentials, or generated runtime bootstrap files.

## Releases

Tags matching `v*.*.*` build Linux Hub binaries and two signed Windows
executables: the full Hub runtime and lean plugin runtime. The workflow emits a
runtime manifest containing artifact sizes and SHA-256 hashes. The standalone
plugin repository pins an exact release and manifest checksum; it does not
compile Rust or download `latest` during packaging.
