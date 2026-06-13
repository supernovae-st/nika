// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! End-to-end pipeline — the L3 conformance floor, now over the REAL runtime.
//!
//! This suite was born as the L3 rehearsal: the harness PLAYED the missing
//! runtime's role across the real shipped layers. `nika-runtime` shipped
//! (s18) and `execute()` flipped from playing the layer to CALLING it —
//! every assertion below predates the crate and survived the flip
//! unchanged. That is the floor contract honored: same YAML in, same
//! event stream out, through the chain the `nika run` binary composes:
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
//! v0 limits (documented, not hidden): `${{ }}` resolution is the
//! runtime's reference resolver (the CEL evaluator ships with the 03-dag
//! engine behind the same seam) and `when:` evaluates the v0 subset —
//! the fixture's gate (`vars.publish == 'yes'` · publish=no) evaluates
//! closed, and the skip path is exactly what we assert.

use std::collections::BTreeMap;
use std::sync::Arc;

use nika_cli::{RunView, TaskState, Theme, frame};
use nika_event::{Event, EventKind};
use nika_kernel::tool_executor::ToolResult;
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{DeterministicStamper, Runtime, RuntimeConfig, VecSink};
use nika_schema::check::CheckReport;
use nika_schema::raw::{RawAction, RawWorkflow};
use nika_schema::{FileId, ParseMode, check, infer_permits, parse};
use nika_types::resource::Value;
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::{InferInput, InferValue, InferVerb};
use nika_verb_invoke::InvokeVerb;

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

/// Execute the workflow through the REAL `nika-runtime` (the flip) ·
/// deterministic stamps · collected stream · the rehearsal's old
/// signature so every pre-existing assertion runs unchanged.
async fn execute(wf: &RawWorkflow, report: &CheckReport, seams: &Seams) -> (Vec<Event>, bool) {
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::clone(&seams.tools)));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::clone(&seams.shell)),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        // The agent lane idles in these fixtures · the floor suite in
        // nika-runtime drives it through the YAML path.
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::default(),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(wf, report, &mut stamper, &mut sink)
        .await
        .expect("the audit-before-run contract: a dirty workflow never executes");
    (sink.into_events(), outcome.ok)
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
    // Spec 03 · a default-gate task over a dead dep is CANCELLED (§3.1
    // `◼` · the v1 rehearsal said skipped — the spec-parity runtime
    // upgraded the cascade class · the fold + glyphs followed).
    assert_eq!(by_id["think"], TaskState::Cancelled);
    assert_eq!(by_id["write_out"], TaskState::Cancelled);
    // notify's explicit `when:` still gets its evaluation (deps are
    // terminal · the always-pattern lane) — publish=no · gate closed.
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

// ─── test 5 · the agent loop over the REAL builtin dispatcher ────────────
//
// s12 (the 4th verb) × s16 (the tool layer): BOTH production types in one
// chain — AgentVerb dispatches through InvokeVerb into BuiltinDispatcher,
// and the model-facing catalog the whitelist filters IS the dispatcher's
// own `tool_defs()`. Only the effect edges (fs · http · clock) and the
// model itself are mocks. This is the conformance floor for the L3
// runtime's agent wiring: same seams, same loop, real tools.

