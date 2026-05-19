//! Service wrapper abstraction for running SpaceCat as CLI or background service

use crate::api::SpaceCatApiClient;
use crate::chat::{ChatConfig, ChatServiceManager, DiscordChatService, MatrixChatService};
use crate::chat_updater::ChatUpdater;
use crate::config::{Config, TelescopeConfig};
use crate::error::{ChatError, ServiceError, ServiceResult, SpaceCatError};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

pub struct ServiceWrapper {
    config: Config,
}

impl ServiceWrapper {
    pub fn new(config: Config) -> ServiceResult<Self> {
        Ok(Self { config })
    }

    /// Run chat updaters for all configured telescopes concurrently. The
    /// shared `ChatServiceManager` (one Matrix login, one Discord client) is
    /// built once and shared by reference across every telescope task.
    pub async fn run_cli(&self, interval: u64) -> ServiceResult<()> {
        if self.config.telescopes.is_empty() {
            return Err(ServiceError::Initialization {
                reason: "No telescopes configured.".to_string(),
            });
        }

        let chat_manager = Arc::new(
            build_shared_chat_manager(&self.config.chat)
                .await
                .map_err(|e| ServiceError::Initialization {
                    reason: e.to_string(),
                })?,
        );

        let poll_interval = Duration::from_secs(interval);
        let mut handles = Vec::new();
        for telescope in &self.config.telescopes {
            let telescope = telescope.clone();
            let chat_manager = chat_manager.clone();
            let handle = tokio::spawn(async move {
                match build_chat_updater(telescope.clone(), chat_manager).await {
                    Ok(mut updater) => {
                        updater.start_polling(poll_interval).await;
                    }
                    Err(e) => eprintln!(
                        "[{}] Failed to create chat updater: {e}",
                        telescope.name
                    ),
                }
            });
            handles.push(handle);
        }

        for h in handles {
            let _ = h.await;
        }
        Ok(())
    }

    /// Get the configuration for inspection
    pub fn config(&self) -> &Config {
        &self.config
    }
}

/// Build the process-wide chat manager from the shared chat block. Matrix
/// logs in once here, regardless of how many telescopes are configured.
pub async fn build_shared_chat_manager(
    chat: &ChatConfig,
) -> Result<ChatServiceManager, SpaceCatError> {
    let mut manager = ChatServiceManager::new();

    if let Some(discord) = &chat.discord
        && discord.enabled
    {
        println!(
            "Initializing shared Discord chat service (default webhook: {})...",
            if discord.default_webhook_url.is_some() {
                "configured"
            } else {
                "none — telescopes must override"
            }
        );
        manager.add_service(Box::new(DiscordChatService::new(
            discord.default_webhook_url.clone(),
        )));
    }

    if let Some(matrix) = &chat.matrix
        && matrix.enabled
    {
        println!("Initializing shared Matrix chat service (one login process-wide)...");
        let service = MatrixChatService::new(
            &matrix.homeserver_url,
            &matrix.username,
            &matrix.password,
            matrix.default_room_id.as_deref(),
        )
        .await
        .map_err(|e| {
            SpaceCatError::Chat(ChatError::Initialization {
                service_name: "Matrix".to_string(),
                reason: e.to_string(),
            })
        })?;
        manager.add_service(Box::new(service));
    }

    if manager.service_count() == 0 {
        println!("Warning: No chat services configured. Running in monitoring-only mode.");
    }

    Ok(manager)
}

/// Construct a `ChatUpdater` for one telescope, wired to the shared manager.
pub async fn build_chat_updater(
    telescope: TelescopeConfig,
    chat_manager: Arc<ChatServiceManager>,
) -> Result<ChatUpdater, SpaceCatError> {
    let client = SpaceCatApiClient::new(telescope.api.clone()).map_err(SpaceCatError::Api)?;
    let target = telescope.chat.to_chat_target();
    Ok(
        ChatUpdater::new(client, telescope.name.clone(), target, chat_manager)
            .with_image_cooldown(telescope.image_cooldown_seconds),
    )
}

// Windows service specific implementation
#[cfg(windows)]
mod windows_service_impl {
    use super::*;
    use tokio::time::sleep;

    impl ServiceWrapper {
        /// Run chat updaters for all telescopes as a Windows service with
        /// graceful shutdown. One shared chat manager is constructed inside
        /// the runtime; each telescope poll loop runs as its own task.
        pub fn run_with_shutdown(&self, shutdown_rx: mpsc::Receiver<()>) -> ServiceResult<()> {
            let rt = tokio::runtime::Runtime::new().map_err(|e| ServiceError::Initialization {
                reason: format!("Failed to create Tokio runtime: {}", e),
            })?;

            let telescopes = self.config.telescopes.clone();
            let chat_config = self.config.chat.clone();
            let poll_interval = Duration::from_secs(5);

            rt.block_on(async move {
                let chat_manager = Arc::new(
                    build_shared_chat_manager(&chat_config)
                        .await
                        .map_err(|e| ServiceError::Initialization {
                            reason: format!("Failed to build chat manager: {}", e),
                        })?,
                );

                let shutdown = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                {
                    let shutdown = shutdown.clone();
                    std::thread::spawn(move || {
                        if shutdown_rx.recv().is_ok() {
                            shutdown.store(true, std::sync::atomic::Ordering::SeqCst);
                        }
                    });
                }

                let mut handles = Vec::new();
                for telescope in telescopes {
                    let shutdown = shutdown.clone();
                    let chat_manager = chat_manager.clone();
                    let handle = tokio::spawn(async move {
                        let mut updater =
                            match build_chat_updater(telescope.clone(), chat_manager).await {
                                Ok(u) => u,
                                Err(e) => {
                                    eprintln!(
                                        "[{}] Failed to initialize: {e}",
                                        telescope.name
                                    );
                                    return;
                                }
                            };
                        if let Err(e) = updater.initialize_baseline().await {
                            eprintln!("[{}] Baseline failed: {e}", telescope.name);
                            return;
                        }
                        println!(
                            "[{}] Windows service polling every {:?}",
                            telescope.name, poll_interval
                        );
                        while !shutdown.load(std::sync::atomic::Ordering::SeqCst) {
                            updater.poll_sequence().await;
                            updater.poll_events().await;
                            updater.poll_images().await;
                            sleep(poll_interval).await;
                        }
                        println!("[{}] Shutdown signal received.", telescope.name);
                    });
                    handles.push(handle);
                }

                for h in handles {
                    let _ = h.await;
                }
                println!("Windows service stopped");
                Ok(())
            })
        }
    }
}

// Stub implementation for non-Windows platforms
#[cfg(not(windows))]
impl ServiceWrapper {
    /// Stub implementation for non-Windows service shutdown
    pub fn run_with_shutdown(&self, _shutdown_rx: mpsc::Receiver<()>) -> ServiceResult<()> {
        Err(ServiceError::Runtime {
            reason: "Windows service support is not available on this platform".to_string(),
        })
    }
}
