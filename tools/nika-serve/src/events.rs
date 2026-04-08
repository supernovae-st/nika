// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Server-Sent Events (SSE) for real-time job status streaming.
//!
//! - `EventBus`: per-job broadcast channels for publish/subscribe
//! - `stream_events`: SSE endpoint handler for `/v1/events/{id}`
//! - `ServeEvent`: typed event types pushed to subscribers

use std::collections::{HashMap, VecDeque};
use std::convert::Infallible;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream::Stream;
use serde::Serialize;
use tokio::sync::{broadcast, Mutex};
use tracing::debug;

/// Capacity of each per-job broadcast channel.
const CHANNEL_CAPACITY: usize = 64;

/// Max events kept in history ring buffer for reconnect replay.
const HISTORY_CAPACITY: usize = 256;

/// Typed events published by the worker and streamed to SSE clients.
///
/// Each variant maps to a distinct SSE `event:` type, allowing clients
/// to filter and handle events selectively.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type")]
pub enum ServeEvent {
    /// Job has started executing.
    #[serde(rename = "started")]
    Started { job_id: String },

    /// Individual task started within the workflow.
    #[serde(rename = "task_start")]
    TaskStart {
        job_id: String,
        task_id: String,
        verb: String,
    },

    /// Individual task completed successfully.
    #[serde(rename = "task_complete")]
    TaskComplete {
        job_id: String,
        task_id: String,
        duration_ms: u64,
    },

    /// Individual task failed.
    #[serde(rename = "task_failed")]
    TaskFailed {
        job_id: String,
        task_id: String,
        error: String,
        duration_ms: u64,
    },

    /// An artifact was written to disk.
    #[serde(rename = "artifact_written")]
    ArtifactWritten {
        job_id: String,
        task_id: String,
        path: String,
        size: u64,
    },

    /// Job completed successfully.
    #[serde(rename = "completed")]
    Completed {
        job_id: String,
        output: Option<String>,
    },

    /// Job failed.
    #[serde(rename = "failed")]
    Failed {
        job_id: String,
        error: Option<String>,
    },

    /// Job was cancelled.
    #[serde(rename = "cancelled")]
    Cancelled { job_id: String },
}

impl ServeEvent {
    /// SSE event type name for the `event:` field.
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Started { .. } => "started",
            Self::TaskStart { .. } => "task_start",
            Self::TaskComplete { .. } => "task_complete",
            Self::TaskFailed { .. } => "task_failed",
            Self::ArtifactWritten { .. } => "artifact_written",
            Self::Completed { .. } => "completed",
            Self::Failed { .. } => "failed",
            Self::Cancelled { .. } => "cancelled",
        }
    }

    /// Whether this is a terminal event (stream should end after).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed { .. } | Self::Failed { .. } | Self::Cancelled { .. }
        )
    }
}

/// State for a single job's event channel.
pub struct ChannelState {
    pub sender: broadcast::Sender<(u64, ServeEvent)>,
    pub counter: AtomicU64,
    /// Ring buffer of last N events for reconnect replay.
    pub history: Mutex<VecDeque<(u64, ServeEvent)>>,
}

/// Per-job broadcast event bus.
///
/// Workers publish events, SSE clients subscribe.
/// Channels are lazily created and cleaned up when the last sender drops.
#[derive(Clone, Default)]
pub struct EventBus {
    pub(crate) channels: Arc<Mutex<HashMap<String, Arc<ChannelState>>>>,
}

impl EventBus {
    /// Get or create a broadcast sender for a job.
    pub async fn sender(&self, job_id: &str) -> broadcast::Sender<(u64, ServeEvent)> {
        let mut map = self.channels.lock().await;
        let state = map.entry(job_id.to_string()).or_insert_with(|| {
            Arc::new(ChannelState {
                sender: broadcast::channel(CHANNEL_CAPACITY).0,
                counter: AtomicU64::new(0),
                history: Mutex::new(VecDeque::with_capacity(HISTORY_CAPACITY)),
            })
        });
        state.sender.clone()
    }

