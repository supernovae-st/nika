// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A revision stated over the complete document (R5): the seat's operations, or its whole
//! replacement, are applied to the base ([`document::apply`]), the result is finished by the
//! strict parser and Check, bound to its record and read by the round's judge. A refusal is told
//! back to the same seat in the same talk, the base intact: operations the document refuses, a
//! result Check refuses, or destination links stated where no destination edit applies. The seat
//! then states the revision again, until one holds, it brings nothing new (the same reasons
//! again: [`super::super::repair`] stops there), or the policy's explicit repair bound is reached;
//! a call Stop ends, or that gets no answer, ends the talk. Nothing of a refused revision is
//! applied, and a whole replacement never claims the base's bytes.

use nika_compile_seats::foundry::{ComponentCatalog, document};
use nika_kernel::ai::provider::ProviderInferDyn;
use serde_json::{Value, json};

use super::{Journal, Links, refuse, revision_change, source_original};
use crate::cognition::native::{self, Answer};
use crate::cognition::{AuthoringPolicy, CompileOutcome, CompileRequest, Strategy};
use crate::decide::DecisionSeat;
use crate::fidelity::Diagnostic;
use crate::lexicon;
use crate::types::Input;

/// What the seat is told after a refused revision, before it states it again.
const DOCUMENT_AGAIN: &str = "\nNothing of that revision was applied: the base is unchanged. State the change again over the base as `operations`, every reason above addressed, or, only when no operation can state it, as one whole `replace`.";

/// The schema of a revision stated over the document alone: its operations, or one whole
/// replacement, and the seat's notes.
pub(super) fn document_schema() -> Value {
    let (operations, replace) = document::answer_schema();
    json!({"type": "object", "additionalProperties": false,
        "required": ["operations", "replace", "notes"],
        "properties": {"notes": {"type": "string"}, "operations": operations, "replace": replace}})
}

/// The revision over the document, told back and stated again until it holds or ends.
pub(super) async fn document_settled<P: ProviderInferDyn>(
    request: &CompileRequest,
    (intent, reading): (&str, &lexicon::Reading),
    (policy, provider, decision): (&AuthoringPolicy, &P, Option<&dyn DecisionSeat>),
    journal: Journal<'_>,
    ((first, first_text), catalog): ((Links, String), Option<&dyn ComponentCatalog>),
    mut out: CompileOutcome,
) -> CompileOutcome {
    let Input::Edit { source: base, .. } = &request.input else {
        return out;
    };
    let carried = document::carried(request.plan.as_ref());
    let Journal {
        mut round,
        mut talk,
        sent,
        shown,
        cold,
    } = journal;
    talk.route.push(document::ROUTE.to_owned());
    // Every refusal told back is remembered (unbounded repairs), so a seat cycling between
    // refused statements (A, B, A) brings nothing new and ends, never loops.
    talk.remember_under(policy);
    let (mut links, mut text) = (first, first_text);
    loop {
        let why = match stated(base, &links, catalog, &carried) {
            Ok(applied) => {
                let done = finished(&applied.source);
                if done.status == crate::CompileStatus::Ready || !done.questions.is_empty() {
                    journaled(&mut talk, round, &links, &[]);
                    talk.messages.push(nika_kernel::ai::provider::Message::text(
                        nika_kernel::ai::provider::Role::Assistant,
                        text,
                    ));
                    let parts = (base.as_str(), &applied, done);
                    let seated = (policy, provider, decision);
                    let journal = (&talk, sent.as_slice(), shown, &cold);
                    return kept(request, (intent, reading), seated, journal, parts, out).await;
                }
                checked_refusal(&done)
            }
            Err(why) => why,
        };
        journaled(&mut talk, round, &links, &why);
        let diagnostics: Vec<Diagnostic> = (why.iter())
            .map(|message| Diagnostic {
                kind: "revision",
                message: message.clone(),
            })
            .collect();
        let bounded = policy.repairs.is_some_and(|last| round >= last);
        if bounded || !super::super::repair(&mut talk, text, diagnostics, DOCUMENT_AGAIN) {
            native::record(&mut out, request, &cold, &talk, &sent, None, shown);
            crate::record_route(&mut out, &talk.route);
            refuse(&mut out, &why);
            out.provenance.strategy = Some(Strategy::Native);
            return out;
        }
        round += 1;
        let called = super::super::call::<Links, P>(
            &mut talk,
            round,
            "revision-repair",
            document_schema(),
            policy,
            provider,
            &mut out,
        );
        let Some(next) = called.await else {
            native::record(&mut out, request, &cold, &talk, &sent, None, shown);
            crate::record_route(&mut out, &talk.route);
            out.provenance.strategy = Some(Strategy::Native);
            return out;
        };
        (links, text) = next;
    }
}