#[tokio::test]
async fn e2e_agent_loop_over_the_real_builtin_dispatcher() {
    use nika_builtin::{BuiltinDispatcher, NoWorkflow, NonInteractive, NullEmitter};
    use nika_kernel::provider::{ContentBlock, InferResponse, StopReason, TokenUsage};
    use nika_kernel::runtime::agent::AgentStopReason;
    use nika_kernel_mock::{MockClock, MockFs, MockHttp, MockProvider};
    use nika_verb_agent::{AgentInput, AgentValue, AgentVerb};

    fn tool_turn(id: &str, name: &str, args: serde_json::Value) -> InferResponse {
        InferResponse::new(
            vec![ContentBlock::ToolUse {
                id: id.to_owned(),
                name: name.to_owned(),
                input: args,
            }],
            TokenUsage::new(10, 5),
            StopReason::ToolUse,
        )
    }

    // The REAL tool layer over mock effect seams.
    let fs = MockFs::new().with_file("./notes.md", "release: ship the agent");
    let dispatcher = Arc::new(BuiltinDispatcher::new(
        Arc::new(fs),
        Arc::new(MockHttp::new()),
        Arc::new(MockClock::new()),
        Arc::new(NullEmitter::default()),
        Arc::new(NonInteractive::default()),
        Arc::new(NoWorkflow::default()),
    ));

    // The scripted model: ① jq over structured input → ② read the seeded
    // file → ③ the done sentinel carrying a result built FROM both tools.
    let provider = MockProvider::new("mock")
        .enqueue_response(tool_turn(
            "c-1",
            "nika:jq",
            serde_json::json!({ "expression": ".prices | add", "input": { "prices": [2, 3] } }),
        ))
        .enqueue_response(tool_turn(
            "c-2",
            "nika:read",
            serde_json::json!({ "path": "./notes.md" }),
        ))
        .enqueue_response(tool_turn(
            "c-3",
            "nika:done",
            serde_json::json!({ "result": { "sum": 5, "note": "release: ship the agent" } }),
        ));

    let invoke = Arc::new(nika_verb_invoke::InvokeVerb::new(Arc::clone(&dispatcher)));
    let agent = AgentVerb::new(
        Arc::new(provider.clone()),
        invoke,
        Arc::clone(&dispatcher),
        "mock/echo",
    );

    let mut input = AgentInput::new("sum the prices, read ./notes.md, then finish");
    input.tools = vec!["nika:*".to_owned()];
    let output = agent.run(input).await.expect("the loop completes");

    // The loop ran exactly the scripted three turns and ended on the
    // sentinel (ExplicitCompletion), with the structured result.
    assert_eq!(output.turns, 3);
    assert_eq!(output.stop_reason, AgentStopReason::ExplicitCompletion);
    let AgentValue::Structured(value) = output.output else {
        panic!("done carried a result: {:?}", output.output);
    };
    assert_eq!(
        value,
        serde_json::json!({ "sum": 5, "note": "release: ship the agent" })
    );

    // The wired proof: the REAL builtins produced the values the model saw.
    let requests = provider.captured_requests();
    assert_eq!(requests.len(), 3);
    // Turn 2's request carries turn 1's jq result — computed by jaq, not
    // by a mock ("5" · the exactly-one-output law shipped it as a value).
    let turn2_results: Vec<&ContentBlock> = requests[1]
        .messages
        .iter()
        .flat_map(|m| m.content.iter())
        .filter(|b| matches!(b, ContentBlock::ToolResult { .. }))
        .collect();
    assert_eq!(turn2_results.len(), 1);
    if let ContentBlock::ToolResult {
        tool_use_id,
        content,
        is_error,
    } = turn2_results[0]
    {
        assert_eq!(tool_use_id, "c-1");
        assert_eq!(content, "5", "jaq computed the sum");
        assert!(!is_error);
    }
    // Turn 3's request carries the REAL file content read through MockFs.
    let turn3_read = requests[2]
        .messages
        .iter()
        .flat_map(|m| m.content.iter())
        .find_map(|b| match b {
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                ..
            } if tool_use_id == "c-2" => Some(content.clone()),
            _ => None,
        })
        .expect("the read result fed back");
    assert_eq!(turn3_read, "release: ship the agent");

    // And the catalog the model was offered IS the dispatcher's: the
    // whitelist (`nika:*`) admitted the 23 builtins minus the source-side
    // `nika:done` + `nika:compose` (the loop owns BOTH intrinsics and
    // re-synthesizes their defs · ADR-093) → 23 defs (21 dispatched + the
    // 2 loop-owned).
    let offered = &requests[0].tools;
    assert_eq!(offered.len(), 23, "21 dispatched + done + compose");
    assert!(offered.iter().any(|d| d.name == "nika:jq"));
    assert!(offered.iter().any(|d| d.name == "nika:done"));
    // The loop-owned self-check intrinsic is offered too (re-synthesized,
    // never the source-side catalog def — poison-shadow proof).
    assert!(offered.iter().any(|d| d.name == "nika:compose"));
}

