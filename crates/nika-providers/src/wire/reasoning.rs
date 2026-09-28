// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! An explicit reasoning effort on the wire (R4 B16). A request that names a level carries it
//! only where two catalog facts qualify the route: the exact model documents that level
//! (`ModelCapabilities::reasoning_efforts`), and the endpoint is the exact direct route the
//! admission catalog binds for that provider and model (`InferenceTariff::new`). There the level
//! is written as the provider's two structural keys, `thinking: {type: enabled}` and
//! `reasoning_effort`, while the body is built: before any reservation and before a byte leaves,
//! on the streaming and the buffered door alike. Anywhere else (another provider, a gateway, an
//! unlisted model or level, a base-URL override, a wire that carries no such key) the request
//! refuses with nothing sent. A request that names no level keeps its route's own bytes.

use nika_kernel::ai::provider::{InferRequest, ProviderError, ReasoningEffort};
use serde_json::{Value, json};

/// Write the request's explicit level into `body` on a qualified route, or refuse the request.
///
/// # Errors
/// [`ProviderError::AdmissionDenied`] when the level is not qualified for this exact route, or
/// when a thinking budget or a raw reasoning key would ride beside it.
pub(crate) fn apply(
    body: &mut Value,
    req: &InferRequest,
    provider: &str,
    model: &str,
    endpoint: &str,
) -> Result<(), ProviderError> {
    let Some(level) = req.reasoning_effort else {
        return Ok(());
    };
    let word = level.word();
    if req.thinking_budget.is_some()
        || ["thinking", "reasoning_effort"]
            .into_iter()
            .any(|key| req.extra.params.contains_key(key))
    {
        return Err(refused(format!(
            "the explicit reasoning effort `{word}` cannot ride beside a thinking budget or a raw reasoning key; nothing was sent"
        )));
    }
    let listed = nika_catalog::model_capabilities(provider, model).reasoning_efforts;
    let direct = nika_catalog::admission::InferenceTariff::new(provider, model, endpoint).is_some();
    if !listed.iter().any(|l| l.word() == word) || !direct {
        return Err(refused(unqualified(level, provider, model, listed, direct)));
    }
    let Some(object) = body.as_object_mut() else {
        return Err(refused(format!(
            "the explicit reasoning effort `{word}` has no request object to ride in; nothing was sent"
        )));
    };
    object.insert("thinking".to_owned(), json!({"type": "enabled"}));
    object.insert("reasoning_effort".to_owned(), json!(word));
    Ok(())
}

/// A door that carries no `reasoning_effort` key (the Anthropic and Gemini wires, the mock)
/// refuses any explicit level before it produces anything.
///
/// # Errors
/// [`ProviderError::AdmissionDenied`] when the request names a level.
pub(crate) fn unsupported(
    req: &InferRequest,
    provider: &str,
    model: &str,
) -> Result<(), ProviderError> {
    match req.reasoning_effort {
        Some(level) => Err(refused(format!(
            "the explicit reasoning effort `{}` is not qualified for {provider}/{model}: this wire carries no reasoning effort; nothing was sent",
            level.word()
        ))),
        None => Ok(()),
    }
}

/// Why an explicit level is not qualified here, naming both catalog facts.
fn unqualified(
    level: ReasoningEffort,
    provider: &str,
    model: &str,
    listed: &[nika_catalog::types::model::ReasoningLevel],
    direct: bool,
) -> String {
    let levels = if listed.is_empty() {
        "no effort level".to_owned()
    } else {
        listed
            .iter()
            .map(|l| l.word())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let route = if direct {
        "its exact direct endpoint"
    } else {
        "no exact direct endpoint the catalog binds for it"
    };
    format!(
        "the explicit reasoning effort `{}` is not qualified for {provider}/{model}: the model catalog lists {levels} for it, and this route is {route}; nothing was sent",
        level.word()
    )
}

fn refused(reason: String) -> ProviderError {
    ProviderError::AdmissionDenied { reason }
}

#[cfg(test)]
mod tests;
