// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Read-only choices for the native intelligence picker. Selection still uses the census's
//! existing parser and resolution; a row never establishes reachability or calls a provider.

use super::SessionRuntime;
use crate::intelligence::{
    IntelligenceCensus, IntelligenceKind, ResolvedSessionIntelligence, UserIntelligencePreference,
};
use crate::work::{ModelFacts, Offer, OfferValue, ValueRole};
use nika_providers::model_choice::{ModelInventory, ModelOffer, ModelRole};

/// The facts an option states of its model, when it names one.
fn model_of(offer: &Offer) -> Option<&ModelFacts> {
    offer.values.iter().find_map(|value| value.choice.as_ref())
}

impl SessionRuntime {
    /// The option recommended for this conversation, by key: the one recommendation policy.
    /// The operator's saved default comes first while this machine serves it and an option
    /// names it; else the newest model among the configured providers. The catalogue states no
    /// release dates, so each configured provider's own default model stands for its newest, in
    /// the catalogue's provider order. A recommendation selects nothing.
    pub(super) fn recommended(&self, options: &[Offer]) -> Option<String> {
        let census = self.census.as_ref()?;
        (self.operator_default.as_ref())
            .and_then(|default| saved(census, default, options))
            .or_else(|| newest(options))
    }
}

/// The ready option the operator's saved default names, when this machine serves it.
fn saved(
    census: &IntelligenceCensus,
    default: &UserIntelligencePreference,
    options: &[Offer],
) -> Option<String> {
    if !ResolvedSessionIntelligence::resolve(default, census).ready {
        return None;
    }
    let (class, via) = match &default.kind {
        IntelligenceKind::Api { provider } => ("api", provider),
        IntelligenceKind::Harness { seat, .. } => ("harness", seat),
        IntelligenceKind::Local { provider } => ("local", provider),
        _ => return None,
    };
    let names = |facts: &ModelFacts| {
        facts.configured
            && facts.class == class
            && facts.via == *via
            && default
                .model
                .as_deref()
                .is_none_or(|model| model == facts.model)
    };
    (options.iter())
        .find(|offer| model_of(offer).is_some_and(names))
        .map(|offer| offer.key.clone())
}

/// The ready option serving the first configured provider's default model, in the catalogue's
/// provider order.
fn newest(options: &[Offer]) -> Option<String> {
    nika_catalog::all_providers().iter().find_map(|provider| {
        let model = format!("{}/{}", provider.id, provider.default_model);
        (options.iter())
            .find(|offer| {
                model_of(offer).is_some_and(|facts| facts.configured && facts.model == model)
            })
            .map(|offer| offer.key.clone())
    })
}

/// What the catalogue states of `offer`, typed for a host to show: its prices per million tokens
/// on a metered route (the qualified tariff the route is admitted with, both prices from that one
/// row; else the catalogue's list row for the model), its context window and whether the model
/// is legacy. The window is the model's own: its provider row's when a source is recorded for
/// it, else the tariff's, else the list row's. A tariff that prices only a cheaper part of that
/// window (Claude Haiku 5.5's rates hold up to 100,000 prompt tokens of its million) says how
/// far its prices hold. A subscription or local route carries no price, and an unknown one stays
/// `None`, never zero. None of these facts selects, admits or calls anything.
fn facts(census: &IntelligenceCensus, offer: &ModelOffer) -> ModelFacts {
    let route = &offer.route;
    let (provider, name) = offer.model.split_once('/').unwrap_or(("", &offer.model));
    let endpoint = (census.provider_context.iter())
        .find(|probe| probe.id == route.access)
        .map(|probe| probe.endpoint.as_str());
    let tariff = endpoint
        .filter(|_| offer.output_usd_per_million.is_some())
        .and_then(|endpoint| {
            nika_catalog::admission::InferenceTariff::new(provider, name, endpoint)
        })
        .filter(|tariff| tariff.currency == "USD");
    let listed = nika_catalog::find_pricing_for(&offer.model);
    // Qualified rates are integers below 2^53: exact in an f64.
    #[allow(clippy::cast_precision_loss)]
    let per_million = |nano: i128| nano as f64 / 1000.0;
    let (input, output) = match (tariff, listed) {
        (Some(tariff), _) => (
            tariff.price_native(1, 0, 0).map(per_million),
            offer.output_usd_per_million,
        ),
        (None, Some(row)) if route.billing.is_usd_metered() => {
            (Some(row.input_per_million), Some(row.output_per_million))
        }
        (None, _) => (None, None),
    };
    let context = sourced_window(provider, name)
        .or_else(|| tariff.map(|tariff| tariff.context_tokens))
        .or_else(|| {
            listed
                .and_then(|row| row.context_window_tokens)
                .map(u64::from)
        });
    let reach = (tariff.map(|tariff| tariff.context_tokens))
        .filter(|tokens| context.is_some_and(|window| *tokens < window));
    let legacy = listed.is_some_and(nika_catalog::ModelPricing::is_deprecated)
        || nika_catalog::model_facts(provider, name).is_some_and(|facts| facts.lifecycle.is_some());
    ModelFacts::new(
        "author",
        offer.model.clone(),
        (
            route.access.clone(),
            route.class.as_str().to_owned(),
            route.billing.as_str().to_owned(),
        ),
        route.configured,
        output,
    )
    .with_catalogue(input, context, legacy)
    .with_prices_up_to(reach)
}

