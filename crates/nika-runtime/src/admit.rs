// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run-ADMISSION preflight (issue #603) — the missing-required-input
//! refusal.
//!
//! A `required: true` input with no declared `default:` has exactly one
//! other value source: the operator's `--var` override (F4). When neither
//! exists, `envelope_values` builds the `inputs` map WITHOUT the key and
//! the run used to learn about it mid-DAG — wave 1 already spent (infer
//! tasks burned tokens · exec tasks mutated) and the first
//! `${{ inputs.x }}` read died NIKA-VAR-001. The preflight moves the
//! refusal to ADMISSION: it fires before the prologue, so a refused run
//! emits zero events and spends zero tasks.
//!
//! ONE constructor, TWO surfaces (the trust-gate posture): the runtime
//! gate inside `Runtime::run` is the fail-closed word for every embedder;
//! the CLI's input gauntlet (the `--var` validation seam) surfaces the
//! SAME constructor natively — same predicate, same text, no drift.
//!
//! What satisfies a `required: true` input (either is enough):
//! - a declared `default:` (the author's value) ·
//! - a `--var <name>=<value>` override (the operator's value).
//!
//! A NON-required input is never refused here: declared optional means an
//! unbound read stays the read-time NIKA-VAR-001 (unchanged). `config:`,
//! `const:`, `secrets:` are untouched — the preflight reads `inputs:`
//! only.

use std::collections::BTreeMap;

use nika_schema::check::CheckReport;
use nika_schema::raw::RawWorkflow;
use nika_schema::types::VarDecl;
use serde_json::Value;

use crate::errors::RuntimeError;

/// The run's launch gates, in order: the report trust check
/// (audit-before-run) · the required-input preflight below — both
/// refuse BEFORE the prologue, so a refused run emits zero events and
/// spends zero tasks.
pub(crate) fn gates(
    wf: &RawWorkflow,
    report: &CheckReport,
    overrides: &BTreeMap<String, Value>,
) -> Result<(), RuntimeError> {
    crate::trust::check_report(wf, report)?;
    if let Some(err) = required_inputs_refusal(wf, overrides) {
        return Err(err);
    }
    Ok(())
}

