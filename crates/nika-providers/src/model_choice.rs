// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The models each role can be served by, and the validation of one selection — a capability
//! an agent lists and calls. The agent reads the person's words in context and selects; the
//! engine admits only what the inventory holds: no fabricated id, no change to a provider,
//! route or protocol the selection closed, no read across roles. Pure over its inventory.

use std::collections::{BTreeMap, BTreeSet};

use nika_types::access::{AccessClass, AccessProtocol};

use crate::probe::{ExecutionLocus, ProviderProbe};
use crate::profile::{canonical_provider, pricing_provider_matches};
use crate::resolve_access::{AccessCandidate, candidates_for, judge, pin_matches, provider_of};

/// The selection a choice decides. Each role keeps its own offers, so deciding the workflow's
/// model never reads or moves the Author or decision selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ModelRole {
    /// The workflow's `model:` and `run.access`: what a Run calls.
    Run,
    /// The conversation that authors with the person.
    Author,
    /// The decision seat that makes the compiler's typed choices.
    Decision,
}

/// One model over one route: a row an agent lists and selects.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ModelOffer {
    /// The exact `provider/name` the engine resolves.
    pub model: String,
    /// The route (`run.access.via` names its `access`): class, readiness and fix.
    pub route: AccessCandidate,
    /// The catalogue's output list price (USD per million tokens) on a metered route; `None`
    /// is unknown or unmetered, never free.
    pub output_usd_per_million: Option<f64>,
}

impl ModelOffer {
    /// Construct (INV-019).
    #[must_use]
    pub fn new(model: impl Into<String>, route: AccessCandidate, price: Option<f64>) -> Self {
        Self {
            model: model.into(),
            route,
            output_usd_per_million: price,
        }
    }
}

/// What an agent selected for one role, in the inventory's own words: each `Some` closes a
/// dimension, each `None` leaves it open.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ModelAsk {
    /// A provider id or catalogue alias (`deepseek`).
    pub provider: Option<String>,
    /// A model as an offer's `provider/name` or its bare name.
    pub model: Option<String>,
    /// A route id (`deepseek` · `claude-code`) or class (`api` · `harness`).
    pub via: Option<String>,
    /// The protocol.
    pub protocol: Option<AccessProtocol>,
    /// The pick the person delegated among the offers that fit.
    pub delegate: Option<Delegation>,
}

impl ModelAsk {
    /// An open selection (INV-019); a caller closes its dimensions field by field.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// A pick the person delegated among the offers that fit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Delegation {
    /// The strongest. The catalogue marks no per-model tier: the rule applied is the highest
    /// output list price among the ready offers that fit, recorded as such.
    Strongest,
}

/// The engine's answer to one selection for one role.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ModelChoice {
    /// Exactly one ready offer fits.
    Exact(ModelOffer),
    /// The delegated rule picked one offer.
    Delegated {
        /// The offer picked.
        chosen: ModelOffer,
        /// The rule as applied.
        rule: &'static str,
        /// Every ready offer that fit, in inventory order.
        among: Vec<ModelOffer>,
    },
    /// Several ready offers fit and no rule ranks them: one targeted question.
    Choose(Vec<ModelOffer>),
    /// No ready offer fits.
    Unsupported {
        /// Why, with the route's own fix when an offer fits but is not ready.
        why: String,
        /// Ready offers keeping every closed dimension but the model, else all ready ones.
        alternatives: Vec<ModelOffer>,
    },
}

/// The offers of each role. This machine's routes serve the Run and Author roles; a decision
/// seat is its host's own and enters through [`Self::with_offers`].
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct ModelInventory {
    roles: BTreeMap<ModelRole, Vec<ModelOffer>>,
}

impl ModelInventory {
    /// This machine's Run and Author offers: every route a probe row gives a provider (its own
    /// row or a seat serving it) with the route's observed listing, else the catalogue's models
    /// on a vendor endpoint or a seat. An unobserved local or operator-pointed endpoint offers
    /// nothing; a seat's acceptance of a model is judged where it runs.
    #[must_use]
    pub fn from_probes(probes: &[ProviderProbe]) -> Self {
        let mut offers = Vec::new();
        for provider in crate::profile::CANONICAL_IDS {
            for route in candidates_for(probes, provider) {
                let probe = probes.iter().find(|p| p.id == route.access);
                for name in route_models(probe, provider) {
                    let price = nika_catalog::find_pricing_scoped(provider, &name)
                        .filter(|_| route.billing.is_usd_metered())
                        .map(|row| row.output_per_million);
                    let model = format!("{provider}/{name}");
                    offers.push(ModelOffer::new(model, route.clone(), price));
                }
            }
        }
        Self::default()
            .with_offers(ModelRole::Run, offers.clone())
            .with_offers(ModelRole::Author, offers)
    }

    /// Replace one role's offers (a host's own seat · a fixture); other roles keep theirs.
    #[must_use]
    pub fn with_offers(mut self, role: ModelRole, offers: Vec<ModelOffer>) -> Self {
        self.roles.insert(role, offers);
        self
    }

    /// One role's offers, in inventory order: the listing an agent reads.
    #[must_use]
    pub fn offers(&self, role: ModelRole) -> &[ModelOffer] {
        self.roles.get(&role).map_or(&[][..], Vec::as_slice)
    }

