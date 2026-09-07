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
//! the fixture's gate (`const.publish == 'yes'` · publish=no) evaluates
//! closed, and the skip path is exactly what we assert.

use std::collections::BTreeMap;
use std::sync::Arc;

use nika_check::CheckReport;
use nika_check::{check, infer_permits};
use nika_cli::{RunView, TaskState, Theme, frame};
use nika_event::{Event, EventKind};
use nika_kernel::tool_executor::ToolResult;
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{DeterministicStamper, Runtime, RuntimeConfig, VecSink};
use nika_schema::raw::{RawAction, RawWorkflow};
use nika_schema::{FileId, ParseMode, parse};
use nika_types::resource::Value;
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::{InferInput, InferValue, InferVerb};
use nika_verb_invoke::InvokeVerb;

// ─── the fixture · one workflow, all three shipped verbs ───────────────

/// Human-gated diamond DAG: an approve gate dominating both roots
/// (NEP-0002), two parallel sources, a typed extraction, a synthesis
/// joining both sources, a persist joining both infers, and a
/// statically-closed gate. 5 waves · 7 tasks.
const WORKFLOW_OK: &str = r#"
nika: e2e-veille

model: mock/echo

permits:
  tools: ["nika:prompt", "nika:read", "nika:write"]
  exec: ["wc", "echo"]
  fs:
    read: ["./news.json"]
    write: ["./out/report.md"]

const:
  source: "./news.json"
  publish: "no"

tasks:
  # The human gate (NEP-0002 · v2.2 made `exec:` born-ingress). `probe`'s
  # stdout is content this workflow did not author, it meets a private
  # `fs.read` and reaches `fs.write` + a second exec: the lethal trifecta.
  # The gate comes FIRST and BOTH roots descend from it — `gather` carries
  # the private read, `probe` the untrusted content — because a gate on a
  # sibling branch dominates nothing.
  approve:
    invoke:
      tool: "nika:prompt"
      args:
        mode: "confirm"
        message: "read ${{ const.source }}, summarize it and write the report?"

  gather:
    with:
      go: ${{ tasks.approve.output }}
    when: ${{ with.go == true }}
    invoke:
      tool: "nika:read"
      args: { path: "${{ const.source }}" }

  probe:
    with:
      go: ${{ tasks.approve.output }}
    when: ${{ with.go == true }}
    exec:
      command: ["wc", "-l", "./news.json"]

  extract:
    with:
      gathered: ${{ tasks.gather.output }}
    infer:
      prompt: "Extract the story fields · ${{ with.gathered }}"
      schema:
        type: object
        properties:
          headline: { type: string }
          score: { type: integer }
        required: [headline, score]

  think:
    with:
      gathered: ${{ tasks.gather.output }}
      lines: ${{ tasks.probe.output }}
    infer:
      prompt: "Summarize · ${{ with.gathered }} · lines ${{ with.lines }}"
      max_tokens: 800

  write_out:
    with:
      summary: ${{ tasks.think.output }}
    after:
      extract: success
    invoke:
      tool: "nika:write"
      args:
        path: "./out/report.md"
        content: "${{ with.summary }}"

  notify:
    after:
      write_out: terminal
    when: ${{ const.publish == 'yes' }}
    exec:
      command: ["echo", "done"]

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

