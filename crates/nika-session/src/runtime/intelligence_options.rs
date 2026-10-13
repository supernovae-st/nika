// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Read-only choices for the native intelligence picker. Selection still uses the census's
//! existing parser and resolution; a row never establishes reachability or calls a provider.

use super::SessionRuntime;
use crate::work::Offer;
use nika_providers::model_choice::{ModelInventory, ModelRole};

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
            let setup = if key_missing {
                key_required(&offer.route.access).map(|why| format!(" · {why}"))
            } else {
                None
            };
            options.push(Offer::new(
                key,
                format!(
                    "{model} · {} · {}{}{}",
                    offer.route.access,
                    offer.route.billing.as_str(),
                    if selected { " · current" } else { "" },
                    setup.unwrap_or_default()
                ),
                selected,
            ));
        }
        // A census built without full catalogue probes still offers its actual connections.
        for provider in &census.api_keys {
            let key = format!("2 {provider}");
            if census.choose(&key).is_ok()
                && !options
                    .iter()
                    .any(|o| o.key.starts_with(&format!("{key}/")))
            {
                options.push(Offer::new(
                    key,
                    format!("{provider} API · configured default model · metered"),
                    false,
                ));
            }
        }
        offer_missing_apis(&mut options, &census.api_keys);
        for seat in &census.seats {
            let key = format!("1 {}", seat.id);
            if census.choose(&key).is_ok()
                && !options
                    .iter()
                    .any(|o| o.key.starts_with(&format!("{key}/")))
            {
                options.push(Offer::new(
                    key,
                    format!("{} · agent account", seat.id),
                    false,
                ));
            }
        }
        for provider in &census.locals {
            let key = format!("3 {provider}");
            if census.choose(&key).is_ok()
                && !options
                    .iter()
                    .any(|o| o.key.starts_with(&format!("{key}/")))
            {
                options.push(Offer::new(
                    key,
                    format!("{provider} · configured local route (not probed)"),
                    false,
                ));
            }
        }
        options.sort_by_key(|option| !option.recommended);
        options
    }
}
