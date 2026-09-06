// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The resolver's tests (beside the module at the 1,500-line wall).

use super::*;

fn local(id: &str, configured: bool) -> AccessCandidate {
    AccessCandidate::new(id, AccessClass::Local, configured)
}

fn api(id: &str, configured: bool, fix: &str) -> AccessCandidate {
    AccessCandidate::new(id, AccessClass::Api, configured).with_fix_var(fix)
}

/// B18 / issue 1306: `grok/grok-3` is the xAI seat for doctor/access.
/// B18 / issue 1306: `grok/grok-3` is the xAI seat for doctor/access.
#[test]
fn grok_alias_provider_of_is_xai() {
    assert_eq!(provider_of("grok/grok-3"), "xai");
    assert_eq!(provider_of("xai/grok-3"), "xai");
}

#[test]
fn a_single_configured_api_candidate_is_the_plan() {
    let plan = resolve_access(
        "mistral/mistral-small-latest",
        &[api("mistral", true, "MISTRAL_API_KEY")],
        None,
        None,
    )
    .expect("one configured candidate must resolve");
    assert_eq!(plan.model, "mistral/mistral-small-latest");
    assert_eq!(plan.provider, "mistral");
    assert_eq!(plan.access, "mistral");
    assert_eq!(plan.chosen, AccessClass::Api);
    assert_eq!(plan.billing, BillingClass::ApiMetered);
    assert!(!plan.pinned);
    assert!(plan.rejected.is_empty());
}

// ── P3 B6 · harness adapters ride the probe vec (R-5c) ────────

/// A harness-class probe row as the composer hands it over once the
/// adapter is detected on this machine: `serves` names the providers
/// it fronts, `configured` carries the auth surface's verdict.
fn harness_probe(id: &str, serves: &[&str], authenticated: bool) -> ProviderProbe {
    ProviderProbe::new(
        id,
        false,
        true,
        "",
        false,
        crate::probe::ProviderReadiness::new(
            true,
            authenticated,
            None,
            None,
            false,
            crate::probe::ExecutionLocus::Cloud,
            AccessClass::Harness,
        ),
        "",
    )
    .with_serves(serves.iter().map(|s| (*s).to_owned()).collect())
}

#[test]
fn a_detected_authenticated_adapter_row_becomes_a_harness_candidate() {
    let probes = vec![harness_probe("codex", &["openai"], true)];
    let candidates = candidates_for(&probes, "openai");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].access, "codex");
    assert_eq!(candidates[0].class, AccessClass::Harness);
    assert!(candidates[0].configured);
}

#[test]
fn a_harness_row_offers_nothing_to_a_provider_it_does_not_serve() {
    let probes = vec![harness_probe("codex", &["openai"], true)];
    assert!(candidates_for(&probes, "anthropic").is_empty());
}

#[test]
fn an_unauthenticated_adapter_row_is_a_candidate_marked_unconfigured() {
    let probes = vec![harness_probe("codex", &["openai"], false)];
    let candidates = candidates_for(&probes, "openai");
    assert_eq!(
        candidates.len(),
        1,
        "the refusal must NAME the harness's sign-in"
    );
    assert!(!candidates[0].configured);
    let refusal = resolve_access("openai/gpt-5", &candidates, None, None)
        .expect_err("unauthenticated never resolves");
    let witness = &refusal.rejected[0].witness;
    assert!(
        witness.contains("sign in to `codex` itself"),
        "the harness fix line rides verbatim, never `unset`: {witness}"
    );
    assert!(!witness.contains("unset"), "{witness}");
}

#[test]
fn the_harness_row_outranks_the_metered_api_when_both_serve() {
    // The sovereign order (local < mock < harness < oauth < api):
    // an authenticated harness wins over the metered key — the
    // access doctrine's whole point (the operator's own plan first).
    let api_row = ProviderProbe::new(
        "anthropic",
        true,
        true,
        "ANTHROPIC_API_KEY",
        true,
        crate::probe::ProviderReadiness::new(
            true,
            true,
            None,
            None,
            true,
            crate::probe::ExecutionLocus::Cloud,
            AccessClass::Api,
        ),
        "https://api.anthropic.com",
    );
    let harness_row = harness_probe("claude-code", &["anthropic"], true);
    let plan = resolve_access(
        "anthropic/claude-sonnet-4-5",
        &candidates_for(&[api_row, harness_row], "anthropic"),
        None,
        None,
    )
    .expect("both paths configured → a plan");
    assert_eq!(plan.access, "claude-code");
    assert_eq!(plan.chosen, AccessClass::Harness);
    assert_eq!(
        plan.rejected.len(),
        0,
        "the api row is outranked, NOT rejected (dispo au pin): {:?}",
        plan.rejected
    );
}

