// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The authored requirement through the ONE resolver: exact routes, the
//! protocol law per role, the file-versus-flag conflict, and the old
//! resolution byte-identical when the file declares nothing.

use std::collections::BTreeMap;

use nika_types::access::{AccessClass, AccessFallback, AccessProtocol, AccessRequirement};

use super::*;
use crate::probe::{ExecutionLocus, ProviderReadiness};

fn api_probe(id: &str, key_present: bool) -> ProviderProbe {
    ProviderProbe::new(
        id,
        true,
        key_present,
        format!("{}_API_KEY", id.to_uppercase()),
        false,
        ProviderReadiness::new(
            true,
            key_present,
            None,
            None,
            true,
            ExecutionLocus::Cloud,
            AccessClass::Api,
        ),
        "https://api.example.com",
    )
}

/// A harness row: `acp` = the ACP speaker is on PATH (`key_present`),
/// `signed_in` = its login answered; the product binary is present.
#[cfg(feature = "access-harness")]
fn harness_probe(id: &str, serves: &[&str], acp: bool, signed_in: bool) -> ProviderProbe {
    ProviderProbe::new(
        id,
        false,
        acp,
        "",
        false,
        ProviderReadiness::new(
            true,
            signed_in,
            None,
            None,
            false,
            ExecutionLocus::Loopback,
            AccessClass::Harness,
        ),
        "",
    )
    .with_serves(serves.iter().map(|s| (*s).to_owned()).collect())
}

fn codex_route() -> AccessRequirement {
    AccessRequirement::new()
        .with_via(Some("codex".into()))
        .with_protocol(Some(AccessProtocol::Acp))
        .with_fallback(Some(AccessFallback::None))
}

fn agent(model: &str) -> ModelNeed {
    ModelNeed::new(model, false, true)
}

fn infer(model: &str) -> ModelNeed {
    ModelNeed::new(model, true, false)
}

fn verbs(needs: &[ModelNeed]) -> VerbNeeds {
    VerbNeeds::new(needs.iter().any(|n| n.infer), needs.iter().any(|n| n.agent))
}

fn plan(
    needs: &[ModelNeed],
    probes: &[ProviderProbe],
    pin: Option<&str>,
    req: &AccessRequirement,
) -> ExecutionAccessPlan {
    resolve_execution_plan_declared(needs, probes, pin, verbs(needs), Some(req))
}

fn refusal(plan: &ExecutionAccessPlan) -> (&'static str, String) {
    let (code, message) = match plan.pin_refusal.as_ref().expect("refused") {
        PinRefusal::UnknownToken { message } => ("1802", message),
        PinRefusal::PinUnsatisfied { message } => ("1801", message),
        PinRefusal::NoPath { message } => ("1800", message),
        PinRefusal::Unavailable { message } => ("1803", message),
    };
    (code, message.clone())
}

/// No declaration = the existing resolution, field for field.
#[test]
fn without_a_requirement_the_plan_is_the_existing_one() {
    let probes = vec![api_probe("openai", true), api_probe("mistral", false)];
    let needs = [
        infer("openai/gpt-5-mini"),
        agent("mistral/mistral-small-latest"),
    ];
    let old = resolve_execution_plan_for(&needs, &probes, None, verbs(&needs));
    let new = resolve_execution_plan_declared(&needs, &probes, None, verbs(&needs), None);
    assert_eq!(old, new);
    assert_eq!(new.requirement, None);
    assert_eq!(new.resume_pin(), None);
}

/// An effort alone keeps today's route and only records the requirement.
#[test]
fn an_effort_alone_keeps_the_route_and_is_recorded() {
    let probes = vec![api_probe("deepseek", true)];
    let needs = [infer("deepseek/deepseek-flash")];
    let req = AccessRequirement::new().with_effort(Some("high".into()));
    let plan = plan(&needs, &probes, None, &req);
    assert!(plan.is_admitted(), "{:?}", plan.pin_refusal);
    let lane = plan.lane("deepseek/deepseek-flash").expect("admitted");
    assert_eq!(lane.plan.chosen, AccessClass::Api);
    assert_eq!(lane.plan.requirement.as_deref(), Some(&req));
    assert_eq!(plan.requirement.as_ref(), Some(&req));
    assert_eq!(plan.pin, None);
    assert_eq!(
        plan.resume_pin().as_deref(),
        Some("run.access[effort=high]")
    );
}

