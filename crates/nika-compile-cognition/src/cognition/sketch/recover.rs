// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Source recovery (`AuthoringPolicy::source_recovery`, explicit, off by default): after the sketch
//! door's exhaustion on a CREATE, the same seat, talk and request authority answer the whole source
//! on the retired source wire, judged (`native::judge`), settled and examined like any candidate;
//! nothing is granted or reset. No failed seat, cut answer, open question or edit is recovered.

use super::native::{self, Answer, Question, Shaped, Talk, judge};
use super::{AuthoringPolicy, CompileOutcome, CompileRequest, DiagnosticKind, Filled};
use super::{Rehearsals, Step, diagnostics_record, evidence, examine, next_round, reopen, repair};
use crate::{CompileDiagnostic, fidelity::Diagnostic, lexicon::Reading};
use nika_kernel::ai::provider::{ContentBlock, Message, ProviderInferDyn, Role};
use serde_json::{Value, json};

/// The retired source door's answer, read only under an explicit recovery policy.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceAnswer {
    #[serde(default, deserialize_with = "crate::cognition::nullable_default")]
    candidate: String,
    #[serde(default, deserialize_with = "crate::cognition::nullable_default")]
    candidate_lines: Vec<String>,
    #[serde(default, deserialize_with = "crate::cognition::nullable_default")]
    questions: Vec<Question>,
    #[serde(default, deserialize_with = "crate::cognition::nullable_default")]
    gaps: Vec<String>,
    #[serde(default, deserialize_with = "crate::cognition::nullable_default")]
    notes: String,
}

impl Shaped for SourceAnswer {
    const KEYS: &'static [&'static str] = &["candidate", "candidate_lines"];
}

const RECOVER: &str = include_str!("../../../assets/native_source_recovery.md");
const SCHEMA: &str = include_str!("../../../assets/native_answer_schema.json");
const AGAIN: &str = "\nAnswer the whole corrected source again in the same {\"candidate\", \"candidate_lines\", \"questions\", \"gaps\", \"notes\"} object.";
pub(super) const ROUTE: &str = "native: source recovery after structured exhaustion";
const TARGET: &str = "authoring_recovery";
const OPENED: &str = "Source recovery (explicit operator policy) opened: the structured doors ended without an accepted candidate, and a whole-source request to the same seat was attempted under the same authority. Only a READY outcome states that a recovered source passed every check.";
const PASSED: &str = "The recovered source is seat-written: it passed the strict parser, the pure Check, the fidelity laws, the rehearsal when one was offered and the whole-request judgment. It carries no semantic record, so a later revision in words is kept or source-anchored, never semantic.";
const SPENT: &str = "The evidence refused the recovered source and the recovery rounds are spent: nothing is READY.";