    /// Publish an event with an auto-incrementing ID and store in history.
    ///
    /// Acquires `channels` lock briefly to get/create the channel state,
    /// then drops it before touching `history` — avoids nested async locks.
    pub async fn publish(&self, job_id: &str, event: ServeEvent) {
        let state = {
            let mut map = self.channels.lock().await;
            Arc::clone(map.entry(job_id.to_string()).or_insert_with(|| {
                Arc::new(ChannelState {
                    sender: broadcast::channel(CHANNEL_CAPACITY).0,
                    counter: AtomicU64::new(0),
                    history: Mutex::new(VecDeque::with_capacity(HISTORY_CAPACITY)),
                })
            }))
            // channels lock released here
        };
        // Relaxed is safe: the Mutex provides happens-before ordering
        let id = state.counter.fetch_add(1, Ordering::Relaxed) + 1;
        {
            let mut hist = state.history.lock().await;
            if hist.len() >= HISTORY_CAPACITY {
                hist.pop_front();
            }
            hist.push_back((id, event.clone()));
        }
        let _ = state.sender.send((id, event));
    }

    /// Subscribe to events for a job. Returns a receiver.
    pub async fn subscribe(&self, job_id: &str) -> broadcast::Receiver<(u64, ServeEvent)> {
        self.sender(job_id).await.subscribe()
    }

    /// Subscribe to an existing channel without creating one.
    ///
    /// Returns `Some((receiver, channel_state))` if the channel exists, `None` otherwise.
    /// Existence check + subscribe happen in a single lock acquisition
    /// to prevent TOCTOU races (S10).
    pub async fn try_subscribe(
        &self,
        job_id: &str,
    ) -> Option<(broadcast::Receiver<(u64, ServeEvent)>, Arc<ChannelState>)> {
        let map = self.channels.lock().await;
        map.get(job_id)
            .map(|state| (state.sender.subscribe(), Arc::clone(state)))
    }

    /// Remove the channel for a job (called after job completes).
    pub async fn remove(&self, job_id: &str) {
        self.channels.lock().await.remove(job_id);
    }
}

