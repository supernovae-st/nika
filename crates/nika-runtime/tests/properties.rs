// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! Property battery — random DAGs through the REAL parse → check → run
//! chain. The determinism THEOREMS hold over the whole input space,
//! not just the hand-picked fixtures:
//!
//! 1. **Replay** · same workflow twice ⇒ byte-identical event streams.
//! 2. **Cap-equivalence** · cap=1 vs wave-width ⇒ identical streams
//!    (ordered settlement makes concurrency unobservable).
//! 3. **Settle-exactly-once** · every task gets EXACTLY ONE terminal
//!    frame · every task has a record · every record is terminal.
//! 4. **Cascade soundness** · a default-gate task over a dead dep is
//!    CANCELLED (Dead-Path-Elimination · transitively).
//! 5. **Verdict law** · `ok` ⟺ zero Failure records.
//!
//! Every task is `infer` over the mock/echo provider (echo = pure
//! function of the prompt · no mock queues ⇒ no dequeue-order coupling
//! under concurrency) · runtime failures are injected via `for_each`
//! over a task output that is a scalar (statically clean · NIKA-VAR-006 at
//! run time). The collection rides `scalar_src`, not a `const:`: a constant
//! is never caller-supplied, so its literal IS its run value and the check
//! now refuses a non-array one outright.

use std::num::NonZeroUsize;
use std::sync::Arc;

use nika_check::check;
use nika_event::{Event, EventKind};
use nika_kernel_mock::{MockClock, MockProvider, MockShell, MockToolDefinitionProvider};
use nika_providers::{NoHttp, ProviderRegistry, ProvidersConfig};
use nika_runtime::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, TaskStatus, VecSink};
use nika_schema::{FileId, ParseMode, parse};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;
use proptest::prelude::*;

/// One generated task's shape.
#[derive(Debug, Clone)]
enum Kind {
    /// Plain infer · prompt renders every dep's output.
    Normal,
    /// Explicit `when:` over publish=no — always skipped (the gate
    /// REPLACES the default · evaluated even over dead deps).
    Gated,
    /// `for_each` over a scalar TASK OUTPUT — statically clean · fails at run
    /// time (`NIKA-VAR-006` · `FailedBeforeStart` · cascades).
    Fails,
    /// `agent:` over the echo provider (one text turn · no tools) —
    /// exercises the BUFFERED telemetry path (ADR-096): the agent's
    /// decisions ride the Finish to the ordered settle, so the
    /// determinism theorems must hold over them too.
    Agent,
}

#[derive(Debug, Clone)]
struct TaskSpec {
    deps: Vec<usize>,
    kind: Kind,
}

/// Random DAG: task i may depend on tasks < i (acyclic by construction).
fn dag_strategy() -> impl Strategy<Value = Vec<TaskSpec>> {
    proptest::collection::vec((proptest::bits::u8::ANY, 0_u8..10), 1..7).prop_map(|raws| {
        raws.into_iter()
            .enumerate()
            .map(|(i, (dep_bits, kind_roll))| {
                // Up to 3 deps drawn from the prior tasks' indices.
                let deps: Vec<usize> = (0..i.min(3))
                    .filter(|bit| dep_bits & (1 << bit) != 0)
                    .map(|bit| i - 1 - bit)
                    .collect();
                let kind = match kind_roll {
                    0 | 1 => Kind::Gated,
                    2 => Kind::Fails,
                    3 | 4 => Kind::Agent,
                    _ => Kind::Normal,
                };
                TaskSpec { deps, kind }
            })
            .collect()
    })
}

