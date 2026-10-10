// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The document door (R5 · C13): a fresh CREATE composed as the complete document, in the whole
//! language. The author writes the `.nika` itself, any construct the strict parser accepts, and
//! composes the admitted components of the lent catalogue into it through the document's own
//! operations: resolved by identity, bound at their holes, expanded and receipted. No restricted
//! round has to fail first, and no part of the request is dropped to fit a projection. What an
//! answer makes of the document and the record that follows it are the door's own laws
//! ([`nika_compile_seats::foundry::document::create`]); this module asks, judges and settles.
//!
//! A document is judged as any candidate ([`native::judge`]: the strict parser, the pure Check,
//! the fidelity laws against the original request), settled by the native conclusion (its
//! questions asked, its answers baked, its record replayable), then examined by the same evidence
//! and whole-request judgment as the sketch door's. A refusal goes back as named findings; the
//! next round restates the whole document or states operations over the last one. The rounds run
//! as the policy states them (no count: until no progress, a refused or failed call, or Stop).
//! Once the document is READY, [`bind`](nika_compile_seats::foundry::document::create::bind)
//! binds its native record to the final bytes.

use super::native::{
    self, Answer, Question, Shaped, Talk, cold, conclude, floor_refuses, judge_resolved, prelude,
    system_message,
};
use super::rehearsal::Rehearsals;
use super::sketch::{
    Exit, Step, call, diagnostics_record, evidence, examine, judged_attempts, next_round, reopen,
    repair, stop_reason, within,
};
use super::{AuthoringPolicy, CompileOutcome, CompileRequest, Strategy};
use crate::decide::DecisionSeat;
use crate::fidelity::Diagnostic;
use crate::fidelity::resolution::Resolution;
use crate::{CompileError, lexicon::Reading};
use nika_compile_seats::foundry::ComponentCatalog;
use nika_compile_seats::foundry::document::{
    self,
    create::{Made, language, made, record},
};
use nika_compile_seats::repairs;
use nika_kernel::ai::provider::{Message, ProviderInferDyn, Role};
use serde_json::{Value, json};

pub(super) use nika_compile_seats::foundry::document::create::{ROUTE, bind, receipts};

/// What the door tells the author, after the card and the references.
const INSTRUCTION: &str = include_str!("document_create/instructions.md");
/// The whole-source answer the recovery reads, widened here by the document's operations.
const SCHEMA: &str = include_str!("../../assets/native_answer_schema.json");
/// The selections an author states rather than copies, with their provenance.
const RESOLUTIONS: &str = include_str!("../../assets/resolutions_schema.json");
const AGAIN: &str = "\nAnswer again in the same {\"candidate\", \"candidate_lines\", \"operations\", \"questions\", \"gaps\", \"notes\", \"resolutions\"} object: the whole corrected document, or operations over the last one with `candidate` left empty.";
const SPENT: &str =
    "The evidence refused this document and the rounds are spent: nothing is READY.";
const REPEATED: &str = "The evidence refused this document with findings the author had already been asked to repair, so Nika stopped the door: nothing is READY.";

/// The author's answer: the document, the operations over it, and the native settlement fields.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DocumentAnswer {
    #[serde(default, deserialize_with = "crate::cognition::nullable_default")]
    candidate: String,
    #[serde(default, deserialize_with = "crate::cognition::nullable_default")]
    candidate_lines: Vec<String>,
    #[serde(default, deserialize_with = "crate::cognition::nullable_default")]
    operations: Vec<Value>,
    #[serde(default, deserialize_with = "crate::cognition::nullable_default")]
    questions: Vec<Question>,
    #[serde(default, deserialize_with = "crate::cognition::nullable_default")]
    gaps: Vec<String>,
    #[serde(default, deserialize_with = "crate::cognition::nullable_default")]
    notes: String,
    #[serde(default, deserialize_with = "crate::cognition::nullable_default")]
    resolutions: Vec<Value>,
}

