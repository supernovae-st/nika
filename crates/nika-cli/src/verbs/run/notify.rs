// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! ADR-111 — outbound pause delivery: the question is heard.
//!
//! A paused run journals `workflow_paused` and exits; nothing pushes. When
//! the operator configures `NIKA_NOTIFY_URL`, each lane POSTs the pause
//! payload ONCE — a `CloudEvents` 1.0.2 structured envelope
//! (`sh.nika.run.paused` · deterministic id from trace × task) with
//! Standard Webhooks headers (`webhook-id` / `webhook-timestamp` always ·
//! `webhook-signature` `v1,`-HMAC-SHA256 when `NIKA_NOTIFY_SECRET` holds a
//! `whsec_` secret) — then journals the outcome (`notify_delivered` /
//! `notify_failed`) BEFORE the seal, so the chain covers the delivery
//! claim. Delivery is observable history, never control flow: the run
//! exits `paused` with the same code whether the webhook landed, failed,
//! or was never configured (absent URL ⇒ not even a socket).
//!
//! Transport: the same `ReqwestHttp` the trace-anchor verb composes — the
//! SSRF floor stays ON; the configured host rides a
//! [`NetBoundary::Declared`] allowlist of exactly one entry, so an exact
//! loopback literal (a local relay · ntfy on localhost) is declassified
//! per the same carve-out `permits.net.http` grants, while
//! metadata/private ranges keep refusing.

use std::path::Path;
use std::time::Duration;

use base64::Engine as _;
use hmac::Mac as _;
use nika_event::{Event, EventKind};
use nika_http::{HttpConfig, NetBoundary, ReqwestHttp};
use nika_kernel::http::{HttpError, HttpPostDyn as _, HttpRequest};
use nika_runtime::{EventSink, Stamper, WorkflowPause};
use nika_types::id::EventId;
use nika_types::resource::{KeyValue, Value};
use nika_types::timestamp::Timestamp;

/// The workflow's display label — the same fallback expression the
/// runtime's envelope derivation uses (`"workflow"` when the header is
/// unreadable, which a checked run never is).
pub(super) fn workflow_label(wf: &nika_schema::raw::RawWorkflow) -> String {
    wf.workflow
        .as_ref()
        .map_or_else(|| String::from("workflow"), |w| w.value.clone())
}

/// The operator's delivery configuration — env-borne, per the engine's
/// existing `NIKA_*` surface. Absent URL ⇒ the feature is OFF.
pub(super) struct NotifyConfig {
    url: String,
    /// Decoded `whsec_` secret bytes (absent or undecodable ⇒ unsigned).
    secret: Option<Vec<u8>>,
}

impl NotifyConfig {
    /// Read `NIKA_NOTIFY_URL` (+ optional `NIKA_NOTIFY_SECRET`). The
    /// sovereign default is silence: no URL, no socket, no config object.
    pub(super) fn from_env() -> Option<Self> {
        let url = env_value("NIKA_NOTIFY_URL")?;
        if url.trim().is_empty() {
            return None;
        }
        let secret = env_value("NIKA_NOTIFY_SECRET")
            .as_deref()
            .and_then(decode_whsec);
        Some(Self { url, secret })
    }
}

/// Read the notify configuration from the environment — the operator's
/// deployment seam (ADR-111). The workspace `disallowed_methods` ban on
/// `std::env::var` routes SECRET reads through the vault seam; the
/// webhook URL is deployment config, and the optional signing secret is
/// env-borne BY DESIGN in R1 — ADR-111 names the store-backed reference
/// as the R2 follow-up, and this scoped allow is that debt, visible.
#[allow(clippy::disallowed_methods)]
fn env_value(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Standard Webhooks secret form: `whsec_` + base64(key). A bare base64
/// string is tolerated (the prefix is a labeling convention).
fn decode_whsec(s: &str) -> Option<Vec<u8>> {
    let b64 = s.strip_prefix("whsec_").unwrap_or(s);
    base64::engine::general_purpose::STANDARD.decode(b64).ok()
}

/// The `CloudEvents` 1.0.2 structured envelope (JSON format). Field order
/// is declaration order — pinned by the golden test below.
#[derive(serde::Serialize)]
struct CloudEvent<'a> {
    specversion: &'static str,
    id: &'a str,
    source: String,
    #[serde(rename = "type")]
    kind: &'static str,
    subject: String,
    time: &'a str,
    datacontenttype: &'static str,
    data: PauseData<'a>,
}

