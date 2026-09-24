// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Output caps and the seat's catalog row. A reasoning model may think before it answers, and
//! a route may count that thinking inside `max_tokens`; the compiler cannot know which route a
//! run will use, so it never claims it. It sizes only the caps it generates itself. A
//! candidate's cap may be the human's and nothing in the candidate proves otherwise, so it is
//! never rewritten: the authoring seat receives the recorded facts (and their uncertainty)
//! before it writes, and the native door admits the candidate against the catalog's judges. A
//! cap is not an output length (« three lines » stays in the prompt), and nothing here reads or
//! moves a cost ceiling.

use serde_json::{Value, json};

use crate::{CompileOutcome, DiagnosticKind};

/// The generated cap on a seat the catalog records as reasoning: room for a thinking trace
/// before the visible answer. 1200 was measured too small (openai/gpt-5-mini · `NIKA-INFER-002
/// · cut off at the token limit`); 4096 leaves the answer its room. A cap is a ceiling the run
/// never exceeds, never a spend.
pub(crate) const REASONING_MAX_TOKENS: u32 = 4096;

/// A seat's provider and model when the catalog can judge it: never `mock` (its fixture row
/// claims every capability) and never a template (`nika check` judges the resolved seat).
fn judged(seat: &str) -> Option<(&str, &str)> {
    if seat.contains("${{") {
        return None;
    }
    seat.split_once('/')
        .filter(|(provider, _)| *provider != "mock")
}

/// The output limit the catalog records for a seat, with where it is recorded: a capability
/// rule's, else the provider row's for that exact model id. `None` when the catalog records
/// none; a route's own limit (an endpoint-bound tariff) is only known at run time.
fn known_limit(provider: &str, name: &str) -> Option<(u32, String)> {
    let rule = nika_catalog::model_capabilities(provider, name).max_output_tokens;
    if let Some(limit) = rule.filter(|limit| *limit > 0) {
        return Some((limit, format!("capability rule {provider}/{name}")));
    }
    let row = nika_catalog::find_provider(provider)?;
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

/// The sizing law for a cap the compiler generates: the reasoning floor first, then the known
/// output limit.
fn bounded(reasoning: bool, limit: Option<u32>, base: u32) -> u32 {
    let wanted = if reasoning {
        base.max(REASONING_MAX_TOKENS)
    } else {
        base
    };
    limit.map_or(wanted, |limit| wanted.min(limit))
}

/// A cap the assembler generates, sized for the doc's seat from its catalog records alone:
/// raised when a rule records the model as reasoning, never above the recorded output limit.
/// Any other seat keeps the base.
pub(crate) fn sized(seat: Option<&str>, base: u32) -> u32 {
    let Some((provider, name)) = seat.and_then(judged) else {
        return base;
    };
    let reasoning = nika_catalog::model_capabilities(provider, name).reasoning;
    let limit = known_limit(provider, name).map(|(limit, _)| limit);
    bounded(reasoning, limit, base)
}

/// The catalog's output facts for the workflow's model, given to the authoring seat before it
/// writes a candidate. `reasoning_capability` is `recorded` only when a catalog rule records
/// it; the catalog default is no evidence, so every other model is `unrecorded`, never
/// non-reasoning. Whether thinking counts inside the cap belongs to the route the run
/// resolves, so it stays `unknown` here. `None` only for `mock` and templated seats.
#[must_use]
pub fn output_caps(model: &str) -> Option<Value> {
    let (provider, name) = judged(model)?;
    let reasoning = nika_catalog::model_capabilities(provider, name).reasoning;
    let limit = known_limit(provider, name);
    let bound = limit.as_ref().map(|(limit, _)| *limit);
    let evidence = if reasoning { "recorded" } else { "unrecorded" };
    let suggested = reasoning.then_some(bounded(true, bound, REASONING_MAX_TOKENS));
    Some(json!({
        "model": model,
        "reasoning_capability": evidence,
        "thinking_counted_in_cap": "unknown",
        "route": "resolved at run time",
        "suggested_default_max_tokens": suggested,
        "max_output_tokens": bound,
        "max_output_tokens_source": limit.map(|(_, source)| source),
    }))
}

/// The model-neutral cap the compiler generates for a language task that states none: room for
/// a thinking trace before the answer, whatever the model. Bounded by the recorded output limit
/// once the workflow's model is seated.
pub(crate) const NEUTRAL_MAX_TOKENS: u32 = 4096;

/// Fills the cap of every `infer:` task that states none, once the workflow's model is seated.
/// Nobody named a bound there, so the default is the compiler's: the neutral cap, bounded by
/// the recorded output limit. A stated cap is never touched. The edit goes through the
/// presentation-preserving emitter; `None` when nothing is missing or the edit cannot be proven
/// safe (the candidate then stays as written, and Check says it is unbounded).
pub(crate) fn fill_defaults(source: &str) -> Option<String> {
    let before = crate::edit::literal_projection(source)?;
    let mut after = before.clone();
    let envelope = after["model"].as_str().map(str::to_owned);
    let mut filled = false;
    for task in after["tasks"].as_object_mut()?.values_mut() {
        let Some(infer) = task.get_mut("infer").and_then(Value::as_object_mut) else {
            continue;
        };
        if infer.contains_key("max_tokens") {
            continue;
        }
        let seat = infer
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| envelope.clone());
        let limit = seat
            .as_deref()
            .and_then(judged)
            .and_then(|(provider, name)| known_limit(provider, name))
            .map(|(limit, _)| limit);
        let cap = bounded(false, limit, NEUTRAL_MAX_TOKENS);
        infer.insert("max_tokens".to_owned(), json!(cap));
        filled = true;
    }
    if !filled {
        return None;
    }
    crate::edit::emit_preserving(source, &before, &after)
        .ok()
        .flatten()
}

