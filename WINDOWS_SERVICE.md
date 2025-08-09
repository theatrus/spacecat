# SpaceCat Windows Service

SpaceCat can run as a Windows service for automated, production deployments. This allows SpaceCat to start automatically with Windows and run in the background without requiring a user to be logged in.

## Features

- **Automatic startup**: Service starts automatically when Windows boots
- **Background execution**: Runs without requiring user login
- **Windows Event Log integration**: Logs to Windows Event Log
- **Service management**: Standard Windows service controls (start/stop/restart)
- **Secure execution**: Runs under dedicated service account
- **Configuration management**: Uses system-wide configuration location

## Prerequisites

- Windows 10/11 or Windows Server 2016+
- SpaceCat Windows build with `windows-service` feature enabled
- Administrator privileges for installation/uninstallation

## Installation

### Step 1: Download SpaceCat

Download the Windows release from the [GitHub releases page](https://github.com/theatrus/spacecat/releases/latest):

```powershell
# Download using PowerShell
Invoke-WebRequest -Uri "https://github.com/theatrus/spacecat/releases/latest/download/spacecat-windows-x64.exe" -OutFile "spacecat.exe"
```

### Step 2: Install as Service

Run PowerShell or Command Prompt **as Administrator**:

```powershell
# Install the service
.\spacecat.exe windows-service install
```

This will:
- Register SpaceCat as a Windows service named "SpaceCat"
- Set it to start automatically on system boot
- Create the service configuration directory

### Step 3: Configure the Service

Edit the service configuration file:

```powershell
# Open configuration in notepad
notepad "C:\ProgramData\SpaceCat\config.json"
```

Example configuration:

```json
{
  "api": {
    "base_url": "http://192.168.1.100:1888",
    "timeout_seconds": 30,
    "retry_attempts": 3
  },
  "logging": {
    "level": "info"
  },
  "discord": {
    "enabled": true,
    "webhook_url": "https://discord.com/api/webhooks/YOUR_WEBHOOK_URL",
    "image_cooldown_seconds": 60
  }
}
```

### Step 4: Start the Service

```powershell
# Start the service
.\spacecat.exe windows-service start

# Or use Windows Services management
services.msc
```

## Service Management

### Command Line Management

```powershell
# Check service status
.\spacecat.exe windows-service status

# Start the service
.\spacecat.exe windows-service start

# Stop the service
.\spacecat.exe windows-service stop

# Uninstall the service
.\spacecat.exe windows-service uninstall
```

### Windows Services Management

1. Open **Services** (`services.msc`)
2. Find "SpaceCat" in the list
3. Right-click to Start, Stop, or configure properties

### Service Configuration

- **Service Name**: SpaceCat
- **Display Name**: SpaceCat Discord Updater
- **Start Type**: Automatic
- **Log On As**: Local System (default)

## Configuration

### Configuration Location

The service reads configuration from:
```
C:\ProgramData\SpaceCat\config.json
```

This location is automatically created when the service is installed.

### Configuration Changes

After modifying the configuration:

```powershell
# Restart the service to apply changes
.\spacecat.exe windows-service stop
.\spacecat.exe windows-service start
```

## Logging

### Windows Event Log

The service logs to the Windows Event Log:

1. Open **Event Viewer** (`eventvwr.msc`)
2. Navigate to **Windows Logs** > **Application**
3. Look for events from source "SpaceCat"

### Viewing Logs

```powershell
# View recent service events
Get-EventLog -LogName Application -Source "SpaceCat" -Newest 20
```

## Troubleshooting

### Service Won't Start

1. **Check configuration file exists**:
   ```powershell
   Test-Path "C:\ProgramData\SpaceCat\config.json"
   ```

2. **Validate configuration**:
   ```powershell
   # Test configuration by running manually
   .\spacecat.exe discord-updater --interval 5
   ```

3. **Check Windows Event Log** for error messages

### Permission Issues

- Ensure the service is installed as Administrator
- Verify configuration file is readable by Local System account
- Check network access permissions

### Network Connectivity

- Test API connectivity from command line
- Verify Windows Firewall isn't blocking connections
- Check if antivirus is interfering

### Manual Testing

Run SpaceCat manually to test configuration:

```powershell
# Test outside of service
.\spacecat.exe discord-updater --interval 5
```

## Uninstallation

### Remove Service

```powershell
# Stop the service first
.\spacecat.exe windows-service stop

# Uninstall the service
.\spacecat.exe windows-service uninstall
```

### Clean Up Files

```powershell
# Remove configuration (optional)
Remove-Item -Recurse "C:\ProgramData\SpaceCat"

# Remove executable
Remove-Item "spacecat.exe"
```

## Security Considerations

### Service Account

By default, the service runs as **Local System**, which has broad system privileges. For enhanced security, consider:

1. Creating a dedicated service account
2. Granting minimal required permissions
3. Configuring the service to use the custom account

### Network Security

- Configure Windows Firewall appropriately
- Use HTTPS endpoints when possible
- Secure Discord webhook URLs

### Configuration Security

- Protect the configuration file from unauthorized access
- Use secure storage for sensitive information
- Regularly rotate Discord webhook URLs if needed

## Advanced Configuration

### Custom Service Account

To run with a custom account:

1. Create a service account
2. Grant "Log on as a service" privilege
3. Update service properties in Services management console

### Startup Dependencies

The service automatically waits for network availability, but you can configure additional dependencies through the Services management console.

### Recovery Options

Configure automatic recovery in Services management console:
- First failure: Restart the Service
- Second failure: Restart the Service  
- Subsequent failures: Take No Action

## Support

For issues with Windows service functionality:

1. Check the [GitHub Issues](https://github.com/theatrus/spacecat/issues)
2. Review Windows Event Log for specific errors
3. Test manual execution before reporting service-specific issues
4. Include relevant Event Log entries when reporting issues