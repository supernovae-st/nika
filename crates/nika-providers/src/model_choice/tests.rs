// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The model-choice capability against a fixture inventory (no call, no key, no network) and
//! this build's catalogue. The asks are what an agent selects after reading the person's words.

use nika_types::access::{AccessClass, AccessProtocol};

use super::{Delegation, ModelAsk, ModelChoice, ModelInventory, ModelOffer, ModelRole};
use crate::probe::{ExecutionLocus, ModelListing, ProviderProbe, ProviderReadiness};
use crate::resolve_access::AccessCandidate;

const API: Option<AccessProtocol> = Some(AccessProtocol::Api);

fn api(id: &str, configured: bool) -> AccessCandidate {
    let var = format!("{}_API_KEY", id.to_ascii_uppercase());
    AccessCandidate::new(id, AccessClass::Api, configured).with_fix_var(var)
}

fn seat(id: &str) -> AccessCandidate {
    AccessCandidate::new(id, AccessClass::Harness, true)
}

fn pro_api() -> ModelOffer {
    ModelOffer::new(
        "deepseek/deepseek-v4-pro",
        api("deepseek", true),
        Some(0.87),
    )
}

fn flash_api() -> ModelOffer {
    ModelOffer::new("deepseek/deepseek-flash", api("deepseek", true), Some(0.6))
}

/// A fixture seat fronting `deepseek`: the second, materially different route (no shipped seat
/// serves `deepseek`; this row exists only here).
fn pro_seat() -> ModelOffer {
    ModelOffer::new("deepseek/deepseek-v4-pro", seat("fixture-seat"), None)
}

fn sonnet_unready() -> ModelOffer {
    ModelOffer::new(
        "anthropic/claude-sonnet-4-5",
        api("anthropic", false),
        Some(15.0),
    )
}

fn author_seat() -> ModelOffer {
    ModelOffer::new("anthropic/claude-opus-4-5", seat("claude-code"), None)
}

fn decision_seat() -> ModelOffer {
    ModelOffer::new("typesafe/jev-1.13.0", api("typesafe", true), None)
}

fn fixture() -> ModelInventory {
    let run = vec![pro_api(), flash_api(), pro_seat(), sonnet_unready()];
    ModelInventory::default()
        .with_offers(ModelRole::Run, run)
        .with_offers(ModelRole::Author, vec![author_seat()])
        .with_offers(ModelRole::Decision, vec![decision_seat()])
}

fn run(ask: &ModelAsk) -> ModelChoice {
    fixture().choose(ModelRole::Run, ask)
}

/// The agent's selection: each `Some` closes one dimension (a caller sets the public fields).
fn select(
    provider: Option<&str>,
    model: Option<&str>,
    via: Option<&str>,
    protocol: Option<AccessProtocol>,
) -> ModelAsk {
    let mut ask = ModelAsk::new();
    ask.provider = provider.map(str::to_owned);
    ask.model = model.map(str::to_owned);
    ask.via = via.map(str::to_owned);
    ask.protocol = protocol;
    ask
}

fn delegated(mut ask: ModelAsk) -> ModelAsk {
    ask.delegate = Some(Delegation::Strongest);
    ask
}

/// UXP-04 · « deepseek v4 pro par api »: the one supported target with its exact id, route and
/// class; the full id over the `api` class and a catalogue provider alias name the same offers.
#[test]
fn a_named_model_over_a_named_protocol_is_the_one_supported_target() {
    let ask = select(Some("deepseek"), Some("deepseek-v4-pro"), None, API);
    let ModelChoice::Exact(target) = run(&ask) else {
        panic!("one target");
    };
    assert_eq!(target, pro_api());
    assert_eq!(target.route.access, "deepseek");
    assert_eq!(target.route.class, AccessClass::Api);
    let full = select(None, Some("deepseek/deepseek-v4-pro"), Some("api"), None);
    assert_eq!(run(&full), ModelChoice::Exact(pro_api()));
    let aliased = select(Some("deep-seek"), Some("deepseek-flash"), None, None);
    assert_eq!(run(&aliased), ModelChoice::Exact(flash_api()));
}

