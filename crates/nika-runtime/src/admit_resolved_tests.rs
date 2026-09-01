// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! W0-D-R1 / issue 1297: the resolved-id budget walk. Split from
//! `admit.rs` so that file stays under the 1,500-LOC wall.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeMap;
use std::sync::Arc;

use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_schema::raw::RawWorkflow;
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;
use serde_json::Value;

use crate::admit::{budget_floor_refusal, gates, unpriced_cloud_seat};
use crate::{DeterministicStamper, Runtime, RuntimeConfig, RuntimeError, VecSink};

type MockRuntime = Runtime<
    MockShell,
    MockToolExecutor,
    nika_providers::NoHttp,
    MockProvider,
    MockToolDefinitionProvider,
    MockClock,
>;

fn parse(yaml: &str) -> RawWorkflow {
    nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .unwrap_or_else(|_| panic!("fixture must parse"))
}

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

async fn run_refused(runtime: &MockRuntime, wf: &RawWorkflow) -> RuntimeError {
    let report = nika_check::check(wf);
    assert!(report.is_clean(), "the fixture checks clean");
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let err = runtime
        .run(wf, &report, &mut stamper, &mut sink)
        .await
        .err()
        .unwrap_or_else(|| panic!("a refused run must never reach dispatch"));
    assert!(
        sink.events().is_empty(),
        "refusal must happen before any event, including the prologue"
    );
    err
}

/// W0-D-R1 / issue 1297: `infer.model: ${{ inputs.model }}` is a
/// run-time seat (the MODELS rung leaves it unjudged). The static
/// report has no unpriced-cloud endpoint — that is the door the
/// first W0-D walk left open.
fn runtime_model_input_wf() -> String {
    "nika: b20-var\ninputs:\n  model: { type: string, required: true }\npermits: {}\ntasks:\n  ping:\n    infer: { prompt: \"PONG\", max_tokens: 16, model: \"${{ inputs.model }}\" }\n"
        .to_owned()
}

fn canary_override() -> BTreeMap<String, Value> {
    BTreeMap::from([(
        "model".to_owned(),
        Value::String("gemini/nika-b20-unpriced-canary".to_owned()),
    )])
}

#[test]
fn static_report_misses_a_var_resolved_unpriced_cloud_seat() {
    let wf = parse(&runtime_model_input_wf());
    let report = nika_check::check(&wf);
    assert!(
        report.is_clean(),
        "unjudged run-time model must not be dirty"
    );
    assert!(
        report.data_journey.model_endpoints.is_empty(),
        "without a default, check must not name the seat"
    );
    assert!(
        budget_floor_refusal(&wf, &report, Some(0.20), None).is_none(),
        "the static-only door: priced:false is not on the report, so \
         budget_floor_refusal(…, None) would start"
    );
}

#[test]
fn var_resolved_unpriced_cloud_plus_cap_refuses_to_start() {
    let wf = parse(&runtime_model_input_wf());
    let report = nika_check::check(&wf);
    let err = gates(
        &wf,
        &report,
        &canary_override(),
        Some(0.20),
        None,
        None,
        &[],
    )
    .expect_err("resolved gemini canary + $0.20 must NIKA-1709");
    assert_eq!(err.spec_code(), "NIKA-1709");
    let msg = err.to_string();
    assert!(msg.contains("unpriced"));
    assert!(msg.contains("nika-b20-unpriced-canary"));
    assert!(msg.contains("0.200000"), "cap must ride");
    assert!(
        gates(&wf, &report, &canary_override(), None, None, None, &[]).is_ok(),
        "no cap → unpriced cloud may still start"
    );
}

