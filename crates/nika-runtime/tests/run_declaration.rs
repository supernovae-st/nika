// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

//! F-P3 · the `run:` declaration's determinism contract, proven end to end.
//!
//! - **(a)** `entropy: seeded(42)` — two runs of the SAME file produce
//!   BYTE-identical event streams (the replay law the declaration
//!   purchases: deterministic stamps · keyed jitter · virtual clock —
//!   the seams the declaration forces, resolved by the ONE
//!   [`RunSeams::of`] the production composer calls).
//! - **(a′)** `entropy: none` — the strict lane replays byte-identical
//!   too (the fixed zero stream).
//! - **(b)** `seeded(42)` vs `seeded(43)` — the streams DIFFER, and only
//!   where entropy lives: the retry jitter's journaled `delay_ms`. The
//!   seed IS the entropy source (without this, (a) would be vacuous).
//! - **(d)** Absent `run:` — the ambient status quo: two runs DIFFER
//!   (the live `UUIDv7` lane) — the anti-vacuity guard for (a).
//!
//! Hermetic: mock verbs everywhere except the three seams the
//! declaration owns (stamper · clock · jitter seed).

use std::sync::Arc;

use nika_event::EventKind;
use nika_kernel_mock::{MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{RunOutcome, RunSeams, Runtime, RuntimeConfig, VecSink};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

/// One mock run's serialized event stream (one JSON line per event —
/// the journal's own shape) through the SAME [`RunSeams`] resolution the
/// production composer uses.
async fn run_stream(yaml: &str, shell: MockShell) -> (RunOutcome, Vec<String>) {
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(
        report.is_clean(),
        "fixture passes the ladder: {}",
        serde_json::to_string(&report).unwrap_or_default()
    );
    let seams = RunSeams::of(wf.run.as_ref().map(|s| &s.value));
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(shell)),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        seams.clock.clone(),
        RuntimeConfig::new(None, seams.jitter_seed),
    );
    let mut stamper = seams.stamper();
    let mut sink = VecSink::new();
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        runtime.run(&wf, &report, stamper.as_mut(), &mut sink),
    )
    .await
    .expect("the run settles")
    .expect("clean run");
    let stream = sink
        .into_events()
        .iter()
        .map(|event| serde_json::to_string(event).expect("event serializes"))
        .collect();
    (outcome, stream)
}

fn shell_two_ok() -> MockShell {
    MockShell::new().enqueue_ok("one\n").enqueue_ok("two\n")
}

