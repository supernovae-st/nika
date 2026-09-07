// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The OTLP/JSON projection — a run journal folded to one
//! `{"resourceSpans":[…]}` line (the de-facto file-exporter shape).
//!
//! Projection, never instrumentation: the journal stays the source of
//! truth; this is a post-hoc fold. Deterministic identity — trace id =
//! the `workflow_started` event UUID, span ids = the LOW 8 bytes of
//! each event's `UUIDv7` (the random half, never the timestamp-prefixed
//! high half) — so re-exporting the same journal is idempotent.
//!
//! Hand-writer laws (each one a documented ecosystem bite): trace/span
//! ids are HEX strings (the proto3-JSON exception, never base64) ·
//! `*TimeUnixNano` are decimal STRINGS · attributes are typed envelopes
//! (`{"key":…,"value":{"stringValue":…}}`) · `end ≥ start` (cache hits
//! settle at +1ns) · never an all-zero id.
//!
//! Content policy (Rule 1): recorded outputs ride ONLY under
//! `include_content` — the default projection carries structure,
//! timing, spend and identity, never payloads. That gate covers every
//! field whose VALUE can carry run text, not only `output`: the failure
//! `detail` (the status message keeps the error CODE alone), the
//! success frame's `warning`. The T9-F04 sensitivity pass (2026-09-06)
//! found both riding the content-free projection. `note` stays: it is
//! the engine's own stage wording (`invoke · nika:fetch` · `cache hit`
//! · `when: gate closed`), never a payload.
//!
//! Descended from `nika-cli`'s `trace_otel` verb (2026-07-09 · the W0
//! trace descent); the CLI keeps the file plumbing and the operator's
//! message as a shim over this fold.

use std::fmt::Write as _;

use nika_event::{Event, EventKind};
use nika_types::resource::Value as FieldValue;

use crate::chain::Verdict;

/// The de-facto cost attribute (Langfuse maps it to USD total); the
/// spec has no stable cost attribute yet (checked 2026-07-06).
const COST_ATTR: &str = "gen_ai.usage.cost";

/// One journal → one `{"resourceSpans":[…]}` JSON value (one line).
///
/// `chain` carries the verify verdict so the anchor (or the broken
/// flag) travels with the export: `Intact`/`TornTail` ride their head,
/// `Broken` is flagged, anything else (a pre-chain journal) claims
/// nothing. `exporter_version` names the tool doing the projection —
/// the journal's own attestation wins where present; this is only the
/// fallback for pre-attestation journals (and the scope version).
///
/// # Errors
///
/// A human-readable refusal when the journal records no
/// `workflow_started` event (not a run journal), or when the payload
/// will not serialize.
pub fn project(
    events: &[Event],
    include_content: bool,
    chain: Option<&Verdict>,
    exporter_version: &str,
) -> Result<String, String> {
    let started = events
        .iter()
        .find(|e| e.kind == EventKind::WorkflowStarted)
        .ok_or_else(|| "no workflow_started event — not a run journal".to_owned())?;

    let trace_id = hex_bytes(started.id.uuid.as_bytes());
    let root_span_id = span_id_of(started);
    let workflow = field_str(started, "workflow").unwrap_or("workflow");

    let last_ns = events
        .iter()
        .map(|e| e.timestamp.unix_ns)
        .max()
        .unwrap_or(started.timestamp.unix_ns);

    let mut spans = vec![root_span(
        started,
        &trace_id,
        &root_span_id,
        workflow,
        last_ns,
        events,
    )];
    spans.extend(task_spans(
        events,
        &trace_id,
        &root_span_id,
        include_content,
    ));

    // The resource names the engine that RAN, not the one exporting —
    // the journal's own attestation (Q11) wins; the exporter's version
    // is only the fallback for pre-attestation journals.
    let ran_version = field_str(started, "engine_version").unwrap_or(exporter_version);
    let mut resource_attrs = vec![
        kv_str("service.name", "nika"),
        kv_str("service.version", ran_version),
    ];
    if let Some(platform) = field_str(started, "platform") {
        resource_attrs.push(kv_str("nika.platform", platform));
    }
    match chain {
        Some(Verdict::Intact { head, .. }) => {
            resource_attrs.push(kv_str("nika.trace.chain_head", head));
        }
        // NEP-0011 §3 · the incomplete class is NAMED, never merged with
        // the intact one (the projection says what the walk said).
        Some(Verdict::Incomplete { head, .. }) => {
            resource_attrs.push(kv_str("nika.trace.chain_head", head));
            resource_attrs.push(kv_str("nika.trace.lifecycle", "incomplete"));
        }
        Some(Verdict::TornTail { head, .. }) => {
            resource_attrs.push(kv_str("nika.trace.chain_head", head));
            resource_attrs.push(kv_str("nika.trace.lifecycle", "torn_tail"));
        }
        Some(Verdict::Broken { .. }) => {
            resource_attrs.push(kv_str("nika.trace.chain", "broken"));
        }
        _ => {} // unchained (pre-chain journal) — nothing to claim
    }
    let payload = serde_json::json!({
        "resourceSpans": [{
            "resource": { "attributes": resource_attrs },
            "scopeSpans": [{
                "scope": { "name": "nika", "version": exporter_version },
                "spans": spans,
            }],
        }],
    });
    serde_json::to_string(&payload).map_err(|e| format!("serialize: {e}"))
}

fn root_span(
    started: &Event,
    trace_id: &str,
    root_span_id: &str,
    workflow: &str,
    last_ns: i64,
    events: &[Event],
) -> serde_json::Value {
    let mut attributes = vec![kv_str("gen_ai.workflow.name", workflow)];
    if let Some(permits) = field_str(started, "permits") {
        attributes.push(kv_str("nika.permits", permits));
    }
    if let Some(sha) = field_str(started, "workflow_sha256") {
        attributes.push(kv_str("nika.workflow.sha256", sha));
    }
    // The run verdict → span status. A pause is neither Ok nor Error —
    // status stays Unset and the attr names the gate.
    let mut status = serde_json::json!({});
    for e in events {
        match e.kind {
            EventKind::WorkflowCompleted => status = serde_json::json!({ "code": 1 }),
            EventKind::WorkflowFailed => {
                status = serde_json::json!({ "code": 2, "message": "workflow failed" });
            }
            EventKind::WorkflowPaused => {
                attributes.push(kv_bool("nika.run.paused", true));
                if let Some(task) = field_str(e, "task") {
                    attributes.push(kv_str("nika.pause.task", task));
                }
            }
            _ => {}
        }
    }
    serde_json::json!({
        "traceId": trace_id,
        "spanId": root_span_id,
        "name": format!("invoke_workflow {workflow}"),
        "kind": 1,
        "startTimeUnixNano": started.timestamp.unix_ns.to_string(),
        "endTimeUnixNano": last_ns.max(started.timestamp.unix_ns.saturating_add(1)).to_string(),
        "attributes": attributes,
        "status": status,
    })
}

