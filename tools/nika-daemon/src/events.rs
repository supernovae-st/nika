// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Event bus — broadcast channel for daemon events.
//!
//! The event bus allows clients (TUI, CLI) to subscribe to real-time
//! daemon events like job state changes, cache hits, and watch triggers.
//! Uses tokio::sync::broadcast for multiple subscribers.

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// Default channel capacity.
const CHANNEL_CAPACITY: usize = 256;

/// A daemon event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DaemonEvent {
    // ── Jobs ────────────────────────────────────────────────────────────
    JobSubmitted { job_id: String, workflow: String },
    JobStarted { job_id: String },
    JobCompleted { job_id: String, exit_code: i32 },
    JobFailed { job_id: String, error: String },
    JobCancelled { job_id: String },

    // ── Cache ───────────────────────────────────────────────────────────
    CacheHit { key: String, model: String },
    CacheMiss { key: String, model: String },

    // ── Watch ───────────────────────────────────────────────────────────
    WatchTriggered { path: String },
    WatchStarted { dir: String, patterns: Vec<String> },
    WatchStopped,

    // ── Daemon lifecycle ────────────────────────────────────────────────
    DaemonStarted { pid: u32, version: String },
    DaemonStopping,
}

/// The event bus.
pub struct EventBus {
    tx: broadcast::Sender<DaemonEvent>,
}

impl EventBus {
    /// Create a new event bus.
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self { tx }
    }

    /// Publish an event to all subscribers.
    pub fn publish(&self, event: DaemonEvent) {
        // Ignore send errors (no subscribers = ok)
        let _ = self.tx.send(event);
    }

    /// Subscribe to events. Returns a receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
        self.tx.subscribe()
    }

    /// Get the number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_bus_publish_no_subscribers_ok() {
        let bus = EventBus::new();
        // Should not panic even with no subscribers
        bus.publish(DaemonEvent::DaemonStarted {
            pid: 1234,
            version: "0.46.1".into(),
        });
    }

    #[tokio::test]
    async fn event_bus_single_subscriber() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();

        bus.publish(DaemonEvent::JobSubmitted {
            job_id: "j1".into(),
            workflow: "test.nika.yaml".into(),
        });

        let event = rx.recv().await.unwrap();
        match event {
            DaemonEvent::JobSubmitted { job_id, workflow } => {
                assert_eq!(job_id, "j1");
                assert_eq!(workflow, "test.nika.yaml");
            }
            _ => panic!("unexpected event"),
        }
    }

    #[tokio::test]
    async fn event_bus_multiple_subscribers() {
        let bus = EventBus::new();
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        bus.publish(DaemonEvent::CacheHit {
            key: "k1".into(),
            model: "sonnet".into(),
        });

        // Both subscribers receive the event
        assert!(rx1.recv().await.is_ok());
        assert!(rx2.recv().await.is_ok());
    }

    #[test]
    fn event_bus_subscriber_count() {
        let bus = EventBus::new();
        assert_eq!(bus.subscriber_count(), 0);

        let _rx1 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 1);

        let _rx2 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 2);

        drop(_rx1);
        assert_eq!(bus.subscriber_count(), 1);
    }

    #[test]
    fn daemon_event_serialize_roundtrip() {
        let events = vec![
            DaemonEvent::JobSubmitted {
                job_id: "j1".into(),
                workflow: "w.nika.yaml".into(),
            },
            DaemonEvent::JobStarted {
                job_id: "j1".into(),
            },
            DaemonEvent::JobCompleted {
                job_id: "j1".into(),
                exit_code: 0,
            },
            DaemonEvent::JobFailed {
                job_id: "j1".into(),
                error: "timeout".into(),
            },
            DaemonEvent::JobCancelled {
                job_id: "j1".into(),
            },
            DaemonEvent::CacheHit {
                key: "k".into(),
                model: "m".into(),
            },
            DaemonEvent::CacheMiss {
                key: "k".into(),
                model: "m".into(),
            },
            DaemonEvent::WatchTriggered {
                path: "/a/b.nika.yaml".into(),
            },
            DaemonEvent::WatchStarted {
                dir: ".".into(),
                patterns: vec!["*.nika.yaml".into()],
            },
            DaemonEvent::WatchStopped,
            DaemonEvent::DaemonStarted {
                pid: 1,
                version: "0.46.1".into(),
            },
            DaemonEvent::DaemonStopping,
        ];

        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            let decoded: DaemonEvent = serde_json::from_str(&json).unwrap();
            // Verify roundtrip doesn't panic
            let _ = serde_json::to_string(&decoded).unwrap();
        }
    }

    #[tokio::test]
    async fn event_bus_dropped_subscriber_doesnt_block() {
        let bus = EventBus::new();
        let rx = bus.subscribe();
        drop(rx); // Drop subscriber

        // Publishing should still work
        bus.publish(DaemonEvent::DaemonStopping);
    }
}
