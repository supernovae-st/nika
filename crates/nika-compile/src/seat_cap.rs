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

use crate::plan::Plan;
use crate::{CompileOutcome, DiagnosticKind, HotPolicy};
/// What a declared low effort tells the person, under the proposal.
pub(crate) use nika_compile_fidelity::fidelity::budget::LOW_EFFORT_DECLARED as LOW_EFFORT;
use nika_compile_fidelity::fidelity::budget::{self, ReasoningFacts};

/// The generated cap on a seat the catalog records as reasoning: room for a thinking trace
/// before the visible answer. 1200 was measured too small (openai/gpt-5-mini · `NIKA-INFER-002
/// · cut off at the token limit`); a real `DeepSeek` structured summary exhausted 4096
/// tokens in reasoning with no visible answer. Leave 16384 for thinking plus the answer.
/// This remains a finite ceiling subject to the known model limit and run admission,
/// never a spend or permission to raise a cap the requester explicitly supplied.
pub(crate) const REASONING_MAX_TOKENS: u32 = 16_384;

/// A seat's provider and model when the catalog can judge it: never `mock` (its fixture row
/// claims every capability) and never a template (`nika check` judges the resolved seat).
fn judged(seat: &str) -> Option<(&str, &str)> {
    if seat.contains("${{") {
        return None;
    }
    seat.split_once('/')
        .filter(|(provider, _)| *provider != "mock")
}

/// A cap the compiler generates, sized for the doc's seat from its catalog records alone
/// ([`nika_catalog::generated_cap`]): the reasoning floor first, never above the recorded output
/// limit. Any other seat keeps the base.
pub(crate) fn sized(seat: Option<&str>, base: u32) -> u32 {
    seat.and_then(judged).map_or(base, |(provider, name)| {
        nika_catalog::generated_cap(provider, name, base, REASONING_MAX_TOKENS)
    })
}

/// The catalog's output facts for the workflow's model ([`nika_catalog::output_caps`]), given to
/// the authoring seat before it writes a candidate, with the reasoning room this compiler
/// generates. `None` only for `mock` and templated seats.
#[must_use]
pub fn output_caps(model: &str) -> Option<Value> {
    let (provider, name) = judged(model)?;
    Some(nika_catalog::output_caps(
        provider,
        name,
        REASONING_MAX_TOKENS,
    ))
}

/// Whether a workflow on `seat` gets the documented low effort (2026-10-11): a request the
/// reader admits whole under the strict contract (an effort said in a clause of its own is left
/// unread) with no constraint beside its work, whose plan's model steps are typed drafts
/// ([`budget::drafts_only`]), on a seat whose exact row lists `low`.
pub(crate) fn lowered(seat: Option<&str>, intent: &str, plan: &Plan) -> bool {
    let intent = &crate::lexicon::fold_apostrophes(intent);
    let reading = crate::lexicon::read(intent);
    let whole = crate::doors::admit_hot(intent, &reading, HotPolicy::Strict).is_ok();
    let facts = seat
        .and_then(output_caps)
        .map(|caps| ReasoningFacts::of_caps(&caps));
    whole
        && reading.plan.constraints.is_empty()
        && budget::drafts_only(plan)
        && facts.is_some_and(|f| f.reasons_by_default && f.low_effort)
}

/// [`lowered`] on a native candidate for a creation's words `intent`, declared in its own text
/// ([`budget::declared_low_effort`]) unless it declares `run.reasoning` (the person's or the
/// author's) or `run.access` (a route the level may not reach); kept only when the parser reads
/// exactly that addition.
pub(crate) fn low_effort(source: &str, intent: &str) -> Option<String> {
    let before = crate::edit::literal_projection(source)?;
    let plan = crate::lexicon::read(intent).plan;
    let chosen = !before["run"]["reasoning"].is_null() || !before["run"]["access"].is_null();
    if chosen || !lowered(before["model"].as_str(), intent, &plan) {
        return None;
    }
    let mut after = before.clone();
    after["run"]["reasoning"] = json!({"effort": "low"});
    let edited = budget::declared_low_effort(source)?;
    (crate::edit::literal_projection(&edited)? == after).then_some(edited)
}

/// The model-neutral cap the compiler generates for a language task that states none: room for
/// a thinking trace before the answer, whatever the model. Bounded by the recorded output limit
/// once the workflow's model is seated.
pub(crate) const NEUTRAL_MAX_TOKENS: u32 = 4096;