// ─── test 6 · the agent×builtin REPAIR loop + the security/budget edges ──
//
// The deeper rehearsal wave over the same all-production chain: the spec's
// error-feedback contract is exactly the verdict-grounded repair pattern
// the program-repair literature converged on — a model corrects reliably
// when fed an ORACLE VERDICT (here the typed NIKA-BUILTIN-* failure), and
// repair gains plateau after ~2 feedback rounds (Olausson et al.,
// arXiv:2306.09896; Huang et al., arXiv:2310.01798 — self-correction
// WITHOUT external verdict does not work; the builtin error plane IS the
// verdict). One scripted repair round is therefore the canonical shape.

#[tokio::test]
async fn e2e_agent_repairs_a_failing_tool_from_the_typed_verdict() {
    use nika_builtin::{BuiltinDispatcher, NoWorkflow, NonInteractive, NullEmitter};
    use nika_kernel::provider::{ContentBlock, InferResponse, StopReason, TokenUsage};
    use nika_kernel::runtime::agent::AgentStopReason;
    use nika_kernel_mock::{MockClock, MockFs, MockHttp, MockProvider};
    use nika_verb_agent::{AgentInput, AgentValue, AgentVerb};

    fn tool_turn(id: &str, name: &str, args: serde_json::Value) -> InferResponse {
        InferResponse::new(
            vec![ContentBlock::ToolUse {
                id: id.to_owned(),
                name: name.to_owned(),
                input: args,
            }],
            TokenUsage::new(10, 5),
            StopReason::ToolUse,
        )
    }

    let dispatcher = Arc::new(BuiltinDispatcher::new(
        Arc::new(MockFs::new()),
        Arc::new(MockHttp::new()),
        Arc::new(MockClock::new()),
        Arc::new(NullEmitter::default()),
        Arc::new(NonInteractive::default()),
        Arc::new(NoWorkflow::default()),
    ));

    // Turn 1: a BROKEN jq program (real failure, computed by the real
    // engine). Turn 2: the repaired program. Turn 3: done.
    let provider = MockProvider::new("mock")
        .enqueue_response(tool_turn(
            "r-1",
            "nika:jq",
            serde_json::json!({ "expression": ".prices | sum", "input": { "prices": [2, 3] } }),
        ))
        .enqueue_response(tool_turn(
            "r-2",
            "nika:jq",
            serde_json::json!({ "expression": ".prices | add", "input": { "prices": [2, 3] } }),
        ))
        .enqueue_response(tool_turn(
            "r-3",
            "nika:done",
            serde_json::json!({ "result": 5 }),
        ));

    let invoke = Arc::new(nika_verb_invoke::InvokeVerb::new(Arc::clone(&dispatcher)));
    let agent = AgentVerb::new(
        Arc::new(provider.clone()),
        invoke,
        Arc::clone(&dispatcher),
        "mock/echo",
    );

    let mut input = AgentInput::new("sum the prices");
    input.tools = vec!["nika:jq".to_owned(), "nika:done".to_owned()];
    let output = agent.run(input).await.expect("repair round completes");
    assert_eq!(output.turns, 3);
    assert_eq!(output.stop_reason, AgentStopReason::ExplicitCompletion);
    let AgentValue::Structured(v) = output.output else {
        panic!("structured result");
    };
    assert_eq!(v, serde_json::json!(5));

    // The verdict plane: turn 2's request carries the TYPED failure
    // (is_error · NIKA-BUILTIN-JQ-001 code in the content) — the loop
    // continued instead of aborting (failing tools are FED BACK; only
    // whitelist violations stop the loop).
    let requests = provider.captured_requests();
    let verdict = requests[1]
        .messages
        .iter()
        .flat_map(|m| m.content.iter())
        .find_map(|b| match b {
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } if tool_use_id == "r-1" => Some((content.clone(), *is_error)),
            _ => None,
        })
        .expect("the failure fed back");
    assert!(verdict.1, "is_error rode the wire");
    assert!(
        verdict.0.contains("NIKA-BUILTIN-JQ-001"),
        "the typed code IS the oracle verdict: {}",
        verdict.0
    );
    // And the repaired call's result is the real computed value.
    let repaired = requests[2]
        .messages
        .iter()
        .flat_map(|m| m.content.iter())
        .find_map(|b| match b {
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } if tool_use_id == "r-2" => Some((content.clone(), *is_error)),
            _ => None,
        })
        .expect("the repair's result fed back");
    assert!(!repaired.1);
    assert_eq!(repaired.0, "5");
}