#[test]
fn a_pin_names_an_adapter_id_once_its_row_rides_the_probes() {
    let probes = vec![harness_probe("codex", &["openai"], true)];
    assert!(
        refuse_pin(["openai/gpt-5"], &probes, "codex").is_none(),
        "a detected adapter id is a legal pin"
    );
    assert!(
        matches!(
            refuse_pin(["openai/gpt-5"], &[], "codex"),
            Some(PinRefusal::Unavailable { .. })
        ),
        "known token + no harness rows → 1803, never 1802"
    );
}

#[test]
fn a_known_cli_token_is_never_1802() {
    for id in [
        "claude-code",
        "codex",
        "gemini-cli",
        "kimi-code",
        "qwen-code",
    ] {
        match refuse_pin(["ollama/qwen3.5:4b"], &[], id) {
            Some(PinRefusal::Unavailable { message }) => {
                assert!(
                    !message.contains("NIKA-1802") && !message.contains("neither"),
                    "{id}: {message}"
                );
            }
            other => panic!("{id} must be a known token, got {other:?}"),
        }
    }
}

#[test]
fn a_missing_cli_teaches_the_dummy_install_line() {
    let rt = nika_types::access::HarnessRuntime::CLAUDE_CODE;
    let probes = vec![harness_probe_absent(rt.id, rt.not_installed)];
    match refuse_pin(["ollama/qwen3.5:4b"], &probes, "claude-code") {
        Some(PinRefusal::Unavailable { message }) => {
            assert!(
                message.contains("Claude Code is not installed"),
                "{message}"
            );
            assert!(message.contains("Nika local / Nika Cloud"), "{message}");
        }
        other => panic!("expected 1803 Unavailable, got {other:?}"),
    }
}

#[test]
fn a_retired_alias_teaches_the_live_token() {
    match refuse_pin(["openai/gpt-5"], &[], "claude-agent-acp") {
        Some(PinRefusal::UnknownToken { message }) => {
            assert!(message.contains("retired"), "{message}");
            assert!(message.contains("--access claude-code"), "{message}");
        }
        other => panic!("retired alias must be 1802, got {other:?}"),
    }
    match refuse_pin(["openai/gpt-5"], &[], "codex-acp") {
        Some(PinRefusal::UnknownToken { message }) => {
            assert!(message.contains("--access codex"), "{message}");
        }
        other => panic!("retired alias must be 1802, got {other:?}"),
    }
}

#[test]
fn access_harness_class_with_no_runtime_is_1803_not_api_keys() {
    let api_row = ProviderProbe::new(
        "anthropic",
        true,
        false,
        "ANTHROPIC_API_KEY",
        true,
        crate::probe::ProviderReadiness::new(
            true,
            false,
            None,
            None,
            true,
            crate::probe::ExecutionLocus::Cloud,
            AccessClass::Api,
        ),
        "https://api.anthropic.com",
    );
    match refuse_pin(["anthropic/claude-sonnet-4-5"], &[api_row], "harness") {
        Some(PinRefusal::Unavailable { message }) => {
            assert!(
                message.contains("No agentic CLI runtime is installed"),
                "{message}"
            );
            assert!(!message.contains("ANTHROPIC_API_KEY"), "{message}");
            assert!(!message.contains("GEMINI_API_KEY"), "{message}");
        }
        other => panic!("--access harness must not walk API keys, got {other:?}"),
    }
}

fn harness_probe_absent(id: &str, message: &str) -> ProviderProbe {
    ProviderProbe::new(
        id,
        false,
        false,
        message,
        false,
        crate::probe::ProviderReadiness::new(
            true,
            false,
            None,
            None,
            false,
            crate::probe::ExecutionLocus::Cloud,
            AccessClass::Harness,
        ),
        "",
    )
}

