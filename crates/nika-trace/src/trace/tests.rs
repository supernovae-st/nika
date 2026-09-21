// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The batteries of the static trace readers (`outputs` · `peek` · `flow`), a child
//! module of [`super`] in its own file: the file-LOC gate measures `wc -l` and does
//! not subtract `#[cfg(test)]`, and the sibling suites already live apart.
use super::*;
use crate::demo;
use crate::exit;

fn plain() -> Theme {
    Theme::new(false, false, false)
}

/// Stage a real NDJSON trace from the demo storyboard events.
fn stage(name: &str, events: &[nika_event::Event]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("nika-cli-trace-verb");
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let path = dir.join(name);
    let mut body = String::new();
    for ev in events {
        body.push_str(&serde_json::to_string(ev).expect("event serializes"));
        body.push('\n');
    }
    std::fs::write(&path, body).expect("trace staged");
    path
}

/// One row per task: verb (the started note) · duration · tokens ·
/// preview or the honest dash — plus the totals + peek hint with
/// the REAL trace path.
/// `outputs --json` (#1247): one document — the run's state and one
/// row per task with its id, verb and status (the projection the engine
/// carried with no verb to print it).
#[test]
fn outputs_json_projects_every_task() {
    let path = stage("outputs-json.ndjson", &demo::success());
    let out = outputs_json(&path.to_string_lossy());
    assert_eq!(out.code, exit::OK);
    let doc: serde_json::Value = serde_json::from_str(&out.text).expect("one JSON document");
    assert_eq!(doc["outputs_version"], 1);
    assert_eq!(doc["state"], "succeeded", "{doc}");
    assert_eq!(doc["settlement"]["status"], "succeeded", "{doc}");
    let tasks = doc["tasks"].as_array().expect("the task rows");
    assert!(!tasks.is_empty(), "{doc}");
    for task in tasks {
        assert!(task["id"].is_string(), "{task}");
        assert!(task["status"].is_string(), "{task}");
        assert!(task.get("recovered_from").is_some(), "{task}");
    }
    assert!(
        tasks.iter().any(|t| t["status"] == "ok"),
        "the demo completes tasks: {doc}"
    );
}

#[test]
fn outputs_table_renders_per_task_rows_and_totals() {
    let path = stage("outputs-demo.ndjson", &demo::success());
    let trace = path.to_string_lossy();
    let out = outputs(&trace, plain());
    assert_eq!(out.code, exit::OK);
    let text = &out.text;
    assert!(
        text.contains("task") && text.contains("verb") && text.contains("output"),
        "header row: {text}"
    );
    assert!(
        text.contains("invoke · nika:fetch"),
        "verb column carries the started note: {text}"
    );
    // The demo reports tokens on exactly one completion (710).
    assert!(text.contains("710"), "token cell: {text}");
    // Demo completions carry no ADR-099 output field → honest dash.
    assert!(text.contains('—'), "no output → dash: {text}");
    assert!(
        text.contains(&format!("full value: nika trace peek {trace} <task>")),
        "peek hint carries the real path: {text}"
    );
    assert!(text.contains("5 tasks"), "totals: {text}");
    assert!(text.contains("710 tok"), "token total: {text}");
}