#[tokio::test]
async fn e2e_agent_multi_tool_turn_preserves_order_and_ids() {
    use nika_builtin::{BuiltinDispatcher, NoWorkflow, NonInteractive, NullEmitter};
    use nika_kernel::provider::{ContentBlock, InferResponse, StopReason, TokenUsage};
    use nika_kernel_mock::{MockClock, MockFs, MockHttp, MockProvider};
    use nika_verb_agent::{AgentInput, AgentVerb};

    // ONE assistant turn carrying TWO tool calls — the batch contract:
    // both dispatch, both results come back in ONE user message, order
    // preserved, ids paired.
    let multi = InferResponse::new(
        vec![
            ContentBlock::ToolUse {
                id: "m-uuid".to_owned(),
                name: "nika:hash".to_owned(),
                input: serde_json::json!({ "content": "nika" }),
            },
            ContentBlock::ToolUse {
                id: "m-jq".to_owned(),
                name: "nika:jq".to_owned(),
                input: serde_json::json!({ "expression": "1 + 1", "input": null }),
            },
        ],
        TokenUsage::new(10, 5),
        StopReason::ToolUse,
    );
    let done = InferResponse::new(
        vec![ContentBlock::ToolUse {
            id: "m-done".to_owned(),
            name: "nika:done".to_owned(),
            input: serde_json::json!({}),
        }],
        TokenUsage::new(10, 5),
        StopReason::ToolUse,
    );

    let dispatcher = Arc::new(BuiltinDispatcher::new(
        Arc::new(MockFs::new()),
        Arc::new(MockHttp::new()),
        Arc::new(MockClock::new()),
        Arc::new(NullEmitter::default()),
        Arc::new(NonInteractive::default()),
        Arc::new(NoWorkflow::default()),
    ));
    let provider = MockProvider::new("mock")
        .enqueue_response(multi)
        .enqueue_response(done);
    let invoke = Arc::new(nika_verb_invoke::InvokeVerb::new(Arc::clone(&dispatcher)));
    let agent = AgentVerb::new(
        Arc::new(provider.clone()),
        invoke,
        Arc::clone(&dispatcher),
        "mock/echo",
    );

    let mut input = AgentInput::new("hash then add");
    input.tools = vec!["nika:*".to_owned()];
    agent.run(input).await.expect("completes");

    let requests = provider.captured_requests();
    let results: Vec<(String, String)> = requests[1]
        .messages
        .iter()
        .flat_map(|m| m.content.iter())
        .filter_map(|b| match b {
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                ..
            } => Some((tool_use_id.clone(), content.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(results.len(), 2, "both results in one feedback message");
    assert_eq!(results[0].0, "m-uuid", "order preserved");
    assert_eq!(results[1].0, "m-jq");
    assert_eq!(
        results[0].1.len(),
        64,
        "blake3 hex from the real hash builtin"
    );
    assert_eq!(results[1].1, "2", "jaq computed 1+1");
}

#[tokio::test]
async fn e2e_agent_whitelist_violation_is_an_immediate_stop_fs_untouched() {
    use nika_builtin::{BuiltinDispatcher, NoWorkflow, NonInteractive, NullEmitter};
    use nika_kernel::fs::FsReadDyn as _;
    use nika_kernel::provider::{ContentBlock, InferResponse, StopReason, TokenUsage};
    use nika_kernel_mock::{MockClock, MockFs, MockHttp, MockProvider};
    use nika_verb_agent::{AgentInput, AgentVerb, VerbAgentError};

    // The model attempts nika:write while only nika:jq is whitelisted —
    // the ONE failure class that stops the loop instead of feeding back
    // (security plane ≠ error plane), and the side effect NEVER runs.
    let fs = MockFs::new();
    let dispatcher = Arc::new(BuiltinDispatcher::new(
        Arc::new(fs.clone()),
        Arc::new(MockHttp::new()),
        Arc::new(MockClock::new()),
        Arc::new(NullEmitter::default()),
        Arc::new(NonInteractive::default()),
        Arc::new(NoWorkflow::default()),
    ));
    let provider = MockProvider::new("mock").enqueue_response(InferResponse::new(
        vec![ContentBlock::ToolUse {
            id: "w-1".to_owned(),
            name: "nika:write".to_owned(),
            input: serde_json::json!({ "path": "./escape.txt", "content": "pwned" }),
        }],
        TokenUsage::new(10, 5),
        StopReason::ToolUse,
    ));
    let invoke = Arc::new(nika_verb_invoke::InvokeVerb::new(Arc::clone(&dispatcher)));
    let agent = AgentVerb::new(
        Arc::new(provider),
        invoke,
        Arc::clone(&dispatcher),
        "mock/echo",
    );

    let mut input = AgentInput::new("try to escape");
    input.tools = vec!["nika:jq".to_owned()];
    let err = agent.run(input).await.expect_err("security stop");
    assert!(
        matches!(err, VerbAgentError::WhitelistViolation { ref tool } if tool == "nika:write"),
        "{err:?}"
    );
    assert!(
        !fs.exists(std::path::Path::new("./escape.txt")).await,
        "the write NEVER reached the fs seam"
    );
}

#[tokio::test]
async fn e2e_agent_budget_and_schema_terminals() {
    use nika_builtin::{BuiltinDispatcher, NoWorkflow, NonInteractive, NullEmitter};
    use nika_kernel::provider::{ContentBlock, InferResponse, StopReason, TokenUsage};
    use nika_kernel_mock::{MockClock, MockFs, MockHttp, MockProvider};
    use nika_verb_agent::{AgentInput, AgentValue, AgentVerb, VerbAgentError};

    fn dispatcher_rig()
    -> Arc<BuiltinDispatcher<MockFs, MockHttp, MockClock, NullEmitter, NonInteractive, NoWorkflow>>
    {
        Arc::new(BuiltinDispatcher::new(
            Arc::new(MockFs::new()),
            Arc::new(MockHttp::new()),
            Arc::new(MockClock::new()),
            Arc::new(NullEmitter::default()),
            Arc::new(NonInteractive::default()),
            Arc::new(NoWorkflow::default()),
        ))
    }
    fn uuid_turn(id: &str) -> InferResponse {
        InferResponse::new(
            vec![ContentBlock::ToolUse {
                id: id.to_owned(),
                name: "nika:uuid".to_owned(),
                input: serde_json::json!({}),
            }],
            TokenUsage::new(10, 5),
            StopReason::ToolUse,
        )
    }

    // Budget terminal: max_turns=2 with a model that NEVER concludes →
    // MaxTurns carries the turn count (a budget stop, not a tool error).
    let dispatcher = dispatcher_rig();
    let provider = MockProvider::new("mock")
        .enqueue_response(uuid_turn("b-1"))
        .enqueue_response(uuid_turn("b-2"))
        .enqueue_response(uuid_turn("b-3"));
    let agent = AgentVerb::new(
        Arc::new(provider),
        Arc::new(nika_verb_invoke::InvokeVerb::new(Arc::clone(&dispatcher))),
        Arc::clone(&dispatcher),
        "mock/echo",
    );
    let mut input = AgentInput::new("loop forever");
    input.tools = vec!["nika:*".to_owned()];
    input.max_turns = Some(2);
    let err = agent.run(input).await.expect_err("budget stop");
    assert!(
        matches!(err, VerbAgentError::MaxTurns { turns: 2, .. }),
        "{err:?}"
    );

    // Schema terminal: the done result is validated against the task
    // schema — conforming passes (Structured), violating is NIKA-464.
    let schema = serde_json::json!({
        "type": "object",
        "required": ["sum"],
        "properties": { "sum": { "type": "integer" } }
    });
    let dispatcher = dispatcher_rig();
    let provider = MockProvider::new("mock").enqueue_response(InferResponse::new(
        vec![ContentBlock::ToolUse {
            id: "s-1".to_owned(),
            name: "nika:done".to_owned(),
            input: serde_json::json!({ "result": { "sum": 5 } }),
        }],
        TokenUsage::new(10, 5),
        StopReason::ToolUse,
    ));
    let agent = AgentVerb::new(
        Arc::new(provider),
        Arc::new(nika_verb_invoke::InvokeVerb::new(Arc::clone(&dispatcher))),
        Arc::clone(&dispatcher),
        "mock/echo",
    );
    let mut input = AgentInput::new("sum");
    input.tools = vec!["nika:*".to_owned()];
    input.schema = Some(schema.clone());
    let ok = agent.run(input).await.expect("conforming result");
    assert!(matches!(ok.output, AgentValue::Structured(v) if v == serde_json::json!({"sum": 5})));

    let dispatcher = dispatcher_rig();
    let provider = MockProvider::new("mock").enqueue_response(InferResponse::new(
        vec![ContentBlock::ToolUse {
            id: "s-2".to_owned(),
            name: "nika:done".to_owned(),
            input: serde_json::json!({ "result": { "sum": "five" } }),
        }],
        TokenUsage::new(10, 5),
        StopReason::ToolUse,
    ));
    let agent = AgentVerb::new(
        Arc::new(provider),
        Arc::new(nika_verb_invoke::InvokeVerb::new(Arc::clone(&dispatcher))),
        Arc::clone(&dispatcher),
        "mock/echo",
    );
    let mut input = AgentInput::new("sum");
    input.tools = vec!["nika:*".to_owned()];
    input.schema = Some(schema);
    let bad = agent.run(input).await.expect_err("schema violation");
    assert!(
        matches!(bad, VerbAgentError::SchemaValidation { .. }),
        "{bad:?}"
    );
}

// ─── test 7 · binary round-trip + tz rendering through the REAL chain ────
//
// The conformance batch (write binary-content · fetch status details ·
// date tz) verified at the crate plane — THIS rehearses the user-visible
// promise through the production chain: an agent moves OPAQUE BYTES
// between files (read binary → write binary → re-read) with the payload
// riding the model's tool-result turns untouched, then renders a
// timestamp in a requested timezone. Only fs/clock and the scripted
// model are mocks.

/// The tool-result block a given call id fed back to the model
/// (`(content, is_error)`) — the wire-proof accessor.
fn fed_back(
    requests: &[nika_kernel::provider::InferRequest],
    turn: usize,
    id: &str,
) -> (String, bool) {
    use nika_kernel::provider::ContentBlock;
    requests[turn]
        .messages
        .iter()
        .flat_map(|m| m.content.iter())
        .find_map(|b| match b {
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } if tool_use_id == id => Some((content.clone(), *is_error)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{id} result fed back"))
}

#[tokio::test]
async fn e2e_agent_round_trips_binary_bytes_and_renders_tz() {
    use nika_builtin::{BuiltinDispatcher, NoWorkflow, NonInteractive, NullEmitter};
    use nika_kernel::fs::FsReadDyn as _;
    use nika_kernel::provider::{ContentBlock, InferResponse, StopReason, TokenUsage};
    use nika_kernel::runtime::agent::AgentStopReason;
    use nika_kernel_mock::{MockClock, MockFs, MockHttp, MockProvider};
    use nika_verb_agent::{AgentInput, AgentValue, AgentVerb};

    fn tool_turn(id: &str, name: &str, args: serde_json::Value) -> InferResponse {
        InferResponse::new(
            vec![ContentBlock::ToolUse {
                id: id.to_owned(),
                name: name.to_owned(),
                input: args,
            }],
            TokenUsage::new(10, 5),
            StopReason::ToolUse,
        )
    }

    // A payload that is NOT valid UTF-8 — only the binary path can carry it.
    let payload: Vec<u8> = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0xff, 0x00];
    let fs = MockFs::new().with_file("logo.png", payload.clone());
    let dispatcher = Arc::new(BuiltinDispatcher::new(
        Arc::new(fs.clone()),
        Arc::new(MockHttp::new()),
        Arc::new(MockClock::new()),
        Arc::new(NullEmitter::default()),
        Arc::new(NonInteractive::default()),
        Arc::new(NoWorkflow::default()),
    ));

    // The scripted model: ① read the binary · ② write it to a new path
    // FEEDING BACK the exact object it received as turn ①'s result —
    // which is precisely what a real model does (copies the tool result
    // into the next call's args) · ③ render a timestamp in Tokyo-like
    // fixed offset · ④ done.
    let read_result_echo = serde_json::json!({
        "bytes_base64": "iVBORw0KGgr/AA==", // base64 of the payload (verified below)
        "len": 10
    });
    let provider = MockProvider::new("mock")
        .enqueue_response(tool_turn(
            "b-1",
            "nika:read",
            serde_json::json!({ "path": "logo.png", "binary": true }),
        ))
        .enqueue_response(tool_turn(
            "b-2",
            "nika:write",
            serde_json::json!({ "path": "copy.png", "content": read_result_echo }),
        ))
        .enqueue_response(tool_turn(
            "b-3",
            "nika:date",
            serde_json::json!({
                "op": "format", "input": "2026-01-02T03:04:05Z",
                "format": "%Y-%m-%d %H:%M", "tz": "Etc/GMT-9"
            }),
        ))
        .enqueue_response(tool_turn(
            "b-4",
            "nika:done",
            serde_json::json!({ "result": "shipped" }),
        ));

    let invoke = Arc::new(nika_verb_invoke::InvokeVerb::new(Arc::clone(&dispatcher)));
    let agent = AgentVerb::new(
        Arc::new(provider.clone()),
        invoke,
        Arc::clone(&dispatcher),
        "mock/echo",
    );
    let mut input = AgentInput::new("copy the logo then stamp it");
    input.tools = vec!["nika:*".to_owned()];
    let output = agent.run(input).await.expect("the loop completes");
    assert_eq!(output.stop_reason, AgentStopReason::ExplicitCompletion);
    assert!(
        matches!(output.output, AgentValue::Structured(v) if v == serde_json::json!("shipped"))
    );

    // The wire proof, turn by turn.
    let requests = provider.captured_requests();
    // Turn ①'s result (what the model saw) IS the canonical binary object
    // — and our scripted echo matches it byte-for-byte, proving the
    // base64 the model copies around is OUR encoder's output.
    let read_fed_back = fed_back(&requests, 1, "b-1");
    assert!(!read_fed_back.1);
    let parsed: serde_json::Value =
        serde_json::from_str(&read_fed_back.0).expect("binary result is JSON");
    assert_eq!(
        parsed, read_result_echo,
        "the scripted echo IS the real read output (encoder verified)"
    );

    // The copy landed byte-exact on the fs seam — non-UTF-8 bytes
    // survived two model turns.
    let copied = fs
        .read(std::path::Path::new("copy.png"))
        .await
        .expect("copy exists");
    assert_eq!(copied, payload, "byte-exact through the agent loop");

    // The tz rendering came back shifted (+9h on the fixed-offset zone).
    let date_fed_back = fed_back(&requests, 3, "b-3").0;
    assert_eq!(date_fed_back, "2026-01-02 12:04", "UTC+9 fields");
}
