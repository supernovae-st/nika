// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
use http_body_util::BodyExt as _;
use hyper::body::{Frame, Incoming};
use hyper::header::{CACHE_CONTROL, CONTENT_TYPE, HeaderValue};
use hyper::{Request, Response, StatusCode};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::TryAcquireError;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::{JobEvent, JobId, JobReceipt, JobRecord, JobStatus, JobStoreError};

use super::error::{ApiError, ResponseBody};
use super::store::EventPage;
use super::{AppState, ServerError};

const SSE_BUFFER: usize = 8;

pub(super) fn is_events_path(path: &str) -> bool {
    events_job_id(path).is_some()
}

pub(super) async fn handle(
    request: Request<Incoming>,
    state: Arc<AppState>,
) -> Response<ResponseBody> {
    let path = request.uri().path().to_owned();
    let Some(raw_id) = events_job_id(&path) else {
        return ApiError::new(StatusCode::NOT_FOUND, "not_found", "route not found")
            .into_response();
    };
    let after = match last_event_id(request.headers()) {
        Ok(after) => after,
        Err(error) => return error.into_response(),
    };
    let Ok(id) = JobId::parse(raw_id) else {
        return ApiError::job_not_found().into_response();
    };
    if let Err(response) = preview_cursor(&state, id.clone(), after).await {
        return response;
    }
    let permit = match Arc::clone(&state.sse_slots).try_acquire_owned() {
        Ok(permit) => permit,
        Err(TryAcquireError::NoPermits) => return ApiError::sse_capacity().into_response(),
        Err(TryAcquireError::Closed) => return ApiError::internal().into_response(),
    };
    let (tx, rx) = mpsc::channel(SSE_BUFFER);
    let pump = tokio::spawn(pump_events(
        Arc::clone(&state),
        id,
        after,
        tx,
        state.limits.request_timeout(),
    ));
    sse_response(SseBody {
        receiver: rx,
        pump: Some(pump),
        _permit: permit,
    })
}

async fn preview_cursor(
    state: &AppState,
    id: JobId,
    after: u64,
) -> Result<EventPage, Response<ResponseBody>> {
    match state
        .store
        .events_after(id, after, state.event_page_limit)
        .await
    {
        Ok(page) => Ok(page),
        Err(ServerError::JobStore(JobStoreError::JobNotFound(_))) => {
            Err(ApiError::job_not_found().into_response())
        }
        Err(ServerError::JobStore(JobStoreError::CursorBeyondLatest { .. })) => {
            Err(ApiError::cursor_beyond_latest().into_response())
        }
        Err(ServerError::JobStore(JobStoreError::Busy) | ServerError::StoreQueueFull) => {
            Err(ApiError::store_busy().into_response())
        }
        Err(_) => Err(ApiError::internal().into_response()),
    }
}

fn sse_response(body: SseBody) -> Response<ResponseBody> {
    let mut response = Response::new(body.boxed_unsync());
    *response.status_mut() = StatusCode::OK;
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

fn events_job_id(path: &str) -> Option<&str> {
    let tail = path.strip_prefix("/v1/jobs/")?;
    let id = tail.strip_suffix("/events")?;
    (!id.is_empty() && !id.contains('/')).then_some(id)
}

fn last_event_id(headers: &hyper::HeaderMap) -> Result<u64, ApiError> {
    let mut values = headers.get_all("last-event-id").iter();
    let Some(value) = values.next() else {
        return Ok(0);
    };
    if values.next().is_some() {
        return Err(ApiError::invalid_cursor());
    }
    let value = value.to_str().map_err(|_| ApiError::invalid_cursor())?;
    parse_cursor(value)
}

fn parse_cursor(value: &str) -> Result<u64, ApiError> {
    let value = value.trim();
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ApiError::invalid_cursor());
    }
    if value.len() > 1 && value.starts_with('0') {
        return Err(ApiError::invalid_cursor());
    }
    value.parse().map_err(|_| ApiError::invalid_cursor())
}

/// A paused observation completes while leaving the job resumable.
fn observation_ended(status: JobStatus) -> bool {
    status.is_settled() || status == JobStatus::Paused
}

fn is_pause_boundary(event: &JobEvent) -> bool {
    string_field(event.payload(), "kind") == Some("execution.settled")
        && string_field(event.payload(), "status") == Some("paused")
}

#[derive(Debug, Serialize)]
struct ProjectedEvent<'a> {
    sequence: u64,
    kind: Option<&'a str>,
    status: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    outputs: Option<Cow<'a, BTreeMap<String, Value>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    receipt: Option<Cow<'a, JobReceipt>>,
    /// ADR-128 · the runtime's settlement, whole, on the terminal frame.
    #[serde(skip_serializing_if = "Option::is_none")]
    settlement: Option<&'a Value>,
}