/// Per-task spans: start at `task_started` (cache hits settle where they
/// land, +1ns wide), end at the terminal event, retries as span events.
fn task_spans(
    events: &[Event],
    trace_id: &str,
    root_span_id: &str,
    include_content: bool,
) -> Vec<serde_json::Value> {
    let mut order: Vec<&str> = Vec::new();
    let mut by_task: std::collections::BTreeMap<&str, Vec<&Event>> =
        std::collections::BTreeMap::new();
    for e in events {
        let Some(task) = field_str(e, "task") else {
            continue;
        };
        if !by_task.contains_key(task) {
            order.push(task);
        }
        by_task.entry(task).or_default().push(e);
    }

    let last_ns = events
        .iter()
        .map(|e| e.timestamp.unix_ns)
        .max()
        .unwrap_or(0);
    let mut spans = Vec::new();
    for task in order {
        let task_events = &by_task[task];
        let span = one_task_span(task, task_events, trace_id, root_span_id, include_content)
            .or_else(|| unfinished_task_span(task, task_events, trace_id, root_span_id, last_ns));
        if let Some(span) = span {
            spans.push(span);
        }
    }
    spans
}

const TERMINALS: [EventKind; 5] = [
    EventKind::TaskCompleted,
    EventKind::TaskFailed,
    EventKind::TaskSkipped,
    EventKind::TaskCancelled,
    EventKind::TaskCacheHit,
];

#[allow(clippy::too_many_lines)] // one linear attribute walk — splitting it hides the mapping table
fn one_task_span(
    task: &str,
    task_events: &[&Event],
    trace_id: &str,
    root_span_id: &str,
    include_content: bool,
) -> Option<serde_json::Value> {
    let terminal = task_events.iter().find(|e| TERMINALS.contains(&e.kind))?;
    let started = task_events
        .iter()
        .find(|e| e.kind == EventKind::TaskStarted);
    // Identity anchors on the event that BEGAN the story (started ·
    // else the terminal itself for scheduled-only settles).
    let anchor = started.unwrap_or(terminal);
    // The span WINDOW comes from `duration_ms`, not the frame gap: the
    // runtime settles a task in one burst (started · retries · terminal
    // share one stamp — measured 2ms apart on a 2009ms task), so the
    // terminal is the settle instant and the measured duration walks
    // backward from it. Frame-gap fallback for duration-less terminals
    // (skip · cancel · cache-hit) — those genuinely take ~no time.
    let end_ns = terminal.timestamp.unix_ns;
    // A negative measured duration is a corrupt journal — treat it as
    // duration-less (the frame-gap arm), never a forward-running span.
    let start_ns = match field(terminal, "duration_ms") {
        Some(FieldValue::Int(ms)) if *ms >= 0 => {
            end_ns.saturating_sub(ms.saturating_mul(1_000_000))
        }
        _ => anchor.timestamp.unix_ns,
    };
    let end_ns = end_ns.max(start_ns.saturating_add(1));

    let mut attributes = vec![kv_str("nika.task.id", task)];
    if let Some(note) = field_str(terminal, "note") {
        attributes.push(kv_str("nika.task.note", note));
    }
    for key in ["def_hash", "input_hash"] {
        if let Some(v) = field_str(terminal, key) {
            attributes.push(kv_str(&format!("nika.task.{key}"), v));
        }
    }
    if let Some(FieldValue::Int(tokens)) = field(terminal, "tokens") {
        attributes.push(kv_int("nika.tokens", *tokens));
    }
    push_usage_semconv(&mut attributes, terminal);
    push_genai_semconv(&mut attributes, terminal);
    if let Some(FieldValue::Float(usd)) = field(terminal, "cost_usd") {
        attributes.push(kv_double(COST_ATTR, *usd));
        attributes.push(kv_double("nika.cost.usd", *usd));
    }
    let status = terminal_status(terminal, include_content, &mut attributes);
    if include_content && let Some(output) = field_str(terminal, "output") {
        attributes.push(kv_str("nika.task.output", output));
    }
    // OBS-E non-fatal diagnostics ride the success frame — surface them
    // WITH the content gate: a warning quotes what went wrong (a blank
    // model answer · a path), which is content.
    if include_content && let Some(warning) = field_str(terminal, "warning") {
        attributes.push(kv_str("nika.task.warning", warning));
    }

    // Retries and agent routing decisions ride as span events — the
    // attempt story stays visible in any OTel viewer without inventing
    // child spans the journal lacks. Retry frames carry the REAL fields
    // (attempt · max_attempts · delay_ms — ints, settle-emitted).
    let span_events = task_span_events(task_events);

    Some(serde_json::json!({
        "traceId": trace_id,
        "spanId": span_id_of(anchor),
        "parentSpanId": root_span_id,
        "name": task,
        "kind": 1,
        "startTimeUnixNano": start_ns.to_string(),
        "endTimeUnixNano": end_ns.to_string(),
        "attributes": attributes,
        "events": span_events,
        "status": status,
    }))
}

/// The span status the terminal kind dictates, plus the kind's own
/// attributes (skipped · cancelled · cache hit). A failed span's message
/// is the whole `detail` only with the content; otherwise the error CODE
/// alone — `detail` is `<code> · <message>` and the message half can
/// quote a path, a payload, a model answer.
fn terminal_status(
    terminal: &Event,
    include_content: bool,
    attributes: &mut Vec<serde_json::Value>,
) -> serde_json::Value {
    match terminal.kind {
        EventKind::TaskCompleted => serde_json::json!({ "code": 1 }),
        EventKind::TaskFailed => {
            let detail = field_str(terminal, "detail").unwrap_or("task failed");
            let message = if include_content {
                detail
            } else {
                failure_code(detail)
            };
            serde_json::json!({ "code": 2, "message": message })
        }
        EventKind::TaskSkipped => {
            attributes.push(kv_bool("nika.task.skipped", true));
            if let Some(when) = field_str(terminal, "when") {
                attributes.push(kv_str("nika.task.when", when));
            }
            serde_json::json!({})
        }
        EventKind::TaskCancelled => {
            attributes.push(kv_bool("nika.task.cancelled", true));
            if let Some(culprit) = field_str(terminal, "blocked_by") {
                attributes.push(kv_str("nika.task.blocked_by", culprit));
            }
            serde_json::json!({})
        }
        EventKind::TaskCacheHit => {
            attributes.push(kv_bool("nika.cache.hit", true));
            serde_json::json!({ "code": 1 })
        }
        _ => serde_json::json!({}),
    }
}

