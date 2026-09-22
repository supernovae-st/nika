// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The seat's proposal: its closed shape, its decoding, and the merge that joins it to the
//! deterministic reading under the anchoring, accounting and policy laws. The deterministic
//! facts win every disagreement; a model changes HOW a duty is realized, never whether.
//! Split from `cognition.rs` at the file-LOC cap (2026-09-22); the laws are unchanged.

use super::backstops::{gate_finds_its_effect, reconcile_refund_backstop, starts_with_prohibition};
use super::{backstop, plan_record, record_ledger};
use crate::plan::{Effect, EffectPolicy, EffectVerb, Obligation, ObligationKind, Op, Plan, Step};
use crate::{CompileOutcome, DiagnosticKind, QuestionType, lexicon::Reading};
use nika_kernel::ai::provider::{ContentBlock, InferResponse, StopReason};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Proposal {
    steps: Vec<ProposedStep>,
    effects: Vec<ProposedEffect>,
    obligations: Vec<ProposedObligation>,
    constraints: Vec<String>,
    unknowns: Vec<String>,
    #[serde(default)]
    regions: Vec<ProposedRegion>,
    #[serde(default)]
    approval_bypass: Option<ProposedBypass>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProposedStep {
    op: String,
    detail: String,
    evidence: String,
    #[serde(default, deserialize_with = "nullable_vec")]
    categories: Vec<String>,
    #[serde(default)]
    computation: Option<crate::predicate::ProposedComputation>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProposedEffect {
    verb: String,
    target: String,
    policy: String,
    evidence: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProposedObligation {
    kind: String,
    #[serde(default)]
    value: Option<u32>,
    evidence: String,
}
/// One contiguous region of the request and what it is for: the accounting the model owes.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProposedRegion {
    pub(crate) text: String,
    pub(crate) role: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProposedBypass {
    present: bool,
    #[serde(default, deserialize_with = "nullable_string")]
    evidence: String,
}

/// A provider's strict structured-output mode may turn an optional property into an explicit
/// `null`; the decoder reads it as the absent default rather than refusing the plan.
pub(crate) fn nullable_vec<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<String>, D::Error> {
    Ok(Option::<Vec<String>>::deserialize(d)?.unwrap_or_default())
}

pub(crate) fn nullable_string<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(d)?.unwrap_or_default())
}

/// Typographic quotes read as their plain twins: a model that answers `“version”` for a
/// request that wrote `"version"` still names the same text.
fn fold_quote(ch: char) -> char {
    match ch {
        '“' | '”' | '„' | '«' | '»' => '"',
        '‘' | '’' | '‚' => '\'',
        other => other,
    }
}

/// The exact request excerpt a proposal's evidence names: the evidence itself when it is a
/// verbatim substring, else the request substring it matches once runs of whitespace are
/// folded on both sides (a model may wrap a line or drop a double space; it may not change
/// a word). None when nothing in the request matches.
pub(crate) fn exact_excerpt(intent: &str, evidence: &str) -> Option<String> {
    let evidence = evidence.trim();
    if evidence.is_empty() {
        return None;
    }
    if intent.contains(evidence) {
        return Some(evidence.to_owned());
    }
    let mut folded = String::new();
    let mut offsets: Vec<usize> = Vec::new();
    let mut pending_space = false;
    for (index, ch) in intent.char_indices() {
        if ch.is_whitespace() {
            pending_space = !folded.is_empty();
            continue;
        }
        if pending_space {
            folded.push(' ');
            offsets.push(index);
            pending_space = false;
        }
        let ch = fold_quote(ch);
        folded.push(ch);
        for _ in 0..ch.len_utf8() {
            offsets.push(index);
        }
    }
    // An excerpt that abbreviates a long clause with an ellipsis names the contiguous span
    // from its first fragment to its last; every fragment must occur, in order, verbatim.
    let fragments: Vec<String> = evidence
        .chars()
        .map(fold_quote)
        .collect::<String>()
        .replace('…', "...")
        .split("...")
        .map(|part| part.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|part| !part.is_empty())
        .collect();
    if fragments.is_empty() {
        return None;
    }
    let first_at = folded.find(fragments.first()?)?;
    let mut cursor = first_at + fragments.first()?.len();
    let mut last_end = cursor;
    for fragment in fragments.iter().skip(1) {
        let at = folded.get(cursor..)?.find(fragment.as_str())? + cursor;
        cursor = at + fragment.len();
        last_end = cursor;
    }
    let start = *offsets.get(first_at)?;
    let last = *offsets.get(last_end - 1)?;
    let end = last + intent.get(last..)?.chars().next()?.len_utf8();
    intent.get(start..end).map(str::to_owned)
}

/// The head of a rejected excerpt for the finding: enough to see what the model wrote,
/// never the whole text.
pub(super) fn excerpt_head(text: &str) -> String {
    let trimmed = text.trim();
    let mut head: String = trimmed.chars().take(80).collect();
    if head.len() < trimmed.len() {
        head.push('…');
    }
    head
}

/// An evidence of a proposal the request never wrote, seen before the merge judges the
/// plan: the one defect a seat repairs from a verifier's counterexample without changing
/// what it understood (a seat that answers `extraits` for a request that wrote `extrais`
/// names the right clause with the wrong letters).
pub(super) struct Unanchored {
    /// `operation`, `effect` or `obligation`: the element whose evidence failed.
    pub(super) role: &'static str,
    /// The element's own word: its op, verb or kind.
    pub(super) label: String,
    /// The evidence as the seat wrote it.
    pub(super) evidence: String,
}

/// The first evidence of a proposal that is not an exact excerpt of the request, if any,
/// in the merge's own order: operations, then effects, then obligations.
pub(super) fn unanchored(intent: &str, proposal: &Proposal) -> Option<Unanchored> {
    let miss = |role: &'static str, label: &str, evidence: &str| {
        exact_excerpt(intent, evidence)
            .is_none()
            .then(|| Unanchored {
                role,
                label: label.to_owned(),
                evidence: evidence.to_owned(),
            })
    };
    proposal
        .steps
        .iter()
        .find_map(|step| miss("operation", &step.op, &step.evidence))
        .or_else(|| {
            proposal
                .effects
                .iter()
                .find_map(|effect| miss("effect", &effect.verb, &effect.evidence))
        })
        .or_else(|| {
            proposal
                .obligations
                .iter()
                .find_map(|obligation| miss("obligation", &obligation.kind, &obligation.evidence))
        })
}

