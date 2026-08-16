//! Chat delivery and updater orchestration for plugin-owned Direct runtimes.

use crate::chat::{
    ChatServiceManager, DiscordChatService, MatrixChatService, StaticRigResolver, run_bot,
};
use crate::chat_updater::ChatUpdater;
use crate::config::{Config, TelescopeConfig};
use crate::error::{ChatError, ChatstronomyError, ServiceError, ServiceResult};
use crate::source::SharedRigSource;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

pub struct ServiceWrapper {
    config: Config,
}

impl ServiceWrapper {
    pub fn new(config: Config) -> ServiceResult<Self> {
        Ok(Self { config })
    }

    /// Run every configured profile with an explicit Direct source. A missing
    /// source is a configuration error; there is no HTTP polling fallback.
    pub async fn run_cli_with_sources(
        &self,
        interval: u64,
        sources: HashMap<String, SharedRigSource>,
    ) -> ServiceResult<()> {
        if self.config.telescopes.is_empty() {
            return Err(ServiceError::Initialization {
                reason: "No telescopes configured.".to_string(),
            });
        }
        for telescope in &self.config.telescopes {
            if !sources.contains_key(&telescope.name) {
                return Err(ServiceError::Initialization {
                    reason: format!("No Direct source supplied for '{}'.", telescope.name),
                });
            }
        }

        let (chat_manager, _bot_join) = build_shared_chat_manager(&self.config, &sources)
            .await
            .map_err(|error| ServiceError::Initialization {
                reason: error.to_string(),
            })?;
        let chat_manager = Arc::new(chat_manager);
        let poll_interval = Duration::from_secs(interval);
        let mut handles = Vec::new();

        for telescope in &self.config.telescopes {
            let telescope = telescope.clone();
            let manager = chat_manager.clone();
            let source = sources
                .get(&telescope.name)
                .expect("source coverage validated")
                .clone();
            handles.push(tokio::spawn(async move {
                let mut updater = build_chat_updater(telescope, manager, source);
                updater.start_polling(poll_interval).await;
            }));
        }
        for handle in handles {
            let _ = handle.await;
        }
        Ok(())
    }
}

async fn build_shared_chat_manager(
    config: &Config,
    sources: &HashMap<String, SharedRigSource>,
) -> Result<(ChatServiceManager, Option<tokio::task::JoinHandle<()>>), ChatstronomyError> {
    let mut manager = ChatServiceManager::new();
    let mut bot_join = None;

    if let Some(discord) = &config.chat.discord
        && discord.enabled
    {
        manager.add_service(Box::new(DiscordChatService::new(
            discord.default_webhook_url.clone(),
        )));
    }

    if let Some(matrix) = &config.chat.matrix
        && matrix.enabled
    {
        let service = MatrixChatService::new(
            &matrix.homeserver_url,
            &matrix.username,
            &matrix.password,
            matrix.default_room_id.as_deref(),
        )
        .await
        .map_err(|error| {
            ChatstronomyError::Chat(ChatError::Initialization {
                service_name: "Matrix".to_string(),
                reason: error.to_string(),
            })
        })?;
        manager.add_service(Box::new(service));
    }

    if let Some(bot) = &config.chat.discord_bot
        && bot.enabled
    {
        let mut channel_to_telescope = HashMap::new();
        for telescope in &config.telescopes {
            if let Some(channel_id) = telescope.chat.discord_channel_id {
                channel_to_telescope.insert(channel_id, telescope.name.clone());
                if telescope.chat.discord_webhook_url.is_some() {
                    eprintln!(
                        "[{}] Both a Discord channel and webhook are configured; the bot channel wins.",
                        telescope.name
                    );
                }
            }
        }
        let resolver = Arc::new(StaticRigResolver {
            rig_sources: sources.clone(),
            channel_to_telescope,
            write_acl: bot.write_acl.iter().copied().collect(),
        });
        let (service, join) = run_bot(bot, resolver)
            .await
            .map_err(ChatstronomyError::Chat)?;
        manager.add_service(Box::new(service));
        bot_join = Some(join);
    }

    if manager.service_count() == 0 {
        println!("Warning: no chat services configured; monitoring only.");
    }
    Ok((manager, bot_join))
}

fn build_chat_updater(
    telescope: TelescopeConfig,
    manager: Arc<ChatServiceManager>,
    source: SharedRigSource,
) -> ChatUpdater {
    ChatUpdater::new(
        source,
        telescope.name,
        telescope.chat.to_chat_target(),
        manager,
    )
    .with_image_cooldown(telescope.image_cooldown_seconds)
    .with_reconnect_backoff(
        telescope.reconnect.initial_seconds,
        telescope.reconnect.max_seconds,
    )
}
