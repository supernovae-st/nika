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
use super::{AuthoringPolicy, CompileOutcome, CompileRequest, DiagnosticKind, Strategy};
use crate::fidelity::{self, Diagnostic};
use crate::types::{EditChange, Input};
use crate::{CompileDiagnostic, CompileError, CompileQuestion, CompileStatus, lexicon::Reading};
use nika_kernel::ai::provider::{Message, ProviderInferDyn, Role};
use serde_json::{Value, json};

#[cfg(test)]
use super::QuestionType;

mod answer;
mod bounds;
mod decode;
mod revision;
pub(super) use answer::{Answer, Question};
pub(super) use decode::decode;

fn schema() -> Value {
    serde_json::from_str(include_str!("../../assets/native_answer_schema.json"))
        .unwrap_or_else(|_| json!({"type": "object"}))
}

/// Whether a cold outcome calls for the native strategy: a question that hands the human a
/// machine's problem (a rewrite, a jq expression, a glob), or a dead end (no candidate and
/// nothing to answer). A cold outcome waiting on business values is not a failure.
pub(super) fn escalates(out: &CompileOutcome, reading: &Reading) -> bool {
    // A refusal is the floor (a bypassed approval, a literal-only policy): no door reopens it.
    // A contradiction the reader found between the request's own words for one effect is a
    // reading, not a refusal: the seat reads the request whole, and what it realizes of that
    // effect is stated to the review.
    if out.status == CompileStatus::Refused {
        return words_contradict_only(out, reading);
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

/// Whether an outcome's only obstacle is the reader's contradiction for an effect (asked and
/// banned by the request's own words): no hard refusal, no approval-bypass floor.
pub(super) fn words_contradict_only(out: &CompileOutcome, reading: &Reading) -> bool {
    reading
        .plan
        .effects
        .iter()
        .any(|e| e.policy == crate::plan::EffectPolicy::Conflict)
        && !reading
            .plan
            .unknowns
            .iter()
            .any(|u| u.contains("approval-bypass"))
        && !out
            .diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::Refused)
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

/// Whether the reader's words settle an effect: not undecided, not both asked and banned.
fn settled(effect: &crate::plan::Effect) -> bool {
    !matches!(
        effect.policy,
        crate::plan::EffectPolicy::Undecided | crate::plan::EffectPolicy::Conflict
    )
}

fn effect_fact(effect: &crate::plan::Effect) -> Value {
    json!({"verb": effect.verb.word(), "target": effect.target, "policy": effect.policy.word()})
}

/// The effects the reader could not settle from the words, stated to the seat as open
/// readings, never as facts: the seat realizes one as the request means it, and the review
/// states what it realized. A reading grants nothing: permits, Check and consent decide.
fn unsettled(reading: &Reading) -> Option<Value> {
    let open: Vec<Value> = reading
        .plan
        .effects
        .iter()
        .filter(|e| !settled(e))
        .map(effect_fact)
        .collect();
    (!open.is_empty()).then(|| {
        json!({
            "effects": open,
            "how": "The reader's words do not settle these (an effect left undecided, or both asked and banned). They are hypotheses, not facts: read the request whole and realize what it means, or leave the effect out; what you realize is stated to the human's review.",
        })
    })
}

/// The facts the reader holds the candidate to, stated to the seat as data.
fn floor(intent: &str, reading: &Reading) -> Value {
    let plan = &reading.plan;
    json!({
        "sources": crate::hot::stated_sources(intent),
        "destinations": crate::hot::stated_destinations(intent),
        "effects": plan.effects.iter().filter(|e| settled(e)).map(effect_fact).collect::<Vec<_>>(),
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
        _ => "other",
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
    /// Exact malformed response repeated without progress; not an accepted candidate.
    last_decode: Option<String>,
    /// The last candidate the laws refused: the record keeps its text, so a refusal can be
    /// read (a hash names nothing).
    pub(super) refused: Option<String>,
    pub(super) route: Vec<String>,
    /// The values the human answered: a candidate may carry them without inventing them.
    pub(super) allowed: Vec<String>,
    /// The whole names the human typed for the read's source question: the laws read a stated
    /// source through them exactly as the assembler's emission does.
    pub(super) clarified: Vec<String>,
    /// The repair principles a knowledge snapshot wires to diagnostic codes, for the repair
    /// message.
    pub(super) repairs: std::collections::BTreeMap<String, Vec<String>>,
    /// The observed world of the stated files, when the host read one: a column, field, key
    /// or value name is stated there, never asked.
    pub(super) observed: Option<Value>,
    /// The base and the change words of a revision in words: a stated path the change leaves
    /// behind is waived only as `revision` proves it or the human disposes of it.
    pub(super) revision: Option<(String, String)>,
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
            last_decode: None,
            refused: None,
            route,
            allowed,
            clarified: fidelity::clarified_sources(&request.answers),
            observed: request.knowledge.clone(),
            revision: revision::of(request),
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
    /// The seat repeated a refused candidate or malformed answer: stop honestly.
    Stalled,
    /// A terminal call/answer failure, or no authorized syntax repair remains.
    Stop,
}

/// Author natively: the opening call, then at most `policy.repairs` repair calls. Completed
/// syntax errors may receive feedback; every decoded candidate faces the same laws. The
/// receipt and diagnostics retain every call, including any previous cold round.
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
        format!(
            "{}\n\n# Native answer transport\nPrefer candidate_lines: one physical YAML line per array element, preserving indentation and blank lines; an empty final element represents a final newline. Set candidate to the empty string in line mode. Otherwise send the complete candidate string with real newlines and an empty candidate_lines array. Populate exactly one representation. Never encode structural line breaks as HTML or symbols. Keep questions, gaps and notes as specified by the card.",
            system_message(&references, &callables)
        ),
        opening.to_string(),
        route,
        allowed,
        request,
    );
    let mut accepted: Option<Answer> = None;
    let mut round_policy = policy.clone();
    round_policy.max_tokens = policy.initial_max_tokens.unwrap_or(policy.max_tokens);
    for round in 0..=policy.repairs.min(5) {
        match exchange(
            round,
            &mut talk,
            intent,
            reading,
            &mut round_policy,
            policy.max_tokens,
            provider,
            &mut out,
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
                Some(nika_compile_fidelity::candidate::delta(&base, &revised))
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
    let mut opening = json!({
        "request": intent,
        "facts_the_compiler_holds_you_to": floor(intent, reading),
        "observed_world": request.knowledge,
        "answers_already_given": request.answers,
        "output_caps": knowledge::output_caps(&request.answers),
        "base_candidate": revision.map(|(source, _)| source),
        "change": revision.map(|(_, words)| words),
    });
    if let Some(open) = unsettled(reading) {
        opening["readings_the_words_leave_open"] = open;
    }
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
    policy: &mut AuthoringPolicy,
    hard_max_tokens: u32,
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
    if bounds::expand(&response, round, policy, hard_max_tokens, talk) {
        return Round::Repair;
    }
    let (answer, text) = match decode::native(&response, round, policy, talk, out) {
        Ok(answer) => answer,
        Err(decision) => return decision,
    };
    let waived = revision::waivable(intent, talk.revision.as_ref(), &answer.gaps);
    let diagnostics = judge(
        intent,
        reading,
        &answer.candidate,
        &answer.questions,
        &talk.allowed,
        &waived,
        &talk.clarified,
        talk.observed.as_ref(),
    );
    talk.rounds.push(json!({
        "round": round,
        "candidate_sha256": knowledge::sha256(&answer.candidate),
        "candidate": answer.candidate,
        "questions": answer.questions.iter().map(|q| q.key.clone()).collect::<Vec<_>>(),
        "gaps": answer.gaps.clone(),
        "notes": answer.notes.clone(),
        "diagnostics": diagnostics.iter().map(|d| json!({"kind": d.kind, "message": d.message})).collect::<Vec<_>>(),
    }));
    if diagnostics.is_empty() {
        return Round::Accepted(answer);
    }
    let repeated = talk.refused.as_ref() == Some(&answer.candidate)
        && talk.last.as_ref() == Some(&diagnostics);
    talk.refused = Some(answer.candidate.clone());
    if repeated {
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

/// The end of the conversation: an accepted candidate settles; an exhausted budget is an
/// honest technical finding, never a replacement request or a substitute workflow.
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
            // The seat's reading of what the words left open is stated to the review, never
            // refused and never taken for authority.
            if let Some(doc) = crate::edit::literal_projection(&answer.candidate) {
                for effect in fidelity::unsettled_performed(&reading.plan, &doc) {
                    super::super::finding(
                        out,
                        DiagnosticKind::Applied,
                        "reading",
                        format!(
                            "The request's words do not settle {effect}; the candidate performs it as the seat reads the whole request. Review it before approving."
                        ),
                    );
                }
            }
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
            out.questions.retain(|question| {
                !matches!(
                    question.key.as_str(),
                    "intent.clarification" | "const.rule_expression" | "const.source_glob"
                )
            });
            // Technical failures preserve the request and the recorded cause. They do
            // not invent a business decision or ask the human to replace their intent.
        }
        None => {
            route.push("native: exhausted".to_owned());
            super::record_route(out, &route);
            super::super::finding(
                out,
                DiagnosticKind::Unknown,
                "authoring_native",
                "No candidate passed the checks within the repair budget; the original request, candidates and diagnostics are retained. No workflow was emitted. Inspect the last diagnostic before another bounded attempt.",
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
    let judged = rounds
        .iter()
        .filter(|r| r.get("candidate_sha256").is_some() || r.get("sketch_sha256").is_some())
        .count();
    format!(
        "The authoring conversation recorded {} round(s), including {judged} candidate or sketch judgment(s): {}.",
        rounds.len(),
        parts.join("; ")
    )
}

pub(super) fn system_message(references: &[Reference], callables: &[Reference]) -> String {
    let mut text = format!("{}\n\n{}", knowledge::card(), knowledge::CONVENTIONS);
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
        "COMPILER DIAGNOSTICS on your candidate. Return the complete corrected JSON answer (candidate or candidate_lines, questions, gaps, notes); fix every item, change nothing the request did not ask.\n",
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

/// Phrases that leave a column or a field of a file open (FR · EN, folded, whole words): which
/// one is the human's business choice, never the seat's.
const OPEN_COLUMNS: &[&str] = &[
    "une colonne",
    "une des colonnes",
    "l une des colonnes",
    "un champ",
    "un des champs",
    "a column",
    "one column",
    "one of the columns",
    "a field",
    "one of the fields",
];

/// The slug endings that ask for a column or a field by its name.
const COLUMN_SLUGS: &[&str] = &["_column", "_col", "_field", "_header"];

/// A text's folded words, one space apart and one around: whole-word phrases match inside.
fn spaced(text: &str) -> String {
    let folded = crate::hot::fold(text);
    let words: Vec<&str> = folded
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    format!(" {} ", words.join(" "))
}

/// The observed alternatives of a column the request leaves open (« Additionne une colonne de
/// ventes.csv »): the request speaks of « une colonne », names none of the columns of the one
/// file the host observed with several, and the question asks for a column by name. Each
/// offer is one of that file's observed columns, verbatim, never a name the seat's label
/// proposes. None otherwise: the observed world then states the name, and it is not asked.
fn open_column(intent: &str, slug: &str, observed: Option<&Value>) -> Option<Vec<Value>> {
    if !COLUMN_SLUGS.iter().any(|end| slug.ends_with(end)) {
        return None;
    }
    let words = spaced(intent);
    if !OPEN_COLUMNS
        .iter()
        .any(|phrase| words.contains(&format!(" {phrase} ")))
    {
        return None;
    }
    let observed = observed?;
    let rows = observed
        .get("observed")
        .and_then(Value::as_array)
        .or_else(|| observed.as_array())?;
    let mut files = rows.iter().filter_map(|row| {
        let path = row.get("path")?.as_str()?;
        let columns: Vec<&str> = row
            .get("columns")?
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .collect();
        (columns.len() >= 2).then_some((path, columns))
    });
    let (path, columns) = files.next()?;
    // Two observed files: which one the column belongs to is not the question's to say.
    if files.next().is_some() || columns.iter().any(|column| words.contains(&spaced(column))) {
        return None;
    }
    Some(
        columns
            .iter()
            .map(|column| json!({"key": column, "label": format!("column `{column}` of {path}")}))
            .collect(),
    )
}

/// A candidate's questions, admitted: `const.<snake_slug>` keys only, each declared under
/// `const:` in the candidate as a placeholder, at most eight; never a machine's construct;
/// never a name the observed world states. A column the request leaves open is admitted with
/// its observed alternatives (`open_column`), the only answers the compiler takes.
fn admitted_questions(
    intent: &str,
    candidate: &str,
    questions: &[Question],
    observed: Option<&Value>,
) -> Result<Vec<(Question, Vec<Value>)>, Diagnostic> {
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
        // `const.open_value`, `const.region_column` beside the observed header and value set) —
        // unless the request leaves the column open among the observed ones (DIALOG-03,
        // 2026-09-24: « Additionne une colonne de ventes.csv » over `montant, autre`).
        let mut options = Vec::new();
        if let Some(world) = world.as_deref()
            && STRUCTURE_SLUGS.lines().any(|s| slug.ends_with(s))
        {
            let Some(offered) = open_column(intent, slug, observed) else {
                return Err(Diagnostic {
                    kind: "question",
                    message: format!(
                        "the question `{}` asks the human for a column, field, key or value name; the observed world states them — {world} — write those exact names and values into the candidate, never a question",
                        question.key
                    ),
                });
            };
            options = offered;
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
        admitted.push((question.clone(), options));
    }
    Ok(admitted)
}

/// The judge: the strict parser, the pure Check, then the fidelity laws against the original
/// request and the reader's floor. Every refusal is one structured diagnostic. A `waived` path
/// is one a revision's seat names in its gaps and the change never names (`revision`): the
/// path law leaves it to the gap, which the settlement proves or leaves to the human.
pub(super) fn judge(
    intent: &str,
    reading: &Reading,
    candidate: &str,
    questions: &[Question],
    allowed: &[String],
    waived: &[String],
    clarified: &[String],
    observed: Option<&Value>,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let Some(doc) = admit(candidate, questions, &mut out) else {
        return out;
    };
    fidelity::laws(
        intent,
        &reading.plan,
        &doc,
        allowed,
        waived,
        clarified,
        &mut out,
    );
    if let Err(diagnostic) = admitted_questions(intent, candidate, questions, observed) {
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
                "the candidate arrived as ONE line ({} characters, no newline): a `.nika` is a multi-line YAML document — write a real newline (`\\n` inside the JSON string) after every field and every task, never a space in its place; alternatively send candidate_lines, one physical line per array element, and leave candidate empty",
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

/// One admitted question as the record keeps it: a column the request leaves open is a closed
/// choice among its observed alternatives, the only answers the replay bakes.
fn recorded(question: &Question, options: &[Value]) -> Value {
    let mut record = json!({"key": question.key, "label": question.label, "answer_type": question.answer_type, "why": question.why});
    if !options.is_empty() {
        record["answer_type"] = json!("choice");
        record["options"] = json!(options);
    }
    record
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
    let admitted = admitted_questions(intent, candidate, questions, request.knowledge.as_ref())
        .unwrap_or_default();
    let gaps: Vec<&str> = gaps
        .iter()
        .map(|g| g.trim())
        .filter(|g| !g.is_empty())
        .take(revision::KEPT_GAPS)
        .collect();
    // A revision's gap the change proves superseded is recorded and applied, never asked; every
    // other gap stays for the human.
    let (gaps, superseded) = revision::settle(intent, request, candidate, &gaps);
    let mut record = json!({
        "strategy": Strategy::Native.word(),
        "intent_sha256": super::intent_sha256(intent),
        "source": candidate,
        "questions": admitted.iter().map(|(q, options)| recorded(q, options)).collect::<Vec<_>>(),
        "gaps": gaps,
        "trigger": trigger,
    });
    if !superseded.is_empty() {
        record["superseded"] = json!(
            superseded
                .iter()
                .map(|s| json!({"path": s.path, "by": s.by, "gap": s.gap}))
                .collect::<Vec<_>>()
        );
    }
    for proof in &superseded {
        super::super::finding(
            out,
            DiagnosticKind::Applied,
            "revision",
            format!(
                "`{}` is replaced by `{}`, as the change states: the revised candidate is the base with that path replaced and nothing else changed.",
                proof.path, proof.by
            ),
        );
    }
    // What the candidate BUILDS, in the plan record's own vocabulary (operations · effects ·
    // obligations · bindings), so provenance reads the native strategy as it reads the
    // others; `operations_from` says the reading is of the bytes, not of the intent.
    if let Some(doc) = crate::edit::literal_projection(candidate) {
        let built = nika_compile_fidelity::candidate::plan_of_document(&doc).to_json();
        for key in ["operations", "effects", "obligations", "bindings"] {
            record[key] = built[key].clone();
        }
        record["operations_from"] = json!("candidate");
    }
    out.provenance.plan = Some(record.clone());
    crate::native_apply(&record, request, out);
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
        assert!(!escalates(&refused, &crate::lexicon::read("")));
        let mut failed = outcome();
        crate::finding(
            &mut failed,
            DiagnosticKind::Unknown,
            "authoring_provider",
            "timed out",
        );
        assert!(!escalates(&failed, &crate::lexicon::read("")));
        let mut machine = outcome();
        crate::question(
            &mut machine,
            "const.rule_expression",
            "which jq?",
            QuestionType::Text,
        );
        assert!(escalates(&machine, &crate::lexicon::read("")));
        let mut business = outcome();
        crate::question(
            &mut business,
            "const.send_endpoint",
            "where?",
            QuestionType::Text,
        );
        assert!(!escalates(&business, &crate::lexicon::read("")));
        assert!(escalates(&outcome(), &crate::lexicon::read("")));
    }

    #[test]
    fn a_refusal_resting_only_on_the_words_contradicting_one_effect_escalates() {
        let contradiction = crate::lexicon::read(
            "Lis ./note.txt et envoie-la à https://hooks.example.test/in; ne l'envoie jamais à https://hooks.example.test/in.",
        );
        assert!(
            contradiction
                .plan
                .effects
                .iter()
                .any(|e| e.policy == crate::plan::EffectPolicy::Conflict)
        );
        let mut refused = outcome();
        refused.status = CompileStatus::Refused;
        assert!(escalates(&refused, &contradiction));
        // A hard refusal beside it, or the approval-bypass floor, is never reopened.
        let mut hard = refused.clone();
        crate::finding(
            &mut hard,
            DiagnosticKind::Refused,
            "intent",
            "literal-only policy",
        );
        assert!(!escalates(&hard, &contradiction));
        let mut bypass = contradiction.clone();
        bypass
            .plan
            .unknowns
            .push("approval-bypass wording".to_owned());
        assert!(!escalates(&refused, &bypass));
        // Without a contradiction, a refusal stays the floor.
        assert!(!escalates(
            &refused,
            &crate::lexicon::read("Lis ./note.txt.")
        ));
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
        assert!(admitted_questions("", candidate, &[asked("const.source_glob")], None).is_err());
        assert!(
            admitted_questions("", candidate, &[asked("const.filter_expression")], None).is_err()
        );
        assert!(admitted_questions("", candidate, &[asked("const.missing")], None).is_err());
        assert!(
            admitted_questions("", candidate, &[asked("const.notify_channel")], None).is_err(),
            "a channel is never a question (the live run of 2026-09-22 asked two for one send)"
        );
        assert!(admitted_questions("", candidate, &[asked("model")], None).is_err());
        // A name the observed world states is never a question; without an observed world the
        // same slug is a declared placeholder like any other.
        let shaped = "nika: x\nconst:\n  status_field: \"\"\n  region_column: \"\"\ntasks: {}\n";
        let world = json!({"observed": [{"path": "./tickets.json", "kind": "json", "columns": ["id", "status", "topic"], "values": {"status": ["closed", "open"]}}]});
        let Err(refused) =
            admitted_questions("", shaped, &[asked("const.status_field")], Some(&world))
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
        assert!(
            admitted_questions("", shaped, &[asked("const.region_column")], Some(&world)).is_err()
        );
        assert_eq!(
            admitted_questions("", shaped, &[asked("const.status_field")], None).map(|q| q.len()),
            Ok(1)
        );
        assert_eq!(
            admitted_questions("", candidate, &[asked("const.source_folder")], None)
                .map(|q| q.len()),
            Ok(1)
        );
    }

    #[test]
    fn a_column_the_request_leaves_open_is_a_choice_among_the_observed_ones_and_nothing_else_is() {
        let asked = |key: &str| Question {
            key: key.to_owned(),
            label: "Quelle colonne additionner ?".to_owned(),
            answer_type: "text".to_owned(),
            why: String::new(),
        };
        let shaped = "nika: x\nconst:\n  sum_column: \"\"\n  status_value: \"\"\ntasks: {}\n";
        let world = json!({"observed": [{"path": "ventes.csv", "kind": "csv", "delimiter": ",", "columns": ["montant", "autre"]}]});
        let open = "Additionne une colonne de ventes.csv dans total.txt.";
        let Ok(admitted) =
            admitted_questions(open, shaped, &[asked("const.sum_column")], Some(&world))
        else {
            panic!("an open column over two observed ones is the human's choice");
        };
        assert_eq!(
            admitted[0].1,
            vec![
                json!({"key": "montant", "label": "column `montant` of ventes.csv"}),
                json!({"key": "autre", "label": "column `autre` of ventes.csv"}),
            ]
        );
        let record = recorded(&admitted[0].0, &admitted[0].1);
        assert_eq!(record["answer_type"], "choice", "{record}");
        assert_eq!(record["options"][0]["key"], "montant", "{record}");
        assert!(
            admitted_questions(
                "Sum a column of ventes.csv into total.txt",
                shaped,
                &[asked("const.sum_column")],
                Some(&world)
            )
            .is_ok()
        );
        // A named column is stated; a value is not a column; two observed files or one column
        // leave nothing the question could choose among; « les montants » names the column.
        let two = json!({"observed": [
            {"path": "ventes.csv", "columns": ["montant", "autre"]},
            {"path": "achats.csv", "columns": ["prix", "quantite"]}]});
        let one = json!({"observed": [{"path": "ventes.csv", "columns": ["montant"]}]});
        for (intent, key, observed) in [
            (
                "Additionne la colonne montant de ventes.csv dans total.txt.",
                "const.sum_column",
                &world,
            ),
            (open, "const.status_value", &world),
            (open, "const.sum_column", &two),
            (open, "const.sum_column", &one),
            (
                "Additionne les montants de ventes.csv dans total.txt.",
                "const.sum_column",
                &world,
            ),
        ] {
            assert!(
                admitted_questions(intent, shaped, &[asked(key)], Some(observed)).is_err(),
                "{intent} · {key}"
            );
        }
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
}
