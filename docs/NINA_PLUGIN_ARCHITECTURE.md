# N.I.N.A. integration architecture

SpaceCat treats **how observatory data is collected** and **where chat services
run** as independent choices. Direct mode is an additional integration, not a
replacement for Advanced API mode.

## Supported combinations

| Source | Local delivery | Hosted delivery |
|---|---|---|
| Advanced API | Rust agent polls N.I.N.A. and runs user-owned Discord/Matrix clients | Rust agent polls N.I.N.A. and relays outbound to the hosted service |
| N.I.N.A. Direct | N.I.N.A. plugin sends native events to the local Rust agent | N.I.N.A. plugin sends native events to the Rust agent, which relays outbound |

Advanced API deployments remain usable without installing the SpaceCat N.I.N.A.
plugin. When the plugin is installed in Advanced API mode, it acts as an
in-application configuration and health surface for the Rust agent rather than
becoming a second data source.

## Components

### `RigSource` (Rust)

The source-neutral interface consumed by polling, state tracking, chat commands,
and image delivery. `AdvancedApiSource` is the first implementation and delegates
to the existing `SpaceCatApiClient`.

`NinaDirectSource` will satisfy the same operations from the latest native
snapshot and request/response messages carried over local IPC. Each source
advertises capabilities so callers can report unsupported operations clearly.

Only one source is authoritative for a rig. SpaceCat will not automatically
merge or fail over between Direct and Advanced API sources because duplicate
events and ambiguous command results are more dangerous than a visible outage.

### SpaceCat N.I.N.A. plugin (C#)

The plugin is intentionally thin. It will:

1. Subscribe to N.I.N.A. device, image-save, autofocus, guider, and sequence
   mediators.
2. Convert native information into versioned SpaceCat messages.
3. Execute an explicit allowlist of commands through N.I.N.A. mediators.
4. Provide source/delivery settings and connection health inside N.I.N.A.
5. Connect to a per-user Rust agent over a Windows named pipe.

It will not contain Discord, Matrix, notification formatting, chart rendering,
or hosted-service logic.

### SpaceCat agent (Rust)

One agent process per Windows user can serve multiple N.I.N.A. processes and
profiles. It owns state reduction, durable event delivery, chart rendering, and
chat clients. In hosted mode it makes an outbound authenticated WebSocket
connection; no N.I.N.A. or SpaceCat listener is exposed to the internet.

The agent will be included in the N.I.N.A. plugin ZIP, but it remains possible
to install and run the Rust application independently for Advanced API and
non-Windows deployments.

### Hosted SpaceCat service

The hosted service owns the central Discord/Matrix credentials, tenant routing,
and connected-agent registry. Read requests use a freshness-stamped cached
snapshot or make a bounded round trip to the agent. Write commands are never
queued for an offline rig.

## Protocol direction

The Direct protocol will use versioned envelopes for:

- agent/plugin hello and capability negotiation;
- full snapshots and incremental events;
- image metadata and bounded thumbnails;
- queries and query results;
- commands and command results;
- acknowledgements and heartbeats.

Local communication uses a Windows named pipe. Hosted communication uses the
same logical envelopes over TLS WebSockets. The initial wire representation will
be JSON so both implementations can be inspected and evolved easily.

## Identity and safety

- A rig is identified by a SpaceCat installation ID plus the N.I.N.A. profile
  ID, not by a user-editable telescope name.
- Discord and Matrix secrets for local mode are stored using Windows-protected
  credential storage, never in normal profile JSON or process arguments.
- Remote control is disabled by default and enforced at both the chat layer and
  the N.I.N.A. plugin.
- Commands carry correlation IDs and expiry times. Disconnected or expired
  commands fail rather than executing later.
- Only thumbnails and selected metadata leave the observatory by default.

## Delivery sequence

1. Preserve current behavior behind `AdvancedApiSource`.
2. Remove concrete API-client dependencies from chat polling and commands.
3. Define and test the Direct IPC envelopes.
4. Add N.I.N.A. native snapshots/events and read-only commands.
5. Add local sidecar supervision and plugin settings.
6. Validate feature parity, including autofocus and guide graphs.
7. Add hosted pairing and relay.
8. Package the plugin DLL and Windows agent into a checksummed N.I.N.A. plugin ZIP.
