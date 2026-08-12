// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic, clippy::unreachable)]

//! Boundary differential — the `permits.exec` boundary decided the same way by
//! BOTH enforcement paths, over the whole generated permit lattice.
//!
//! The proposition (spec 01 §permits · NIKA-SEC-004): an effect outside the
//! declared boundary is refused. Two independent implementations decide it —
//! the static checker (`nika-cap`/`permits_fit`, consumed by `nika check`) and
//! the runtime exec sink (`nika-runtime`, `check_exec_permits`). They share no
//! code, so a common bug would have to be born twice. This battery pins BOTH to
//! the SAME reference semantics, `Permits::allows_program`, so they agree:
//!
//! * **static side** — a LITERAL `command[0]` is statically checkable, so
//!   `check()` decides; `check`-accepts <=> `allows_program`.
//! * **runtime side** — an INTERPOLATED `command[0]` is opaque to the static
//!   ladder (the report is clean), so the runtime sink is the last gate;
//!   `runtime`-allows <=> `allows_program`.
//!
//! Composed: `check`-accepts <=> `allows_program` <=> `runtime`-allows. Any
//! divergence on either leg is a soundness (static laxer than runtime) or
//! completeness (static stricter) bug in the boundary.

use std::sync::Arc;

use nika_check::check;
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, VecSink};
use nika_schema::types::{ExecPermit, Permits};
use nika_schema::{FileId, ParseMode, parse};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;
use proptest::prelude::*;

// -- generators --------------------------------------------------------------
// Programs share a small alphabet so `Programs` lists produce a real allow/deny
// mix against the attempted program (`allows_program` is EXACT match, not glob).

fn prog() -> impl Strategy<Value = String> {
    prop_oneof!["git", "rm", "ls", "a", "b"].prop_map(String::from)
}

fn prog_list() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(prog(), 0..4)
}

fn arb_exec() -> impl Strategy<Value = Option<ExecPermit>> {
    prop_oneof![
        Just(None),
        Just(Some(ExecPermit::No)),
        Just(Some(ExecPermit::Any)),
        prog_list().prop_map(|l| Some(ExecPermit::Programs(l))),
    ]
}

/// exec-category ENABLED only (Any or Programs). The static ladder is clean
/// for these (the category is permitted; only the interpolated program is
/// opaque), so `runtime.run` proceeds and the exec sink is the real decider.
/// None/No are category-denied and refuse statically (covered by the static leg).
fn arb_exec_enabled() -> impl Strategy<Value = Option<ExecPermit>> {
    prop_oneof![
        Just(Some(ExecPermit::Any)),
        prog_list().prop_map(|l| Some(ExecPermit::Programs(l))),
    ]
}

/// The reference boundary both paths are pinned to.
fn permits_of(exec: Option<&ExecPermit>) -> Permits {
    let mut p = Permits::new();
    p.exec = exec.cloned();
    p
}

/// The `permits:` block for a generated `exec:` category (serde-exact form:
/// `false | true | [programs]`). `None` renders `{}` — the empty, pure-compute
/// boundary that default-denies exec. The wildcard is unreachable: `arb_exec`
/// only yields the four variants below.
fn permits_block(exec: Option<&ExecPermit>) -> String {
    match exec {
        None => "permits: {}".to_string(),
        Some(ExecPermit::No) => "permits:\n  exec: false".to_string(),
        Some(ExecPermit::Any) => "permits:\n  exec: true".to_string(),
        Some(ExecPermit::Programs(list)) => {
            let items = list
                .iter()
                .map(|p| format!("\"{p}\""))
                .collect::<Vec<_>>()
                .join(", ");
            format!("permits:\n  exec: [{items}]")
        }
        _ => unreachable!("arb_exec only yields None/No/Any/Programs"),
    }
}

// -- runtime driver (no cleanliness assert -- the differential needs dirty runs
//    to be possible; the interpolated form keeps the STATIC report clean) -----

async fn drive(yaml: &str, shell: MockShell) -> RunOutcome {
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
    let report = check(&wf);
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
        .expect("run completes")
}

proptest! {
    // -- static leg ----------------------------------------------------------
    // A LITERAL program is statically checkable; `check()` must accept exactly
    // when the reference boundary allows the program. `capability_escapes` with
    // `floor == false` is the NIKA-SEC-004 (declared-boundary) verdict; the
    // SSRF floor (SEC-005, `floor == true`) is a different law and filtered out.
    #[test]
    fn static_check_matches_allows_program(exec in arb_exec(), program in prog()) {
        let yaml = format!(
"nika: diff
{permits}
tasks:
  t:
    exec:
      command: [\"{program}\", \"--version\"]
",
            permits = permits_block(exec.as_ref()),
        );
        let wf = parse(&yaml, FileId::new(0), ParseMode::Strict).expect("parses");
        let report = check(&wf);
        let static_accepts = !report
            .capability_escapes
            .iter()
            .any(|e| !e.floor && e.task == "t");
        prop_assert_eq!(static_accepts, permits_of(exec.as_ref()).allows_program(&program));
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 96, ..ProptestConfig::default() })]

    // -- runtime leg ---------------------------------------------------------
    // An INTERPOLATED program is opaque to the static ladder (the report stays
    // clean), so the runtime exec sink is the last gate. It must allow exactly
    // when the reference boundary allows the resolved program. A denied program
    // fails the task with NIKA-SEC-004 before any spawn (the enqueued ok is
    // simply unused); an allowed program consumes it and succeeds.
    #[test]
    fn runtime_gate_matches_allows_program(exec in arb_exec_enabled(), program in prog()) {
        let yaml = format!(
"nika: diff
{permits}
const:
  prog: \"{program}\"
tasks:
  t:
    exec:
      command: [\"${{{{ const.prog }}}}\", \"--version\"]
",
            permits = permits_block(exec.as_ref()),
        );
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        let outcome = rt.block_on(drive(&yaml, MockShell::new().enqueue_ok("ok\n")));
        let runtime_allows = outcome.records["t"]
            .error
            .as_ref()
            .is_none_or(|e| e.code != "NIKA-SEC-004");
        prop_assert_eq!(runtime_allows, permits_of(exec.as_ref()).allows_program(&program));
    }
}