#[test]
fn an_unconfigured_candidate_is_rejected_with_its_fix_var() {
    let plan = resolve_access(
        "openai/gpt-5",
        &[
            api("openai", false, "OPENAI_API_KEY"),
            local("lmstudio", true),
        ],
        None,
        None,
    )
    .expect("the local path must survive");
    assert_eq!(plan.access, "lmstudio");
    assert_eq!(plan.chosen, AccessClass::Local);
    assert_eq!(plan.rejected.len(), 1);
    let r = &plan.rejected[0];
    assert_eq!(r.access, "openai");
    assert_eq!(r.dimension, RejectionDimension::NotConfigured);
    assert_eq!(r.layer, RejectionLayer::Access);
    assert!(r.witness.contains("OPENAI_API_KEY unset"), "{}", r.witness);
}

#[test]
fn sovereign_order_prefers_local_over_metered_api() {
    let plan = resolve_access(
        "qwen/qwen3",
        &[
            api("qwen", true, "DASHSCOPE_API_KEY"),
            local("ollama", true),
        ],
        None,
        None,
    )
    .expect("both admissible");
    assert_eq!(plan.access, "ollama");
    assert_eq!(plan.chosen, AccessClass::Local);
    // The outranked api path FAILED nothing — it stays available,
    // silently (a pin can still name it), never a fake witness.
    assert!(plan.rejected.is_empty());
}

/// `order_key`'s configured-first leg (the `!` is load-bearing):
/// twin candidates differing only in key state resolve to the
/// CONFIGURED one, whichever order they arrive in.
#[test]
fn the_configured_twin_wins_either_order() {
    for pair in [
        vec![
            api("mistral", false, "MISTRAL_API_KEY"),
            api("mistral", true, "MISTRAL_API_KEY"),
        ],
        vec![
            api("mistral", true, "MISTRAL_API_KEY"),
            api("mistral", false, "MISTRAL_API_KEY"),
        ],
    ] {
        let plan = resolve_access("mistral/mistral-large", &pair, None, None)
            .expect("the configured twin resolves");
        assert_eq!(plan.access, "mistral");
        assert_eq!(plan.rejected.len(), 1, "the unconfigured twin is rejected");
        assert!(
            plan.rejected[0].witness.contains("MISTRAL_API_KEY unset"),
            "with its witness: {}",
            plan.rejected[0].witness
        );
    }
}

/// The FULL sovereign chain, pinned pairwise (mutation-killers for
/// every `sovereign_rank` arm — a deleted arm must flip a winner).
#[test]
fn the_sovereign_chain_is_total_local_mock_harness_oauth_api() {
    let all = [
        AccessCandidate::new("the-local", AccessClass::Local, true),
        AccessCandidate::new("the-mock", AccessClass::Mock, true),
        AccessCandidate::new("the-harness", AccessClass::Harness, true),
        AccessCandidate::new("the-oauth", AccessClass::Oauth, true),
        AccessCandidate::new("the-api", AccessClass::Api, true),
    ];
    for (winner, loser_rank) in [
        ("the-local", 0),
        ("the-mock", 1),
        ("the-harness", 2),
        ("the-oauth", 3),
    ] {
        let _ = loser_rank;
        let idx = all
            .iter()
            .position(|c| c.access == winner)
            .expect("present");
        let mut subset = all[idx..].to_vec();
        subset.reverse(); // enumeration order must never matter
        let plan = resolve_access("p/m", &subset, None, None).expect("configured");
        assert_eq!(
            plan.access, winner,
            "against every lower-ranked class, {winner} wins"
        );
    }
}

/// `classify_pin_refusal` (NIKA-1800 vs 1801): the pin-layer-only
/// refusal reads 1801; a candidate failing EARLIER (not configured)
/// reads 1800 with its access-layer witness.
#[test]
fn the_pin_refusal_classification_reads_the_failing_layer() {
    // Every rejection at the PIN layer → PinUnsatisfied (1801).
    let pin_refusal = resolve_access(
        "openai/gpt-5",
        &[
            api("openai", true, "OPENAI_API_KEY"),
            AccessCandidate::new("codex", AccessClass::Harness, true),
        ],
        None,
        Some("local"),
    )
    .expect_err("nothing matches the pin");
    assert!(matches!(
        classify_pin_refusal("openai/gpt-5", "local", &pin_refusal),
        PinRefusal::PinUnsatisfied { .. }
    ));
    // The api row unconfigured → it fails at the ACCESS layer first
    // → NoPath (1800) naming that witness.
    let access_refusal = resolve_access(
        "openai/gpt-5",
        &[api("openai", false, "OPENAI_API_KEY")],
        None,
        Some("codex"),
    )
    .expect_err("unconfigured fails before the pin");
    assert!(matches!(
        classify_pin_refusal("openai/gpt-5", "codex", &access_refusal),
        PinRefusal::NoPath { .. }
    ));
}

