//! Process-wide Discord bot using Serenity + Poise.
//!
//! One bot identity (one token) serves every telescope. Each telescope can
//! map to a Discord channel via `TelescopeChatOverrides::discord_channel_id`;
//! slash commands invoked in that channel default to that telescope. Outbound
//! posts (events, image notifications, etc.) flow through this service via
//! `ChatService::send_message` and use the bot's `Arc<Http>` to deliver
//! directly without going through the gateway loop.
//!
//! Read-only slash commands (Phase 1):
//!   /status, /sequence, /target, /mount, /filter, /focus, /guider,
//!   /events, /last-image.

use super::status_state::{StatusMessage, StatusState};
use super::{ChatMessage, ChatService, ChatTarget, DiscordBotConfig};
use crate::api::SpaceCatApiClient;
use crate::error::ChatError;
use async_trait::async_trait;
use poise::serenity_prelude::{self as serenity, CreateAttachment, CreateMessage};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Per-bot state carried by the Poise framework. Each slash command has
/// `ctx.data()` access to this.
pub struct BotData {
    /// One API client per telescope, keyed by telescope name.
    pub api_clients: HashMap<String, SpaceCatApiClient>,
    /// Discord channel ID -> telescope name. Slash commands invoked in a
    /// mapped channel default to that telescope.
    pub channel_to_telescope: HashMap<u64, String>,
    /// Discord user IDs allowed to invoke write commands (Phase 3).
    #[allow(dead_code)]
    pub write_acl: std::collections::HashSet<u64>,
}

impl BotData {
    fn known_names(&self) -> Vec<&str> {
        let mut v: Vec<&str> = self.api_clients.keys().map(|s| s.as_str()).collect();
        v.sort();
        v
    }

    /// Resolve a telescope from an explicit override or the channel a
    /// command was invoked in. Returns a user-facing error string if the
    /// channel isn't mapped and no override was provided.
    fn resolve_client(
        &self,
        override_name: Option<&str>,
        channel_id: u64,
    ) -> Result<(String, SpaceCatApiClient), String> {
        if let Some(name) = override_name {
            return self
                .api_clients
                .get(name)
                .cloned()
                .map(|c| (name.to_string(), c))
                .ok_or_else(|| {
                    format!(
                        "Unknown telescope '{name}'. Known: {:?}",
                        self.known_names()
                    )
                });
        }
        if let Some(name) = self.channel_to_telescope.get(&channel_id) {
            let client = self
                .api_clients
                .get(name)
                .cloned()
                .expect("channel_to_telescope -> api_clients invariant");
            return Ok((name.clone(), client));
        }
        Err(format!(
            "No telescope mapped to this channel. Pass `telescope:<name>`. Known: {:?}",
            self.known_names()
        ))
    }
}

pub type BotError = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, BotData, BotError>;

// ---------- Outbound posting (ChatService impl) ----------

/// Chat service that posts via the Discord bot. Holds the bot's `Arc<Http>`
/// after the gateway task is spawned, plus an optional default channel and
/// the persistent live-status state.
pub struct DiscordBotService {
    http: Arc<serenity::Http>,
    default_channel_id: Option<u64>,
    /// Per-telescope (channel_id, message_id) for the pinned live-status
    /// message. Shared across telescope tasks via Mutex; reads are cheap,
    /// writes happen once per poll cycle per telescope.
    status_state: Arc<Mutex<StatusState>>,
    state_file: PathBuf,
    /// Whether live-status upserts are enabled at all (config-driven).
    live_status: bool,
}

impl DiscordBotService {
    pub fn new(
        http: Arc<serenity::Http>,
        default_channel_id: Option<u64>,
        status_state: Arc<Mutex<StatusState>>,
        state_file: PathBuf,
        live_status: bool,
    ) -> Self {
        Self {
            http,
            default_channel_id,
            status_state,
            state_file,
            live_status,
        }
    }

    fn resolve_channel(&self, target: &ChatTarget) -> Option<serenity::ChannelId> {
        target
            .discord_channel_id
            .or(self.default_channel_id)
            .map(serenity::ChannelId::new)
    }

    fn build_embed(message: &ChatMessage) -> serenity::CreateEmbed {
        let mut embed = serenity::CreateEmbed::new().title(&message.title);
        if let Some(color) = message.color {
            embed = embed.color(color);
        }
        for field in &message.fields {
            embed = embed.field(&field.name, &field.value, field.inline);
        }
        if let Some(footer) = &message.footer {
            embed = embed.footer(serenity::CreateEmbedFooter::new(footer));
        }
        if let Some(ts) = &message.timestamp
            && let Ok(parsed) = serenity::Timestamp::parse(ts)
        {
            embed = embed.timestamp(parsed);
        }
        embed
    }
}