/// Fills the cap of every `infer:` task that states none, once the workflow's model is seated.
/// Nobody named a bound there, so the default is the compiler's: the neutral cap, bounded by
/// the recorded output limit, with the reasoning floor when the catalog records it.
/// A stated cap is never touched. The edit goes through the
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
        let cap = sized(seat.as_deref(), NEUTRAL_MAX_TOKENS);
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
            nika_catalog::model_output_limit(provider, name).map(|(limit, _)| limit),
        )
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
        let chat =
            nika_catalog::generated_cap("deepseek", "deepseek-chat", 800, REASONING_MAX_TOKENS);
        assert_eq!(sized(Some("deepseek/deepseek-chat"), 800), chat);
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
            json!(REASONING_MAX_TOKENS)
        );
        assert_eq!(doc["tasks"]["named"]["infer"]["max_tokens"], json!(700));
        assert_eq!(
            doc["tasks"]["draft"]["infer"]["prompt"],
            json!("Say hi in three lines")
        );
        assert_eq!(fill_defaults(&filled), None, "nothing is missing any more");
    }

    /// Drafts the reader admits whole, on a seat documenting `low`, get it declared in the
    /// candidate's own text, atop a `run:` block it has. A declared effort or route, a constraint
    /// beside the work, a stated effort the reader cannot read, other work, or a seat that
    /// documents no `low` declares nothing.
    #[test]
    fn the_documented_low_effort_is_declared_only_for_drafts_read_whole() {
        let whole = "Résume ./notes/brief.md en 3 puces dans ./out/resume.md";
        let source = "nika: digest\nmodel: deepseek/deepseek-flash\ntasks:\n  draft:\n    infer:\n      prompt: \"Summarize\"\n      max_tokens: 16384\n";
        let block = |run: &str| source.replacen("\ntasks:", &format!("\nrun:\n{run}tasks:"), 1);
        let low = "  reasoning:\n    effort: low\n";
        assert_eq!(low_effort(source, whole), Some(block(low)));
        let pro = source.replacen("deepseek-flash", "deepseek-v4-pro", 1);
        let declared = low_effort(&pro, whole).expect("the pro row documents low too");
        assert!(declared.contains(low), "{declared}");
        let clock = "  clock: virtual\n";
        let atop = low_effort(&block(clock), whole);
        assert_eq!(atop, Some(block(&format!("{low}{clock}"))));
        let high = block("  reasoning:\n    effort: high\n");
        assert_eq!(low_effort(&high, whole), None, "a declared effort stays");
        let routed = block("  access:\n    via: openrouter\n");
        assert_eq!(low_effort(&routed, whole), None, "a chosen route stays");
        // Typographic apostrophes folded, as the HOT door folds them.
        let curly = "Résume l’article de ./notes/brief.md en 3 puces dans ./out/resume.md";
        assert_eq!(low_effort(source, curly), Some(block(low)));
        for intent in [
            "Résume ./notes/brief.md en 3 puces dans ./out/resume.md. Keep the tone formal.",
            "Read ./notes/brief.md, summarize it in three bullets, and write the summary to ./out/summary.md. Think hard.",
            "Read ./data/invoice.txt, extract the supplier, the date and the amount, and write them to ./out/invoice.json",
            "Résume notes.md en trois lignes dans digest.md.",
        ] {
            assert_eq!(low_effort(source, intent), None, "{intent}");
        }
        for seat in ["deepseek-reasoner", "deepseek-chat"] {
            let seated = source.replacen("deepseek-flash", seat, 1);
            assert_eq!(low_effort(&seated, whole), None, "{seat}");
        }
        for seat in ["mock/echo", "acme/unheard-of-model", "${{ inputs.seat }}"] {
            let seated = source.replacen("deepseek/deepseek-flash", seat, 1);
            assert_eq!(low_effort(&seated, whole), None, "{seat}");
        }
    }

    /// The declaration's two properties, judged apart: the document means the source plus
    /// `run.reasoning.effort: low` and nothing else (AST equivalence), and every byte of the source
    /// outside the inserted lines stands as written (source preservation), with or without a
    /// `run:` block, a comment and a flow mapping in it.
    #[test]
    fn the_declaration_is_the_only_change_in_meaning_and_in_bytes() {
        let whole = "Résume ./notes/brief.md en 3 puces dans ./out/resume.md";
        let bare = "nika: digest\n# the author's own note\nmodel: deepseek/deepseek-flash\ntasks:\n  draft:\n    infer: { prompt: 'Summarize', max_tokens: 16384 }\n";
        let clocked = bare.replacen("\ntasks:", "\nrun:\n  clock: virtual\ntasks:", 1);
        let low = "  reasoning:\n    effort: low\n";
        for (source, inserted) in [
            (bare.to_owned(), format!("run:\n{low}")),
            (clocked, low.to_owned()),
        ] {
            let edited = low_effort(&source, whole).expect("drafts read whole on a low seat");
            let mut meant = crate::edit::literal_projection(&source).expect("a document");
            meant["run"]["reasoning"] = json!({"effort": "low"});
            let read = crate::edit::literal_projection(&edited);
            assert_eq!(read, Some(meant), "AST: only the declaration");
            let kept = edited.replacen(&inserted, "", 1);
            assert_eq!(kept, source, "source: every other byte");
        }
    }
}
