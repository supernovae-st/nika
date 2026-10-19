// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The model-choice capability against a fixture inventory (no call, no key, no network) and
//! this build's catalogue. The asks are what an agent selects after reading the person's words.

use nika_types::access::{AccessClass, AccessProtocol};

use super::{
    Delegation, ModelAsk, ModelChoice, ModelInventory, ModelOffer, ModelRole, admitted_output_price,
};
use crate::probe::{ExecutionLocus, KeyAuth, ModelListing, ProviderProbe, ProviderReadiness};
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

/// A compatible listing that advertises `models`; the type is the size-cap member's, so it is
/// built through its constructor, never a literal.
fn compatible_listing(models: &[&str]) -> ModelListing {
    let mut listing = ModelListing::new("listed");
    listing.protocol_compatible = Some(true);
    listing.http_status = Some(200);
    listing.models = models.iter().map(|&model| model.to_owned()).collect();
    listing.failure = None;
    listing
}

fn probe(id: &str, access: AccessClass, locus: ExecutionLocus) -> ProviderProbe {
    let readiness = ProviderReadiness::new(true, true, None, None, true, locus, access);
    let var = format!("{}_API_KEY", id.to_ascii_uppercase());
    let keyed = access == AccessClass::Api;
    // A probe names its effective endpoint: here the provider's seeded one.
    let seeded = crate::profile::seed().into_iter().find(|p| p.id == id);
    let endpoint = seeded.map_or("", |p| p.base_url);
    ProviderProbe::new(id, keyed, true, var, true, readiness, endpoint)
}

