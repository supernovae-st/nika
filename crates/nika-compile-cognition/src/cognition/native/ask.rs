// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! An answer that writes no candidate and asks (pack 113: an empty candidate beside questions
//! or gaps is the incomplete ask path). Its questions face the laws a candidate's questions
//! face, less the placeholder no candidate can declare; a gap is asked under a key its clause
//! names, never its position; and an answered value is never asked again. Genuine, the human
//! answers first and no call is bought; refused, the diagnostics go
//! back like a candidate's, within the same repair budget. Nothing is parsed, nothing is kept
//! for replay (the answers author the request again), and an ask is never Ready.

use super::{Answer, Question, Round, Talk, journal, knowledge, revision};
use crate::fidelity::Diagnostic;
use crate::{CompileOutcome, DiagnosticKind, QuestionType};
use serde_json::{Value, json};

/// The questions an ask was admitted with, and the options each offers.
pub(super) type Admitted = Vec<(Question, Vec<Value>)>;

/// Whether an answer writes nothing and asks: a candidate empty or blank (lines that join to
/// whitespace alone) beside a question or a gap.
pub(super) fn only(answer: &Answer) -> bool {
    answer.candidate.trim().is_empty()
        && (!answer.questions.is_empty() || answer.gaps.iter().any(|gap| !gap.trim().is_empty()))
}

/// One ask round, journaled with what it asked: genuine, the human answers first (with the
/// questions as admitted); refused, its diagnostics go back to the seat.
pub(super) fn round(
    round: u32,
    answer: Answer,
    text: String,
    intent: &str,
    talk: &mut Talk,
) -> Round {
    let (admitted, diagnostics) = match admitted(intent, &answer, talk) {
        Ok(admitted) => (admitted, Vec::new()),
        Err(diagnostic) => (Vec::new(), vec![diagnostic]),
    };
    talk.rounds.push(json!({
        "round": round,
        "asked": answer.questions.iter().map(|q| q.key.clone()).collect::<Vec<_>>(),
        "gaps": gaps(&answer),
        "gaps_dropped": dropped(&answer),
        "notes": answer.notes.clone(),
        "diagnostics": diagnostics.iter().map(|d| json!({"kind": d.kind, "message": d.message})).collect::<Vec<_>>(),
    }));
    if diagnostics.is_empty() {
        return Round::Asked(Box::new(answer), admitted);
    }
    super::send_back(String::new(), text, diagnostics, talk)
}

/// The questions of an ask, admitted: at most eight, each key once and never an answered one,
/// a label the human reads, `text` or `literal`, then the laws of a candidate's questions.
fn admitted(
    intent: &str,
    answer: &Answer,
    talk: &Talk,
) -> Result<Vec<(Question, Vec<Value>)>, Diagnostic> {
    let refuse = |message: String| {
        Err(Diagnostic {
            kind: "question",
            message,
        })
    };
    if answer.questions.len() > 8 {
        return refuse(format!(
            "an ask carries at most eight questions, not {}",
            answer.questions.len()
        ));
    }
    for (n, question) in answer.questions.iter().enumerate() {
        let key = &question.key;
        if answer.questions[..n].iter().any(|q| &q.key == key) {
            return refuse(format!("the question `{key}` is asked twice"));
        }
        if talk.answered.contains(key) {
            return refuse(format!(
                "the question `{key}` is already answered: write the answer into the candidate, never ask it again"
            ));
        }
        if question.label.trim().is_empty() {
            return refuse(format!(
                "the question `{key}` has no label: say what the human decides"
            ));
        }
        if !matches!(question.answer_type.as_str(), "" | "text" | "literal") {
            return refuse(format!(
                "the question `{key}` has answer_type `{}`: a question takes `text` or `literal`",
                question.answer_type
            ));
        }
    }
    for clause in gaps(answer) {
        let key = gap_key(clause);
        if talk.answered.contains(&key) {
            return refuse(format!(
                "the gap « {clause} » is already answered (`{key}`): write that answer into the candidate, never ask it again"
            ));
        }
    }
    super::admitted_questions(intent, "", &answer.questions, talk.observed.as_ref())
}