/// An effort the catalog does not list for the exact model refuses before
/// task 1, naming the listed levels; the mock lists none.
#[test]
fn an_effort_the_provider_catalog_does_not_list_refuses() {
    let probes = vec![api_probe("deepseek", true)];
    let needs = [infer("deepseek/deepseek-flash")];
    let req = AccessRequirement::new().with_effort(Some("xhigh".into()));
    let (code, message) = refusal(&plan(&needs, &probes, None, &req));
    assert_eq!(code, "1800");
    assert!(
        message.contains("run.reasoning.effort: xhigh") && message.contains("low · high · max"),
        "{message}"
    );
    let mock = plan(&[infer("mock/echo")], &[], None, &req);
    let (code, message) = refusal(&mock);
    assert_eq!(code, "1800");
    assert!(message.contains("no effort level"), "{message}");
}

/// Spec fixture 017 · `via: codex` + `protocol: api` is the file's own
/// contradiction: refused whatever this machine offers.
#[test]
fn a_route_its_protocol_cannot_reach_refuses_before_any_probe() {
    let req = codex_route().with_protocol(Some(AccessProtocol::Api));
    for probes in [vec![], vec![api_probe("openai", true)]] {
        let plan = plan(&[agent("openai/gpt-5.5")], &probes, None, &req);
        let (code, message) = refusal(&plan);
        assert_eq!(code, "1800");
        assert!(
            message.contains("run.access.via: codex")
                && message.contains("run.access.protocol: api"),
            "{message}"
        );
        assert!(plan.admitted().next().is_none(), "no lane, no substitute");
        assert_eq!(plan.seat, None);
    }
    let openai_acp = AccessRequirement::new()
        .with_via(Some("openai".into()))
        .with_protocol(Some(AccessProtocol::Acp));
    let (code, message) = refusal(&plan(
        &[agent("openai/gpt-5.5")],
        &[api_probe("openai", true)],
        None,
        &openai_acp,
    ));
    assert_eq!(code, "1800");
    assert!(message.contains("run.access.protocol: acp"), "{message}");
}

/// Spec fixture 016 · an unknown route id is the pin judge's 1802, named
/// as the file field; a class word is not a route.
#[test]
fn an_unknown_route_and_a_class_word_refuse_with_the_file_field() {
    let unknown = AccessRequirement::new().with_via(Some("lighthouse".into()));
    let (code, message) = refusal(&plan(
        &[agent("openai/gpt-5.5")],
        &[api_probe("openai", true)],
        None,
        &unknown,
    ));
    assert_eq!(code, "1802");
    assert!(message.contains("run.access.via: lighthouse"), "{message}");
    let class = AccessRequirement::new().with_via(Some("api".into()));
    let (code, message) = refusal(&plan(
        &[agent("openai/gpt-5.5")],
        &[api_probe("openai", true)],
        None,
        &class,
    ));
    assert_eq!(code, "1802");
    assert!(
        message.contains("an access class") && message.contains("run.access.protocol"),
        "{message}"
    );
}

/// Spec fixture 015 · an explicit flag naming another route refuses before
/// task 1; the same route as the file is no conflict.
#[test]
fn a_flag_that_contradicts_the_file_refuses() {
    let probes = vec![api_probe("openai", true)];
    let needs = [agent("openai/gpt-5.5")];
    let (code, message) = refusal(&plan(&needs, &probes, Some("openai"), &codex_route()));
    assert_eq!(code, "1801");
    assert!(
        message.contains("`--access openai`") && message.contains("run.access.via: codex"),
        "{message}"
    );
    let api_only = AccessRequirement::new().with_protocol(Some(AccessProtocol::Api));
    let (code, message) = refusal(&plan(&needs, &probes, Some("codex"), &api_only));
    assert_eq!(code, "1801");
    assert!(message.contains("run.access.protocol: api"), "{message}");
}

