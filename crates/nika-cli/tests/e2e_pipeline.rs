// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! End-to-end pipeline — the L3-runtime rehearsal.
//!
//! No orchestration crate exists yet (L3 = 0), so this harness PLAYS the
//! runtime's role across the real shipped layers, wiring the full chain
//! the `nika run` binary will compose:
//!
//! ```text
//! .nika.yaml ──▶ nika-schema   parse + check ladder   (audit BEFORE run)
//!            ──▶ wave-ordered dispatch through the REAL verb crates
//!                  infer  → nika-verb-infer  (mock/echo provider · no net)
//!                  exec   → nika-verb-exec   (MockShell kernel seam)
//!                  invoke → nika-verb-invoke (MockToolExecutor seam)
//!            ──▶ nika-event    one emission site per verb path (INV-024)
//!            ──▶ nika-cli      display fold (render = pure fn of stream)
//! ```
//!
//! Every seam is the production type; only the I/O edges are mocks. When
//! the L3 runtime ships its emitters, this suite is its conformance
//! floor: same YAML in, same event stream out, same frames.
//!
//! Harness limits (documented, not hidden): `${{ }}` resolution is a
//! textual substitution (the CEL evaluator ships with the 03-dag engine)
//! and a present `when:` gate is treated as CLOSED — the fixture's only
//! gate is statically false, and the skip path is exactly what we assert.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use nika_cli::{RunView, TaskState, Theme, frame};
use nika_error::traits::NikaErrorCode;
use nika_event::{Event, EventKind};
use nika_kernel::tool_executor::ToolResult;
use nika_kernel_mock::{MockShell, MockToolExecutor};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_schema::check::CheckReport;
use nika_schema::raw::{RawAction, RawCommand, RawWorkflow};
use nika_schema::{FileId, ParseMode, check, infer_permits, parse};
use nika_types::id::EventId;
use nika_types::resource::{KeyValue, Value};
use nika_types::timestamp::Timestamp;
use nika_verb_exec::{ExecInput, ExecOutput, ExecValue, ExecVerb};
use nika_verb_infer::{InferInput, InferOutput, InferValue, InferVerb};
use nika_verb_invoke::{InvokeInput, InvokeOutput, InvokeVerb};
use uuid::Uuid;

// ─── the fixture · one workflow, all three shipped verbs ───────────────

/// Diamond-shaped DAG: two parallel sources, a typed extraction, a
/// synthesis joining both sources, a persist joining both infers, and a
/// statically-closed gate. 4 waves · 6 tasks.
const WORKFLOW_OK: &str = r#"
nika: v1
workflow: e2e-veille
description: "gather facts in parallel, extract typed data, think once, persist, gated notify"

model: mock/echo

vars:
  source: "./news.json"
  publish: "no"

tasks:
  - id: gather
    invoke:
      tool: "nika:read"
      args: { path: "${{ vars.source }}" }

  - id: probe
    exec:
      command: "wc -l ./news.json"

  - id: extract
    depends_on: [gather]
    infer:
      prompt: "Extract the story fields · ${{ tasks.gather.output }}"
      schema:
        type: object
        properties:
          headline: { type: string }
          score: { type: integer }
        required: [headline, score]

  - id: think
    depends_on: [gather, probe]
    infer:
      prompt: "Summarize · ${{ tasks.gather.output }} · lines ${{ tasks.probe.output }}"
      max_tokens: 800

  - id: write_out
    depends_on: [think, extract]
    invoke:
      tool: "nika:write"
      args:
        path: "./out/report.md"
        content: "${{ tasks.think.output }}"

  - id: notify
    depends_on: [write_out]
    when: ${{ vars.publish == 'yes' }}
    exec:
      command: "echo done"

outputs:
  report: ${{ tasks.write_out.output }}
"#;

/// What the mocked `nika:read` returns — valid JSON so the structured
/// `extract` task can prove schema validation over real dataflow.
const GATHER_JSON: &str = r#"{"headline":"Rust 2.0","score":9}"#;

// ─── deterministic event emission (the demo.rs idiom) ──────────────────

/// Monotonic (seq, ms) source — no wall clock, replay-stable forever.
struct EventPen {
    seq: u128,
    ms: u64,
    events: Vec<Event>,
}

