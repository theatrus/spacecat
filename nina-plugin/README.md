# Chatstronomy N.I.N.A. plugin

The N.I.N.A. plugin lives in the Chatstronomy repository so the C# adapter, Rust
hub, Direct protocol, compatibility tests, and release artifacts can be
versioned together. N.I.N.A.'s community plugin manifest can point to a ZIP
artifact produced by this repository; it does not require the plugin source to
live in a separate repository.

The plugin supports two independent configuration axes. The source mode decides
how observatory data reaches Chatstronomy:

- **Direct** — replace the Advanced API hooks Chatstronomy consumes with native
  snapshots, bounded histories, and approved commands built from N.I.N.A.
  mediators. This preserves source capability parity without hosting an HTTP API.
- **Advanced API** — supervise/configure the Rust agent while it continues to
  use the separately installed Advanced API plugin. The N.I.N.A. plugin is not
  required for existing headless Advanced API deployments. A plugin-owned local
  runtime receives the API endpoint and polling interval as an explicit source
  configuration.

Both source modes feed the same source-neutral Rust runtime. The delivery mode
decides where chat is owned:

- **Discord webhook:** the local Chatstronomy runtime sends simple notifications
  through a user-owned Discord webhook.
- **Discord app / bot:** the local runtime owns a user-created Discord bot and can
  provide interactive commands as well as notifications.
- **Matrix:** the local runtime owns a Matrix account without requiring Discord.
- **Chatstronomy.com:** the plugin connects outbound to the hosted service using
  an opaque credential reference produced by the hosted sign-in/pairing flow.

Matrix can run by itself or alongside either local Discord choice. The plugin
captures the HTTPS homeserver URL, username, password, and default room ID,
allowing the same local runtime to publish to Discord and Matrix together.

Webhook URLs, Discord bot tokens, and Matrix passwords are stored per N.I.N.A.
profile in Windows Credential Manager, not in profile JSON. Hosted credentials
are similarly not serialized by this configuration layer: it stores only the
reference that the hosted credential flow resolves.

In Direct mode, several N.I.N.A. instances can feed one Chatstronomy bot even when
they run on different computers. Local plugins use a named pipe; remote plugins
open an outbound authenticated WebSocket to the central hub. Each rig is keyed
by a persistent per-installation node ID plus N.I.N.A.'s profile GUID, so no
Advanced API port or inbound listener is required on the imaging computers.

Direct connection mode remains explicit:

- **Local (default):** no endpoint or pairing setup; the plugin connects to the
  bundled on-machine Chatstronomy runtime and the user owns the local chat keys.
- **Remote:** the plugin uses its hosted credential and opens an outbound
  connection to Chatstronomy.com. The hosted service owns chat credentials.

Remote mode never silently falls back to Local, preventing duplicate
notifications or an unexpected second bot.

## Local runtime handoff

When **Start Chatstronomy with N.I.N.A.** is enabled, the plugin launches the
bundled rig-only Windows runtime and transfers the validated source and delivery
configuration over a random current-user-only named pipe. The process command
line contains only the pipe name and a non-secret log path; webhook URLs, bot
tokens, and Matrix passwords are never placed in arguments or a temporary JSON
file. The same pipe carries graceful shutdown, while the opt-out setting allows
the runtime to detach and continue independently.

The supervised runtime supports both source payloads. `advanced_api_polling`
carries a URL and 1–300 second interval. `nina_direct` carries a random
current-user-only data-pipe name plus negotiated capabilities and requires no
Advanced API URL. Direct answers the same logical `RigSource` operations from
mediator snapshots and plugin-maintained history; it does not expose a duplicate
HTTP API.

Local Direct mode always starts and stops the bundled runtime with N.I.N.A.
because the native data pipe belongs to the loaded plugin session. Advanced API
mode may instead point at a separately managed Chatstronomy process.

## Build

The initial project targets N.I.N.A. 3.2 and .NET 8:

```powershell
dotnet build nina-plugin/Chatstronomy.NINA/Chatstronomy.NINA.csproj
dotnet run --project nina-plugin/Chatstronomy.NINA.Tests/Chatstronomy.NINA.Tests.csproj
```

## Package and registry manifest

The package script builds the plugin and the rig-only Rust runtime, places the
runtime under `runtime/chatstronomy.exe`, and generates a N.I.N.A.-compatible beta
manifest with the archive's SHA-256 checksum:

```powershell
./nina-plugin/build-package.ps1 -Version 0.1.0.0
```

For a local registry test, override the installer URL with the address that
serves the generated archive:

```powershell
./nina-plugin/build-package.ps1 `
  -Version 0.1.0.0 `
  -InstallerUrl http://127.0.0.1:8765/packages/Chatstronomy.NINA.0.1.0.0.zip `
  -FeaturedImageUrl http://127.0.0.1:8765/images/chatstronomy-featured.png
```

The generated manifest can be added alongside existing entries in a N.I.N.A.
registry; it does not replace or require changes to other plugins. Tagged
`nina-vX.Y.Z.B` builds publish the archive and manifest as a separate beta
GitHub release from the main Chatstronomy agent releases.

The plugin manifest uses the 512 px derivative
`assets/branding/chatstronomy-featured.png`; the plugin assembly repeats that
image URL as `FeaturedImageURL` metadata so N.I.N.A. also shows it after
installation. The N.I.N.A. catalog, installed-plugin view, main Windows binary,
Start menu shortcut, and Add/Remove Programs entry all share the same logo
artwork. Windows surfaces use the multi-resolution
`assets/branding/chatstronomy.ico` derivative.

The project exports the N.I.N.A. plugin manifest, options UI, validated
source/delivery configuration, Windows-protected local secret storage, supervised
process controller, secure bootstrap and Direct data pipes, bounded native event,
image, thumbnail, and guider histories, live equipment snapshots, the bundled
runtime, and the shared multi-system identity handshake. Sequence normalization,
native autofocus result details and command execution, the plugin's remote
WebSocket client, and hosted credential acquisition remain follow-up stages.