/// The missing-required-input refusal — `Some` run-abort error when a
/// `required: true` input has neither a declared `default:` nor an
/// operator override, `None` when every required input is satisfied.
/// The ONE constructor both admission surfaces (the runtime's launch
/// gate · the CLI's input gauntlet) speak.
#[must_use]
pub fn required_inputs_refusal(
    wf: &RawWorkflow,
    overrides: &BTreeMap<String, Value>,
) -> Option<RuntimeError> {
    let missing: Vec<String> = wf
        .inputs
        .iter()
        .filter(|(key, decl)| {
            matches!(
                decl,
                VarDecl::Typed {
                    required: true,
                    default: None,
                    ..
                }
            ) && !overrides.contains_key(&key.value)
        })
        .map(|(key, _)| key.value.clone())
        .collect();
    if missing.is_empty() {
        return None;
    }
    let declared = wf.inputs.iter().map(|(key, _)| key.value.clone()).collect();
    Some(RuntimeError::MissingRequiredInputs { missing, declared })
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
    use serde_json::json;

    use super::*;
    use crate::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, VecSink};

    type MockRuntime = Runtime<
        MockShell,
        MockToolExecutor,
        nika_providers::NoHttp,
        MockProvider,
        MockToolDefinitionProvider,
        MockClock,
    >;

    fn runtime_with(shell: MockShell) -> MockRuntime {
        let executor = MockToolExecutor::new();
        let provider = MockProvider::new("mock");
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

    fn parse(yaml: &str) -> RawWorkflow {
        nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses")
    }

    /// One `exec` task inside its declared boundary (clean report) · the
    /// `inputs:` block varies per case. NOTHING reads the input — the
    /// preflight judges the DECLARATION ⊕ the overrides, never the reads.
    fn fixture(inputs: &str) -> String {
        format!(
            "nika: v1\nworkflow:\n  id: admit\n{inputs}permits: {{ exec: [\"true\"] }}\ntasks:\n  t:\n    exec: {{ command: [\"true\"] }}\n"
        )
    }

    async fn run(runtime: &MockRuntime, wf: &RawWorkflow) -> RunOutcome {
        let report = nika_schema::check(wf);
        assert!(report.is_clean(), "the fixture checks clean");
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        runtime
            .run(wf, &report, &mut stamper, &mut sink)
            .await
            .expect("run settles")
    }

    /// A refused run ABORTS at the preflight — the error, for inspection.
    /// The refusal precedes the prologue (fail-closed): the sink MUST
    /// stay empty (the trust-gate pin, mirrored once here).
    async fn run_refused(runtime: &MockRuntime, wf: &RawWorkflow) -> RuntimeError {
        let report = nika_schema::check(wf);
        assert!(report.is_clean(), "the fixture checks clean");
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        let err = runtime
            .run(wf, &report, &mut stamper, &mut sink)
            .await
            .expect_err("an unsatisfied required input never reaches dispatch");
        assert!(
            sink.events().is_empty(),
            "refused BEFORE any event — not even the prologue: {:?}",
            sink.events()
        );
        err
    }

    #[test]
    fn required_without_default_or_override_is_the_missing_set() {
        let wf = parse(&fixture(
            "inputs:\n  needle: { type: string, required: true }\n  region: { type: string, required: true }\n  limit: { type: integer, default: 3 }\n  note: { type: string }\n",
        ));
        let err = required_inputs_refusal(&wf, &BTreeMap::new()).expect("two unsatisfied");
        let RuntimeError::MissingRequiredInputs { missing, declared } = &err else {
            panic!("expected MissingRequiredInputs, got {err:?}");
        };
        // Declaration order — the author's own reading order.
        assert_eq!(missing, &["needle", "region"]);
        assert_eq!(declared, &["needle", "region", "limit", "note"]);
    }

    #[test]
    fn satisfied_cases_pass_the_predicate() {
        let no_override = BTreeMap::new();
        // A declared `default:` satisfies (required or not).
        let wf = parse(&fixture(
            "inputs:\n  needle: { type: string, required: true, default: \"x\" }\n",
        ));
        assert!(required_inputs_refusal(&wf, &no_override).is_none());
        // A `--var` override satisfies a default-less required input.
        let wf = parse(&fixture(
            "inputs:\n  needle: { type: string, required: true }\n",
        ));
        let overrides = BTreeMap::from([("needle".to_owned(), json!("ok"))]);
        assert!(required_inputs_refusal(&wf, &overrides).is_none());
        // A NON-required input without a default is unaffected (an unbound
        // OPTIONAL read stays the read-time NIKA-VAR-001).
        let wf = parse(&fixture("inputs:\n  note: { type: string }\n"));
        assert!(required_inputs_refusal(&wf, &no_override).is_none());
        // No `inputs:` block at all — nothing to refuse.
        let wf = parse(&fixture(""));
        assert!(required_inputs_refusal(&wf, &no_override).is_none());
    }

    /// (a) The #603 mechanism, refused: a default-less `required: true`
    /// input with no override ABORTS the run at admission — before one
    /// event, one task, one effect (the mock shell is the spend probe).
    #[tokio::test]
    async fn a_missing_required_input_is_refused_before_any_event() {
        let wf = parse(&fixture(
            "inputs:\n  needle: { type: string, required: true }\n",
        ));
        let shell = MockShell::new(); // EMPTY — any execution is a bug
        let probe = shell.clone();
        let runtime = runtime_with(shell);
        let err = run_refused(&runtime, &wf).await;
        assert_eq!(err.spec_code(), "NIKA-1708");
        let msg = err.to_string();
        assert!(msg.contains("`needle`"), "the input is named: {msg}");
        assert!(msg.contains("--var needle=<value>"), "the fix rides: {msg}");
        assert!(
            probe.executed_commands().is_empty(),
            "not one task spent: {:?}",
            probe.executed_commands()
        );
    }

    /// (b) The positive control: a `--var` override on the required input
    /// passes admission and the run completes (F4 — the override IS the
    /// input's value).
    #[tokio::test]
    async fn a_var_override_satisfies_the_admission_gate() {
        let wf = parse(&fixture(
            "inputs:\n  needle: { type: string, required: true }\n",
        ));
        let shell = MockShell::new().enqueue_ok("ok");
        let probe = shell.clone();
        let runtime = runtime_with(shell)
            .with_var_overrides(BTreeMap::from([("needle".to_owned(), json!("ok"))]));
        let outcome = run(&runtime, &wf).await;
        assert!(outcome.ok, "the satisfied gate is inert — no false refusal");
        assert_eq!(probe.executed_commands().len(), 1, "the exec ran");
    }

    /// (c) The author-side satisfaction: a declared `default:` passes
    /// admission with NO override.
    #[tokio::test]
    async fn a_declared_default_satisfies_the_admission_gate() {
        let wf = parse(&fixture(
            "inputs:\n  needle: { type: string, required: true, default: \"x\" }\n",
        ));
        let shell = MockShell::new().enqueue_ok("ok");
        let probe = shell.clone();
        let runtime = runtime_with(shell);
        let outcome = run(&runtime, &wf).await;
        assert!(outcome.ok, "a defaulted required input never refuses");
        assert_eq!(probe.executed_commands().len(), 1, "the exec ran");
    }
}