/// The `workflow_paused` payload, envelope-side — the same facts the
/// journal frame carries (secret-masked upstream by construction), plus
/// the two things a remote surface needs to act: where the trace lives
/// and the exact resume teaching line.
#[derive(serde::Serialize)]
struct PauseData<'a> {
    workflow: &'a str,
    task: &'a str,
    mode: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<&'a str>,
    #[serde(skip_serializing_if = "slice_is_empty")]
    choices: &'a [String],
    trace_path: &'a str,
    resume_hint: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    approval_digest: Option<String>,
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's skip_serializing_if contract
fn slice_is_empty(v: &&[String]) -> bool {
    v.is_empty()
}

/// Deterministic `CloudEvents` `id` — sha256 over `trace_id:task`,
/// lowercase hex. Re-delivering the same pause yields the same id
/// (consumers dedup for free); a resumed run writes a NEW trace, so a
/// later re-pause of the same task yields a new id.
pub(super) fn event_id(trace_id: &str, task: &str) -> String {
    use sha2::Digest as _;
    let mut h = sha2::Sha256::new();
    h.update(trace_id.as_bytes());
    h.update(b":");
    h.update(task.as_bytes());
    let digest = h.finalize();
    let mut out = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Render the envelope. `time` is RFC 3339 from the stamped event
/// timestamp (the same instant the journal outcome will carry).
#[allow(clippy::too_many_arguments)] // the envelope's own field list
fn envelope_json(
    ce_id: &str,
    time: &str,
    workflow: &str,
    pause: &WorkflowPause,
    trace_id: &str,
    trace_path: &str,
    resume_hint: &str,
) -> String {
    let event = CloudEvent {
        specversion: "1.0",
        id: ce_id,
        source: format!("/nika/runs/{trace_id}"),
        kind: "sh.nika.run.paused",
        subject: format!("task:{}", pause.task),
        time,
        datacontenttype: "application/json",
        data: PauseData {
            workflow,
            task: &pause.task,
            mode: &pause.mode,
            message: pause.message.as_deref(),
            choices: &pause.choices,
            trace_path,
            resume_hint,
            approval_digest: pause
                .approval
                .as_ref()
                .and_then(nika_runtime::approval::ApprovalTicket::digest),
        },
    };
    serde_json::to_string(&event).unwrap_or_else(|_| String::from("{}"))
}

/// RFC 3339 rendering of a journal [`Timestamp`] (jiff carries the
/// calendar; the engine's own type stays nanoseconds-since-epoch).
fn rfc3339(ts: Timestamp) -> String {
    match jiff::Timestamp::from_nanosecond(i128::from(ts.unix_ns)) {
        Ok(t) => t.to_string(),
        // Unreachable within Timestamp's documented ±292y range — but a
        // formatter must not be able to kill a pause path.
        Err(_) => String::from("1970-01-01T00:00:00Z"),
    }
}

/// Standard Webhooks `v1,` signature over `{id}.{timestamp}.{payload}`.
fn sign_v1(key: &[u8], ce_id: &str, unix_secs: i64, body: &str) -> Option<String> {
    type HmacSha256 = hmac::Hmac<sha2::Sha256>;
    let mut mac = HmacSha256::new_from_slice(key).ok()?;
    mac.update(ce_id.as_bytes());
    mac.update(b".");
    mac.update(unix_secs.to_string().as_bytes());
    mac.update(b".");
    mac.update(body.as_bytes());
    let sig = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());
    Some(format!("v1,{sig}"))
}

/// One delivery attempt's journaled shape.
struct DeliveryReport {
    host: String,
    /// `Ok(duration_ms)` on a 2xx · `Err(class)` otherwise.
    result: Result<u64, String>,
}