/// UXP-05 · « deepseek le meilleur modèle par api »: the delegated pick among the compatible
/// ready API offers, recorded with the rule actually applied.
#[test]
fn a_delegated_best_is_picked_by_its_named_rule() {
    let ask = delegated(select(Some("deepseek"), None, None, API));
    assert_eq!(
        run(&ask),
        ModelChoice::Delegated {
            chosen: pro_api(),
            rule: "highest list price among ready offers",
            among: vec![pro_api(), flash_api()],
        }
    );
    // Over both routes the seat has no list price: the rule cannot rank, the person chooses.
    let open = delegated(select(Some("deepseek"), None, None, None));
    assert_eq!(
        run(&open),
        ModelChoice::Choose(vec![pro_api(), flash_api(), pro_seat()])
    );
    // A shared top price is no ranking either.
    let chat = ModelOffer::new("deepseek/deepseek-chat", api("deepseek", true), Some(0.87));
    let tied = ModelInventory::default().with_offers(ModelRole::Run, vec![pro_api(), chat.clone()]);
    assert_eq!(
        tied.choose(ModelRole::Run, &ask),
        ModelChoice::Choose(vec![pro_api(), chat])
    );
}

/// « utilise deepseek » over two routes: one targeted question with every ready option; naming
/// the model alone still leaves the route to the person (no silent protocol).
#[test]
fn a_provider_over_two_routes_asks_one_targeted_question() {
    let ask = select(Some("deepseek"), None, None, None);
    assert_eq!(
        run(&ask),
        ModelChoice::Choose(vec![pro_api(), flash_api(), pro_seat()])
    );
    let model_only = select(None, Some("deepseek/deepseek-v4-pro"), None, None);
    assert_eq!(
        run(&model_only),
        ModelChoice::Choose(vec![pro_api(), pro_seat()])
    );
    let acp = Some(AccessProtocol::Acp);
    let over_acp = select(None, Some("deepseek/deepseek-v4-pro"), None, acp);
    assert_eq!(run(&over_acp), ModelChoice::Exact(pro_seat()));
}

/// UXP-N03 · an exact model the inventory does not hold: the exact reason, the real ready
/// alternatives keeping the provider and protocol, and no near name taken for it.
#[test]
fn an_unsupported_exact_model_is_diagnosed_with_real_alternatives() {
    let ask = select(Some("deepseek"), Some("deepseek-v9-ultra"), None, API);
    assert_eq!(
        run(&ask),
        ModelChoice::Unsupported {
            why: "no Run offer here fits `deepseek · deepseek-v9-ultra · api`".to_owned(),
            alternatives: vec![pro_api(), flash_api()],
        }
    );
    let near = select(None, Some("deepseek-v4-pr"), None, None);
    assert!(matches!(run(&near), ModelChoice::Unsupported { .. }));
}

/// An offer that fits on a route that is not ready is refused with the route's own witness,
/// never replaced by another provider's ready offer.
#[test]
fn an_unready_route_is_refused_with_its_own_fix() {
    let ask = select(Some("anthropic"), None, None, None);
    assert_eq!(
        run(&ask),
        ModelChoice::Unsupported {
            why: "`anthropic/claude-sonnet-4-5` is not ready here: anthropic · not_configured \
                  (access layer) · ANTHROPIC_API_KEY unset in process env"
                .to_owned(),
            alternatives: vec![pro_api(), flash_api(), pro_seat()],
        }
    );
}

/// UXP-04/05 role scope: a role is decided from its own offers only — the workflow model never
/// reaches the Author or decision seats, and theirs never reach a Run.
#[test]
fn each_role_is_decided_from_its_own_offers_only() {
    let inventory = fixture();
    let jev = select(None, Some("typesafe/jev-1.13.0"), None, None);
    assert_eq!(
        inventory.choose(ModelRole::Decision, &jev),
        ModelChoice::Exact(decision_seat())
    );
    let ModelChoice::Unsupported { alternatives, .. } = inventory.choose(ModelRole::Run, &jev)
    else {
        panic!("the decision seat is no Run offer");
    };
    assert_eq!(alternatives, vec![pro_api(), flash_api(), pro_seat()]);
    let deepseek = select(Some("deepseek"), None, None, None);
    let ModelChoice::Unsupported { alternatives, .. } =
        inventory.choose(ModelRole::Author, &deepseek)
    else {
        panic!("the Run's DeepSeek is no Author offer");
    };
    assert_eq!(alternatives, vec![author_seat()]);
    assert_eq!(inventory.offers(ModelRole::Author), [author_seat()]);
    assert_eq!(inventory.offers(ModelRole::Decision), [decision_seat()]);
}