#[test]
fn priced_envelope_cel_overridden_to_unpriced_cloud_refuses() {
    let wf = parse(
        "nika: b20-env\nmodel: \"${{ inputs.seat }}\"\ninputs:\n  seat: { type: string, required: true, default: \"gemini/gemini-2.5-flash\" }\npermits: {}\ntasks:\n  ping:\n    infer: { prompt: \"PONG\", max_tokens: 16 }\n",
    );
    let report = nika_check::check(&wf);
    let ep = report
        .data_journey
        .model_endpoints
        .iter()
        .find(|e| e.task == "ping")
        .expect("check sees the priced default");
    assert!(ep.priced, "static door must look priced");
    assert!(
        budget_floor_refusal(&wf, &report, Some(0.20), None).is_none(),
        "check JSON looks priced — the static walk admits"
    );
    let overrides = BTreeMap::from([(
        "seat".to_owned(),
        Value::String("gemini/nika-b20-unpriced-canary".to_owned()),
    )]);
    let err = gates(&wf, &report, &overrides, Some(0.20), None, None, &[])
        .expect_err("live --var seat is the unpriced canary");
    assert_eq!(err.spec_code(), "NIKA-1709");
    assert!(err.to_string().contains("nika-b20-unpriced-canary"));
}

#[test]
fn cli_model_override_to_unpriced_cloud_refuses() {
    let wf = parse(
        "nika: b20-cli\nmodel: gemini/gemini-2.5-flash\npermits: {}\ntasks:\n  ping:\n    infer: { prompt: \"PONG\", max_tokens: 16 }\n",
    );
    let report = nika_check::check(&wf);
    assert!(
        report
            .data_journey
            .model_endpoints
            .iter()
            .any(|e| e.priced && e.model.contains("gemini-2.5-flash")),
        "file must be priced flash"
    );
    let err = budget_floor_refusal(
        &wf,
        &report,
        Some(0.20),
        Some("gemini/nika-b20-unpriced-canary"),
    )
    .expect("CLI --model to the canary must refuse under $0.20");
    assert_eq!(err.spec_code(), "NIKA-1709");
    assert!(err.to_string().contains("nika-b20-unpriced-canary"));
}

#[test]
fn unpriced_cloud_seat_is_gemini_canary_not_mock_or_flash() {
    assert!(
        unpriced_cloud_seat("gemini/nika-b20-unpriced-canary"),
        "the canary is the unpriced-cloud class"
    );
    assert!(!unpriced_cloud_seat("gemini/gemini-2.5-flash"));
    assert!(!unpriced_cloud_seat("mock/echo"));
    assert!(!unpriced_cloud_seat("ollama/llama3"));
    assert!(
        !unpriced_cloud_seat("not-a-provider/ghost"),
        "unknown is not promoted to cloud"
    );
}

#[test]
fn var_resolved_mock_plus_cap_still_admits() {
    let wf = parse(&runtime_model_input_wf());
    let report = nika_check::check(&wf);
    let overrides = BTreeMap::from([("model".to_owned(), Value::String("mock/echo".to_owned()))]);
    assert!(
        gates(&wf, &report, &overrides, Some(0.20), None, None, &[]).is_ok(),
        "mock is a proven zero — a cap must not refuse the rehearsal"
    );
}

#[tokio::test]
async fn var_resolved_unpriced_cloud_under_cap_refuses_before_any_infer() {
    let wf = parse(&runtime_model_input_wf());
    let runtime = runtime_with(MockShell::new())
        .with_max_cost_usd(Some(0.20))
        .with_var_overrides(canary_override());
    let err = run_refused(&runtime, &wf).await;
    assert_eq!(err.spec_code(), "NIKA-1709");
    assert!(err.to_string().contains("nika-b20-unpriced-canary"));
}

/// N01 / issue 1319: CEL index `${{ inputs['model'] }}` is the same
/// binding as the dotted form — a cap must refuse the unpriced canary
/// before infer HTTP.
#[test]
fn cel_index_unpriced_cloud_plus_cap_refuses_to_start() {
    let wf = parse(
        "nika: n01-idx\ninputs:\n  model: { type: string, required: true }\npermits: {}\ntasks:\n  ping:\n    infer: { prompt: \"PONG\", max_tokens: 16, model: \"${{ inputs['model'] }}\" }\n",
    );
    let report = nika_check::check(&wf);
    let err = gates(
        &wf,
        &report,
        &canary_override(),
        Some(0.20),
        None,
        None,
        &[],
    )
    .expect_err("inputs['model'] canary + $0.20 must NIKA-1709");
    assert_eq!(err.spec_code(), "NIKA-1709");
    assert!(err.to_string().contains("nika-b20-unpriced-canary"));
}

