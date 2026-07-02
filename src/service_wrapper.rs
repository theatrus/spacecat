//! Service wrapper abstraction for running SpaceCat as CLI or background service

use crate::api::SpaceCatApiClient;
use crate::chat::{ChatServiceManager, DiscordChatService, MatrixChatService, run_bot};
use crate::chat_updater::ChatUpdater;
use crate::config::{Config, TelescopeConfig};
use crate::error::{ChatError, ServiceError, ServiceResult, SpaceCatError};
use std::collections::HashMap;
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
    /// shared `ChatServiceManager` (one Matrix login, one Discord client,
    /// one Discord bot) is built once and shared by reference across every
    /// telescope task.
    pub async fn run_cli(&self, interval: u64) -> ServiceResult<()> {
        if self.config.telescopes.is_empty() {
            return Err(ServiceError::Initialization {
                reason: "No telescopes configured.".to_string(),
            });
        }

        let (chat_manager, _bot_join) =
            build_shared_chat_manager(&self.config).await.map_err(|e| {
                ServiceError::Initialization {
                    reason: e.to_string(),
                }
            })?;
        let chat_manager = Arc::new(chat_manager);

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
                    Err(e) => eprintln!("[{}] Failed to create chat updater: {e}", telescope.name),
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

/// Build the process-wide chat manager. Matrix logs in once, the Discord
/// bot connects once, regardless of how many telescopes are configured.
///
/// Returns `(manager, bot_gateway_join)`. The bot gateway join handle keeps
/// the bot task alive for the life of the process; drop it to detach.
pub async fn build_shared_chat_manager(
    config: &Config,
) -> Result<(ChatServiceManager, Option<tokio::task::JoinHandle<()>>), SpaceCatError> {
    let chat = &config.chat;
    let mut manager = ChatServiceManager::new();
    let mut bot_join: Option<tokio::task::JoinHandle<()>> = None;

    if let Some(discord) = &chat.discord
        && discord.enabled
    {
        println!(
            "Initializing shared Discord webhook service (default webhook: {})...",
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

    if let Some(bot_config) = &chat.discord_bot
        && bot_config.enabled
    {
        println!(
            "Initializing shared Discord bot (state file: {})...",
            bot_config.state_file
        );

        // Build the per-telescope API client map and the channel routing
        // table the bot needs for slash commands. Warn (don't error) if a
        // telescope has both a channel_id and a webhook_url — the channel
        // takes precedence and the webhook is ignored.
        let mut api_clients = HashMap::new();
        let mut channel_to_telescope = HashMap::new();
        for telescope in &config.telescopes {
            let client =
                SpaceCatApiClient::new(telescope.api.clone()).map_err(SpaceCatError::Api)?;
            api_clients.insert(telescope.name.clone(), client);
            if let Some(channel_id) = telescope.chat.discord_channel_id {
                channel_to_telescope.insert(channel_id, telescope.name.clone());
                if telescope.chat.discord_webhook_url.is_some() {
                    eprintln!(
                        "[{}] Warning: both discord_channel_id and discord_webhook_url \
                         configured; webhook will be ignored (bot takes precedence).",
                        telescope.name
                    );
                }
            }
        }

        let (service, join) = run_bot(bot_config, api_clients, channel_to_telescope)
            .await
            .map_err(SpaceCatError::Chat)?;
        manager.add_service(Box::new(service));
        bot_join = Some(join);
    }

    if manager.service_count() == 0 {
        println!("Warning: No chat services configured. Running in monitoring-only mode.");
    }

    Ok((manager, bot_join))
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
            .with_image_cooldown(telescope.image_cooldown_seconds)
            .with_reconnect_backoff(
                telescope.reconnect.initial_seconds,
                telescope.reconnect.max_seconds,
            ),
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

            let config = self.config.clone();
            let telescopes = self.config.telescopes.clone();
            let poll_interval = Duration::from_secs(5);

            rt.block_on(async move {
                let (manager, _bot_join) =
                    build_shared_chat_manager(&config).await.map_err(|e| {
                        ServiceError::Initialization {
                            reason: format!("Failed to build chat manager: {}", e),
                        }
                    })?;
                let chat_manager = Arc::new(manager);

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
                                    eprintln!("[{}] Failed to initialize: {e}", telescope.name);
                                    return;
                                }
                            };
                        // Don't give up permanently if the rig is offline at
                        // startup — retry the baseline with exponential backoff
                        // until it comes back, honoring the shutdown signal.
                        let mut delay = updater.reconnect_initial();
                        loop {
                            if shutdown.load(std::sync::atomic::Ordering::SeqCst) {
                                return;
                            }
                            // Map the (non-Send) error to a String so no
                            // `Box<dyn Error>` is held across the await below.
                            match updater
                                .initialize_baseline()
                                .await
                                .map_err(|e| e.to_string())
                            {
                                Ok(()) => break,
                                Err(msg) => {
                                    eprintln!(
                                        "[{}] Baseline failed: {msg}; retrying in {delay:?}",
                                        telescope.name
                                    );
                                    sleep(delay).await;
                                    delay = updater.next_reconnect_delay(delay);
                                }
                            }
                        }
                        println!(
                            "[{}] Windows service polling every {:?}",
                            telescope.name, poll_interval
                        );
                        // Same reachability-gated backoff as the CLI path: the
                        // pollers report whether the API answered, so a mid-run
                        // drop backs off instead of hammering every endpoint.
                        let mut reconnect_delay = updater.reconnect_initial();
                        while !shutdown.load(std::sync::atomic::Ordering::SeqCst) {
                            let seq_ok = updater.poll_sequence().await;
                            let events_ok = updater.poll_events().await;
                            let images_ok = updater.poll_images().await;
                            let reachable = seq_ok || events_ok || images_ok;
                            updater.record_reachability(reachable).await;
                            if reachable {
                                reconnect_delay = updater.reconnect_initial();
                                sleep(poll_interval).await;
                            } else {
                                sleep(reconnect_delay).await;
                                reconnect_delay = updater.next_reconnect_delay(reconnect_delay);
                            }
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