fn projected<'a>(
    event: &'a JobEvent,
    record: &'a JobRecord,
    terminal_sequence: Option<u64>,
) -> ProjectedEvent<'a> {
    let terminal = terminal_sequence == Some(event.sequence());
    ProjectedEvent {
        sequence: event.sequence(),
        kind: string_field(event.payload(), "kind"),
        status: string_field(event.payload(), "status"),
        code: string_field(event.payload(), "code"),
        message: string_field(event.payload(), "message"),
        outputs: if terminal {
            record.outputs().map(Cow::Borrowed)
        } else if is_pause_boundary(event) {
            event
                .payload()
                .get("outputs")
                .cloned()
                .and_then(|value| serde_json::from_value(value).ok())
                .map(Cow::Owned)
        } else {
            None
        },
        receipt: if terminal {
            record.receipt().map(Cow::Borrowed)
        } else if is_pause_boundary(event) {
            event
                .payload()
                .get("receipt")
                .cloned()
                .and_then(|value| serde_json::from_value(value).ok())
                .map(Cow::Owned)
        } else {
            None
        },
        settlement: event.payload().get("settlement"),
    }
}

fn string_field<'a>(payload: &'a Value, key: &'a str) -> Option<&'a str> {
    payload.get(key).and_then(Value::as_str)
}

fn encode_frame(
    event: &JobEvent,
    record: &JobRecord,
    terminal_sequence: Option<u64>,
) -> Option<Bytes> {
    let json = serde_json::to_string(&projected(event, record, terminal_sequence)).ok()?;
    Some(Bytes::from(format!(
        "id: {}\ndata: {json}\n\n",
        event.sequence()
    )))
}

async fn pump_events(
    state: Arc<AppState>,
    id: JobId,
    mut after: u64,
    tx: mpsc::Sender<Bytes>,
    lag: Duration,
) {
    let notify = state.store.event_notify();
    let reconnect_ms = state.limits.sse_reconnect().as_millis();
    if !send_frame(&tx, Bytes::from(format!("retry: {reconnect_ms}\n\n")), lag).await {
        return;
    }
    loop {
        let notified = notify.clone().notified_owned();
        tokio::pin!(notified);
        notified.as_mut().enable();
        let Ok(page) = state
            .store
            .events_after(id.clone(), after, state.event_page_limit)
            .await
        else {
            return;
        };
        if page.events.is_empty() {
            if observation_ended(page.record.status()) {
                return;
            }
            tokio::select! {
                () = &mut notified => {}
                () = tokio::time::sleep(state.limits.sse_heartbeat()) => {
                    if !send_frame(&tx, Bytes::from_static(b": heartbeat\n\n"), lag).await {
                        return;
                    }
                }
            }
            continue;
        }
        if !emit_page(
            &tx,
            &page.events,
            &page.record,
            page.terminal_sequence,
            &mut after,
            lag,
        )
        .await
        {
            return;
        }
    }
}

async fn emit_page(
    tx: &mpsc::Sender<Bytes>,
    events: &[JobEvent],
    record: &JobRecord,
    terminal_sequence: Option<u64>,
    after: &mut u64,
    lag: Duration,
) -> bool {
    for event in events {
        let Some(frame) = encode_frame(event, record, terminal_sequence) else {
            continue;
        };
        if !send_frame(tx, frame, lag).await {
            return false;
        }
        *after = event.sequence();
        // Even if a later resume is already in this page, this stream owns
        // only the observation leg that ended at the pause. A new stream
        // can continue from this durable sequence.
        if is_pause_boundary(event) {
            return false;
        }
    }
    true
}

async fn send_frame(tx: &mpsc::Sender<Bytes>, frame: Bytes, lag: Duration) -> bool {
    match tx.try_send(frame) {
        Ok(()) => true,
        Err(mpsc::error::TrySendError::Closed(_)) => false,
        Err(mpsc::error::TrySendError::Full(frame)) => {
            matches!(tokio::time::timeout(lag, tx.send(frame)).await, Ok(Ok(())))
        }
    }
}

