# SpaceCat N.I.N.A. plugin

The N.I.N.A. plugin lives in the SpaceCat repository so the C# adapter, Rust
hub, Direct protocol, compatibility tests, and release artifacts can be
versioned together. N.I.N.A.'s community plugin manifest can point to a ZIP
artifact produced by this repository; it does not require the plugin source to
live in a separate repository.

The plugin will support two source modes:

- **Direct** — collect snapshots and events from N.I.N.A. mediators and execute
  approved commands through those mediators.
- **Advanced API** — supervise/configure the Rust agent while it continues to
  use the separately installed Advanced API plugin. The N.I.N.A. plugin is not
  required for existing headless Advanced API deployments.

Both source modes feed the same source-neutral Rust runtime and can use either
a locally owned Discord/Matrix bot or a central SpaceCat service.

In Direct mode, several N.I.N.A. instances can feed one SpaceCat bot even when
they run on different computers. Local plugins use a named pipe; remote plugins
open an outbound authenticated WebSocket to the central hub. Each rig is keyed
by a persistent per-installation node ID plus N.I.N.A.'s profile GUID, so no
Advanced API port or inbound listener is required on the imaging computers.

Connection mode is explicit:

- **Local (default):** no endpoint or pairing setup; the plugin connects to the
  bundled on-machine SpaceCat runtime and the user owns the local chat keys.
- **Remote:** the user supplies a `wss://` hub URL and pairs the node; Discord
  and Matrix keys remain only on the central SpaceCat hub.

Remote mode never silently falls back to Local, preventing duplicate
notifications or an unexpected second bot.

## Build

The initial project targets N.I.N.A. 3.2 and .NET 8:

```powershell
dotnet build nina-plugin/SpaceCat.NINA/SpaceCat.NINA.csproj
```

The project currently exports the N.I.N.A. plugin manifest and the shared
multi-system identity handshake. Native event collection, named-pipe/WebSocket
transports, pairing, and the options UI are the next implementation stages.
