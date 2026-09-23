// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Native authoring (treatment D of the A/B/C/D/E comparison): a strong seat writes the
//! `.nika` candidate itself, from the authoring workspace's knowledge, and the compiler judges
//! it as it judges any source — the strict parser, the pure Check, then deterministic fidelity
//! laws that compare the candidate to the ORIGINAL request and the reader's floor (every
//! source the request names is read, every destination written, every stated approval gates
//! its effect through `nika:prompt`, no prohibited effect, no invented path or host) — and
//! sends every refusal back as structured diagnostics for a bounded repair round. A candidate
//! that survives is READY once its business questions are answered; one that does not is a
//! recorded failure with its last diagnostics, never a substitute. The seat never asks a human
//! for jq, a glob or any internal syntax: those are the compiler's diagnostics, not questions.

use super::knowledge::{self, Reference};
use super::{
    AuthoringPolicy, CompileOutcome, CompileRequest, DiagnosticKind, QuestionType, Strategy,
};
use crate::fidelity::{self, Diagnostic};
use crate::types::{EditChange, Input};
use crate::{CompileDiagnostic, CompileError, CompileQuestion, CompileStatus, lexicon::Reading};
use nika_kernel::ai::provider::{
    ContentBlock, InferResponse, Message, ProviderInferDyn, Role, StopReason,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// The seat's answer: the candidate, its business questions, the clauses it could not realize.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Answer {
    pub(super) candidate: String,
    #[serde(default, deserialize_with = "nullable_questions")]
    pub(super) questions: Vec<Question>,
    #[serde(default, deserialize_with = "super::nullable_vec")]
    pub(super) gaps: Vec<String>,
    #[serde(default, deserialize_with = "super::nullable_string")]
    pub(super) notes: String,
}

#[derive(serde::Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub(super) struct Question {
    pub(super) key: String,
    pub(super) label: String,
    #[serde(default, deserialize_with = "super::nullable_string")]
    pub(super) answer_type: String,
    #[serde(default, deserialize_with = "super::nullable_string")]
    pub(super) why: String,
}

pub(super) fn nullable_questions<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<Question>, D::Error> {
    use serde::Deserialize as _;
    Ok(Option::<Vec<Question>>::deserialize(d)?.unwrap_or_default())
}

fn schema() -> Value {
    serde_json::from_str(include_str!("../../assets/native_answer_schema.json"))
        .unwrap_or_else(|_| json!({"type": "object"}))
}

/// Whether a cold outcome calls for the native strategy: a question that hands the human a
/// machine's problem (a rewrite, a jq expression, a glob), or a dead end (no candidate and
/// nothing to answer). A cold outcome waiting on business values is not a failure.
pub(super) fn escalates(out: &CompileOutcome) -> bool {
    // A refusal is the floor (a bypassed approval, a literal-only policy): no door reopens it.
    if out.status == CompileStatus::Refused {
        return false;
    }
    // A seat that failed, timed out or was cut at its cap is not asked again through another
    // door: the provider finding stands, and the calls stay bounded.
    if out
        .diagnostics
        .iter()
        .any(|d| d.target == "authoring_provider")
    {
        return false;
    }
    let machine = out.questions.iter().any(|q| {
        matches!(
            q.key.as_str(),
            "intent.clarification" | "const.rule_expression" | "const.source_glob"
        )
    });
    machine || (out.candidate.is_none() && out.questions.is_empty())
}

/// The floor the reader states before any seat writes: a request that reuses, skips or
/// presupposes an approval is refused at the native door too, with zero calls.
pub(super) fn floor_refuses(reading: &Reading, out: &mut CompileOutcome) -> bool {
    let Some(refusal) = reading
        .plan
        .unknowns
        .iter()
        .find(|u| u.contains("approval-bypass"))
    else {
        return false;
    };
    super::super::finding(
        out,
        DiagnosticKind::Refused,
        "authoring_native",
        refusal.clone(),
    );
    out.status = CompileStatus::Refused;
    out.candidate = None;
    true
}

/// The facts the reader holds the candidate to, stated to the seat as data.
fn floor(intent: &str, reading: &Reading) -> Value {
    let plan = &reading.plan;
    json!({
        "sources": crate::hot::stated_sources(intent),
        "destinations": crate::hot::stated_destinations(intent),
        "effects": plan.effects.iter().map(|e| json!({"verb": e.verb.word(), "target": e.target, "policy": e.policy.word()})).collect::<Vec<_>>(),
        "obligations": plan.obligations.iter().map(|o| o.kind.word()).collect::<Vec<_>>(),
        "trigger": plan.trigger,
        "constraints": plan.constraints,
    })
}

/// The status and the open questions of the cold round, kept in the record as data.
pub(super) fn cold_report(out: &CompileOutcome) -> Value {
    let status = match out.status {
        CompileStatus::Ready => "ready",
        CompileStatus::Incomplete => "incomplete",
        CompileStatus::Refused => "refused",
    };
    json!({
        "status": status,
        "questions": out.questions.iter().map(|q| q.key.clone()).collect::<Vec<_>>(),
        "diagnostics": out.diagnostics.iter().filter(|d| d.kind != DiagnosticKind::Applied).map(|d| d.message.clone()).collect::<Vec<_>>(),
    })
}

/// One conversation with the seat: its messages so far, the journal of every round and the
/// last diagnostics (a repeat is no progress).
pub(super) struct Talk {
    pub(super) messages: Vec<Message>,
    pub(super) rounds: Vec<Value>,
    pub(super) last: Option<Vec<Diagnostic>>,
    /// The last candidate the laws refused: the record keeps its text, so a refusal can be
    /// read (a hash names nothing).
    pub(super) refused: Option<String>,
    pub(super) route: Vec<String>,
    /// The values the human answered: a candidate may carry them without inventing them.
    pub(super) allowed: Vec<String>,
    /// The repair principles a knowledge snapshot wires to diagnostic codes, for the repair
    /// message.
    pub(super) repairs: std::collections::BTreeMap<String, Vec<String>>,
    /// The observed world of the stated files, when the host read one: a column, field, key
    /// or value name is stated there, never asked.
    pub(super) observed: Option<Value>,
}

