# Discord Webhook Integration

SpaceCat includes Discord webhook support for real-time notifications of astronomical events and captured images.

## Features

- **Real-time Event Notifications**: Get notified when equipment connects/disconnects, sequences start/stop, filters change, etc.
- **Image Capture Alerts**: Receive detailed information when new images are captured
- **Thumbnail Attachments**: Automatically downloads and includes actual thumbnail images
- **Rich Embeds**: Beautiful formatted messages with color coding and detailed fields
- **Async Operation**: Non-blocking webhook calls that won't interrupt your observations

## Setup

1. **Create a Discord Webhook**:
   - Go to your Discord server settings
   - Navigate to Integrations → Webhooks
   - Click "New Webhook"
   - Copy the webhook URL

2. **Configure SpaceCat**:
   Add the webhook configuration to your `config.json` file:
   ```json
   {
     "api": {
       "base_url": "http://192.168.0.82:1888",
       "timeout_seconds": 30,
       "retry_attempts": 3
     },
     "logging": {
       "level": "info",
       "enable_file_logging": false,
       "log_file": "spacecat.log"
     },
     "discord": {
       "webhook_url": "https://discord.com/api/webhooks/YOUR_WEBHOOK_ID/YOUR_WEBHOOK_TOKEN",
       "enabled": true
     }
   }
   ```
   
   Or copy the example configuration:
   ```bash
   cp config.example.json config.json
   # Edit config.json with your webhook URL
   ```

3. **Run with Discord Integration**:
   ```bash
   cargo run -- discord-updater
   ```
   
   To temporarily disable Discord notifications without removing the webhook URL:
   ```json
   "discord": {
     "webhook_url": "https://discord.com/api/webhooks/...",
     "enabled": false
   }
   ```

## Message Types

### Event Notifications

Events are color-coded based on type:
- 🟢 **Green**: Image saves, successful operations
- 🔵 **Blue**: Filter wheel changes
- 🟦 **Cyan**: Sequence starts
- 🟧 **Orange**: Sequence stops, warnings
- 🟡 **Yellow**: Mount parking
- 🔴 **Red**: Errors
- ⚫ **Gray**: Other events

### Image Notifications

Images are color-coded by type:
- 🟢 **Green**: LIGHT frames (science images)
- ⚫ **Gray**: DARK frames
- 🔵 **Blue**: FLAT frames
- 🟣 **Purple**: BIAS frames
- 🟦 **Cyan**: Other frame types

Each image notification includes:
- Camera name and temperature
- Filter used
- Exposure time
- Number of stars detected and HFR (Half Flux Radius)
- Image statistics (mean, median, standard deviation)
- Telescope information
- **Thumbnail attachment**: Automatically downloads and attaches the actual thumbnail image from the SpaceCat API

## API Usage

### Simple Message
```rust
use spacecat::discord::DiscordWebhook;

let webhook = DiscordWebhook::new(webhook_url)?;
webhook.execute_simple("Observatory is online! 🔭").await?;
```

### Rich Embed
```rust
use spacecat::discord::{DiscordWebhook, Embed, colors};

let embed = Embed::new()
    .title("Observation Complete")
    .description("Successfully captured M31 - Andromeda Galaxy")
    .color(colors::GREEN)
    .field("Total Images", "120", true)
    .field("Total Time", "6 hours", true)
    .field("Filters", "L, R, G, B, Ha", false)
    .footer("SpaceCat Observatory", None)
    .timestamp(&chrono::Utc::now().to_rfc3339());

webhook.execute_with_embed(None, embed).await?;
```

### Custom Integration
```rust
use spacecat::discord::DiscordWebhook;
use spacecat::dual_poller::DualPoller;

let mut poller = DualPoller::new(client);
poller = poller.with_discord_webhook(&webhook_url)?;
```

## Configuration Options

The Discord configuration in `config.json` supports:

- `webhook_url`: The Discord webhook URL for notifications
- `enabled`: Boolean to enable/disable Discord notifications (default: true)

## Examples

See `examples/discord_webhook.rs` for complete examples of:
- Simple text messages
- Custom usernames and avatars
- Rich embeds with fields
- Alert notifications
- Success confirmations

## Error Handling

The Discord integration is designed to be non-blocking. If webhook calls fail:
- Errors are logged to stderr
- The main polling loop continues uninterrupted
- No observations are affected

Common errors:
- Invalid webhook URL format
- Network connectivity issues
- Discord API rate limits
- Invalid message content

## Best Practices

1. **Rate Limiting**: Discord webhooks have rate limits. SpaceCat automatically handles this gracefully.
2. **Message Size**: Keep embed descriptions under 2048 characters and field values under 1024 characters.
3. **Testing**: Test your webhook URL with the example script before using in production.
4. **Privacy**: Never commit `config.json` with real webhook URLs to version control. Use `config.example.json` as a template.
5. **Configuration Management**: 
   - Keep your `config.json` in `.gitignore`
   - Use `config.example.json` for sharing configuration templates
   - Set `enabled: false` to temporarily disable notifications without removing the webhook URL