fn yaml_of(specs: &[TaskSpec]) -> String {
    use std::fmt::Write as _;
    let mut y = String::from(
        "nika: prop\nmodel: mock/echo\nconst:\n  publish: \"no\"\ntasks:\n  \
         scalar_src:\n    infer: { prompt: \"not a list\" }\n",
    );
    for (i, spec) in specs.iter().enumerate() {
        let _ = writeln!(y, "  t{i}:");
        // W2 doors: a Normal consumer BINDS its inputs (the binding IS the
        // edge); every other kind orders on state via `after:`.
        if !spec.deps.is_empty() {
            if matches!(spec.kind, Kind::Normal) {
                let _ = writeln!(y, "    with:");
                for d in &spec.deps {
                    let _ = writeln!(y, "      b{d}: \"${{{{ tasks.t{d}.output }}}}\"");
                }
            } else {
                let entries: Vec<String> =
                    spec.deps.iter().map(|d| format!("t{d}: success")).collect();
                let _ = writeln!(y, "    after: {{ {} }}", entries.join(", "));
            }
        }
        match spec.kind {
            Kind::Gated => {
                let _ = writeln!(y, "    when: ${{{{ const.publish == 'yes' }}}}");
                let _ = writeln!(y, "    infer: {{ prompt: \"gated {i}\" }}");
            }
            Kind::Fails => {
                let _ = writeln!(
                    y,
                    "    with: {{ src: \"${{{{ tasks.scalar_src.output }}}}\" }}"
                );
                let _ = writeln!(y, "    for_each: {{ items: \"${{{{ with.src }}}}\" }}");
                let _ = writeln!(y, "    infer: {{ prompt: \"iter ${{{{ item }}}}\" }}");
            }
            Kind::Normal => {
                let mut refs = String::new();
                for d in &spec.deps {
                    let _ = write!(refs, " ${{{{ with.b{d} }}}}");
                }
                let _ = writeln!(y, "    infer: {{ prompt: \"t{i}{refs}\" }}");
            }
            Kind::Agent => {
                let _ = writeln!(y, "    agent: {{ prompt: \"agent t{i}\" }}");
            }
        }
    }
    y
}

type PropRuntime = Runtime<
    MockShell,
    nika_kernel_mock::MockToolExecutor,
    NoHttp,
    MockProvider,
    MockToolDefinitionProvider,
    MockClock,
>;

fn prop_runtime(cap: Option<NonZeroUsize>, agent_tasks: usize) -> PropRuntime {
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(
        nika_kernel_mock::MockToolExecutor::new(),
    )));
    // One IDENTICAL text turn per agent task: the shared FIFO queue's
    // dequeue order under concurrency becomes inconsequential — the
    // streams stay byte-comparable across caps (the determinism law
    // must hold over the BUFFERED telemetry path too · ADR-096).
    let mut provider = MockProvider::new("mock");
    for _ in 0..agent_tasks {
        provider = provider.enqueue_text("done");
    }
    Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(provider),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::new(cap, 7),
    )
}

fn run_once(yaml: &str, cap: Option<NonZeroUsize>, agent_tasks: usize) -> (RunOutcome, Vec<Event>) {
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("generated YAML parses");
    let report = check(&wf);
    assert!(
        report.is_clean(),
        "generated DAG passes the ladder:\n{yaml}"
    );
    let runtime = prop_runtime(cap, agent_tasks);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("test runtime");
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = rt
        .block_on(runtime.run(&wf, &report, &mut stamper, &mut sink))
        .expect("clean run");
    (outcome, sink.into_events())
}

fn task_field(event: &Event) -> Option<&str> {
    event.fields.iter().find(|f| f.key == "task").and_then(|f| {
        if let nika_types::resource::Value::String(s) = &f.value {
            Some(s.as_str())
        } else {
            None
        }
    })
}