/// The context window `provider`'s catalogue row states for the wire `name` from a recorded
/// source (the provider's model page): the model's own window, which a tariff may price only in
/// part. A curated row with no recorded source yields to the tariff and list rows.
fn sourced_window(provider: &str, name: &str) -> Option<u64> {
    // A row whose limits carry no recorded source answers no window here.
    nika_catalog::model_facts(provider, name)?.limits_source?;
    let row = (nika_catalog::find_provider(provider)?.models.iter()).find(|m| m.model == name)?;
    Some(u64::from(row.context_window_tokens)).filter(|tokens| *tokens > 0)
}

fn key_required(provider: &str) -> Option<String> {
    let profile = nika_providers::profile::seed()
        .into_iter()
        .find(|profile| {
            profile.id == provider && profile.access_class() == nika_types::access::AccessClass::Api
        })?;
    Some(format!(
        "key required · configure {}",
        profile.env_candidates().join(" or ")
    ))
}

/// An operator-pointed endpoint may have no observed model catalogue: offer configuration
/// of a known provider before its key exists, without inventing a model.
fn offer_missing_apis(options: &mut Vec<Offer>, configured: &[String]) {
    for profile in nika_providers::profile::seed() {
        if configured.iter().any(|key| key == profile.id) {
            continue;
        }
        let Some(setup) = key_required(profile.id) else {
            continue;
        };
        let key = format!("2 {}", profile.id);
        if !options
            .iter()
            .any(|offer| offer.key == key || offer.key.starts_with(&format!("{key}/")))
        {
            options.push(Offer::new(
                key,
                format!("{} API · {setup}", profile.id),
                false,
            ));
        }
    }
}

impl SessionRuntime {
    /// A connection label opens actual choices; it is never a provider id.
    pub(super) fn api_intelligence_screen(&mut self) -> super::TurnOutcome {
        self.pending_choice = true;
        let options: Vec<_> = self
            .intelligence_options()
            .into_iter()
            .filter(|option| option.key.starts_with("2 "))
            .map(|option| format!("  {} · {}", option.key, option.label))
            .collect();
        let choices = if options.is_empty() {
            "No configured API connection is available. `nika doctor` lists providers and the key variables they accept; `/intelligence` shows the other connections.".to_owned()
        } else {
            options.join("\n")
        };
        let waiting = if self.interrupted.is_some() {
            " Your waiting request is kept."
        } else {
            ""
        };
        super::TurnOutcome::Ask(format!(
            "Choose an API connection for this conversation.{waiting}\n{choices}"
        ))
    }

