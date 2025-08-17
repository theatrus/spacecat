# SpaceCat 🔭

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust](https://img.shields.io/badge/rust-1.89+-orange.svg)](https://www.rust-lang.org)
[![Build Status](https://github.com/theatrus/spacecat/workflows/CI/badge.svg)](https://github.com/theatrus/spacecat/actions)

**SpaceCat** is a Rust-based tool for posting events to multiple chat services (Discord, Matrix) from a [NINA](https://nighttime-imaging.eu)
installation, specifically using the [Advanced API](https://github.com/christian-photo/ninaAPI) extension.

## On Vibe Coding

This code base was a test of extensively vibe-coding this integration. It was
fast to iterate on, but has a lot of misc code smells. Not the worst result
really.

## 🚀 Quick Start

### Installation

#### Option 1: RPM Package (Recommended for Fedora/RHEL/CentOS)

```bash
# Download the latest release
wget https://github.com/theatrus/spacecat/releases/latest/download/spacecat-*.rpm

# Install the package
sudo dnf install ./spacecat-*.rpm

# Configure the system
sudo vim /etc/spacecat/config.json

# Enable and start the service
sudo systemctl enable --now spacecat.service

# Check status
sudo systemctl status spacecat.service
```

#### Option 2: Build from Source

```bash
# Clone the repository
git clone https://github.com/theatrus/spacecat.git
cd spacecat

# Build the project (Windows service support included automatically on Windows)
cargo build --release

# Copy the binary to your PATH
sudo cp target/release/spacecat /usr/local/bin/

# Create configuration
cp packaging/config/spacecat.conf ~/.config/spacecat/config.json
```

### Configuration

Create a `config.json` file with your NINA Advanced API settings. `base_url` is
the location of the NINA Advanced API. Use `http://localhost:1888` if you are
running it on the same system.

```json
{
  "api": {
    "base_url": "http://192.168.0.82:1888",
    "timeout_seconds": 30,
    "retry_attempts": 3
  },
  "logging": {
    "level": "info"
  },
  "chat": {
    "discord": {
      "enabled": true,
      "webhook_url": "https://discord.com/api/webhooks/YOUR_WEBHOOK_URL"
    },
    "matrix": {
      "homeserver_url": "https://matrix.example.com",
      "username": "@spacecat:matrix.example.com",
      "password": "your_matrix_password",
      "room_id": "!roomid:matrix.example.com",
      "enabled": false
    }
  },
  "image_cooldown_seconds": 60
}
```

## 📖 Usage

### Command Line Interface

SpaceCat provides a comprehensive CLI with multiple commands:

```bash
# Show help
spacecat --help

# Get current sequence information
spacecat sequence

# View event history
spacecat events

# Show last 10 events with details
spacecat last-events --count 10

# Get image history and statistics
spacecat images

# Download a specific image
spacecat get-image 0 --params "autoPrepare=true"

# Get image thumbnail
spacecat get-thumbnail 5 --output "image_5.jpg" --image-type "LIGHT"

# Poll for new events (5 cycles, 2 second intervals)
spacecat poll --interval 2 --count 5

# Start continuous chat updates (recommended)
spacecat chat-updater --interval 5

# Get latest autofocus results
spacecat last-autofocus

# Check mount information
spacecat mount-info
```

### Service Mode

#### Linux (systemd)

For production use on Linux, run SpaceCat as a systemd service:

```bash
# Check service logs
journalctl -u spacecat.service -f

# Restart after configuration changes
sudo systemctl restart spacecat.service

# Monitor service status
sudo systemctl status spacecat.service
```

#### Windows Service

For production use on Windows, SpaceCat can run as a Windows service:

```powershell
# Install the service (run as Administrator)
spacecat.exe windows-service install

# Configure the service
# Edit C:\ProgramData\SpaceCat\config.json

# Start the service
spacecat.exe windows-service start

# Check service status
spacecat.exe windows-service status

# Stop the service
spacecat.exe windows-service stop

# Uninstall the service
spacecat.exe windows-service uninstall
```

**Note**: Windows service functionality is automatically available when running on Windows.

## ✨ Features

- **Multi-Platform Chat**: Send notifications to Discord and/or Matrix simultaneously
- **Smart Target Tracking**: Automatically detects actual observation targets from TS-TARGETSTART events
- **Real-time Monitoring**: Live updates for equipment events, image captures, and autofocus sessions
- **Rich Notifications**: Color-coded embeds with detailed telescope status and metadata
- **Auto-Configuration**: Matrix bot auto-joins room invitations and shows startup status
- **Image Sharing**: Automatic thumbnail downloads and sharing with configurable cooldowns
- **Windows Service**: Production-ready background service support on Windows
- **Comprehensive CLI**: Full command-line interface for all functionality

## 🤖 Chat Services

### Discord Setup

1. Create a Discord webhook in your server:
   - Go to Server Settings → Integrations → Webhooks
   - Create a new webhook and copy the URL
   - Add the URL to your `config.json`

### Matrix Setup

1. Create a Matrix account and room:
   - Register an account on your Matrix homeserver
   - Create or join a room for SpaceCat notifications
   - Get the room ID (starts with `!`)

2. Configure Matrix in `config.json`:
   ```json
   {
     "chat": {
       "matrix": {
         "homeserver_url": "https://matrix.org",
         "username": "@spacecat:matrix.org", 
         "password": "your_password",
         "room_id": "!your_room_id:matrix.org",
         "enabled": true
       }
     }
   }
   ```

3. SpaceCat will automatically:
   - Log into Matrix with provided credentials
   - Join any pending room invitations
   - List all joined rooms on startup
   - Send formatted messages with telescope data

## 🏗️ Development

### Prerequisites

- **Rust 1.89+**: Install from [rustup.rs](https://rustup.rs/)
- **Git**: For version control
- **OpenSSL development headers**: For HTTPS support

```bash
# Fedora/RHEL/CentOS
sudo dnf install rust cargo openssl-devel git

# Ubuntu/Debian  
sudo apt install cargo rustc libssl-dev git pkg-config

# macOS
brew install rust openssl git
```

### Building

```bash
# Clone and build
git clone https://github.com/theatrus/spacecat.git
cd spacecat

# Development build
cargo build

# Release build
cargo build --release

# Run tests
cargo test

# Check code quality
cargo clippy --all-targets --all-features -- -D warnings

# Format code
cargo fmt
```


## 🎛️ Configuration Reference

### API Configuration

```json
{
  "api": {
    "base_url": "http://192.168.0.82:1888",    // SpaceCat API base URL
    "timeout_seconds": 30,                      // HTTP request timeout
    "retry_attempts": 3                         // Number of retry attempts
  }
}
```

### Chat Service Integration

```json
{
  "chat": {
    "discord": {
      "enabled": true,                          // Enable Discord notifications
      "webhook_url": "https://discord.com/..." // Discord webhook URL
    },
    "matrix": {
      "homeserver_url": "https://matrix.org",   // Matrix homeserver URL
      "username": "@bot:matrix.org",            // Matrix username
      "password": "password",                   // Matrix password
      "room_id": "!room:matrix.org",            // Target room ID
      "enabled": false                          // Enable Matrix notifications
    }
  },
  "image_cooldown_seconds": 60                 // Cooldown between image posts
}
```

### Logging

```json
{
  "logging": {
    "level": "info"                            // Log level: error, warn, info, debug, trace
  }
}
```

## 🔌 API Integration

SpaceCat integrates with the NINA Advanced API through the following endpoints:

- **`/v2/api/version`** - API health check and version info
- **`/v2/api/event-history`** - Equipment event monitoring  
- **`/v2/api/image-history?all=true`** - Image metadata and session statistics
- **`/v2/api/sequence`** - Current sequence information
- **`/v2/api/last-autofocus`** - Latest autofocus results
- **`/v2/api/mount-info`** - Mount position and status
- **`/image/{index}`** - Individual image download
- **`/image/thumbnail/{index}`** - Image thumbnail download


## 🤝 Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

### Development Workflow

1. **Fork** the repository
2. **Clone** your fork: `git clone https://github.com/YOUR-USERNAME/spacecat.git`
3. **Create** a feature branch: `git checkout -b feature/amazing-feature`
4. **Make** your changes and add tests
5. **Test** your changes: `cargo test && cargo clippy`
6. **Commit** your changes: `git commit -m 'Add amazing feature'`
7. **Push** to your branch: `git push origin feature/amazing-feature`
8. **Create** a Pull Request

### Code Style

- Follow Rust standard formatting: `cargo fmt`
- Ensure clippy passes: `cargo clippy --all-targets --all-features -- -D warnings`
- Add tests for new functionality
- Update documentation as needed

## 📄 License

This project is licensed under the **Apache License 2.0** - see the [LICENSE](LICENSE) file for details.


---

**Made with ❤️ for the astronomy community**

*SpaceCat helps astronomers automate and monitor their observations, bringing the universe closer to everyone.*