fn probe(id: &str, access: AccessClass, locus: ExecutionLocus) -> ProviderProbe {
    let readiness = ProviderReadiness::new(true, true, None, None, true, locus, access);
    let var = format!("{}_API_KEY", id.to_ascii_uppercase());
    let keyed = access == AccessClass::Api;
    ProviderProbe::new(id, keyed, true, var, true, readiness, "")
}

/// This machine's inventory from its probe rows and this build's catalogue: a configured
/// `deepseek` key offers its catalogue models priced (deprecated ids left out), the best of them
/// by list price is its Pro model, a seat offers its served provider's models unpriced, a local
/// server offers only what it listed, and no machine route serves the decision seat.
#[test]
fn the_machine_inventory_comes_from_probe_rows_and_the_catalogue() {
    let local = probe("ollama", AccessClass::Local, ExecutionLocus::Loopback);
    let mut listed = local.clone();
    listed.readiness.model_listing = Some(ModelListing {
        protocol_compatible: Some(true),
        http_status: Some(200),
        models: vec!["qwen3:8b".to_owned()],
        failure: None,
    });
    let claude = probe("claude-code", AccessClass::Harness, ExecutionLocus::Unknown)
        .with_serves(vec!["anthropic".to_owned()]);
    let deepseek_key = probe("deepseek", AccessClass::Api, ExecutionLocus::Cloud);
    let inventory = ModelInventory::from_probes(&[deepseek_key, claude]);
    let offers = inventory.offers(ModelRole::Run);
    let on = |route: &str| {
        let on_route = offers.iter().filter(|o| o.route.access == route);
        on_route.collect::<Vec<&ModelOffer>>()
    };
    let deepseek = on("deepseek");
    for name in ["deepseek-v4-pro", "deepseek-flash"] {
        let model = format!("deepseek/{name}");
        let offer = deepseek.iter().find(|o| o.model == model).expect("offered");
        let listed =
            nika_catalog::find_pricing_scoped("deepseek", name).map(|p| p.output_per_million);
        assert!(listed.is_some(), "{name}");
        assert_eq!(offer.output_usd_per_million, listed, "{name}");
    }
    assert!(
        !deepseek
            .iter()
            .any(|o| o.model == "deepseek/deepseek-v4-flash")
    );
    let best = delegated(select(Some("deepseek"), None, None, API));
    let ModelChoice::Delegated { chosen, .. } = inventory.choose(ModelRole::Run, &best) else {
        panic!("the catalogue ranks DeepSeek's API models");
    };
    assert_eq!(chosen.model, "deepseek/deepseek-v4-pro");
    let seated = on("claude-code");
    assert!(!seated.is_empty());
    assert!(seated.iter().all(|o| o.model.starts_with("anthropic/")));
    assert!(seated.iter().all(|o| o.output_usd_per_million.is_none()));
    assert_eq!(inventory.offers(ModelRole::Author), offers);
    assert!(inventory.offers(ModelRole::Decision).is_empty());
    let served = |inventory: &ModelInventory, route: &str| -> Vec<String> {
        let offers = inventory.offers(ModelRole::Run).iter();
        let on_route = offers.filter(|o| o.route.access == route);
        on_route.map(|o| o.model.clone()).collect()
    };
    assert!(served(&ModelInventory::from_probes(&[local]), "ollama").is_empty());
    let offered = ModelInventory::from_probes(&[listed]);
    assert_eq!(served(&offered, "ollama"), ["ollama/qwen3:8b"]);
    // The rehearsal backend is compiled in: its catalogue models stay classed `mock`.
    assert_eq!(served(&offered, "mock"), ["mock/mock-default", "mock/echo"]);
}