impl Shaped for DocumentAnswer {
    const KEYS: &'static [&'static str] = &["candidate", "candidate_lines", "operations"];
}

impl DocumentAnswer {
    /// The document text the answer writes: `candidate`, else its lines joined; none when empty.
    fn written(&self) -> Option<String> {
        let text = match self.candidate.trim() {
            "" => self.candidate_lines.join("\n"),
            _ => self.candidate.clone(),
        };
        (!text.trim().is_empty()).then_some(text)
    }
}

/// The one schema the author answers under: the whole-source answer and the operations.
fn answer_schema() -> Value {
    let mut schema = super::sketch::schema(SCHEMA);
    schema["properties"]["operations"] = document::answer_schema().0;
    schema["properties"]["resolutions"] = super::sketch::schema(RESOLUTIONS);
    if let Some(required) = schema["required"].as_array_mut() {
        required.extend([json!("operations"), json!("resolutions")]);
    }
    schema
}

/// The journal entry of one round: what the answer made (or why nothing), never its free text.
fn round_entry(
    round: u32,
    answer: &DocumentAnswer,
    made: Result<&Made, &[String]>,
    found: &[Diagnostic],
) -> Value {
    let notes = super::receipt::withheld(&answer.notes, &[], "document notes");
    let mut entry = json!({"round": round, "phase": "document",
        "operations": answer.operations.len(), "notes": notes,
        "diagnostics": diagnostics_record(found)});
    match made {
        Ok(made) => {
            entry["mode"] = json!(made.mode);
            entry["candidate_sha256"] = json!(super::knowledge::sha256(&made.source));
            entry["changed"] = json!(made.changed);
            let ids: Vec<&Value> = (made.receipts.iter())
                .map(|r| &r["component"]["id"])
                .collect();
            entry["components"] = json!(ids);
        }
        Err(why) => entry["refused"] = json!(why),
    }
    entry
}

/// The author's rounds until a document passes the judge: each answer made and judged, a refusal
/// repaired, a repeat no progress; a failed call or an answer off the wire ends them. `last`
/// keeps the last document the door made, for operations over it. A gap first declared after a
/// refusal is told back once with that refusal while a round is left (R7); declared again, it is
/// accepted, and the door surfaces it.
async fn draft<P: ProviderInferDyn>(
    (intent, reading, policy): (&str, &Reading, &AuthoringPolicy),
    (provider, catalog): (&P, Option<&dyn ComponentCatalog>),
    talk: &mut Talk,
    last: &mut Option<Made>,
    out: &mut CompileOutcome,
) -> Option<(Answer, Made)> {
    let mut round = next_round(talk);
    // The gaps declared before any refusal, and whether a later one was told back already.
    let (mut honest, mut told) = (Vec::new(), false);
    while within(policy.repairs, round) {
        let role = if round == 0 {
            "document"
        } else {
            "document-repair"
        };
        let refusal = talk.last.clone().unwrap_or_default();
        let schema = answer_schema();
        let called = call::<DocumentAnswer, P>(talk, round, role, schema, policy, provider, out);
        let (answer, text) = called.await?;
        if refusal.is_empty() {
            honest.extend(answer.gaps.iter().cloned());
        }
        let stated = made(answer.written(), &answer.operations, last.as_ref(), catalog);
        // The values the request authorizes without spelling them, each with its provenance.
        let selections = Resolution::read_all(&answer.resolutions);
        let (found, made) = match stated {
            Ok(made) => {
                let (allowed, clarified) = (&talk.allowed, &talk.clarified);
                let questions = &answer.questions;
                let observed = talk.observed.as_ref();
                let mut found = match &selections {
                    Ok(rows) => judge_resolved(
                        intent,
                        reading,
                        &made.source,
                        questions,
                        (allowed, rows),
                        &[],
                        clarified,
                        observed,
                    ),
                    Err(why) => vec![Diagnostic {
                        kind: "resolution",
                        message: format!("the selections cannot be read: {why}"),
                    }],
                };
                if found.is_empty() && !told && within(policy.repairs, round + 1) {
                    found = repairs::gaps_after_refusal(&answer.gaps, &honest, &refusal);
                    told = !found.is_empty();
                }
                talk.rounds
                    .push(round_entry(round, &answer, Ok(&made), &found));
                (found, Some(made))
            }
            Err(why) => {
                let found: Vec<Diagnostic> = (why.iter())
                    .map(|message| Diagnostic {
                        kind: "document",
                        message: message.clone(),
                    })
                    .collect();
                talk.rounds
                    .push(round_entry(round, &answer, Err(&why), &found));
                (found, None)
            }
        };
        round += 1;
        if let Some(made) = made {
            if found.is_empty() {
                talk.messages.push(Message::text(Role::Assistant, text));
                talk.resolved = (selections.unwrap_or_default().iter())
                    .map(Resolution::to_json)
                    .collect();
                let (questions, gaps) = (answer.questions, answer.gaps);
                let candidate = made.source.clone();
                return Some((
                    Answer {
                        candidate,
                        questions,
                        gaps,
                    },
                    made,
                ));
            }
            talk.refused = Some(made.source.clone());
            *last = Some(made);
        }
        if !repair(talk, text, found, AGAIN) {
            return None;
        }
    }
    None
}