/// The error's journal class — coarse, stable, greppable.
fn class_of(e: &HttpError) -> String {
    match e {
        HttpError::SsrfBlocked { .. } => String::from("ssrf_blocked"),
        HttpError::HostNotAllowed { .. } => String::from("host_not_allowed"),
        HttpError::Timeout { .. } => String::from("timeout"),
        _ => String::from("transport"),
    }
}

/// POST the envelope. Single attempt · 3 s timeout · never follow a
/// redirect (the `CloudEvents` HTTP-Webhook delivery discipline) · any 2xx
/// counts as delivered.
async fn deliver(cfg: &NotifyConfig, ce_id: &str, unix_secs: i64, body: String) -> DeliveryReport {
    let host = url::Url::parse(&cfg.url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_else(|| String::from("?"));
    let mut config = HttpConfig::default();
    // The operator-configured target is the ONLY admitted host — the
    // SSRF floor stays on beneath, with the exact-loopback carve-out the
    // boundary already grants (a local relay is a legitimate target).
    config.net = NetBoundary::Declared(vec![host.clone()]);
    let http = match ReqwestHttp::with_config(config) {
        Ok(h) => h,
        Err(e) => {
            return DeliveryReport {
                host,
                result: Err(class_of(&e)),
            };
        }
    };
    let mut request = HttpRequest::post(cfg.url.clone());
    request.follow_redirects = false;
    request.timeout = Some(Duration::from_secs(3));
    request.headers.insert(
        String::from("content-type"),
        String::from("application/cloudevents+json"),
    );
    request
        .headers
        .insert(String::from("webhook-id"), ce_id.to_owned());
    request
        .headers
        .insert(String::from("webhook-timestamp"), unix_secs.to_string());
    if let Some(sig) = cfg
        .secret
        .as_deref()
        .and_then(|key| sign_v1(key, ce_id, unix_secs, &body))
    {
        request
            .headers
            .insert(String::from("webhook-signature"), sig);
    }
    request.body = Some(bytes::Bytes::from(body));
    let started = std::time::Instant::now();
    match http.post(request).await {
        Ok(resp) if (200..300).contains(&resp.status) => DeliveryReport {
            host,
            result: Ok(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)),
        },
        Ok(resp) => DeliveryReport {
            host,
            result: Err(format!("status_{}", resp.status)),
        },
        Err(e) => DeliveryReport {
            host,
            result: Err(class_of(&e)),
        },
    }
}

/// Journal the outcome under the run's own chain (BEFORE the seal — the
/// caller's ordering contract).
fn journal_outcome(report: &DeliveryReport, id_ts: (EventId, Timestamp), sink: &mut dyn EventSink) {
    let (id, ts) = id_ts;
    let (kind, fields) = match &report.result {
        Ok(ms) => (
            EventKind::NotifyDelivered,
            vec![
                KeyValue::new("target_host", Value::String(report.host.clone())),
                KeyValue::new(
                    "duration_ms",
                    Value::Int(i64::try_from(*ms).unwrap_or(i64::MAX)),
                ),
            ],
        ),
        Err(class) => (
            EventKind::NotifyFailed,
            vec![
                KeyValue::new("target_host", Value::String(report.host.clone())),
                KeyValue::new("error", Value::String(class.clone())),
            ],
        ),
    };
    sink.emit(Event::new(id, ts, kind).with_fields(fields));
}

