//! Server-Sent Events (SSE) for real-time job status streaming.
//!
//! - `EventBus`: per-job broadcast channels for publish/subscribe
//! - `stream_events`: SSE endpoint handler for `/v1/events/{id}`
//! - `ServeEvent`: typed event types pushed to subscribers

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream::Stream;
use serde::Serialize;
use tokio::sync::{broadcast, Mutex};
use tracing::debug;

/// Capacity of each per-job broadcast channel.
const CHANNEL_CAPACITY: usize = 64;

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

/// Per-job broadcast event bus.
///
/// Workers publish events, SSE clients subscribe.
/// Channels are lazily created and cleaned up when the last sender drops.
#[derive(Clone, Default)]
pub struct EventBus {
    pub(crate) channels: Arc<Mutex<HashMap<String, broadcast::Sender<ServeEvent>>>>,
}

impl EventBus {
    /// Get or create a broadcast sender for a job.
    pub async fn sender(&self, job_id: &str) -> broadcast::Sender<ServeEvent> {
        let mut map = self.channels.lock().await;
        map.entry(job_id.to_string())
            .or_insert_with(|| broadcast::channel(CHANNEL_CAPACITY).0)
            .clone()
    }

    /// Subscribe to events for a job. Returns a receiver.
    pub async fn subscribe(&self, job_id: &str) -> broadcast::Receiver<ServeEvent> {
        self.sender(job_id).await.subscribe()
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
/// Returns 404 if the job doesn't exist in storage and has no active
/// event channel (BUG-5: prevents orphan channel creation).
pub async fn stream_events(
    State(state): State<crate::state::AppState>,
    Path(job_id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, crate::error::ServeError> {
    // BUG-5: Verify job exists before creating a channel
    let has_channel = state.event_bus.channels.lock().await.contains_key(&job_id);
    if !has_channel {
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
    }

    let rx = state.event_bus.subscribe(&job_id).await;
    let storage = state.storage.clone();
    let id = job_id.clone();

    let stream = async_stream::stream! {
        // Check if job already has a terminal state (replay)
        if let Ok(Some(job)) = storage.get_job(&id).await {
            match job.state {
                nika_storage::JobState::Completed => {
                    let event = ServeEvent::Completed {
                        job_id: id.clone(),
                        output: job.output,
                    };
                    if let Ok(data) = serde_json::to_string(&event) {
                        yield Ok(Event::default().event("completed").data(data));
                    }
                    return;
                }
                nika_storage::JobState::Failed => {
                    let event = ServeEvent::Failed {
                        job_id: id.clone(),
                        error: job.output,
                    };
                    if let Ok(data) = serde_json::to_string(&event) {
                        yield Ok(Event::default().event("failed").data(data));
                    }
                    return;
                }
                nika_storage::JobState::Cancelled => {
                    let event = ServeEvent::Cancelled { job_id: id.clone() };
                    if let Ok(data) = serde_json::to_string(&event) {
                        yield Ok(Event::default().event("cancelled").data(data));
                    }
                    return;
                }
                nika_storage::JobState::Running => {
                    // Emit a started event for running jobs
                    let event = ServeEvent::Started { job_id: id.clone() };
                    if let Ok(data) = serde_json::to_string(&event) {
                        yield Ok(Event::default().event("started").data(data));
                    }
                }
                _ => {} // Pending — wait for events
            }
        }

        // Stream live events from the broadcast channel
        let mut rx = rx;
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let event_type = event.event_type();
                    let is_terminal = event.is_terminal();

                    if let Ok(data) = serde_json::to_string(&event) {
                        yield Ok(Event::default().event(event_type).data(data));
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

        // Publish an event
        let tx = bus.sender("job-1").await;
        tx.send(ServeEvent::Started {
            job_id: "job-1".into(),
        })
        .unwrap();

        let event = rx.recv().await.unwrap();
        assert!(matches!(event, ServeEvent::Started { .. }));
    }

    #[tokio::test]
    async fn event_bus_task_events_received() {
        let bus = EventBus::default();
        let mut rx = bus.subscribe("job-1").await;

        let tx = bus.sender("job-1").await;
        tx.send(ServeEvent::TaskStart {
            job_id: "job-1".into(),
            task_id: "step1".into(),
            verb: "infer".into(),
        })
        .unwrap();

        let event = rx.recv().await.unwrap();
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
}