/// N01: `with.model: ${{ inputs.model }}` then `infer.model: ${{ with.model }}`.
#[test]
fn with_alias_unpriced_cloud_plus_cap_refuses_to_start() {
    let wf = parse(
        "nika: n01-with\ninputs:\n  model: { type: string, required: true }\npermits: {}\ntasks:\n  ping:\n    with: { model: \"${{ inputs.model }}\" }\n    infer: { prompt: \"PONG\", max_tokens: 16, model: \"${{ with.model }}\" }\n",
    );
    let report = nika_check::check(&wf);
    let err = gates(
        &wf,
        &report,
        &canary_override(),
        Some(0.20),
        None,
        None,
        &[],
    )
    .expect_err("with.model alias + $0.20 must NIKA-1709");
    assert_eq!(err.spec_code(), "NIKA-1709");
    assert!(err.to_string().contains("nika-b20-unpriced-canary"));
}

/// N01: concat `${{ inputs.provider }}/${{ inputs.name }}`.
#[test]
fn concat_provider_name_unpriced_cloud_plus_cap_refuses_to_start() {
    let wf = parse(
        "nika: n01-cat\ninputs:\n  provider: { type: string, required: true }\n  name: { type: string, required: true }\npermits: {}\ntasks:\n  ping:\n    infer: { prompt: \"PONG\", max_tokens: 16, model: \"${{ inputs.provider }}/${{ inputs.name }}\" }\n",
    );
    let report = nika_check::check(&wf);
    let overrides = BTreeMap::from([
        ("provider".to_owned(), Value::String("gemini".to_owned())),
        (
            "name".to_owned(),
            Value::String("nika-b20-unpriced-canary".to_owned()),
        ),
    ]);
    let err = gates(&wf, &report, &overrides, Some(0.20), None, None, &[])
        .expect_err("concat seat + $0.20 must NIKA-1709");
    assert_eq!(err.spec_code(), "NIKA-1709");
    assert!(err.to_string().contains("nika-b20-unpriced-canary"));
}

/// N01 sparing arm: mock through the index form still runs under a cap.
#[test]
fn cel_index_mock_plus_cap_still_admits() {
    let wf = parse(
        "nika: n01-mock\ninputs:\n  model: { type: string, required: true }\npermits: {}\ntasks:\n  ping:\n    infer: { prompt: \"PONG\", max_tokens: 16, model: \"${{ inputs['model'] }}\" }\n",
    );
    let report = nika_check::check(&wf);
    let overrides = BTreeMap::from([("model".to_owned(), Value::String("mock/echo".to_owned()))]);
    assert!(
        gates(&wf, &report, &overrides, Some(0.20), None, None, &[]).is_ok(),
        "mock through the index form is still a proven zero"
    );
}

// ---------------------------------------------------------------------
// Issue 1368 · the uncataloged-id budget hole. An id the ONE resolver
// (`nika_providers::resolve_refusal` — the MODELS rung's own predicate)
// refuses used to skip BOTH budget arms: `unpriced_cloud_seat` spares an
// unknown provider by construction, so a bare/unknown id floored at $0
// and ANY `--max-cost-usd` passed. The run then died at dispatch — after
// earlier tasks may have spent — or, under `on_error: { skip: true }`,
// « succeeded » with the cap silently disarmed.
// ---------------------------------------------------------------------

/// The gauntlet's exact shape (#1368): the DOT form `claude-opus-4.1` is
/// not a catalog id (the catalog's row is the DASH `claude-opus-4-1`).
fn gauntlet_wf(model: &str) -> String {
    format!(
        "nika: g1368\nmodel: \"{model}\"\npermits: {{}}\ntasks:\n  ping:\n    infer: {{ prompt: \"PONG\", max_tokens: 768 }}\n"
    )
}