/// The in-span story: retry frames (`attempt`/`max_attempts`/`delay_ms`)
/// and agent routing decisions (`tools_selected` · offered/universe)
/// become `OTel` span events, timestamped where the journal put them.
fn task_span_events(task_events: &[&Event]) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for e in task_events {
        let (name, keys): (&str, &[&str]) = match e.kind {
            EventKind::TaskRetrying => ("retry", &["attempt", "max_attempts", "delay_ms"]),
            EventKind::AgentToolsSelected => ("tools_selected", &["turn", "offered", "universe"]),
            EventKind::AgentStalled => ("agent_stalled", &["turn", "attempt"]),
            EventKind::AgentNudge => ("agent_nudge", &["turn", "attempt"]),
            _ => continue,
        };
        let mut attrs = Vec::new();
        for key in keys {
            if let Some(FieldValue::Int(v)) = field(e, key) {
                attrs.push(kv_int(&format!("nika.{name}.{key}"), *v));
            }
        }
        out.push(serde_json::json!({
            "name": name,
            "timeUnixNano": e.timestamp.unix_ns.to_string(),
            "attributes": attrs,
        }));
    }
    out
}

/// A task that STARTED but never settled (a pause gate holding it · a
/// crash mid-flight) still deserves a span — Unset status, flagged, and
/// ended at the journal's last breath so the trace stays renderable.
fn unfinished_task_span(
    task: &str,
    task_events: &[&Event],
    trace_id: &str,
    root_span_id: &str,
    last_ns: i64,
) -> Option<serde_json::Value> {
    let started = task_events
        .iter()
        .find(|e| e.kind == EventKind::TaskStarted)?;
    let start_ns = started.timestamp.unix_ns;
    let mut attributes = vec![
        kv_str("nika.task.id", task),
        kv_bool("nika.task.unfinished", true),
    ];
    if let Some(note) = field_str(started, "note") {
        attributes.push(kv_str("nika.task.note", note));
    }
    Some(serde_json::json!({
        "traceId": trace_id,
        "spanId": span_id_of(started),
        "parentSpanId": root_span_id,
        "name": task,
        "kind": 1,
        "startTimeUnixNano": start_ns.to_string(),
        "endTimeUnixNano": last_ns.max(start_ns.saturating_add(1)).to_string(),
        "attributes": attributes,
        "events": task_span_events(task_events),
        "status": {},
    }))
}

// ─── identity + envelope helpers ─────────────────────────────────────────────

/// Span id = the LOW 8 bytes of the event's `UUIDv7` — the random half.
/// The high half is a millisecond timestamp: two events born in the
/// same ms would collide there.
/// The error CODE a `detail` opens with (`NIKA-…` up to the first
/// ` · `), or the generic wording when the detail carries no code —
/// never the message half.
fn failure_code(detail: &str) -> &str {
    let head = detail.split(" · ").next().unwrap_or(detail).trim();
    if head.starts_with("NIKA-") && !head.contains(char::is_whitespace) {
        head
    } else {
        "task failed"
    }
}

fn span_id_of(event: &Event) -> String {
    let id = hex_bytes(&event.id.uuid.as_bytes()[8..16]);
    // The docstring's own law, enforced: an all-zero id (nil-uuid line
    // in a corrupted journal) is OTLP-invalid — strict consumers drop
    // the span silently. Substitute a constant non-zero sentinel; the
    // ids of a nil-uuid journal were never meaningful to begin with.
    if id == "0000000000000000" {
        return "6e696b612d302d30".to_owned(); // "nika-0-0"
    }
    id
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

/// Project an infer/agent terminal's access facts (D-2026-08-04-N1 ·
/// `model` = `<provider>/<name>` · `provider` = the prefix) to the
/// CURRENT `OTel` `GenAI` semconv names, so every viewer and eval tool
/// reads the model without a translation shim. NEVER the deprecated
/// `gen_ai.system` (renamed in semconv v1.37.0). One place, so a
/// pre-stable-semconv rename upstream is one edit, never a scatter.
fn push_genai_semconv(attributes: &mut Vec<serde_json::Value>, terminal: &Event) {
    if let Some(model) = field_str(terminal, "model") {
        // `gen_ai.request.model` is the model NAME; the provider rides
        // its own attribute (semconv keeps them distinct). Split the
        // canonical `provider/name` form at the FIRST slash — the
        // inner-slash convention (`openrouter/anthropic/claude` ·
        // `huggingface/Qwen/Qwen3.5-9B`) keeps the tail whole; a
        // slash-less value stays whole.
        let name = model.split_once('/').map_or(model, |(_, n)| n);
        attributes.push(kv_str("gen_ai.request.model", name));
    }
    // `gen_ai.response.model` is the model that SERVED the response, as
    // the PROVIDER reports it — emitted only from the frame's own
    // `model_served` (the wires' `gen_ai.response.model`, now
    // journaled). The requested name is never re-emitted here: aliases
    // (`-latest` · nicknames) make served ≠ requested, and an eval tool
    // would read "requested" as "served" (ADR-112 L71-77 · its return
    // condition is now met at the frame, not guessed).
    if let Some(served) = field_str(terminal, "model_served") {
        attributes.push(kv_str("gen_ai.response.model", served));
    }
    if let Some(id) = field_str(terminal, "response_id") {
        attributes.push(kv_str("gen_ai.response.id", id));
    }
    if let Some(provider) = field_str(terminal, "provider") {
        // The normalization table: canonical nika ids → the semconv
        // well-known values, where one exists and differs (registry
        // checked 2026-08-06). `gemini` → `gcp.gemini` is the well-known
        // for `generativelanguage.googleapis.com` — the exact endpoint
        // the gemini profile dials. Everything else passes through
        // verbatim: the ids that already ARE the well-known (`openai` ·
        // `anthropic` · `deepseek` · `groq`) and the ones with none (the
        // five local servers · `openrouter` · `huggingface` · `nvidia` ·
        // `mock`).
        let well_known = match provider {
            "gemini" => "gcp.gemini",
            "mistral" => "mistral_ai",
            "xai" => "x_ai",
            other => other,
        };
        attributes.push(kv_str("gen_ai.provider.name", well_known));
    }
}

/// the usage SPLIT as `OTel` `GenAI` counters, so a collector
/// prices the call without parsing our own names — and our own
/// `nika.tokens.*` mirrors beside them, so a nika reader never depends
/// on a semconv still marked `development`.
///
/// Names read from `open-telemetry/semantic-conventions` v1.37.0's
/// `gen_ai` registry (the version this module already pins for
/// `gen_ai.provider.name`): `gen_ai.usage.input_tokens` (SHOULD include
/// the cached subset — which is exactly what the frame's `tokens_in`
/// carries), `gen_ai.usage.output_tokens`,
/// `gen_ai.usage.cache_read.input_tokens`,
/// `gen_ai.usage.cache_write.input_tokens`,
/// `gen_ai.usage.reasoning.output_tokens`.
///
/// Counters only — content-free by construction: an absent meter emits
/// NOTHING (a projection must never invent a zero the journal did not
/// carry).
fn push_usage_semconv(attributes: &mut Vec<serde_json::Value>, terminal: &Event) {
    const METERS: [(&str, &str, &str); 5] = [
        ("tokens_in", "gen_ai.usage.input_tokens", "nika.tokens.in"),
        (
            "tokens_out",
            "gen_ai.usage.output_tokens",
            "nika.tokens.out",
        ),
        (
            "tokens_cache_read",
            "gen_ai.usage.cache_read.input_tokens",
            "nika.tokens.cache_read",
        ),
        (
            "tokens_cache_write",
            "gen_ai.usage.cache_write.input_tokens",
            "nika.tokens.cache_write",
        ),
        (
            "tokens_reasoning",
            "gen_ai.usage.reasoning.output_tokens",
            "nika.tokens.reasoning",
        ),
    ];
    for (field_key, semconv, mirror) in METERS {
        if let Some(FieldValue::Int(n)) = field(terminal, field_key) {
            attributes.push(kv_int(semconv, *n));
            attributes.push(kv_int(mirror, *n));
        }
    }
}

fn field<'e>(event: &'e Event, key: &str) -> Option<&'e FieldValue> {
    event.fields.iter().find(|f| f.key == key).map(|f| &f.value)
}