/// Output-carrying completions preview their bounded shape + size.
#[test]
fn outputs_table_previews_shapes_with_sizes() {
    use nika_event::EventKind;
    use nika_types::resource::{KeyValue, Value};
    let events = vec![
        demo::bare_event(EventKind::TaskStarted, 0)
            .with_field(KeyValue::new("task", Value::String("audit".into())))
            .with_field(KeyValue::new(
                "note",
                Value::String("infer · mock/echo".into()),
            )),
        demo::bare_event(EventKind::TaskCompleted, 40)
            .with_field(KeyValue::new("task", Value::String("audit".into())))
            .with_field(KeyValue::new(
                "output",
                Value::String(r#"{"total":9,"fixes":["a","b"]}"#.into()),
            ))
            .with_field(KeyValue::new("tokens", Value::Int(90)))
            .with_field(KeyValue::new("duration_ms", Value::Int(38))),
    ];
    let path = stage("outputs-shapes.ndjson", &events);
    let out = outputs(&path.to_string_lossy(), plain());
    assert!(
        out.text.contains("{fixes[2], total} · 29B"),
        "bounded preview + byte size: {}",
        out.text
    );
    assert!(out.text.contains("38ms"), "measured duration: {}", out.text);
    // ASCII parity: the dash cell + no unicode leak.
    let ascii = outputs(&path.to_string_lossy(), Theme::new(false, true, false));
    assert!(!ascii.text.contains('—'), "ascii dash: {}", ascii.text);
}

/// An unreadable path is the environment class — actionable message,
/// exit 3, never a panic.
#[test]
fn missing_trace_is_env_class() {
    let out = outputs("/nonexistent/trace.ndjson", plain());
    assert_eq!(out.code, exit::ENV);
    assert!(out.text.contains("cannot read"), "{}", out.text);
}

/// The interactive accents bracket the dur column (`[38ms]` ·
/// nextest school) while the sober register (accents off · every
/// pipe) keeps the bare right-aligned cell — pinned on the SAME
/// staged trace.
#[test]
fn outputs_table_brackets_durations_under_accents_only() {
    use nika_event::EventKind;
    use nika_types::resource::{KeyValue, Value};
    let events = vec![
        demo::bare_event(EventKind::TaskStarted, 0)
            .with_field(KeyValue::new("task", Value::String("audit".into()))),
        demo::bare_event(EventKind::TaskCompleted, 40)
            .with_field(KeyValue::new("task", Value::String("audit".into())))
            .with_field(KeyValue::new("duration_ms", Value::Int(38))),
    ];
    let path = stage("outputs-accents.ndjson", &events);
    let sober = outputs(&path.to_string_lossy(), plain());
    assert!(
        !sober.text.contains("[38ms]"),
        "sober register: no brackets: {}",
        sober.text
    );
    let mut accented = plain();
    accented.accents = true;
    let rich = outputs(&path.to_string_lossy(), accented);
    assert!(
        rich.text.contains("[38ms]"),
        "accents bracket the dur cell: {}",
        rich.text
    );
}

/// A trace with the ADR-099 checkpoint trio for one task.
fn peek_fixture(name: &str) -> std::path::PathBuf {
    use nika_event::EventKind;
    use nika_types::resource::{KeyValue, Value};
    let events = vec![
        demo::bare_event(EventKind::TaskStarted, 0)
            .with_field(KeyValue::new("task", Value::String("audit".into())))
            .with_field(KeyValue::new(
                "note",
                Value::String("infer · mock/echo".into()),
            )),
        demo::bare_event(EventKind::TaskCompleted, 40)
            .with_field(KeyValue::new("task", Value::String("audit".into())))
            .with_field(KeyValue::new(
                "output",
                Value::String(r#"{"fixes":["a"],"total":9}"#.into()),
            ))
            .with_field(KeyValue::new("tokens", Value::Int(90)))
            .with_field(KeyValue::new("duration_ms", Value::Int(38)))
            .with_field(KeyValue::new(
                "def_hash",
                Value::String("5b2fa9e9232ed4174f3af03bf835".into()),
            ))
            .with_field(KeyValue::new(
                "input_hash",
                Value::String("7f14c732ad33dd042b82325cda86".into()),
            )),
        demo::bare_event(EventKind::TaskSkipped, 50)
            .with_field(KeyValue::new("task", Value::String("deploy".into())))
            .with_field(KeyValue::new(
                "note",
                Value::String("when: gate closed".into()),
            )),
    ];
    stage(name, &events)
}

/// The pretty peek: identity block (verb · time · tokens · clipped
/// hashes) then the FULL value pretty-printed.
#[test]
fn peek_renders_identity_block_and_pretty_value() {
    let path = peek_fixture("peek-pretty.ndjson");
    let out = peek(&path.to_string_lossy(), "audit", false, plain());
    assert_eq!(out.code, exit::OK);
    let text = &out.text;
    assert!(text.contains("audit · infer · mock/echo"), "title: {text}");
    assert!(text.contains("38ms · 90 tok · 25B"), "meta: {text}");
    assert!(
        text.contains("def_hash 5b2fa9e9232e… · input_hash 7f14c732ad33…"),
        "clipped hashes: {text}"
    );
    assert!(
        text.contains("\"fixes\": [") && text.contains("\"total\": 9"),
        "pretty value: {text}"
    );
    // ASCII parity: the hash clip mark degrades, no unicode leak.
    let ascii = peek(
        &path.to_string_lossy(),
        "audit",
        false,
        Theme::new(false, true, false),
    );
    assert!(
        ascii.text.contains("5b2fa9e9232e.."),
        "ascii clip: {}",
        ascii.text
    );
    assert!(!ascii.text.contains('…'), "no unicode under --ascii");
}

/// A call the transport re-sent carries its account on the sealed
/// frame (`attempts` · `waited_ms` · `retried_on`); the peek renders it
/// in the identity block instead of hiding it behind a whitelist. A
/// single attempt says nothing (the ordinary case stays quiet).
#[test]
fn peek_renders_the_transport_account_of_a_retried_call() {
    use nika_event::EventKind;
    use nika_types::resource::{KeyValue, Value};
    let events = vec![
        demo::bare_event(EventKind::TaskStarted, 0)
            .with_field(KeyValue::new("task", Value::String("draft".into())))
            .with_field(KeyValue::new(
                "note",
                Value::String("infer · mock/echo".into()),
            )),
        demo::bare_event(EventKind::TaskCompleted, 2_100)
            .with_field(KeyValue::new("task", Value::String("draft".into())))
            .with_field(KeyValue::new("output", Value::String("ok".into())))
            .with_field(KeyValue::new("tokens", Value::Int(12)))
            .with_field(KeyValue::new("duration_ms", Value::Int(2_080)))
            .with_field(KeyValue::new("attempts", Value::Int(2)))
            .with_field(KeyValue::new("waited_ms", Value::Int(2_000)))
            .with_field(KeyValue::new("retried_on", Value::String("429".into()))),
        demo::bare_event(EventKind::TaskStarted, 2_200)
            .with_field(KeyValue::new("task", Value::String("save".into()))),
        demo::bare_event(EventKind::TaskCompleted, 2_210)
            .with_field(KeyValue::new("task", Value::String("save".into())))
            .with_field(KeyValue::new("output", Value::String("ok".into())))
            .with_field(KeyValue::new("duration_ms", Value::Int(9)))
            .with_field(KeyValue::new("attempts", Value::Int(1))),
    ];
    let path = stage("peek-retried.ndjson", &events);
    let out = peek(&path.to_string_lossy(), "draft", false, plain());
    assert_eq!(out.code, exit::OK);
    assert!(
        out.text
            .contains("2.1s · 12 tok · 2 attempts (waited 2.0s on 429) · 2B"),
        "transport account in the meta line: {}",
        out.text
    );
    let quiet = peek(&path.to_string_lossy(), "save", false, plain());
    assert_eq!(quiet.code, exit::OK);
    assert!(
        !quiet.text.contains("attempt"),
        "one attempt is the ordinary case, never announced: {}",
        quiet.text
    );
}

/// A failed task's peek performs the autopsy the failure card
/// promised: the recorded failure + the explain teach line — never
/// the « older engine's trace? » shrug. `--raw` keeps its jq-pipe
/// contract and still refuses (a failure has no value).
#[test]
fn peek_on_a_failed_task_performs_the_autopsy() {
    use nika_event::EventKind;
    use nika_types::resource::{KeyValue, Value};
    let events = vec![
        demo::bare_event(EventKind::TaskStarted, 0)
            .with_field(KeyValue::new("task", Value::String("greet".into())))
            .with_field(KeyValue::new(
                "note",
                Value::String("infer · mistral/mistral-small-latest".into()),
            )),
        demo::bare_event(EventKind::TaskFailed, 12)
            .with_field(KeyValue::new("task", Value::String("greet".into())))
            .with_field(KeyValue::new("duration_ms", Value::Int(9)))
            .with_field(KeyValue::new(
                "detail",
                Value::String(
                    "NIKA-INFER-001 · model `mistral/mistral-small-latest` failed to \
                     resolve: no API key for 'mistral'"
                        .into(),
                ),
            )),
    ];
    let path = stage("peek-autopsy.ndjson", &events);
    let out = peek(&path.to_string_lossy(), "greet", false, plain());
    assert_eq!(out.code, exit::OK);
    assert!(
        out.text
            .contains("greet · infer · mistral/mistral-small-latest"),
        "identity: {}",
        out.text
    );
    assert!(
        out.text.contains("no API key for 'mistral'"),
        "the recorded failure: {}",
        out.text
    );
    assert!(
        out.text.contains("fix: nika explain NIKA-INFER-001"),
        "teach line: {}",
        out.text
    );
    assert!(!out.text.contains("older engine"), "no shrug: {}", out.text);
    let raw = peek(&path.to_string_lossy(), "greet", true, plain());
    assert_eq!(raw.code, exit::ENV, "raw refuses a valueless row");
    assert!(
        raw.text.contains("settled before it produced a value"),
        "raw teach: {}",
        raw.text
    );
}

/// The wire-code scanner: finds real codes, never prose.
#[test]
fn wire_code_finds_codes_and_ignores_prose() {
    assert_eq!(
        nika_dap::recover::first_wire_code("NIKA-INFER-001 · model x failed"),
        Some("NIKA-INFER-001")
    );
    assert_eq!(
        nika_dap::recover::first_wire_code("cycle found (DAG-003) in wave 2"),
        Some("DAG-003")
    );
    assert_eq!(
        nika_dap::recover::first_wire_code("plain prose failure - nothing coded"),
        None
    );
}

/// A guarded skip explains itself — no hypothesis, no blame.
#[test]
fn peek_on_a_skipped_task_explains_the_skip() {
    let path = peek_fixture("peek-skip.ndjson");
    let out = peek(&path.to_string_lossy(), "deploy", false, plain());
    assert_eq!(out.code, exit::ENV);
    assert!(
        out.text
            .contains("a guarded skip never runs, so never records"),
        "skip teach: {}",
        out.text
    );
    assert!(
        out.text.contains("outputs recorded for: audit"),
        "still names the rows that have one: {}",
        out.text
    );
}

/// `--raw` prints the EXACT recorded JSON text and nothing else —
/// the jq-pipe contract.
#[test]
fn peek_raw_is_the_exact_value_only() {
    let path = peek_fixture("peek-raw.ndjson");
    let out = peek(&path.to_string_lossy(), "audit", true, plain());
    assert_eq!(out.code, exit::OK);
    assert_eq!(out.text, r#"{"fixes":["a"],"total":9}"#);
}

/// Stage a workflow file whose bindings draw the mockup DAG:
/// `read_payload` → `audit` → `outputs.geo_score`.
fn flow_workflow(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("nika-cli-trace-verb");
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let path = dir.join(name);
    std::fs::write(
        &path,
        "nika: geo-audit\nmodel: mock/echo\ntasks:\n  read_payload:\n    invoke: { tool: \"nika:read\", args: { path: \"x.json\" } }\n  audit:\n    with:\n      payload: ${{ tasks.read_payload.output }}\n    infer: { prompt: \"score ${{ with.payload }}\" }\noutputs:\n  geo_score: ${{ tasks.audit.output }}\n",
    )
    .expect("workflow staged");
    path
}

/// A trace with recorded outputs for both tasks (real sizes).
fn flow_trace(name: &str) -> std::path::PathBuf {
    use nika_event::EventKind;
    use nika_types::resource::{KeyValue, Value};
    let s = |k: &str, v: &str| KeyValue::new(k, Value::String(v.to_owned()));
    let events = vec![
        demo::bare_event(EventKind::WorkflowStarted, 0).with_field(s("workflow", "geo-audit")),
        demo::bare_event(EventKind::TaskCompleted, 10)
            .with_field(s("task", "read_payload"))
            .with_field(s("output", &format!("\"{}\"", "p".repeat(3598)))),
        demo::bare_event(EventKind::TaskCompleted, 40)
            .with_field(s("task", "audit"))
            .with_field(s("output", r#"{"total":9}"#)),
    ];
    stage(name, &events)
}

/// The waterfall: plan-binding edges × trace sizes + the
/// outputs.<name> terminal edge + the totals line naming the widest.
#[test]
fn flow_joins_plan_edges_with_trace_sizes() {
    let wf = flow_workflow("flow.nika");
    let tr = flow_trace("flow.ndjson");
    let out = flow(&tr.to_string_lossy(), &wf.to_string_lossy(), plain());
    assert_eq!(out.code, exit::OK, "{}", out.text);
    let text = &out.text;
    assert!(
        text.contains("read_payload ─3.6KB→ audit"),
        "sized task edge: {text}"
    );
    assert!(
        text.contains("audit        ─11B→ outputs.geo_score"),
        "terminal outputs edge (aligned): {text}"
    );
    assert!(
        text.contains("2 edges · widest: read_payload→audit"),
        "totals + widest: {text}"
    );
    assert!(
        text.contains("derived from plan bindings × trace sizes"),
        "honesty label: {text}"
    );
    // No mismatch note when the names agree.
    assert!(!text.contains("note:"), "{text}");

    // ASCII parity — rails, arrows and the join glyph all degrade.
    let ascii = flow(
        &tr.to_string_lossy(),
        &wf.to_string_lossy(),
        Theme::new(false, true, false),
    );
    assert!(
        ascii.text.contains("read_payload -3.6KB-> audit"),
        "{}",
        ascii.text
    );
    assert!(
        ascii.text.contains("plan bindings x trace sizes"),
        "{}",
        ascii.text
    );
    for glyph in ['─', '→', '×'] {
        assert!(
            !ascii.text.contains(glyph),
            "unicode {glyph} leaked into --ascii: {}",
            ascii.text
        );
    }
}

/// A trace from an older engine (no output fields): the STRUCTURE
/// still renders (bare arrows · never an invented size), and a
/// name mismatch between trace and file says so.
#[test]
fn flow_degrades_honestly_without_sizes_and_flags_mismatch() {
    use nika_types::resource::{KeyValue, Value};
    let wf = flow_workflow("flow-bare.nika");
    let events = vec![
        demo::bare_event(nika_event::EventKind::WorkflowStarted, 0)
            .with_field(KeyValue::new("workflow", Value::String("other-run".into()))),
        demo::bare_event(nika_event::EventKind::TaskCompleted, 10)
            .with_field(KeyValue::new("task", Value::String("read_payload".into()))),
    ];
    let tr = stage("flow-bare.ndjson", &events);
    let out = flow(&tr.to_string_lossy(), &wf.to_string_lossy(), plain());
    assert_eq!(out.code, exit::OK);
    assert!(
        out.text.contains("read_payload → audit"),
        "structure without sizes: {}",
        out.text
    );
    assert!(
        out.text
            .contains("note: the trace records workflow `other-run`"),
        "mismatch surfaces: {}",
        out.text
    );
    assert!(
        !out.text.contains("widest"),
        "no sized edge → no widest claim: {}",
        out.text
    );
}

/// Errors teach: an unknown task lists what the trace records; a
/// task without an output names its state + the rows that have one.
#[test]
fn peek_errors_are_readable_and_actionable() {
    let path = peek_fixture("peek-errors.ndjson");
    let trace = path.to_string_lossy();
    let unknown = peek(&trace, "ghost", false, plain());
    assert_eq!(unknown.code, exit::ENV);
    assert!(
        unknown.text.contains("unknown task `ghost`") && unknown.text.contains("audit · deploy"),
        "{}",
        unknown.text
    );
    let skipped = peek(&trace, "deploy", false, plain());
    assert_eq!(skipped.code, exit::ENV);
    assert!(
        skipped.text.contains("recorded no output (skipped)")
            && skipped.text.contains("outputs recorded for: audit"),
        "{}",
        skipped.text
    );
}

/// Wave 3 · persona 10: a fan-out over a runtime collection earns no
/// resume stamp, so its aggregate value is never checkpointed — but its
/// item table (index · item · status · code · message) IS on the
/// terminal frame. `peek` must deliver the table instead of refusing
/// with « recorded no output (ok) »; `--raw` still has no value to pipe.
#[test]
fn peek_delivers_the_item_table_when_the_value_was_not_checkpointed() {
    use nika_event::EventKind;
    use nika_types::resource::{KeyValue, Value};
    let items = r#"[{"index":0,"item":"./items/a.md","status":"ok"},{"index":1,"item":"./items/b.md","status":"ok"},{"index":2,"item":"./items/c.md","status":"recovered","code":"NIKA-BUILTIN-READ-001","message":"file not found: ./items/c.md"}]"#;
    let events = vec![
        demo::bare_event(EventKind::TaskStarted, 0)
            .with_field(KeyValue::new("task", Value::String("read".into())))
            .with_field(KeyValue::new(
                "note",
                Value::String("for_each · nika:read".into()),
            )),
        demo::bare_event(EventKind::TaskRecovered, 20)
            .with_field(KeyValue::new("task", Value::String("read".into())))
            .with_field(KeyValue::new(
                "code",
                Value::String("NIKA-BUILTIN-READ-001".into()),
            )),
        demo::bare_event(EventKind::TaskCompleted, 40)
            .with_field(KeyValue::new("task", Value::String("read".into())))
            .with_field(KeyValue::new(
                "note",
                Value::String("for_each · 2/3 ok · 1 recovered: ./items/c.md".into()),
            ))
            .with_field(KeyValue::new("duration_ms", Value::Int(4)))
            .with_field(KeyValue::new("items", Value::String(items.into()))),
        demo::bare_event(EventKind::WorkflowCompleted, 50),
    ];
    let path = stage("unstamped-fan.ndjson", &events);
    let trace = path.to_string_lossy();

    let out = peek(&trace, "read", false, plain());
    assert_eq!(out.code, exit::OK, "{}", out.text);
    for needle in [
        "items · 3",
        "NIKA-BUILTIN-READ-001",
        "file not found: ./items/c.md",
        "recovered from NIKA-BUILTIN-READ-001",
        "value not checkpointed",
    ] {
        assert!(
            out.text.contains(needle),
            "peek carries `{needle}`:\n{}",
            out.text
        );
    }

    let raw = peek(&trace, "read", true, plain());
    assert_eq!(raw.code, exit::ENV, "no value, no pipe: {}", raw.text);
    assert!(
        raw.text.contains("not checkpointed") && raw.text.contains("no resume stamp"),
        "the refusal says why the value is absent: {}",
        raw.text
    );
}

/// B23 / issue 1275: peek + outputs + json never render a recovered
/// task as a clean success.
#[test]
fn recovered_task_is_not_a_clean_success_on_peek_outputs_or_json() {
    use nika_event::EventKind;
    use nika_types::resource::{KeyValue, Value};
    let events = vec![
        demo::bare_event(EventKind::TaskStarted, 0)
            .with_field(KeyValue::new("task", Value::String("each".into())))
            .with_field(KeyValue::new("note", Value::String("exec · do".into()))),
        demo::bare_event(EventKind::TaskRecovered, 20)
            .with_field(KeyValue::new("task", Value::String("each".into())))
            .with_field(KeyValue::new("code", Value::String("NIKA-EXEC-001".into()))),
        demo::bare_event(EventKind::TaskCompleted, 40)
            .with_field(KeyValue::new("task", Value::String("each".into())))
            .with_field(KeyValue::new(
                "output",
                Value::String("\"FALLBACK-DATA\"".into()),
            )),
        demo::bare_event(EventKind::WorkflowCompleted, 50),
    ];
    let path = stage("recovered-fan.ndjson", &events);
    let trace = path.to_string_lossy();

    let peek_out = peek(&trace, "each", false, plain());
    assert_eq!(peek_out.code, exit::OK);
    assert!(
        peek_out.text.contains("recovered") && peek_out.text.contains("NIKA-EXEC-001"),
        "peek names recovered_from: {}",
        peek_out.text
    );

    let table = outputs(&trace, plain());
    assert!(
        table.text.contains("recovered"),
        "outputs marks recovered: {}",
        table.text
    );

    let (view, evs) = load_view_and_events(&trace).expect("loads");
    let json = tasks_json(&view, &evs);
    // ADR-128 · `recovered` is a fact on the task row (and a tally on the
    // settlement), never a run STATE: the run succeeded.
    assert_eq!(json["state"], "succeeded");
    assert_eq!(json["tasks"][0]["status"], "recovered");
    assert_eq!(json["tasks"][0]["recovered_from"], "NIKA-EXEC-001");
    assert_eq!(json["tasks"][0]["error_code"], "NIKA-EXEC-001");
}

/// #1276 · #1397 · a fan-out's item table reaches every reader: the
/// autopsy prints one line per item with the recorded code, the machine
/// projection carries the rows, `show`'s companion tallies them.
#[test]
fn a_fan_out_autopsy_prints_the_item_table() {
    use nika_event::EventKind;
    use nika_types::resource::{KeyValue, Value};
    let items = r#"[{"index":0,"item":"alpha","status":"ok"},{"index":1,"item":"beta","status":"failed","code":"NIKA-EXEC-001","message":"for_each item [1] beta: exit 1"},{"index":2,"item":"gamma","status":"never_started"}]"#;
    let events = vec![
        demo::bare_event(EventKind::TaskStarted, 0)
            .with_field(KeyValue::new("task", Value::String("fan".into())))
            .with_field(KeyValue::new("note", Value::String("exec · false".into()))),
        demo::bare_event(EventKind::TaskFailed, 12)
            .with_field(KeyValue::new("task", Value::String("fan".into())))
            .with_field(KeyValue::new("duration_ms", Value::Int(9)))
            .with_field(KeyValue::new(
                "detail",
                Value::String("NIKA-EXEC-001 · for_each item [1] beta: exit 1".into()),
            ))
            .with_field(KeyValue::new("items", Value::String(items.into()))),
    ];
    let path = stage("peek-fan-items.ndjson", &events);
    let out = peek(&path.to_string_lossy(), "fan", false, plain());
    assert_eq!(out.code, exit::OK);
    assert!(
        out.text.contains("items · 3"),
        "the table header: {}",
        out.text
    );
    assert!(out.text.contains("alpha"), "{}", out.text);
    assert!(
        out.text.contains("beta") && out.text.contains("NIKA-EXEC-001"),
        "the failed item with its code: {}",
        out.text
    );
    assert!(
        out.text.contains("gamma") && out.text.contains("never started"),
        "the never-started item: {}",
        out.text
    );
    let (view, events) = load_view_and_events(&path.to_string_lossy()).expect("loads");
    let json = tasks_json(&view, &events);
    assert_eq!(json["tasks"][0]["items"][2]["status"], "never_started");
    assert_eq!(json["tasks"][0]["items"][1]["code"], "NIKA-EXEC-001");
    let summary = item_summary_lines(&view, &path.to_string_lossy(), plain());
    assert_eq!(summary.len(), 1, "{summary:?}");
    assert!(
        summary[0].contains("1 ok · 1 failed · 1 never started"),
        "the tally: {}",
        summary[0]
    );
}

/// #1444 · a task fed by a recovered fallback says so on `outputs`, on
/// `peek` and in the JSON · the 3 am reader no longer mistakes it for a
/// clean success.
#[test]
fn a_task_fed_by_a_recovered_fallback_names_its_source_on_every_surface() {
    use nika_event::EventKind;
    use nika_types::resource::{KeyValue, Value};
    let task = |name: &str| KeyValue::new("task", Value::String(name.into()));
    let events = vec![
        demo::bare_event(EventKind::TaskStarted, 0)
            .with_field(task("b"))
            .with_field(KeyValue::new("note", Value::String("exec · false".into()))),
        demo::bare_event(EventKind::TaskRecovered, 1)
            .with_field(task("b"))
            .with_field(KeyValue::new("code", Value::String("NIKA-EXEC-001".into()))),
        demo::bare_event(EventKind::TaskCompleted, 2)
            .with_field(task("b"))
            .with_field(KeyValue::new(
                "output",
                Value::String("\"FALLBACK-DATA\"".into()),
            )),
        demo::bare_event(EventKind::TaskStarted, 3)
            .with_field(task("c"))
            .with_field(KeyValue::new(
                "note",
                Value::String("invoke · nika:jq".into()),
            )),
        demo::bare_event(EventKind::TaskCompleted, 4)
            .with_field(task("c"))
            .with_field(KeyValue::new(
                "output",
                Value::String("\"FALLBACK-DATA\"".into()),
            ))
            .with_field(KeyValue::new(
                "integrity",
                Value::String("untrusted".into()),
            ))
            .with_field(KeyValue::new("integrity_source", Value::String("b".into()))),
        demo::bare_event(EventKind::WorkflowCompleted, 5),
    ];
    let path = stage("lineage.ndjson", &events);
    let table = outputs(&path.to_string_lossy(), plain());
    assert_eq!(table.code, exit::OK);
    let c_row = table
        .text
        .lines()
        .find(|l| l.trim_start().starts_with("c "))
        .expect("c's row");
    assert!(
        c_row.contains("untrusted input from b"),
        "outputs names the lineage: {c_row}"
    );
    let peeked = peek(&path.to_string_lossy(), "c", false, plain());
    assert!(
        peeked.text.contains("untrusted input from b"),
        "peek names the lineage: {}",
        peeked.text
    );
    let (view, events) = load_view_and_events(&path.to_string_lossy()).expect("loads");
    let json = tasks_json(&view, &events);
    assert_eq!(json["tasks"][1]["integrity_source"], "b");
    assert!(
        json["tasks"][0]["integrity_source"].is_null(),
        "b itself has no source"
    );
}

/// The OBS-E `warning` a terminal frame carried (a `nika:glob` naming
/// the directories it left out · V9 wave 3 p10) reaches the machine
/// projection per task, and a clean task projects none — `trace
/// outputs --json` must say what `trace show` says.
#[test]
fn a_task_warning_is_projected_per_task() {
    use nika_event::EventKind;
    use nika_types::resource::{KeyValue, Value};
    let task = |id: &str| KeyValue::new("task", Value::String(id.into()));
    let said = "nika:glob returns files only · 1 directory also matched `./items/*.md` and was left out: ./items/item-07.md";
    let events = vec![
        demo::bare_event(EventKind::WorkflowStarted, 0),
        demo::bare_event(EventKind::TaskStarted, 1)
            .with_field(task("discover"))
            .with_field(KeyValue::new(
                "note",
                Value::String("invoke · nika:glob".into()),
            )),
        demo::bare_event(EventKind::TaskCompleted, 2)
            .with_field(task("discover"))
            .with_field(KeyValue::new("output", Value::String("[]".into())))
            .with_field(KeyValue::new("warning", Value::String(said.into()))),
        demo::bare_event(EventKind::TaskStarted, 3)
            .with_field(task("merge"))
            .with_field(KeyValue::new(
                "note",
                Value::String("infer · mock/echo".into()),
            )),
        demo::bare_event(EventKind::TaskCompleted, 4)
            .with_field(task("merge"))
            .with_field(KeyValue::new("output", Value::String("\"ok\"".into()))),
        demo::bare_event(EventKind::WorkflowCompleted, 5),
    ];
    let path = stage("glob-warning.ndjson", &events);
    let (view, events) = load_view_and_events(&path.to_string_lossy()).expect("loads");
    let json = tasks_json(&view, &events);
    assert_eq!(json["tasks"][0]["id"], "discover");
    assert_eq!(json["tasks"][0]["warning"], said);
    assert!(
        json["tasks"][1]["warning"].is_null(),
        "a clean task projects no warning"
    );
}
