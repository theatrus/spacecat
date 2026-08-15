//! Per-rig chat updaters on the hub.
//!
//! A reconcile loop compares live `/v1/direct` connections against running
//! `ChatUpdater` tasks: a connected telescope with a routed channel gets an
//! updater; a disconnected or replaced one has its updater stopped. Session
//! IDs distinguish a reconnect (new updater against the fresh connection)
//! from steady state.

use super::db::Db;
use super::direct_server::RigConnections;
use super::direct_source::DirectRigSource;
use crate::chat::{ChatServiceManager, ChatTarget};
use crate::chat_updater::ChatUpdater;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

/// How often the reconcile loop runs.
const RECONCILE_INTERVAL: Duration = Duration::from_secs(10);

/// Poll interval handed to each ChatUpdater.
const UPDATER_POLL_INTERVAL: Duration = Duration::from_secs(5);

struct RunningUpdater {
    session_id: Uuid,
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
                .is_some_and(|c| c.session_id == updater.session_id);
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

            let session_id = connection.session_id;
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
            .with_image_cooldown(telescope.image_cooldown_seconds.max(0) as u64);
            let handle = tokio::spawn(async move {
                updater.start_polling(UPDATER_POLL_INTERVAL).await;
            });
            running.insert(
                telescope_id,
                RunningUpdater {
                    session_id,
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

    /// Reconcile forever.
    pub async fn run(self: Arc<Self>) {
        loop {
            self.reconcile_once();
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
        let session = connections.get(id).unwrap().session_id;
        connections.remove_if_current(id, session);
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
    async fn reconnect_replaces_updater() {
        let (db, connections, manager, id) = setup();
        db.add_channel_route(id, 100, 42, "obs", "g", 1).unwrap();

        connect(&connections, id);
        assert_eq!(manager.reconcile_once(), (1, 0));

        // A new session takes the slot (rig reconnected).
        connect(&connections, id);
        assert_eq!(manager.reconcile_once(), (1, 1));
        assert_eq!(manager.running_count(), 1);
    }
}
