//! Per-rig chat updaters on the hub.
//!
//! A reconcile loop compares live `/v1/direct` connections against running
//! `ChatUpdater` tasks: a connected telescope with a routed channel gets an
//! updater; a disconnected or replaced one has its updater stopped. The
//! server-issued connection ID distinguishes a reconnect from steady state;
//! the client session ID deliberately survives reconnects.

use super::db::Db;
use super::direct_server::RigConnections;
use super::direct_source::DirectRigSource;
use crate::chat::{ChatMessage, ChatServiceManager, ChatTarget};
use crate::chat_updater::ChatUpdater;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// How often the reconcile loop runs.
const RECONCILE_INTERVAL: Duration = Duration::from_secs(10);

/// Poll interval handed to each ChatUpdater.
const UPDATER_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// How long a scope must stay disconnected before chat hears about it.
/// Absorbs hub deploys, rig reconnects, and plugin flapping.
const PRESENCE_OFFLINE_GRACE: Duration = Duration::from_secs(90);

/// What chat currently believes about one scope, and what we observe.
struct Presence {
    /// The state chat was last told (None until adopted or announced).
    announced: Option<bool>,
    /// The state we currently observe.
    current: bool,
    /// When `current` last changed (or was first observed).
    since: Instant,
}

/// A presence transition worth telling chat about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresenceEvent {
    pub telescope_id: i64,
    pub telescope_name: String,
    pub online: bool,
}

struct RunningUpdater {
    connection_id: Uuid,
    /// The config the updater was built with. A change in the database
    /// (destinations added or removed, cooldown adjusted) restarts the
    /// updater, which otherwise freezes its config at construction.
    channels: Vec<i64>,
    image_cooldown_seconds: i64,
    handle: tokio::task::JoinHandle<()>,
}

pub struct UpdaterManager {
    db: Db,
    connections: Arc<RigConnections>,
    chat_manager: Arc<ChatServiceManager>,
    running: Mutex<HashMap<i64, RunningUpdater>>,
    presence: Mutex<HashMap<i64, Presence>>,
    offline_grace: Duration,
}

impl UpdaterManager {
    pub fn new(
        db: Db,
        connections: Arc<RigConnections>,
        chat_manager: Arc<ChatServiceManager>,
    ) -> Self {
        Self {
            db,
            connections,
            chat_manager,
            running: Mutex::new(HashMap::new()),
            presence: Mutex::new(HashMap::new()),
            offline_grace: PRESENCE_OFFLINE_GRACE,
        }
    }

    /// Shrink the offline grace for tests.
    #[cfg(test)]
    fn with_offline_grace(mut self, grace: Duration) -> Self {
        self.offline_grace = grace;
        self
    }

    /// Compare observed connection state against what chat believes, and
    /// return the transitions worth announcing.
    ///
    /// The first observation of a scope is adopted silently — after a hub
    /// restart every scope is "first observed", so deploys never generate
    /// chat traffic. A disconnect is announced only after the grace period,
    /// which also absorbs reconnect flapping; a reconnect is announced
    /// immediately once the scope was believed (or adopted as) offline.
    pub fn presence_events(&self) -> Vec<PresenceEvent> {
        let mut events = Vec::new();
        let Ok(mut presence) = self.presence.lock() else {
            return events;
        };
        let connected: std::collections::HashSet<i64> = self
            .connections
            .connected_telescopes()
            .into_iter()
            .collect();
        // Every telescope with destinations participates; others have no
        // audience to tell.
        let mut ids: std::collections::HashSet<i64> = connected.clone();
        ids.extend(presence.keys().copied());
        for id in ids {
            let observed = connected.contains(&id);
            let entry = presence.entry(id).or_insert(Presence {
                announced: None,
                current: observed,
                since: Instant::now(),
            });
            if entry.current != observed {
                entry.current = observed;
                entry.since = Instant::now();
            }
            match (entry.announced, entry.current) {
                // Silent adoption: what chat first learns is the baseline.
                (None, true) => entry.announced = Some(true),
                (None, false) => {
                    if entry.since.elapsed() >= self.offline_grace {
                        entry.announced = Some(false);
                    }
                }
                (Some(true), false) => {
                    if entry.since.elapsed() >= self.offline_grace {
                        entry.announced = Some(false);
                        if let Some(name) = self.telescope_name(id) {
                            events.push(PresenceEvent {
                                telescope_id: id,
                                telescope_name: name,
                                online: false,
                            });
                        }
                    }
                }
                (Some(false), true) => {
                    entry.announced = Some(true);
                    if let Some(name) = self.telescope_name(id) {
                        events.push(PresenceEvent {
                            telescope_id: id,
                            telescope_name: name,
                            online: true,
                        });
                    }
                }
                _ => {}
            }
        }
        events
    }

    fn telescope_name(&self, telescope_id: i64) -> Option<String> {
        self.db
            .get_telescope(telescope_id)
            .ok()
            .flatten()
            .map(|t| t.name)
    }

