// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What the catalog says of a model a person names or a seat a host offers (descended from
//! `nika-session` on 2026-09-29, C11): a cloud model the catalog does not price, which a run
//! under a spending ceiling would refuse (NIKA-1709), and the priced models of its provider; and
//! the static table of each provider's stronger authoring model. Pure: a name in, facts and
//! neutral words out. A host's own gestures (take the seat, cancel, the question that still
//! waits), the form of its code citation and its registry reads stay with the host.

/// A CLOUD model the catalog does not price, as (the provider's row id, the model, the priced
/// `<row>/<model>` of that provider). `None` when the model is priced, when the provider is a
/// local engine (unpriced by nature, never refused), or when the name is not `provider/model`.
#[must_use]
pub fn unpriced_cloud(name: &str) -> Option<(String, String, Vec<String>)> {
    let (provider, model) = name.trim().split_once('/')?;
    let row = nika_catalog::all_providers()
        .iter()
        .find(|p| p.id == provider || p.aliases.contains(&provider))?;
    if !row.requires_key || nika_catalog::find_pricing_scoped(row.id, model).is_some() {
        return None;
    }
    let priced = row
        .models
        .iter()
        .filter(|m| nika_catalog::find_pricing_scoped(row.id, m.model).is_some())
        .map(|m| format!("{}/{}", row.id, m.model))
        .collect();
    Some((row.id.to_owned(), model.to_owned(), priced))
}

/// The fact a host says of an unpriced cloud model, as `shown`: a run under a spending ceiling
/// would refuse it. The host adds the code it cites and its own gesture.
#[must_use]
pub fn unpriced_warning(shown: &str) -> String {
    format!(
        "`{shown}` is not priced in Nika's catalog: a run under a spending ceiling would refuse it"
    )
}

/// The stronger authoring model of a provider — the escalation the product law permits
/// (quality first): `None` when the model already is the table's strongest, or the provider has
/// none in it.
#[must_use]
pub fn stronger_model(model: &str) -> Option<&'static str> {
    stronger_model_under(model, false)
}

/// The table behind [`stronger_model`], with the gateway fact explicit: an OpenAI-compatible base
/// URL (Scaleway's gateway, a local server) serves its OWN models under the `openai` provider id —
/// the provider's flagship is not there, so no escalation is offered across it. A static table,
/// relocated whole from `nika-session` (C11): it is written here, never read from the catalog when
/// called, and naming a model here grants it nothing.
#[must_use]
pub fn stronger_model_under(model: &str, openai_base_overridden: bool) -> Option<&'static str> {
    let (provider, name) = model.split_once('/')?;
    let strongest = match provider {
        "openai" if openai_base_overridden => return None,
        "openai" => "openai/gpt-5.2",
        "xai" => "xai/grok-4.7",
        "deepseek" => "deepseek/deepseek-v4-pro",
        "gemini" => "gemini/gemini-2.5-pro",
        "mistral" => "mistral/mistral-large-latest",
        _ => return None,
    };
    (format!("{provider}/{name}") != strongest).then_some(strongest)
}

/// The priced models of the provider `row`, joined, or that the catalog knows none yet.
#[must_use]
pub fn priced_words(row: &str, priced: &[String]) -> String {
    if priced.is_empty() {
        format!("no priced model is known for `{row}` yet")
    } else {
        format!("priced for `{row}`: {}", priced.join(" · "))
    }
}

#[cfg(test)]
mod tests;