fn field_str<'e>(event: &'e Event, key: &str) -> Option<&'e str> {
    match field(event, key) {
        Some(FieldValue::String(s)) => Some(s.as_str()),
        _ => None,
    }
}

fn kv_str(key: &str, value: &str) -> serde_json::Value {
    serde_json::json!({ "key": key, "value": { "stringValue": value } })
}

fn kv_int(key: &str, value: i64) -> serde_json::Value {
    serde_json::json!({ "key": key, "value": { "intValue": value.to_string() } })
}

fn kv_double(key: &str, value: f64) -> serde_json::Value {
    serde_json::json!({ "key": key, "value": { "doubleValue": value } })
}

fn kv_bool(key: &str, value: bool) -> serde_json::Value {
    serde_json::json!({ "key": key, "value": { "boolValue": value } })
}

#[cfg(test)]
mod tests {
    use nika_types::id::EventId;
    use nika_types::resource::KeyValue;
    use nika_types::timestamp::Timestamp;
    use uuid::Uuid;

    use super::*;

    /// The chain-less projection the unit pins drive.
    fn project_bare(events: &[Event], include_content: bool) -> Result<String, String> {
        project(events, include_content, None, "test-exporter")
    }

    /// Deterministic fixture event — uuid from a seed byte, ns from `ts`.
    fn ev(seed: u8, ts: i64, kind: EventKind, fields: &[(&str, FieldValue)]) -> Event {
        let mut event = Event::new(
            EventId::new(Uuid::from_bytes([seed; 16])),
            Timestamp::from_unix_ns(ts),
            kind,
        );
        for (key, value) in fields {
            event = event.with_field(KeyValue::new(*key, value.clone()));
        }
        event
    }

    fn s(v: &str) -> FieldValue {
        FieldValue::String(v.to_owned())
    }

    fn run_fixture() -> Vec<Event> {
        vec![
            ev(
                1,
                1_000,
                EventKind::WorkflowStarted,
                &[
                    ("workflow", s("demo")),
                    ("permits", s("engine floor (no boundary declared)")),
                    ("workflow_sha256", s(&"ab".repeat(32))),
                ],
            ),
            ev(2, 1_100, EventKind::TaskStarted, &[("task", s("fetch"))]),
            ev(
                3,
                1_500,
                EventKind::TaskRetrying,
                &[
                    ("task", s("fetch")),
                    ("attempt", FieldValue::Int(2)),
                    ("max_attempts", FieldValue::Int(3)),
                    ("delay_ms", FieldValue::Int(250)),
                ],
            ),
            ev(
                4,
                2_000,
                EventKind::TaskCompleted,
                &[
                    ("task", s("fetch")),
                    ("note", s("invoke · nika:fetch")),
                    ("duration_ms", FieldValue::Int(900)),
                    ("cost_usd", FieldValue::Float(0.01)),
                    ("tokens", FieldValue::Int(42)),
                    ("output", s("{\"x\":1}")),
                ],
            ),
            ev(
                5,
                2_100,
                EventKind::TaskCacheHit,
                &[("task", s("cached_one")), ("note", s("cache hit"))],
            ),
            ev(
                6,
                2_200,
                EventKind::TaskSkipped,
                &[
                    ("task", s("gated")),
                    ("note", s("when: gate closed")),
                    ("when", s("${{ tasks.fetch.status == 'failure' }}")),
                ],
            ),
            ev(
                7,
                2_300,
                EventKind::TaskCancelled,
                &[("task", s("downstream")), ("blocked_by", s("gated"))],
            ),
            ev(
                8,
                2_400,
                EventKind::TaskStarted,
                &[("task", s("held")), ("note", s("infer · gate"))],
            ),
            ev(
                9,
                3_000,
                EventKind::WorkflowCompleted,
                &[("workflow", s("demo"))],
            ),
        ]
    }

