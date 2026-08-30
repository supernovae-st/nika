// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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

fn is_terminal(status: JobStatus) -> bool {
    matches!(
        status,
        JobStatus::Succeeded | JobStatus::Failed | JobStatus::Interrupted
    )
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
    outputs: Option<&'a BTreeMap<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    receipt: Option<&'a JobReceipt>,
}

fn projected<'a>(event: &'a JobEvent, record: &'a JobRecord) -> ProjectedEvent<'a> {
    let terminal = event_settles_record(event, record);
    ProjectedEvent {
        sequence: event.sequence(),
        kind: string_field(event.payload(), "kind"),
        status: string_field(event.payload(), "status"),
        code: string_field(event.payload(), "code"),
        message: string_field(event.payload(), "message"),
        outputs: if terminal { record.outputs() } else { None },
        receipt: if terminal { record.receipt() } else { None },
    }
}

fn event_settles_record(event: &JobEvent, record: &JobRecord) -> bool {
    let kind = string_field(event.payload(), "kind");
    let status = string_field(event.payload(), "status");
    record.status().is_settled()
        && match record.status() {
            JobStatus::Succeeded => {
                kind == Some("execution.settled") && status == Some("succeeded")
            }
            JobStatus::Failed => kind == Some("execution.settled") && status == Some("failed"),
            JobStatus::Interrupted => {
                matches!(kind, Some("execution.interrupted" | "interrupted"))
                    && status == Some("interrupted")
            }
            JobStatus::Queued | JobStatus::Running | JobStatus::Paused => false,
        }
}

fn string_field<'a>(payload: &'a Value, key: &'a str) -> Option<&'a str> {
    payload.get(key).and_then(Value::as_str)
}

fn encode_frame(event: &JobEvent, record: &JobRecord) -> Option<Bytes> {
    let json = serde_json::to_string(&projected(event, record)).ok()?;
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
            if is_terminal(page.record.status()) {
                return;
            }
            notified.await;
            continue;
        }
        if !emit_page(&tx, &page.events, &page.record, &mut after, lag).await {
            return;
        }
    }
}

async fn emit_page(
    tx: &mpsc::Sender<Bytes>,
    events: &[JobEvent],
    record: &JobRecord,
    after: &mut u64,
    lag: Duration,
) -> bool {
    for event in events {
        let Some(frame) = encode_frame(event, record) else {
            continue;
        };
        if !send_frame(tx, frame, lag).await {
            return false;
        }
        *after = event.sequence();
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

    fn projected_from(payload: &serde_json::Value, sequence: u64) -> ProjectedEvent<'_> {
        ProjectedEvent {
            sequence,
            kind: string_field(payload, "kind"),
            status: string_field(payload, "status"),
            code: string_field(payload, "code"),
            message: string_field(payload, "message"),
            outputs: None,
            receipt: None,
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