/// `GET /v1/events/{id}` — Stream job events via Server-Sent Events.
///
/// Subscribes to the event bus for the given job ID and streams events
/// as they occur. Includes a keep-alive ping every 15 seconds.
///
/// Supports reconnection via `Last-Event-Id` header: missed events are
/// replayed from an in-memory ring buffer (last 256 events per job).
///
/// Returns 404 if the job doesn't exist in storage and has no active
/// event channel (BUG-5: prevents orphan channel creation).
pub async fn stream_events(
    State(state): State<crate::state::AppState>,
    principal: Option<axum::Extension<crate::token_store::Principal>>,
    Path(job_id): Path<String>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, crate::error::ServeError> {
    // L2 scope enforcement: fail-closed — if we can't verify scope, deny access
    if let Some(axum::Extension(ref p)) = principal {
        let job = state
            .storage
            .get_job(&job_id)
            .await
            .map_err(crate::error::ServeError::Storage)?
            .ok_or(crate::error::ServeError::NotFound)?;
        if !p.can_access(&job.workflow) {
            return Err(crate::error::ServeError::Forbidden(format!(
                "token '{}' scope '{}' does not cover workflow '{}'",
                p.token_name, p.scope, job.workflow
            )));
        }
    }

    // Parse Last-Event-Id for reconnect replay
    let last_event_id: Option<u64> = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok());

    // BUG-5 + S10: Check existence + subscribe in one lock to prevent TOCTOU.
    let (rx, channel_state) = match state.event_bus.try_subscribe(&job_id).await {
        Some((rx, state)) => (rx, Some(state)),
        None => {
            let job_exists = state
                .storage
                .get_job(&job_id)
                .await
                .ok()
                .flatten()
                .is_some();
            if !job_exists {
                return Err(crate::error::ServeError::NotFound);
            }
            let rx = state.event_bus.subscribe(&job_id).await;
            (rx, None)
        }
    };
    let storage = state.storage.clone();
    let id = job_id.clone();

    let stream = async_stream::stream! {
        // Track the highest event ID we've replayed, to avoid duplicates
        // in the live broadcast stream that may still have these events buffered.
        let mut skip_up_to: u64 = last_event_id.unwrap_or(0);

        // Replay missed events from history if reconnecting
        if let (Some(last_id), Some(ref ch)) = (last_event_id, &channel_state) {
            let hist = ch.history.lock().await;
            for (event_id, event) in hist.iter() {
                if *event_id > last_id {
                    if let Ok(data) = serde_json::to_string(event) {
                        yield Ok(Event::default()
                            .event(event.event_type())
                            .data(data)
                            .id(event_id.to_string()));
                    }
                    skip_up_to = *event_id;
                    if event.is_terminal() {
                        debug!(job_id = %id, "SSE replay hit terminal event");
                        return;
                    }
                }
            }
        }

        // Check if job already has a terminal state (replay for fresh connections)
        if last_event_id.is_none() {
            if let Ok(Some(job)) = storage.get_job(&id).await {
                match job.state {
                    nika_storage::JobState::Completed => {
                        let event = ServeEvent::Completed {
                            job_id: id.clone(),
                            output: job.output,
                        };
                        if let Ok(data) = serde_json::to_string(&event) {
                            yield Ok(Event::default().event("completed").data(data).id("0"));
                        }
                        return;
                    }
                    nika_storage::JobState::Failed => {
                        let event = ServeEvent::Failed {
                            job_id: id.clone(),
                            error: job.output,
                        };
                        if let Ok(data) = serde_json::to_string(&event) {
                            yield Ok(Event::default().event("failed").data(data).id("0"));
                        }
                        return;
                    }
                    nika_storage::JobState::Cancelled => {
                        let event = ServeEvent::Cancelled { job_id: id.clone() };
                        if let Ok(data) = serde_json::to_string(&event) {
                            yield Ok(Event::default().event("cancelled").data(data).id("0"));
                        }
                        return;
                    }
                    nika_storage::JobState::Running => {
                        let event = ServeEvent::Started { job_id: id.clone() };
                        if let Ok(data) = serde_json::to_string(&event) {
                            yield Ok(Event::default().event("started").data(data).id("0"));
                        }
                    }
                    _ => {} // Pending — wait for events
                }
            }
        }

        // Stream live events from the broadcast channel
        let mut rx = rx;
        loop {
            match rx.recv().await {
                Ok((event_id, event)) => {
                    // Skip events already replayed from history
                    if event_id <= skip_up_to {
                        continue;
                    }

                    let event_type = event.event_type();
                    let is_terminal = event.is_terminal();

                    if let Ok(data) = serde_json::to_string(&event) {
                        yield Ok(Event::default()
                            .event(event_type)
                            .data(data)
                            .id(event_id.to_string()));
                    }

                    if is_terminal {
                        debug!(job_id = %id, "SSE stream ending (terminal event)");
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    debug!(job_id = %id, skipped = n, "SSE client lagged, skipping events");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    debug!(job_id = %id, "SSE stream ending (channel closed)");
                    break;
                }
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serve_event_serializes_with_type_tag() {
        let event = ServeEvent::Started {
            job_id: "abc123".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"started"#));
        assert!(json.contains(r#""job_id":"abc123"#));
    }

    #[test]
    fn serve_event_completed_with_output() {
        let event = ServeEvent::Completed {
            job_id: "x".into(),
            output: Some("result data".into()),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"completed"#));
        assert!(json.contains(r#""output":"result data"#));
    }

    #[test]
    fn task_start_event_has_verb() {
        let event = ServeEvent::TaskStart {
            job_id: "j1".into(),
            task_id: "step1".into(),
            verb: "infer".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"task_start"#));
        assert!(json.contains(r#""task_id":"step1"#));
        assert!(json.contains(r#""verb":"infer"#));
    }

    #[test]
    fn task_complete_event_has_duration() {
        let event = ServeEvent::TaskComplete {
            job_id: "j1".into(),
            task_id: "step1".into(),
            duration_ms: 1200,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"task_complete"#));
        assert!(json.contains(r#""duration_ms":1200"#));
    }

    #[test]
    fn artifact_written_event() {
        let event = ServeEvent::ArtifactWritten {
            job_id: "j1".into(),
            task_id: "report".into(),
            path: "output/report.md".into(),
            size: 4096,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"artifact_written"#));
        assert!(json.contains(r#""path":"output/report.md"#));
        assert!(json.contains(r#""size":4096"#));
    }

    #[test]
    fn event_type_returns_correct_name() {
        assert_eq!(
            ServeEvent::Started { job_id: "".into() }.event_type(),
            "started"
        );
        assert_eq!(
            ServeEvent::TaskStart {
                job_id: "".into(),
                task_id: "".into(),
                verb: "".into()
            }
            .event_type(),
            "task_start"
        );
        assert_eq!(
            ServeEvent::Completed {
                job_id: "".into(),
                output: None
            }
            .event_type(),
            "completed"
        );
    }

    #[test]
    fn terminal_events_detected() {
        assert!(!ServeEvent::Started { job_id: "".into() }.is_terminal());
        assert!(!ServeEvent::TaskStart {
            job_id: "".into(),
            task_id: "".into(),
            verb: "".into()
        }
        .is_terminal());
        assert!(ServeEvent::Completed {
            job_id: "".into(),
            output: None
        }
        .is_terminal());
        assert!(ServeEvent::Failed {
            job_id: "".into(),
            error: None
        }
        .is_terminal());
        assert!(ServeEvent::Cancelled { job_id: "".into() }.is_terminal());
    }

    #[tokio::test]
    async fn event_bus_subscribe_and_receive() {
        let bus = EventBus::default();
        let mut rx = bus.subscribe("job-1").await;

        bus.publish(
            "job-1",
            ServeEvent::Started {
                job_id: "job-1".into(),
            },
        )
        .await;

        let (id, event) = rx.recv().await.unwrap();
        assert_eq!(id, 1);
        assert!(matches!(event, ServeEvent::Started { .. }));
    }

    #[tokio::test]
    async fn event_bus_task_events_received() {
        let bus = EventBus::default();
        let mut rx = bus.subscribe("job-1").await;

        bus.publish(
            "job-1",
            ServeEvent::TaskStart {
                job_id: "job-1".into(),
                task_id: "step1".into(),
                verb: "infer".into(),
            },
        )
        .await;

        let (id, event) = rx.recv().await.unwrap();
        assert_eq!(id, 1);
        assert!(matches!(event, ServeEvent::TaskStart { .. }));
    }

    #[tokio::test]
    async fn event_bus_remove_cleans_up() {
        let bus = EventBus::default();
        let _rx = bus.subscribe("job-2").await;
        bus.remove("job-2").await;

        let map = bus.channels.lock().await;
        assert!(!map.contains_key("job-2"));
    }

    #[tokio::test]
    async fn sse_events_have_incrementing_ids() {
        let bus = EventBus::default();
        let mut rx = bus.subscribe("job-3").await;

        bus.publish(
            "job-3",
            ServeEvent::Started {
                job_id: "job-3".into(),
            },
        )
        .await;
        bus.publish(
            "job-3",
            ServeEvent::TaskStart {
                job_id: "job-3".into(),
                task_id: "s1".into(),
                verb: "infer".into(),
            },
        )
        .await;
        bus.publish(
            "job-3",
            ServeEvent::Completed {
                job_id: "job-3".into(),
                output: None,
            },
        )
        .await;

        let (id1, _) = rx.recv().await.unwrap();
        let (id2, _) = rx.recv().await.unwrap();
        let (id3, _) = rx.recv().await.unwrap();
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(id3, 3);
    }

    #[tokio::test]
    async fn sse_reconnect_replays_from_last_id() {
        let bus = EventBus::default();

        for i in 0..5 {
            bus.publish(
                "job-4",
                ServeEvent::TaskStart {
                    job_id: "job-4".into(),
                    task_id: format!("s{i}"),
                    verb: "infer".into(),
                },
            )
            .await;
        }

        let (_, ch) = bus.try_subscribe("job-4").await.unwrap();
        let hist = ch.history.lock().await;

        let replayed: Vec<u64> = hist
            .iter()
            .filter(|(id, _)| *id > 3)
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(replayed, vec![4, 5]);
    }

    #[tokio::test]
    async fn sse_history_bounded() {
        let bus = EventBus::default();

        for i in 0..300 {
            bus.publish(
                "job-5",
                ServeEvent::TaskStart {
                    job_id: "job-5".into(),
                    task_id: format!("s{i}"),
                    verb: "infer".into(),
                },
            )
            .await;
        }

        let (_, ch) = bus.try_subscribe("job-5").await.unwrap();
        let hist = ch.history.lock().await;
        assert_eq!(hist.len(), HISTORY_CAPACITY);
        assert_eq!(hist.front().unwrap().0, 45);
        assert_eq!(hist.back().unwrap().0, 300);
    }
}