/// `access_plan_map`: a templated model is not a static fact — it
/// never appears in the map (the `!contains` filter, mutation-pinned).
#[test]
fn the_plan_map_skips_templated_models() {
    let map = access_plan_map(
        &["mock/echo".to_owned(), "${{ inputs.model }}".to_owned()],
        &[],
        None,
    );
    assert!(map.contains_key("mock/echo"));
    assert_eq!(map.len(), 1, "the templated row is absent, never guessed");
}

#[test]
fn a_ready_harness_pin_stamps_every_static_model() {
    let probes = vec![harness_probe("claude-code", &["anthropic"], true)];
    let map = access_plan_map(
        &[
            "ollama/qwen3.5:4b".to_owned(),
            "anthropic/claude-sonnet-4-5".to_owned(),
        ],
        &probes,
        Some("claude-code"),
    );
    assert_eq!(map.len(), 2);
    for plan in map.values() {
        assert_eq!(plan.access, "claude-code");
        assert_eq!(plan.chosen, AccessClass::Harness);
        assert_eq!(plan.billing, BillingClass::Unknown, "never guessed");
        assert!(plan.pinned);
    }
    assert_eq!(first_ready_harness(&probes), Some("claude-code"));
}

#[test]
fn a_codex_pin_stamps_requested_model_and_subscription_quota_without_a_price() {
    let probes = vec![harness_probe("codex", &["anthropic"], true)];
    let map = access_plan_map(
        &["anthropic/claude-sonnet-4-6".to_owned()],
        &probes,
        Some("codex"),
    );
    let plan = map
        .get("anthropic/claude-sonnet-4-6")
        .expect("the requested model is receipt evidence");
    assert_eq!(plan.model, "anthropic/claude-sonnet-4-6");
    assert_eq!(plan.access, "codex");
    assert_eq!(
        plan.billing,
        BillingClass::Unknown,
        "a seat's quota is never observable"
    );
    assert!(!plan.billing.is_usd_metered());
}

#[test]
fn a_pin_by_class_forces_the_outranked_path() {
    let plan = resolve_access(
        "qwen/qwen3",
        &[
            local("ollama", true),
            api("qwen", true, "DASHSCOPE_API_KEY"),
        ],
        None,
        Some("api"),
    )
    .expect("the pinned api path is admissible");
    assert_eq!(plan.access, "qwen");
    assert_eq!(plan.chosen, AccessClass::Api);
    assert!(plan.pinned);
    assert_eq!(plan.rejected.len(), 1);
    assert_eq!(
        plan.rejected[0].dimension,
        RejectionDimension::PinUnsatisfied
    );
    assert_eq!(plan.rejected[0].layer, RejectionLayer::Pin);
}

#[test]
fn a_pin_by_id_matches_the_candidate_id() {
    let plan = resolve_access(
        "ollama/llama3.2",
        &[local("ollama", true)],
        None,
        Some("ollama"),
    )
    .expect("pin names the only candidate");
    assert_eq!(plan.access, "ollama");
    assert!(plan.pinned);
}

#[test]
fn an_unsatisfied_pin_refuses_and_never_substitutes() {
    // A-4: both paths are admissible, the pin names neither — the
    // resolver must refuse, never quietly hand back a survivor.
    let refusal = resolve_access(
        "qwen/qwen3",
        &[
            local("ollama", true),
            api("qwen", true, "DASHSCOPE_API_KEY"),
        ],
        None,
        Some("harness"),
    )
    .expect_err("an unsatisfied pin is a refusal");
    assert_eq!(refusal.provider, "qwen");
    assert_eq!(refusal.rejected.len(), 2);
    for r in &refusal.rejected {
        assert_eq!(r.dimension, RejectionDimension::PinUnsatisfied);
        assert!(r.witness.contains("--access harness"), "{}", r.witness);
    }
}