    pub(super) fn intelligence_options(&self) -> Vec<Offer> {
        let Some(census) = &self.census else {
            return Vec::new();
        };
        let mut options = Vec::new();
        for offer in ModelInventory::from_probes(&census.provider_context).offers(ModelRole::Author)
        {
            let key_missing = offer.route.class == nika_types::access::AccessClass::Api
                && !census.api_keys.iter().any(|key| key == &offer.route.access);
            if !offer.route.configured && !key_missing {
                continue;
            }
            let model = &offer.model;
            let (pick, named) = match offer.route.class.as_str() {
                "api" => ("2", model.clone()),
                "local" => ("3", model.clone()),
                "harness" => (
                    "1",
                    format!(
                        "{}/{}",
                        offer.route.access,
                        model
                            .split_once('/')
                            .map_or(model.as_str(), |(_, name)| name)
                    ),
                ),
                _ => continue,
            };
            let key = format!("{pick} {named}");
            if census.choose(&key).is_err() || options.iter().any(|o: &Offer| o.key == key) {
                continue;
            }
            let selected = self.intelligence.ready
                && self.intelligence.model.as_deref() == Some(model.as_str());
            // A seat that writes no workflow says so on each of its rows, before any choice.
            let authors = if pick == "1" {
                writes_workflows(&offer.route.access)
            } else {
                String::new()
            };
            let setup = if key_missing {
                key_required(&offer.route.access).map(|why| format!(" · {why}"))
            } else {
                None
            };
            let value = OfferValue::new(ValueRole::Value, model.clone(), None)
                .with_choice(Some(facts(census, offer)));
            options.push(
                Offer::new(
                    key,
                    format!(
                        "{model} · {} · {}{}{}{authors}",
                        offer.route.access,
                        offer.route.billing.as_str(),
                        if selected { " · current" } else { "" },
                        setup.unwrap_or_default()
                    ),
                    selected,
                )
                .with_values(vec![value]),
            );
        }
        // A census built without full catalogue probes still offers its actual connections.
        for provider in &census.api_keys {
            let label = format!("{provider} API · configured default model · metered");
            unprobed(&mut options, census, format!("2 {provider}"), label);
        }
        offer_missing_apis(&mut options, &census.api_keys);
        for seat in &census.seats {
            let label = format!("{} · agent account{}", seat.id, writes_workflows(&seat.id));
            unprobed(&mut options, census, format!("1 {}", seat.id), label);
        }
        for provider in &census.locals {
            let label = format!("{provider} · configured local route (not probed)");
            unprobed(&mut options, census, format!("3 {provider}"), label);
        }
        options.sort_by_key(|option| !option.recommended);
        options
    }
}

/// The label part of a row of the subscription seat `seat` when it cannot write workflows
/// here ([`crate::authoring::NO_AUTHORING`]), else nothing.
fn writes_workflows(seat: &str) -> String {
    crate::authoring::seat_cannot_author(seat).map_or_else(String::new, |_| {
        format!(" · {}", crate::authoring::NO_AUTHORING)
    })
}

