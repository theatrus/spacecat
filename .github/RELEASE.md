# Chatstronomy Release Process

## Creating a Release

1. Update version in `Cargo.toml`
2. Update `CLAUDE.md` with any new features or changes
3. Commit changes: `git commit -m "Bump version to vX.Y.Z"`
4. Create and push tag: `git tag vX.Y.Z && git push origin vX.Y.Z`
5. GitHub Actions will automatically build and create the release

## Release Artifacts

Each release includes:

- **chatstronomy-linux-x86_64**: Linux binary for x86_64 systems
- **chatstronomy-linux-aarch64**: Linux binary for ARM64 systems (Raspberry Pi 4, etc.)
- **chatstronomy-windows-x64.exe**: Full Windows binary for 64-bit systems
- **chatstronomy-plugin-runtime-windows-x64.exe**: Lean Windows runtime bundled by the N.I.N.A. plugin
- **chatstronomy-plugin-contracts-v1.zip**: Versioned Direct protocol schema and fixtures
- **chatstronomy-runtime-manifest.json**: Release identity, protocol versions, asset names, sizes, and SHA-256 hashes

The N.I.N.A. plugin repository pins an exact Chatstronomy release and manifest
checksum. It never downloads `latest` or builds the Rust runtime from a backend
branch.

## Installation

### Linux
```bash
# Download the appropriate binary for your architecture
curl -L -o chatstronomy https://github.com/USERNAME/chatstronomy/releases/latest/download/chatstronomy-linux-x86_64
chmod +x chatstronomy
sudo mv chatstronomy /usr/local/bin/
```

### Windows
1. Download `chatstronomy-windows-x64.exe`
2. Rename to `chatstronomy.exe`
3. Add to your PATH or run from the download directory

## Usage

```bash
# Show help
chatstronomy --help

# Create sample config
cp config.example.json config.json
# Edit config.json with your API settings

# Run basic commands
chatstronomy sequence
chatstronomy events
chatstronomy discord-updater
```

See [CLAUDE.md](../CLAUDE.md) for detailed documentation.