/// Textual `${{ }}` substitution — the rehearsal stand-in for the W2
/// boundary materialization (03-dag engine work). Replaces
/// `${{ with.<binding> }}` (bindings keyed by their `with:` name) and
/// `${{ const.<key> }}` occurrences.
fn interpolate(
    text: &str,
    bindings: &BTreeMap<String, String>,
    consts: &BTreeMap<String, String>,
) -> String {
    let mut out = text.to_owned();
    for (name, value) in bindings {
        out = out.replace(&format!("${{{{ with.{name} }}}}"), value);
    }
    for (key, value) in consts {
        out = out.replace(&format!("${{{{ const.{key} }}}}"), value);
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

const PLAIN: Theme = Theme::new(false, false, false);

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
            vec!["approve"],
            vec!["gather", "probe"],
            vec!["extract", "think"],
            vec!["write_out"],
            vec!["notify"],
        ],
        "human gate → parallel sources → two infers → persist → conditional send"
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
        .enqueue_ok(
            // The REAL confirm routes its typed value on the structured
            // plane (bool · verb-invoke docs) — the affirmative `when:`
            // gate reads THAT, so the mock speaks the same shape.
            ToolResult::success("call-approve", "true").with_structured(serde_json::json!(true)),
        )
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
    assert_eq!(calls.len(), 3, "approve + gather + write_out");
    // Find by tool name, not by index — intra-wave dispatch order is the
    // engine's freedom, not this test's contract.
    let read = calls
        .iter()
        .find(|c| c.name == "nika:read")
        .expect("gather hit the tool seam");
    assert_eq!(
        read.input.get("path").and_then(serde_json::Value::as_str),
        Some("./news.json"),
        "const.source resolved into the tool args"
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
    // Seven since the NEP-0002 gate joined the graph.
    assert_eq!(view.rows().len(), 7);
    assert_eq!(view.done_count(), 7);
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

    // The frame: identity line + a 7/7 meter, no failure card.
    let lines = frame(&view, &PLAIN, 0);
    assert!(
        lines[0].contains("e2e-veille · 7 tasks"),
        "header: {}",
        lines[0]
    );
    let meter = lines
        .iter()
        .find(|l| l.contains("done"))
        .expect("meter line present");
    assert!(meter.contains("7/7 done"), "meter: {meter}");
    assert!(
        !lines.iter().any(|l| l.contains("NIKA-")),
        "no failure card on the happy path"
    );
}

// ─── test 1ter · the human says NO — the runtime half of NIKA-SEC-014 ────
//
// The static half (the checker refuses a bare `after:` gate) has its
// conformance corpus; the RUNTIME half — a refusal at the gate fires the
// gated effects ZERO times — had no test at any level. Same diamond as
// the happy path, one mock flipped: the approve builtin answers the
// refusal shape the real `nika:prompt` confirm returns.

#[tokio::test]
async fn e2e_refused_gate_fires_zero_effects() {
    let (wf, report) = parse_and_check(WORKFLOW_OK);

    // NEP-0020: a REFUSED confirm settles SUCCESS with value `false`
    // (never a failure — the gate is a decision, and that is exactly why
    // the static law exists). The real builtin routes the typed bool on
    // the structured plane; the mock speaks the same shape.
    let shell = MockShell::new(); // NOTHING enqueued: reaching it fails loudly
    let tools = MockToolExecutor::new()
        .enqueue_ok(
            ToolResult::success("call-approve", "false").with_structured(serde_json::json!(false)),
        )
        .enqueue_ok(ToolResult::success("call-write", "0.0 KB written"));
    let seams = Seams::new(shell, tools);

    let (events, ok) = execute(&wf, &report, &seams).await;
    assert!(ok, "a refusal is a decision: the run completes clean");

    // THE LAW: zero exec effects — probe's `wc` never ran (its `when:`
    // read the refusal) and notify's `echo` never ran (publish=no stays
    // closed). An empty-queue MockShell would also have panicked; the
    // explicit assertion names the contract.
    assert!(
        seams.shell.executed_commands().is_empty(),
        "the gated execs never fired: {:?}",
        seams.shell.executed_commands()
    );

    // The tool seam: approve asked, write_out persisted — `nika:read`
    // (gather's private read) was NEVER touched.
    let calls = seams.tools.captured_calls();
    assert_eq!(calls.len(), 2, "approve + write_out · gather never ran");
    assert!(
        calls.iter().all(|c| c.name != "nika:read"),
        "the private read never left the gate"
    );

    // The fold, task by task (the semantics named, not guessed):
    // · gather/probe — `when: ${{ with.go == true }}` over the refusal →
    //   SKIPPED (spec 03 §when: a closed gate is a decision).
    // · extract/think — GATE-v2 (spec 03 §gate algebra): a VALUE edge
    //   (`with:` reading `tasks.X.output`) ADMITS a skipped producer and
    //   the output reads defined-`null` (spec 04), so the consumers RUN
    //   on nulls. A skip is not a failure: nothing cascades.
    // · write_out — both its edges admit (think succeeded · extract
    //   succeeded), so the persist lane RUNS: the refusal's contract is
    //   zero gated effects + zero private-data flow, never a DAG-wide
    //   halt.
    // · notify — publish=no, its own `when:` closes it as on the happy
    //   path.
    let view = fold(&events);
    assert_eq!(view.verdict, Some(true));
    let by_id = states(&view);
    assert_eq!(by_id["approve"], TaskState::Ok);
    assert_eq!(by_id["gather"], TaskState::Skipped);
    assert_eq!(by_id["probe"], TaskState::Skipped);
    assert_eq!(by_id["extract"], TaskState::Ok);
    assert_eq!(by_id["think"], TaskState::Ok);
    assert_eq!(by_id["write_out"], TaskState::Ok);
    assert_eq!(by_id["notify"], TaskState::Skipped);
    assert_eq!(view.done_count(), 7, "every task reached a terminal state");

    // The written report carries the defined-null reads — and NEVER the
    // private bytes gather would have read had the human said yes.
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
        !written.contains(GATHER_JSON),
        "the refusal stops the DATA too: {written}"
    );

    // The stream opens and closes like any completed run — a refusal is
    // not a failure, nothing failed.
    let kinds: Vec<EventKind> = events.iter().map(|e| e.kind).collect();
    assert_eq!(kinds[0], EventKind::WorkflowStarted);
    assert_eq!(
        *kinds.last().expect("stream non-empty"),
        EventKind::WorkflowCompleted
    );
    assert!(
        !kinds.contains(&EventKind::TaskFailed),
        "a refused gate fails nothing"
    );
}

