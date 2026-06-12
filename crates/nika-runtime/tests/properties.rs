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
//! over a scalar var (statically clean · NIKA-VAR-006 at run time).

use std::num::NonZeroUsize;
use std::sync::Arc;

use nika_event::{Event, EventKind};
use nika_kernel_mock::{MockClock, MockProvider, MockShell, MockToolDefinitionProvider};
use nika_providers::{NoHttp, ProviderRegistry, ProvidersConfig};
use nika_runtime::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, TaskStatus, VecSink};
use nika_schema::{FileId, ParseMode, check, parse};
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
    /// `for_each` over a scalar var — statically clean · fails at run
    /// time (`NIKA-VAR-006` · `FailedBeforeStart` · cascades).
    Fails,
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
        "nika: v1\nworkflow: prop\nmodel: mock/echo\nvars:\n  publish: \"no\"\n  scalar: \"not a list\"\ntasks:\n",
    );
    for (i, spec) in specs.iter().enumerate() {
        let _ = writeln!(y, "  - id: t{i}");
        if !spec.deps.is_empty() {
            let deps: Vec<String> = spec.deps.iter().map(|d| format!("t{d}")).collect();
            let _ = writeln!(y, "    depends_on: [{}]", deps.join(", "));
        }
        match spec.kind {
            Kind::Gated => {
                let _ = writeln!(y, "    when: ${{{{ vars.publish == 'yes' }}}}");
                let _ = writeln!(y, "    infer: {{ prompt: \"gated {i}\" }}");
            }
            Kind::Fails => {
                let _ = writeln!(y, "    for_each: ${{{{ vars.scalar }}}}");
                let _ = writeln!(y, "    infer: {{ prompt: \"iter ${{{{ item }}}}\" }}");
            }
            Kind::Normal => {
                let mut refs = String::new();
                for d in &spec.deps {
                    let _ = write!(refs, " ${{{{ tasks.t{d}.output }}}}");
                }
                let _ = writeln!(y, "    infer: {{ prompt: \"t{i}{refs}\" }}");
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

fn prop_runtime(cap: Option<NonZeroUsize>) -> PropRuntime {
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(
        nika_kernel_mock::MockToolExecutor::new(),
    )));
    Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::new(cap, 7),
    )
}

fn run_once(yaml: &str, cap: Option<NonZeroUsize>) -> (RunOutcome, Vec<Event>) {
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("generated YAML parses");
    let report = check(&wf);
    assert!(
        report.is_clean(),
        "generated DAG passes the ladder:\n{yaml}"
    );
    let runtime = prop_runtime(cap);
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
        let n = specs.len();

        // ── 1 · replay: same workflow twice ⇒ byte-identical streams.
        let (outcome_a, events_a) = run_once(&yaml, None);
        let (_, events_b) = run_once(&yaml, None);
        prop_assert_eq!(&events_a, &events_b, "replay diverged:\n{}", yaml);

        // ── 2 · cap-equivalence: cap=1 ⇒ the SAME stream.
        let (_, events_seq) = run_once(&yaml, NonZeroUsize::new(1));
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
        for i in 0..n {
            let id = format!("t{i}");
            let terminals = events_a
                .iter()
                .filter(|e| TERMINAL_TASK_KINDS.contains(&e.kind) && task_field(e) == Some(id.as_str()))
                .count();
            prop_assert_eq!(terminals, 1, "task {} settles exactly once:\n{}", id, yaml);
        }

        // ── 4 · cascade soundness (Dead-Path-Elimination · transitive):
        // a DEFAULT-gate task over any dep in {failure, cancelled} is
        // cancelled · a Gated task is always skipped (publish=no · the
        // explicit gate replaces the default · evaluated even over dead
        // deps) · a Fails task with live deps is failure.
        for (i, spec) in specs.iter().enumerate() {
            let id = format!("t{i}");
            let status = outcome_a.records[&id].status;
            let dead_dep = spec.deps.iter().any(|d| {
                matches!(
                    outcome_a.records[&format!("t{d}")].status,
                    TaskStatus::Failure | TaskStatus::Cancelled
                )
            });
            match spec.kind {
                Kind::Gated => prop_assert_eq!(
                    status, TaskStatus::Skipped,
                    "gated t{} is skipped whatever its deps:\n{}", i, yaml
                ),
                Kind::Normal | Kind::Fails if dead_dep => prop_assert_eq!(
                    status, TaskStatus::Cancelled,
                    "default-gate t{} over a dead dep is cancelled:\n{}", i, yaml
                ),
                Kind::Fails => prop_assert_eq!(
                    status, TaskStatus::Failure,
                    "live fails-task t{} is failure:\n{}", i, yaml
                ),
                Kind::Normal => prop_assert_eq!(
                    status, TaskStatus::Success,
                    "live normal t{} succeeds:\n{}", i, yaml
                ),
            }
        }

        // ── 5 · verdict law: ok ⟺ zero Failure records.
        let any_failure = outcome_a.records.values().any(|r| r.status == TaskStatus::Failure);
        prop_assert_eq!(outcome_a.ok, !any_failure, "verdict law:\n{}", yaml);
    }
}
