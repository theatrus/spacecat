//! Live Discord guild checks backing web authorization.
//!
//! The OAuth-time guild snapshot answers "could this user manage this guild
//! when they logged in". These checks answer "is that still true": the bot
//! must be installed in the guild, and the user must still be a member.
//! Positive results are cached briefly; negatives never are, so revocation
//! takes effect immediately.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const POSITIVE_CACHE_TTL: Duration = Duration::from_secs(60);

#[async_trait]
pub trait GuildChecker: Send + Sync {
    /// Is the bot installed in this guild?
    async fn bot_in_guild(&self, guild_id: u64) -> bool;
    /// Is this user currently a member of this guild?
    async fn user_in_guild(&self, guild_id: u64, user_id: u64) -> bool;
    /// Does this user currently hold guild management rights (owner,
    /// ADMINISTRATOR, or MANAGE_GUILD via any role)? Unlike the OAuth
    /// snapshot, this reflects demotions immediately.
    async fn user_can_manage(&self, guild_id: u64, user_id: u64) -> bool;
}

/// Production checker using the bot token over Discord's REST API. No
/// gateway connection is needed for these lookups.
pub struct SerenityGuildChecker {
    http: serenity::http::Http,
}

impl SerenityGuildChecker {
    pub fn new(bot_token: &str) -> Self {
        Self {
            http: serenity::http::Http::new(bot_token),
        }
    }
}

#[async_trait]
impl GuildChecker for SerenityGuildChecker {
    async fn bot_in_guild(&self, guild_id: u64) -> bool {
        // A bot can only fetch guilds it is a member of.
        self.http
            .get_guild(serenity::model::id::GuildId::new(guild_id))
            .await
            .is_ok()
    }

    async fn user_in_guild(&self, guild_id: u64, user_id: u64) -> bool {
        self.http
            .get_member(
                serenity::model::id::GuildId::new(guild_id),
                serenity::model::id::UserId::new(user_id),
            )
            .await
            .is_ok()
    }

    async fn user_can_manage(&self, guild_id: u64, user_id: u64) -> bool {
        use serenity::model::Permissions;
        use serenity::model::id::{GuildId, UserId};

        let guild = match self.http.get_guild(GuildId::new(guild_id)).await {
            Ok(guild) => guild,
            Err(_) => return false,
        };
        if guild.owner_id.get() == user_id {
            return true;
        }
        let member = match self
            .http
            .get_member(GuildId::new(guild_id), UserId::new(user_id))
            .await
        {
            Ok(member) => member,
            Err(_) => return false,
        };
        let manage = Permissions::ADMINISTRATOR | Permissions::MANAGE_GUILD;
        // The @everyone role (id == guild id) applies to every member.
        let everyone = guild
            .roles
            .get(&serenity::model::id::RoleId::new(guild_id))
            .is_some_and(|role| role.permissions.intersects(manage));
        everyone
            || member.roles.iter().any(|role_id| {
                guild
                    .roles
                    .get(role_id)
                    .is_some_and(|role| role.permissions.intersects(manage))
            })
    }
}

#[derive(Hash, PartialEq, Eq)]
enum CacheKey {
    Bot(u64),
    Member(u64, u64),
    Manage(u64, u64),
}

/// Wraps any checker with a positive-only TTL cache.
pub struct CachedGuildChecker<C> {
    inner: C,
    ttl: Duration,
    cache: Mutex<HashMap<CacheKey, Instant>>,
}

impl<C> CachedGuildChecker<C> {
    pub fn new(inner: C) -> Self {
        Self::with_ttl(inner, POSITIVE_CACHE_TTL)
    }

    pub fn with_ttl(inner: C, ttl: Duration) -> Self {
        Self {
            inner,
            ttl,
            cache: Mutex::new(HashMap::new()),
        }
    }

    fn cached(&self, key: &CacheKey) -> bool {
        let Ok(mut cache) = self.cache.lock() else {
            return false;
        };
        // Opportunistic eviction keeps the map bounded.
        let ttl = self.ttl;
        cache.retain(|_, at| at.elapsed() < ttl);
        cache.contains_key(key)
    }

    fn remember(&self, key: CacheKey) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(key, Instant::now());
        }
    }
}

#[async_trait]
impl<C: GuildChecker> GuildChecker for CachedGuildChecker<C> {
    async fn bot_in_guild(&self, guild_id: u64) -> bool {
        let key = CacheKey::Bot(guild_id);
        if self.cached(&key) {
            return true;
        }
        let ok = self.inner.bot_in_guild(guild_id).await;
        if ok {
            self.remember(key);
        }
        ok
    }

    async fn user_in_guild(&self, guild_id: u64, user_id: u64) -> bool {
        let key = CacheKey::Member(guild_id, user_id);
        if self.cached(&key) {
            return true;
        }
        let ok = self.inner.user_in_guild(guild_id, user_id).await;
        if ok {
            self.remember(key);
        }
        ok
    }

    async fn user_can_manage(&self, guild_id: u64, user_id: u64) -> bool {
        let key = CacheKey::Manage(guild_id, user_id);
        if self.cached(&key) {
            return true;
        }
        let ok = self.inner.user_can_manage(guild_id, user_id).await;
        if ok {
            self.remember(key);
        }
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingChecker {
        calls: AtomicUsize,
        answer: bool,
    }

    #[async_trait]
    impl GuildChecker for CountingChecker {
        async fn bot_in_guild(&self, _guild_id: u64) -> bool {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.answer
        }
        async fn user_in_guild(&self, _guild_id: u64, _user_id: u64) -> bool {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.answer
        }
        async fn user_can_manage(&self, _guild_id: u64, _user_id: u64) -> bool {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.answer
        }
    }

    #[tokio::test]
    async fn positive_results_cached() {
        let checker = CachedGuildChecker::new(CountingChecker {
            calls: AtomicUsize::new(0),
            answer: true,
        });
        assert!(checker.user_in_guild(1, 2).await);
        assert!(checker.user_in_guild(1, 2).await);
        assert_eq!(checker.inner.calls.load(Ordering::SeqCst), 1);
        // A different key misses the cache.
        assert!(checker.bot_in_guild(1).await);
        assert_eq!(checker.inner.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn negative_results_not_cached() {
        let checker = CachedGuildChecker::new(CountingChecker {
            calls: AtomicUsize::new(0),
            answer: false,
        });
        assert!(!checker.user_in_guild(1, 2).await);
        assert!(!checker.user_in_guild(1, 2).await);
        assert_eq!(checker.inner.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn cache_expires() {
        let checker = CachedGuildChecker::with_ttl(
            CountingChecker {
                calls: AtomicUsize::new(0),
                answer: true,
            },
            Duration::from_millis(0),
        );
        assert!(checker.user_in_guild(1, 2).await);
        assert!(checker.user_in_guild(1, 2).await);
        assert_eq!(checker.inner.calls.load(Ordering::SeqCst), 2);
    }
}