/// This machine's inventory from its probe rows and this build's catalogue: a configured
/// `deepseek` key offers its catalogue models priced (deprecated ids left out), the best of them
/// by list price is its Pro model, a seat offers its served provider's models unpriced, a local
/// server offers only what it listed, and no machine route serves the decision seat.
#[test]
fn the_machine_inventory_comes_from_probe_rows_and_the_catalogue() {
    let local = probe("ollama", AccessClass::Local, ExecutionLocus::Loopback);
    let mut listed = local.clone();
    listed.readiness.model_listing = Some(compatible_listing(&["qwen3:8b"]));
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
        let listed = admitted_output_price(
            "deepseek",
            name,
            "https://api.deepseek.com/v1/chat/completions",
        );
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

/// Claude Sonnet 5.5 is a native offered choice on an Anthropic key route without a live
/// listing: the catalog row offers it, and choosing it by id is exact, never unsupported
/// with alternatives.
#[test]
fn claude_sonnet_5_5_is_offered_on_an_anthropic_key() {
    let anthropic = probe("anthropic", AccessClass::Api, ExecutionLocus::Cloud);
    let inventory = ModelInventory::from_probes(std::slice::from_ref(&anthropic));
    let offers = inventory.offers(ModelRole::Run);
    assert!(
        offers
            .iter()
            .any(|o| o.model == "anthropic/claude-sonnet-5-5" && o.route.access == "anthropic"),
        "{offers:?}"
    );
    let ask = select(Some("anthropic"), Some("claude-sonnet-5-5"), None, None);
    let ModelChoice::Exact(chosen) = inventory.choose(ModelRole::Run, &ask) else {
        panic!("Claude Sonnet 5.5 is chosen exactly");
    };
    assert_eq!(chosen.model, "anthropic/claude-sonnet-5-5");
}

/// An offer shows the output price its admission uses for the exact route and
/// endpoint, or none. A model the snapshot prices but no tariff admits, a native route the
/// snapshot alone prices and a configured gateway in a provider's place show no bare price.
#[test]
fn an_option_label_never_shows_a_price_the_admission_does_not_use() {
    let probes = [
        probe("deepseek", AccessClass::Api, ExecutionLocus::Cloud),
        probe("openrouter", AccessClass::Api, ExecutionLocus::Cloud),
        probe("anthropic", AccessClass::Api, ExecutionLocus::Cloud),
    ];
    let inventory = ModelInventory::from_probes(&probes);
    let offers = inventory.offers(ModelRole::Run);
    for offer in offers {
        let (provider, name) = offer.model.split_once('/').expect("provider/name");
        let endpoint = probes.iter().find(|p| p.id == offer.route.access);
        let admitted = endpoint
            .filter(|_| offer.route.billing.is_usd_metered())
            .and_then(|p| crate::InferenceAdmission::qualify(provider, name, &p.endpoint).ok());
        assert_eq!(
            offer.output_usd_per_million.is_some(),
            admitted.is_some(),
            "{}",
            offer.model
        );
    }
    let price = |model: &str| {
        let offer = offers.iter().find(|o| o.model == model);
        offer.map(|o| o.output_usd_per_million)
    };
    assert_eq!(
        price("openrouter/z-ai/glm-5.3-flash"),
        Some(Some(1.6)),
        "the admission's peak, not a listed endpoint's price"
    );
    assert_eq!(price("deepseek/deepseek-v4-pro"), Some(Some(3.96)));
    assert_eq!(
        price("openrouter/qwen/qwen3-32b"),
        Some(None),
        "snapshot only"
    );
    assert_eq!(price("anthropic/claude-sonnet-4-6"), Some(Some(15.0)));
    assert_eq!(
        price("anthropic/claude-opus-4-5-20251101"),
        Some(None),
        "no tariff"
    );
    let mut gateway = probe("openrouter", AccessClass::Api, ExecutionLocus::Cloud);
    gateway.endpoint = "https://gateway.example/v1/chat/completions".into();
    let gated = ModelInventory::from_probes(&[gateway]);
    let gated = gated.offers(ModelRole::Run).iter();
    assert!(
        gated
            .filter(|o| o.route.access == "openrouter")
            .all(|o| o.output_usd_per_million.is_none())
    );
}

/// An Anthropic key on Anthropic's own endpoint offers the Author the current Claude models first:
/// Claude Sonnet 5.5 (the provider's catalog default), Opus 5.5, Haiku 5.5 and Fable 5.1, each a
/// ready, priced pick; the earlier models follow, unpriced. The picker lists these offers as is.
#[test]
fn an_anthropic_key_offers_the_current_claude_models_sonnet_5_5_first() {
    let anthropic = probe("anthropic", AccessClass::Api, ExecutionLocus::Cloud);
    let inventory = ModelInventory::from_probes(std::slice::from_ref(&anthropic));
    let offers: Vec<&ModelOffer> = inventory
        .offers(ModelRole::Author)
        .iter()
        .filter(|offer| offer.route.access == "anthropic")
        .collect();
    let first: Vec<(&str, Option<f64>)> = offers
        .iter()
        .take(4)
        .map(|offer| (offer.model.as_str(), offer.output_usd_per_million))
        .collect();
    assert_eq!(
        first,
        [
            ("anthropic/claude-sonnet-5-5", Some(10.0)),
            ("anthropic/claude-opus-5-5", Some(20.0)),
            ("anthropic/claude-haiku-5-5", Some(0.5)),
            ("anthropic/claude-fable-5-1", Some(50.0)),
        ]
    );
    let default = nika_catalog::find_provider("anthropic").map(|row| row.default_model);
    assert_eq!(default, Some("claude-sonnet-5-5"));
    assert!(offers.iter().all(|offer| offer.route.configured));
    // The legacy models the admission prices follow priced; the others stay unpriced.
    let priced: Vec<(&str, f64)> = (offers[4..].iter())
        .filter_map(|offer| Some((offer.model.as_str(), offer.output_usd_per_million?)))
        .collect();
    assert_eq!(
        priced,
        [
            ("anthropic/claude-sonnet-4-6", 15.0),
            ("anthropic/claude-haiku-4-5-20251001", 5.0),
        ],
        "{offers:?}"
    );
    assert!(offers.len() > 4, "the earlier models stay offered");
}

/// The current Claude models carry their admitted output price on Anthropic's own endpoint
/// (Sonnet 5.5 $10, Opus 5.5 $20, Haiku 5.5 $0.50, Fable 5.1 $50 per million); an earlier model
/// stays unpriced, so strongest still cannot rank the whole Anthropic inventory, and a named
/// Sonnet 5.5 is exact.
#[test]
fn sonnet_5_5_is_priced_not_ranked_and_exact_when_named() {
    let anthropic = probe("anthropic", AccessClass::Api, ExecutionLocus::Cloud);
    let inventory = ModelInventory::from_probes(std::slice::from_ref(&anthropic));
    for (model, price) in [
        ("claude-sonnet-5-5", Some(10.0)),
        ("claude-opus-5-5", Some(20.0)),
        ("claude-haiku-5-5", Some(0.5)),
        ("claude-fable-5-1", Some(50.0)),
        ("claude-sonnet-4-6", Some(15.0)),
        ("claude-opus-4-5-20251101", None),
    ] {
        assert_eq!(
            admitted_output_price("anthropic", model, &anthropic.endpoint),
            price,
            "{model}"
        );
    }
    for role in [ModelRole::Run, ModelRole::Author] {
        let offers = inventory.offers(role);
        let sonnet = offers
            .iter()
            .find(|offer| offer.model == "anthropic/claude-sonnet-5-5")
            .expect("Sonnet 5.5 is offered");
        assert_eq!(sonnet.route.access, "anthropic");
        assert_eq!(sonnet.output_usd_per_million, Some(10.0));
        // The compiled-in rehearsal backend offers its mock models beside the key's:
        // the ask names Anthropic, so the choice handed back is the key's offers.
        let on_key: Vec<ModelOffer> = offers
            .iter()
            .filter(|offer| offer.route.access == "anthropic")
            .cloned()
            .collect();
        let strongest = delegated(select(Some("anthropic"), None, Some("api"), API));
        assert_eq!(
            inventory.choose(role, &strongest),
            ModelChoice::Choose(on_key)
        );
        let exact = select(
            Some("anthropic"),
            Some("claude-sonnet-5-5"),
            Some("api"),
            API,
        );
        assert_eq!(
            inventory.choose(role, &exact),
            ModelChoice::Exact(sonnet.clone())
        );
    }
}

/// `DeepSeek` discontinued `deepseek-chat` and `deepseek-reasoner` on 2026-07-24 (its API change
/// log of 2026-04-24): the catalogue keeps them for recorded runs, but a model whose price row is
/// retired is never offered. A live listing, where a route has one, governs what is offered.
#[test]
fn retired_or_unlisted_models_are_not_offered() {
    let deepseek = probe("deepseek", AccessClass::Api, ExecutionLocus::Cloud);
    let names = |inventory: &ModelInventory| -> Vec<String> {
        let offers = inventory.offers(ModelRole::Run).iter();
        let on_route = offers.filter(|o| o.route.access == "deepseek");
        on_route.map(|o| o.model.clone()).collect()
    };
    let probes = std::slice::from_ref(&deepseek);
    let offered = names(&ModelInventory::from_probes(probes));
    for retired in ["deepseek/deepseek-chat", "deepseek/deepseek-reasoner"] {
        assert!(
            !offered.iter().any(|m| m == retired),
            "{retired}: {offered:?}"
        );
        let name = retired.trim_start_matches("deepseek/");
        let row = nika_catalog::find_pricing_scoped("deepseek", name);
        assert!(
            row.is_some_and(nika_catalog::ModelPricing::is_deprecated),
            "{name} still prices a recorded run"
        );
    }
    for current in ["deepseek/deepseek-flash", "deepseek/deepseek-v4-pro"] {
        assert!(
            offered.iter().any(|m| m == current),
            "{current}: {offered:?}"
        );
    }
    let mut listed = deepseek;
    listed.readiness.model_listing = Some(compatible_listing(&["deepseek-flash"]));
    let governed = names(&ModelInventory::from_probes(&[listed]));
    assert_eq!(governed, ["deepseek/deepseek-flash"], "the listing governs");
}

/// A route whose key the provider refused when it was listed is never offered; the listing of a
/// route that accepted its key still governs.
#[test]
fn a_route_whose_key_was_refused_offers_nothing() {
    // The rehearsal backend's `mock` offers ride every inventory: judge the deepseek route only.
    let names = |inventory: &ModelInventory| -> Vec<String> {
        let offers = inventory.offers(ModelRole::Run).iter();
        let on_route = offers.filter(|o| o.route.access == "deepseek");
        on_route.map(|o| o.model.clone()).collect()
    };
    let mut refused = probe("deepseek", AccessClass::Api, ExecutionLocus::Cloud);
    refused.readiness.key_auth = Some(KeyAuth::Unauthorized);
    assert!(names(&ModelInventory::from_probes(&[refused])).is_empty());
    let mut listed = probe("deepseek", AccessClass::Api, ExecutionLocus::Cloud);
    listed.readiness.key_auth = Some(KeyAuth::Ok);
    listed.readiness.model_listing = Some(compatible_listing(&["deepseek-flash"]));
    assert_eq!(
        names(&ModelInventory::from_probes(&[listed])),
        ["deepseek/deepseek-flash"]
    );
}