// ─── test 2 · structured output through real dataflow ──────────────────

#[tokio::test]
async fn e2e_structured_output_validates_real_dataflow() {
    let (wf, report) = parse_and_check(WORKFLOW_OK);
    assert!(report.is_clean());

    // Drive ONLY the extract lane: the prompt carries gather's mocked
    // JSON through real interpolation, and the schema'd mock SYNTHESIZES
    // a conformant instance (F3 · mock-from-schema) that must come back
    // schema-validated and typed.
    let shell = MockShell::new().enqueue_ok("42\n");
    let tools = MockToolExecutor::new()
        .enqueue_ok(
            // The REAL confirm routes its typed value on the structured
            // plane (bool · verb-invoke docs) — the affirmative `when:`
            // gate reads THAT, so the mock speaks the same shape.
            ToolResult::success("call-approve", "true").with_structured(serde_json::json!(true)),
        )
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
    bindings.insert("gathered".to_owned(), GATHER_JSON.to_owned());
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
    // The synthesized minimal instance of the extract schema (F3): the
    // required string sits on "mock", the bounded integer on its floor.
    assert_eq!(value["headline"], "mock");
    assert_eq!(value["score"], 0);
    assert_eq!(out.model_resolved, "mock/echo");
    assert!(out.usage.output_tokens > 0, "usage flows back");
}

// ─── test 3 · failure cascade with partial scheduling ───────────────────

#[tokio::test]
async fn e2e_failure_cascade_partial_schedule_and_card() {
    let (wf, report) = parse_and_check(WORKFLOW_OK);

    // probe explodes; gather's lane stays alive.
    let shell = MockShell::new().enqueue_fail(7, "disk full: /var/news");
    let tools = MockToolExecutor::new()
        .enqueue_ok(
            // The REAL confirm routes its typed value on the structured
            // plane (bool · verb-invoke docs) — the affirmative `when:`
            // gate reads THAT, so the mock speaks the same shape.
            ToolResult::success("call-approve", "true").with_structured(serde_json::json!(true)),
        )
        .enqueue_ok(ToolResult::success("call-gather", GATHER_JSON));
    let seams = Seams::new(shell, tools);

    let (events, ok) = execute(&wf, &report, &seams).await;
    assert!(!ok, "a failed task fails the workflow");

    // Partial scheduling is the point: extract (gather-only deps) RAN,
    // think/write_out/notify never did.
    let calls = seams.tools.captured_calls();
    assert_eq!(
        calls.len(),
        2,
        "approve ran · write_out never reached the tool seam"
    );
    // By NAME, not index — the file's own rule twenty lines up: intra-wave
    // dispatch order is the engine's freedom. The gate made `calls[0]` the
    // prompt and this line the only one that noticed.
    assert!(
        calls.iter().any(|c| c.name == "nika:read"),
        "gather hit the tool seam"
    );

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
    assert_eq!(
        view.done_count(),
        7,
        "every task reached a terminal state · the gate included"
    );

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
    // The workflow-visible code is the SPEC code (NIKA-EXEC-001 · spec 05
    // §142 · `on_codes:`/explain filter on it) — never the engine-internal
    // NIKA-440 (verb_err → ExecError::spec_code · nika-verb-exec errors.rs).
    assert!(
        detail.contains("NIKA-EXEC-001"),
        "exec non-zero exit spec code: {detail}"
    );
    assert!(
        detail.contains("status 7"),
        "exit status surfaces: {detail}"
    );

    let lines = frame(&view, &PLAIN, 0);
    let card = lines.join("\n");
    assert!(card.contains("NIKA-EXEC-001"), "card: {card}");
    assert!(
        card.contains("fix: nika explain NIKA-EXEC-001"),
        "explain hint: {card}"
    );
}

// ─── test 4 · the audit-before-run contract refuses a dirty file ────────