/// The talk the door opens: the card, the conventions and the qualified pack's references, the
/// door's instruction and the language's complete schema; then the request's facts with every
/// admitted component of the lent catalogue. Returns the talk and the receipts of what it sent.
fn opened(
    (intent, reading, request): (&str, &Reading, &CompileRequest),
    catalog: Option<&dyn ComponentCatalog>,
    route: Vec<String>,
) -> (Talk, Vec<Value>) {
    let native::Prelude {
        references,
        callables,
        mut sent,
        mut opening,
        allowed,
        ..
    } = prelude(intent, reading, request);
    let components = document::components(catalog);
    let language = language();
    let sha = super::knowledge::sha256;
    sent.push(
        json!({"id": "spec:workflow.schema.json", "kind": "language",
        "bytes": language.len(), "sha256": sha(&language)}),
    );
    let listed = components.to_string();
    sent.push(json!({"id": "catalogue:components", "kind": "components",
        "release": catalog.map(|c| c.release().version),
        "count": components.as_array().map_or(0, Vec::len),
        "bytes": listed.len(), "sha256": sha(&listed)}));
    opening["components"] = components;
    let mut system = system_message(&references, &callables);
    system.push_str("\n\n");
    system.push_str(INSTRUCTION);
    system.push_str("\n\n# The language's complete schema (the Spec's workflow schema)\n");
    system.push_str(&language);
    let mut talk = Talk::open(system, opening.to_string(), route, allowed, request);
    talk.presented = json!(sent);
    (talk, sent)
}

