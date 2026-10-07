// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The decision model a door that holds a seat for many rounds (Serve) opens once, in the
//! `--decision-model` words of `nika compile`: `typesafe/<jev>` through the System One adapter
//! the caller's host opens (`nika_cli_host::compile::typesafe::seat`), or a direct
//! `provider/name`. A harness or a name that does not resolve refuses; the key it resolved is
//! handed back to be withheld, never printed.

use std::sync::Arc;
use std::time::Duration;

use nika_kernel::ai::provider::ProviderInferDyn;
use nika_kernel::secret::Secret;
use nika_providers::{ProviderRegistry, ProvidersConfig};

use crate::compile::AuthoringReasoning;
use crate::compile::decide::{DecisionSeat, ProviderChoice};

/// A System One seat the host opened: the seat, and its key to withhold.
pub type Opened = (Box<dyn DecisionSeat + Send + Sync>, Secret);

/// The seat one round passes the core, borrowed from a [`DecisionModel`].
pub enum Judge<'a, P: ProviderInferDyn> {
    /// The System One seat.
    Typesafe(&'a (dyn DecisionSeat + Send + Sync)),
    /// The provider, asked through a closed JSON-schema choice.
    Provider(ProviderChoice<'a, P>),
}

impl<P: ProviderInferDyn> Judge<'_, P> {
    /// The seat, as the core reads it.
    #[must_use]
    pub fn seat(&self) -> &dyn DecisionSeat {
        match self {
            Self::Typesafe(seat) => *seat,
            Self::Provider(choice) => choice,
        }
    }
}

/// A decision model, opened.
#[non_exhaustive]
pub enum DecisionModel {
    /// A `typesafe/<jev>` System One seat (its name kept): one request per question, its own
    /// client.
    Typesafe(Box<dyn DecisionSeat + Send + Sync>, String),
    /// A direct `provider/name` model, resolved per round through the provider client.
    Provider(String),
}

impl DecisionModel {
    /// Open `model`; the key it resolved (`TYPESAFE_API_KEY`, or the provider's) is returned to
    /// be withheld.
    ///
    /// # Errors
    /// A harness seat, a name that is not `provider/name`, an unknown provider or a missing key.
    pub fn open(
        model: &str,
        providers: &ProvidersConfig,
        typesafe: impl FnOnce(&str) -> Result<Opened, String>,
    ) -> Result<(Self, Option<Secret>), String> {
        let decision = |reason: String| format!("as the decision model, {reason}");
        if let Some(jev) = model.strip_prefix("typesafe/") {
            let (seat, key) = typesafe(jev).map_err(decision)?;
            return Ok((Self::Typesafe(seat, model.to_owned()), Some(key)));
        }
        let (_, key) = direct_provider(model, providers).map_err(decision)?;
        Ok((Self::Provider(model.to_owned()), key))
    }

    /// The provider of a `provider/name` model, resolved for one round over the provider client
    /// (`None` for a System One seat), for the door to wrap in its own gate.
    ///
    /// # Errors
    /// The provider client cannot be built, or the model no longer resolves.
    pub fn resolve(
        &self,
        providers: &ProvidersConfig,
    ) -> Result<Option<impl ProviderInferDyn + use<>>, String> {
        let Self::Provider(model) = self else {
            return Ok(None);
        };
        let http = nika_runtime::compose::provider_http().map_err(|e| e.to_string())?;
        let registry = ProviderRegistry::new(Arc::new(http), providers.clone());
        registry.resolve(model).map(Some).map_err(|e| e.to_string())
    }

    /// The seat one round asks: this System One seat, or `provider` (the [`Self::resolve`]d
    /// one, behind the door's gate) through a closed choice at the round's bounds and effort.
    #[must_use]
    pub fn judge<'a, P: ProviderInferDyn>(
        &'a self,
        provider: Option<&'a P>,
        (timeout, max_tokens): (Duration, u32),
        reasoning: Option<AuthoringReasoning>,
    ) -> Option<Judge<'a, P>> {
        match (self, provider) {
            (Self::Typesafe(seat, _), _) => Some(Judge::Typesafe(seat.as_ref())),
            (Self::Provider(model), Some(provider)) => {
                let choice = ProviderChoice::new(provider, model, timeout, max_tokens);
                Some(Judge::Provider(match reasoning {
                    Some(level) => choice.with_reasoning(level, max_tokens),
                    None => choice,
                }))
            }
            (Self::Provider(_), None) => None,
        }
    }

    /// Name this seat in an authoring receipt's backend: the model, and `note` (the CLI's words
    /// for its client).
    pub fn stamp(&self, backend: &mut serde_json::Value, note: &str) {
        backend["authority"]["decision_seat"] = serde_json::json!(note);
        backend["decision_model"] = serde_json::json!(self.model());
    }

    /// The model as the operator named it.
    #[must_use]
    pub fn model(&self) -> String {
        match self {
            Self::Typesafe(_, model) | Self::Provider(model) => model.clone(),
        }
    }
}

/// A direct provider model that resolves now (known provider, its key present): its canonical id
/// and the key it resolved (`None` when keyless). A harness seat or any other name refuses,
/// never falls back.
///
/// # Errors
/// Why the model cannot be seated.
pub fn direct_provider(
    model: &str,
    providers: &ProvidersConfig,
) -> Result<(String, Option<Secret>), String> {
    let Some((id, _)) = model.split_once('/') else {
        return Err("name it `provider/name`".to_owned());
    };
    if nika_types::access::HarnessRuntime::lookup(id).is_some() {
        return Err(
            "a harness seat cannot author on the server; seat a direct provider model".to_owned(),
        );
    }
    let http = nika_runtime::compose::provider_http().map_err(|e| e.to_string())?;
    let resolved = ProviderRegistry::new(Arc::new(http), providers.clone())
        .resolve(model)
        .map_err(|e| e.to_string())?;
    let canonical = nika_providers::profile::canonical_provider(id).to_owned();
    Ok((canonical, resolved.key().cloned()))
}

/// Whether `document` carries a withheld value, raw or as a JSON string carries it (escaped):
/// every nonempty value counts, however short.
#[must_use]
pub fn discloses(withheld: &[Secret], document: &[u8]) -> bool {
    let contains = |needle: &[u8]| {
        !needle.is_empty()
            && document
                .windows(needle.len())
                .any(|window| window == needle)
    };
    // A value as the body of a JSON string spells it (the enclosing quotes removed).
    let escaped = |value: &str| {
        let quoted = serde_json::to_string(value).ok()?;
        Some(quoted.strip_prefix('"')?.strip_suffix('"')?.to_owned())
    };
    (withheld.iter().map(Secret::expose))
        .filter(|secret| !secret.is_empty())
        .any(|secret| {
            contains(secret.as_bytes()) || escaped(secret).is_some_and(|e| contains(e.as_bytes()))
        })
}
