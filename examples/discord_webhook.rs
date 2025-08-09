use spacecat::discord::{DiscordWebhook, Embed, WebhookMessage, colors};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Example webhook URL - replace with your actual webhook URL
    let webhook_url = "https://discord.com/api/webhooks/YOUR_WEBHOOK_ID/YOUR_WEBHOOK_TOKEN";
    
    // Create webhook client
    let webhook = DiscordWebhook::new(webhook_url.to_string())?;
    
    // Example 1: Simple text message
    println!("Sending simple message...");
    webhook.execute_simple("Hello from SpaceCat! 🔭").await?;
    
    // Example 2: Message with custom username and avatar
    println!("Sending custom message...");
    let custom_message = WebhookMessage {
        content: Some("SpaceCat is monitoring the observatory!".to_string()),
        username: Some("SpaceCat Observatory".to_string()),
        avatar_url: Some("https://example.com/spacecat-avatar.png".to_string()),
        tts: Some(false),
        embeds: None,
        allowed_mentions: None,
        components: None,
        files: None,
        payload_json: None,
        attachments: None,
        flags: None,
    };
    webhook.execute(&custom_message).await?;
    
    // Example 3: Rich embed message
    println!("Sending embed message...");
    let embed = Embed::new()
        .title("Observatory Status Report")
        .description("Latest observations from SpaceCat")
        .color(colors::BLUE)
        .field("Camera", "ZWO ASI2600MM Duo", true)
        .field("Filter", "Hydrogen Alpha (HA)", true)
        .field("Temperature", "-10°C", true)
        .field("Exposure", "300s", true)
        .field("Stars Detected", "142", true)
        .field("HFR", "2.35", true)
        .footer("SpaceCat Observatory", None)
        .timestamp(&chrono::Utc::now().to_rfc3339());
    
    webhook.execute_with_embed(Some("New image captured!"), embed).await?;
    
    // Example 4: Alert embed
    println!("Sending alert embed...");
    let alert_embed = Embed::new()
        .title("⚠️ Weather Alert")
        .description("Cloud cover detected - pausing observations")
        .color(colors::ORANGE)
        .field("Cloud Cover", "85%", false)
        .field("Action Taken", "Sequence paused, mount parked", false)
        .timestamp(&chrono::Utc::now().to_rfc3339());
    
    webhook.execute_with_embed(None, alert_embed).await?;
    
    // Example 5: Success notification
    println!("Sending success notification...");
    let success_embed = Embed::new()
        .title("✅ Sequence Complete")
        .description("Successfully captured all planned images")
        .color(colors::GREEN)
        .field("Total Images", "50", true)
        .field("Duration", "4h 32m", true)
        .field("Filters Used", "L, R, G, B, HA, OIII, SII", false)
        .author("SpaceCat", None, None)
        .timestamp(&chrono::Utc::now().to_rfc3339());
    
    webhook.execute_with_embed(Some("Great news!"), success_embed).await?;
    
    println!("All messages sent successfully!");
    Ok(())
}