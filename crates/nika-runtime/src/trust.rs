// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run-start report gate — audit-before-run, THREE clauses.
//!
//! 1. **Dirty** (spec §3 · NIKA-1700) — a dirty [`CheckReport`] never
//!    executes.
//! 2. **Binding** (F-P2 · NIKA-1707) — the judged-vs-booted semantic
//!    binding: a report STAMPED with the semantic hash of the workflow
//!    it judged (the CLI's load seam stamps it) must match the workflow
//!    the run is booting. A file edited after the check — same
//!    structure, a prompt string changed — re-keys the semantic hash,
//!    so its stale report refuses at boot; a COSMETIC edit (whitespace
//!    · a comment) re-keys nothing and never refuses (the semantic
//!    grain, exactly like resume). An UNSTAMPED report skips the clause
//!    (today's posture — the stamp is opt-in at the producer).
//! 3. **Trust** (NIKA-1707) — the audit's « honor system » backstop.
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
//! The trust clauses are NOT a second check for the CLI, which re-checks
//! right before run; they are the fail-closed word for every other driver.

use nika_check::CheckReport;
use nika_schema::raw::RawWorkflow;

use crate::errors::RuntimeError;

/// The audit-before-run gate, the three clauses, in order: a dirty report is
/// [`RuntimeError::DirtyReport`] (NIKA-1700) · a stamped report whose
/// judged semantic hash is not the workflow's boot hash is
/// [`RuntimeError::ReportMismatch`] (NIKA-1707 · F-P2) · a clean report whose
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
    // The judged-vs-booted binding (F-P2): the report names the semantic
    // hash of the workflow it judged — the run refuses when the workflow
    // it is BOOTING hashes differently (edited after the check, even
    // structure-preserving). An unprojectable workflow carries no boot
    // hash: the clause stays silent (no claim, never a guess).
    if let Some(judged) = report.workflow_semantic.as_deref()
        && let Some(booted) = crate::proof::ir::semantic_ir_hash(wf)
        && judged != booted.as_hex()
    {
        return Err(RuntimeError::ReportMismatch {
            detail: "the report was judged over different workflow bytes (semantic hash mismatch — the file changed after the check)"
                .to_owned(),
        });
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
    let trifecta = match nika_check::analyze(wf) {
        Ok(a) => nika_check::trifecta::scan_trifecta(wf, &a.edges, &a.topo_waves),
        Err(_) => Vec::new(),
    };
    let escapes = nika_check::permits_fit::scan_escapes(wf);
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
    const TRIFECTA: &str = "nika: tri\npermits: { fs: { read: [\"./inbox/**\"], write: [\"./out/**\"] }, net: { http: [\"api.example.com\"] }, tools: [\"nika:fetch\", \"nika:write\"] }\ntasks:\n  fetch_page:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.example.com/data\" } }\n  leak:\n    after: { fetch_page: success }\n    with: { body: \"${{ tasks.fetch_page.output }}\" }\n    invoke: { tool: \"nika:write\", args: { path: \"./out/leak.txt\", content: \"${{ with.body }}\" } }\n";

    /// The hand-built clean report: `check` refuses the REAL workflow, so
    /// the run is fed the CLEAN report of its wide-boundary twin — same
    /// tasks, same waves (the `permits:` line swaps away). F-O8: the twin
    /// is WIDE, not permit-free (absent = zero authority now) — and it
    /// drops the private-read leg so the trifecta never completes.
    fn forged_clean_report(yaml: &str) -> (nika_schema::raw::RawWorkflow, nika_check::CheckReport) {
        let wf = parse(yaml);
        assert!(
            !nika_check::check(&wf).is_clean(),
            "the honest check refuses the real workflow (the static half)"
        );
        let twin_yaml = yaml
            .lines()
            .map(|line| {
                if line.starts_with("permits:") {
                    "permits: { fs: { write: [\"./out/**\"] }, net: { http: [\"api.example.com\"] }, tools: [\"nika:fetch\", \"nika:write\"] }"
                } else {
                    line
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let report = nika_check::check(&parse(&twin_yaml));
        assert!(
            report.is_clean(),
            "the wide-boundary twin checks clean (same tasks → same waves)"
        );
        (wf, report)
    }

    async fn run(
        runtime: &MockRuntime,
        wf: &nika_schema::raw::RawWorkflow,
        report: &nika_check::CheckReport,
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
        report: &nika_check::CheckReport,
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
        let analyzed = nika_check::analyze(&wf).expect("the DAG derives");
        let trifecta =
            nika_check::trifecta::scan_trifecta(&wf, &analyzed.edges, &analyzed.topo_waves);
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
        let report = nika_check::check(&wf);
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
            "nika: honest\npermits: { exec: [\"echo\"] }\ntasks:\n  t:\n    exec: { command: [\"echo\", \"hi\"] }\n",
        );
        let report = nika_check::check(&wf);
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

    /// (c) No `permits:` block → the 1707 re-derivation is skipped (that
    /// gate is the declared-block twin), and F-O8's ZERO-AUTHORITY seam
    /// speaks instead: a forged clean report over an effect workflow
    /// still dies at dispatch — the tool gate refuses NIKA-SEC-004
    /// before any call (the defense-in-depth filet under the check).
    /// The honest report is dirty the same way (NIKA-AUTH-006 · a floor
    /// escape OR an undeclared one), so both refusal classes hold.
    #[tokio::test]
    async fn no_declared_boundary_skips_the_gate_but_the_seam_refuses() {
        let yaml = "nika: floor\ntasks:\n  t:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.example.com/x\" } }\n";
        let wf = parse(yaml);
        assert!(
            !nika_check::check(&wf).is_clean(),
            "F-O8: absent + an effect dirties the honest report (NIKA-AUTH-006)"
        );
        // The forged clean report: a pure-compute twin (no effects →
        // the LEGAL zero · clean) — the hand-built-report shape an
        // embedder feeds the run.
        let twin = parse(
            "nika: floor\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: \"hi\", max_tokens: 5 }\n",
        );
        let report = nika_check::check(&twin);
        assert!(report.is_clean(), "the pure-compute twin checks clean");
        let executor = MockToolExecutor::new(); // EMPTY — any call is a bug
        let probe = executor.clone();
        let runtime = runtime_with(MockShell::new(), executor, MockProvider::new("mock"));
        let outcome = run(&runtime, &wf, &report).await;
        assert!(!outcome.ok, "the seam refuses the zero-authority fetch");
        let err = outcome.records["t"].error.as_ref().expect("recorded");
        assert_eq!(err.code, "NIKA-SEC-004", "the dispatch seam speaks (F-O8)");
        assert!(
            err.message.contains("zero authority"),
            "the refusal names the law: {}",
            err.message
        );
        assert!(
            probe.captured_calls().is_empty(),
            "the refused tool NEVER executed"
        );
    }

    /// (F-P2 · the judged-vs-booted binding) A report STAMPED with a
    /// different semantic hash refuses at boot — before one event: the
    /// file changed after the check, even structure-preserving.
    #[tokio::test]
    async fn a_stamped_report_over_different_bytes_refuses_at_boot() {
        let wf = parse(
            "nika: stale\npermits: { exec: [\"echo\"] }\ntasks:\n  t:\n    exec: { command: [\"echo\", \"hi\"] }\n",
        );
        let mut report = nika_check::check(&wf);
        assert!(report.is_clean(), "the fixture fits its boundary");
        report.workflow_semantic = Some("0".repeat(64));
        let runtime = runtime_with(
            MockShell::new(),
            MockToolExecutor::new(),
            MockProvider::new("mock"),
        );
        let err = run_refused(&runtime, &wf, &report).await;
        let crate::RuntimeError::ReportMismatch { detail } = &err else {
            panic!("expected ReportMismatch, got {err:?}");
        };
        assert_eq!(err.spec_code(), "NIKA-1707");
        assert!(
            detail.contains("semantic hash mismatch"),
            "the refusal names the binding: {detail}"
        );
    }

    /// The binding's positive control + the semantic grain: a report
    /// stamped with the workflow's OWN hash runs (no false refusal),
    /// and a cosmetic twin (a comment) re-keys nothing — the SAME stamp
    /// still vouches, exactly the resume grain. The unstamped-skip half
    /// is the (b′) test above: `check()` produces `workflow_semantic:
    /// None` and that report passes the gate untouched.
    #[tokio::test]
    async fn a_stamped_matching_report_passes_and_the_grain_is_semantic() {
        let yaml = "nika: honest\npermits: { exec: [\"echo\"] }\ntasks:\n  t:\n    exec: { command: [\"echo\", \"hi\"] }\n";
        let wf = parse(yaml);
        let mut report = nika_check::check(&wf);
        let stamp = crate::proof::ir::semantic_ir_hash(&wf)
            .expect("the fixture projects")
            .as_hex()
            .to_owned();
        report.workflow_semantic = Some(stamp.clone());
        let shell = MockShell::new().enqueue_ok("ok");
        let runtime = runtime_with(shell, MockToolExecutor::new(), MockProvider::new("mock"));
        let outcome = run(&runtime, &wf, &report).await;
        assert!(outcome.ok, "the stamped match vouches — no false refusal");
        // The cosmetic twin: a comment changes the bytes, never the
        // semantic hash — the SAME stamp would still match.
        let twin = parse(&format!("# a cosmetic comment\n{yaml}"));
        let twin_stamp = crate::proof::ir::semantic_ir_hash(&twin)
            .expect("the twin projects")
            .as_hex()
            .to_owned();
        assert_eq!(stamp, twin_stamp, "the grain is semantic, never bytes");
    }
}