/// The native door's admission of a seated candidate against the catalog's judges: a cap above
/// the seat's known output limit, a reasoning seat under its floor, a thinking block on a seat
/// that cannot reason. Each one is a refusal that names its repair. No cap is rewritten,
/// because nothing proves who chose it. A candidate that does not parse is left to `finish`.
pub(crate) fn admit(source: &str, out: &mut CompileOutcome) {
    let Ok(wf) = crate::parse(source) else {
        return;
    };
    let capacity = nika_check::capacity_findings(&wf)
        .into_iter()
        .map(|f| (f.task, f.why));
    let thinking = nika_check::thinking_findings(&wf)
        .into_iter()
        .map(|f| (f.task, f.why));
    for (task, why) in capacity.chain(thinking) {
        crate::finding(out, DiagnosticKind::Refused, &task, why);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(provider: &str, name: &str) -> (bool, Option<u32>) {
        let reasoning = nika_catalog::model_capabilities(provider, name).reasoning;
        (
            reasoning,
            known_limit(provider, name).map(|(limit, _)| limit),
        )
    }

    #[test]
    fn the_law_lifts_a_generated_reasoning_cap_and_never_passes_a_known_limit() {
        // A generated extraction (800), draft (1200) and validation (400) cap.
        for base in [400, 800, 1200] {
            assert_eq!(bounded(true, None, base), REASONING_MAX_TOKENS);
            assert_eq!(bounded(false, None, base), base);
        }
        assert_eq!(bounded(true, None, 6000), 6000, "a larger cap stays");
        assert_eq!(bounded(true, Some(2048), 800), 2048, "the known limit wins");
        assert_eq!(bounded(false, Some(512), 800), 512);
    }

    #[test]
    fn a_generated_cap_is_sized_from_the_seat_records_alone() {
        let (reasoning, limit) = row("deepseek", "deepseek-flash");
        assert!(reasoning, "the catalog records deepseek-flash as thinking");
        assert_eq!(limit, Some(384_000), "the deepseek row's flash limit");
        for base in [800, 1200] {
            assert_eq!(
                sized(Some("deepseek/deepseek-flash"), base),
                REASONING_MAX_TOKENS
            );
        }
        // The Scaleway-served name is recorded as reasoning; no route limit is claimed here.
        assert_eq!(row("openai", "gpt-oss-120b"), (true, None));
        assert_eq!(
            sized(Some("openai/gpt-oss-120b"), 800),
            REASONING_MAX_TOKENS
        );
        for seat in [
            Some("acme/unheard-of-model"),
            Some("mock/echo"),
            Some("${{ inputs.seat }}"),
            Some("no-provider"),
            None,
        ] {
            assert_eq!(sized(seat, 800), 800, "{seat:?}");
        }
        let (reasoning, limit) = row("deepseek", "deepseek-chat");
        assert_eq!(
            sized(Some("deepseek/deepseek-chat"), 800),
            bounded(reasoning, limit, 800)
        );
    }

    #[test]
    fn the_authoring_facts_keep_their_source_and_their_uncertainty() {
        let flash = output_caps("deepseek/deepseek-flash").expect("a literal seat");
        assert_eq!(flash["reasoning_capability"], json!("recorded"));
        assert_eq!(flash["thinking_counted_in_cap"], json!("unknown"));
        assert_eq!(
            flash["suggested_default_max_tokens"],
            json!(REASONING_MAX_TOKENS)
        );
        assert_eq!(flash["max_output_tokens"], json!(384_000));
        assert_eq!(
            flash["max_output_tokens_source"],
            json!("provider row deepseek/flash")
        );
        let unknown = output_caps("acme/unheard-of-model").expect("a literal seat");
        assert_eq!(unknown["reasoning_capability"], json!("unrecorded"));
        assert_eq!(unknown["suggested_default_max_tokens"], Value::Null);
        assert_eq!(unknown["max_output_tokens"], Value::Null);
        let oss = output_caps("openai/gpt-oss-120b").expect("a literal seat");
        assert_eq!(oss["reasoning_capability"], json!("recorded"));
        assert_eq!(
            oss["max_output_tokens"],
            Value::Null,
            "the route limit is a run fact"
        );
        for model in ["mock/echo", "${{ inputs.seat }}", "echo"] {
            assert_eq!(output_caps(model), None, "{model}");
        }
    }

    #[test]
    fn a_missing_cap_gets_the_neutral_default_and_a_stated_cap_is_kept() {
        let source = "nika: fill\nmodel: deepseek/deepseek-flash\ntasks:\n  draft:\n    infer:\n      prompt: \"Say hi in three lines\"\n  named:\n    infer:\n      prompt: \"Say hi\"\n      max_tokens: 700\n";
        let filled = fill_defaults(source).expect("one cap is missing");
        let doc = crate::edit::literal_projection(&filled).expect("a document");
        assert_eq!(
            doc["tasks"]["draft"]["infer"]["max_tokens"],
            json!(NEUTRAL_MAX_TOKENS)
        );
        assert_eq!(doc["tasks"]["named"]["infer"]["max_tokens"], json!(700));
        assert_eq!(
            doc["tasks"]["draft"]["infer"]["prompt"],
            json!("Say hi in three lines")
        );
        assert_eq!(fill_defaults(&filled), None, "nothing is missing any more");
        assert_eq!(
            bounded(false, Some(2048), NEUTRAL_MAX_TOKENS),
            2048,
            "a recorded limit"
        );
    }
}