    fn spans_of(line: &str) -> Vec<serde_json::Value> {
        let v: serde_json::Value = serde_json::from_str(line).expect("valid JSON");
        v["resourceSpans"][0]["scopeSpans"][0]["spans"]
            .as_array()
            .expect("spans array")
            .clone()
    }

    fn attr<'a>(span: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
        span["attributes"]
            .as_array()?
            .iter()
            .find(|kv| kv["key"] == key)
            .map(|kv| &kv["value"])
    }

    #[test]
    fn projection_carries_identity_hierarchy_and_the_hand_writer_laws() {
        let line = project_bare(&run_fixture(), false).expect("projects");
        let spans = spans_of(&line);

        // Root: trace id = the started event's uuid, 32 hex chars.
        let root = &spans[0];
        assert_eq!(root["traceId"], "01".repeat(16));
        assert_eq!(root["spanId"].as_str().unwrap().len(), 16);
        assert_eq!(root["name"], "invoke_workflow demo");
        assert_eq!(
            attr(root, "nika.workflow.sha256").unwrap()["stringValue"],
            "ab".repeat(32)
        );
        assert_eq!(root["status"]["code"], 1);
        // Timestamps are decimal STRINGS (the proto3-JSON law).
        assert!(root["startTimeUnixNano"].is_string());

        // Task span: child of root, cost on BOTH attrs, retry as event.
        let fetch = spans.iter().find(|sp| sp["name"] == "fetch").unwrap();
        assert_eq!(fetch["parentSpanId"], root["spanId"]);
        assert_eq!(
            attr(fetch, "gen_ai.usage.cost").unwrap()["doubleValue"],
            0.01
        );
        assert_eq!(attr(fetch, "nika.cost.usd").unwrap()["doubleValue"], 0.01);
        assert_eq!(attr(fetch, "nika.tokens").unwrap()["intValue"], "42");
        assert_eq!(fetch["events"][0]["name"], "retry");
        assert_eq!(
            fetch["events"][0]["attributes"][0]["key"], "nika.retry.attempt",
            "retry events carry the REAL journal fields (attempt · ints)"
        );
        // The span WINDOW is duration_ms walked back from the settle —
        // the runtime emits started+terminal in one burst, so the frame
        // gap is ~0 and duration_ms is the only true width.
        let f_start: i64 = fetch["startTimeUnixNano"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        let f_end: i64 = fetch["endTimeUnixNano"].as_str().unwrap().parse().unwrap();
        assert_eq!(f_end - f_start, 900 * 1_000_000, "width = duration_ms");
        // Content stays OUT by default (Rule 1).
        assert!(attr(fetch, "nika.task.output").is_none());

        // Cache hit: +1ns wide, flagged, Ok.
        let cached = spans.iter().find(|sp| sp["name"] == "cached_one").unwrap();
        let start: i64 = cached["startTimeUnixNano"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        let end: i64 = cached["endTimeUnixNano"].as_str().unwrap().parse().unwrap();
        assert_eq!(end, start + 1);
        assert_eq!(attr(cached, "nika.cache.hit").unwrap()["boolValue"], true);

        // The why fields ride.
        let gated = spans.iter().find(|sp| sp["name"] == "gated").unwrap();
        assert!(
            attr(gated, "nika.task.when").unwrap()["stringValue"]
                .as_str()
                .unwrap()
                .contains("tasks.fetch.status")
        );
        let cancelled = spans.iter().find(|sp| sp["name"] == "downstream").unwrap();
        assert_eq!(
            attr(cancelled, "nika.task.blocked_by").unwrap()["stringValue"],
            "gated"
        );

        // A started-but-never-settled task (pause gate · crash) still
        // renders: Unset status, flagged, ended at the journal's last ns.
        let held = spans.iter().find(|sp| sp["name"] == "held").unwrap();
        assert_eq!(
            attr(held, "nika.task.unfinished").unwrap()["boolValue"],
            true
        );
        assert_eq!(held["endTimeUnixNano"], "3000");
    }

    #[test]
    fn deterministic_and_content_gated() {
        let a = project_bare(&run_fixture(), false).expect("projects");
        let b = project_bare(&run_fixture(), false).expect("projects");
        assert_eq!(a, b, "same journal → same line, byte for byte");

        let with_content = project_bare(&run_fixture(), true).expect("projects");
        let spans = spans_of(&with_content);
        let fetch = spans.iter().find(|sp| sp["name"] == "fetch").unwrap();
        assert_eq!(
            attr(fetch, "nika.task.output").unwrap()["stringValue"],
            "{\"x\":1}"
        );
    }

    /// T9-F04 · the content-free projection carries NO payload text:
    /// not the failure detail's message half (the status keeps the
    /// code), not a success warning. Canaries in every content-bearing
    /// field; the gated projection must not contain one of them, the
    /// content projection carries them all.
    #[test]
    fn content_free_projection_carries_no_payload_canary() {
        let mut events = run_fixture();
        events.insert(
            8,
            ev(
                10,
                2_350,
                EventKind::TaskFailed,
                &[
                    ("task", s("broken")),
                    ("note", s("invoke · nika:write")),
                    (
                        "detail",
                        s("NIKA-BUILTIN-WRITE-002 · CANARY-detail /home/op/secret.txt exists"),
                    ),
                    ("duration_ms", FieldValue::Int(3)),
                    ("items", s("[{\"index\":0,\"message\":\"CANARY-items\"}]")),
                ],
            ),
        );
        if let Some(done) = events
            .iter_mut()
            .find(|e| e.kind == EventKind::TaskCompleted)
        {
            *done = done
                .clone()
                .with_field(KeyValue::new("warning", s("CANARY-warning blank answer")));
        }
        let gated = project_bare(&events, false).expect("projects");
        assert!(
            !gated.contains("CANARY"),
            "a content-free projection carries no payload text: {gated}"
        );
        let broken = spans_of(&gated)
            .into_iter()
            .find(|sp| sp["name"] == "broken")
            .expect("the failed span");
        assert_eq!(
            broken["status"]["message"], "NIKA-BUILTIN-WRITE-002",
            "the status keeps the CODE, the one part that is vocabulary"
        );
        assert!(
            attr(&broken, "nika.task.note").is_some(),
            "the stage wording stays"
        );

        let with_content = project_bare(&events, true).expect("projects");
        for canary in ["CANARY-detail", "CANARY-warning"] {
            assert!(
                with_content.contains(canary),
                "{canary} rides under include_content"
            );
        }
    }

    #[test]
    fn a_detail_without_a_code_projects_the_generic_wording() {
        assert_eq!(failure_code("NIKA-X-001 · the message"), "NIKA-X-001");
        assert_eq!(failure_code("NIKA-X-001"), "NIKA-X-001");
        assert_eq!(failure_code("just a message · with a dot"), "task failed");
        assert_eq!(failure_code("NIKA-X 001 · spaced"), "task failed");
        assert_eq!(failure_code(""), "task failed");
    }

    /// T9-F03 · admission before allocation: a file past the writer's
    /// journal bound is refused by its SIZE, before a byte is read (a
    /// sparse file makes the point without writing 256 MiB), and no
    /// export is left behind. A non-regular path is refused by shape.
    #[test]
    fn export_refuses_a_journal_beyond_the_bound_before_reading() {
        let dir = tempfile::tempdir().expect("dir");
        let trace = dir.path().join("huge.ndjson");
        let file = std::fs::File::create(&trace).expect("create");
        file.set_len((crate::bounded::MAX_JOURNAL_BYTES as u64) + 1)
            .expect("sparse");
        drop(file);
        let trace_str = trace.to_str().expect("utf8");
        let err = export_journal(trace_str, None, false, "test").expect_err("refused");
        assert!(err.contains("journal bound"), "{err}");
        assert!(
            !std::path::Path::new(&default_out_path(trace_str)).exists(),
            "no export claims to exist"
        );
        let dir_str = dir.path().to_str().expect("utf8");
        let err = export_journal(dir_str, None, false, "test").expect_err("refused");
        assert!(err.contains("not a regular file"), "{err}");
    }

    /// The export is published by rename: the target appears whole and
    /// no temp sibling survives.
    #[test]
    fn export_publishes_whole_and_leaves_no_temp() {
        let dir = tempfile::tempdir().expect("dir");
        let trace = dir.path().join("run.ndjson");
        // A pre-chain journal in the sink's line shape (one event per
        // line) — the recovery path the export reads.
        let mut raw = String::new();
        for event in run_fixture() {
            raw.push_str(&serde_json::to_string(&event).expect("event json"));
            raw.push('\n');
        }
        std::fs::write(&trace, raw).expect("journal");
        let trace_str = trace.to_str().expect("utf8");
        let outcome = export_journal(trace_str, None, false, "test").expect("exports");
        assert!(outcome.target.ends_with("run.otlp.jsonl"));
        let line = std::fs::read_to_string(&outcome.target).expect("export");
        assert!(line.ends_with('\n') && line.contains("resourceSpans"));
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .expect("dir")
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| {
                std::path::Path::new(n)
                    .extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("tmp"))
            })
            .collect();
        assert!(
            leftovers.is_empty(),
            "no temp sibling survives: {leftovers:?}"
        );
    }

    #[test]
    fn a_journal_without_a_start_is_refused() {
        let orphan = vec![ev(9, 1, EventKind::TaskCompleted, &[("task", s("x"))])];
        assert!(project_bare(&orphan, false).is_err());
    }

    /// The access facts (D-2026-08-04-N1) an infer terminal carries
    /// (`model` = `<provider>/<name>` · `provider` = the prefix) project
    /// to the CURRENT `OTel` `GenAI` semconv names (`gen_ai.provider.name`
    /// — the canonical id normalized to the well-known value where one
    /// differs · `gen_ai.request.model`) so every viewer and eval tool
    /// reads the model without a translation shim — and NEVER the
    /// deprecated `gen_ai.system` (semconv v1.37.0 renamed it).
    /// `gen_ai.response.model` stays OUT of THIS frame: the semconv makes
    /// it the provider-reported SERVED model, and this frame carries no
    /// `model_served` (the one that does is judged below).
    #[test]
    fn infer_access_facts_project_to_current_genai_semconv() {
        let events = vec![
            ev(
                1,
                1_000,
                EventKind::WorkflowStarted,
                &[("workflow", s("brief"))],
            ),
            ev(2, 1_050, EventKind::TaskStarted, &[("task", s("draft"))]),
            ev(
                3,
                1_900,
                EventKind::TaskCompleted,
                &[
                    ("task", s("draft")),
                    ("note", s("infer · mistral/mistral-large")),
                    ("duration_ms", FieldValue::Int(800)),
                    ("model", s("mistral/mistral-large")),
                    ("provider", s("mistral")),
                ],
            ),
        ];
        let line = project_bare(&events, false).expect("projects");
        let spans = spans_of(&line);
        let draft = spans.iter().find(|sp| sp["name"] == "draft").unwrap();
        assert_eq!(
            attr(draft, "gen_ai.provider.name").unwrap()["stringValue"],
            "mistral_ai",
            "the canonical id normalizes to the semconv well-known value"
        );
        assert_eq!(
            attr(draft, "gen_ai.request.model").unwrap()["stringValue"],
            "mistral-large",
            "the model NAME (after the provider slash), semconv shape"
        );
        // The served model is a provider-reported fact: without
        // `model_served` on the frame, emitting the requested name here
        // would assert what was never captured.
        assert!(
            attr(draft, "gen_ai.response.model").is_none(),
            "gen_ai.response.model is the SERVED model — unreported here, so unemitted"
        );
        // The deprecated name must NEVER appear.
        assert!(
            attr(draft, "gen_ai.system").is_none(),
            "gen_ai.system was deprecated in semconv v1.37.0 — never emit it"
        );
    }

    /// the measured openai usage projects to the `GenAI` usage
    /// counters a collector prices with — and to our own mirrors. The
    /// figures are the measured ones (prompt 5015 of which 4992 cached ·
    /// one completion token): with only `nika.tokens` a warm-cache span
    /// and a price change were the same sight.
    #[test]
    fn the_usage_split_projects_to_genai_usage_counters() {
        let events = vec![
            ev(
                1,
                1_000,
                EventKind::WorkflowStarted,
                &[("workflow", s("meter"))],
            ),
            ev(2, 1_050, EventKind::TaskStarted, &[("task", s("ask"))]),
            ev(
                3,
                1_900,
                EventKind::TaskCompleted,
                &[
                    ("task", s("ask")),
                    ("note", s("infer · openai/gpt-4o-mini")),
                    ("duration_ms", FieldValue::Int(800)),
                    ("tokens", FieldValue::Int(1)),
                    ("tokens_in", FieldValue::Int(5015)),
                    ("tokens_out", FieldValue::Int(1)),
                    ("tokens_cache_read", FieldValue::Int(4992)),
                    ("cost_usd", FieldValue::Float(0.000_752_85)),
                    ("model", s("openai/gpt-4o-mini")),
                    ("provider", s("openai")),
                    ("model_served", s("gpt-4o-mini-2024-07-18")),
                    ("response_id", s("chatcmpl-p")),
                ],
            ),
        ];
        let line = project_bare(&events, false).expect("projects");
        let spans = spans_of(&line);
        let ask = spans.iter().find(|sp| sp["name"] == "ask").unwrap();
        let int = |key: &str| {
            attr(ask, key).map(|v| {
                v["intValue"]
                    .as_str()
                    .and_then(|s| s.parse::<i64>().ok())
                    .or_else(|| v["intValue"].as_i64())
                    .expect("an int attribute")
            })
        };
        assert_eq!(int("gen_ai.usage.input_tokens"), Some(5015));
        assert_eq!(int("gen_ai.usage.cache_read.input_tokens"), Some(4992));
        assert_eq!(int("gen_ai.usage.output_tokens"), Some(1));
        assert_eq!(int("nika.tokens.in"), Some(5015));
        assert_eq!(int("nika.tokens.cache_read"), Some(4992));
        assert_eq!(int("nika.tokens"), Some(1), "the historical mirror stays");
        // An unreported meter is NOT projected as a zero.
        assert!(attr(ask, "gen_ai.usage.cache_write.input_tokens").is_none());
        assert!(attr(ask, "gen_ai.usage.reasoning.output_tokens").is_none());
        // Now the SERVED model rides — from the provider's own report.
        assert_eq!(
            attr(ask, "gen_ai.response.model").unwrap()["stringValue"],
            "gpt-4o-mini-2024-07-18"
        );
        assert_eq!(
            attr(ask, "gen_ai.response.id").unwrap()["stringValue"],
            "chatcmpl-p"
        );
        assert_eq!(
            attr(ask, "gen_ai.request.model").unwrap()["stringValue"],
            "gpt-4o-mini",
            "requested and served stay distinct"
        );
    }

    /// A slash-less `model` (a bare local id) projects whole — the split
    /// is the `provider/` prefix, nothing else.
    #[test]
    fn a_slash_less_model_projects_whole() {
        let events = vec![
            ev(
                1,
                1_000,
                EventKind::WorkflowStarted,
                &[("workflow", s("brief"))],
            ),
            ev(2, 1_050, EventKind::TaskStarted, &[("task", s("draft"))]),
            ev(
                3,
                1_900,
                EventKind::TaskCompleted,
                &[("task", s("draft")), ("model", s("qwen2.5"))],
            ),
        ];
        let line = project_bare(&events, false).expect("projects");
        let spans = spans_of(&line);
        let draft = spans.iter().find(|sp| sp["name"] == "draft").unwrap();
        assert_eq!(
            attr(draft, "gen_ai.request.model").unwrap()["stringValue"],
            "qwen2.5",
            "no provider slash → the value stays whole"
        );
        assert!(attr(draft, "gen_ai.response.model").is_none());
    }

    /// The inner-slash convention: the split is the FIRST slash, so a
    /// three-segment spec keeps its tail (`openrouter/anthropic/claude`
    /// → `anthropic/claude`) — the same `split_once` the resolver hands
    /// huggingface's `Qwen/Qwen3.5-9B` through untouched.
    #[test]
    fn a_three_segment_spec_splits_at_the_first_slash() {
        let events = vec![
            ev(
                1,
                1_000,
                EventKind::WorkflowStarted,
                &[("workflow", s("brief"))],
            ),
            ev(2, 1_050, EventKind::TaskStarted, &[("task", s("draft"))]),
            ev(
                3,
                1_900,
                EventKind::TaskCompleted,
                &[
                    ("task", s("draft")),
                    ("model", s("openrouter/anthropic/claude")),
                    ("provider", s("openrouter")),
                ],
            ),
        ];
        let line = project_bare(&events, false).expect("projects");
        let spans = spans_of(&line);
        let draft = spans.iter().find(|sp| sp["name"] == "draft").unwrap();
        assert_eq!(
            attr(draft, "gen_ai.request.model").unwrap()["stringValue"],
            "anthropic/claude",
            "split at the FIRST slash — the inner-slash tail stays whole"
        );
        assert_eq!(
            attr(draft, "gen_ai.provider.name").unwrap()["stringValue"],
            "openrouter",
            "no well-known value for openrouter — passes through verbatim"
        );
        assert!(attr(draft, "gen_ai.response.model").is_none());
    }

    /// The provider normalization table: canonical ids with a diverging
    /// semconv well-known value map to it (registry checked 2026-08-06);
    /// ids that already ARE the well-known — or have none — pass through
    /// verbatim.
    #[test]
    fn provider_ids_normalize_to_the_well_known_values() {
        let provider_name = |provider: &str| -> String {
            let events = vec![
                ev(
                    1,
                    1_000,
                    EventKind::WorkflowStarted,
                    &[("workflow", s("brief"))],
                ),
                ev(2, 1_050, EventKind::TaskStarted, &[("task", s("draft"))]),
                ev(
                    3,
                    1_900,
                    EventKind::TaskCompleted,
                    &[("task", s("draft")), ("provider", s(provider))],
                ),
            ];
            let line = project_bare(&events, false).expect("projects");
            let spans = spans_of(&line);
            let draft = spans.iter().find(|sp| sp["name"] == "draft").unwrap();
            attr(draft, "gen_ai.provider.name").unwrap()["stringValue"]
                .as_str()
                .unwrap()
                .to_owned()
        };
        // The three divergences — gemini's well-known names the exact
        // endpoint the profile dials (generativelanguage.googleapis.com).
        assert_eq!(provider_name("gemini"), "gcp.gemini");
        assert_eq!(provider_name("mistral"), "mistral_ai");
        assert_eq!(provider_name("xai"), "x_ai");
        // Already the well-known, or none exists: verbatim.
        assert_eq!(provider_name("openai"), "openai");
        assert_eq!(provider_name("anthropic"), "anthropic");
        assert_eq!(provider_name("deepseek"), "deepseek");
        assert_eq!(provider_name("groq"), "groq");
        assert_eq!(provider_name("ollama"), "ollama");
        assert_eq!(provider_name("openrouter"), "openrouter");
        assert_eq!(provider_name("huggingface"), "huggingface");
        assert_eq!(provider_name("mock"), "mock");
    }

    /// The chain verdict rides the resource: intact/torn anchor their
    /// head, broken is flagged, a pre-chain journal claims nothing.
    #[test]
    fn the_chain_verdict_rides_the_resource_attributes() {
        let resource_attr = |chain: Option<&Verdict>, key: &str| -> Option<String> {
            let line = project(&run_fixture(), false, chain, "test-exporter").expect("projects");
            let v: serde_json::Value = serde_json::from_str(&line).expect("valid JSON");
            v["resourceSpans"][0]["resource"]["attributes"]
                .as_array()
                .expect("attrs")
                .iter()
                .find(|kv| kv["key"] == key)
                .and_then(|kv| kv["value"]["stringValue"].as_str())
                .map(ToOwned::to_owned)
        };
        let intact = crate::chain::walk(&chained_raw());
        assert!(matches!(intact, Verdict::Intact { .. }), "fixture intact");
        let head = match &intact {
            Verdict::Intact { head, .. } => head.clone(),
            _ => unreachable!(),
        };
        assert_eq!(
            resource_attr(Some(&intact), "nika.trace.chain_head").as_deref(),
            Some(head.as_str()),
            "the anchor travels with the export"
        );
        assert_eq!(resource_attr(None, "nika.trace.chain_head"), None);
        let broken = crate::chain::walk(&broken_raw());
        assert!(matches!(broken, Verdict::Broken { .. }), "fixture broken");
        assert_eq!(
            resource_attr(Some(&broken), "nika.trace.chain").as_deref(),
            Some("broken")
        );
    }

    /// A minimal chained journal — the sink's shape: `chain` is a
    /// TOP-LEVEL key, the first line carries the sha of the genesis
    /// tag's bytes, each next line the sha of the previous line's
    /// exact bytes.
    fn chained_raw() -> String {
        let first = serde_json::json!({
            "chain": nika_event::source_id::sha256_hex(crate::chain::CHAIN_GENESIS),
            "kind": "workflow_started",
        })
        .to_string();
        let second = serde_json::json!({
            "chain": nika_event::source_id::sha256_hex(first.as_bytes()),
            "kind": "workflow_completed",
        })
        .to_string();
        format!("{first}\n{second}\n")
    }

    fn broken_raw() -> String {
        let first = serde_json::json!({
            "chain": nika_event::source_id::sha256_hex(crate::chain::CHAIN_GENESIS),
            "kind": "workflow_started",
        })
        .to_string();
        let second = serde_json::json!({
            "chain": "0".repeat(64),
            "kind": "workflow_completed",
        })
        .to_string();
        format!("{first}\n{second}\n")
    }
}

