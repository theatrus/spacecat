//! Small fixed-window rate limiter keyed by string (usually a client IP).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct RateLimiter {
    max_per_window: u32,
    window: Duration,
    hits: Mutex<HashMap<String, (Instant, u32)>>,
}

impl RateLimiter {
    pub fn new(max_per_window: u32, window: Duration) -> Self {
        Self {
            max_per_window,
            window,
            hits: Mutex::new(HashMap::new()),
        }
    }

    /// Is `key` already at or over the limit? Does not record a hit.
    pub fn blocked(&self, key: &str) -> bool {
        let Ok(hits) = self.hits.lock() else {
            return false;
        };
        hits.get(key).is_some_and(|(start, count)| {
            start.elapsed() < self.window && *count >= self.max_per_window
        })
    }

    /// Record a hit for `key` and return whether it is within the limit.
    pub fn check(&self, key: &str) -> bool {
        let Ok(mut hits) = self.hits.lock() else {
            return true;
        };
        let now = Instant::now();
        // Opportunistic eviction keeps the map bounded.
        let window = self.window;
        hits.retain(|_, (start, _)| now.duration_since(*start) < window);
        let entry = hits.entry(key.to_string()).or_insert((now, 0));
        if now.duration_since(entry.0) >= window {
            *entry = (now, 0);
        }
        entry.1 += 1;
        entry.1 <= self.max_per_window
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_up_to_limit_then_blocks() {
        let limiter = RateLimiter::new(3, Duration::from_secs(60));
        assert!(limiter.check("a"));
        assert!(limiter.check("a"));
        assert!(limiter.check("a"));
        assert!(!limiter.check("a"));
        // Another key is unaffected.
        assert!(limiter.check("b"));
    }

    #[test]
    fn window_resets() {
        let limiter = RateLimiter::new(1, Duration::from_millis(0));
        assert!(limiter.check("a"));
        // Zero-length window: every call starts a fresh window.
        assert!(limiter.check("a"));
    }
}