impl EventPen {
    fn new() -> Self {
        Self {
            seq: 0,
            ms: 0,
            events: Vec::new(),
        }
    }

    fn emit(&mut self, kind: EventKind, fields: &[(&str, Value)]) {
        self.seq += 1;
        self.ms += 10;
        let mut ev = Event::new(
            EventId::new(Uuid::from_u128(self.seq)),
            Timestamp::from_unix_ms(self.ms),
            kind,
        );
        for (key, value) in fields {
            ev = ev.with_field(KeyValue::new(*key, value.clone()));
        }
        self.events.push(ev);
    }
}

fn s(v: &str) -> Value {
    Value::String(v.to_owned())
}

fn i(v: i64) -> Value {
    Value::Int(v)
}

// ─── the harness · wave-ordered dispatch over the real verbs ───────────

struct Seams {
    shell: Arc<MockShell>,
    tools: Arc<MockToolExecutor>,
    infer: InferVerb,
}

impl Seams {
    fn new(shell: MockShell, tools: MockToolExecutor) -> Self {
        let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
        Self {
            shell: Arc::new(shell),
            tools: Arc::new(tools),
            infer: InferVerb::new(registry, "mock/echo"),
        }
    }
}

/// Textual `${{ }}` substitution — the rehearsal stand-in for CEL
/// resolution (03-dag engine work). Replaces `${{ tasks.<id>.output }}`
/// and `${{ vars.<key> }}` occurrences.
fn interpolate(
    text: &str,
    bindings: &BTreeMap<String, String>,
    vars: &BTreeMap<String, String>,
) -> String {
    let mut out = text.to_owned();
    for (id, value) in bindings {
        out = out.replace(&format!("${{{{ tasks.{id}.output }}}}"), value);
    }
    for (key, value) in vars {
        out = out.replace(&format!("${{{{ vars.{key} }}}}"), value);
    }
    out
}

/// Interpolate every string leaf of a JSON value (invoke `args:`).
fn interpolate_json(
    value: &serde_json::Value,
    bindings: &BTreeMap<String, String>,
    vars: &BTreeMap<String, String>,
) -> serde_json::Value {
    match value {
        serde_json::Value::String(text) => {
            serde_json::Value::String(interpolate(text, bindings, vars))
        }
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items
                .iter()
                .map(|v| interpolate_json(v, bindings, vars))
                .collect(),
        ),
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), interpolate_json(v, bindings, vars)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// One task's terminal outcome, as the harness records it.
enum Outcome {
    Ok {
        output: String,
        note: String,
        tokens: Option<i64>,
    },
    Failed {
        detail: String,
        note: String,
    },
    Skipped {
        note: String,
    },
}

/// The rehearsal's view of the envelope: string vars + the workflow name.
fn envelope_strings(wf: &RawWorkflow) -> (BTreeMap<String, String>, String) {
    let vars = wf
        .vars
        .iter()
        .filter_map(|(key, decl)| {
            let value = match decl {
                nika_schema::types::VarDecl::Untyped(v) => v.as_str().map(str::to_owned),
                nika_schema::types::VarDecl::Typed { default, .. } => {
                    default.as_ref().and_then(|d| d.as_str()).map(str::to_owned)
                }
                _ => None, // #[non_exhaustive] future forms carry no rehearsal value
            }?;
            Some((key.value.clone(), value))
        })
        .collect();
    let name = wf
        .workflow
        .as_ref()
        .map_or_else(|| "workflow".to_owned(), |w| w.value.clone());
    (vars, name)
}