#[async_trait]
impl ChatService for DiscordBotService {
    async fn send_message(
        &self,
        message: &ChatMessage,
        target: &ChatTarget,
    ) -> Result<(), ChatError> {
        let channel = self
            .resolve_channel(target)
            .ok_or_else(|| ChatError::Discord {
                message: "No Discord channel available (no default and no telescope override)"
                    .to_string(),
            })?;
        let payload = CreateMessage::new().embed(Self::build_embed(message));
        channel
            .send_message(&self.http, payload)
            .await
            .map_err(|e| ChatError::Discord {
                message: e.to_string(),
            })?;
        Ok(())
    }

    async fn send_message_with_image(
        &self,
        message: &ChatMessage,
        target: &ChatTarget,
        image_data: &[u8],
        filename: &str,
    ) -> Result<(), ChatError> {
        let channel = self
            .resolve_channel(target)
            .ok_or_else(|| ChatError::Discord {
                message: "No Discord channel available (no default and no telescope override)"
                    .to_string(),
            })?;
        let attachment = CreateAttachment::bytes(image_data.to_vec(), filename);
        let payload = CreateMessage::new()
            .embed(Self::build_embed(message))
            .add_file(attachment);
        channel
            .send_message(&self.http, payload)
            .await
            .map_err(|e| ChatError::Discord {
                message: e.to_string(),
            })?;
        Ok(())
    }

    fn service_name(&self) -> &'static str {
        "Discord (bot)"
    }

    fn can_route(&self, target: &ChatTarget) -> bool {
        target.discord_channel_id.is_some() || self.default_channel_id.is_some()
    }

    fn supports_status_upsert(&self) -> bool {
        self.live_status
    }

    /// Edit-or-post the live status message for this telescope.
    ///
    /// On first call (or when state has no record), posts a fresh message
    /// and remembers `(channel_id, message_id)`. On subsequent calls, edits
    /// the existing message in place. If the previous message was deleted
    /// (404 from Discord), reposts and updates state.
    async fn upsert_status(
        &self,
        telescope: &str,
        target: &ChatTarget,
        message: &ChatMessage,
    ) -> Result<(), ChatError> {
        let channel = self
            .resolve_channel(target)
            .ok_or_else(|| ChatError::Discord {
                message: "No Discord channel available for status upsert".to_string(),
            })?;
        let embed = Self::build_embed(message);

        let existing = {
            let state = self.status_state.lock().await;
            state.get(telescope)
        };

        // Try to edit if we have a known message in the same channel.
        if let Some(known) = existing
            && known.channel_id == channel.get()
        {
            let edit = serenity::EditMessage::new()
                .content("")
                .embed(embed.clone());
            match channel
                .edit_message(&self.http, serenity::MessageId::new(known.message_id), edit)
                .await
            {
                Ok(_) => return Ok(()),
                Err(serenity::Error::Http(serenity::HttpError::UnsuccessfulRequest(err)))
                    if err.status_code == reqwest::StatusCode::NOT_FOUND =>
                {
                    // Message was deleted; fall through and repost.
                    eprintln!(
                        "[{telescope}] status message {} not found — reposting",
                        known.message_id
                    );
                }
                Err(e) => {
                    return Err(ChatError::Discord {
                        message: format!("edit_message failed: {e}"),
                    });
                }
            }
        }

        // Post new message and record its ID.
        let payload = CreateMessage::new().embed(embed);
        let posted = channel
            .send_message(&self.http, payload)
            .await
            .map_err(|e| ChatError::Discord {
                message: format!("status post failed: {e}"),
            })?;

        let mut state = self.status_state.lock().await;
        state.set(
            telescope,
            StatusMessage {
                channel_id: channel.get(),
                message_id: posted.id.get(),
            },
        );
        if let Err(e) = state.save(&self.state_file) {
            eprintln!(
                "Warning: failed to persist status state to {}: {e}",
                self.state_file.display()
            );
        }
        Ok(())
    }
}

// ---------- Bot startup ----------

