use crate::api::SpaceCatApiClient;
use crate::events::Event;
use std::collections::HashSet;
use std::time::Duration;
use tokio::time::{Instant, sleep};

#[derive(Debug)]
pub struct EventPoller {
    client: SpaceCatApiClient,
    seen_events: HashSet<String>,
    poll_interval: Duration,
    last_poll_time: Option<Instant>,
}

#[derive(Debug)]
pub struct PollResult {
    pub new_events: Vec<Event>,
    pub total_events: usize,
    pub poll_duration: Duration,
}

impl EventPoller {
    /// Create a new event poller with the given client and poll interval
    pub fn new(client: SpaceCatApiClient, poll_interval: Duration) -> Self {
        Self {
            client,
            seen_events: HashSet::new(),
            poll_interval,
            last_poll_time: None,
        }
    }

    /// Create a new event poller with default 5-second poll interval
    pub fn new_with_default_interval(client: SpaceCatApiClient) -> Self {
        Self::new(client, Duration::from_secs(5))
    }

    /// Poll for new events and return only events not seen before
    pub async fn poll_new_events(&mut self) -> Result<PollResult, Box<dyn std::error::Error>> {
        let start_time = Instant::now();

        // Respect poll interval
        if let Some(last_poll) = self.last_poll_time {
            let elapsed = start_time.duration_since(last_poll);
            if elapsed < self.poll_interval {
                let sleep_duration = self.poll_interval - elapsed;
                sleep(sleep_duration).await;
            }
        }

        // Fetch current events
        let response = self.client.get_event_history().await?;
        let poll_duration = start_time.elapsed();
        self.last_poll_time = Some(Instant::now());

        // Find new events
        let new_events = self.find_new_events(&response.response);

        Ok(PollResult {
            new_events,
            total_events: response.response.len(),
            poll_duration,
        })
    }

    /// Poll continuously and call a callback function for each batch of new events
    pub async fn poll_continuously<F, Fut>(
        &mut self,
        mut callback: F,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        F: FnMut(PollResult) -> Fut,
        Fut: std::future::Future<Output = Result<(), Box<dyn std::error::Error>>>,
    {
        loop {
            match self.poll_new_events().await {
                Ok(result) => {
                    if !result.new_events.is_empty()
                        && let Err(e) = callback(result).await
                    {
                        eprintln!("Callback error: {e}");
                    }
                }
                Err(e) => {
                    eprintln!("Polling error: {e}");
                    // Sleep longer on error to avoid spamming
                    sleep(Duration::from_secs(30)).await;
                }
            }
        }
    }

    /// Get the number of unique events seen so far
    pub fn seen_event_count(&self) -> usize {
        self.seen_events.len()
    }

    /// Clear the seen events cache (useful for testing or reset scenarios)
    pub fn reset_seen_events(&mut self) {
        self.seen_events.clear();
    }

    /// Set a new poll interval
    pub fn set_poll_interval(&mut self, interval: Duration) {
        self.poll_interval = interval;
    }

    /// Get the current poll interval
    pub fn poll_interval(&self) -> Duration {
        self.poll_interval
    }

    /// Find new events by comparing against previously seen events
    fn find_new_events(&mut self, events: &[Event]) -> Vec<Event> {
        let mut new_events = Vec::new();

        for event in events {
            let event_key = self.create_event_key(event);

            if !self.seen_events.contains(&event_key) {
                self.seen_events.insert(event_key);
                new_events.push(event.clone());
            }
        }

        new_events
    }

    /// Create a unique key for an event based on timestamp and event type
    /// This handles the case where events might not have unique IDs
    fn create_event_key(&self, event: &Event) -> String {
        match &event.details {
            Some(details) => {
                // For events with details, include them in the key for uniqueness
                format!("{}:{}:{:?}", event.time, event.event, details)
            }
            None => {
                // For simple events, use time and event type
                format!("{}:{}", event.time, event.event)
            }
        }
    }
}

impl PollResult {
    /// Check if any new events were found
    pub fn has_new_events(&self) -> bool {
        !self.new_events.is_empty()
    }

    /// Get events of a specific type from the new events
    pub fn get_events_by_type(&self, event_type: &str) -> Vec<&Event> {
        self.new_events
            .iter()
            .filter(|event| event.event == event_type)
            .collect()
    }

    /// Get summary statistics about the poll result
    pub fn summary(&self) -> String {
        format!(
            "Poll completed in {:?}: {} new events out of {} total",
            self.poll_duration,
            self.new_events.len(),
            self.total_events
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::SpaceCatApiClient;
    use crate::config::ApiConfig;

    #[tokio::test]
    async fn test_event_key_creation() {
        let config = ApiConfig::default();
        let client = SpaceCatApiClient::new(config).unwrap();
        let poller = EventPoller::new_with_default_interval(client);

        let event = Event {
            time: "2023-01-01T12:00:00".to_string(),
            event: "TEST-EVENT".to_string(),
            details: None,
        };

        let key = poller.create_event_key(&event);
        assert_eq!(key, "2023-01-01T12:00:00:TEST-EVENT");
    }

    #[test]
    fn test_poll_result_helpers() {
        let events = vec![
            Event {
                time: "2023-01-01T12:00:00".to_string(),
                event: "IMAGE-SAVE".to_string(),
                details: None,
            },
            Event {
                time: "2023-01-01T12:00:01".to_string(),
                event: "FILTERWHEEL-CHANGED".to_string(),
                details: None,
            },
        ];

        let result = PollResult {
            new_events: events,
            total_events: 100,
            poll_duration: Duration::from_millis(250),
        };

        assert!(result.has_new_events());
        assert_eq!(result.get_events_by_type("IMAGE-SAVE").len(), 1);
        assert_eq!(result.get_events_by_type("NONEXISTENT").len(), 0);
        assert!(result.summary().contains("2 new events"));
    }
}