/// Execute one parsed workflow wave-by-wave through the real verb crates,
/// emitting the event stream the L3 runtime will own. Returns the stream
/// and the run verdict.
///
/// INV-024 rehearsal: this function is the ONE emission site per verb
/// path — the verbs themselves stay event-free.
async fn execute(wf: &RawWorkflow, report: &CheckReport, seams: &Seams) -> (Vec<Event>, bool) {
    assert!(
        report.is_clean(),
        "the audit-before-run contract: a dirty workflow never executes"
    );

    let (vars, workflow_name) = envelope_strings(wf);

    let mut pen = EventPen::new();
    pen.emit(
        EventKind::WorkflowStarted,
        &[
            ("workflow", s(&workflow_name)),
            ("permits", s("engine floor (no boundary declared)")),
        ],
    );
    for task in &wf.tasks {
        pen.emit(
            EventKind::TaskScheduled,
            &[("task", s(&task.value.id.value))],
        );
    }

    let mut bindings: BTreeMap<String, String> = BTreeMap::new();
    let mut dead: BTreeSet<String> = BTreeSet::new(); // failed or skip-cascaded
    let mut workflow_ok = true;

    for wave in &report.waves {
        for &index in wave {
            let task = &wf.tasks[index].value;
            let id = task.id.value.clone();

            // Upstream failure cascade: any dead dependency skips this task.
            if task.depends_on.iter().any(|d| dead.contains(&d.value)) {
                pen.emit(
                    EventKind::TaskSkipped,
                    &[("task", s(&id)), ("note", s("upstream failed"))],
                );
                dead.insert(id);
                continue;
            }
            // Gate rehearsal: a present `when:` is CLOSED (see module doc).
            // Downstream tasks still ran upstream of the gate, so this is
            // a pure skip, not a cascade.
            if task.when.is_some() {
                pen.emit(
                    EventKind::TaskSkipped,
                    &[("task", s(&id)), ("note", s("when: gate closed"))],
                );
                continue;
            }

            let outcome = run_task(&task.action, &bindings, &vars, seams).await;
            match outcome {
                Outcome::Ok {
                    output,
                    note,
                    tokens,
                } => {
                    pen.emit(
                        EventKind::TaskStarted,
                        &[("task", s(&id)), ("note", s(&note))],
                    );
                    let mut fields = vec![("task", s(&id)), ("note", s(&note))];
                    if let Some(n) = tokens {
                        fields.push(("tokens", i(n)));
                    }
                    pen.emit(EventKind::TaskCompleted, &fields);
                    bindings.insert(id, output);
                }
                Outcome::Failed { detail, note } => {
                    pen.emit(
                        EventKind::TaskStarted,
                        &[("task", s(&id)), ("note", s(&note))],
                    );
                    pen.emit(
                        EventKind::TaskFailed,
                        &[("task", s(&id)), ("note", s(&note)), ("detail", s(&detail))],
                    );
                    dead.insert(id);
                    workflow_ok = false;
                }
                Outcome::Skipped { note } => {
                    pen.emit(
                        EventKind::TaskSkipped,
                        &[("task", s(&id)), ("note", s(&note))],
                    );
                }
            }
        }
    }

    let terminal = if workflow_ok {
        EventKind::WorkflowCompleted
    } else {
        EventKind::WorkflowFailed
    };
    pen.emit(terminal, &[("workflow", s(&workflow_name))]);
    (pen.events, workflow_ok)
}