impl Talk {
    /// A conversation opened on the system message and the opening: no round yet, the
    /// answered values allowed, the observed world and the repair principles at hand.
    pub(super) fn open(
        system: String,
        opening: String,
        route: Vec<String>,
        allowed: Vec<String>,
        request: &CompileRequest,
    ) -> Self {
        Self {
            messages: vec![
                Message::text(Role::System, system),
                Message::text(Role::User, opening),
            ],
            rounds: Vec::new(),
            last: None,
            refused: None,
            route,
            allowed,
            observed: request.knowledge.clone(),
            repairs: request
                .authoring_knowledge
                .as_ref()
                .map(|pack| pack.repairs.clone())
                .unwrap_or_default(),
        }
    }
}

/// The cold round's own report stays in the record; the native round starts clean.
pub(super) fn cold(out: &mut CompileOutcome) -> Cold {
    let cold = Cold {
        report: cold_report(out),
        questions: std::mem::take(&mut out.questions),
        diagnostics: std::mem::take(&mut out.diagnostics),
    };
    out.candidate = None;
    out.status = CompileStatus::Incomplete;
    cold
}

/// What the cold round left when the native door opened: its report for the record, its
/// questions and diagnostics, kept in case the door never judges a candidate.
pub(super) struct Cold {
    pub(super) report: Value,
    pub(super) questions: Vec<CompileQuestion>,
    pub(super) diagnostics: Vec<CompileDiagnostic>,
}

/// What one round decided.
enum Round {
    /// The candidate passed every law.
    Accepted(Answer),
    /// The diagnostics went back to the seat.
    Repair,
    /// The seat returned the same refused candidate: the budget ends honestly.
    Stalled,
    /// The call failed or the answer was not decodable: nothing more to send.
    Stop,
}

/// Author natively: the opening call, then at most `policy.repairs` repair calls, each judged
/// by the same laws; the outcome carries the receipt of every call, the references sent and
/// every round's diagnostics. A previous cold outcome's receipt and record are kept.
pub(super) async fn author<P: ProviderInferDyn>(
    intent: &str,
    reading: &Reading,
    policy: &AuthoringPolicy,
    provider: &P,
    request: &CompileRequest,
    mut route: Vec<String>,
    mut out: CompileOutcome,
) -> Result<CompileOutcome, CompileError> {
    let cold = cold(&mut out);
    if floor_refuses(reading, &mut out) {
        route.push("native: refused by the floor".to_owned());
        super::record_route(&mut out, &route);
        out.provenance.strategy = Some(Strategy::Native);
        return Ok(out);
    }
    let Prelude {
        references,
        callables,
        sent,
        revision,
        opening,
        allowed,
    } = prelude(intent, reading, request);
    let mut talk = Talk::open(
        system_message(&references, &callables),
        opening.to_string(),
        route,
        allowed,
        request,
    );
    let mut accepted: Option<Answer> = None;
    for round in 0..=policy.repairs.min(5) {
        match exchange(
            round, &mut talk, intent, reading, policy, provider, &mut out,
        )
        .await
        {
            Round::Accepted(answer) => {
                accepted = Some(answer);
                break;
            }
            Round::Repair => {}
            Round::Stalled => {
                talk.route.push("native: no progress".to_owned());
                break;
            }
            Round::Stop => break,
        }
    }
    record(
        &mut out,
        request,
        &cold,
        &talk,
        &sent,
        accepted.as_ref(),
        revision,
    );
    conclude(
        intent,
        reading,
        request,
        accepted.as_ref(),
        &talk,
        cold,
        &mut out,
    );
    out.provenance.strategy = Some(Strategy::Native);
    Ok(out)
}

/// The native record of a conversation: the identity, the knowledge pack, the references sent,
/// every round, whether a candidate was accepted (else the last refused text), the revision.
pub(super) fn record(
    out: &mut CompileOutcome,
    request: &CompileRequest,
    cold: &Cold,
    talk: &Talk,
    sent: &[Value],
    accepted: Option<&Answer>,
    revision: Option<(&str, &str)>,
) {
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    decision["cold"] = cold.report.clone();
    decision["native"] = json!({
        "identity": knowledge::identity(),
        "knowledge": request.authoring_knowledge.as_ref().map(|pack| json!({
            "identity": pack.identity,
            "selection": pack.selection,
        })),
        "references": sent,
        "rounds": talk.rounds.clone(),
        "accepted": accepted.is_some(),
        "refused_source": accepted.is_none().then(|| talk.refused.clone()).flatten(),
        "revision": revision.map(|(source, words)| json!({
            "base_sha256": knowledge::sha256(source),
            "change": words,
            "delta": accepted.and_then(|answer| {
                let base = crate::edit::literal_projection(source)?;
                let revised = crate::edit::literal_projection(&answer.candidate)?;
                Some(nika_compile_reader::candidate::delta(&base, &revised))
            }),
        })),
    });
    out.provenance.decision = Some(decision);
}

/// Every string scalar of a document, once: the literals a base candidate already carries.
fn string_leaves(doc: &Value) -> Vec<String> {
    fn walk(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::String(s) if !s.is_empty() && !s.contains("${{") => {
                if !out.contains(s) {
                    out.push(s.clone());
                }
            }
            Value::Array(items) => items.iter().for_each(|v| walk(v, out)),
            Value::Object(map) => map.values().for_each(|v| walk(v, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(doc, &mut out);
    out
}

/// What the native round opens with: the references recalled for the request (the embedded
/// pack's, then the knowledge door's), the callables they name, the receipt of what was sent,
/// the revision when the request is an edit, the opening message, the literals the laws allow.
pub(super) struct Prelude<'a> {
    pub(super) references: Vec<Reference>,
    pub(super) callables: Vec<Reference>,
    pub(super) sent: Vec<Value>,
    pub(super) revision: Option<(&'a str, &'a str)>,
    pub(super) opening: Value,
    pub(super) allowed: Vec<String>,
}

pub(super) fn prelude<'a>(
    intent: &str,
    reading: &Reading,
    request: &'a CompileRequest,
) -> Prelude<'a> {
    let mut references = knowledge::references(intent, 2);
    if let Some(pack) = &request.authoring_knowledge {
        references.extend(pack.references.iter().map(|r| Reference {
            id: r.id.clone(),
            kind: match r.kind.as_str() {
                "pattern" => "pattern",
                "block" => "block",
                "example" => "example",
                "skill" => "skill",
                _ => "reference",
            },
            text: r.text.clone(),
        }));
    }
    let callables = knowledge::callables(&knowledge::builtins_of(&references));
    let sent: Vec<Value> = references
        .iter()
        .chain(callables.iter())
        .map(Reference::receipt)
        .collect();
    let revision = match &request.input {
        Input::Edit {
            source,
            change: EditChange::Text(words),
        } => Some((source.as_str(), words.as_str())),
        _ => None,
    };
    let opening = json!({
        "request": intent,
        "facts_the_compiler_holds_you_to": floor(intent, reading),
        "observed_world": request.knowledge,
        "answers_already_given": request.answers,
        "base_candidate": revision.map(|(source, _)| source),
        "change": revision.map(|(_, words)| words),
    });
    let mut allowed = fidelity::allowed_values(&request.answers);
    if let Some((source, _)) = revision {
        // The base's own literals were the earlier request's or the human's: never invented.
        allowed.extend(
            crate::edit::literal_projection(source)
                .map(|doc| string_leaves(&doc))
                .unwrap_or_default(),
        );
    }
    Prelude {
        references,
        callables,
        sent,
        revision,
        opening,
        allowed,
    }
}

