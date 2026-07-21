// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run-start report gate — audit-before-run, TWO clauses.
//!
//! 1. **Dirty** (spec §3 · NIKA-1700) — a dirty [`CheckReport`] never
//!    executes.
//! 2. **Trust** (NIKA-1707) — the audit's « honor system » backstop.
//!    Nothing BINDS a clean report to the workflow bytes: a library
//!    embedder can hand `run` a hand-built clean one (a skipped check
//!    and a forged report are indistinguishable). Under a declared
//!    `permits:` block the gate re-derives the PURE boundary subset —
//!    permits-fit + trifecta, the cheap L0 lanes, never the whole check
//!    ladder — and refuses when it disagrees with the report's lanes:
//!    the report does not describe this workflow. Without a declared
//!    boundary there is nothing to verify (today's posture · the SSRF
//!    floor has its own live enforcement in the effect path) and the
//!    clause costs nothing — it never runs.
//!
//! The trust clause is NOT a second check for the CLI, which re-checks
//! right before run; it is the fail-closed word for every other driver.

use nika_schema::check::CheckReport;
use nika_schema::raw::RawWorkflow;

use crate::errors::RuntimeError;

/// The audit-before-run gate, both clauses, in order: a dirty report is
/// [`RuntimeError::DirtyReport`] (NIKA-1700) · a clean report whose
/// boundary lanes the workflow's own re-derivation contradicts is
/// [`RuntimeError::ReportMismatch`] (NIKA-1707) — `Ok` only when the
/// report may vouch for THESE bytes.
pub(crate) fn check_report(wf: &RawWorkflow, report: &CheckReport) -> Result<(), RuntimeError> {
    // Bound the named list — a fully-escaping workflow must not bomb
    // the diagnostic (the full lanes are one `nika check` away).
    const MAX_NAMED: usize = 8;
    if !report.is_clean() {
        return Err(RuntimeError::DirtyReport);
    }
    if wf.permits.is_none() {
        return Ok(());
    }
    // The two PURE boundary lanes of `nika check`, re-derived WITHOUT the
    // rest of the ladder — same input, same seams, so an honest report's
    // lanes equal these by construction. The checker's own gating,
    // mirrored one-for-one: the trifecta lane reads the derived graph (a
    // valid DAG or no claim — skipped, never wrong) · permits-fit is
    // DAG-independent and runs unconditionally.
    let trifecta = match nika_schema::analyze(wf) {
        Ok(a) => nika_schema::check::trifecta::scan_trifecta(wf, &a.edges, &a.topo_waves),
        Err(_) => Vec::new(),
    };
    let escapes = nika_schema::check::permits_fit::scan_escapes(wf);
    // Post-`is_clean` the report's boundary lanes are empty, so an
    // inequality here always means the re-derivation found something the
    // report does not know. The comparison stays symmetric anyway — one
    // truth: the lanes must EQUAL, and gate order is not the proof.
    if escapes == report.capability_escapes && trifecta == report.trifecta_findings {
        return Ok(());
    }
    let total = escapes.len() + trifecta.len();
    let mut named: Vec<String> = escapes
        .iter()
        .map(|e| format!("capability escape · task `{}`", e.task))
        .chain(
            trifecta
                .iter()
                .map(|t| format!("trifecta · task `{}`", t.task)),
        )
        .take(MAX_NAMED)
        .collect();
    if total > named.len() {
        named.push(format!("…and {} more", total - named.len()));
    }
    Err(RuntimeError::ReportMismatch {
        detail: named.join(" · "),
    })
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::sync::Arc;

    use nika_kernel::tool_executor::ToolResult;
    use nika_kernel_mock::{
        MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
    };
    use nika_verb_agent::AgentVerb;
    use nika_verb_exec::ExecVerb;
    use nika_verb_infer::InferVerb;
    use nika_verb_invoke::InvokeVerb;

    use crate::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, VecSink};

    type MockRuntime = Runtime<
        MockShell,
        MockToolExecutor,
        nika_providers::NoHttp,
        MockProvider,
        MockToolDefinitionProvider,
        MockClock,
    >;

    fn runtime_with(
        shell: MockShell,
        executor: MockToolExecutor,
        provider: MockProvider,
    ) -> MockRuntime {
        let invoke = Arc::new(InvokeVerb::new(Arc::new(executor)));
        Runtime::new(
            ExecVerb::new(Arc::new(shell)),
            Arc::clone(&invoke),
            InferVerb::new(
                Arc::new(nika_providers::ProviderRegistry::without_http(
                    nika_providers::ProvidersConfig::new(),
                )),
                "mock/echo",
            ),
            AgentVerb::new(
                Arc::new(provider),
                invoke,
                Arc::new(MockToolDefinitionProvider::new()),
                "mock/echo",
            ),
            MockClock::new(),
            RuntimeConfig::default(),
        )
    }

    fn parse(yaml: &str) -> nika_schema::raw::RawWorkflow {
        nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses")
    }

    /// The NEP-0002 fixture (one-line `permits:` — the twin is a
    /// line-strip away): all three legs declared, the realized flow
    /// `fetch_page → leak`, no blocking `nika:prompt` gate → the static
    /// lane names `leak` (NIKA-SEC-009).
    const TRIFECTA: &str = "nika: v1\nworkflow:\n  id: tri\npermits: { fs: { read: [\"./inbox/**\"], write: [\"./out/**\"] }, net: { http: [\"api.example.com\"] }, tools: [\"nika:fetch\", \"nika:write\"] }\ntasks:\n  fetch_page:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.example.com/data\" } }\n  leak:\n    after: { fetch_page: success }\n    with: { body: \"${{ tasks.fetch_page.output }}\" }\n    invoke: { tool: \"nika:write\", args: { path: \"./out/leak.txt\", content: \"${{ with.body }}\" } }\n";

    /// The hand-built clean report: `check` refuses the REAL workflow, so
    /// the run is fed the CLEAN report of its permit-free twin — same
    /// tasks, same waves (the `permits:` line strips away).
    fn forged_clean_report(
        yaml: &str,
    ) -> (nika_schema::raw::RawWorkflow, nika_schema::CheckReport) {
        let wf = parse(yaml);
        assert!(
            !nika_schema::check(&wf).is_clean(),
            "the honest check refuses the real workflow (the static half)"
        );
        let twin_yaml = yaml
            .lines()
            .filter(|line| !line.starts_with("permits:"))
            .collect::<Vec<_>>()
            .join("\n");
        let report = nika_schema::check(&parse(&twin_yaml));
        assert!(
            report.is_clean(),
            "the permit-free twin checks clean (same tasks → same waves)"
        );
        (wf, report)
    }

    async fn run(
        runtime: &MockRuntime,
        wf: &nika_schema::raw::RawWorkflow,
        report: &nika_schema::CheckReport,
    ) -> RunOutcome {
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        runtime
            .run(wf, report, &mut stamper, &mut sink)
            .await
            .expect("run settles")
    }

    /// A refused run ABORTS at a gate — the error, for inspection. Both
    /// gates precede the prologue (fail-closed): the sink MUST stay
    /// empty, pinned here once for every caller.
    async fn run_refused(
        runtime: &MockRuntime,
        wf: &nika_schema::raw::RawWorkflow,
        report: &nika_schema::CheckReport,
    ) -> crate::RuntimeError {
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        let err = runtime
            .run(wf, report, &mut stamper, &mut sink)
            .await
            .expect_err("a refused report never reaches dispatch");
        assert!(
            sink.events().is_empty(),
            "refused BEFORE any event — not even the prologue: {:?}",
            sink.events()
        );
        err
    }

    /// (a) A trifecta-complete boundary (the realized flow → NIKA-SEC-009)
    /// under a HAND-BUILT clean report: the run re-derives the lane from
    /// the bytes, finds `leak` the report does not know, and refuses —
    /// before one event, one tool call.
    #[tokio::test]
    async fn trifecta_complete_is_unmasked_at_the_trust_gate() {
        let (wf, report) = forged_clean_report(TRIFECTA);
        // The re-derivation itself, run the way the gate runs it: the
        // trifecta lane over the workflow's own graph names the sink.
        let analyzed = nika_schema::analyze(&wf).expect("the DAG derives");
        let trifecta =
            nika_schema::check::trifecta::scan_trifecta(&wf, &analyzed.edges, &analyzed.topo_waves);
        assert_eq!(
            trifecta.len(),
            1,
            "the re-derivation sees the realized sink: {trifecta:?}"
        );
        let executor = MockToolExecutor::new(); // EMPTY — any call is a bug
        let probe = executor.clone();
        let runtime = runtime_with(MockShell::new(), executor, MockProvider::new("mock"));
        let err = run_refused(&runtime, &wf, &report).await;
        let crate::RuntimeError::ReportMismatch { detail } = &err else {
            panic!("expected ReportMismatch, got {err:?}");
        };
        assert_eq!(err.spec_code(), "NIKA-1707");
        assert_eq!(detail, "trifecta · task `leak`");
        assert!(
            err.to_string().contains("re-check the file"),
            "the refusal teaches the repair: {err}"
        );
        assert!(
            probe.captured_calls().is_empty(),
            "not one tool call left the gate"
        );
    }

    /// (b) The SAME workflow with its REAL report: the report is dirty
    /// (it carries the SEC-009), so the refusal is the dirty-report class
    /// — the 1700 gate fires first, the re-derivation never runs.
    #[tokio::test]
    async fn trifecta_complete_with_the_real_report_is_the_dirty_twin() {
        let wf = parse(TRIFECTA);
        let report = nika_schema::check(&wf);
        assert!(!report.is_clean(), "the honest report carries the SEC-009");
        let runtime = runtime_with(
            MockShell::new(),
            MockToolExecutor::new(),
            MockProvider::new("mock"),
        );
        let err = run_refused(&runtime, &wf, &report).await;
        assert!(
            matches!(err, crate::RuntimeError::DirtyReport),
            "the dirty class precedes the re-derivation: {err:?}"
        );
        assert_eq!(err.spec_code(), "NIKA-1700");
    }

    /// (b′) The positive control: an honest clean report over a
    /// boundary-fitting workflow — the re-derivation AGREES, the gate is
    /// inert, the run proceeds. (No false refusal on the CLI's shape.)
    #[tokio::test]
    async fn an_honest_clean_report_passes_the_gate_and_runs() {
        let wf = parse(
            "nika: v1\nworkflow:\n  id: honest\npermits: { exec: [\"echo\"] }\ntasks:\n  t:\n    exec: { command: [\"echo\", \"hi\"] }\n",
        );
        let report = nika_schema::check(&wf);
        assert!(report.is_clean(), "the fixture fits its boundary");
        let shell = MockShell::new().enqueue_ok("ok");
        let probe = shell.clone();
        let runtime = runtime_with(shell, MockToolExecutor::new(), MockProvider::new("mock"));
        let outcome = run(&runtime, &wf, &report).await;
        assert!(
            outcome.ok,
            "the re-derivation agrees with the honest report — no false refusal"
        );
        assert_eq!(probe.executed_commands().len(), 1, "the exec ran");
    }

    /// (c) No `permits:` block → the gate is skipped WHOLESALE. The proof
    /// is behavioral: a literal loopback fetch is a FLOOR escape
    /// (NIKA-SEC-005) the static lane flags even with no boundary
    /// declared, so the honest report is dirty and the run is fed the
    /// clean report of its public-host twin. Were the gate to run, the
    /// re-derivation would flag the floor escape and abort NIKA-1707 —
    /// instead the run proceeds (today's posture unchanged: the floor's
    /// LIVE enforcement in the effect path owns that world, never the
    /// report).
    #[tokio::test]
    async fn no_declared_boundary_skips_the_gate_wholesale() {
        let yaml = "nika: v1\nworkflow:\n  id: floor\ntasks:\n  t:\n    invoke: { tool: \"nika:fetch\", args: { url: \"http://127.0.0.1/x\" } }\n";
        let wf = parse(yaml);
        assert!(
            !nika_schema::check(&wf).is_clean(),
            "the floor escape dirties the honest report even without permits"
        );
        let twin = parse(&yaml.replace("http://127.0.0.1/x", "https://api.example.com/x"));
        let report = nika_schema::check(&twin);
        assert!(report.is_clean(), "the public-host twin checks clean");
        let executor = MockToolExecutor::new().enqueue_ok(ToolResult::success("t1", "page"));
        let probe = executor.clone();
        let runtime = runtime_with(MockShell::new(), executor, MockProvider::new("mock"));
        let outcome = run(&runtime, &wf, &report).await;
        assert!(
            outcome.ok,
            "no permits → the gate never runs (the re-derivation costs nothing here)"
        );
        assert_eq!(probe.captured_calls().len(), 1, "the fetch executed");
    }
}