/// The plan helpers: the effective route, the resume identity.
#[test]
fn the_route_and_the_resume_pin_follow_the_requirement() {
    let req = codex_route().with_effort(Some("high".into()));
    let plan = ExecutionAccessPlan::new(BTreeMap::new(), None, None, None)
        .with_requirement(Some(req.clone()));
    assert_eq!(plan.route_pin(), Some("codex"));
    assert_eq!(
        plan.resume_pin().as_deref(),
        Some("run.access[via=codex;protocol=acp;effort=high]")
    );
    let flagged = ExecutionAccessPlan::new(BTreeMap::new(), Some("codex".into()), None, None)
        .with_requirement(Some(req));
    assert_eq!(
        flagged.resume_pin().as_deref(),
        Some("codex · run.access[via=codex;protocol=acp;effort=high]")
    );
    let acp = ExecutionAccessPlan::new(BTreeMap::new(), None, None, None).with_requirement(Some(
        AccessRequirement::new().with_protocol(Some(AccessProtocol::Acp)),
    ));
    assert_eq!(acp.route_pin(), Some("harness"));
    let old = ExecutionAccessPlan::new(BTreeMap::new(), Some("api".into()), None, None);
    assert_eq!(
        old.resume_pin().as_deref(),
        Some("api"),
        "a flag alone is unchanged"
    );
}

#[cfg(feature = "access-harness")]
mod seats {
    use nika_types::access::BillingClass;

    use super::*;

    /// The canon's case: `via: codex` · `protocol: acp` · `fallback: none`
    /// with an `openai` key READY beside the seat — the file alone decides,
    /// the agent lane rides codex, the key is never substituted, no flag.
    #[test]
    fn the_file_route_wins_over_a_ready_key_with_no_flag() {
        let probes = vec![
            api_probe("openai", true),
            harness_probe("codex", &["openai"], true, true),
        ];
        let needs = [agent("openai/gpt-5.5")];
        let plan = plan(&needs, &probes, None, &codex_route());
        assert!(plan.is_admitted(), "{:?}", plan.pin_refusal);
        let lane = plan.lane("openai/gpt-5.5").expect("admitted");
        assert_eq!(
            (lane.plan.access.as_str(), lane.plan.chosen),
            ("codex", AccessClass::Harness)
        );
        assert_eq!(lane.plan.billing, BillingClass::Unknown);
        assert_eq!(lane.plan.requirement.as_deref(), Some(&codex_route()));
        assert_eq!(plan.seat.as_deref(), Some("codex"));
        assert_eq!(plan.pin, None, "the flag field stays the operator's");
        assert_eq!(plan.route_pin(), Some("codex"));
        assert_eq!(plan.seat_for("openai/gpt-5.5"), Some("codex"));
        assert_eq!(
            plan.seat_for("${{ inputs.model }}"),
            Some("codex"),
            "a templated model rides the declared seat too"
        );
        assert!(
            plan.admitted()
                .all(|(_, l)| l.plan.chosen != AccessClass::Api),
            "the ready key is never substituted"
        );
    }

    /// Spec fixture 014 · an `infer:` task over codex ACP has no qualified
    /// one-shot: refused before task 1, the role and the open gap named, no
    /// seat spawned.
    #[test]
    fn an_infer_task_over_an_acp_route_without_a_one_shot_refuses() {
        let probes = vec![
            api_probe("openai", true),
            harness_probe("codex", &["openai"], true, true),
        ];
        let needs = [infer("openai/gpt-5.5")];
        let plan = plan(&needs, &probes, None, &codex_route());
        let (code, message) = refusal(&plan);
        assert_eq!(code, "1800");
        assert!(
            message.contains("`infer:` tasks cannot ride `codex`")
                && message.contains("no qualified tool-free ACP one-shot profile yet")
                && message.contains("CODEX_CONFIG")
                && message.contains("run.access.protocol: acp"),
            "{message}"
        );
        assert_eq!(plan.seat, None);
    }

    /// `via: claude-code, protocol: acp` seats the route for `infer:` too:
    /// its audited completion profile is the attested ACP one-shot, so the
    /// same declaration serves both roles over the same seat.
    #[test]
    fn an_infer_task_rides_the_attested_claude_code_one_shot() {
        let probes = vec![
            api_probe("anthropic", true),
            harness_probe("claude-code", &["anthropic"], true, true),
        ];
        let req = AccessRequirement::new()
            .with_via(Some("claude-code".into()))
            .with_protocol(Some(AccessProtocol::Acp))
            .with_effort(Some("max".into()));
        let needs = [
            infer("anthropic/claude-opus-5-5"),
            agent("anthropic/claude-opus-5-5"),
        ];
        let plan = plan(&needs, &probes, None, &req);
        assert!(plan.is_admitted(), "{:?}", plan.pin_refusal);
        assert_eq!(plan.seat.as_deref(), Some("claude-code"));
        let lane = plan.lane("anthropic/claude-opus-5-5").expect("admitted");
        assert_eq!(lane.plan.chosen, AccessClass::Harness);
        assert_eq!(lane.plan.access, "claude-code");
        assert_eq!(lane.plan.requirement.as_deref(), Some(&req));
    }