/// The issue's repro: `claude-opus-4.1` metered a $0.000000 floor, so a
/// $0.02 cap passed and the run proceeded with zero budget protection.
#[test]
fn uncataloged_dot_variant_plus_cap_refuses_to_start() {
    let wf = parse(&gauntlet_wf("claude-opus-4.1"));
    let report = nika_check::check(&wf);
    assert!(
        report.is_clean(),
        "the MODELS rung is a CLI ladder — the nika-check report stays clean"
    );
    let err = gates(&wf, &report, &BTreeMap::new(), Some(0.02), None, None, &[])
        .expect_err("an uncataloged id must not pass an armed cap by metering $0");
    assert_eq!(err.spec_code(), "NIKA-1709");
    let msg = err.to_string();
    assert!(msg.contains("claude-opus-4.1"), "names the seat");
    assert!(
        msg.contains("<provider>/<model>"),
        "teaches the pin contract"
    );
    assert!(msg.contains("nika catalog"), "names the catalog");
    assert!(msg.contains("0.020000"), "the armed cap rides");
}

/// The issue's « excellent behavior — keep it »: the cataloged seat
/// spelling `anthropic/claude-opus-4-1` prices a $0.057600 floor (768
/// output tokens × $75/M) that refuses the $0.02 cap — the floor arm,
/// with its exact teaching text, unchanged.
#[test]
fn cataloged_prefixed_twin_keeps_the_1709_floor_refusal() {
    let wf = parse(&gauntlet_wf("anthropic/claude-opus-4-1"));
    let report = nika_check::check(&wf);
    assert!(report.is_clean());
    let err = gates(&wf, &report, &BTreeMap::new(), Some(0.02), None, None, &[])
        .expect_err("the priced floor refuses the tiny cap");
    assert_eq!(err.spec_code(), "NIKA-1709");
    let msg = err.to_string();
    assert!(
        msg.contains("unavoidable cost floor $0.057600"),
        "the floor arm's own text, kept verbatim"
    );
}

/// The bare DASH form is priced (the floor sees it) but never RESOLVES — a bare id is not
/// `<provider>/<model>`. Under a generous cap the run used to start and die at dispatch;
/// admission now refuses with the resolver's contract teaching (no unique pasteable
/// seat exists for this id, so the generic form speaks).
#[test]
fn bare_cataloged_dash_under_a_generous_cap_refuses_at_admission() {
    let wf = parse(&gauntlet_wf("claude-opus-4-1"));
    let report = nika_check::check(&wf);
    assert!(report.is_clean());
    let err = gates(&wf, &report, &BTreeMap::new(), Some(1.00), None, None, &[])
        .expect_err("a bare id — priced or not — cannot be bounded by a cap");
    assert_eq!(err.spec_code(), "NIKA-1709");
    let msg = err.to_string();
    assert!(msg.contains("claude-opus-4-1"), "names the seat");
    assert!(
        msg.contains("bare model id"),
        "the resolver's why rides verbatim"
    );
    assert!(msg.contains("<provider>/<model>"), "the contract is taught");
}

/// The spare arm that hid the hole: an UNKNOWN provider prefix stayed
/// « unknown, never promoted to cloud » — floor $0, any cap passed.
#[test]
fn unknown_provider_prefix_plus_cap_refuses_to_start() {
    let wf = parse(&gauntlet_wf("not-a-provider/gpt-4"));
    let report = nika_check::check(&wf);
    let err = gates(&wf, &report, &BTreeMap::new(), Some(0.02), None, None, &[])
        .expect_err("an unresolvable prefix must not pass an armed cap");
    assert_eq!(err.spec_code(), "NIKA-1709");
    let msg = err.to_string();
    assert!(msg.contains("not-a-provider"), "names the prefix");
    assert!(msg.contains("does not resolve"), "the resolver names why");
}