/// One round: the call, the decoded answer, the judge's verdict, the journal entry, and the
/// diagnostics sent back when the candidate is refused.
async fn exchange<P: ProviderInferDyn>(
    round: u32,
    talk: &mut Talk,
    intent: &str,
    reading: &Reading,
    policy: &AuthoringPolicy,
    provider: &P,
    out: &mut CompileOutcome,
) -> Round {
    let role = if round == 0 {
        "native"
    } else {
        "native-repair"
    };
    let Some(response) =
        super::call_with_schema(policy, provider, role, talk.messages.clone(), schema(), out).await
    else {
        talk.rounds.push(json!({"round": round, "call": "failed"}));
        return Round::Stop;
    };
    let Some((answer, text)) = decode::<Answer>(&response, "native", round, talk, out) else {
        return Round::Stop;
    };
    let diagnostics = judge(
        intent,
        reading,
        &answer.candidate,
        &answer.questions,
        &talk.allowed,
        talk.observed.as_ref(),
    );
    talk.rounds.push(json!({
        "round": round,
        "candidate_sha256": knowledge::sha256(&answer.candidate),
        "questions": answer.questions.iter().map(|q| q.key.clone()).collect::<Vec<_>>(),
        "gaps": answer.gaps.clone(),
        "notes": answer.notes.clone(),
        "diagnostics": diagnostics.iter().map(|d| json!({"kind": d.kind, "message": d.message})).collect::<Vec<_>>(),
    }));
    if diagnostics.is_empty() {
        return Round::Accepted(answer);
    }
    talk.refused = Some(answer.candidate.clone());
    if talk.last.as_ref() == Some(&diagnostics) {
        return Round::Stalled;
    }
    talk.messages.push(Message::text(Role::Assistant, text));
    talk.messages.push(Message::text(
        Role::User,
        repair_message(&diagnostics, &talk.repairs),
    ));
    talk.last = Some(diagnostics);
    Round::Repair
}

/// The seat's text decoded as `T`, or the honest reason a talk ends: an answer cut at the
/// authoring cap is named as such (a reasoning seat may spend the whole budget before its
/// answer — the eco-60 `DeepSeek` V4 Pro lane, 2026-09-22: 12/60 answers cut at exactly 16384
/// output tokens), a text that is not one complete JSON object, a JSON that is not the answer.
pub(super) fn decode<T: serde::de::DeserializeOwned>(
    response: &InferResponse,
    what: &str,
    round: u32,
    talk: &mut Talk,
    out: &mut CompileOutcome,
) -> Option<(T, String)> {
    let text = match response.content.as_slice() {
        [ContentBlock::Text { text }] if response.stop_reason == StopReason::EndTurn => {
            text.clone()
        }
        _ if response.stop_reason == StopReason::MaxTokens => {
            talk.rounds
                .push(json!({"round": round, "answer": "cut at the authoring cap"}));
            super::super::finding(
                out,
                DiagnosticKind::Unknown,
                "authoring_native",
                format!(
                    "The seat's answer was cut at the authoring cap ({} output tokens): raise --authoring-max-tokens, or seat a model that does not spend the budget on its reasoning.",
                    response.usage.output_tokens
                ),
            );
            return None;
        }
        _ => {
            talk.rounds
                .push(json!({"round": round, "answer": "not one complete JSON text"}));
            super::super::finding(
                out,
                DiagnosticKind::Unknown,
                "authoring_native",
                "The seat did not return one complete JSON text; nothing was assembled.",
            );
            return None;
        }
    };
    match serde_json::from_str::<T>(super::first_json_object(&text).unwrap_or(&text)) {
        Ok(answer) => Some((answer, text)),
        Err(error) => {
            talk.rounds
                .push(json!({"round": round, "answer": format!("not a {what} answer: {error}")}));
            super::super::finding(
                out,
                DiagnosticKind::Unknown,
                "authoring_native",
                format!(
                    "The seat's answer is not a {what} answer ({error}); nothing was assembled."
                ),
            );
            None
        }
    }
}