    /// Spec fixture 012 · the ACP speaker is absent while the key and the
    /// product CLI are ready: 1803 named as the file field, nothing
    /// substituted.
    #[test]
    fn an_absent_acp_speaker_refuses_with_zero_substitution() {
        let probes = vec![
            api_probe("openai", true),
            harness_probe("codex", &["openai"], false, true),
        ];
        let plan = plan(&[agent("openai/gpt-5.5")], &probes, None, &codex_route());
        let (code, message) = refusal(&plan);
        assert_eq!(code, "1803");
        assert!(
            message.contains("run.access.via: codex") && message.contains("codex-acp"),
            "{message}"
        );
        assert_eq!(plan.seat, None);
        assert!(
            plan.admitted()
                .all(|(_, l)| l.plan.chosen != AccessClass::Api)
        );
    }

    /// Spec fixture 007 · `protocol: api` with a ready codex seat: the
    /// key path serves, no seat is spawned.
    #[test]
    fn protocol_api_keeps_the_key_path_with_a_seat_ready() {
        let probes = vec![
            api_probe("openai", true),
            harness_probe("codex", &["openai"], true, true),
        ];
        let req = AccessRequirement::new()
            .with_via(Some("openai".into()))
            .with_protocol(Some(AccessProtocol::Api));
        let plan = plan(
            &[infer("openai/gpt-5.5"), agent("openai/gpt-5.5")],
            &probes,
            None,
            &req,
        );
        assert!(plan.is_admitted(), "{:?}", plan.pin_refusal);
        assert_eq!(
            plan.lane("openai/gpt-5.5").map(|l| l.plan.chosen),
            Some(AccessClass::Api)
        );
        assert_eq!(plan.seat, None);
        assert_eq!(plan.seat_for("openai/gpt-5.5"), None);
    }

    /// A direct one-shot cannot carry a declared effort: an `infer:` on
    /// codex without `protocol: acp` refuses the effort before task 1.
    #[test]
    fn a_declared_effort_cannot_ride_the_direct_one_shot() {
        let probes =
            vec![harness_probe("codex", &["openai"], true, true).with_product_present(true)];
        let req = AccessRequirement::new()
            .with_via(Some("codex".into()))
            .with_effort(Some("high".into()));
        let (code, message) = refusal(&plan(&[infer("openai/gpt-5.5")], &probes, None, &req));
        assert_eq!(code, "1800");
        assert!(
            message.contains("run.reasoning.effort: high") && message.contains("direct"),
            "{message}"
        );
    }

    /// `protocol: acp` with a compatible flag narrows to that seat.
    #[test]
    fn a_compatible_flag_narrows_the_declared_protocol() {
        let probes = vec![harness_probe("claude-code", &["anthropic"], true, true)];
        let req = AccessRequirement::new().with_protocol(Some(AccessProtocol::Acp));
        let plan = plan(
            &[agent("anthropic/claude-sonnet-5")],
            &probes,
            Some("claude-code"),
            &req,
        );
        assert!(plan.is_admitted(), "{:?}", plan.pin_refusal);
        assert_eq!(plan.seat.as_deref(), Some("claude-code"));
        assert_eq!(plan.pin.as_deref(), Some("claude-code"));
    }
}

fn api_route(via: &str) -> AccessRequirement {
    AccessRequirement::new()
        .with_via(Some(via.into()))
        .with_protocol(Some(AccessProtocol::Api))
        .with_fallback(Some(AccessFallback::None))
}