    /// Post presence transitions to the telescope's destination channels.
    pub async fn announce(&self, events: Vec<PresenceEvent>) {
        for event in events {
            let channels = self.route_channels(event.telescope_id);
            if channels.is_empty() {
                continue;
            }
            let target = ChatTarget {
                discord_webhook_url: None,
                matrix_room_id: None,
                discord_channel_id: None,
                discord_channel_ids: channels.iter().map(|c| *c as u64).collect(),
            };
            let message = if event.online {
                ChatMessage::new(&format!(
                    "🔭 [{}] Telescope connected",
                    event.telescope_name
                ))
                .color(0x3fb950)
            } else {
                ChatMessage::new(&format!(
                    "🔌 [{}] Telescope disconnected",
                    event.telescope_name
                ))
                .color(0xd29922)
            };
            self.chat_manager.send_message(&message, &target).await;
        }
    }

    /// This telescope's destination channels, sorted for comparison.
    fn route_channels(&self, telescope_id: i64) -> Vec<i64> {
        let mut channels: Vec<i64> = self
            .db
            .telescope_routes(telescope_id)
            .map(|routes| routes.iter().map(|r| r.channel_id).collect())
            .unwrap_or_default();
        channels.sort_unstable();
        channels
    }

    /// One reconcile pass. Returns (started, stopped) counts.
    pub fn reconcile_once(&self) -> (usize, usize) {
        let mut started = 0;
        let mut stopped = 0;
        let Ok(mut running) = self.running.lock() else {
            return (0, 0);
        };

        // Stop updaters whose connection is gone or replaced, or whose
        // database config no longer matches what they were built with.
        running.retain(|telescope_id, updater| {
            let connection_current = self
                .connections
                .get(*telescope_id)
                .is_some_and(|connection| connection.connection_id == updater.connection_id);
            let config_current = matches!(
                self.db.get_telescope(*telescope_id),
                Ok(Some(row)) if row.image_cooldown_seconds == updater.image_cooldown_seconds
            ) && self.route_channels(*telescope_id) == updater.channels;
            let keep = connection_current && config_current;
            if !keep {
                updater.handle.abort();
                stopped += 1;
                println!("Stopped chat updater for telescope {telescope_id}");
            }
            keep
        });

        // Start updaters for connected telescopes with a routed channel.
        for telescope_id in self.connections.connected_telescopes() {
            if running.contains_key(&telescope_id) {
                continue;
            }
            let Some(connection) = self.connections.get(telescope_id) else {
                continue;
            };
            let telescope = match self.db.get_telescope(telescope_id) {
                Ok(Some(row)) => row,
                _ => continue,
            };
            let channels = self.route_channels(telescope_id);
            if channels.is_empty() {
                // No destinations yet; nothing to post to.
                continue;
            }

            let connection_id = connection.connection_id;
            let source = Arc::new(DirectRigSource::new(connection));
            let target = ChatTarget {
                discord_webhook_url: None,
                matrix_room_id: None,
                discord_channel_id: None,
                discord_channel_ids: channels.iter().map(|c| *c as u64).collect(),
            };
            let mut updater = ChatUpdater::new(
                source,
                telescope.name.clone(),
                target,
                self.chat_manager.clone(),
            )
            .with_image_cooldown(telescope.image_cooldown_seconds.max(0) as u64)
            // Hub updaters restart on every deploy, reconnect, and config
            // change; presence is announced from connection state instead.
            .with_lifecycle_announcements(false);
            let handle = tokio::spawn(async move {
                updater.start_polling(UPDATER_POLL_INTERVAL).await;
            });
            running.insert(
                telescope_id,
                RunningUpdater {
                    connection_id,
                    channels,
                    image_cooldown_seconds: telescope.image_cooldown_seconds,
                    handle,
                },
            );
            started += 1;
            println!(
                "Started chat updater for telescope {telescope_id} ({})",
                telescope.name
            );
        }
        (started, stopped)
    }

    pub fn running_count(&self) -> usize {
        self.running.lock().map(|r| r.len()).unwrap_or(0)
    }