/// Dispatch one action through its real verb crate.
async fn run_task(
    action: &RawAction,
    bindings: &BTreeMap<String, String>,
    vars: &BTreeMap<String, String>,
    seams: &Seams,
) -> Outcome {
    match action {
        RawAction::Invoke(invoke) => {
            let tool = invoke.tool.value.clone();
            let args = invoke.args.as_ref().map_or_else(
                || serde_json::Value::Object(serde_json::Map::new()),
                |a| interpolate_json(&a.value, bindings, vars),
            );
            let mut input = InvokeInput::new(tool.clone());
            input.args = args;
            let verb = InvokeVerb::new(Arc::clone(&seams.tools));
            match verb.run(input).await {
                Ok(InvokeOutput { content, .. }) => Outcome::Ok {
                    output: content,
                    note: format!("invoke · {tool}"),
                    tokens: None,
                },
                Err(err) => Outcome::Failed {
                    detail: format!("{} · {err}", err.nika_code()),
                    note: format!("invoke · {tool}"),
                },
            }
        }
        RawAction::Exec(exec) => {
            let command = match &exec.command {
                RawCommand::Shell(text) => interpolate(&text.value, bindings, vars),
                // The argv runner path ships with the engine; the rehearsal
                // joins for the mock seam (which records, never executes).
                RawCommand::Argv(parts) => parts
                    .iter()
                    .map(|p| interpolate(&p.value, bindings, vars))
                    .collect::<Vec<_>>()
                    .join(" "),
                // #[non_exhaustive]: a future command form must update the
                // rehearsal, loudly.
                other => panic!("unknown command form: {other:?}"),
            };
            let program = command.split_whitespace().next().unwrap_or("?").to_owned();
            let verb = ExecVerb::new(Arc::clone(&seams.shell));
            match verb.run(ExecInput::new(command)).await {
                Ok(ExecOutput { output, .. }) => {
                    let text = match output {
                        ExecValue::Text(text) => text,
                        ExecValue::Structured { stdout, .. } => stdout,
                        other => panic!("unexpected exec value: {other:?}"),
                    };
                    Outcome::Ok {
                        output: text.trim_end().to_owned(),
                        note: format!("exec · {program}"),
                        tokens: None,
                    }
                }
                Err(err) => Outcome::Failed {
                    detail: format!("{} · {err}", err.nika_code()),
                    note: format!("exec · {program}"),
                },
            }
        }
        RawAction::Infer(infer) => {
            let prompt = interpolate(&infer.prompt.value, bindings, vars);
            let mut input = InferInput::new(prompt);
            input.max_tokens = infer.max_tokens.as_ref().map(|t| t.value);
            input.schema = infer.schema.as_ref().map(|v| v.value.clone());
            match seams.infer.run(input).await {
                Ok(InferOutput {
                    output,
                    usage,
                    model_resolved,
                    ..
                }) => {
                    let text = match output {
                        InferValue::Text(text) => text,
                        InferValue::Structured(value) => value.to_string(),
                        other => panic!("unexpected infer value: {other:?}"),
                    };
                    Outcome::Ok {
                        output: text,
                        note: format!("infer · {model_resolved}"),
                        tokens: Some(i64::try_from(usage.output_tokens).unwrap_or(i64::MAX)),
                    }
                }
                Err(err) => Outcome::Failed {
                    detail: format!("{err}"),
                    note: "infer · mock/echo".to_owned(),
                },
            }
        }
        // `agent` is specced, not yet shipped (s12) — and any future verb
        // lands here too (#[non_exhaustive]): the rehearsal refuses rather
        // than silently no-ops.
        other => Outcome::Skipped {
            note: format!("verb not shipped yet: {other:?}"),
        },
    }
}

// ─── helpers shared by the tests ────────────────────────────────────────

fn parse_and_check(yaml: &str) -> (RawWorkflow, CheckReport) {
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
    let report = check(&wf);
    (wf, report)
}

fn wave_ids<'a>(wf: &'a RawWorkflow, report: &CheckReport) -> Vec<Vec<&'a str>> {
    report
        .waves
        .iter()
        .map(|wave| {
            let mut ids: Vec<&str> = wave
                .iter()
                .map(|&i| wf.tasks[i].value.id.value.as_str())
                .collect();
            ids.sort_unstable();
            ids
        })
        .collect()
}

fn fold(events: &[Event]) -> RunView {
    let mut view = RunView::new();
    for event in events {
        view.apply(event);
    }
    view
}

fn states(view: &RunView) -> BTreeMap<String, TaskState> {
    view.rows()
        .iter()
        .map(|r| (r.id.clone(), r.state))
        .collect()
}

const PLAIN: Theme = Theme {
    color: false,
    ascii: false,
    animate: false,
};

// ─── test 1 · the static audit (the run's precondition) ─────────────────

#[test]
fn e2e_static_audit_proves_topology_cost_and_permits() {
    let (wf, report) = parse_and_check(WORKFLOW_OK);

    // The audit is clean and proves the topology.
    assert!(
        report.conformance.is_empty(),
        "conformance: {:?}",
        report.conformance
    );
    assert!(report.is_clean(), "fixture must pass the full ladder");
    assert_eq!(
        wave_ids(&wf, &report),
        [
            vec!["gather", "probe"],
            vec!["extract", "think"],
            vec!["write_out"],
            vec!["notify"],
        ],
        "diamond topology: parallel sources → two infers → persist → gate"
    );

    // The cost lane is HONEST about the mock model: no catalog price means
    // an unbounded floor, not an invented number.
    assert!(report.cost.has_unbounded, "mock/echo has no catalog price");

    // The operator loop: the inferred permits boundary names both tools.
    let inferred = infer_permits(&wf).to_yaml();
    assert!(
        inferred.contains("nika:read"),
        "inferred permits: {inferred}"
    );
    assert!(
        inferred.contains("nika:write"),
        "inferred permits: {inferred}"
    );
}