struct SseBody {
    receiver: mpsc::Receiver<Bytes>,
    pump: Option<JoinHandle<()>>,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl Drop for SseBody {
    fn drop(&mut self) {
        if let Some(pump) = self.pump.take() {
            pump.abort();
        }
    }
}

impl hyper::body::Body for SseBody {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match self.receiver.poll_recv(cx) {
            Poll::Ready(Some(data)) => Poll::Ready(Some(Ok(Frame::data(data)))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn events_path_requires_a_single_job_segment() {
        assert!(is_events_path(
            "/v1/jobs/01234567-89ab-4def-8123-456789abcdef/events"
        ));
        assert!(!is_events_path("/v1/jobs/x/events/extra"));
        assert!(!is_events_path("/v1/jobs/x/status"));
        assert!(!is_events_path("/v1/jobs/x/cancel"));
        assert!(!is_events_path("/v1/jobs/x/artifacts"));
    }

    #[test]
    fn cursor_rejects_non_decimal_and_padded_values() {
        assert_eq!(parse_cursor("0").expect("zero"), 0);
        assert_eq!(parse_cursor("12").expect("twelve"), 12);
        assert_eq!(parse_cursor(" 7 ").expect("trimmed"), 7);
        assert!(parse_cursor("").is_err());
        assert!(parse_cursor("01").is_err());
        assert!(parse_cursor("+1").is_err());
        assert!(parse_cursor("1.0").is_err());
        assert!(parse_cursor("nope").is_err());
        assert!(parse_cursor("-1").is_err());
    }

    #[test]
    fn projection_allowlists_sequence_kind_and_status_only() {
        let payload = json!({
            "kind": "interrupted",
            "status": "interrupted",
            "secret": "s3cret",
            "path": "/tmp/job.json",
            "incarnation_generation": 3
        });
        let json = serde_json::to_value(projected_from(&payload, 4)).expect("projected json");
        let object = json.as_object().expect("object");
        let mut keys = object.keys().cloned().collect::<Vec<_>>();
        keys.sort();
        assert_eq!(keys, ["kind", "sequence", "status"]);
        assert_eq!(json["sequence"], 4);
        assert_eq!(json["kind"], "interrupted");
        assert_eq!(json["status"], "interrupted");
        assert!(json.get("secret").is_none());
        assert!(json.get("path").is_none());
        assert!(json.get("incarnation_generation").is_none());
    }

    #[test]
    fn projection_forwards_redacted_failure_code_and_drops_paths() {
        let payload = json!({
            "kind": "execution.settled",
            "status": "failed",
            "code": "NIKA-ASSERT-001",
            "message": "task boom: expected true",
            "secret": "s3cret",
            "path": "/tmp/job.json"
        });
        let json = serde_json::to_value(projected_from(&payload, 2)).expect("projected json");
        assert_eq!(json["code"], "NIKA-ASSERT-001");
        assert_eq!(json["message"], "task boom: expected true");
        assert!(json.get("secret").is_none());
        assert!(json.get("path").is_none());
    }

    #[test]
    fn terminal_projection_uses_persisted_sequence_not_caller_shaped_fields() {
        let root = tempfile::tempdir().expect("root");
        let store = crate::JobStore::open(root.path()).expect("store");
        let admission = store
            .create_or_replay(
                crate::IdempotencyKey::new("sse-terminal-sequence").expect("key"),
                crate::RequestDigest::from_bytes([58; 32]),
            )
            .expect("create");
        let record = admission.record().clone();
        let snapshot_digest = crate::RequestDigest::from_bytes([59; 32])
            .as_str()
            .to_owned();
        store
            .stamp_execution_identity(
                record.id(),
                "execution-58".to_owned(),
                "trace-58".to_owned(),
                snapshot_digest.clone(),
            )
            .expect("stamp identity");
        store
            .transition_with_events(
                record.id(),
                JobStatus::Running,
                &[json!({"kind": "execution.settled", "status": "succeeded"})],
            )
            .expect("caller-shaped running event");
        let receipt = JobReceipt::new(
            record.id().clone(),
            "execution-58",
            "trace-58",
            snapshot_digest,
            None,
        )
        .expect("receipt");
        store
            .settle_with_events(
                record.id(),
                JobStatus::Succeeded,
                &[json!({"kind": "execution.finished", "status": "done"})],
                Some(BTreeMap::from([("answer".to_owned(), json!(58))])),
                Some(receipt),
            )
            .expect("terminal settlement");

        let (events, record, terminal_sequence) = store
            .event_page(
                record.id(),
                0,
                crate::EventPageLimit::new(crate::MAX_EVENT_PAGE_LEN).expect("page limit"),
            )
            .expect("event page");
        assert_eq!(terminal_sequence, Some(2));
        let projected = events
            .iter()
            .map(|event| {
                serde_json::to_value(super::projected(event, &record, terminal_sequence))
                    .expect("project event")
            })
            .collect::<Vec<_>>();
        assert!(projected[0].get("outputs").is_none());
        assert!(projected[0].get("receipt").is_none());
        assert_eq!(projected[1]["outputs"]["answer"], 58);
        assert_eq!(projected[1]["receipt"]["execution_id"], "execution-58");
        assert_eq!(
            projected
                .iter()
                .filter(|event| event.get("receipt").is_some())
                .count(),
            1
        );
    }

    fn paused_then_resumed_job() -> (tempfile::TempDir, crate::JobStore, crate::JobId) {
        let root = tempfile::tempdir().expect("root");
        let store = crate::JobStore::open(root.path()).expect("store");
        let admission = store
            .create_or_replay(
                crate::IdempotencyKey::new("paused-leg").expect("key"),
                crate::RequestDigest::from_bytes([61; 32]),
            )
            .expect("create");
        let id = admission.record().id();
        let digest = crate::RequestDigest::from_bytes([62; 32])
            .as_str()
            .to_owned();
        store
            .start_execution(
                id,
                "execution-61".to_owned(),
                "trace-61".to_owned(),
                digest.clone(),
                &[json!({"kind": "execution.started", "status": "running"})],
            )
            .expect("start");
        let paused_receipt = JobReceipt::new(
            id.clone(),
            "execution-61",
            "trace-61",
            digest.clone(),
            Some("paused-head".to_owned()),
        )
        .expect("receipt");
        store
            .transition_with_events(
                id,
                JobStatus::Paused,
                &[json!({
                    "kind": "execution.settled", "status": "paused",
                    "settlement": {"status": "paused", "cause": "human_gate"},
                    "outputs": {"draft": 42}, "receipt": paused_receipt,
                })],
            )
            .expect("pause");
        store
            .transition_with_events(
                id,
                JobStatus::Running,
                &[json!({"kind": "execution.started", "status": "running"})],
            )
            .expect("resume remains legal");
        let final_receipt = JobReceipt::new(
            id.clone(),
            "execution-61",
            "trace-61",
            digest,
            Some("final-head".to_owned()),
        )
        .expect("final receipt");
        store
            .settle_with_events(
                id,
                JobStatus::Succeeded,
                &[json!({"kind": "execution.settled", "status": "succeeded"})],
                Some(BTreeMap::from([("final".to_owned(), json!(84))])),
                Some(final_receipt),
            )
            .expect("settle resumed leg");
        (root, store, id.clone())
    }

    #[tokio::test]
    async fn a_pause_boundary_keeps_its_own_evidence_after_the_job_resumes() {
        let (_root, store, id) = paused_then_resumed_job();
        let limit = crate::EventPageLimit::new(crate::MAX_EVENT_PAGE_LEN).expect("limit");
        let (events, record, terminal) = store
            .event_page(&id, 0, limit)
            .expect("journal still verifies");
        let (tx, mut rx) = mpsc::channel(8);
        let mut after = 0;
        assert!(
            !emit_page(
                &tx,
                &events,
                &record,
                terminal,
                &mut after,
                Duration::from_secs(1)
            )
            .await
        );
        assert_eq!(after, 2, "first observation stops before the resumed leg");
        let _started = rx.try_recv().expect("started frame");
        let pause = rx.try_recv().expect("paused frame");
        assert!(rx.try_recv().is_err(), "later leg belongs to a new stream");
        let pause = String::from_utf8(pause.to_vec()).expect("utf8 frame");
        let data = pause
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .expect("data");
        let pause: Value = serde_json::from_str(data).expect("event");
        assert_eq!(pause["outputs"]["draft"], 42);
        assert!(pause["outputs"].get("final").is_none());
        assert_eq!(pause["receipt"]["chain_head"], "paused-head");
        assert_eq!(
            record.receipt().expect("final receipt").chain_head(),
            Some("final-head")
        );

        let (events, record, terminal) = store
            .event_page(&id, after, limit)
            .expect("next observation");
        assert!(
            emit_page(
                &tx,
                &events,
                &record,
                terminal,
                &mut after,
                Duration::from_secs(1)
            )
            .await
        );
        assert_eq!(after, 4, "cursor can observe the later resumed leg");
    }

    fn projected_from(payload: &serde_json::Value, sequence: u64) -> ProjectedEvent<'_> {
        ProjectedEvent {
            sequence,
            kind: string_field(payload, "kind"),
            status: string_field(payload, "status"),
            code: string_field(payload, "code"),
            message: string_field(payload, "message"),
            outputs: None,
            receipt: None,
            settlement: None,
        }
    }

    #[tokio::test]
    async fn lagged_buffer_drops_instead_of_blocking() {
        let (tx, _rx) = mpsc::channel(1);
        assert!(
            send_frame(&tx, Bytes::from_static(b"a"), Duration::from_millis(10)).await,
            "first frame fits"
        );
        let started = std::time::Instant::now();
        assert!(
            !send_frame(&tx, Bytes::from_static(b"b"), Duration::from_millis(20)).await,
            "full buffer must drop the slow client"
        );
        assert!(started.elapsed() < Duration::from_millis(200));
    }
}
