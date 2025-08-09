# SpaceCat Release Process

## Creating a Release

1. Update version in `Cargo.toml`
2. Update `CLAUDE.md` with any new features or changes
3. Commit changes: `git commit -m "Bump version to vX.Y.Z"`
4. Create and push tag: `git tag vX.Y.Z && git push origin vX.Y.Z`
5. GitHub Actions will automatically build and create the release

## Release Artifacts

Each release includes:

- **spacecat-linux-x86_64**: Linux binary for x86_64 systems
- **spacecat-linux-aarch64**: Linux binary for ARM64 systems (Raspberry Pi 4, etc.)
- **spacecat-windows-x64.exe**: Windows binary for 64-bit systems

## Installation

### Linux
```bash
# Download the appropriate binary for your architecture
curl -L -o spacecat https://github.com/USERNAME/spacecat/releases/latest/download/spacecat-linux-x86_64
chmod +x spacecat
sudo mv spacecat /usr/local/bin/
```

### Windows
1. Download `spacecat-windows-x64.exe`
2. Rename to `spacecat.exe`
3. Add to your PATH or run from the download directory

## Usage

```bash
# Show help
spacecat --help

# Create sample config
cp config.example.json config.json
# Edit config.json with your API settings

# Run basic commands
spacecat sequence
spacecat events
spacecat dual-poll
```

See [CLAUDE.md](../CLAUDE.md) for detailed documentation.