/// The operations (or the whole replacement) applied to the base, or why they were not: links
/// where no destination edit applies are refused, never guessed into operations.
fn stated(
    base: &str,
    links: &Links,
    catalog: Option<&dyn ComponentCatalog>,
    carried: &[Value],
) -> Result<document::Applied, Vec<String>> {
    if !links.over_the_document() {
        return Err(vec![
            "the change was stated as destination links where no destination edit applies (the base writes none, or the request it answers is unknown); state it as operations".to_owned(),
        ]);
    }
    document::apply(
        base,
        (&links.operations, links.replacement()),
        catalog,
        carried,
    )
}

/// The revised source finished as any candidate: the strict parser, then Check.
fn finished(source: &str) -> CompileOutcome {
    let mut done = crate::initial();
    nika_compile::finish(source.to_owned(), &mut done);
    done.provenance.strategy = Some(Strategy::Native);
    done
}

/// What Check (or the strict parser) refused of a revised source, in its own words.
fn checked_refusal(done: &CompileOutcome) -> Vec<String> {
    let why: Vec<String> = (done.diagnostics.iter())
        .map(|diagnostic| format!("{}: {}", diagnostic.target, diagnostic.message))
        .collect();
    if why.is_empty() {
        vec!["the revised workflow is not ready and no reason was given".to_owned()]
    } else {
        why
    }
}

/// One round of the revision as the journal keeps it: how much was stated, the notes withheld,
/// and every reason it was refused (none for the round kept).
fn journaled(talk: &mut native::Talk, round: u32, links: &Links, why: &[String]) {
    let notes = crate::cognition::receipt::withheld(&links.notes, &[], "revision notes");
    let stated = json!({"operations": links.operations.len(),
        "replaced": links.replacement().is_some(), "links": !links.over_the_document()});
    talk.rounds.push(json!({"round": round, "phase": "revision",
        "document": stated, "notes": notes, "refused": why}));
}

/// The revision kept: its record bound to the revised bytes, the talk journaled, the components
/// the bytes hold witnessed, and, when READY, the round's judge reading it.
async fn kept<P: ProviderInferDyn>(
    request: &CompileRequest,
    (intent, reading): (&str, &lexicon::Reading),
    (policy, provider, decision): (&AuthoringPolicy, &P, Option<&dyn DecisionSeat>),
    (talk, sent, shown, cold): (&native::Talk, &[Value], Option<(&str, &str)>, &native::Cold),
    (base, applied, mut done): (&str, &document::Applied, CompileOutcome),
    mut out: CompileOutcome,
) -> CompileOutcome {
    done.provenance.authoring = out.provenance.authoring.take();
    let resolved = source_original(request).map_or_else(
        || intent.to_owned(),
        |original| format!("{original}\n{}", revision_change(request)),
    );
    let sha = nika_compile::intent_sha256(intent);
    let record = document::record((base, &applied.source), &resolved, &sha, applied);
    let mut decision_record = done.provenance.decision.take().unwrap_or_else(|| json!({}));
    decision_record["document_revision"] = record["document_revision"].clone();
    done.provenance.decision = Some(decision_record);
    done.provenance.plan = Some(record);
    let answer = Answer {
        candidate: applied.source.clone(),
        questions: Vec::new(),
        gaps: Vec::new(),
    };
    native::record(&mut done, request, cold, talk, sent, Some(&answer), shown);
    crate::record_route(&mut done, &talk.route);
    // What reuse the bytes really hold: each composed component witnessed on the candidate.
    let qualification = json!({"by": null, "why": "a revision: components are composed by operations, none is qualified here"});
    crate::cognition::knowledge::reused(request, qualification, &applied.receipts, &mut done);
    if done.status != crate::CompileStatus::Ready {
        return done;
    }
    crate::cognition::verify::judged_native(
        intent,
        reading,
        policy,
        (provider, decision),
        request,
        done,
    )
    .await
}