/// The lane hook — deliver the pause outward and journal the outcome.
///
/// Called by every lane AFTER the tee splits and BEFORE `surface_trace`
/// seals. No configured URL ⇒ silent no-op. Failure never touches the
/// verdict: the run exits `paused` with the same code either way.
pub(super) async fn deliver_paused(
    workflow: &str,
    pause: &WorkflowPause,
    trace_path: &Path,
    resume_hint: &str,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    let Some(cfg) = NotifyConfig::from_env() else {
        return;
    };
    let trace_id = trace_path
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("run");
    let ce_id = event_id(trace_id, &pause.task);
    // One stamp serves both faces: the envelope's `time` and the journal
    // outcome's timestamp (delivery duration rides its own field).
    let (event_id, ts) = stamper.next();
    let time = rfc3339(ts);
    let unix_secs = ts.unix_ns / 1_000_000_000;
    let body = envelope_json(
        &ce_id,
        &time,
        workflow,
        pause,
        trace_id,
        &trace_path.display().to_string(),
        resume_hint,
    );
    let report = deliver(&cfg, &ce_id, unix_secs, body).await;
    journal_outcome(&report, (event_id, ts), sink);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pause(task: &str) -> WorkflowPause {
        WorkflowPause::new(
            task.to_owned(),
            String::from("confirm"),
            Some(String::from("Deploy to production?")),
            vec![],
        )
    }

    #[test]
    fn event_id_is_deterministic_and_task_scoped() {
        let a = event_id("2026-08-06T09-07-18Z-402b", "approve");
        let b = event_id("2026-08-06T09-07-18Z-402b", "approve");
        let c = event_id("2026-08-06T09-07-18Z-402b", "other_task");
        assert_eq!(a, b, "same pause, same id — consumer dedup rides on it");
        assert_ne!(a, c, "a different task is a different event");
        assert_eq!(a.len(), 64, "sha256 lowercase hex");
        assert!(a.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn envelope_golden_pins_the_cloudevents_shape() {
        // The required CloudEvents 1.0.2 attributes, structured mode, in
        // declaration order — a byte-level pin so the wire form cannot
        // drift silently (ADR-111 test 6).
        let p = pause("approve");
        let json = envelope_json(
            "aabbcc",
            "2026-08-06T10:12:33Z",
            "gate-probe",
            &p,
            "2026-08-06T09-07-18Z-402b",
            "/tmp/t.ndjson",
            "nika run wf.nika.yaml --resume /tmp/t.ndjson --answer approve=<value>",
        );
        assert_eq!(
            json,
            "{\"specversion\":\"1.0\",\"id\":\"aabbcc\",\
             \"source\":\"/nika/runs/2026-08-06T09-07-18Z-402b\",\
             \"type\":\"sh.nika.run.paused\",\"subject\":\"task:approve\",\
             \"time\":\"2026-08-06T10:12:33Z\",\
             \"datacontenttype\":\"application/json\",\
             \"data\":{\"workflow\":\"gate-probe\",\"task\":\"approve\",\
             \"mode\":\"confirm\",\"message\":\"Deploy to production?\",\
             \"trace_path\":\"/tmp/t.ndjson\",\
             \"resume_hint\":\"nika run wf.nika.yaml --resume /tmp/t.ndjson --answer approve=<value>\"}}"
        );
    }

    #[test]
    fn signature_matches_an_independent_implementation() {
        // ADR-111 test 7 — the expected value was computed by CPython's
        // hmac/hashlib (an implementation this code shares nothing with):
        //   key     = b"nika-test-secret-32-bytes-long!!"
        //   secret  = whsec_bmlrYS10ZXN0LXNlY3JldC0zMi1ieXRlcy1sb25nISE=
        //   signed  = "{id}.{ts}.{payload}"
        let key = decode_whsec("whsec_bmlrYS10ZXN0LXNlY3JldC0zMi1ieXRlcy1sb25nISE=")
            .expect("the vector secret decodes");
        assert_eq!(key, b"nika-test-secret-32-bytes-long!!");
        let sig = sign_v1(
            &key,
            "a3f1c2e4d5b6978012345678deadbeefa3f1c2e4d5b6978012345678deadbeef",
            1_754_500_000,
            "{\"specversion\":\"1.0\"}",
        )
        .expect("hmac accepts any key length");
        assert_eq!(sig, "v1,j5EOg6CtX2cp9Tq4LOPEg9iHm4rh8DDuNteKqOMl95M=");
    }

    #[test]
    fn whsec_prefix_is_a_labeling_convention() {
        let with = decode_whsec("whsec_aGVsbG8=").expect("prefixed form decodes");
        let bare = decode_whsec("aGVsbG8=").expect("bare form decodes");
        assert_eq!(with, bare);
        assert_eq!(with, b"hello");
        assert!(
            decode_whsec("whsec_%%%").is_none(),
            "garbage refuses quietly"
        );
    }

    #[test]
    fn error_classes_are_stable_and_greppable() {
        assert_eq!(
            class_of(&HttpError::Timeout { duration_ms: 3000 }),
            "timeout"
        );
    }
}
