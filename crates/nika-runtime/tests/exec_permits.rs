// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! `permits.exec` RUNTIME enforcement (spec 01 §permits · NIKA-SEC-004).
//!
//! The static `nika check` (`permits_fit`) refuses statically-detectable
//! escapes; this battery covers what only the runtime can decide — an
//! INTERPOLATED `command[0]` that resolves to a program OUTSIDE the declared
//! allowlist. `permits_fit`'s `static_program` returns `None` for an
//! interpolated leading token, so the workflow passes the check and the exec
//! sink is the last gate.

use std::sync::Arc;

use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, TaskStatus, VecSink};
use nika_schema::{FileId, ParseMode, check, parse};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

async fn run(yaml: &str, shell: MockShell) -> RunOutcome {
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
    let report = check(&wf);
    assert!(
        report.is_clean(),
        "fixture passes the static ladder (the interpolated program is not statically checkable)"
    );
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
        MockClock::new(),
        RuntimeConfig::default(),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean run")
}

/// A program resolved from `${{ }}` that lands OUTSIDE `permits.exec` is
/// refused at the sink with NIKA-SEC-004 — the runner is never reached.
#[tokio::test]
async fn interpolated_program_outside_allowlist_is_refused() {
    let yaml = r#"
nika: v1
workflow:
  id: permits-deny
permits:
  exec: ["git"]
const:
  prog: "rm"
tasks:
  danger:
    exec:
      command: ["${{ const.prog }}", "-rf", "/tmp/x"]
"#;
    // The MockShell is EMPTY — the security gate must fire before any spawn
    // (an `enqueue`-less mock would panic if `run` were ever called).
    let outcome = run(yaml, MockShell::new()).await;
    assert!(!outcome.ok, "a denied capability fails the run");
    let rec = &outcome.records["danger"];
    assert_eq!(rec.status, TaskStatus::Failure);
    assert_eq!(
        rec.error.as_ref().expect("security error record").code,
        "NIKA-SEC-004"
    );
}

/// A program resolved from `${{ }}` that IS in `permits.exec` runs normally.
#[tokio::test]
async fn interpolated_program_in_allowlist_runs() {
    let yaml = r#"
nika: v1
workflow:
  id: permits-allow
permits:
  exec: ["git"]
const:
  prog: "git"
tasks:
  ok:
    exec:
      command: ["${{ const.prog }}", "status"]
"#;
    let outcome = run(yaml, MockShell::new().enqueue_ok("clean\n")).await;
    assert!(outcome.ok, "an allowed program runs to success");
    assert_eq!(outcome.records["ok"].status, TaskStatus::Success);
}