    /// Validate one selection against one role's offers only.
    #[must_use]
    pub fn choose(&self, role: ModelRole, ask: &ModelAsk) -> ModelChoice {
        let offers = self.offers(role);
        let fit: Vec<&ModelOffer> = offers.iter().filter(|o| fits(o, ask, true)).collect();
        let ready = fit.iter().copied().filter(|o| o.route.configured);
        let mut ready: Vec<ModelOffer> = ready.cloned().collect();
        let chosen = ask.delegate.and_then(|_| strongest(&ready));
        match (ready.len(), chosen) {
            (0, _) => unsupported(role, ask, offers, fit.first().copied()),
            (1, _) => ModelChoice::Exact(ready.remove(0)),
            (_, Some(chosen)) => ModelChoice::Delegated {
                chosen,
                rule: "highest list price among ready offers",
                among: ready,
            },
            (_, None) => ModelChoice::Choose(ready),
        }
    }
}

/// Whether an offer keeps every dimension the selection closed (the model only `by_model`):
/// exact ids and catalogue provider aliases, never a near name.
fn fits(offer: &ModelOffer, ask: &ModelAsk, by_model: bool) -> bool {
    let provider = provider_of(&offer.model);
    let name = offer.model.split_once('/').map_or("", |(_, name)| name);
    let model = |asked: &str| match asked.split_once('/') {
        Some((p, n)) => canonical_provider(p) == provider && n.eq_ignore_ascii_case(name),
        None => asked.eq_ignore_ascii_case(name),
    };
    let same = |p: &str| canonical_provider(p) == provider;
    kept(ask.provider.as_deref(), same)
        && (!by_model || kept(ask.model.as_deref(), model))
        && kept(ask.via.as_deref(), |via| pin_matches(via, &offer.route))
        && ask.protocol.is_none_or(|p| p.admits(offer.route.class))
}

/// Whether a dimension is open, or closed on a value `keep` accepts.
fn kept(closed: Option<&str>, keep: impl Fn(&str) -> bool) -> bool {
    closed.is_none_or(keep)
}

/// The single highest-priced offer; `None` when one is unpriced or the top price is shared.
fn strongest(offers: &[ModelOffer]) -> Option<ModelOffer> {
    let price = |o: &ModelOffer| o.output_usd_per_million;
    let top = offers.iter().map(price).reduce(|a, b| Some(a?.max(b?)))??;
    let at = |o: &&ModelOffer| price(o).is_some_and(|p| p.total_cmp(&top).is_eq());
    let mut at_top = offers.iter().filter(at);
    let chosen = at_top.next()?;
    at_top.next().is_none().then(|| chosen.clone())
}

/// The refusal: an unready route's own witness, else the closed words nothing fits.
fn unsupported(
    role: ModelRole,
    ask: &ModelAsk,
    offers: &[ModelOffer],
    unready: Option<&ModelOffer>,
) -> ModelChoice {
    let witness = unready.and_then(|o| {
        let line = judge(&o.route, provider_of(&o.model), None, None)?.witness_line();
        Some(format!("`{}` is not ready here: {line}", o.model))
    });
    let closed = [&ask.provider, &ask.model, &ask.via].into_iter().flatten();
    let mut words: Vec<&str> = closed.map(String::as_str).collect();
    words.extend(ask.protocol.map(AccessProtocol::as_str));
    let words = words.join(" · ");
    let why = witness.unwrap_or_else(|| format!("no {role:?} offer here fits `{words}`"));
    let ready = offers.iter().filter(|o| o.route.configured);
    let keeps = ready.clone().filter(|o| fits(o, ask, false));
    let mut alternatives: Vec<ModelOffer> = keeps.cloned().collect();
    if alternatives.is_empty() {
        alternatives = ready.cloned().collect();
    }
    ModelChoice::Unsupported { why, alternatives }
}

/// The model names a route offers for `provider`: its observed listing; else the catalogue's
/// (row ids, then non-deprecated priced ids) on a vendor endpoint, a seat or the mock; else none.
fn route_models(probe: Option<&ProviderProbe>, provider: &str) -> Vec<String> {
    let listing = probe.and_then(|p| p.readiness.model_listing.as_ref());
    if let Some(listing) = listing.filter(|l| l.protocol_compatible == Some(true)) {
        return listing.models.clone();
    }
    let vendor = |p: &ProviderProbe| p.readiness.execution_locus == ExecutionLocus::Cloud;
    if !probe.is_none_or(|p| p.readiness.access == AccessClass::Harness || vendor(p)) {
        return Vec::new();
    }
    let row = nika_catalog::find_provider(provider).map_or(&[][..], |row| row.models);
    let priced = nika_catalog::all_pricing()
        .iter()
        .filter(|p| !p.is_deprecated() && pricing_provider_matches(p.provider, provider))
        .map(|p| p.model_pattern);
    let mut seen = BTreeSet::new();
    let names = row.iter().map(|m| m.model).chain(priced);
    let fresh = names.filter(|n| seen.insert(*n));
    fresh.map(str::to_owned).collect()
}

#[cfg(test)]
mod tests;