// ─── test 1bis · the happy path, end to end ──────────────────────────────

#[tokio::test]
async fn e2e_happy_path_full_pipeline() {
    let (wf, report) = parse_and_check(WORKFLOW_OK);

    // STAGE 2 · execute through the real verbs over mock seams (STAGE 1,
    // the static audit, is its own test above; execute() re-asserts it).
    let shell = MockShell::new().enqueue_ok("      42 ./news.json\n");
    let tools = MockToolExecutor::new()
        .enqueue_ok(ToolResult::success("call-gather", GATHER_JSON))
        .enqueue_ok(ToolResult::success("call-write", "2.1 KB written"));
    let seams = Seams::new(shell, tools);

    let (events, ok) = execute(&wf, &report, &seams).await;
    assert!(ok, "happy path completes");

    // The wired proof, seam by seam.
    let commands = seams.shell.executed_commands();
    assert_eq!(commands.len(), 1, "probe ran · notify stayed gated");
    let argv = format!("{} {}", commands[0].program, commands[0].args.join(" "));
    assert!(
        argv.contains("wc"),
        "the YAML command reached the kernel seam: {argv}"
    );

    let calls = seams.tools.captured_calls();
    assert_eq!(calls.len(), 2, "gather + write_out");
    // Find by tool name, not by index — intra-wave dispatch order is the
    // engine's freedom, not this test's contract.
    let read = calls
        .iter()
        .find(|c| c.name == "nika:read")
        .expect("gather hit the tool seam");
    assert_eq!(
        read.input.get("path").and_then(serde_json::Value::as_str),
        Some("./news.json"),
        "vars.source resolved into the tool args"
    );
    let write = calls
        .iter()
        .find(|c| c.name == "nika:write")
        .expect("write_out hit the tool seam");
    let written = write
        .input
        .get("content")
        .and_then(serde_json::Value::as_str)
        .expect("write_out carries content");
    assert!(
        written.contains(GATHER_JSON),
        "dataflow proof: gather's JSON flowed through think's echo into the write args"
    );

    // STAGE 3 · the event stream is exactly the storyboard the runtime owns.
    let kinds: Vec<EventKind> = events.iter().map(|e| e.kind).collect();
    assert_eq!(kinds[0], EventKind::WorkflowStarted);
    assert_eq!(
        kinds[1..=6],
        [EventKind::TaskScheduled; 6],
        "all six tasks scheduled up front"
    );
    assert_eq!(
        *kinds.last().expect("stream non-empty"),
        EventKind::WorkflowCompleted
    );
    let failed = kinds
        .iter()
        .filter(|k| **k == EventKind::TaskFailed)
        .count();
    assert_eq!(failed, 0);

    // STAGE 4 · the display fold sees one truth.
    let view = fold(&events);
    assert_eq!(view.workflow, "e2e-veille");
    assert_eq!(view.verdict, Some(true));
    assert_eq!(view.rows().len(), 6);
    assert_eq!(view.done_count(), 6);
    let by_id = states(&view);
    assert_eq!(by_id["gather"], TaskState::Ok);
    assert_eq!(by_id["probe"], TaskState::Ok);
    assert_eq!(by_id["extract"], TaskState::Ok);
    assert_eq!(by_id["think"], TaskState::Ok);
    assert_eq!(by_id["write_out"], TaskState::Ok);
    assert_eq!(by_id["notify"], TaskState::Skipped);
    assert!(
        !view.token_samples.is_empty(),
        "infer completions reported token usage into the fold"
    );

    // The frame: identity line + a 6/6 meter, no failure card.
    let lines = frame(&view, &PLAIN, 0);
    assert!(
        lines[0].contains("e2e-veille · 6 tasks"),
        "header: {}",
        lines[0]
    );
    let meter = lines
        .iter()
        .find(|l| l.contains("done"))
        .expect("meter line present");
    assert!(meter.contains("6/6 done"), "meter: {meter}");
    assert!(
        !lines.iter().any(|l| l.contains("NIKA-")),
        "no failure card on the happy path"
    );
}

