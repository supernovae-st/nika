//! `nika trace export` — project a run journal to OTLP/JSON lines.
//!
//! Every `OTel` tool becomes a nika viewer with zero vendor capture: the
//! artifact is a LOCAL `.otlp.jsonl` file (one `{"resourceSpans":[…]}`
//! per line — the de-facto file-exporter shape). Jaeger UI ≥ 1.60 takes
//! it by drag-and-drop; any OTLP/HTTP endpoint takes it line-by-line at
//! `:4318/v1/traces` (Jaeger all-in-one · Aspire · Grafana · Langfuse).
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
//! `--include-content` — the default export carries structure, timing,
//! spend and identity, never payloads.

use std::fmt::Write as _;

use nika_event::{Event, EventKind};
use nika_types::resource::Value as FieldValue;

use super::VerbOutput;

/// The de-facto cost attribute (Langfuse maps it to USD total); the
/// spec has no stable cost attribute yet (checked 2026-07-06).
const COST_ATTR: &str = "gen_ai.usage.cost";

pub fn export(trace: &str, out: Option<&str>, include_content: bool) -> VerbOutput {
    let raw = match std::fs::read_to_string(trace) {
        Ok(raw) => raw,
        Err(e) => return VerbOutput::env(format!("cannot read {trace}: {e}")),
    };
    let recovered = match super::run::recover_events(&raw, trace) {
        Ok(recovered) => recovered,
        Err(e) => return VerbOutput::env(e.to_string()),
    };
    // Verify before trusting (the git index-pack model — affordable at
    // 0.4µs/line): a BROKEN chain still exports, but says so; the head
    // rides the export so the anchor travels with the trace.
    let chain = match super::trace_verify::walk(&raw) {
        super::trace_verify::Verdict::Intact { head, .. }
        | super::trace_verify::Verdict::TornTail { head, .. } => Some(("intact", head)),
        super::trace_verify::Verdict::Broken { line, .. } => {
            Some(("broken", format!("chain broken at line {line}")))
        }
        _ => None, // unchained (pre-chain journal) — nothing to claim
    };
    let line = match project_with_chain(&recovered.events, include_content, chain.as_ref()) {
        Ok(line) => line,
        Err(e) => return VerbOutput::env(e),
    };
    let target = out.map_or_else(|| default_out_path(trace), ToOwned::to_owned);
    if let Err(e) = std::fs::write(&target, format!("{line}\n")) {
        return VerbOutput::env(format!("cannot write {target}: {e}"));
    }
    let mut msg = format!(
        "exported → {target}\n  view: drag into Jaeger UI (≥1.60) · or POST the line to any OTLP/HTTP endpoint (`:4318/v1/traces`)"
    );
    if let Some(("broken", detail)) = chain.as_ref().map(|(k, d)| (*k, d)) {
        use std::fmt::Write as _;
        let _ = write!(
            msg,
            "\n  WARNING — this journal fails verification ({detail}); its claims are unverified"
        );
    }
    VerbOutput::ok(msg)
}

/// `run.ndjson` → `run.otlp.jsonl`, beside the journal.
fn default_out_path(trace: &str) -> String {
    trace.strip_suffix(".ndjson").map_or_else(
        || format!("{trace}.otlp.jsonl"),
        |stem| format!("{stem}.otlp.jsonl"),
    )
}

/// One journal → one `{"resourceSpans":[…]}` JSON value (one line) —
/// the chain-less shape the unit pins drive.
#[cfg(test)]
pub(crate) fn project(events: &[Event], include_content: bool) -> Result<String, String> {
    project_with_chain(events, include_content, None)
}

/// The full projection — `chain` carries the verify verdict so the
/// anchor (or the broken flag) travels with the export.
pub(crate) fn project_with_chain(
    events: &[Event],
    include_content: bool,
    chain: Option<&(&str, String)>,
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
    let ran_version = field_str(started, "engine_version").unwrap_or(env!("CARGO_PKG_VERSION"));
    let mut resource_attrs = vec![
        kv_str("service.name", "nika"),
        kv_str("service.version", ran_version),
    ];
    if let Some(platform) = field_str(started, "platform") {
        resource_attrs.push(kv_str("nika.platform", platform));
    }
    match chain {
        Some(("intact", head)) => {
            resource_attrs.push(kv_str("nika.trace.chain_head", head));
        }
        Some(("broken", _)) => {
            resource_attrs.push(kv_str("nika.trace.chain", "broken"));
        }
        _ => {}
    }
    let payload = serde_json::json!({
        "resourceSpans": [{
            "resource": { "attributes": resource_attrs },
            "scopeSpans": [{
                "scope": { "name": "nika", "version": env!("CARGO_PKG_VERSION") },
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
    if let Some(FieldValue::Float(usd)) = field(terminal, "cost_usd") {
        attributes.push(kv_double(COST_ATTR, *usd));
        attributes.push(kv_double("nika.cost.usd", *usd));
    }
    let mut status = serde_json::json!({});
    match terminal.kind {
        EventKind::TaskCompleted => status = serde_json::json!({ "code": 1 }),
        EventKind::TaskFailed => {
            let detail = field_str(terminal, "detail").unwrap_or("task failed");
            status = serde_json::json!({ "code": 2, "message": detail });
        }
        EventKind::TaskSkipped => {
            attributes.push(kv_bool("nika.task.skipped", true));
            if let Some(when) = field_str(terminal, "when") {
                attributes.push(kv_str("nika.task.when", when));
            }
        }
        EventKind::TaskCancelled => {
            attributes.push(kv_bool("nika.task.cancelled", true));
            if let Some(culprit) = field_str(terminal, "blocked_by") {
                attributes.push(kv_str("nika.task.blocked_by", culprit));
            }
        }
        EventKind::TaskCacheHit => {
            attributes.push(kv_bool("nika.cache.hit", true));
            status = serde_json::json!({ "code": 1 });
        }
        _ => {}
    }
    if include_content && let Some(output) = field_str(terminal, "output") {
        attributes.push(kv_str("nika.task.output", output));
    }
    // OBS-E non-fatal diagnostics ride the success frame — surface them.
    if let Some(warning) = field_str(terminal, "warning") {
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
        let line = project(&run_fixture(), false).expect("projects");
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
        let a = project(&run_fixture(), false).expect("projects");
        let b = project(&run_fixture(), false).expect("projects");
        assert_eq!(a, b, "same journal → same line, byte for byte");

        let with_content = project(&run_fixture(), true).expect("projects");
        let spans = spans_of(&with_content);
        let fetch = spans.iter().find(|sp| sp["name"] == "fetch").unwrap();
        assert_eq!(
            attr(fetch, "nika.task.output").unwrap()["stringValue"],
            "{\"x\":1}"
        );
    }

    #[test]
    fn a_journal_without_a_start_is_refused() {
        let orphan = vec![ev(9, 1, EventKind::TaskCompleted, &[("task", s("x"))])];
        assert!(project(&orphan, false).is_err());
    }
}