/// (a) · the flagship: `entropy: seeded(42)` replays BYTE-identical.
#[tokio::test]
async fn seeded_runs_are_byte_identical() {
    const YAML: &str = "\
nika: seeded-replay
permits: { exec: [\"echo\"] }
run:
  entropy:
    seeded: 42
tasks:
  first:
    exec: { command: [\"echo\", \"one\"] }
  second:
    with: { prior: \"${{ tasks.first.output }}\" }
    exec: { command: [\"echo\", \"${{ with.prior }}\"] }
";
    let (outcome_a, stream_a) = run_stream(YAML, shell_two_ok()).await;
    let (outcome_b, stream_b) = run_stream(YAML, shell_two_ok()).await;
    assert!(outcome_a.ok && outcome_b.ok, "both runs settle green");
    assert_eq!(stream_a.len(), stream_b.len(), "same stream shape");
    assert_eq!(
        stream_a, stream_b,
        "two runs of the same seeded file = the same journal, byte for byte"
    );
    // The deterministic stamps are the seq-keyed ones (id 1 · t 10ms).
    assert!(
        stream_a
            .iter()
            .all(|line| !line.contains("\"timestamp\":0")),
        "every event is stamped (none at the zero instant): {stream_a:?}"
    );
}

/// (a′) · `entropy: none` — the strict lane replays too.
#[tokio::test]
async fn entropy_none_runs_are_byte_identical() {
    const YAML: &str = "\
nika: strict-replay
permits: { exec: [\"echo\"] }
run:
  entropy: none
tasks:
  only:
    exec: { command: [\"echo\", \"one\"] }
";
    let (_, stream_a) = run_stream(YAML, shell_two_ok()).await;
    let (_, stream_b) = run_stream(YAML, shell_two_ok()).await;
    assert_eq!(stream_a, stream_b, "the zero stream is replay-stable");
}

/// (b) · `seeded(42)` vs `seeded(43)` — the runs DIFFER, and only at the
/// jitter the seed keys (the retry's journaled `delay_ms`).
#[tokio::test]
async fn the_seed_is_where_the_entropy_lives() {
    const TEMPLATE: &str = "\
nika: seeded-divergence
permits: { exec: [\"flaky\"] }
run:
  entropy:
    seeded: SEED
tasks:
  flaky:
    exec: { command: [\"flaky\"] }
    retry:
      max_attempts: 2
      backoff_ms: 60000
      jitter: true
      on_codes: [\"NIKA-EXEC-001\"]
";
    let shell = || {
        MockShell::new()
            .enqueue_fail(9, "boom")
            .enqueue_ok("recovered\n")
    };
    let yaml42 = TEMPLATE.replace("SEED", "42");
    let yaml43 = TEMPLATE.replace("SEED", "43");
    let (outcome42, stream42) = run_stream(&yaml42, shell()).await;
    let (outcome43, stream43) = run_stream(&yaml43, shell()).await;
    assert!(outcome42.ok && outcome43.ok, "the retry recovers both runs");
    assert_eq!(stream42.len(), stream43.len(), "same events, same order");
    assert_ne!(stream42, stream43, "different seeds diverge");

    // The divergence is EXACTLY two lines — the boot manifest NAMING its
    // seed (F-P2: the journal self-describes its determinism contract)
    // and the jittered retry delay the seed keys. Every other event line
    // is byte-equal (stamps are seq-keyed, durations are virtual;
    // nothing else reads the seed).
    let differing: Vec<usize> = (0..stream42.len())
        .filter(|&i| stream42[i] != stream43[i])
        .collect();
    assert_eq!(
        differing.len(),
        2,
        "the manifest's seed claim + the jittered delay: {differing:?}"
    );
    let manifest = &stream42[differing[0]];
    assert!(
        manifest.contains(&format!(
            "\"kind\":\"{}\"",
            EventKind::WorkflowStarted.as_str()
        )),
        "the first divergence is the boot manifest's seed claim: {manifest}"
    );
    assert!(manifest.contains("\"seed\""), "{manifest}");
    let line = &stream42[differing[1]];
    assert!(
        line.contains(&format!(
            "\"kind\":\"{}\"",
            EventKind::TaskRetrying.as_str()
        )),
        "the second divergence IS the journaled retry delay: {line}"
    );
    assert!(line.contains("\"delay_ms\""), "{line}");
}

/// (d) · the regression guard: an absent `run:` block keeps the ambient
/// lane — two runs DIFFER (live `UUIDv7` ids), so (a)'s byte-identity is
/// a property of the declaration, never of the engine standing still.
#[tokio::test]
async fn absent_run_block_stays_the_ambient_status_quo() {
    const YAML: &str = "\
nika: ambient-default
permits: { exec: [\"echo\"] }
tasks:
  only:
    exec: { command: [\"echo\", \"one\"] }
";
    let (outcome_a, stream_a) = run_stream(YAML, shell_two_ok()).await;
    let (outcome_b, stream_b) = run_stream(YAML, shell_two_ok()).await;
    assert!(outcome_a.ok && outcome_b.ok, "the ambient lane still runs");
    assert_eq!(stream_a.len(), stream_b.len(), "same shape");
    assert_ne!(
        stream_a, stream_b,
        "ambient entropy: two live runs mint different ids (the status quo)"
    );
}