#[test]
fn policy_allowlist_rejects_the_provider_with_a_witness() {
    let allowed = vec![String::from("ollama"), String::from("mistral")];
    let refusal = resolve_access(
        "openai/gpt-5",
        &[api("openai", true, "OPENAI_API_KEY")],
        Some(&allowed),
        None,
    )
    .expect_err("provider outside the allowlist");
    assert_eq!(refusal.rejected.len(), 1);
    let r = &refusal.rejected[0];
    assert_eq!(r.dimension, RejectionDimension::ProviderNotAllowed);
    assert_eq!(r.layer, RejectionLayer::Policy);
    assert!(r.witness.contains("openai"), "{}", r.witness);
}

#[test]
fn the_access_step_wins_the_witness_over_policy_and_pin() {
    // Steps 4→5→6: an unconfigured candidate under a forbidding
    // policy AND a foreign pin still teaches the credential first.
    let allowed = vec![String::from("nobody")];
    let refusal = resolve_access(
        "openai/gpt-5",
        &[api("openai", false, "OPENAI_API_KEY")],
        Some(&allowed),
        Some("harness"),
    )
    .expect_err("nothing survives");
    assert_eq!(refusal.rejected.len(), 1);
    assert_eq!(
        refusal.rejected[0].dimension,
        RejectionDimension::NotConfigured
    );
}

#[test]
fn no_candidates_refuses_with_the_parsed_provider() {
    let refusal = resolve_access("mistral/mistral-small-latest", &[], None, None)
        .expect_err("nothing to choose from");
    assert_eq!(refusal.model, "mistral/mistral-small-latest");
    assert_eq!(refusal.provider, "mistral");
    assert!(refusal.rejected.is_empty());
}

#[test]
fn same_class_ties_break_on_codepoint_id_order() {
    let plan = resolve_access(
        "prov/m",
        &[local("bbb", true), local("aaa", true)],
        None,
        None,
    )
    .expect("both admissible");
    assert_eq!(plan.access, "aaa");
}

#[test]
fn billing_evidence_override_reaches_the_plan() {
    let candidate = AccessCandidate::new("anthropic", AccessClass::Api, true)
        .with_billing(BillingClass::CreditMetered);
    let plan = resolve_access("anthropic/claude-sonnet-4-6", &[candidate], None, None)
        .expect("admissible");
    assert_eq!(plan.billing, BillingClass::CreditMetered);
}

#[test]
fn candidates_bridge_reads_the_probe_truth() {
    use crate::registry::{ProviderRegistry, ProvidersConfig};
    let registry = ProviderRegistry::without_http(ProvidersConfig::new());
    let probes = crate::probe::collect_provider_probes(&registry);
    let candidates = candidates_for(&probes, "ollama");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].access, "ollama");
    assert_eq!(candidates[0].class, AccessClass::Local);
    // Keyless local: no fix var to teach.
    assert!(candidates[0].fix_var.is_none());
    let candidates = candidates_for(&probes, "mistral");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].class, AccessClass::Api);
    assert!(candidates[0].fix_var.is_some());
}

/// The plan carries the chosen candidate's rung (ADR-134): a seat on PATH
/// is discovered, a keyless local path is declared, only a probe that
/// answered is observed — never above the evidence.
#[test]
fn the_plan_carries_the_candidates_rung_never_above_the_evidence() {
    let seat = AccessCandidate::new("codex", AccessClass::Harness, true);
    assert_eq!(seat.trust, Trust::Discovered);
    let plan = resolve_access("openai/gpt-x", &[seat], None, None).expect("admitted");
    assert_eq!(plan.trust, Trust::Discovered);
    assert_eq!(local("ollama", true).trust, Trust::Declared);
    let pinged = local("ollama", true).with_trust(Trust::from_evidence(
        AccessClass::Local,
        true,
        Some(true),
    ));
    assert_eq!(pinged.trust, Trust::Observed);
    let plan = resolve_access("ollama/x", &[pinged], None, None).expect("admitted");
    assert_eq!(plan.trust, Trust::Observed);
    assert_eq!(
        AccessCandidate::new("mock", AccessClass::Mock, true).trust,
        Trust::Observed
    );
}