/// The task-level pin is judged the same as the envelope pin.
#[test]
fn task_level_dot_variant_plus_cap_refuses_to_start() {
    let wf = parse(
        "nika: g1368-task\nmodel: mock/echo\npermits: {}\ntasks:\n  ping:\n    infer: { prompt: \"PONG\", max_tokens: 768, model: \"claude-opus-4.1\" }\n",
    );
    let report = nika_check::check(&wf);
    assert!(report.is_clean());
    let err = gates(&wf, &report, &BTreeMap::new(), Some(0.02), None, None, &[])
        .expect_err("the task-pinned dot variant refuses too");
    assert_eq!(err.spec_code(), "NIKA-1709");
    assert!(err.to_string().contains("claude-opus-4.1"));
}

/// The `--var`-resolved form (W0-D-R1's walk): the live id is judged.
#[test]
fn var_resolved_dot_variant_plus_cap_refuses_to_start() {
    let wf = parse(&runtime_model_input_wf());
    let report = nika_check::check(&wf);
    let overrides = BTreeMap::from([(
        "model".to_owned(),
        Value::String("claude-opus-4.1".to_owned()),
    )]);
    let err = gates(&wf, &report, &overrides, Some(0.02), None, None, &[])
        .expect_err("a var-resolved dot variant refuses under a cap");
    assert_eq!(err.spec_code(), "NIKA-1709");
    assert!(err.to_string().contains("claude-opus-4.1"));
}

/// The CLI `--model` override is the live id the gate prices (#342) —
/// overriding INTO an uncataloged id refuses the same way.
#[test]
fn cli_model_override_to_a_dot_variant_refuses() {
    let wf = parse(&gauntlet_wf("mock/echo"));
    let report = nika_check::check(&wf);
    let err = gates(
        &wf,
        &report,
        &BTreeMap::new(),
        Some(0.02),
        Some("claude-opus-4.1"),
        None,
        &[],
    )
    .expect_err("the override is the run's real seat");
    assert_eq!(err.spec_code(), "NIKA-1709");
    assert!(err.to_string().contains("claude-opus-4.1"));
}

/// The sparing arms that MUST keep working (#1368's explicit constraint):
/// local/self-hosted seats (the catalog's five loopback providers) and
/// the mock are free by CONSTRUCTION, never by silence — a cap admits
/// them, as today.
#[test]
fn local_and_mock_seats_still_admit_under_a_cap() {
    for model in [
        "mock/echo",
        "ollama/qwen3.5:4b",
        "lmstudio/anything-pulled",
        "llamacpp/anything-pulled",
        "localai/anything-pulled",
        "vllm/anything-pulled",
    ] {
        let wf = parse(&gauntlet_wf(model));
        let report = nika_check::check(&wf);
        assert!(
            gates(&wf, &report, &BTreeMap::new(), Some(0.02), None, None, &[]).is_ok(),
            "{model} is free by construction — a cap must keep admitting it"
        );
    }
}

/// The honest boundary of the fix: with NO cap armed the admission gate
/// makes no budget claim — the FORM law (NIKA-PROVIDER) still speaks
/// loud at dispatch, per task, and the run fails there as today. What
/// changed is that an ARMED cap can no longer be disarmed by an
/// uncataloged id.
#[test]
fn without_a_cap_the_gate_makes_no_budget_claim() {
    let wf = parse(&gauntlet_wf("claude-opus-4.1"));
    let report = nika_check::check(&wf);
    assert!(
        gates(&wf, &report, &BTreeMap::new(), None, None, None, &[]).is_ok(),
        "no budget armed → no budget-floor claim (dispatch owns the FORM refusal)"
    );
}

/// The run-level pin: the refusal precedes the prologue — zero events,
/// zero spend (the `run_refused` helper asserts the empty sink).
#[tokio::test]
async fn dot_variant_under_cap_refuses_before_the_prologue() {
    let wf = parse(&gauntlet_wf("claude-opus-4.1"));
    let runtime = runtime_with(MockShell::new()).with_max_cost_usd(Some(0.02));
    let err = run_refused(&runtime, &wf).await;
    assert_eq!(err.spec_code(), "NIKA-1709");
    let msg = err.to_string();
    assert!(msg.contains("claude-opus-4.1"), "names the seat");
    assert!(msg.contains("nika catalog"), "names the catalog");
}