/// A connection `census` holds that no probed model of it offers: `key`, read as `label`,
/// when the census takes that key and no option names a model under it.
fn unprobed(options: &mut Vec<Offer>, census: &IntelligenceCensus, key: String, label: String) {
    let under = format!("{key}/");
    if census.choose(&key).is_ok() && !options.iter().any(|o| o.key.starts_with(&under)) {
        options.push(Offer::new(key, label, false));
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::reasoner::NoReasoner;
    use nika_providers::probe::{ExecutionLocus, ProviderProbe, ProviderReadiness};
    use nika_types::access::AccessClass;

    /// A keyed provider at its vendor endpoint: the catalogue lists its models.
    fn probe(id: &str, endpoint: &str) -> ProviderProbe {
        let locus = ExecutionLocus::classify(Some(endpoint), endpoint);
        let readiness =
            ProviderReadiness::new(true, true, None, None, false, locus, AccessClass::Api);
        let var = format!("{}_API_KEY", id.to_uppercase());
        ProviderProbe::new(id, true, true, var, true, readiness, endpoint)
    }

    const ANTHROPIC: (&str, &str) = ("anthropic", "https://api.anthropic.com/v1/messages");
    const DEEPSEEK: (&str, &str) = ("deepseek", "https://api.deepseek.com/chat/completions");
    const MISTRAL: (&str, &str) = ("mistral", "https://api.mistral.ai/v1/chat/completions");

    /// These providers keyed, nothing chosen yet, the operator's home `home`.
    fn keyed(
        providers: &[(&str, &str)],
        home: Option<&std::path::Path>,
    ) -> (tempfile::TempDir, SessionRuntime) {
        let mut census = IntelligenceCensus::empty();
        for (id, endpoint) in providers {
            census.api_keys.push((*id).to_owned());
            census.provider_context.push(probe(id, endpoint));
        }
        let root = tempfile::tempdir().expect("project");
        let session = SessionRuntime::open_unchosen(
            root.path(),
            census,
            home,
            Box::new(|_| Box::new(NoReasoner)),
        );
        (root, session)
    }

    /// Anthropic and `DeepSeek`, both keyed, nothing chosen yet.
    fn session() -> (tempfile::TempDir, SessionRuntime) {
        keyed(&[ANTHROPIC, DEEPSEEK], None)
    }

    fn recommended(session: &SessionRuntime) -> Option<String> {
        session.recommended(&session.intelligence_options())
    }

    /// A subscription seat that cannot write workflows says so on its row before any choice,
    /// and one that can does not (live gauntlet r08 B1: copilot accepted, then refused as « not
    /// infer-grade for `json_schema` » only once chosen).
    #[test]
    fn a_seat_that_cannot_author_says_so_on_its_row() {
        let (_root, mut session) = keyed(&[], None);
        let census = session.census.as_mut().expect("a census");
        for seat in ["copilot", "claude-code"] {
            census.seats.push(crate::intelligence::SeatSeen::new(
                seat.to_owned(),
                true,
                true,
                true,
            ));
        }
        let options = session.intelligence_options();
        let label = |key: &str| {
            (options.iter())
                .find(|option| option.key == key)
                .map(|option| option.label.clone())
        };
        assert_eq!(
            label("1 copilot").as_deref(),
            Some("copilot · agent account · cannot write workflows here"),
            "{options:#?}"
        );
        assert_eq!(
            label("1 claude-code").as_deref(),
            Some("claude-code · agent account"),
            "{options:#?}"
        );
    }

    /// Without a saved default, an Anthropic key at this catalogue yields Claude Sonnet 5.5:
    /// the first configured provider's default model, in the catalogue's provider order.
    #[test]
    fn an_anthropic_key_recommends_claude_sonnet_5_5() {
        let (_root, session) = session();
        assert_eq!(
            recommended(&session).as_deref(),
            Some("2 anthropic/claude-sonnet-5-5")
        );
        let (_root, alone) = keyed(&[ANTHROPIC], None);
        assert_eq!(
            recommended(&alone).as_deref(),
            Some("2 anthropic/claude-sonnet-5-5")
        );
        let (_root, deepseek) = keyed(&[DEEPSEEK], None);
        assert_eq!(
            recommended(&deepseek).as_deref(),
            Some("2 deepseek/deepseek-flash")
        );
        let (_root, none) = keyed(&[], None);
        assert_eq!(
            recommended(&none),
            None,
            "nothing configured, nothing offered"
        );
    }

    /// The operator's saved default comes first while this machine serves it; one it cannot
    /// serve leaves the catalogue's pick.
    #[test]
    fn a_servable_saved_default_comes_first() {
        let home = tempfile::tempdir().expect("home");
        let saved = UserIntelligencePreference::new(
            IntelligenceKind::Api {
                provider: "deepseek".to_owned(),
            },
            Some("deepseek/deepseek-v4-pro".to_owned()),
        );
        saved.save(home.path()).expect("saved default");
        let (_root, session) = keyed(&[ANTHROPIC, DEEPSEEK], Some(home.path()));
        assert_eq!(
            recommended(&session).as_deref(),
            Some("2 deepseek/deepseek-v4-pro")
        );
        let work = session.work();
        assert_eq!(
            work.intelligence.and_then(|i| i.recommended),
            None,
            "no picker open"
        );
        let (_root, unserved) = keyed(&[ANTHROPIC], Some(home.path()));
        assert_eq!(
            recommended(&unserved).as_deref(),
            Some("2 anthropic/claude-sonnet-5-5"),
            "no DeepSeek key here"
        );
    }

    fn facts_of<'a>(options: &'a [Offer], key: &str) -> &'a ModelFacts {
        (options.iter().find(|offer| offer.key == key))
            .and_then(|offer| offer.values.first())
            .and_then(|value| value.choice.as_ref())
            .unwrap_or_else(|| panic!("{key}: {options:#?}"))
    }

    fn prices(facts: &ModelFacts) -> (Option<f64>, Option<f64>) {
        (
            facts.input_usd_per_million.map(|price| price.0),
            facts.output_usd_per_million.map(|price| price.0),
        )
    }

    /// Each model row carries typed facts: both prices from the qualified tariff where one admits
    /// the route, else the catalogue's list row on a metered route; the model's context window;
    /// the legacy mark of a model scheduled to retire. No provider is called.
    #[test]
    fn each_model_row_carries_its_prices_context_and_lifecycle() {
        let (_root, session) = session();
        let options = session.intelligence_options();
        let tariff = facts_of(&options, "2 deepseek/deepseek-flash");
        assert_eq!(prices(tariff), (Some(0.3), Some(1.2)), "{tariff:?}");
        assert_eq!(
            (tariff.context_tokens, tariff.prices_up_to_tokens),
            (Some(1_000_000), None),
            "the model's sourced window, which its tariff covers"
        );
        assert_eq!(
            (
                tariff.role.as_str(),
                tariff.via.as_str(),
                tariff.class.as_str()
            ),
            ("author", "deepseek", "api")
        );
        assert!(!tariff.legacy);
        let sonnet = facts_of(&options, "2 anthropic/claude-sonnet-5-5");
        assert_eq!(prices(sonnet), (Some(2.0), Some(10.0)), "{sonnet:?}");
        assert_eq!(
            (sonnet.context_tokens, sonnet.prices_up_to_tokens),
            (Some(1_000_000), None),
            "the model's window, priced across"
        );
        assert!(!sonnet.legacy);
        let (_root, mistral) = keyed(&[MISTRAL], None);
        let mistral = mistral.intelligence_options();
        let listed = facts_of(&mistral, "2 mistral/mistral-large-latest");
        assert_eq!(
            prices(listed),
            (Some(0.5), Some(1.5)),
            "the list row: {listed:?}"
        );
        assert_eq!(
            (listed.context_tokens, listed.prices_up_to_tokens),
            (Some(262_144), None),
            "a curated provider row with no source yields to the list row"
        );
        let retiring = facts_of(&options, "2 anthropic/claude-haiku-4-5-20251001");
        assert!(retiring.legacy, "{retiring:?}");
        let value = (options
            .iter()
            .find(|offer| offer.key == "2 deepseek/deepseek-flash"))
        .and_then(|offer| offer.values.first())
        .expect("a value");
        assert_eq!(value.value, "deepseek/deepseek-flash");
        assert_eq!(value.role, ValueRole::Value);
    }

    /// Claude Haiku 5.5's tariff prices prompts up to 100,000 tokens of its million-token window:
    /// its row states the model's window and how far the prices hold, never the tier as its
    /// context.
    #[test]
    fn a_tiered_tariff_states_the_window_and_how_far_its_prices_hold() {
        let (_root, session) = keyed(&[ANTHROPIC], None);
        let options = session.intelligence_options();
        let haiku = facts_of(&options, "2 anthropic/claude-haiku-5-5");
        assert_eq!(prices(haiku), (Some(0.1), Some(0.5)), "{haiku:?}");
        assert_eq!(
            (haiku.context_tokens, haiku.prices_up_to_tokens),
            (Some(1_000_000), Some(100_000))
        );
    }
}
