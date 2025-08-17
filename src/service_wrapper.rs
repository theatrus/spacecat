//! Service wrapper abstraction for running SpaceCat as CLI or background service

use crate::config::Config;
use std::sync::mpsc;

pub struct ServiceWrapper {
    config: Config,
}

impl ServiceWrapper {
    pub fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self { config })
    }

    /// Run the chat updater as a regular CLI application
    pub async fn run_cli(&self, _interval: u64) -> Result<(), Box<dyn std::error::Error>> {
        // TODO: Temporarily disabled until service wrapper is refactored for new chat architecture
        Err("Service wrapper needs to be updated for new chat architecture. Use the 'chat-updater' command directly instead.".into())
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
        /// Run the discord updater as a Windows service with shutdown support
        pub fn run_with_shutdown(
            &self,
            shutdown_rx: mpsc::Receiver<()>,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            // Create a Tokio runtime for the service
            let rt = tokio::runtime::Runtime::new()?;

            rt.block_on(async {
                // TODO: Temporarily disabled until service wrapper is refactored for new chat architecture
                Err("Windows service needs to be updated for new chat architecture".into())
            })
        }

        /// Main service loop that can be gracefully shutdown
        async fn run_service_loop(
            &self,
            _poller: (), // TODO: Update when refactoring
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
            _poller: &mut (), // TODO: Update when refactoring
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            // This is a simplified version of the initialization
            // The actual implementation would need to access private methods of DiscordUpdater
            // For now, we'll let the first poll cycle establish the baseline
            Ok(())
        }

        async fn poll_once(
            &self,
            _poller: &mut (), // TODO: Update when refactoring
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            // This would need to call the polling logic from DiscordUpdater
            // For now, this is a placeholder - we would need to refactor DiscordUpdater
            // to expose individual polling methods
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
