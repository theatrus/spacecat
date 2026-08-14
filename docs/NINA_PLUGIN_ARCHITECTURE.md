# N.I.N.A. integration architecture

SpaceCat treats **how observatory data is collected** and **where chat services
run** as independent choices. Direct mode is an additional integration, not a
replacement for Advanced API mode.

## Supported combinations

| Source | Local delivery | Central/hosted delivery |
|---|---|---|
| Advanced API | Rust agent polls N.I.N.A. and runs user-owned Discord/Matrix clients | Rust agent polls N.I.N.A. and relays outbound to the hosted service |
| N.I.N.A. Direct | N.I.N.A. plugin connects to SpaceCat over a Windows named pipe | N.I.N.A. plugin opens an outbound authenticated WebSocket to the SpaceCat hub |

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
snapshot and request/response messages carried over the Direct protocol. Each
source advertises capabilities so callers can report unsupported operations
clearly.

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
5. Connect to SpaceCat over a Windows named pipe when local or an outbound TLS
   WebSocket when SpaceCat runs on another system.

It will not contain Discord, Matrix, notification formatting, chart rendering,
or hosted-service logic.

### SpaceCat runtime and hub (Rust)

One SpaceCat process owns state reduction, durable event delivery, chart
rendering, and the Discord/Matrix clients. It accepts any number of Direct
plugin connections and gives every connected profile an independent rig source.

When SpaceCat runs on the N.I.N.A. computer, the plugin uses a per-user named
pipe. When SpaceCat runs centrally, each plugin connects outward to its TLS
WebSocket endpoint. The remote N.I.N.A. computers do not run Discord or Matrix
bots and do not need an inbound listener or Advanced API.

The Windows SpaceCat executable may still be included in the plugin ZIP for the
all-in-one local option. It remains possible to install and run the Rust
application independently for Advanced API and non-Windows deployments.

### Hosted SpaceCat service

The hosted service owns the central Discord/Matrix credentials, tenant routing,
and connected-plugin registry. Read requests use a freshness-stamped cached
snapshot or make a bounded round trip to the relevant plugin. Write commands
are never queued for an offline rig.

## Multiple N.I.N.A. instances

Every plugin sends a versioned `client_hello` containing three identifiers:

- `node_id`: generated once and stored under the Windows user's local SpaceCat
  data directory; all N.I.N.A. processes in that installation share it;
- `profile_id`: the stable GUID exposed by `IProfileService.ActiveProfile.Id`;
- `session_id`: generated each time the plugin loads and used to distinguish a
  reconnect from a second process.

The stable rig key is `(node_id, profile_id)`. This supports both multiple
profiles on one computer and profiles on entirely different computers. A
profile name is display metadata and can change without changing routing.

One profile may have only one active session. SpaceCat rejects a second active
process claiming the same rig, but permits the original session to reconnect
and atomically replaces its stale transport. If N.I.N.A. switches profiles on a
live connection, the registry moves that connection only after confirming the
new profile is not already active. A transport disconnect or missed-heartbeat
lease expiry releases the profile so a restarted N.I.N.A. process can register.

For example, all of these feed one bot process:

```text
N.I.N.A. PC A / profile C925    --WSS-->  SpaceCat hub  --> Discord + Matrix
N.I.N.A. PC A / profile Esprit  --WSS-->
N.I.N.A. PC B / profile Remote  --WSS-->
```

## Protocol direction

The Direct protocol will use versioned envelopes for:

- agent/plugin hello and capability negotiation;
- full snapshots and incremental events;
- image metadata and bounded thumbnails;
- queries and query results;
- commands and command results;
- acknowledgements and heartbeats.

Local communication uses a Windows named pipe named
`spacecat-agent-v1-<node-id>` so installations cannot collide.
Cross-system communication uses the same logical envelopes over an outbound TLS
WebSocket at `/v1/direct`. The initial wire representation is JSON so both
implementations can be inspected and evolved easily.

## Identity and safety

- A rig is identified by a per-installation node ID plus the N.I.N.A. profile
  ID, not by a user-editable telescope name, IP address, or port.
- A node ID is not a credential. Remote connections must pair separately, and
  the resulting credential must be bound to the claimed node ID.
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
3. Define and test Direct identities, registration, and transport-neutral
   envelopes.
4. Add N.I.N.A. native snapshots/events and read-only commands.
5. Add local named-pipe and remote WebSocket transports plus plugin settings.
6. Validate feature parity, including autofocus and guide graphs.
7. Add pairing, node-bound credentials, and relay hardening.
8. Package the plugin DLL and optional Windows runtime into a checksummed
   N.I.N.A. plugin ZIP.