#[tokio::test]
async fn e2e_check_ladder_refuses_dirty_workflow() {
    // Two injected defects: a typo'd builtin and a body reference whose
    // edge-declaring `with:` block is gone (the binding IS the edge).
    let dirty = WORKFLOW_OK.replace("\"nika:read\"", "\"nika:reed\"").replace(
        "    with:\n      gathered: ${{ tasks.gather.output }}\n      lines: ${{ tasks.probe.output }}\n",
        "",
    );
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

    // The dangling `${{ with.gathered }}` read (its edge-declaring
    // binding was removed) lands in the conformance lane with a spec code.
    assert!(
        !report.conformance.is_empty(),
        "an unbound with.* read is a conformance finding: {report:?}"
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
        .enqueue_ok(
            // The REAL confirm routes its typed value on the structured
            // plane (bool · verb-invoke docs) — the affirmative `when:`
            // gate reads THAT, so the mock speaks the same shape.
            ToolResult::success("call-approve", "true").with_structured(serde_json::json!(true)),
        )
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
        .enqueue_ok(
            // The REAL confirm routes its typed value on the structured
            // plane (bool · verb-invoke docs) — the affirmative `when:`
            // gate reads THAT, so the mock speaks the same shape.
            ToolResult::success("call-approve", "true").with_structured(serde_json::json!(true)),
        )
        .enqueue_ok(ToolResult::success("call-gather", GATHER_JSON))
        .enqueue_ok(ToolResult::success("call-write", "2.1 KB written"));
    let seams2 = Seams::new(shell2, tools2);
    let (events2, _) = execute(&wf, &report, &seams2).await;
    let ndjson2: String = events2
        .iter()
        .map(|e| serde_json::to_string(e).expect("event serializes"))
        .collect::<Vec<_>>()
        .join("\n");
    // One field is per-run BY DESIGN and must be: the approval ticket's
    // digest folds `minted_at_ms`, which is what gives its TTL and its
    // anti-replay window meaning (nika-runtime/src/approval.rs · a ticket
    // minted at another instant is another capability). Everything else —
    // event ids, timestamps, every content hash — is byte-identical, and
    // that is the property this test exists for.
    //
    // What it also shows, and what is NOT by design: the mint reads the
    // WALL clock while every event timestamp comes from the injected
    // stamper, so the approval path escapes the Clock injection INV-027
    // asks for. Routing the mint through the injected clock would make
    // this carve-out unnecessary — recorded rather than papered over.
    assert_eq!(
        strip_approval_digest(&ndjson),
        strip_approval_digest(&ndjson2),
        "two runs, one byte-identical trace (modulo the minted-at ticket digest)"
    );
}

/// Blank the one per-run field: the approval ticket digest (see above).
fn strip_approval_digest(ndjson: &str) -> String {
    let mut out = String::with_capacity(ndjson.len());
    for line in ndjson.split('\n') {
        if line.contains("approval_decided")
            && let Some(i) = line.find("{\"key\":\"digest\",\"value\":\"")
        {
            let head = &line[..i];
            let rest = &line[i..];
            let end = rest.find("\"}").map_or(rest.len(), |j| j + 2);
            out.push_str(head);
            out.push_str("{\"key\":\"digest\",\"value\":\"<minted-at-varies>\"}");
            out.push_str(&rest[end..]);
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
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

    // The offered catalog IS the dispatcher's — DERIVED, never pinned (a
    // pin went stale the day a builtin landed while this suite ran
    // nowhere). Below min_universe the full set ships each turn.
    let offered = &requests[0].tools;
    let catalog = nika_builtin::tool_defs().len();
    assert_eq!(offered.len(), catalog, "the catalog, derived");
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
        matches!(err, VerbAgentError::WhitelistViolation { ref tool, .. } if tool == "nika:write"),
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
    // schema — conforming passes (Structured), violating is the
    // schema-gate verdict (wire NIKA-INFER-002).
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

    // A violating done result is fed back and re-asked under the loop's
    // repair budget (two by default) before the schema gate goes fatal, so
    // the seat that never conforms answers three times.
    let dispatcher = dispatcher_rig();
    let provider = MockProvider::new("mock")
        .enqueue_response(violating_done("s-2"))
        .enqueue_response(violating_done("s-3"))
        .enqueue_response(violating_done("s-4"));
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

/// A `nika:done` answer whose `result` misses the `sum` schema (a string
/// where an integer is required) — the shape the schema gate refuses.
fn violating_done(id: &str) -> nika_kernel::provider::InferResponse {
    use nika_kernel::provider::{ContentBlock, InferResponse, StopReason, TokenUsage};
    InferResponse::new(
        vec![ContentBlock::ToolUse {
            id: id.to_owned(),
            name: "nika:done".to_owned(),
            input: serde_json::json!({ "result": { "sum": "five" } }),
        }],
        TokenUsage::new(10, 5),
        StopReason::ToolUse,
    )
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
