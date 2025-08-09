//! Service wrapper abstraction for running SpaceCat as CLI or background service

use crate::api::SpaceCatApiClient;
use crate::config::Config;
use crate::discord_updater::DiscordUpdater;
use std::sync::mpsc;
use std::time::Duration;

pub struct ServiceWrapper {
    config: Config,
}

impl ServiceWrapper {
    pub fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self { config })
    }

    /// Run the discord updater as a regular CLI application
    pub async fn run_cli(&self, interval: u64) -> Result<(), Box<dyn std::error::Error>> {
        println!("Starting SpaceCat Discord Updater...");
        println!("Press Ctrl+C to stop\n");

        let client = SpaceCatApiClient::new(self.config.api.clone())?;
        let mut poller = DiscordUpdater::new(client);

        // Check for Discord webhook configuration
        if let Some(discord_config) = &self.config.discord {
            if discord_config.enabled && !discord_config.webhook_url.is_empty() {
                println!("Discord webhook configured, events will be sent to Discord");
                let cooldown = discord_config.image_cooldown_seconds;

                poller = poller
                    .with_discord_webhook(&discord_config.webhook_url)?
                    .with_discord_image_cooldown(cooldown);

                println!("Image cooldown set to {} seconds", cooldown);
            } else {
                println!("Discord webhook disabled or not configured");
            }
        } else {
            println!("Discord configuration not found");
        }

        let poll_interval = Duration::from_secs(interval);
        poller.start_polling(poll_interval).await;

        Ok(())
    }

    /// Get the configuration for inspection
    pub fn config(&self) -> &Config {
        &self.config
    }
}

// Windows service specific implementation
#[cfg(all(windows, feature = "windows-service"))]
mod windows_service_impl {
    use super::*;
    use tokio::time::sleep;

    impl ServiceWrapper {
        /// Run the discord updater as a Windows service with shutdown support
        pub fn run_with_shutdown(
            &self,
            shutdown_rx: mpsc::Receiver<()>,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            // Create a Tokio runtime for the service
            let rt = tokio::runtime::Runtime::new()?;

            rt.block_on(async {
                let client = SpaceCatApiClient::new(self.config.api.clone())
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

                let mut poller = DiscordUpdater::new(client);

                // Configure Discord if available
                if let Some(discord_config) = &self.config.discord {
                    if discord_config.enabled && !discord_config.webhook_url.is_empty() {
                        let cooldown = discord_config.image_cooldown_seconds;

                        poller = poller
                            .with_discord_webhook(&discord_config.webhook_url)
                            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
                            .with_discord_image_cooldown(cooldown);
                    }
                }

                // Run the service with shutdown monitoring
                let poll_interval = Duration::from_secs(5); // Default 5 second interval for service
                self.run_service_loop(poller, poll_interval, shutdown_rx)
                    .await
            })
        }

        /// Main service loop that can be gracefully shutdown
        async fn run_service_loop(
            &self,
            mut poller: DiscordUpdater,
            poll_interval: Duration,
            shutdown_rx: mpsc::Receiver<()>,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            // Initialize baseline
            if let Err(e) = self.initialize_baseline(&mut poller).await {
                return Err(format!("Failed to initialize baseline: {}", e).into());
            }

            loop {
                // Check for shutdown signal (non-blocking)
                if shutdown_rx.try_recv().is_ok() {
                    break;
                }

                // Poll for events and images
                if let Err(e) = self.poll_once(&mut poller).await {
                    // Log error but continue running
                    eprintln!("Polling error: {}", e);
                }

                // Sleep for the specified interval
                sleep(poll_interval).await;
            }

            Ok(())
        }

        async fn initialize_baseline(
            &self,
            _poller: &mut DiscordUpdater,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            // This is a simplified version of the initialization
            // The actual implementation would need to access private methods of DiscordUpdater
            // For now, we'll let the first poll cycle establish the baseline
            Ok(())
        }

        async fn poll_once(
            &self,
            _poller: &mut DiscordUpdater,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            // This would need to call the polling logic from DiscordUpdater
            // For now, this is a placeholder - we would need to refactor DiscordUpdater
            // to expose individual polling methods
            Ok(())
        }
    }
}

// Stub implementation for non-Windows platforms
#[cfg(not(all(windows, feature = "windows-service")))]
impl ServiceWrapper {
    /// Stub implementation for non-Windows service shutdown
    pub fn run_with_shutdown(
        &self,
        _shutdown_rx: mpsc::Receiver<()>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Err("Windows service support is not available on this platform".into())
    }
}
