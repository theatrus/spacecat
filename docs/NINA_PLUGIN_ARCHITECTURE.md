# N.I.N.A. integration architecture

Chatstronomy treats **how observatory data is collected** and **where chat services
run** as independent choices. Direct mode is an additional integration, not a
replacement for Advanced API mode.

## Repository and release boundary

The Rust backend is the source of truth for the hub, bots, renderers, local
runtime, Direct protocol, schemas, and golden fixtures. Each backend release
publishes two Windows executables:

- `chatstronomy-windows-x64.exe`, the full hub-capable runtime used by
  compatibility probes and normal Windows installations;
- `chatstronomy-plugin-runtime-windows-x64.exe`, the lean
  `--no-default-features` runtime bundled by the N.I.N.A. plugin.

It also publishes `chatstronomy-runtime-manifest.json`, whose protocol versions,
asset names, sizes, and SHA-256 hashes are derived from those exact binaries.
The separately released N.I.N.A. plugin pins one backend release and manifest
checksum in `runtime.lock.json`. Plugin builds never use a backend branch,
resolve `latest`, or invoke Cargo as a fallback. This lets the C# plugin and Rust
service version independently without weakening reproducibility.

Functionally, Direct is a native in-process replacement for every Advanced API
hook Chatstronomy consumes. It preserves the same source capabilities and
normalized response shapes so the Rust updater, renderers, and chat commands do
not care which provider is active. It does **not** reproduce the Advanced API's
HTTP server or require the C# plugin to parse HTTP route names.

## Supported combinations

| Source | Local delivery | Central/hosted delivery |
|---|---|---|
| Advanced API | Rust agent polls N.I.N.A. and runs user-owned Discord/Matrix clients | Rust agent polls N.I.N.A. and relays outbound to the hosted service |
| N.I.N.A. Direct | N.I.N.A. plugin connects to Chatstronomy over a Windows named pipe | N.I.N.A. plugin opens an outbound authenticated WebSocket to the Chatstronomy hub |

Advanced API deployments remain usable without installing the Chatstronomy N.I.N.A.
plugin. When the plugin is installed in Advanced API mode, it acts as an
in-application configuration and health surface for the Rust agent rather than
becoming a second data source.

## Chat delivery configuration

Chat delivery is selected separately from the source mode. The N.I.N.A. options
surface provides four mutually exclusive choices:

| Delivery choice | Runtime owner | Credential location | Capability |
|---|---|---|---|
| Discord webhook | Local computer | Windows Credential Manager | Outbound notifications |
| Discord app / bot | Local computer | Windows Credential Manager | Notifications and Discord commands |
| Matrix | Local computer | Windows Credential Manager | Matrix notifications and commands |
| Chatstronomy.com | Hosted service | Windows Credential Manager (rig credential) | Hosted chat delivery and remote routing |

Matrix can be selected as the only local chat service or enabled alongside either
local Discord choice. Its homeserver must use HTTPS. The Matrix password uses the
same profile-scoped Windows Credential Manager storage as the Discord secrets.
One local runtime can therefore deliver to Discord and Matrix simultaneously.

Local delivery configuration includes the runtime executable path, whether the
plugin should start it with N.I.N.A., and whether a plugin-owned process should
stop with N.I.N.A. Advanced API polling also carries its explicit endpoint and a
bounded polling interval; these source settings are not mixed into Discord or
Matrix delivery. Native Direct uses the same runtime controller but replaces the
HTTP polling source with the stable node-scoped Direct pipe.

For a plugin-owned process, the controller creates a random, current-user-only
bootstrap pipe before launching `chatstronomy plugin-runtime`. The command line
contains only that non-secret pipe name and a log path. The validated source and
delivery payload, including credentials, crosses the pipe once and is converted
directly to the Rust runtime's in-memory configuration. The pipe then remains open
for graceful shutdown. No credential-bearing command argument or temporary JSON
file is created. If the user elects to leave the runtime running after N.I.N.A.
exits, the process redirects output to its profile log and detaches from the
control pipe safely.

The hosted options accept an HTTPS service origin (or its WSS Direct endpoint)
and a one-time pairing code. Pairing codes and durable rig credentials live in
Windows Credential Manager and are scoped to the N.I.N.A. profile plus hub
origin; neither is serialized into the delivery configuration or profile JSON.

## Components

### `RigSource` (Rust)

The source-neutral interface consumed by polling, state tracking, chat commands,
and image delivery. `AdvancedApiSource` is the first implementation and delegates
to the existing `ChatstronomyApiClient`.