/// Start the Discord bot. Builds a Serenity client wired with the Poise
/// framework + Phase 1 commands, spawns the gateway loop, and returns the
/// service handle (which holds `Arc<Http>`) plus the join handle for the
/// gateway task. The join handle is kept by the caller so the service stays
/// alive for the life of the process.
pub async fn run_bot(
    bot_config: &DiscordBotConfig,
    api_clients: HashMap<String, SpaceCatApiClient>,
    channel_to_telescope: HashMap<u64, String>,
) -> Result<(DiscordBotService, tokio::task::JoinHandle<()>), ChatError> {
    let write_acl: std::collections::HashSet<u64> = bot_config.write_acl.iter().copied().collect();
    let token = bot_config.token.clone();
    let default_channel_id = bot_config.default_channel_id;
    let state_file = PathBuf::from(&bot_config.state_file);
    let status_state = StatusState::load(&state_file).unwrap_or_else(|e| {
        eprintln!(
            "Warning: could not load status state from {}: {e} — starting fresh",
            state_file.display()
        );
        StatusState::default()
    });
    let status_state = Arc::new(Mutex::new(status_state));

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: phase1_commands(),
            ..Default::default()
        })
        .setup(move |ctx, ready, framework| {
            Box::pin(async move {
                println!(
                    "Discord bot connected as {} (id {}), guilds: {}",
                    ready.user.name,
                    ready.user.id,
                    ready.guilds.len()
                );
                poise::builtins::register_globally(ctx, &framework.options().commands)
                    .await
                    .map_err(|e| -> BotError { Box::new(e) })?;
                Ok(BotData {
                    api_clients,
                    channel_to_telescope,
                    write_acl,
                })
            })
        })
        .build();

    let intents = serenity::GatewayIntents::GUILDS;
    let mut client = serenity::ClientBuilder::new(&token, intents)
        .framework(framework)
        .await
        .map_err(|e| ChatError::Initialization {
            service_name: "Discord bot".to_string(),
            reason: e.to_string(),
        })?;

    let http = client.http.clone();

    let join = tokio::spawn(async move {
        if let Err(e) = client.start().await {
            eprintln!("Discord bot gateway error: {e}");
        }
    });

    Ok((
        DiscordBotService::new(
            http,
            default_channel_id,
            status_state,
            state_file,
            bot_config.live_status,
        ),
        join,
    ))
}

// ---------- Slash commands (Phase 1, read-only) ----------

/// SpaceCat telescope monitoring commands.
#[poise::command(
    slash_command,
    subcommands(
        // Read-only (anyone)
        "status",
        "sequence",
        "target",
        "mount",
        "filter",
        "focus",
        "guider",
        "events",
        "last_image",
        // Write (ACL-gated; destructive ones require confirmation)
        "park",
        "unpark",
        "home",
        "change_filter",
        "guider_start",
        "guider_stop",
        "cool",
        "warm",
        "abort_capture",
        "stop_sequence",
        "start_sequence",
    )
)]
async fn spacecat(_ctx: Context<'_>) -> Result<(), BotError> {
    // Parent never runs directly when subcommands are defined.
    Ok(())
}

fn phase1_commands() -> Vec<poise::Command<BotData, BotError>> {
    vec![spacecat()]
}

/// Shorthand for "resolve telescope, send an ephemeral error to the user if
/// it fails."
async fn resolve_or_reply<'a>(
    ctx: Context<'a>,
    telescope: Option<String>,
) -> Result<(String, SpaceCatApiClient), BotError> {
    match ctx
        .data()
        .resolve_client(telescope.as_deref(), ctx.channel_id().get())
    {
        Ok(v) => Ok(v),
        Err(msg) => {
            ctx.send(poise::CreateReply::default().content(msg).ephemeral(true))
                .await?;
            Err("telescope resolution failed".into())
        }
    }
}