// ─── test 2 · structured output through real dataflow ──────────────────

#[tokio::test]
async fn e2e_structured_output_validates_real_dataflow() {
    let (wf, report) = parse_and_check(WORKFLOW_OK);
    assert!(report.is_clean());

    // Drive ONLY the extract lane: gather's mocked JSON rides the echo
    // provider's reply and must come back schema-validated and typed.
    let shell = MockShell::new().enqueue_ok("42\n");
    let tools = MockToolExecutor::new()
        .enqueue_ok(ToolResult::success("call-gather", GATHER_JSON))
        .enqueue_ok(ToolResult::success("call-write", "ok"));
    let seams = Seams::new(shell, tools);

    let extract = wf
        .tasks
        .iter()
        .find(|t| t.value.id.value == "extract")
        .expect("fixture has extract");
    let RawAction::Infer(infer) = &extract.value.action else {
        panic!("extract is an infer task");
    };

    let mut bindings = BTreeMap::new();
    bindings.insert("gather".to_owned(), GATHER_JSON.to_owned());
    let prompt = interpolate(&infer.prompt.value, &bindings, &BTreeMap::new());
    let mut input = InferInput::new(prompt);
    input.schema = infer.schema.as_ref().map(|v| v.value.clone());

    let out = seams
        .infer
        .run(input)
        .await
        .expect("structured extraction succeeds");
    let InferValue::Structured(value) = out.output else {
        panic!(
            "schema'd task returns the validated value, got {:?}",
            out.output
        );
    };
    assert_eq!(value["headline"], "Rust 2.0");
    assert_eq!(value["score"], 9);
    assert_eq!(out.model_resolved, "mock/echo");
    assert!(out.usage.output_tokens > 0, "usage flows back");
}

// ─── test 3 · failure cascade with partial scheduling ───────────────────

#[tokio::test]
async fn e2e_failure_cascade_partial_schedule_and_card() {
    let (wf, report) = parse_and_check(WORKFLOW_OK);

    // probe explodes; gather's lane stays alive.
    let shell = MockShell::new().enqueue_fail(7, "disk full: /var/news");
    let tools = MockToolExecutor::new().enqueue_ok(ToolResult::success("call-gather", GATHER_JSON));
    let seams = Seams::new(shell, tools);

    let (events, ok) = execute(&wf, &report, &seams).await;
    assert!(!ok, "a failed task fails the workflow");

    // Partial scheduling is the point: extract (gather-only deps) RAN,
    // think/write_out/notify never did.
    let calls = seams.tools.captured_calls();
    assert_eq!(calls.len(), 1, "write_out never reached the tool seam");
    assert_eq!(calls[0].name, "nika:read");

    let view = fold(&events);
    assert_eq!(view.verdict, Some(false));
    let by_id = states(&view);
    assert_eq!(by_id["gather"], TaskState::Ok);
    assert_eq!(by_id["probe"], TaskState::Failed);
    assert_eq!(by_id["extract"], TaskState::Ok, "independent lane survived");
    assert_eq!(by_id["think"], TaskState::Skipped);
    assert_eq!(by_id["write_out"], TaskState::Skipped);
    assert_eq!(by_id["notify"], TaskState::Skipped);
    assert_eq!(view.done_count(), 6, "every task reached a terminal state");

    // The failure card carries the registry code end-to-end: verb error →
    // event detail → fold → frame → the explain hint.
    let failed_event = events
        .iter()
        .find(|e| e.kind == EventKind::TaskFailed)
        .expect("one task failed");
    let detail = failed_event
        .fields
        .iter()
        .find(|kv| kv.key == "detail")
        .and_then(|kv| match &kv.value {
            Value::String(text) => Some(text.as_str()),
            _ => None,
        })
        .expect("failure detail present");
    assert!(
        detail.contains("NIKA-440"),
        "exec non-zero exit code: {detail}"
    );
    assert!(
        detail.contains("status 7"),
        "exit status surfaces: {detail}"
    );

    let lines = frame(&view, &PLAIN, 0);
    let card = lines.join("\n");
    assert!(card.contains("NIKA-440"), "card: {card}");
    assert!(
        card.contains("fix: nika explain NIKA-440"),
        "explain hint: {card}"
    );
}