`DirectPipeRigSource` satisfies native operations from snapshots and
request/response messages carried over the plugin-owned data pipe. Each source
advertises capabilities so callers skip unsupported operations and report an
unsupported write clearly.

The parity boundary is the `RigSource` operation set, not the Advanced API wire
format:

| Chatstronomy operation | Native N.I.N.A. implementation |
|---|---|
| Event history | Subscribe to application/device events and keep a bounded journal |
| Image history and thumbnails | Observe saved images and keep a bounded metadata/thumbnail cache |
| Sequence | Read the active sequence through N.I.N.A. sequence services |
| Last autofocus | Capture autofocus completion/results in a bounded cache |
| Mount, filter wheel, guider, rotator, focuser | Build live snapshots from the corresponding mediators |
| Guider graph | Accumulate recent guide-step events in a bounded ring buffer |
| Commands | Execute typed, explicitly allowed operations through N.I.N.A. mediators |

Advanced API mode obtains these values by polling HTTP endpoints. Direct mode
answers the same logical reads from native snapshots and plugin-maintained
history, while allowing incremental events to be pushed as the protocol grows.
Any refresh cadence used by the Rust state reducer is independent of the HTTP
transport and must not require an Advanced API URL in Direct mode.

The protocol carries typed command variants such as `park_mount`, `cool_camera`,
and `start_sequence`. Advanced API sources translate those semantic operations
to legacy routes internally; route strings never cross into the N.I.N.A. plugin.
Native execution uses the same closed set of thirteen operations through the
corresponding N.I.N.A. mediators; arbitrary route names or reflection-based
command invocation never cross the Direct boundary.

Guider and autofocus graph payloads are rendered by the shared Rust chart layer,
then delivered as PNG attachments by Discord webhooks, the Discord bot, and
Matrix. Discord's `/guider`, `/focus`, and `/last-image` inspection tools also
surface those graphics on demand. Direct graph contract tests cover N.I.N.A.'s
raw-pixel RMS semantics, arcsecond display scaling, signed correction pulses,
dither markers, and an actual named-pipe-to-PNG round trip.

Only one source is authoritative for a rig. Chatstronomy will not automatically
merge or fail over between Direct and Advanced API sources because duplicate
events and ambiguous command results are more dangerous than a visible outage.

### Chatstronomy N.I.N.A. plugin (C#)

The plugin is intentionally limited to the native source boundary. It will:

1. Subscribe to N.I.N.A. device, image-save, autofocus, guider, and sequence
   mediators.
2. Maintain the bounded histories needed to provide parity with the Advanced API
   hooks Chatstronomy currently reads.
3. Convert native information into versioned Chatstronomy messages and normalized
   response payloads.
4. Execute an explicit allowlist of typed commands through N.I.N.A. mediators.
5. Provide source/delivery settings and connection health inside N.I.N.A.
6. Connect to Chatstronomy over a Windows named pipe when local or an outbound TLS
   WebSocket when Chatstronomy runs on another system.

It does not contain Discord, Matrix, notification formatting, chart rendering,
OAuth, tenant routing, or other hosted application logic. Its hosted role is the
authenticated Direct transport and secure local credential persistence.

### Chatstronomy runtime and hub (Rust)

One Chatstronomy process owns state reduction, durable event delivery, chart
rendering, and the Discord/Matrix clients. It accepts any number of Direct
plugin connections and gives every connected profile an independent rig source.

When Chatstronomy runs on the N.I.N.A. computer, the plugin uses a per-user named
pipe. When Chatstronomy runs centrally, each plugin connects outward to its TLS
WebSocket endpoint. The remote N.I.N.A. computers do not run Discord or Matrix
bots and do not need an inbound listener or Advanced API.

The Windows Chatstronomy rig-only executable is included under `runtime/` in the
plugin ZIP for the all-in-one local option. It excludes the hosted web stack but
includes Advanced API polling, Discord webhook/bot, Matrix, and the secure plugin
runtime command. It remains possible to install and run the Rust application
independently for Advanced API and non-Windows deployments.

### Hosted Chatstronomy service

The hosted service owns the central chat credentials, tenant routing, and
connected-plugin registry. Read requests use a freshness-stamped cached
snapshot or make a bounded round trip to the relevant plugin. Write commands
are never queued for an offline rig.

## Direct connection modes

Direct mode has two explicit connection choices. Chatstronomy does not probe and
guess between them, and Remote never silently falls back to Local.

### Local (default)

Local is the simple all-in-one path for a single imaging computer:

1. Install the Chatstronomy N.I.N.A. plugin.
2. Leave **Connection mode** set to **Local**.
3. Select Discord webhook, Discord app / bot, or Matrix-only delivery. Matrix can
   also be enabled alongside either Discord choice.
4. Leave **N.I.N.A. data source** set to **Direct** for the simple setup. In
   Advanced API compatibility mode, leave the endpoint at
   `http://127.0.0.1:1888/` or enter the configured API address and choose the
   polling interval.
5. The plugin starts the bundled Chatstronomy runtime, transfers the in-memory
   configuration over its bootstrap pipe, and supervises the owned process.

Native Direct local mode requires no URL, pairing code, open port, or Advanced API
installation because the plugin implements Chatstronomy's required source hooks
directly from N.I.N.A. The initial working compatibility path continues to poll
the separately installed Advanced API plugin. Chat credentials remain on that
computer in Windows Credential Manager rather than normal profile JSON.
The local Direct runtime is necessarily supervised for the lifetime of the
N.I.N.A. plugin session; only Advanced API mode can detach and continue after
N.I.N.A. exits.

### Remote

Remote connects one or more imaging computers to a central Chatstronomy hub:

1. Select **Chatstronomy.com — hosted bot**.
2. Enter the hub's HTTPS origin or `wss://` Direct URL and a one-time pairing
   code issued by its telescope page.
3. Choose **Pair / reconnect**. The plugin exchanges the code, clears it after
   success, stores the resulting node-bound credential in Windows Credential
   Manager, and opens an outbound WebSocket to the hub.

The central hub owns its configured chat credentials and bot connections (the
current hosted implementation is Discord; local mode continues to support
Matrix). The imaging computers do not run a local bot, accept inbound
connections, or expose Advanced API. Losing the remote connection produces a
visible offline state; it does not start a second local bot.

## Multiple N.I.N.A. instances

Every plugin sends a versioned `client_hello` containing three identifiers:

- `node_id`: generated once and stored under the Windows user's local Chatstronomy
  data directory; all N.I.N.A. processes in that installation share it;
- `profile_id`: the stable GUID exposed by `IProfileService.ActiveProfile.Id`;
- `session_id`: generated each time the plugin loads and used to distinguish a
  reconnect from a second process.

The stable rig key is `(node_id, profile_id)`. This supports both multiple
profiles on one computer and profiles on entirely different computers. A
profile name is display metadata and can change without changing routing.

One telescope may have only one active session. A newer authenticated session
atomically replaces the older transport. If N.I.N.A. switches profiles, the
plugin closes the old session, resets native histories, loads the new profile's
origin-scoped credential, and connects with a fresh hello. Disconnects are
retried with bounded exponential backoff; fatal authentication errors stop and
remain visible in N.I.N.A. until the user repairs or forgets the credential.

For example, all of these feed one bot process:

```text
N.I.N.A. PC A / profile C925    --WSS-->  Chatstronomy hub  --> Discord + Matrix
N.I.N.A. PC A / profile Esprit  --WSS-->
N.I.N.A. PC B / profile Remote  --WSS-->
```

## Protocol direction

The Direct protocol uses versioned envelopes for:

- agent/plugin hello and capability negotiation;
- full snapshots and incremental events;
- image metadata and bounded thumbnails;
- queries and query results;
- commands and command results;
- acknowledgements and heartbeats.

The supervised local runtime uses a random current-user-only data pipe whose name
is transferred through the separate secure bootstrap pipe. The node-scoped
`chatstronomy-agent-v1-<node-id>` name remains reserved for independently managed
local agent discovery.
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
- Remote control is disabled by default in the hub policy. The N.I.N.A. plugin
  independently enforces the closed typed-command allowlist and query expiry.
- Commands carry correlation IDs and expiry times. Disconnected or expired
  commands fail rather than executing later.
- Only thumbnails and selected metadata leave the observatory by default.

## Delivery sequence

1. Preserve current behavior behind `AdvancedApiSource`.
2. Remove concrete API-client dependencies from chat polling and commands.
3. Define and test Direct identities, registration, and transport-neutral
   envelopes.
4. Complete sequence, autofocus-result, native command, and chart-rendering
   parity for the current Chatstronomy `RigSource` surface.
5. Extend the working local named-pipe transport with the remote plugin WebSocket
   client and hosted settings. *(complete)*
6. Validate full feature parity against supported N.I.N.A. versions and real
   hardware/profile combinations.
7. Add pairing, node-bound credentials, command expiry, heartbeat, and reconnect
   hardening. *(complete)*
8. Package the plugin DLL and optional Windows runtime into a checksummed
   N.I.N.A. plugin ZIP.
