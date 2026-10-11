// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A model's output facts as the catalog records them: its output limit with where it is
//! recorded, and the facts an authoring seat reads before it writes a candidate. Descended from
//! `nika-compile`'s seat sizing (2026-10-11), which keeps the room it generates.

use crate::data::generated::find_provider;
use crate::data::models::model_capabilities;

/// The output limit the catalog records for `provider`/`name`, with where it is recorded: a
/// capability rule's, else the provider row's for that exact model id. `None` when the catalog
/// records none; a route's own limit (an endpoint-bound tariff) is only known at run time.
#[must_use]
pub fn model_output_limit(provider: &str, name: &str) -> Option<(u32, String)> {
    let rule = model_capabilities(provider, name).max_output_tokens;
    if let Some(limit) = rule.filter(|limit| *limit > 0) {
        return Some((limit, format!("capability rule {provider}/{name}")));
    }
    let row = find_provider(provider)?;
    let model = row
        .models
        .iter()
        .find(|m| m.id == name || m.model == name)?;
    (model.max_output_tokens > 0).then(|| {
        (
            model.max_output_tokens,
            format!("provider row {}/{}", row.id, model.id),
        )
    })
}

/// The sizing law for a cap a caller generates: `room` first on a reasoning model, then the
/// known output limit.
fn bounded(reasoning: bool, limit: Option<u32>, base: u32, room: u32) -> u32 {
    let wanted = if reasoning { base.max(room) } else { base };
    limit.map_or(wanted, |limit| wanted.min(limit))
}

/// The cap a caller generates for `provider`/`name` from the catalog's records alone: raised to
/// `room` when a rule records the model as reasoning, never above the recorded output limit.
#[must_use]
pub fn generated_cap(provider: &str, name: &str, base: u32, room: u32) -> u32 {
    let reasoning = model_capabilities(provider, name).reasoning;
    let limit = model_output_limit(provider, name).map(|(limit, _)| limit);
    bounded(reasoning, limit, base, room)
}

/// The catalog's output facts for `provider`/`name`, given to an authoring seat before it writes
/// a candidate. `reasoning_capability` is `recorded` only when a catalog rule records it; the
/// catalog default is no evidence, so every other model is `unrecorded`, never non-reasoning.
/// Whether thinking counts inside the cap belongs to the route the run resolves, so it stays
/// `unknown` here. `reasoning_efforts` lists the `run.reasoning.effort` levels the exact model
/// documents. `room` is the cap the caller generates on a reasoning model, suggested within the
/// recorded output limit.
#[cfg(feature = "serde")]
#[must_use]
pub fn output_caps(provider: &str, name: &str, room: u32) -> serde_json::Value {
    let caps = model_capabilities(provider, name);
    let limit = model_output_limit(provider, name);
    let bound = limit.as_ref().map(|(limit, _)| *limit);
    let evidence = if caps.reasoning {
        "recorded"
    } else {
        "unrecorded"
    };
    let suggested = caps
        .reasoning
        .then(|| bound.map_or(room, |limit| room.min(limit)));
    serde_json::json!({
        "model": format!("{provider}/{name}"),
        "reasoning_capability": evidence,
        "reasoning_efforts": caps.reasoning_efforts.iter().map(|l| l.word()).collect::<Vec<_>>(),
        "thinking_counted_in_cap": "unknown",
        "route": "resolved at run time",
        "suggested_default_max_tokens": suggested,
        "max_output_tokens": bound,
        "max_output_tokens_source": limit.map(|(_, source)| source),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The room a reasoning model's generated cap is lifted to here.
    const ROOM: u32 = 16_384;

    #[test]
    fn the_law_lifts_a_generated_reasoning_cap_and_never_passes_a_known_limit() {
        // A generated extraction (800), draft (1200) and validation (400) cap.
        for base in [400, 800, 1200] {
            assert_eq!(bounded(true, None, base, ROOM), ROOM);
            assert_eq!(bounded(false, None, base, ROOM), base);
        }
        assert_eq!(
            bounded(true, None, 24_000, ROOM),
            24_000,
            "a larger cap stays"
        );
        assert_eq!(
            bounded(true, Some(2048), 800, ROOM),
            2048,
            "the known limit wins"
        );
        assert_eq!(bounded(false, Some(512), 800, ROOM), 512);
        // From the records alone: a reasoning row lifted, an unknown model kept.
        assert_eq!(generated_cap("deepseek", "deepseek-flash", 800, ROOM), ROOM);
        assert_eq!(generated_cap("acme", "unheard-of-model", 800, ROOM), 800);
    }

    /// The provider row's limit for the exact model id, named; a route-served name records none
    /// here, and a model the catalog never records has none.
    #[test]
    fn the_output_limit_names_where_the_catalog_records_it() {
        let flash = model_output_limit("deepseek", "deepseek-flash");
        assert_eq!(
            flash,
            Some((384_000, "provider row deepseek/flash".to_owned()))
        );
        assert_eq!(model_output_limit("openai", "gpt-oss-120b"), None);
        assert_eq!(model_output_limit("acme", "unheard-of-model"), None);
    }

    /// The facts a seat reads: reasoning recorded with its documented efforts, the room
    /// suggested within the recorded limit (none recorded, the room as is), and an unknown model
    /// unrecorded, never non-reasoning.
    #[cfg(feature = "serde")]
    #[test]
    fn the_output_facts_suggest_the_room_within_the_recorded_limit() {
        let flash = output_caps("deepseek", "deepseek-flash", 16_384);
        assert_eq!(flash["model"], "deepseek/deepseek-flash");
        assert_eq!(flash["reasoning_capability"], "recorded");
        let efforts = serde_json::json!(["low", "high", "max"]);
        assert_eq!(flash["reasoning_efforts"], efforts);
        assert_eq!(flash["suggested_default_max_tokens"], 16_384);
        assert_eq!(flash["max_output_tokens"], 384_000);
        let oss = output_caps("openai", "gpt-oss-120b", 16_384);
        assert_eq!(oss["suggested_default_max_tokens"], 16_384);
        assert_eq!(oss["max_output_tokens"], serde_json::Value::Null);
        let unknown = output_caps("acme", "unheard-of-model", 16_384);
        assert_eq!(unknown["reasoning_capability"], "unrecorded");
        let none = serde_json::Value::Null;
        assert_eq!(unknown["suggested_default_max_tokens"], none);
    }
}