const TERMINAL_TASK_KINDS: [EventKind; 4] = [
    EventKind::TaskCompleted,
    EventKind::TaskFailed,
    EventKind::TaskSkipped,
    EventKind::TaskCancelled,
];

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 48,
        ..ProptestConfig::default()
    })]

    #[test]
    fn random_dags_uphold_the_determinism_theorems(specs in dag_strategy()) {
        let yaml = yaml_of(&specs);
        // The generated ids are `t0..t{specs.len()}`; the workflow also
        // carries `scalar_src`, the fixed preamble task whose scalar
        // output a `Fails` lane fans over. It is a real task with a real
        // record, so WHOLE-WORKFLOW counts include it while the per-id
        // sweeps below stay over the generated ids alone.
        let generated = specs.len();
        let n = generated + 1;
        // Identical queued turns ⇒ dequeue order is inconsequential ·
        // a generous count covers the live agents (dead ones never draw).
        let agents = specs.iter().filter(|s| matches!(s.kind, Kind::Agent)).count();

        // ── 1 · replay: same workflow twice ⇒ byte-identical streams.
        let (outcome_a, events_a) = run_once(&yaml, None, agents);
        let (_, events_b) = run_once(&yaml, None, agents);
        prop_assert_eq!(&events_a, &events_b, "replay diverged:\n{}", yaml);

        // ── 2 · cap-equivalence: cap=1 ⇒ the SAME stream (the buffered
        // agent telemetry rides the ordered settle · ADR-096 — covered).
        let (_, events_seq) = run_once(&yaml, NonZeroUsize::new(1), agents);
        prop_assert_eq!(&events_a, &events_seq, "cap leaked:\n{}", yaml);

        // ── 3 · settle-exactly-once + records complete.
        prop_assert_eq!(outcome_a.records.len(), n, "every task has a record");
        prop_assert_eq!(events_a.first().map(|e| e.kind), Some(EventKind::WorkflowStarted));
        let scheduled = events_a.iter().filter(|e| e.kind == EventKind::TaskScheduled).count();
        prop_assert_eq!(scheduled, n, "one TaskScheduled per task");
        let last = events_a.last().expect("non-empty").kind;
        prop_assert!(
            matches!(last, EventKind::WorkflowCompleted | EventKind::WorkflowFailed),
            "stream ends on the terminal frame"
        );
        for i in 0..generated {
            let id = format!("t{i}");
            let terminals = events_a
                .iter()
                .filter(|e| TERMINAL_TASK_KINDS.contains(&e.kind) && task_field(e) == Some(id.as_str()))
                .count();
            prop_assert_eq!(terminals, 1, "task {} settles exactly once:\n{}", id, yaml);
        }

        // ── 4 · cascade soundness (GATE-v2 · spec 03 §gate algebra):
        // per-edge pass-sets judge admission — a Normal consumer's value
        // edges pass {success, skipped}; every other kind's `after:
        // success` edges pass {success} only. Any producer outside a
        // pass-set cancels the consumer (dead-path elimination,
        // transitive). Admitted: Gated skips (publish=no · POST-gate) ·
        // Fails fails (for_each over a scalar) · Normal/Agent succeed.
        for (i, spec) in specs.iter().enumerate() {
            let id = format!("t{i}");
            let status = outcome_a.records[&id].status;
            let admitted = spec.deps.iter().all(|d| {
                let dep = outcome_a.records[&format!("t{d}")].status;
                if matches!(spec.kind, Kind::Normal) {
                    matches!(dep, TaskStatus::Success | TaskStatus::Skipped)
                } else {
                    matches!(dep, TaskStatus::Success)
                }
            });
            if !admitted {
                prop_assert_eq!(
                    status, TaskStatus::Cancelled,
                    "t{}'s gate did not admit — cancelled (dead path):\n{}", i, yaml
                );
                continue;
            }
            match spec.kind {
                Kind::Gated => prop_assert_eq!(
                    status, TaskStatus::Skipped,
                    "admitted gated t{} skips (when is POST-gate):\n{}", i, yaml
                ),
                Kind::Fails => prop_assert_eq!(
                    status, TaskStatus::Failure,
                    "admitted fails-task t{} is failure:\n{}", i, yaml
                ),
                Kind::Normal => prop_assert_eq!(
                    status, TaskStatus::Success,
                    "admitted normal t{} succeeds:\n{}", i, yaml
                ),
                Kind::Agent => prop_assert_eq!(
                    status, TaskStatus::Success,
                    "admitted agent t{} succeeds (one echo turn):\n{}", i, yaml
                ),
            }
        }

        // ── 5 · verdict law: ok ⟺ zero Failure records.
        let any_failure = outcome_a.records.values().any(|r| r.status == TaskStatus::Failure);
        prop_assert_eq!(outcome_a.ok, !any_failure, "verdict law:\n{}", yaml);
    }
}