    /// Reconcile forever, announcing real presence transitions.
    pub async fn run(self: Arc<Self>) {
        loop {
            self.reconcile_once();
            let events = self.presence_events();
            self.announce(events).await;
            tokio::time::sleep(RECONCILE_INTERVAL).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub::direct_server::RigConnection;
    use crate::hub::store::UserRow;

    fn setup() -> (Db, Arc<RigConnections>, UpdaterManager, i64) {
        let db = Db::open_in_memory().unwrap();
        db.upsert_user(&UserRow {
            discord_user_id: 1,
            username: "admin".to_string(),
            email: None,
            email_verified: false,
            avatar_url: None,
        })
        .unwrap();
        db.register_guild(100, "g", 1).unwrap();
        let telescope = db.create_telescope(1, "c925").unwrap();
        db.attach_telescope(telescope.id, 100, true, 1).unwrap();
        let connections = Arc::new(RigConnections::default());
        let manager = UpdaterManager::new(
            db.clone(),
            connections.clone(),
            Arc::new(ChatServiceManager::new()),
        );
        (db, connections, manager, telescope.id)
    }

    fn connect(connections: &RigConnections, telescope_id: i64) -> Uuid {
        let session = Uuid::new_v4();
        let (connection, rx) = RigConnection::stub(telescope_id, session);
        std::mem::forget(rx);
        connections.insert(connection);
        session
    }

    #[tokio::test]
    async fn no_updater_without_channel_routing() {
        let (_db, connections, manager, id) = setup();
        connect(&connections, id);
        assert_eq!(manager.reconcile_once(), (0, 0));
        assert_eq!(manager.running_count(), 0);
    }

    #[tokio::test]
    async fn updater_started_and_stopped_with_connection() {
        let (db, connections, manager, id) = setup();
        db.add_channel_route(id, 100, 42, "obs", "g", 1).unwrap();

        connect(&connections, id);
        assert_eq!(manager.reconcile_once(), (1, 0));
        assert_eq!(manager.running_count(), 1);
        // Steady state: nothing changes.
        assert_eq!(manager.reconcile_once(), (0, 0));

        // Connection drops.
        let connection_id = connections.get(id).unwrap().connection_id;
        connections.remove_if_current(id, connection_id);
        assert_eq!(manager.reconcile_once(), (0, 1));
        assert_eq!(manager.running_count(), 0);
    }

    #[tokio::test]
    async fn destination_changes_restart_updater() {
        let (db, connections, manager, id) = setup();
        let first = db.add_channel_route(id, 100, 42, "obs", "g", 1).unwrap();
        connect(&connections, id);
        assert_eq!(manager.reconcile_once(), (1, 0));

        // Adding a second destination restarts the updater with both.
        let second = db.add_channel_route(id, 100, 43, "alerts", "g", 1).unwrap();
        assert_eq!(manager.reconcile_once(), (1, 1));

        // Removing one destination restarts again.
        db.delete_route(second.id).unwrap();
        assert_eq!(manager.reconcile_once(), (1, 1));

        // Removing the last destination stops it without a replacement.
        db.delete_route(first.id).unwrap();
        assert_eq!(manager.reconcile_once(), (0, 1));
        assert_eq!(manager.running_count(), 0);
    }

    #[tokio::test]
    async fn presence_first_observation_is_silent_then_transitions_announce() {
        let (db, connections, manager, id) = setup();
        let manager = manager.with_offline_grace(Duration::ZERO);
        db.add_channel_route(id, 100, 42, "obs", "g", 1).unwrap();

        // First observation (e.g. right after a hub deploy): silent adoption.
        connect(&connections, id);
        assert!(manager.presence_events().is_empty());
        assert!(manager.presence_events().is_empty());

        // A real disconnect (grace elapsed) announces once.
        let connection_id = connections.get(id).unwrap().connection_id;
        connections.remove_if_current(id, connection_id);
        let events = manager.presence_events();
        assert_eq!(events.len(), 1);
        assert!(!events[0].online);
        assert_eq!(events[0].telescope_name, "c925");
        assert!(manager.presence_events().is_empty());

        // Reconnect announces once.
        connect(&connections, id);
        let events = manager.presence_events();
        assert_eq!(events.len(), 1);
        assert!(events[0].online);
        assert!(manager.presence_events().is_empty());
    }

    #[tokio::test]
    async fn presence_flap_within_grace_is_silent() {
        let (db, connections, manager, id) = setup();
        // Default 90s grace: a quick drop and reconnect never reaches chat.
        db.add_channel_route(id, 100, 42, "obs", "g", 1).unwrap();
        connect(&connections, id);
        assert!(manager.presence_events().is_empty());
        let connection_id = connections.get(id).unwrap().connection_id;
        connections.remove_if_current(id, connection_id);
        assert!(manager.presence_events().is_empty());
        connect(&connections, id);
        assert!(manager.presence_events().is_empty());
    }

    #[tokio::test]
    async fn presence_scope_offline_at_startup_announces_when_it_connects() {
        let (db, connections, manager, id) = setup();
        let manager = manager.with_offline_grace(Duration::ZERO);
        db.add_channel_route(id, 100, 42, "obs", "g", 1).unwrap();

        // Seed presence with a disconnected observation (scope was down
        // when the hub started): silent adoption as offline...
        {
            let mut presence = manager.presence.lock().unwrap();
            presence.insert(
                id,
                Presence {
                    announced: None,
                    current: false,
                    since: Instant::now(),
                },
            );
        }
        assert!(manager.presence_events().is_empty());

        // ...so its eventual arrival is news.
        connect(&connections, id);
        let events = manager.presence_events();
        assert_eq!(events.len(), 1);
        assert!(events[0].online);
    }

    #[tokio::test]
    async fn reconnect_replaces_updater() {
        let (db, connections, manager, id) = setup();
        db.add_channel_route(id, 100, 42, "obs", "g", 1).unwrap();

        connect(&connections, id);
        assert_eq!(manager.reconcile_once(), (1, 0));

        // A new WebSocket generation takes the slot (rig reconnected).
        connect(&connections, id);
        assert_eq!(manager.reconcile_once(), (1, 1));
        assert_eq!(manager.running_count(), 1);
    }
}