/// One-page summary embed: target + mount + sequence + filter.
#[poise::command(slash_command)]
async fn status(
    ctx: Context<'_>,
    #[description = "Telescope name (defaults to this channel's telescope)"] telescope: Option<
        String,
    >,
) -> Result<(), BotError> {
    let (name, client) = match resolve_or_reply(ctx, telescope).await {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    ctx.defer().await?;

    let mut embed = serenity::CreateEmbed::new().title(format!("[{name}] Status"));

    if let Ok(mount) = client.get_mount_info().await {
        let (ra, dec) = mount.get_coordinates();
        let (alt, az) = mount.get_alt_az();
        embed = embed.field(
            "Mount",
            format!(
                "Connected: {}\nTracking: {}\nParked: {}\nRA: {} Dec: {}\nAlt: {} Az: {}",
                mount.is_connected(),
                mount.response.tracking_enabled,
                mount.response.at_park,
                ra,
                dec,
                alt,
                az
            ),
            false,
        );
    }

    if let Ok(seq) = client.get_sequence().await {
        let containers = seq.get_containers();
        let running = containers
            .iter()
            .filter(|c| c.status.eq_ignore_ascii_case("RUNNING"))
            .count();
        let active_target =
            crate::sequence::extract_current_target(&seq).unwrap_or_else(|| "(none)".to_string());
        embed = embed.field(
            "Sequence",
            format!(
                "Target: {active_target}\nContainers: {} total, {running} running",
                containers.len()
            ),
            false,
        );
    }

    if let Ok(fw) = client.get_filterwheel_info().await
        && fw.response.connected
        && let Some(sel) = &fw.response.selected_filter
    {
        embed = embed.field("Filter", format!("{} (ID: {})", sel.name, sel.id), true);
    }

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

#[poise::command(slash_command)]
async fn sequence(
    ctx: Context<'_>,
    #[description = "Telescope name (defaults to this channel's telescope)"] telescope: Option<
        String,
    >,
) -> Result<(), BotError> {
    let (name, client) = match resolve_or_reply(ctx, telescope).await {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    ctx.defer().await?;
    let seq = client.get_sequence().await?;
    let containers = seq.get_containers();
    let lines: Vec<String> = containers
        .iter()
        .map(|c| format!("• {} — {} ({} items)", c.name, c.status, c.items.len()))
        .collect();
    let active_target =
        crate::sequence::extract_current_target(&seq).unwrap_or_else(|| "(none)".to_string());
    let flip = crate::sequence::extract_meridian_flip_time(&seq)
        .map(crate::sequence::meridian_flip_time_formatted_with_clock)
        .unwrap_or_else(|| "(n/a)".to_string());

    let embed = serenity::CreateEmbed::new()
        .title(format!("[{name}] Sequence"))
        .field("Active target", active_target, true)
        .field("Meridian flip in", flip, true)
        .field("Containers", lines.join("\n"), false);
    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

#[poise::command(slash_command)]
async fn target(
    ctx: Context<'_>,
    #[description = "Telescope name"] telescope: Option<String>,
) -> Result<(), BotError> {
    let (name, client) = match resolve_or_reply(ctx, telescope).await {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    ctx.defer().await?;
    let seq = client.get_sequence().await?;
    let active_target =
        crate::sequence::extract_current_target(&seq).unwrap_or_else(|| "(none)".to_string());

    // Look for the latest TS-TARGETSTART event for richer coords.
    let events = client.get_event_history().await.ok();
    let ts_target = events.as_ref().and_then(|e| {
        e.response.iter().rev().find_map(|ev| {
            if let Some(crate::events::EventDetails::TargetStart {
                target_name,
                coordinates,
                project_name,
                rotation,
                ..
            }) = &ev.details
            {
                Some((
                    target_name.clone(),
                    coordinates.clone(),
                    project_name.clone(),
                    *rotation,
                ))
            } else {
                None
            }
        })
    });

    let mut embed = serenity::CreateEmbed::new().title(format!("[{name}] Target"));
    if let Some((tname, coords, project, rot)) = ts_target {
        embed = embed
            .field("Name", tname, true)
            .field("Project", project, true)
            .field("Rotation", format!("{rot}°"), true);
        if let Some(s) = coords.display() {
            embed = embed.field("Coordinates", s, false);
        }
    } else {
        embed = embed.field("Sequence target", active_target, false);
    }
    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

#[poise::command(slash_command)]
async fn mount(
    ctx: Context<'_>,
    #[description = "Telescope name"] telescope: Option<String>,
) -> Result<(), BotError> {
    let (name, client) = match resolve_or_reply(ctx, telescope).await {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    ctx.defer().await?;
    let mount = client.get_mount_info().await?;
    let m = &mount.response;
    let (ra, dec) = mount.get_coordinates();
    let (alt, az) = mount.get_alt_az();
    let flip = mount.get_time_to_meridian_flip_string();

    let embed = serenity::CreateEmbed::new()
        .title(format!("[{name}] Mount"))
        .field(
            "Status",
            format!(
                "Connected: {}\nTracking: {}\nParked: {}\nSlewing: {}\nAt home: {}",
                m.connected, m.tracking_enabled, m.at_park, m.slewing, m.at_home
            ),
            false,
        )
        .field("RA / Dec", format!("RA: {ra}\nDec: {dec}"), true)
        .field("Alt / Az", format!("Alt: {alt}\nAz: {az}"), true)
        .field("Pier side", mount.get_side_of_pier().to_string(), true)
        .field("Sidereal time", &m.sidereal_time_string, true)
        .field("Time to flip", flip, true);
    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

#[poise::command(slash_command)]
async fn filter(
    ctx: Context<'_>,
    #[description = "Telescope name"] telescope: Option<String>,
) -> Result<(), BotError> {
    let (name, client) = match resolve_or_reply(ctx, telescope).await {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    ctx.defer().await?;
    let fw = client.get_filterwheel_info().await?;
    let mut embed = serenity::CreateEmbed::new().title(format!("[{name}] Filter wheel"));
    if let Some(sel) = &fw.response.selected_filter {
        embed = embed.field("Selected", format!("{} (ID: {})", sel.name, sel.id), false);
    } else {
        embed = embed.field("Selected", "(none)", false);
    }
    if !fw.response.available_filters.is_empty() {
        let avail = fw
            .response
            .available_filters
            .iter()
            .map(|f| format!("{} ({})", f.name, f.id))
            .collect::<Vec<_>>()
            .join(", ");
        embed = embed.field("Available", avail, false);
    }
    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

#[poise::command(slash_command)]
async fn focus(
    ctx: Context<'_>,
    #[description = "Telescope name"] telescope: Option<String>,
) -> Result<(), BotError> {
    let (name, client) = match resolve_or_reply(ctx, telescope).await {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    ctx.defer().await?;
    let af = client.get_last_autofocus().await?;
    let d = &af.response;
    let position_change = d.calculated_focus_point.position - d.previous_focus_point.position;
    let embed = serenity::CreateEmbed::new()
        .title(format!("[{name}] Last autofocus"))
        .field("Filter", &d.filter, true)
        .field("Method", &d.method, true)
        .field("Duration", &d.duration, true)
        .field("Temperature", format!("{:.1}°C", d.temperature), true)
        .field(
            "Position",
            format!(
                "{} (Δ {:+})",
                d.calculated_focus_point.position, position_change
            ),
            true,
        )
        .field(
            "HFR",
            format!("{:.3}", d.calculated_focus_point.value),
            true,
        )
        .field("Best R²", format!("{:.4}", af.get_best_r_squared()), true)
        .field("Timestamp", &d.timestamp, true);
    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

#[poise::command(slash_command)]
async fn guider(
    ctx: Context<'_>,
    #[description = "Telescope name"] telescope: Option<String>,
) -> Result<(), BotError> {
    let (name, client) = match resolve_or_reply(ctx, telescope).await {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    ctx.defer().await?;
    let info = client.get_guider_info().await?;
    let g = &info.response;
    let mut embed = serenity::CreateEmbed::new()
        .title(format!("[{name}] Guider"))
        .field("Connected", g.connected.to_string(), true)
        .field("State", &g.state, true);
    if g.pixel_scale > 0.0 {
        embed = embed.field(
            "Pixel scale",
            format!("{:.3} arcsec/px", g.pixel_scale),
            true,
        );
    }
    if let Some(rms) = &g.rms_error {
        embed = embed.field(
            "RMS error",
            format!(
                "Total: {:.2}\"\nRA: {:.2}\"  Dec: {:.2}\"",
                rms.total.arcseconds, rms.ra.arcseconds, rms.dec.arcseconds
            ),
            false,
        );
    }
    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

#[poise::command(slash_command)]
async fn events(
    ctx: Context<'_>,
    #[description = "Number of events to show (default 10)"] count: Option<u32>,
    #[description = "Telescope name"] telescope: Option<String>,
) -> Result<(), BotError> {
    let (name, client) = match resolve_or_reply(ctx, telescope).await {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    ctx.defer().await?;
    let history = client.get_event_history().await?;
    let count = count.unwrap_or(10).min(25) as usize;
    let events: Vec<&crate::events::Event> = history.response.iter().rev().take(count).collect();
    let lines: Vec<String> = events
        .iter()
        .rev()
        .map(|e| format!("`{}` {}", e.time, e.event))
        .collect();
    let embed = serenity::CreateEmbed::new()
        .title(format!("[{name}] Last {count} events"))
        .description(if lines.is_empty() {
            "(no events)".to_string()
        } else {
            lines.join("\n")
        });
    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

#[poise::command(slash_command, rename = "last-image")]
async fn last_image(
    ctx: Context<'_>,
    #[description = "Telescope name"] telescope: Option<String>,
) -> Result<(), BotError> {
    let (name, client) = match resolve_or_reply(ctx, telescope).await {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    ctx.defer().await?;
    let images = client.get_all_image_history().await?;
    let Some((idx, img)) = images.response.iter().enumerate().next_back() else {
        ctx.send(
            poise::CreateReply::default()
                .content("No images in history.")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    };

    let thumb_bytes = client.get_thumbnail(idx as u32).await.ok().map(|t| t.data);

    let mut embed = serenity::CreateEmbed::new()
        .title(format!("[{name}] Last image"))
        .field("Date", &img.date, true)
        .field("Type", &img.image_type, true)
        .field("Filter", &img.filter, true)
        .field("Exposure", format!("{:.1}s", img.exposure_time), true)
        .field("Temperature", format!("{:.1}°C", img.temperature), true)
        .field("Stars", img.stars.to_string(), true)
        .field("HFR", format!("{:.2}", img.hfr), true)
        .field("RMS", &img.rms_text, true);

    let mut reply = poise::CreateReply::default();
    if let Some(bytes) = thumb_bytes {
        let filename = format!("thumbnail_{idx}.jpg");
        embed = embed.image(format!("attachment://{filename}"));
        reply = reply.attachment(CreateAttachment::bytes(bytes, filename));
    }
    reply = reply.embed(embed);
    ctx.send(reply).await?;
    Ok(())
}

// ---------- Phase 3: write commands (ACL-gated) ----------

/// Reject the invocation with an ephemeral message if the invoking user is
/// not in `chat.discord_bot.write_acl`. Returns `Ok(())` when authorized.
async fn require_write_acl(ctx: Context<'_>) -> Result<(), BotError> {
    let user_id = ctx.author().id.get();
    if ctx.data().write_acl.contains(&user_id) {
        return Ok(());
    }
    ctx.send(
        poise::CreateReply::default()
            .content(format!(
                "🔒 You are not authorized to issue write commands. \
                 Your user ID `{user_id}` is not in `chat.discord_bot.write_acl`."
            ))
            .ephemeral(true),
    )
    .await?;
    Err("user not in write_acl".into())
}

/// Post a Confirm / Cancel button pair and wait for the invoker to click.
/// Returns `Ok(true)` on Confirm, `Ok(false)` on Cancel or 30s timeout.
async fn confirm_destructive(ctx: Context<'_>, action: &str) -> Result<bool, BotError> {
    let prompt =
        format!("⚠️ Confirm **{action}**?\nThis is a destructive operation — you have 30 seconds.");
    let row = serenity::CreateActionRow::Buttons(vec![
        serenity::CreateButton::new("spacecat-confirm")
            .label("Confirm")
            .style(serenity::ButtonStyle::Danger),
        serenity::CreateButton::new("spacecat-cancel")
            .label("Cancel")
            .style(serenity::ButtonStyle::Secondary),
    ]);
    let handle = ctx
        .send(
            poise::CreateReply::default()
                .content(prompt)
                .components(vec![row])
                .ephemeral(true),
        )
        .await?;
    let message = handle.message().await?;

    let interaction = message
        .await_component_interaction(ctx.serenity_context().shard.clone())
        .author_id(ctx.author().id)
        .timeout(std::time::Duration::from_secs(30))
        .await;

    let (confirmed, response_text) = match interaction.as_ref().map(|i| i.data.custom_id.as_str()) {
        Some("spacecat-confirm") => (true, "✅ Confirmed, running command…"),
        Some("spacecat-cancel") => (false, "❎ Cancelled."),
        _ => (false, "⏱️ Timed out — no action taken."),
    };

    // Acknowledge the interaction (or just edit the original message if
    // the user didn't click anything).
    if let Some(i) = interaction {
        let _ = i
            .create_response(
                &ctx.serenity_context().http,
                serenity::CreateInteractionResponse::UpdateMessage(
                    serenity::CreateInteractionResponseMessage::new()
                        .content(response_text)
                        .components(vec![]),
                ),
            )
            .await;
    } else {
        let _ = handle
            .edit(
                ctx,
                poise::CreateReply::default()
                    .content(response_text)
                    .components(vec![]),
            )
            .await;
    }
    Ok(confirmed)
}

/// Issue a NINA command endpoint and reply with a status line. Used by all
/// write commands once authorization (and any confirmation) has passed.
async fn run_command(
    ctx: Context<'_>,
    telescope: &str,
    client: &SpaceCatApiClient,
    label: &str,
    endpoint: &str,
    params: &[(&str, &str)],
) -> Result<(), BotError> {
    let result = client.execute_command(endpoint, params).await;
    let reply = match result {
        Ok(resp) if resp.success => poise::CreateReply::default()
            .content(format!("✅ [{telescope}] {label}: {}", resp.summary())),
        Ok(resp) => poise::CreateReply::default()
            .ephemeral(true)
            .content(format!("❌ [{telescope}] {label}: {}", resp.summary())),
        Err(e) => poise::CreateReply::default()
            .ephemeral(true)
            .content(format!("❌ [{telescope}] {label} failed: {e}")),
    };
    ctx.send(reply).await?;
    Ok(())
}

// --- Non-destructive (ACL only) ---

/// Unpark the mount.
#[poise::command(slash_command)]
async fn unpark(
    ctx: Context<'_>,
    #[description = "Telescope name"] telescope: Option<String>,
) -> Result<(), BotError> {
    if require_write_acl(ctx).await.is_err() {
        return Ok(());
    }
    let (name, client) = match resolve_or_reply(ctx, telescope).await {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    ctx.defer().await?;
    run_command(
        ctx,
        &name,
        &client,
        "Unpark mount",
        "/equipment/mount/unpark",
        &[],
    )
    .await
}

/// Send the mount to its home position.
#[poise::command(slash_command)]
async fn home(
    ctx: Context<'_>,
    #[description = "Telescope name"] telescope: Option<String>,
) -> Result<(), BotError> {
    if require_write_acl(ctx).await.is_err() {
        return Ok(());
    }
    let (name, client) = match resolve_or_reply(ctx, telescope).await {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    ctx.defer().await?;
    run_command(
        ctx,
        &name,
        &client,
        "Home mount",
        "/equipment/mount/home",
        &[],
    )
    .await
}

/// Change the filter wheel position by filter name (resolved to id via
/// `/equipment/filterwheel/info`).
#[poise::command(slash_command, rename = "change-filter")]
async fn change_filter(
    ctx: Context<'_>,
    #[description = "Filter name (e.g. L, R, G, B, HA)"] filter: String,
    #[description = "Telescope name"] telescope: Option<String>,
) -> Result<(), BotError> {
    if require_write_acl(ctx).await.is_err() {
        return Ok(());
    }
    let (name, client) = match resolve_or_reply(ctx, telescope).await {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    ctx.defer().await?;

    let info = match client.get_filterwheel_info().await {
        Ok(i) => i,
        Err(e) => {
            ctx.send(
                poise::CreateReply::default()
                    .ephemeral(true)
                    .content(format!("❌ [{name}] couldn't fetch filterwheel info: {e}")),
            )
            .await?;
            return Ok(());
        }
    };

    let target = info
        .response
        .available_filters
        .iter()
        .find(|f| f.name.eq_ignore_ascii_case(&filter));
    let Some(target) = target else {
        let known: Vec<&str> = info
            .response
            .available_filters
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        ctx.send(
            poise::CreateReply::default()
                .ephemeral(true)
                .content(format!(
                    "❌ [{name}] no filter '{filter}'. Known: {known:?}"
                )),
        )
        .await?;
        return Ok(());
    };

    let id = target.id.to_string();
    run_command(
        ctx,
        &name,
        &client,
        &format!("Change filter → {} (ID {})", target.name, target.id),
        "/equipment/filterwheel/change-filter",
        &[("filterId", id.as_str())],
    )
    .await
}

/// Start guiding (without calibration).
#[poise::command(slash_command, rename = "guider-start")]
async fn guider_start(
    ctx: Context<'_>,
    #[description = "Run calibration first"] calibrate: Option<bool>,
    #[description = "Telescope name"] telescope: Option<String>,
) -> Result<(), BotError> {
    if require_write_acl(ctx).await.is_err() {
        return Ok(());
    }
    let (name, client) = match resolve_or_reply(ctx, telescope).await {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    ctx.defer().await?;
    let cal = if calibrate.unwrap_or(false) {
        "true"
    } else {
        "false"
    };
    run_command(
        ctx,
        &name,
        &client,
        "Start guiding",
        "/equipment/guider/start",
        &[("calibrate", cal)],
    )
    .await
}

/// Stop guiding.
#[poise::command(slash_command, rename = "guider-stop")]
async fn guider_stop(
    ctx: Context<'_>,
    #[description = "Telescope name"] telescope: Option<String>,
) -> Result<(), BotError> {
    if require_write_acl(ctx).await.is_err() {
        return Ok(());
    }
    let (name, client) = match resolve_or_reply(ctx, telescope).await {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    ctx.defer().await?;
    run_command(
        ctx,
        &name,
        &client,
        "Stop guiding",
        "/equipment/guider/stop",
        &[],
    )
    .await
}

/// Cool the camera to a target temperature over `minutes` minutes.
#[poise::command(slash_command)]
async fn cool(
    ctx: Context<'_>,
    #[description = "Target temperature in °C"] temperature: f64,
    #[description = "Minutes to ramp down (default 10)"] minutes: Option<f64>,
    #[description = "Telescope name"] telescope: Option<String>,
) -> Result<(), BotError> {
    if require_write_acl(ctx).await.is_err() {
        return Ok(());
    }
    let (name, client) = match resolve_or_reply(ctx, telescope).await {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    ctx.defer().await?;
    let t = temperature.to_string();
    let m = minutes.unwrap_or(10.0).to_string();
    run_command(
        ctx,
        &name,
        &client,
        &format!(
            "Cool to {temperature:.1}°C over {} min",
            minutes.unwrap_or(10.0)
        ),
        "/equipment/camera/cool",
        &[("temperature", t.as_str()), ("minutes", m.as_str())],
    )
    .await
}

/// Warm the camera over `minutes` minutes.
#[poise::command(slash_command)]
async fn warm(
    ctx: Context<'_>,
    #[description = "Minutes to warm (default 10)"] minutes: Option<f64>,
    #[description = "Telescope name"] telescope: Option<String>,
) -> Result<(), BotError> {
    if require_write_acl(ctx).await.is_err() {
        return Ok(());
    }
    let (name, client) = match resolve_or_reply(ctx, telescope).await {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    ctx.defer().await?;
    let m = minutes.unwrap_or(10.0).to_string();
    run_command(
        ctx,
        &name,
        &client,
        &format!("Warm camera over {} min", minutes.unwrap_or(10.0)),
        "/equipment/camera/warm",
        &[("minutes", m.as_str())],
    )
    .await
}

// --- Destructive (ACL + button confirm) ---

/// Park the mount (requires confirmation).
#[poise::command(slash_command)]
async fn park(
    ctx: Context<'_>,
    #[description = "Telescope name"] telescope: Option<String>,
) -> Result<(), BotError> {
    if require_write_acl(ctx).await.is_err() {
        return Ok(());
    }
    let (name, client) = match resolve_or_reply(ctx, telescope).await {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    if !confirm_destructive(ctx, &format!("park {name}")).await? {
        return Ok(());
    }
    run_command(
        ctx,
        &name,
        &client,
        "Park mount",
        "/equipment/mount/park",
        &[],
    )
    .await
}

/// Abort the current camera exposure (requires confirmation).
#[poise::command(slash_command, rename = "abort-capture")]
async fn abort_capture(
    ctx: Context<'_>,
    #[description = "Telescope name"] telescope: Option<String>,
) -> Result<(), BotError> {
    if require_write_acl(ctx).await.is_err() {
        return Ok(());
    }
    let (name, client) = match resolve_or_reply(ctx, telescope).await {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    if !confirm_destructive(ctx, &format!("abort capture on {name}")).await? {
        return Ok(());
    }
    run_command(
        ctx,
        &name,
        &client,
        "Abort capture",
        "/equipment/camera/abort-exposure",
        &[],
    )
    .await
}

/// Stop the running sequence (requires confirmation).
#[poise::command(slash_command, rename = "stop-sequence")]
async fn stop_sequence(
    ctx: Context<'_>,
    #[description = "Telescope name"] telescope: Option<String>,
) -> Result<(), BotError> {
    if require_write_acl(ctx).await.is_err() {
        return Ok(());
    }
    let (name, client) = match resolve_or_reply(ctx, telescope).await {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    if !confirm_destructive(ctx, &format!("stop sequence on {name}")).await? {
        return Ok(());
    }
    run_command(ctx, &name, &client, "Stop sequence", "/sequence/stop", &[]).await
}

/// Start the loaded sequence (requires confirmation).
#[poise::command(slash_command, rename = "start-sequence")]
async fn start_sequence(
    ctx: Context<'_>,
    #[description = "Skip pre-run validation"] skip_validation: Option<bool>,
    #[description = "Telescope name"] telescope: Option<String>,
) -> Result<(), BotError> {
    if require_write_acl(ctx).await.is_err() {
        return Ok(());
    }
    let (name, client) = match resolve_or_reply(ctx, telescope).await {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    if !confirm_destructive(ctx, &format!("start sequence on {name}")).await? {
        return Ok(());
    }
    let skip = if skip_validation.unwrap_or(false) {
        "true"
    } else {
        "false"
    };
    run_command(
        ctx,
        &name,
        &client,
        "Start sequence",
        "/sequence/start",
        &[("skipValidation", skip)],
    )
    .await
}
