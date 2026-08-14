# SpaceCat N.I.N.A. plugin

The N.I.N.A. plugin lives in the SpaceCat repository so the C# adapter, Rust
sidecar, IPC protocol, compatibility tests, and release artifacts can be
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
a locally owned Discord/Matrix bot or the hosted SpaceCat service.

## Build

The initial project targets N.I.N.A. 3.2 and .NET 8:

```powershell
dotnet build nina-plugin/SpaceCat.NINA/SpaceCat.NINA.csproj
```

The project currently exports only the N.I.N.A. plugin manifest. Native event
collection, named-pipe transport, options UI, and sidecar packaging will be
implemented after the Rust source boundary is merged.