/// The document door: the author's rounds, the native conclusion, the evidence and the
/// whole-request judgment, reopened from their findings within the policy.
pub(super) async fn author<P: ProviderInferDyn>(
    intent: &str,
    reading: &Reading,
    policy: &AuthoringPolicy,
    (provider, decision, rehearsals): (&P, Option<&dyn DecisionSeat>, &mut Rehearsals<'_>),
    request: &CompileRequest,
    mut route: Vec<String>,
    mut out: CompileOutcome,
) -> Result<CompileOutcome, CompileError> {
    // The cold round as the door opened on it: every attempt concludes against the same one.
    let opened_on = out.clone();
    let _ = cold(&mut out);
    if floor_refuses(reading, &mut out) {
        route.push("native: refused by the floor".to_owned());
        super::record_route(&mut out, &route);
        out.provenance.strategy = Some(Strategy::Native);
        return Ok(out);
    }
    let catalog = rehearsals.catalog();
    let (mut talk, sent) = opened((intent, reading, request), catalog, route);
    talk.remember_under(policy);
    // One round count spans the drafts and every reopening; one rehearsal per document they can
    // produce (no count: as many).
    let (last_round, attempts) = (policy.repairs, policy.repairs.map(|r| r.saturating_add(2)));
    let mut last = None;
    loop {
        let words = (intent, reading, policy);
        let drafted = draft(words, (provider, catalog), &mut talk, &mut last, &mut out).await;
        let mut done = out.clone();
        let words = (intent, reading, request);
        settle(
            words,
            &talk,
            &sent,
            drafted.as_ref(),
            (&opened_on, &mut done),
        );
        nika_compile_seats::judge::lent(catalog, &mut done);
        let Some((_, made)) = drafted else {
            return Ok(done);
        };
        let door = (intent, reading, policy, request);
        let (next, seats) = (next_round(&talk), (provider, decision, &mut *rehearsals));
        // A document whose judged defects end the rounds stays the preview (COLD's exit).
        let room = (attempts, Exit::Keep);
        match examine(door, seats, &mut talk, done, (next, last_round), room).await {
            Step::Done(judged) | Step::Withdrawn(judged, _) => return Ok(judged),
            Step::Reopen(mut judged, defects, verdict) => {
                if within(last_round, next) && reopen(&mut talk, defects.clone(), AGAIN) {
                    if let Some(verdict) = &verdict {
                        // A repair from the judge's verdict, named as COLD names its own; or
                        // bytes the room runs, restated after its refusal (A2).
                        let repair = format!("verify: repair {}", judged_attempts(&judged));
                        let restated = verdict
                            .defects
                            .is_empty()
                            .then_some(super::verify::RESTATED);
                        super::verify::route(&mut judged, restated.unwrap_or(&repair));
                    }
                    // The judge's calls belong to the door's one journal.
                    (out.provenance.authoring).clone_from(&judged.provenance.authoring);
                    (out.provenance.decision).clone_from(&judged.provenance.decision);
                    last = Some(made);
                    continue;
                }
                if let Some(verdict) = verdict {
                    // The room's refusal again, or no round left: held, as with no host.
                    if verdict.defects.is_empty() {
                        return Ok(super::verify::preserve_unjudged(judged, &verdict));
                    }
                    // The judge located the same defects again: no progress, the bytes kept.
                    let repairs = judged_attempts(&judged).saturating_sub(1);
                    let mut kept = super::verify::kept(judged, &verdict, repairs);
                    super::verify::route(&mut kept, "native: no progress");
                    return Ok(kept);
                }
                let why = stop_reason(within(last_round, next), &defects, (SPENT, REPEATED));
                evidence::refuse(&mut judged, why);
                return Ok(judged);
            }
        }
    }
}

/// One attempt's conclusion on `done`: the native record and settlement against the cold round
/// the door opened on, then the door's own record of how the document was made.
fn settle(
    (intent, reading, request): (&str, &Reading, &CompileRequest),
    talk: &Talk,
    sent: &[Value],
    drafted: Option<&(Answer, Made)>,
    (opened_on, done): (&CompileOutcome, &mut CompileOutcome),
) {
    let cold = cold(&mut opened_on.clone());
    let answer = drafted.map(|(answer, _)| answer);
    native::record(done, request, &cold, talk, sent, answer, None);
    conclude(intent, reading, request, answer, talk, cold, done);
    if let Some((_, made)) = drafted {
        record(intent, made, done);
        // The record keeps each selection with its provenance; none is a human answer.
        if !talk.resolved.is_empty()
            && let Some(plan) = done.provenance.plan.as_mut()
        {
            plan["resolutions"] = json!(talk.resolved);
        }
    }
    done.provenance.strategy = Some(Strategy::Native);
}

#[cfg(test)]
mod tests;
