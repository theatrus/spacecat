//! Service wrapper abstraction for running SpaceCat as CLI or background service

use crate::api::SpaceCatApiClient;
use crate::chat::{ChatServiceManager, DiscordChatService, MatrixChatService};
use crate::chat_updater::ChatUpdater;
use crate::config::Config;
use std::sync::mpsc;
use std::time::Duration;

pub struct ServiceWrapper {
    config: Config,
}

impl ServiceWrapper {
    pub fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self { config })
    }

    /// Run the chat updater as a regular CLI application
    pub async fn run_cli(&self, interval: u64) -> Result<(), Box<dyn std::error::Error>> {
        // Create API client
        let client = SpaceCatApiClient::new(self.config.api.clone())?;

        // Create chat service manager
        let mut chat_manager = ChatServiceManager::new();

        // Add Discord service if configured
        if let Some(discord_config) = &self.config.discord
            && discord_config.enabled
        {
            let discord_service = DiscordChatService::new(&discord_config.webhook_url)?;
            chat_manager.add_service(Box::new(discord_service));
        }

        // Add chat services from new config structure
        if let Some(discord_config) = &self.config.chat.discord
            && discord_config.enabled
        {
            let discord_service = DiscordChatService::new(&discord_config.webhook_url)?;
            chat_manager.add_service(Box::new(discord_service));
        }

        if let Some(matrix_config) = &self.config.chat.matrix
            && matrix_config.enabled
        {
            let matrix_service = MatrixChatService::new(
                &matrix_config.homeserver_url,
                &matrix_config.username,
                &matrix_config.password,
                &matrix_config.room_id,
            )
            .await?;
            chat_manager.add_service(Box::new(matrix_service));
        }

        // Create chat updater
        let mut chat_updater = ChatUpdater::new(client)
            .with_chat_manager(chat_manager)
            .with_image_cooldown(self.config.image_cooldown_seconds);

        // Start polling
        chat_updater
            .start_polling(Duration::from_secs(interval))
            .await;
        Ok(())
    }

    /// Get the configuration for inspection
    pub fn config(&self) -> &Config {
        &self.config
    }
}

// Windows service specific implementation
#[cfg(windows)]
mod windows_service_impl {
    use super::*;
    use tokio::time::sleep;

    impl ServiceWrapper {
        /// Run the chat updater as a Windows service with shutdown support
        pub fn run_with_shutdown(
            &self,
            shutdown_rx: mpsc::Receiver<()>,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            // Create a Tokio runtime for the service
            let rt = tokio::runtime::Runtime::new()?;

            rt.block_on(async {
                // Create API client
                let client = SpaceCatApiClient::new(self.config.api.clone())
                    .map_err(|e| format!("Failed to create API client: {}", e))?;

                // Create chat service manager
                let mut chat_manager = ChatServiceManager::new();

                // Add Discord service if configured (legacy config)
                if let Some(discord_config) = &self.config.discord
                    && discord_config.enabled
                {
                    let discord_service = DiscordChatService::new(&discord_config.webhook_url)
                        .map_err(|e| format!("Failed to create Discord service: {}", e))?;
                    chat_manager.add_service(Box::new(discord_service));
                }

                // Add chat services from new config structure
                if let Some(discord_config) = &self.config.chat.discord
                    && discord_config.enabled
                {
                    let discord_service = DiscordChatService::new(&discord_config.webhook_url)
                        .map_err(|e| format!("Failed to create Discord service: {}", e))?;
                    chat_manager.add_service(Box::new(discord_service));
                }

                if let Some(matrix_config) = &self.config.chat.matrix
                    && matrix_config.enabled
                {
                    let matrix_service = MatrixChatService::new(
                        &matrix_config.homeserver_url,
                        &matrix_config.username,
                        &matrix_config.password,
                        &matrix_config.room_id,
                    )
                    .await
                    .map_err(|e| format!("Failed to create Matrix service: {}", e))?;
                    chat_manager.add_service(Box::new(matrix_service));
                }

                // Create chat updater
                let mut chat_updater = ChatUpdater::new(client)
                    .with_chat_manager(chat_manager)
                    .with_image_cooldown(self.config.image_cooldown_seconds);

                // Run the service loop with graceful shutdown
                self.run_service_loop(chat_updater, Duration::from_secs(5), shutdown_rx)
                    .await
            })
        }

        /// Main service loop that can be gracefully shutdown
        async fn run_service_loop(
            &self,
            mut chat_updater: ChatUpdater,
            poll_interval: Duration,
            shutdown_rx: mpsc::Receiver<()>,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            // Initialize baseline
            if let Err(e) = chat_updater.initialize_baseline().await {
                return Err(format!("Failed to initialize baseline: {}", e).into());
            }

            println!(
                "Windows service started - polling every {:?}",
                poll_interval
            );

            loop {
                // Check for shutdown signal (non-blocking)
                if shutdown_rx.try_recv().is_ok() {
                    println!("Shutdown signal received, stopping service...");
                    break;
                }

                // Poll for events, sequence, and images
                chat_updater.poll_sequence().await;
                chat_updater.poll_events().await;
                chat_updater.poll_images().await;

                // Sleep for the specified interval
                sleep(poll_interval).await;
            }

            println!("Windows service stopped");
            Ok(())
        }
    }
}

// Stub implementation for non-Windows platforms
#[cfg(not(windows))]
impl ServiceWrapper {
    /// Stub implementation for non-Windows service shutdown
    pub fn run_with_shutdown(
        &self,
        _shutdown_rx: mpsc::Receiver<()>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Err("Windows service support is not available on this platform".into())
    }
}