/// The end of the conversation: an accepted candidate settles; an exhausted budget is an
/// honest finding and the clarification question, never a substitute workflow.
pub(super) fn conclude(
    intent: &str,
    reading: &Reading,
    request: &CompileRequest,
    accepted: Option<&Answer>,
    talk: &Talk,
    cold: Cold,
    out: &mut CompileOutcome,
) {
    // The repair loop is visible on every transport through the diagnostics, not only in
    // the provenance: what the judge refused, and what the seat repaired.
    super::super::finding(
        out,
        DiagnosticKind::Applied,
        "authoring_native",
        journal_line(&talk.rounds),
    );
    let mut route = talk.route.clone();
    let judged = talk
        .rounds
        .iter()
        .any(|r| r.get("candidate_sha256").is_some() || r.get("sketch_sha256").is_some());
    match accepted {
        Some(answer) => {
            route.push("native: accepted".to_owned());
            super::record_route(out, &route);
            settle(
                intent,
                reading.plan.trigger.as_deref(),
                &answer.candidate,
                &answer.questions,
                &answer.gaps,
                request,
                out,
            );
        }
        None if !judged => {
            // The door never judged a candidate (a failed call, an answer that was not an
            // answer): the previous round's questions and diagnostics stand, nothing is
            // replaced by a clarification the human could not act on.
            route.push("native: no candidate".to_owned());
            super::record_route(out, &route);
            out.questions.extend(cold.questions);
            out.diagnostics.extend(cold.diagnostics);
            if out.questions.is_empty() {
                super::super::question(
                    out,
                    "intent.clarification",
                    "Supply a complete replacement request including all work still wanted. It explicitly replaces the earlier intent.",
                    QuestionType::Text,
                );
            }
        }
        None => {
            route.push("native: exhausted".to_owned());
            super::record_route(out, &route);
            super::super::finding(
                out,
                DiagnosticKind::RequiresHuman,
                "authoring_native",
                "No candidate survived the fidelity laws within the repair budget; the rounds and their diagnostics are recorded, no substitute workflow was emitted.",
            );
            super::super::question(
                out,
                "intent.clarification",
                "Supply a complete replacement request including all work still wanted. It explicitly replaces the earlier intent.",
                QuestionType::Text,
            );
        }
    }
}

/// One line a human reads: what each round of the conversation decided and why.
fn journal_line(rounds: &[Value]) -> String {
    let mut parts = Vec::new();
    for round in rounds {
        let n = round["round"].as_u64().unwrap_or_default();
        if let Some(call) = round["call"].as_str() {
            parts.push(format!("round {n}: the call {call}"));
        } else if let Some(answer) = round["answer"].as_str() {
            parts.push(format!("round {n}: {answer}"));
        } else {
            let diagnostics = round["diagnostics"].as_array().cloned().unwrap_or_default();
            if diagnostics.is_empty() {
                parts.push(format!("round {n}: accepted"));
            } else {
                let heads: Vec<String> = diagnostics
                    .iter()
                    .take(3)
                    .map(|d| {
                        d["message"]
                            .as_str()
                            .unwrap_or_default()
                            .chars()
                            .take(90)
                            .collect()
                    })
                    .collect();
                parts.push(format!(
                    "round {n}: refused ({} diagnostic(s): {})",
                    diagnostics.len(),
                    heads.join(" · ")
                ));
            }
        }
    }
    format!(
        "The seat wrote {} candidate(s) from the authoring card and the recalled references; the judge read each one: {}.",
        rounds.len(),
        parts.join("; ")
    )
}

pub(super) fn system_message(references: &[Reference], callables: &[Reference]) -> String {
    let mut text = String::from(knowledge::card());
    text.push_str("\n\n# Callable contracts (the stdlib page, cut)\n");
    for callable in callables {
        text.push_str(&callable.text);
        text.push_str("\n\n");
    }
    text.push_str("\n# References recalled for this request (priors, never prisons)\n");
    for reference in references {
        text.push_str("## ");
        text.push_str(&reference.id);
        text.push('\n');
        if reference.kind == "skeleton" {
            text.push_str("```yaml\n");
            text.push_str(&reference.text);
            text.push_str("\n```\n\n");
        } else {
            text.push_str(&reference.text);
            text.push_str("\n\n");
        }
    }
    text
}

pub(super) fn repair_message(
    diagnostics: &[Diagnostic],
    repairs: &std::collections::BTreeMap<String, Vec<String>>,
) -> String {
    let mut text = String::from(
        "COMPILER DIAGNOSTICS on your candidate. Return the complete corrected JSON answer (candidate, questions, gaps, notes); fix every item, change nothing the request did not ask.\n",
    );
    for (n, d) in diagnostics.iter().enumerate() {
        use std::fmt::Write as _;
        let _ = writeln!(text, "{}. [{}] {}", n + 1, d.kind, d.message);
    }
    // The repair principles the knowledge snapshot wires to the codes these diagnostics
    // name (Foundry, 2026-09-23: the weakest seat 9 → 13 correct with them beside the
    // findings); a capable seat repairs from the message alone.
    let mut principles: Vec<&str> = Vec::new();
    for code in diagnostics.iter().flat_map(|d| codes_in(&d.message)) {
        for line in repairs.get(&code).into_iter().flatten() {
            if !principles.contains(&line.as_str()) && principles.len() < 6 {
                principles.push(line);
            }
        }
    }
    if !principles.is_empty() {
        text.push_str("\nRepair knowledge for these findings:\n");
        for line in principles {
            text.push_str("- ");
            text.push_str(line);
            text.push('\n');
        }
    }
    text
}

/// The diagnostic codes a message names (`NIKA-PARSE-022`, `NIKA-AUTH-006`).
fn codes_in(message: &str) -> Vec<String> {
    message
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .filter(|w| w.starts_with("NIKA-") && w.len() >= 10)
        .map(|w| w.trim_end_matches('-').to_owned())
        .collect()
}

/// Slug fragments that name a machine's construct rather than a business value.
const MACHINE_SLUGS: &str = include_str!("../../assets/machine_slugs.txt");

/// Slug endings that name a column, field, key or value of a file — stated by the observed
/// world when the host read the file, never asked then.
const STRUCTURE_SLUGS: &str = include_str!("../../assets/structure_slugs.txt");

