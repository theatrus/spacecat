# SpaceCat RPM Package

This directory contains packaging files for creating RPM packages compatible with Fedora, RHEL, and CentOS.

## Quick Start

### Prerequisites

On Fedora/RHEL/CentOS, install the required build tools:

```bash
# Fedora
sudo dnf install rpm-build rpm-devel rust cargo systemd-rpm-macros

# RHEL/CentOS (with EPEL enabled)
sudo yum install rpm-build rpm-devel rust cargo systemd-rpm-macros
```

### Building the RPM

```bash
# From the project root directory
./packaging/build-rpm.sh
```

### Installing the RPM

```bash
# Install the generated RPM
sudo dnf install ~/rpmbuild/RPMS/x86_64/spacecat-*.rpm

# Or with dnf local install
sudo dnf localinstall ~/rpmbuild/RPMS/x86_64/spacecat-*.rpm
```

## Service Management

### Configuration

Edit the configuration file:
```bash
sudo vim /etc/spacecat/config.json
```

Enable Discord notifications by setting:
```json
{
  "discord": {
    "enabled": true,
    "webhook_url": "YOUR_DISCORD_WEBHOOK_URL",
    "image_cooldown_seconds": 60
  }
}
```

### Service Control

```bash
# Enable and start the service
sudo systemctl enable --now spacecat.service

# Check service status
sudo systemctl status spacecat.service

# View logs
journalctl -u spacecat.service -f

# Restart the service (e.g., after config changes)
sudo systemctl restart spacecat.service

# Stop the service
sudo systemctl stop spacecat.service

# Disable the service
sudo systemctl disable spacecat.service
```

## Package Details

### Installed Files

- **Binary**: `/usr/bin/spacecat`
- **Configuration**: `/etc/spacecat/config.json`
- **Service Unit**: `/usr/lib/systemd/system/spacecat.service`
- **Log Directory**: `/var/log/spacecat/` (owned by spacecat user)

### User Account

The RPM creates a system user `spacecat:spacecat` with:
- Home directory: `/var/lib/spacecat`
- Shell: `/sbin/nologin`
- Purpose: Running the SpaceCat service securely

### Security Features

The systemd service includes security hardening:
- Runs as non-root user (`spacecat`)
- Private temporary directories
- Protected system directories
- Memory execution protection
- Restricted system calls
- Resource limits

### Service Configuration

The service runs `spacecat dual-poll --interval 5` with:
- **Automatic restart** on failure
- **Journal logging** with identifier `spacecat`
- **Network dependency** (waits for network to be online)
- **Configuration dependency** (requires config.json to exist)

## Troubleshooting

### Service Won't Start

1. Check if config file exists and is valid:
   ```bash
   sudo cat /etc/spacecat/config.json
   ```

2. Verify network connectivity to the API:
   ```bash
   curl http://192.168.0.82:1888/v2/api/version
   ```

3. Check service logs:
   ```bash
   journalctl -u spacecat.service -n 50
   ```

### Permission Issues

Ensure the spacecat user has proper permissions:
```bash
sudo chown -R spacecat:spacecat /var/log/spacecat/
sudo chmod 755 /var/log/spacecat/
```

### Configuration Changes

After modifying `/etc/spacecat/config.json`, restart the service:
```bash
sudo systemctl restart spacecat.service
```

## Manual Installation

If you need to install without the RPM:

```bash
# Build the binary
cargo build --release

# Copy binary
sudo cp target/release/spacecat /usr/bin/

# Create user
sudo useradd -r -s /sbin/nologin spacecat

# Create directories
sudo mkdir -p /etc/spacecat /var/log/spacecat
sudo chown spacecat:spacecat /var/log/spacecat

# Copy configuration
sudo cp packaging/config/spacecat.conf /etc/spacecat/config.json

# Install systemd service
sudo cp packaging/systemd/spacecat.service /etc/systemd/system/
sudo systemctl daemon-reload
```

## Development

To modify the packaging:

1. Edit `packaging/rpm/spacecat.spec` for RPM-specific changes
2. Edit `packaging/systemd/spacecat.service` for service configuration
3. Edit `packaging/config/spacecat.conf` for default settings
4. Run `./packaging/build-rpm.sh` to test changes