/// The structured conclusion, or its recovery; `defects`: the evidence no reopening carried.
pub(super) async fn after<P: ProviderInferDyn>(
    door: (&str, &Reading, &AuthoringPolicy, &CompileRequest),
    (provider, rehearsals): (&P, &mut Rehearsals<'_>),
    talk: &mut Talk,
    (opened, sent): (&CompileOutcome, &[Value]),
    (structured, defects): (CompileOutcome, Vec<Diagnostic>),
) -> CompileOutcome {
    let (intent, reading, policy, request) = door;
    let cut = (talk.rounds.last()).is_some_and(|r| r["failure_class"] == "OUTPUT_TRUNCATED");
    let recoverable = policy.source_recovery > 0
        && matches!(request.input, crate::types::Input::Create(_))
        && structured.status == crate::CompileStatus::Incomplete
        && structured.candidate.is_none()
        && structured.questions.is_empty()
        && !cut
        && !(structured.diagnostics.iter()).any(|d| d.target == "authoring_provider");
    if !recoverable {
        return structured;
    }
    let rounds = policy.source_recovery.min(3);
    let history = structured.diagnostics;
    // The cold round as the door opened on it, every charge so far carried, never reset.
    let mut done = opened.clone();
    let _ = native::cold(&mut done);
    done.provenance.authoring = structured.provenance.authoring;
    let shown = |d: &&CompileDiagnostic| d.kind != DiagnosticKind::Applied;
    let findings: Vec<&str> = history.iter().filter(shown).map(|d| &*d.message).collect();
    let evidence: Vec<&str> = defects.iter().map(|d| d.message.as_str()).collect();
    let facts = json!({"previous_findings": findings, "evidence_defects": evidence,
        "last_refused_candidate": talk.refused});
    // The engine-controlled phase is marked on the System message itself, its text kept.
    let system = (talk.messages.first_mut()).filter(|m| matches!(m.role, Role::System));
    if let Some(ContentBlock::Text { text }) = system.and_then(|m| m.content.first_mut()) {
        *text = format!("{text}\n\n{RECOVER}");
    }
    say(talk, format!("SOURCE RECOVERY · compiler facts:\n{facts}"));
    talk.route.push(ROUTE.to_owned());
    talk.last = None;
    let mut used = 0;
    loop {
        let filled = source(door, provider, talk, (&mut used, rounds), &mut done).await;
        let mut settled = done.clone();
        let (words, pair) = ((intent, reading, request), (None, filled.as_ref()));
        super::settle(
            words,
            talk,
            (sent, None),
            Value::Null,
            pair,
            (opened, &mut settled),
        );
        note(&mut settled, &history, (used, rounds), filled.is_some());
        if filled.is_none() || settled.status != crate::CompileStatus::Ready {
            return kept(settled, &history);
        }
        // Round `next` is the one a reopening would spend; with no round left, refused is final.
        let (next, more) = (next_round(talk), used < rounds);
        let limits = (next, next.saturating_sub(u32::from(!more)));
        let allowance = policy.repairs.min(5) + 2 + rounds;
        let seats = (provider, &mut *rehearsals);
        match examine(door, seats, talk, settled, limits, allowance).await {
            Step::Done(out) | Step::Withdrawn(out, _) => return kept(out, &history),
            Step::Reopen(mut out, defects) => {
                if !more || !reopen(talk, defects, AGAIN) {
                    evidence::refuse(&mut out, SPENT.to_owned());
                    return kept(out, &history);
                }
                done.provenance.authoring = out.provenance.authoring;
            }
        }
    }
}

/// The source rounds within the policy: each answer judged, a refusal repaired, a repeat no
/// progress; a failed call or an answer off the wire ends the recovery.
async fn source<P: ProviderInferDyn>(
    (intent, reading, policy, _): (&str, &Reading, &AuthoringPolicy, &CompileRequest),
    provider: &P,
    talk: &mut Talk,
    (used, rounds): (&mut u32, u32),
    out: &mut CompileOutcome,
) -> Option<Filled> {
    while *used < rounds {
        let role = ["source-recovery", "source-recovery-repair"][usize::from(*used > 0)];
        *used += 1;
        let (round, schema) = (next_round(talk), super::schema(SCHEMA));
        let call = super::call::<SourceAnswer, P>(talk, round, role, schema, policy, provider, out);
        let (answer, text) = call.await?;
        let candidate = match answer.candidate.trim() {
            "" => answer.candidate_lines.join("\n"),
            _ => answer.candidate,
        };
        let (allowed, clarified, seen) = (&talk.allowed, &talk.clarified, talk.observed.as_ref());
        let found = judge(
            intent,
            reading,
            &candidate,
            &answer.questions,
            allowed,
            &[],
            clarified,
            seen,
        );
        talk.rounds.push(json!({"round": round, "phase": "recovery",
            "candidate_sha256": super::super::knowledge::sha256(&candidate),
            "notes": super::super::receipt::withheld(&answer.notes, &[], "recovery notes"),
            "diagnostics": diagnostics_record(&found)}));
        if found.is_empty() {
            talk.messages.push(Message::text(Role::Assistant, text));
            let (questions, gaps) = (answer.questions, answer.gaps);
            return Some((
                Answer {
                    candidate,
                    questions,
                    gaps,
                },
                Vec::new(),
            ));
        }
        talk.refused = Some(candidate);
        if !repair(talk, text, found, AGAIN) {
            return None;
        }
    }
    None
}

/// The recovery on the record: rounds stated and spent, acceptance, the findings it recovered from.
fn note(out: &mut CompileOutcome, history: &[CompileDiagnostic], used: (u32, u32), ok: bool) {
    let after: Vec<Value> = (history.iter())
        .filter(|d| d.kind != DiagnosticKind::Applied)
        .map(|d| json!({"target": d.target, "message": d.message}))
        .collect();
    if let Some(decision) = out.provenance.decision.as_mut() {
        decision["native"]["recovery"] =
            json!({"rounds": used.1, "spent": used.0, "accepted": ok, "after": after});
    }
    crate::finding(out, DiagnosticKind::Applied, TARGET, OPENED);
}

/// The conclusion: with no candidate, the structured findings it could not recover from stay;
/// only a READY outcome states that the recovered source passed every check.
fn kept(mut out: CompileOutcome, history: &[CompileDiagnostic]) -> CompileOutcome {
    if out.candidate.is_none() {
        out.diagnostics.splice(0..0, history.iter().cloned());
    } else if out.status == crate::CompileStatus::Ready {
        crate::finding(&mut out, DiagnosticKind::Applied, TARGET, PASSED);
    }
    out
}

/// One more user turn, joined to the last one when the talk already ends on the user's side.
fn say(talk: &mut Talk, text: String) {
    match talk.messages.last_mut() {
        Some(last) if matches!(last.role, Role::User) => {
            last.content.push(ContentBlock::Text { text });
        }
        _ => talk.messages.push(Message::text(Role::User, text)),
    }
}