/// The observed world in one line — `./tickets.json: id, status, topic (status: closed | open)`
/// — or None when the host read no stated file.
fn observed_names(observed: Option<&Value>) -> Option<String> {
    let observed = observed?;
    let rows = observed
        .get("observed")
        .and_then(Value::as_array)
        .or_else(|| observed.as_array())?;
    let lines: Vec<String> = rows
        .iter()
        .filter_map(|row| {
            let path = row.get("path")?.as_str()?;
            let columns: Vec<&str> = row
                .get("columns")?
                .as_array()?
                .iter()
                .filter_map(Value::as_str)
                .collect();
            let values: Vec<String> = row
                .get("values")
                .and_then(Value::as_object)
                .map(|values| {
                    values
                        .iter()
                        .map(|(column, set)| {
                            let set: Vec<&str> = set
                                .as_array()
                                .map(|s| s.iter().filter_map(Value::as_str).collect())
                                .unwrap_or_default();
                            format!("{column}: {}", set.join(" | "))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let values = if values.is_empty() {
                String::new()
            } else {
                format!(" ({})", values.join("; "))
            };
            Some(format!("{path}: {}{values}", columns.join(", ")))
        })
        .collect();
    (!lines.is_empty()).then(|| lines.join(" · "))
}

/// A candidate's questions, admitted: `const.<snake_slug>` keys only, each declared under
/// `const:` in the candidate as a placeholder, at most eight; never a machine's construct;
/// never a name the observed world states.
fn admitted_questions(
    candidate: &str,
    questions: &[Question],
    observed: Option<&Value>,
) -> Result<Vec<Question>, Diagnostic> {
    let world = observed_names(observed);
    let doc = crate::edit::literal_projection(candidate);
    let consts = doc
        .as_ref()
        .and_then(|d| d.get("const"))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut admitted = Vec::new();
    for question in questions.iter().take(8) {
        let Some(slug) = question.key.strip_prefix("const.") else {
            return Err(Diagnostic {
                kind: "question",
                message: format!(
                    "the question `{}` is not a `const.<snake_slug>` business value; a jq expression, a glob or any syntax is the compiler's to write, never a question",
                    question.key
                ),
            });
        };
        // A value named after a machine's construct is the compiler's problem in a human's
        // clothes: a glob over a stated folder, a jq program, a regex, a selector are written,
        // never asked (the reality check of 2026-09-22 measured the jq question 5 times).
        if MACHINE_SLUGS.lines().any(|m| slug.contains(m)) {
            return Err(Diagnostic {
                kind: "question",
                message: format!(
                    "the question `{}` asks the human for a machine's construct; write it yourself from the request (a glob over the stated folder, the jq program, the pattern) and, when the request names no place at all, ask for the FOLDER or FILE as `const.<slug>` (a path, never a glob or a program)",
                    question.key
                ),
            });
        }
        // A column, field, key or value name of a file the host read is stated in the
        // observed world, never asked (2026-09-22 22:5xZ, claude-code/sonnet: `const.status_field`,
        // `const.open_value`, `const.region_column` beside the observed header and value set).
        if let Some(world) = world.as_deref()
            && STRUCTURE_SLUGS.lines().any(|s| slug.ends_with(s))
        {
            return Err(Diagnostic {
                kind: "question",
                message: format!(
                    "the question `{}` asks the human for a column, field, key or value name; the observed world states them — {world} — write those exact names and values into the candidate, never a question",
                    question.key
                ),
            });
        }
        if slug == "channel" || slug.ends_with("_channel") {
            return Err(Diagnostic {
                kind: "question",
                message: format!(
                    "the question `{}` asks the human for a channel; a send whose destination the request leaves open is ONE placeholder, `const.send_endpoint` (an HTTPS endpoint: `nika:notify` with `channel: webhook` and `target: \"${{{{ const.send_endpoint }}}}\"`, or a `nika:fetch` POST) — the channel a request does not name is `webhook`",
                    question.key
                ),
            });
        }
        if slug.is_empty()
            || !slug
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            || !consts.contains_key(slug)
        {
            return Err(Diagnostic {
                kind: "question",
                message: format!(
                    "the question `{}` needs a snake_case slug declared under `const:` in the candidate as a placeholder (`{slug}: \"\"`)",
                    question.key
                ),
            });
        }
        admitted.push(question.clone());
    }
    Ok(admitted)
}

/// The judge: the strict parser, the pure Check, then the fidelity laws against the original
/// request and the reader's floor. Every refusal is one structured diagnostic.
pub(super) fn judge(
    intent: &str,
    reading: &Reading,
    candidate: &str,
    questions: &[Question],
    allowed: &[String],
    observed: Option<&Value>,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let Some(doc) = admit(candidate, questions, &mut out) else {
        return out;
    };
    fidelity::laws(intent, &reading.plan, &doc, allowed, &[], &mut out);
    if let Err(diagnostic) = admitted_questions(candidate, questions, observed) {
        out.push(diagnostic);
    }
    out.dedup();
    out
}

/// The strict parser and the pure Check (its refusals become diagnostics, the laws still
/// run), then the literal projection the laws read; None when nothing can be read. A
/// `nika:fetch` whose URL rides a placeholder the seat asks for (`const.<slug>` declared
/// empty) is not refused for its unknown host: the answer grants it (`bake`).
fn admit(candidate: &str, questions: &[Question], out: &mut Vec<Diagnostic>) -> Option<Value> {
    // A seat that folds its newlines into spaces (Scaleway gpt-oss-120b, 2026-09-23: 2 577
    // characters on one line, `tasks:` present and unreadable) is told what happened, not
    // only what the parser could not find.
    if candidate.len() > 240 && candidate.lines().count() <= 2 {
        out.push(Diagnostic {
            kind: "parse",
            message: format!(
                "the candidate arrived as ONE line ({} characters, no newline): a `.nika` is a multi-line YAML document — write a real newline (`\\n` inside the JSON string) after every field and every task, never a space in its place",
                candidate.len()
            ),
        });
    }
    let wf = match crate::parse(candidate) {
        Ok(wf) => wf,
        Err(error) => {
            out.push(Diagnostic {
                kind: "parse",
                message: format!("the candidate does not parse: {error}"),
            });
            return None;
        }
    };
    let doc = crate::edit::literal_projection(candidate);
    if let Some(wild) = doc.as_ref().and_then(wildcard_grant) {
        out.push(Diagnostic {
            kind: "permits",
            message: format!(
                "permits.net.http carries `{wild}`: a wildcard is not a boundary; grant the exact host the request states as a literal, or leave `http: []` when the host comes from an answer (the compiler grants the answered host)"
            ),
        });
        return None;
    }
    let tolerated = doc
        .as_ref()
        .is_some_and(|d| placeholder_host_asked(d, questions));
    let report = nika_check::check(&wf);
    if !report.is_clean() {
        let mut kept = 0;
        for violation in report
            .conformance
            .iter()
            .filter(|v| !(tolerated && v.code == "NIKA-SEC-004"))
            .take(6)
        {
            out.push(Diagnostic {
                kind: "check",
                message: format!("{}: {}", violation.code, violation.message),
            });
            kept += 1;
        }
        for finding in report
            .findings
            .iter()
            .filter(|f| matches!(f.severity, nika_check::FindingSeverity::Error))
            .filter(|f| !(tolerated && f.code.as_deref() == Some("NIKA-SEC-004")))
            .take(6)
        {
            out.push(Diagnostic {
                kind: "check",
                message: format!(
                    "{}: {}",
                    finding.code.as_deref().unwrap_or(finding.kind),
                    finding.message
                ),
            });
            kept += 1;
        }
        if kept == 0 && !tolerated {
            out.push(Diagnostic {
                kind: "check",
                message: "the check refuses the candidate (see its report)".to_owned(),
            });
        }
    }
    if doc.is_none() && out.is_empty() {
        out.push(Diagnostic {
            kind: "parse",
            message: "the candidate is not a literal YAML document".to_owned(),
        });
    }
    doc
}

/// Whether a `nika:fetch` URL rides a `const.<slug>` placeholder (declared empty) that the
/// seat asks for: its host is unknown until the answer, and the answer grants it.
/// The first wildcard entry of `permits.net.http`, if any (`*` · `*.example.com` · `**`).
fn wildcard_grant(doc: &Value) -> Option<String> {
    doc.get("permits")?
        .get("net")?
        .get("http")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .find(|h| h.contains('*'))
        .map(str::to_owned)
}

fn placeholder_host_asked(doc: &Value, questions: &[Question]) -> bool {
    let consts = doc.get("const").and_then(Value::as_object);
    let asked: Vec<&str> = questions
        .iter()
        .filter_map(|q| q.key.strip_prefix("const."))
        .filter(|slug| {
            consts
                .and_then(|c| c.get(*slug))
                .and_then(Value::as_str)
                .is_some_and(str::is_empty)
        })
        .collect();
    if asked.is_empty() {
        return false;
    }
    doc.get("tasks")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .any(|(_, task)| {
            task.get("invoke")
                .and_then(|i| i.get("args"))
                .and_then(|a| a.get("url").or_else(|| a.get("target")))
                .and_then(Value::as_str)
                .is_some_and(|url| {
                    asked.iter().any(|slug| {
                        names_placeholder(url, slug) || bound_placeholder(task, url, slug)
                    })
                })
        })
}

fn names_placeholder(text: &str, slug: &str) -> bool {
    text.contains(&format!("const.{slug}"))
}

/// A URL written as `${{ with.<name> }}…` reaches the placeholder when that binding names it:
/// the seat binds an endpoint through `with:` as often as it writes it in the argument.
fn bound_placeholder(task: &Value, url: &str, slug: &str) -> bool {
    let Some(rest) = url.trim_start().strip_prefix("${{") else {
        return false;
    };
    let Some(name) = rest
        .trim_start()
        .strip_prefix("with.")
        .and_then(|r| r.split(['}', ' ', '.', '/']).next())
    else {
        return false;
    };
    task.get("with")
        .and_then(|w| w.get(name))
        .and_then(Value::as_str)
        .is_some_and(|bound| names_placeholder(bound, slug))
}

/// An accepted candidate settles: its questions are asked (mandatory), the answered ones are
/// baked into the candidate, the `model` answer replaces the placeholder, and the source goes
/// through the same finish as every candidate. The record replays it with zero calls.
fn settle(
    intent: &str,
    trigger: Option<&str>,
    candidate: &str,
    questions: &[Question],
    gaps: &[String],
    request: &CompileRequest,
    out: &mut CompileOutcome,
) {
    let admitted =
        admitted_questions(candidate, questions, request.knowledge.as_ref()).unwrap_or_default();
    let gaps: Vec<&str> = gaps
        .iter()
        .map(|g| g.trim())
        .filter(|g| !g.is_empty())
        .take(8)
        .collect();
    let mut record = json!({
        "strategy": Strategy::Native.word(),
        "intent_sha256": super::intent_sha256(intent),
        "source": candidate,
        "questions": admitted.iter().map(|q| json!({"key": q.key, "label": q.label, "answer_type": q.answer_type, "why": q.why})).collect::<Vec<_>>(),
        "gaps": gaps,
        "trigger": trigger,
    });
    // What the candidate BUILDS, in the plan record's own vocabulary (operations · effects ·
    // obligations · bindings), so provenance reads the native strategy as it reads the
    // others; `operations_from` says the reading is of the bytes, not of the intent.
    if let Some(doc) = crate::edit::literal_projection(candidate) {
        let built = nika_compile_reader::candidate::plan_of_document(&doc).to_json();
        for key in ["operations", "effects", "obligations", "bindings"] {
            record[key] = built[key].clone();
        }
        record["operations_from"] = json!("candidate");
    }
    out.provenance.plan = Some(record.clone());
    apply(&record, request, out);
}

/// The trigger the request states is recorded beside the candidate, never in it: the same
/// reading the assembler makes (a cadence, a time of day, an event), bound to the schedule
/// answers when the human gives them.
fn record_trigger(record: &Value, request: &CompileRequest, out: &mut CompileOutcome) {
    let Some(phrase) = record["trigger"].as_str().filter(|p| !p.trim().is_empty()) else {
        return;
    };
    if matches!(
        crate::trigger::classify(phrase),
        crate::trigger::TriggerForm::Sequence
    ) {
        return;
    }
    let mut plan = crate::plan::Plan::default();
    plan.trigger = Some(phrase.to_owned());
    if let Some(mut trigger) = crate::trigger::requirement(&plan, false) {
        let mut recognized = BTreeSet::new();
        crate::trigger::bind_schedule(&mut trigger, request, out, &mut recognized);
        super::super::finding(
            out,
            DiagnosticKind::Applied,
            "trigger",
            crate::trigger::note(&trigger),
        );
        out.requested_trigger = Some(trigger);
    }
}

/// Replay a native record: the same candidate with this round's answers, zero calls.
pub(super) fn replay(
    intent: &str,
    record: &Value,
    request: &CompileRequest,
    out: &mut CompileOutcome,
) {
    super::record_route(out, &["replayed native candidate".to_owned()]);
    if record["intent_sha256"].as_str() != Some(super::intent_sha256(intent).as_str()) {
        super::super::finding(
            out,
            DiagnosticKind::Unknown,
            "recorded_plan",
            "The recorded native candidate was written for another request; compile the intent again without it.",
        );
        return;
    }
    out.provenance.strategy = Some(Strategy::Native);
    out.provenance.plan = Some(record.clone());
    apply(record, request, out);
}

/// Bake the answers into the recorded source and finish it: every unanswered question stays
/// mandatory and no candidate is emitted until all are answered; the `model` placeholder takes
/// the human's model.
fn apply(record: &Value, request: &CompileRequest, out: &mut CompileOutcome) {
    let mut source = record["source"].as_str().unwrap_or_default().to_owned();
    let mut open = false;
    record_trigger(record, request, out);
    for question in record["questions"].as_array().into_iter().flatten() {
        let key = question["key"].as_str().unwrap_or_default();
        if let Some(literal) = request.answers.get(key) {
            if !bake(&mut source, question, literal, out) {
                open = true;
            }
        } else {
            open = true;
            ask(question, out);
        }
    }
    if source.contains("model: mock/echo") && !seat_model(&mut source, request, out) {
        open = true;
    }
    dispose_gaps(record, request, out);
    if open {
        // Questions stay; the candidate waits for them (the same contract as the assembler).
        return;
    }
    super::super::finish(source, out);
}

/// The clauses the seat could not realize never vanish: each is a `Missed` diagnostic on every
/// transport (the clause verbatim, the candidate does not carry it) and an optional question
/// `gap.<n>` the human answers — « drop » or how it should be done — recorded as the human's
/// disposition in the decision record, never baked into the candidate (MP §3.1: nothing
/// important silently disappears into READY).
fn dispose_gaps(record: &Value, request: &CompileRequest, out: &mut CompileOutcome) {
    let gaps: Vec<&str> = record["gaps"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if gaps.is_empty() {
        return;
    }
    let mut dispositions = Vec::new();
    for (n, clause) in gaps.iter().enumerate() {
        let key = format!("gap.{}", n + 1);
        if let Some(answer) = request.answers.get(&key) {
            dispositions.push(json!({"clause": clause, "disposition": answer}));
            continue;
        }
        super::super::finding(
            out,
            DiagnosticKind::Missed,
            "gap",
            format!(
                "the seat could not realize « {clause} »; the candidate does not carry it — answer `{key}` with \"drop\" to accept that, or say in words how it should be done"
            ),
        );
        out.questions.push(super::super::CompileQuestion {
            key,
            label: format!(
                "« {clause} » is not in the candidate: drop it, or say how it should be done"
            ),
            answer_type: QuestionType::Text,
            why: "A clause the request states never disappears silently; the human disposes of it."
                .to_owned(),
            mandatory: false,
            options: Vec::new(),
        });
    }
    if !dispositions.is_empty() {
        let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
        decision["gap_dispositions"] = json!(dispositions);
        out.provenance.decision = Some(decision);
    }
}

/// Bake one answered question into the source's `const:`; false when it could not be.
fn bake(source: &mut String, question: &Value, literal: &str, out: &mut CompileOutcome) -> bool {
    let key = question["key"].as_str().unwrap_or_default();
    let slug = key.trim_start_matches("const.");
    let Some(value) = super::super::literal_answer(Some(literal), key, out) else {
        return false;
    };
    let value = if question["answer_type"] == "literal" {
        value
    } else {
        match value {
            Value::String(s) => Value::String(s),
            other => Value::String(other.to_string()),
        }
    };
    let baked = crate::edit::literal_projection(source.as_str()).and_then(|before| {
        let mut after = before.clone();
        after["const"][slug] = value.clone();
        let edited = crate::edit_source::emit(source.as_str(), &before, &after, slug)?;
        // An answered endpoint also grants its host: the boundary is the compiler's to
        // complete from the answer, never the seat's to guess.
        if !grant_host(&mut after, &value) {
            return Some(edited);
        }
        let before = crate::edit::literal_projection(&edited)?;
        crate::edit_source::emit_at(&edited, &before, &after, &["permits", "net", "http"])
    });
    if let Some(edited) = baked {
        *source = edited;
        super::super::finding(
            out,
            DiagnosticKind::Applied,
            key,
            "Answer applied to the candidate's const.",
        );
        true
    } else {
        super::super::finding(
            out,
            DiagnosticKind::Missed,
            key,
            "The answer could not be baked into the candidate's const; it was not applied.",
        );
        false
    }
}

/// Add the host of an answered URL to `permits.net.http` when that list exists and lacks it.
fn grant_host(after: &mut Value, value: &Value) -> bool {
    let Some(host) = value.as_str().and_then(host_of) else {
        return false;
    };
    let Some(list) = after
        .pointer_mut("/permits/net/http")
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    if list.iter().any(|h| h.as_str() == Some(host)) {
        return false;
    }
    list.push(Value::String(host.to_owned()));
    true
}

/// The host of an `http(s)://` URL, without its port: the form `permits.net.http` lists (the
/// assembler grants `url.host_str()`; the loopback declassification compares exact hosts).
fn host_of(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let host = if authority.starts_with('[') {
        authority
            .find(']')
            .map_or(authority, |close| &authority[..=close])
    } else {
        authority.split(':').next().unwrap_or(authority)
    };
    (!host.is_empty()).then_some(host)
}

/// Ask one recorded business question, mandatory.
fn ask(question: &Value, out: &mut CompileOutcome) {
    out.questions.push(super::super::CompileQuestion {
        key: question["key"].as_str().unwrap_or_default().to_owned(),
        label: question["label"].as_str().unwrap_or_default().to_owned(),
        answer_type: if question["answer_type"] == "literal" {
            QuestionType::Literal
        } else {
            QuestionType::Text
        },
        why: question["why"]
            .as_str()
            .filter(|w| !w.is_empty())
            .unwrap_or("The compiler cannot invent this business value.")
            .to_owned(),
        mandatory: true,
        options: Vec::new(),
    });
}

/// The model placeholder: the human's `model` answer replaces it; else the question is asked.
fn seat_model(source: &mut String, request: &CompileRequest, out: &mut CompileOutcome) -> bool {
    let Some(literal) = request.answers.get("model") else {
        super::super::question(
            out,
            "model",
            "Which model runs the language work of this workflow? Answer a `provider/name` string.",
            QuestionType::Text,
        );
        return false;
    };
    match super::super::literal_answer(Some(literal), "model", out) {
        Some(Value::String(model)) if model.contains('/') => {
            *source = source.replacen("model: mock/echo", &format!("model: {model}"), 1);
            true
        }
        _ => {
            super::super::finding(
                out,
                DiagnosticKind::Missed,
                "model",
                "Answer must be a `provider/name` string.",
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome() -> CompileOutcome {
        crate::initial()
    }

    #[test]
    fn a_refusal_or_a_provider_failure_never_escalates_and_a_machine_question_does() {
        let mut refused = outcome();
        refused.status = CompileStatus::Refused;
        assert!(!escalates(&refused));
        let mut failed = outcome();
        crate::finding(
            &mut failed,
            DiagnosticKind::Unknown,
            "authoring_provider",
            "timed out",
        );
        assert!(!escalates(&failed));
        let mut machine = outcome();
        crate::question(
            &mut machine,
            "const.rule_expression",
            "which jq?",
            QuestionType::Text,
        );
        assert!(escalates(&machine));
        let mut business = outcome();
        crate::question(
            &mut business,
            "const.send_endpoint",
            "where?",
            QuestionType::Text,
        );
        assert!(!escalates(&business));
        assert!(escalates(&outcome()));
    }

    #[test]
    fn an_asked_endpoint_is_tolerated_through_a_with_binding() {
        let asked = |key: &str| Question {
            key: key.to_owned(),
            label: String::new(),
            answer_type: String::new(),
            why: String::new(),
        };
        let doc = |url: &str, bound: &str| {
            json!({
                "const": {"crm_endpoint": ""},
                "tasks": {"lookup": {
                    "with": {"endpoint": bound},
                    "invoke": {"tool": "nika:fetch", "args": {"url": url}},
                }},
            })
        };
        let questions = vec![asked("const.crm_endpoint")];
        assert!(placeholder_host_asked(
            &doc("${{ with.endpoint }}/contacts", "${{ const.crm_endpoint }}"),
            &questions
        ));
        assert!(placeholder_host_asked(
            &doc("${{ const.crm_endpoint }}/contacts", "unused"),
            &questions
        ));
        assert!(!placeholder_host_asked(
            &doc("${{ with.endpoint }}", "${{ inputs.endpoint }}"),
            &questions
        ));
        assert!(!placeholder_host_asked(
            &doc("${{ with.endpoint }}", "${{ const.crm_endpoint }}"),
            &[]
        ));
    }

    #[test]
    fn a_machine_construct_is_never_a_question_and_a_placeholder_must_be_declared() {
        let candidate = "nika: x\nconst:\n  source_folder: \"\"\ntasks: {}\n";
        let asked = |key: &str| Question {
            key: key.to_owned(),
            label: String::new(),
            answer_type: "text".to_owned(),
            why: String::new(),
        };
        assert!(admitted_questions(candidate, &[asked("const.source_glob")], None).is_err());
        assert!(admitted_questions(candidate, &[asked("const.filter_expression")], None).is_err());
        assert!(admitted_questions(candidate, &[asked("const.missing")], None).is_err());
        assert!(
            admitted_questions(candidate, &[asked("const.notify_channel")], None).is_err(),
            "a channel is never a question (the live run of 2026-09-22 asked two for one send)"
        );
        assert!(admitted_questions(candidate, &[asked("model")], None).is_err());
        // A name the observed world states is never a question; without an observed world the
        // same slug is a declared placeholder like any other.
        let shaped = "nika: x\nconst:\n  status_field: \"\"\n  region_column: \"\"\ntasks: {}\n";
        let world = json!({"observed": [{"path": "./tickets.json", "kind": "json", "columns": ["id", "status", "topic"], "values": {"status": ["closed", "open"]}}]});
        let Err(refused) = admitted_questions(shaped, &[asked("const.status_field")], Some(&world))
        else {
            panic!("a field name the observed world states is refused");
        };
        assert!(
            refused
                .message
                .contains("./tickets.json: id, status, topic (status: closed | open)"),
            "{}",
            refused.message
        );
        assert!(admitted_questions(shaped, &[asked("const.region_column")], Some(&world)).is_err());
        assert_eq!(
            admitted_questions(shaped, &[asked("const.status_field")], None).map(|q| q.len()),
            Ok(1)
        );
        assert_eq!(
            admitted_questions(candidate, &[asked("const.source_folder")], None).map(|q| q.len()),
            Ok(1)
        );
    }

    #[test]
    fn the_first_json_object_is_read_whatever_wraps_it() {
        assert_eq!(
            super::super::first_json_object(
                "Sure!\n```json\n{\"a\": \"}\", \"b\": {\"c\": 1}}\n```"
            ),
            Some("{\"a\": \"}\", \"b\": {\"c\": 1}}")
        );
        assert_eq!(
            super::super::first_json_object("  {\"a\":1}  "),
            Some("{\"a\":1}")
        );
        assert_eq!(super::super::first_json_object("no object"), None);
        assert_eq!(super::super::first_json_object("{\"open\": true"), None);
    }

    #[test]
    fn an_answered_url_grants_its_host_without_its_port() {
        assert_eq!(
            host_of("https://hooks.example.invalid/recap"),
            Some("hooks.example.invalid")
        );
        assert_eq!(host_of("http://127.0.0.1:8793/hook"), Some("127.0.0.1"));
        assert_eq!(host_of("http://[::1]:8080/x"), Some("[::1]"));
        assert_eq!(host_of("./out/report.md"), None);
        let mut doc = json!({"permits": {"net": {"http": []}}});
        assert!(grant_host(
            &mut doc,
            &json!("https://hooks.example.invalid/recap")
        ));
        assert!(!grant_host(
            &mut doc,
            &json!("https://hooks.example.invalid/again")
        ));
        assert_eq!(
            doc["permits"]["net"]["http"],
            json!(["hooks.example.invalid"])
        );
        let mut none = json!({"permits": {}});
        assert!(!grant_host(&mut none, &json!("https://x.invalid/")));
    }
}