/// A gap's answer key, from its clause: the same clause keeps its key however the seat orders
/// or numbers its gaps, and a reworded clause is another question, so an answer only ever
/// settles the clause the human read when giving it.
pub(super) fn gap_key(clause: &str) -> String {
    let digest = knowledge::sha256(clause);
    format!("gap.{}", digest.get(..12).unwrap_or(digest.as_str()))
}

/// The clauses an ask could not realize, as the human reads them.
fn gaps(answer: &Answer) -> Vec<&str> {
    answer
        .gaps
        .iter()
        .map(|gap| gap.trim())
        .filter(|gap| !gap.is_empty())
        .take(revision::KEPT_GAPS)
        .collect()
}

/// The clauses reported beyond what one ask carries: counted and stated, never silently lost.
fn dropped(answer: &Answer) -> usize {
    let reported = answer.gaps.iter().filter(|gap| !gap.trim().is_empty());
    reported.count().saturating_sub(revision::KEPT_GAPS)
}

/// An ask settles: the journal line, the seat's questions asked as admitted (mandatory; a
/// column the request leaves open is a choice among the observed ones), every gap a finding
/// and a question, any gap beyond what an ask carries stated, and no plan to replay. The
/// status stays incomplete and no candidate exists.
pub(super) fn conclude(
    answer: &Answer,
    admitted: &Admitted,
    talk: &Talk,
    out: &mut CompileOutcome,
) {
    crate::finding(
        out,
        DiagnosticKind::Applied,
        "authoring_native",
        journal::line(&talk.rounds),
    );
    let mut route = talk.route.clone();
    route.push("native: asked".to_owned());
    crate::record_route(out, &route);
    for (question, options) in admitted {
        ask(question, options, out);
    }
    let beyond = dropped(answer);
    if beyond > 0 {
        crate::finding(
            out,
            DiagnosticKind::Missed,
            "gap",
            format!(
                "the seat reported {beyond} more gap(s) than one ask carries ({}): they were not asked",
                revision::KEPT_GAPS
            ),
        );
    }
    for clause in gaps(answer) {
        let key = gap_key(clause);
        crate::finding(
            out,
            DiagnosticKind::Missed,
            "gap",
            format!(
                "the seat wrote no candidate and could not realize « {clause} »: answer `{key}` with how it should be done, or \"drop\""
            ),
        );
        crate::question(
            out,
            &key,
            &format!("« {clause} » cannot be realized yet: say how it should be done, or drop it"),
            QuestionType::Text,
        );
        if let Some(asked) = out.questions.last_mut() {
            "No candidate exists yet: the seat needs this clause settled to write one."
                .clone_into(&mut asked.why);
        }
    }
    // The cold plan is superseded by the seat's ask: the answers author the request again.
    out.provenance.plan = None;
    if let Some(native) = out
        .provenance
        .decision
        .as_mut()
        .and_then(|decision| decision.get_mut("native"))
    {
        native["asked"] = json!(admitted.iter().map(|(q, _)| &q.key).collect::<Vec<_>>());
    }
}

/// One admitted question, asked as the seat wrote it: its label and reason, mandatory.
fn ask(question: &Question, options: &[Value], out: &mut CompileOutcome) {
    let answer_type = if question.answer_type == "literal" {
        QuestionType::Literal
    } else if options.is_empty() {
        QuestionType::Text
    } else {
        QuestionType::Choice
    };
    crate::question(out, &question.key, &question.label, answer_type);
    if let Some(asked) = out.questions.last_mut() {
        if !question.why.trim().is_empty() {
            asked.why.clone_from(&question.why);
        }
        asked.options = options
            .iter()
            .filter_map(|o| {
                Some(nika_compile::ChoiceOffer::new(
                    o["key"].as_str()?,
                    o["label"].as_str()?,
                ))
            })
            .collect();
    }
}