/// The model's own accounting, read back: a region it labelled as producing (operation,
/// effect, obligation, constraint, policy) must overlap an element the merged plan carries
/// (a step, an effect, an obligation, a constraint or a policy literal). A region consumed
/// without an element is a clause the proposal dropped, and a dropped clause is not
/// understood: it becomes an unknown, never a silent omission.
fn unproduced_regions(plan: &Plan, regions: &[ProposedRegion], folded: &[String]) -> Vec<String> {
    let fold = |text: &str| {
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    let overlaps = |a: &str, b: &str| {
        let (a, b) = (fold(a), fold(b));
        !a.is_empty() && !b.is_empty() && (a.contains(&b) || b.contains(&a))
    };
    let mut produced: Vec<String> = Vec::new();
    produced.extend(plan.steps.iter().map(|s| s.evidence.clone()));
    produced.extend(plan.effects.iter().map(|e| e.evidence.clone()));
    produced.extend(plan.obligations.iter().map(|o| o.evidence.clone()));
    produced.extend(plan.constraints.iter().cloned());
    produced.extend(plan.effects.iter().filter_map(|e| e.policy_literal.clone()));
    produced.extend(folded.iter().cloned());
    // A gate the request states (« pero pídeme confirmación antes de enviar ») is realized
    // by the human-first policy of the effect it dominates, whatever the region says.
    let gated = plan
        .effects
        .iter()
        .any(|e| e.policy == EffectPolicy::HumanFirst);
    let mut gaps = Vec::new();
    for region in regions {
        let text = region.text.trim();
        let producing = matches!(
            region.role.as_str(),
            "operation" | "effect" | "obligation" | "constraint" | "policy"
        );
        if text.is_empty() || !producing {
            continue;
        }
        let lower = text.to_lowercase();
        if gated
            && (crate::gates::named_gate(&lower).is_some()
                || crate::gates::final_gate(&lower).is_some())
        {
            continue;
        }
        // A short region (a connector, a heading, a few words) never carries requested work.
        if text.split_whitespace().count() < 4 {
            continue;
        }
        if produced.iter().any(|p| overlaps(p, text)) {
            continue;
        }
        gaps.push(format!(
            "The proposal read `{text}` as {} but produced nothing for it; that part of the request is not understood.",
            region.role
        ));
    }
    gaps
}

pub(super) fn decode(response: &InferResponse, out: &mut CompileOutcome) -> Option<Proposal> {
    let text = match response.content.as_slice() {
        [ContentBlock::Text { text }]
            if text.len() <= 65_536 && response.stop_reason == StopReason::EndTurn =>
        {
            text
        }
        _ if response.stop_reason == StopReason::MaxTokens => {
            // A reasoning seat spends part of its output cap on its reasoning: 16 of 52
            // gpt-5-mini proposals stopped at exactly 4000 output tokens (eco-60, 2026-09-22).
            // The cap is the operator's knob; the finding names it instead of the shape.
            let spent = response
                .usage_reported
                .then_some(response.usage.output_tokens)
                .map_or_else(|| "its".to_owned(), |n| format!("{n} output tokens, its"));
            crate::finding(
                out,
                DiagnosticKind::Unknown,
                "authoring_provider",
                format!(
                    "The seat stopped at {spent} output cap before the plan was complete (a reasoning seat spends part of the cap on its reasoning). Raise --authoring-max-tokens (up to 8192) or seat a model that reasons less; nothing partial was assembled."
                ),
            );
            return None;
        }
        _ => {
            crate::finding(
                out,
                DiagnosticKind::Unknown,
                "authoring_plan",
                "Authoring must return one complete bounded JSON text, without tools or other content.",
            );
            return None;
        }
    };
    if let Ok(plan) = serde_json::from_str(text) {
        Some(plan)
    } else {
        crate::finding(
            out,
            DiagnosticKind::Unknown,
            "authoring_plan",
            "The authoring response is not a valid closed semantic plan. No source was emitted.",
        );
        None
    }
}

fn fold_words(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Verbs a seat uses for a draft that is no language work (six languages, folded).
const SERIALIZE_VERBS: &[&str] = &[
    "prepar",
    "prépar",
    "serializ",
    "sérialis",
    "format",
    "generat",
    "génér",
    "gener",
    "produ",
    "produir",
    "produz",
    "erzeug",
    "schreib",
    "write ",
    "writ",
    "écri",
    "ecri",
    "escrib",
    "scriv",
    "escrev",
    "assembl",
    "build",
    "construi",
    "compos",
];
/// The data the draft would only carry: the rows, the file content, the result.
const DATA_WORDS: &[&str] = &[
    "csv",
    "json",
    "content",
    "contenu",
    "contenido",
    "contenuto",
    "inhalt",
    "conteúdo",
    "conteudo",
    "rows",
    "row ",
    "lignes",
    "ligne ",
    "filas",
    "fila ",
    "righe",
    "riga ",
    "zeilen",
    "zeile ",
    "linhas",
    "linha ",
    "array",
    "tableau",
    "table",
    "tabelle",
    "list",
    "liste",
    "lista",
    "result",
    "résultat",
    "resultat",
    "resultado",
    "risultato",
    "ergebnis",
    "output",
    "file",
    "fichier",
    "archivo",
    "ficheiro",
    "datei",
    "record",
    "enregistrement",
    "column",
    "colonne",
    "columna",
    "colonna",
    "spalte",
    "coluna",
    "field",
    "champ",
];
/// Words that make a draft language work whatever else it says: a summary, a note, a
/// digest, a reply, a translation, a text with headings.
const LANGUAGE_WORDS: &[&str] = &[
    "summar",
    "résum",
    "resum",
    "riassunt",
    "zusammenfass",
    "note",
    "digest",
    "report",
    "rapport",
    "informe",
    "relazione",
    "bericht",
    "relatório",
    "relatorio",
    "reply",
    "réponse",
    "reponse",
    "respuesta",
    "risposta",
    "antwort",
    "resposta",
    "translat",
    "traduc",
    "traduz",
    "übersetz",
    "ubersetz",
    "letter",
    "lettre",
    "carta",
    "heading",
    "titre",
    "título",
    "titulo",
    "titoli",
    "überschrift",
    "uberschrift",
    "prose",
    "paragraph",
    "paragraphe",
    "sentence",
    "phrase",
    "message",
    "brief",
    "explain",
    "expliqu",
    "describ",
    "décri",
    "decri",
    "narrat",
    "bullet",
    "puces",
];

/// A draft whose detail only prepares, formats or serializes the rows a computation
/// produced (« préparer le contenu CSV filtré pour écriture », « serialize the resulting
/// array as JSON ») is no language work: the write takes the computed rows as they are. A
/// detail that names language work (a summary, a note, a digest, headings) stays a draft.
fn serialization_draft(detail: &str) -> bool {
    let detail = fold_words(detail);
    let cue = SERIALIZE_VERBS.iter().any(|v| detail.contains(v))
        && DATA_WORDS.iter().any(|w| detail.contains(w));
    cue && !LANGUAGE_WORDS.iter().any(|w| detail.contains(w))
}

/// « pídeme confirmación antes de enviar » asks a person before the effect: that is the
/// gate its policy carries, never a version to recheck. Records the fold as applied.
fn gate_phrase(evidence: &str, out: &mut CompileOutcome) -> bool {
    let lower = evidence.to_lowercase();
    if crate::gates::named_gate(&lower).is_none() && crate::gates::final_gate(&lower).is_none() {
        return false;
    }
    crate::finding(
        out,
        DiagnosticKind::Applied,
        "authoring_plan",
        format!(
            "`{}` is the human gate the request states, carried by the effect's policy; the proposal's `revision_check` over the same words was not assembled.",
            evidence.trim()
        ),
    );
    true
}

/// The clauses a proposal states as a record effect (create, update, order…): a `write`
/// proposed over the same clause with no file path in its words is that effect's twin
/// (« Enregistre une écriture dans le logiciel comptable » as a create AND a write).
fn write_twins(effects: &[ProposedEffect]) -> Vec<String> {
    effects
        .iter()
        .filter(|e| e.verb != "write")
        .map(|e| fold_words(&e.evidence))
        .filter(|words| !words.is_empty())
        .filter(|words| crate::paths::literals(words).is_empty())
        .collect()
}

/// One clause is one step. A step proposed over a write clause that is neither the draft
/// nor the computation of what is written (« écrire ces lignes … dans ./out/retards.csv »
/// listed as a validate, an explore, a fetch, a lookup, a classify, an extract and a search
/// at once) is the write the proposal already states; a step proposed over a gate phrase
/// (« pregúntame y espera mi aprobación » as an explore) is the human gate the effect's
/// policy carries. Neither is assembled; the fold is recorded. Returns whether the step
/// was folded.
fn one_clause_one_step(
    step: &ProposedStep,
    op: Op,
    write_clauses: &[String],
    trigger: Option<&str>,
    obligations: &[String],
    intent: &str,
    out: &mut CompileOutcome,
) -> bool {
    let clause = fold_words(&step.evidence);
    if matches!(op, Op::Explore | Op::Validate | Op::Draft)
        && (crate::gates::named_gate(&clause).is_some()
            || crate::gates::final_gate(&clause).is_some())
    {
        crate::finding(
            out,
            DiagnosticKind::Applied,
            "authoring_plan",
            format!(
                "`{}` is the human gate the request states, carried by the effect's policy; the proposal's `{}` over the same words was not assembled.",
                step.evidence.trim(),
                step.op
            ),
        );
        return true;
    }
    if trigger.is_some_and(|trigger| over_a_region(step, &clause, trigger, intent)) {
        crate::finding(
            out,
            DiagnosticKind::Applied,
            "authoring_plan",
            format!(
                "`{}` is the trigger the request states, recorded as requested_trigger; the proposal's `{}` over the same words was not assembled.",
                step.evidence.trim(),
                step.op
            ),
        );
        return true;
    }
    // A look-alike of a safeguard (a classify or a computation for a dedup, a validate for a
    // recheck) is folded on its words alone; any other operation over a safeguard's words is
    // folded only when its detail names nothing the request states outside them.
    let look_alike = matches!(op, Op::Classify | Op::Compute | Op::Validate);
    if obligations.iter().any(|words| {
        (look_alike && !clause.is_empty() && words.contains(&clause))
            || over_a_region(step, &clause, words, intent)
    }) {
        crate::finding(
            out,
            DiagnosticKind::Applied,
            "authoring_plan",
            format!(
                "`{}` is the safeguard the request states, carried by its obligation; the proposal's `{}` over the same words was not assembled.",
                step.evidence.trim(),
                step.op
            ),
        );
        return true;
    }
    if write_clauses.contains(&clause) && !matches!(op, Op::Draft | Op::Compute) {
        crate::finding(
            out,
            DiagnosticKind::Applied,
            "authoring_plan",
            format!(
                "`{}` is the write the proposal states; its `{}` over the same words was not assembled.",
                step.evidence.trim(),
                step.op
            ),
        );
        return true;
    }
    false
}

/// A proposed read of records the request names by their owner or their store (« mes
/// disponibilités et celles des participants », « the customer record »), with no path, no
/// endpoint and no supplied material, is a lookup: the reader's retrieval cues settle the
/// family, and the assembler asks where the records live instead of binding the read to
/// the material an invocation supplies and drafting from an input string.
fn retrieval_family(op: Op, step: &ProposedStep, out: &mut CompileOutcome) -> Op {
    if op != Op::Read {
        return op;
    }
    let names_a_place = !crate::paths::literals(&step.detail).is_empty()
        || step.detail.contains("://")
        || crate::shape::names_supplied_material(&step.evidence)
        || crate::shape::names_supplied_material(&step.detail);
    if names_a_place {
        return op;
    }
    let lower = step.detail.to_lowercase();
    match crate::lexicon::settle_retrieval(&lower, &[Op::Read, Op::Lookup, Op::Search]) {
        Some(settled @ (Op::Lookup | Op::Search)) => {
            crate::finding(
                out,
                DiagnosticKind::Applied,
                "authoring_plan",
                format!(
                    "`{}` names records the request keeps somewhere, never material an invocation supplies; the proposal's `read` is assembled as a `{}`.",
                    step.evidence.trim(),
                    settled.word()
                ),
            );
            settled
        }
        _ => op,
    }
}

/// Whether a step lies over a region the request states (the trigger clause, a safeguard's
/// words) and names nothing beyond it: its clause and the region contain one another, and
/// no content word of its detail is anchored in the request outside the region. A lookup
/// over « Quand le bouton Slack de validation est utilisé » whose detail is « retrouve le
/// dossier dans `MongoDB` » is that operation, mis-anchored, never the trigger.
fn over_a_region(step: &ProposedStep, clause: &str, region: &str, intent: &str) -> bool {
    if clause.is_empty() || region.is_empty() {
        return false;
    }
    let wider = if clause.contains(region) {
        clause
    } else if region.contains(clause) {
        region
    } else {
        return false;
    };
    let quoted = |text: &str| text.chars().map(fold_quote).collect::<String>();
    let wider = quoted(wider);
    let rest = quoted(&fold_words(intent)).replacen(&wider, " ", 1);
    let outside: Vec<String> = content_words(&rest).collect();
    let inside: Vec<String> = content_words(&wider).collect();
    content_words(&quoted(&step.detail))
        .all(|word| !outside.contains(&word) || inside.contains(&word))
}

/// The words of a text that carry content: four characters or more, punctuation stripped,
/// lowercased.
fn content_words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split_whitespace()
        .map(|word| {
            word.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|word| word.chars().count() >= 4)
}

/// Whether a constraint restates a clause the plan carries elsewhere: an operation's
/// evidence or detail, an effect's evidence or target, an obligation's words, the trigger.
fn restates_a_clause(plan: &Plan, constraint: &str) -> bool {
    let wanted = fold_words(constraint);
    if wanted.is_empty() {
        return false;
    }
    let inside = |text: &str| {
        let text = fold_words(text);
        !text.is_empty() && (text.contains(&wanted) || wanted.contains(&text))
    };
    plan.steps
        .iter()
        .any(|s| inside(&s.evidence) || inside(&s.detail))
        || plan
            .effects
            .iter()
            .any(|e| inside(&e.evidence) || inside(&e.target))
        || plan.obligations.iter().any(|o| inside(&o.evidence))
        || plan.trigger.as_deref().is_some_and(inside)
}

/// One clause is one effect. An effect proposed over a language step's own words
/// (« rédige un compte rendu » as a create or a write) with no path and no endpoint in its
/// target is that step, never an action on the outside world; two effects over the same
/// clause with kindred verbs (a publish and a send over « Poste ensuite la réponse dans le
/// fil Slack ») are one effect, the first stands. Returns whether the effect was folded.
fn one_clause_one_effect(
    effect: &ProposedEffect,
    language_clauses: &[String],
    stated: &mut Vec<String>,
    out: &mut CompileOutcome,
) -> bool {
    let clause = fold_words(&effect.evidence);
    if clause.is_empty() {
        return false;
    }
    let names_a_place = crate::paths::literals(&effect.target).iter().any(|shape| {
        matches!(
            shape,
            crate::paths::PathShape::File(_)
                | crate::paths::PathShape::Directory(_)
                | crate::paths::PathShape::Glob(_)
        )
    }) || effect.target.contains("://")
        || effect.target.contains('@');
    if !names_a_place && language_clauses.iter().any(|words| words == &clause) {
        crate::finding(
            out,
            DiagnosticKind::Applied,
            "authoring_plan",
            format!(
                "`{}` is the language step the proposal states; its `{}` over the same words names no file and no endpoint and was not assembled as an effect.",
                effect.evidence.trim(),
                effect.verb
            ),
        );
        return true;
    }
    if stated.contains(&clause) {
        crate::finding(
            out,
            DiagnosticKind::Applied,
            "authoring_plan",
            format!(
                "`{}` is one effect; the proposal's `{}` over the same words was not assembled twice.",
                effect.evidence.trim(),
                effect.verb
            ),
        );
        return true;
    }
    stated.push(clause);
    false
}

/// The proposal joins the deterministic reading; deterministic facts win every disagreement,
/// and the proposal must account for every region of the request.
#[allow(clippy::too_many_lines)] // one validation walk over steps, effects, obligations, regions
pub(super) fn merge(
    intent: &str,
    proposal: Proposal,
    reading: &Reading,
    out: &mut CompileOutcome,
) -> Option<Plan> {
    // The deterministic reading contributes its POLICY floor (effects with their policy,
    // obligations, constraints, bindings, unknowns), never its operation guesses: a clause the
    // reader consumed is not understanding, and the model must account for every region.
    let mut plan = reading.plan.clone();
    plan.steps = Vec::new();
    // Every clause the merge folds (a twin, a restatement) was understood: it counts as
    // produced in the regions accounting.
    let mut folded: Vec<String> = Vec::new();
    let produces_data = proposal
        .steps
        .iter()
        .any(|s| matches!(s.op.as_str(), "compute" | "extract"));
    let write_clauses: Vec<String> = proposal
        .effects
        .iter()
        .filter(|e| e.verb == "write")
        .map(|e| fold_words(&e.evidence))
        .collect();
    let trigger = reading.plan.trigger.as_deref().map(fold_words);
    let obligation_words: Vec<String> = reading
        .plan
        .obligations
        .iter()
        .map(|o| fold_words(&o.evidence))
        .chain(proposal.obligations.iter().map(|o| fold_words(&o.evidence)))
        .filter(|words| !words.is_empty())
        .collect();
    let language_clauses: Vec<String> = proposal
        .steps
        .iter()
        .filter(|s| matches!(s.op.as_str(), "draft" | "extract" | "classify" | "compute"))
        .map(|s| fold_words(&s.evidence))
        .filter(|words| !words.is_empty())
        .collect();
    for step in proposal.steps {
        let Some(op) = Op::parse(&step.op) else {
            reject(out, "unknown operation in the proposal");
            return None;
        };
        let op = retrieval_family(op, &step, out);
        if one_clause_one_step(
            &step,
            op,
            &write_clauses,
            trigger.as_deref(),
            &obligation_words,
            intent,
            out,
        ) {
            folded.push(step.evidence.clone());
            continue;
        }
        if op == Op::Draft && produces_data && serialization_draft(&step.detail) {
            crate::finding(
                out,
                DiagnosticKind::Applied,
                "authoring_plan",
                format!(
                    "`{}` only prepares the rows a computation produces; they are written as they are, and the proposal's `draft` over them was not assembled.",
                    step.detail.trim()
                ),
            );
            folded.push(step.evidence.clone());
            continue;
        }
        let Some(evidence) = exact_excerpt(intent, &step.evidence) else {
            reject(
                out,
                &format!(
                    "an operation lacks an exact source excerpt (`{}` names `{}`)",
                    step.op,
                    excerpt_head(&step.evidence)
                ),
            );
            return None;
        };
        if op == Op::Compute && starts_with_prohibition(&evidence) {
            // "Do not copy more than 10 consecutive words" is a rule the prose obeys, not a
            // computation the workflow runs; it shapes prompts as a constraint.
            if !plan.constraints.contains(&evidence) {
                plan.constraints.push(evidence);
            }
            continue;
        }
        if op == Op::Compute
            && let Some(computation) = step.computation.as_ref().filter(|c| c.present)
            && let Some(rule) = crate::predicate::typed_rule(intent, &evidence, computation)
            && !plan.rules.iter().any(|r| r.text() == rule.text())
        {
            plan.rules.push(rule);
        }
        plan.push_step(Step::new(op, evidence, step.detail, step.categories));
    }
    let twins = write_twins(&proposal.effects);
    let mut stated_effects: Vec<String> = Vec::new();
    for effect in proposal.effects {
        if one_clause_one_effect(&effect, &language_clauses, &mut stated_effects, out) {
            folded.push(effect.evidence.clone());
            continue;
        }
        if effect.verb == "write"
            && twins
                .iter()
                .any(|twin| twin == &fold_words(&effect.evidence))
        {
            crate::finding(
                out,
                DiagnosticKind::Applied,
                "authoring_plan",
                format!(
                    "`{}` is the record the proposal already creates; its `write` twin over the same words names no file and was not assembled.",
                    effect.evidence.trim()
                ),
            );
            folded.push(effect.evidence.clone());
            continue;
        }
        let (Some(verb), Some(policy)) = (
            EffectVerb::parse(&effect.verb),
            match effect.policy.as_str() {
                "automatic" => Some(EffectPolicy::Automatic),
                "human_first" => Some(EffectPolicy::HumanFirst),
                "forbidden" => Some(EffectPolicy::Forbidden),
                "unspecified" => Some(EffectPolicy::Undecided),
                "conflict" => Some(EffectPolicy::Conflict),
                _ => None,
            },
        ) else {
            reject(out, "unknown effect verb or policy in the proposal");
            return None;
        };
        let Some(evidence) = exact_excerpt(intent, &effect.evidence) else {
            reject(
                out,
                &format!(
                    "an effect lacks an exact source excerpt (`{}` names `{}`); no effect was invented",
                    effect.verb,
                    excerpt_head(&effect.evidence)
                ),
            );
            return None;
        };
        if let Some(existing) = plan
            .effects
            .iter_mut()
            .find(|e| e.verb == verb && same_write(verb, &e.target, &effect.target))
        {
            // The deterministic policy is the floor: a model may only strengthen a plain
            // request. Any other disagreement about a recognized effect is a human question.
            if !effect.target.trim().is_empty() {
                existing.target.clone_from(&effect.target);
                existing.evidence.clone_from(&evidence);
            }
            if existing.policy == EffectPolicy::Automatic && policy != EffectPolicy::Automatic {
                existing.policy = policy;
            } else if existing.policy != policy {
                plan.unknowns.push(format!(
                    "The proposal reads `{}` as {} while the request's explicit wording reads {}; the disagreement is not settled by a model.",
                    verb.word(),
                    policy.word(),
                    existing.policy.word()
                ));
            }
        } else {
            plan.effects
                .push(Effect::new(verb, effect.target, evidence, policy));
        }
    }
    gate_finds_its_effect(&mut plan);
    for obligation in proposal.obligations {
        let Some(evidence) = exact_excerpt(intent, &obligation.evidence) else {
            reject(
                out,
                &format!(
                    "an obligation lacks an exact source excerpt (`{}` names `{}`)",
                    obligation.kind,
                    excerpt_head(&obligation.evidence)
                ),
            );
            return None;
        };
        let kind = match (obligation.kind.as_str(), obligation.value) {
            ("dedup", _) => ObligationKind::Dedup,
            ("revision_check", _) if gate_phrase(&evidence, out) => continue,
            ("revision_check", _) => ObligationKind::RevisionCheck,
            ("retry_bound", Some(n)) if n > 0 => ObligationKind::RetryBound(n),
            _ => {
                reject(out, "an obligation is malformed");
                return None;
            }
        };
        if !plan
            .obligations
            .iter()
            .any(|o| o.kind.word() == kind.word())
        {
            plan.obligations.push(Obligation::new(kind, evidence));
        }
    }
    for constraint in proposal.constraints {
        // « Lies ./solar/ertrag.csv » listed once as the read and once as a constraint: a
        // clause the plan carries elsewhere binds nothing new and is not a constraint.
        if restates_a_clause(&plan, &constraint) {
            crate::finding(
                out,
                DiagnosticKind::Applied,
                "authoring_plan",
                format!(
                    "`{}` is a clause the plan already carries; the proposal's constraint over the same words was not filed.",
                    constraint.trim()
                ),
            );
            folded.push(constraint);
            continue;
        }
        if !plan.constraints.contains(&constraint) {
            plan.constraints.push(constraint);
        }
    }
    plan.unknowns.extend(proposal.unknowns);
    // Semantic accounting: the request must be covered by regions the model can name.
    if let Some(bypass) = proposal.approval_bypass
        && bypass.present
        && exact_excerpt(intent, &bypass.evidence).is_some()
    {
        plan.unknowns.push(format!(
            "The request presupposes, reuses or skips an approval it does not give ({}); the compiler never grants that authority.",
            bypass.evidence.trim()
        ));
    }
    for gap in accounting_gaps(intent, &proposal.regions) {
        plan.unknowns.push(gap);
    }
    for gap in unproduced_regions(&plan, &proposal.regions, &folded) {
        plan.unknowns.push(gap);
    }
    // A numeric rule the model demoted to guidance is an operation: promoted here so the
    // composer's signature and feasibility see the compute step.
    crate::shape::promote_stated_rules(&mut plan, intent);
    backstop(intent, &mut plan);
    reconcile_refund_backstop(&mut plan, &proposal.regions);
    plan.unknowns.dedup();
    if !plan.unknowns.is_empty() {
        crate::finding(
            out,
            DiagnosticKind::Unknown,
            "intent",
            "The semantic plan contains unresolved requested work; no substitute workflow was emitted.",
        );
        for unknown in &plan.unknowns {
            crate::finding(out, DiagnosticKind::Unknown, "intent", unknown.clone());
        }
        crate::question(
            out,
            "intent.clarification",
            "Supply a complete replacement request including all work still wanted. It explicitly replaces the earlier intent.",
            QuestionType::Text,
        );
        record_ledger(out, &crate::ledger::Ledger::extract(&plan));
        out.provenance.plan = Some(plan_record(&plan, None));
        return None;
    }
    if plan.steps.is_empty() && plan.effects.is_empty() {
        reject(out, "the proposal names no operation and no effect");
        return None;
    }
    Some(plan)
}

/// Regions the proposal left unaccounted, or named as unknown. A model that cannot say what a
/// span of the request is for has not understood it; nothing is dropped silently.
fn accounting_gaps(intent: &str, regions: &[ProposedRegion]) -> Vec<String> {
    let mut gaps = Vec::new();
    if regions.is_empty() {
        // A seat that returned no regions is not penalized here; the anchored evidence of
        // steps and effects remains the floor.
        return gaps;
    }
    let mut covered = vec![false; intent.len()];
    for region in regions {
        let text = region.text.trim();
        if text.is_empty() {
            continue;
        }
        let mut from = 0;
        while let Some(pos) = intent.get(from..).and_then(|rest| rest.find(text)) {
            let start = from + pos;
            let end = start + text.len();
            for flag in covered.iter_mut().take(end).skip(start) {
                *flag = true;
            }
            from = end;
        }
        if region.role == "unknown" {
            gaps.push(format!(
                "The proposal could not map this part of the request: {text}"
            ));
        }
    }
    // Any uncovered run of meaningful characters is an unaccounted region.
    let mut run = String::new();
    let mut runs = Vec::new();
    for (index, ch) in intent.char_indices() {
        let flagged = covered.get(index).copied().unwrap_or(true);
        if flagged {
            if run.trim().chars().filter(|c| c.is_alphanumeric()).count() >= 12 {
                runs.push(run.trim().to_owned());
            }
            run.clear();
        } else {
            run.push(ch);
        }
    }
    if run.trim().chars().filter(|c| c.is_alphanumeric()).count() >= 12 {
        runs.push(run.trim().to_owned());
    }
    for text in runs {
        gaps.push(format!(
            "The proposal does not account for this part of the request: {text}"
        ));
    }
    gaps
}

/// Two writes are one effect only when they name the same file; a write that names no
/// file joins the recognized one, a write to another file is its own effect.
fn same_write(verb: EffectVerb, existing: &str, proposed: &str) -> bool {
    verb != EffectVerb::Write
        || match (
            crate::paths::single_file(existing),
            crate::paths::single_file(proposed),
        ) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        }
}

pub(super) fn reject(out: &mut CompileOutcome, why: &str) {
    crate::finding(
        out,
        DiagnosticKind::Unknown,
        "authoring_plan",
        format!("The semantic plan was not assembled: {why}."),
    );
}