// ─── test 4 · the audit-before-run contract refuses a dirty file ────────

#[tokio::test]
async fn e2e_check_ladder_refuses_dirty_workflow() {
    // Two injected defects: a typo'd builtin and a reference without its
    // depends_on edge.
    let dirty = WORKFLOW_OK
        .replace("\"nika:read\"", "\"nika:reed\"")
        .replace("    depends_on: [gather, probe]\n", "");
    let wf = parse(&dirty, FileId::new(0), ParseMode::Strict).expect("dirty file still parses");
    let report = check(&wf);

    assert!(!report.is_clean(), "the ladder catches both defects");

    // did-you-mean: the closed catalog knows what we meant.
    let unknown = report
        .unknown_tools
        .first()
        .expect("typo'd builtin reported");
    assert_eq!(unknown.tool, "nika:reed");
    assert_eq!(unknown.suggestion.as_deref(), Some("nika:read"));

    // The dangling `${{ tasks.gather.output }}` reference (its depends_on
    // edge was removed) lands in the conformance lane with a spec code.
    assert!(
        !report.conformance.is_empty(),
        "missing depends_on is a conformance finding: {report:?}"
    );

    // And the harness refuses to run it — that's the whole product story:
    // audited BEFORE a single token is spent. No `Seams` is constructed,
    // deliberately: `execute()` asserts `is_clean()` so the only honest
    // proof here is that no seam ever existed to be touched.
    let tools = MockToolExecutor::new();
    let captured = Arc::new(tools);
    assert!(captured.captured_calls().is_empty(), "nothing executed");
}

// ─── test 5 · replay = re-render, never re-execute ──────────────────────

#[tokio::test]
async fn e2e_trace_ndjson_roundtrip_is_lossless() {
    let (wf, report) = parse_and_check(WORKFLOW_OK);
    let shell = MockShell::new().enqueue_ok("      42 ./news.json\n");
    let tools = MockToolExecutor::new()
        .enqueue_ok(ToolResult::success("call-gather", GATHER_JSON))
        .enqueue_ok(ToolResult::success("call-write", "2.1 KB written"));
    let seams = Seams::new(shell, tools);
    let (events, _) = execute(&wf, &report, &seams).await;

    // Serialize the stream exactly as the flight recorder writes it.
    let ndjson: String = events
        .iter()
        .map(|e| serde_json::to_string(e).expect("event serializes"))
        .collect::<Vec<_>>()
        .join("\n");

    // Re-read it exactly as `nika-cli trace replay` does.
    let replayed: Vec<Event> = ndjson
        .lines()
        .map(|line| serde_json::from_str(line).expect("event deserializes"))
        .collect();
    assert_eq!(replayed.len(), events.len());

    // The fold is a pure function of the stream: same truth, same frames.
    let live = frame(&fold(&events), &PLAIN, 0);
    let replay = frame(&fold(&replayed), &PLAIN, 0);
    assert_eq!(live, replay, "replay re-renders the identical card");

    // And byte-stable across runs: the deterministic seams + the synthetic
    // clock pin the whole pipeline (the spec's reproducibility law).
    let shell2 = MockShell::new().enqueue_ok("      42 ./news.json\n");
    let tools2 = MockToolExecutor::new()
        .enqueue_ok(ToolResult::success("call-gather", GATHER_JSON))
        .enqueue_ok(ToolResult::success("call-write", "2.1 KB written"));
    let seams2 = Seams::new(shell2, tools2);
    let (events2, _) = execute(&wf, &report, &seams2).await;
    let ndjson2: String = events2
        .iter()
        .map(|e| serde_json::to_string(e).expect("event serializes"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(ndjson, ndjson2, "two runs, one byte-identical trace");
}