/// The exported artifact's facts (the CLI voices them).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportOutcome {
    /// Where the `.otlp.jsonl` landed.
    pub target: String,
    /// The chain break line, when the journal fails verification
    /// (the export still lands — the WARNING rides).
    pub broken_at: Option<usize>,
}

/// `run.ndjson` → `run.otlp.jsonl`, beside the journal.
#[must_use]
pub fn default_out_path(trace: &str) -> String {
    trace.strip_suffix(".ndjson").map_or_else(
        || format!("{trace}.otlp.jsonl"),
        |stem| format!("{stem}.otlp.jsonl"),
    )
}

/// The export plumbing (descended from `verbs::trace_otel` 2026-07-21
/// · the 15k wall): read · recover · verify-then-trust (the git
/// index-pack model — a BROKEN chain still exports, but says so) ·
/// project · write. `engine` is the caller's version string (the
/// projection stamps it).
///
/// # Errors
///
/// A reason string on a read, recovery, projection, or write failure.
pub fn export_journal(
    trace: &str,
    out: Option<&str>,
    include_content: bool,
    engine: &str,
) -> Result<ExportOutcome, String> {
    // Admission BEFORE allocation (T9-F03): the writer refuses a journal
    // past `MAX_JOURNAL_BYTES`, so a larger file was never written by
    // this engine — reading it whole would only prove that by running
    // out of memory. A non-regular path (a FIFO · a device) would hang
    // `read_to_string` forever; it is refused by shape, not by waiting.
    let meta = std::fs::metadata(trace) // seam-bypass-ok: L4 verb reading the journal it exports
        .map_err(|e| format!("cannot read {trace}: {e}"))?;
    if !meta.is_file() {
        return Err(format!("cannot read {trace}: not a regular file"));
    }
    let bound = crate::bounded::MAX_JOURNAL_BYTES;
    if meta.len() > bound as u64 {
        return Err(format!(
            "refusing to read {trace}: {} bytes exceed the journal bound of {bound} bytes — \
             no journal this engine wrote is larger, so the file is not one of its journals",
            meta.len()
        ));
    }
    let raw = std::fs::read_to_string(trace) // seam-bypass-ok: L4 verb reading the journal it exports
        .map_err(|e| format!("cannot read {trace}: {e}"))?;
    let recovered = crate::recover::recover_events(&raw, trace).map_err(|e| e.to_string())?;
    let verdict = crate::chain::walk(&raw);
    let broken_at = match &verdict {
        crate::chain::Verdict::Broken { line, .. } => Some(*line),
        _ => None,
    };
    let line = project(&recovered.events, include_content, Some(&verdict), engine)?;
    let target = out.map_or_else(|| default_out_path(trace), ToOwned::to_owned);
    write_whole(&target, &format!("{line}\n"))?;
    Ok(ExportOutcome { target, broken_at })
}

/// Publish the export ATOMICALLY: a sibling temp file, then a rename —
/// a reader never sees a half-written export claiming to be complete
/// (an interrupted `fs::write` left exactly that). The temp name is
/// per-process so two exports of one journal cannot share it.
fn write_whole(target: &str, contents: &str) -> Result<(), String> {
    let path = std::path::Path::new(target);
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(
            || std::path::PathBuf::from("."),
            std::path::Path::to_path_buf,
        );
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("cannot write {target}: no file name"))?;
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, contents) // seam-bypass-ok: L4 verb writing the export beside the journal
        .map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        // seam-bypass-ok: L4 verb publishing the export beside the journal
        let _ = std::fs::remove_file(&tmp); // seam-bypass-ok: cleanup of our own temp
        return Err(format!("cannot write {target}: {e}"));
    }
    Ok(())
}