/// A model rendered at dispatch is judged by the same resolver on the
/// frozen rows: another provider than the file's `via` is refused (the
/// pin judge's 1801, named after the file field), the route itself
/// yields a lane that carries the requirement to the terminal.
#[test]
fn a_rendered_model_is_bound_by_the_declared_api_route() {
    let probes = [api_probe("deepseek", true), api_probe("mistral", true)];
    let frozen = plan(&[], &probes, None, &api_route("deepseek"));
    assert!(frozen.is_admitted(), "{:?}", frozen.pin_refusal);
    let both = VerbNeeds::new(true, true);
    match frozen.task_lane("mistral/mistral-small-latest", both) {
        Err(PinRefusal::PinUnsatisfied { message }) => {
            assert!(message.contains("`run.access.via: deepseek`"), "{message}");
            assert!(
                message.contains("mistral/mistral-small-latest"),
                "{message}"
            );
        }
        other => panic!("another provider must refuse: {other:?}"),
    }
    let lane = frozen
        .task_lane("deepseek/deepseek-flash", both)
        .expect("the route serves it")
        .expect("a lane");
    assert_eq!(
        (lane.access.as_str(), lane.chosen),
        ("deepseek", AccessClass::Api)
    );
    assert_eq!(lane.requirement.as_deref(), Some(&api_route("deepseek")));
}

/// A declared seat serves any rendered model on the seat (its ACP
/// session then selects that exact model or refuses before a prompt).
#[cfg(feature = "access-harness")]
#[test]
fn a_rendered_model_rides_the_declared_seat_with_its_lane() {
    let probes = [
        api_probe("openai", true),
        harness_probe("codex", &["openai"], true, true),
    ];
    let frozen = plan(&[], &probes, None, &codex_route());
    assert_eq!(frozen.seat.as_deref(), Some("codex"));
    let lane = frozen
        .task_lane("openai/gpt-6-astra", VerbNeeds::new(false, true))
        .expect("admitted")
        .expect("a lane");
    assert_eq!(
        (lane.access.as_str(), lane.chosen),
        ("codex", AccessClass::Harness)
    );
    assert_eq!(lane.requirement.as_deref(), Some(&codex_route()));
}

/// Without a requirement nothing changes (no lane, no refusal); under an
/// effort alone a rendered model keeps its own provider path, never
/// refused here, and the lane names it with the requirement.
#[test]
fn an_undeclared_or_effort_only_plan_never_refuses_a_rendered_model() {
    let probes = [api_probe("deepseek", true), api_probe("mistral", true)];
    let bare = resolve_execution_plan_for(&[], &probes, None, VerbNeeds::new(true, false));
    assert_eq!(
        bare.task_lane("mistral/mistral-small-latest", VerbNeeds::new(true, false)),
        Ok(None)
    );
    let effort = AccessRequirement::new().with_effort(Some("high".into()));
    let frozen = plan(&[], &probes, None, &effort);
    let lane = frozen
        .task_lane("mistral/mistral-small-latest", VerbNeeds::new(true, false))
        .expect("never refused")
        .expect("its own provider");
    assert_eq!((lane.access.as_str(), lane.pinned), ("mistral", false));
    assert_eq!(lane.requirement.as_deref(), Some(&effort));
}

/// An operator `--access` pin binds a rendered model like a static one:
/// another provider is the pin judge's own 1801, the pinned provider
/// yields its lane (no declaration, so no requirement rides it).
#[test]
fn a_rendered_model_is_bound_by_an_operator_pin() {
    let probes = [api_probe("deepseek", true), api_probe("mistral", true)];
    let pinned =
        resolve_execution_plan_for(&[], &probes, Some("deepseek"), VerbNeeds::new(true, true));
    assert!(pinned.is_admitted(), "{:?}", pinned.pin_refusal);
    match pinned.task_lane("mistral/mistral-small-latest", VerbNeeds::new(true, false)) {
        Err(PinRefusal::PinUnsatisfied { message }) => {
            assert!(message.contains("`--access deepseek`"), "{message}");
            assert!(
                message.contains("mistral/mistral-small-latest"),
                "{message}"
            );
        }
        other => panic!("another provider must refuse under the pin: {other:?}"),
    }
    let lane = pinned
        .task_lane("deepseek/deepseek-flash", VerbNeeds::new(true, false))
        .expect("the pin serves it")
        .expect("a lane");
    assert_eq!((lane.access.as_str(), lane.pinned), ("deepseek", true));
    assert!(lane.requirement.is_none(), "no declaration, no requirement");
}
