// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The sketch door (W2 of the V6 plan, « bounded semantic actions over typed holes »): the
//! seat proposes a SKETCH — the tasks, their verbs and tools, the stated paths and hosts each
//! one reaches, the data edges, the gate, the loop — and nothing else; the structural laws
//! and the fidelity laws judge it before a word of prompt exists. Then the seat fills ONLY the
//! typed holes the sketch leaves (a prompt, a jq program, a schema, a builtin's argument, an
//! argv) and the compiler emits the `.nika` itself — the permits derived from what the tasks
//! reach, the bindings from the edges, the gate as a `when:`, every open value a `const`
//! placeholder — and judges the emitted document as it judges any candidate. A model never
//! writes authority; a refusal names a task or a hole, and the repair fills that hole again,
//! never the whole file.

use super::native::{
    self, Answer, Prelude, Question, Shaped, Talk, cold, conclude, decode, floor_refuses, judge,
    prelude, repair_message, system_message,
};
use super::proposal::Composition;
use super::{
    AuthoringPolicy, CompileOutcome, CompileRequest, DiagnosticKind, NativeMode, Strategy,
};
use crate::fidelity::Diagnostic;
use crate::sketch::{self as ir, Sketch, judge_sketch, reach_laws, validated};
use crate::{CompileError, lexicon::Reading};
use nika_kernel::ai::provider::{Message, ProviderInferDyn, Role};
use serde_json::{Value, json};

mod evidence;
/// The whole-source recovery an explicit policy allows after the structured rounds.
mod recover;
#[cfg(test)]
mod recover_tests;
/// The semantic revision of a base its record binds, beside the door it reuses.
pub(super) mod revise;
use super::rehearsal::Rehearsals;
use evidence::Evidence;

/// The seat's first answer: the sketch, its business questions, the clauses no task realizes.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SketchAnswer {
    #[serde(default, deserialize_with = "super::nullable_default")]
    name: String,
    #[serde(default, deserialize_with = "super::nullable_default")]
    tasks: Vec<Value>,
    /// The workflow's named results as the seat stated them; read by `Sketch::from_json`.
    #[serde(default)]
    outputs: Option<Value>,
    #[serde(default, deserialize_with = "super::nullable_default")]
    questions: Vec<Question>,
    #[serde(default, deserialize_with = "super::nullable_default")]
    gaps: Vec<String>,
    #[serde(default, deserialize_with = "super::nullable_default")]
    notes: String,
}

/// The seat's second answer: the fills of the holes, and nothing else.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Filling {
    #[serde(default, deserialize_with = "super::nullable_default")]
    fills: Vec<Value>,
    #[serde(default, deserialize_with = "super::nullable_default")]
    notes: String,
}

impl Shaped for SketchAnswer {
    const KEYS: &'static [&'static str] = &["tasks"];
}

impl Shaped for Filling {
    const KEYS: &'static [&'static str] = &["fills"];
}

/// The sketch instruction in the embedded pack, read beside the card.
const SKETCH: &str = include_str!("../../assets/native_authoring_sketch.md");

/// The two answer schemas, embedded beside the crate (the `tests` prove they parse).
const SKETCH_SCHEMA: &str = include_str!("../../assets/sketch_schema.json");
const FILLS_SCHEMA: &str = include_str!("../../assets/fills_schema.json");

fn schema(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|_| json!({"type": "object"}))
}

/// One call under the schema: the seat's text decoded as `T`, or the end of the talk (a
/// failed call is journaled here; the decoder journals the rest).
async fn call<T: Shaped, P: ProviderInferDyn>(
    talk: &mut Talk,
    round: u32,
    role: &'static str,
    schema: Value,
    policy: &AuthoringPolicy,
    provider: &P,
    out: &mut CompileOutcome,
) -> Option<(T, String)> {
    let before = super::receipt::journaled(out);
    let response =
        super::call_with_schema(policy, provider, role, talk.messages.clone(), schema, out).await;
    super::receipt::stamp_references(out, before, &talk.presented);
    let Some(response) = response else {
        talk.rounds
            .push(json!({"round": round, "phase": role, "call": "failed"}));
        return None;
    };
    let what = role.split('-').next().unwrap_or(role);
    decode::<T>(&response, what, round, talk, out)
}

/// The holes as the seat reads them: one line each, the kind and the purpose.
fn holes_message(sketch: &Sketch) -> String {
    let mut text = String::from(
        "The sketch is accepted as written. Fill exactly these holes, keyed `task.field`, and nothing else:\n",
    );
    for hole in ir::holes(sketch) {
        use std::fmt::Write as _;
        let optional = if hole.required { "" } else { " (optional)" };
        let _ = writeln!(
            text,
            "- {}.{} · {}{optional} · {}",
            hole.task, hole.field, hole.kind, hole.why
        );
    }
    text.push_str(
        "Fill each hole once, with its value; fill every hole not marked optional; an `args` object never names an argument the sketch states for its task (the path or glob it reaches, its edge input or bound content, its channel); a program bound to several edges reads them as one input object keyed by their names (`.<name>` for each edge); a content template reads every edge its task is bound to; a builtin's file argument is one of the paths its own task states in reads or writes. Answer one JSON object {\"fills\": [{\"task\", \"field\", \"value\"}], \"notes\"}.",
    );
    text
}

/// The task keys a sketch parse reads; any other key of a task is ignored by it.
const TASK_KEYS: &[&str] = &[
    "id",
    "verb",
    "tool",
    "reads",
    "writes",
    "hosts",
    "after",
    "with",
    "gated_by",
    "for_each",
    "purpose",
    "max_turns",
    "tools",
    "fail_fast",
];

/// How many keys of `object` are not in `known`: data the parse never read.
fn ignored_keys(object: &Value, known: &[&str]) -> usize {
    object.as_object().map_or(0, |map| {
        map.keys().filter(|k| !known.contains(&k.as_str())).count()
    })
}

/// The controls this task's verb carries that the sketch left to the historical emission (an
/// agent's `max_turns` 4 and empty `tools`, a loop's `fail_fast` false): projected so the record
/// never presents them as requested.
fn defaulted(task: &ir::SketchTask) -> Vec<&'static str> {
    let mut out = Vec::new();
    if task.verb == ir::Verb::Agent {
        if task.max_turns.is_none() {
            out.push("max_turns");
        }
        if task.tools.is_none() {
            out.push("tools");
        }
    }
    if task.for_each.is_some() && task.fail_fast.is_none() {
        out.push("fail_fast");
    }
    out
}

/// An accepted sketch as the compiler consumed it: the closed form of every field the parse read,
/// the digest of the seat's raw graph, and how many keys the parse ignored (never their text).
fn consumed_sketch(sketch: &Sketch, raw: &Value) -> Value {
    let tasks = raw["tasks"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let ignored: usize = tasks
        .iter()
        .map(|task| {
            let edges = task["with"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default();
            ignored_keys(task, TASK_KEYS)
                + edges
                    .iter()
                    .map(|e| ignored_keys(e, &["name", "from"]))
                    .sum::<usize>()
        })
        .sum();
    json!({
        "name": sketch.name,
        "tasks": sketch.tasks.iter().map(|t| json!({
            "id": t.id, "verb": t.verb.word(), "tool": t.tool, "reads": t.reads,
            "writes": t.writes, "hosts": t.hosts, "after": t.after,
            "with": t.with.iter().map(|e| json!({"name": e.name, "from": e.from})).collect::<Vec<_>>(),
            "gated_by": t.gated_by, "for_each": t.for_each, "purpose": t.purpose,
            "max_turns": t.max_turns, "tools": t.tools, "fail_fast": t.fail_fast,
            "defaulted": defaulted(t),
        })).collect::<Vec<_>>(),
        "outputs": sketch.outputs.as_ref().map(|outputs| outputs
            .iter()
            .map(|o| json!({"name": o.name, "from": o.from}))
            .collect::<Vec<_>>()),
        "sha256": super::knowledge::sha256(&raw.to_string()),
        "ignored_keys": ignored,
    })
}

/// Every fill the seat sent, in order: the first fill of a declared hole in the closed form the
/// document reads (`task`, `field`, `value`) with the raw fill's digest and how many keys it
/// carried beside them; any other — no such hole, a repeated fill of one, or every fill of an
/// answer no document was emitted from — by digest, shape and reason. Whether the document read
/// a fill is the emitted candidate's to show (an invoke task applies every `args.*` fill).
fn proposed_fills(sketch: &Sketch, fills: &[Value], used: bool) -> Vec<Value> {
    let holes = ir::holes(sketch);
    let mut seen: Vec<(&str, &str)> = Vec::new();
    let keys = ["task", "field", "value"];
    fills
        .iter()
        .map(|fill| {
            let task = fill["task"].as_str().unwrap_or_default();
            let field = fill["field"].as_str().unwrap_or_default();
            let declared = holes.iter().any(|h| h.task == task && h.field == field);
            let reason = if !used {
                "the document was not emitted from these fills"
            } else if !declared {
                "not a declared hole"
            } else if seen.contains(&(task, field)) {
                "a repeated fill of one declared hole"
            } else {
                seen.push((task, field));
                return json!({
                    "task": task,
                    "field": field,
                    "value": fill.get("value").cloned().unwrap_or(Value::Null),
                    "sha256": super::knowledge::sha256(&fill.to_string()),
                    "ignored_keys": ignored_keys(fill, &keys),
                });
            };
            super::receipt::withheld(&fill.to_string(), &keys, reason)
        })
        .collect()
}

/// The journal entry of a fill round refused before emission: no candidate and no candidate
/// digest, the fills by digest only, the named diagnostics.
fn refused_round(
    round: u32,
    sketch: &Sketch,
    filling: &Filling,
    diagnostics: &[Diagnostic],
) -> Value {
    json!({
        "round": round,
        "phase": "fill",
        "fills": filling.fills.len(),
        "proposed_fills": proposed_fills(sketch, &filling.fills, false),
        "notes": super::receipt::withheld(&filling.notes, &[], "fill notes"),
        "diagnostics": diagnostics_record(diagnostics),
    })
}

/// The tail of a fill repair: the sketch stays as accepted, only the named holes are filled again.
const FILL_AGAIN: &str = "\nFill the named holes again (the sketch stays as accepted); answer the same {\"fills\", \"notes\"} object.";

fn diagnostics_record(diagnostics: &[Diagnostic]) -> Vec<Value> {
    diagnostics
        .iter()
        .map(|d| json!({"kind": d.kind, "message": d.message}))
        .collect()
}

/// Whether a repair round opens: the assistant's text and the repair message join the talk
/// and the diagnostics are remembered; a repeated refusal is no progress and ends the talk.
fn repair(talk: &mut Talk, text: String, diagnostics: Vec<Diagnostic>, tail: &str) -> bool {
    if talk.last.as_ref() == Some(&diagnostics) {
        talk.route.push("native: no progress".to_owned());
        return false;
    }
    talk.messages.push(Message::text(Role::Assistant, text));
    talk.messages.push(Message::text(
        Role::User,
        format!("{}{tail}", repair_message(&diagnostics, &talk.repairs)),
    ));
    talk.last = Some(diagnostics);
    true
}

/// One sketch round as the journal keeps it. The graph the laws accepted, as parsed; a refused
/// one by digest, shape and reason. The seat's free text stays out of the record: its notes by
/// digest on every round, and a refused round's question keys and gaps too (an accepted round's
/// passed the laws).
fn sketch_round(
    round: u32,
    record: &Value,
    parsed: Option<&Sketch>,
    answer: &SketchAnswer,
    diagnostics: &[Diagnostic],
) -> Value {
    let proposed = match parsed {
        Some(sketch) if diagnostics.is_empty() => consumed_sketch(sketch, record),
        _ => super::receipt::withheld(&record.to_string(), &["name", "tasks"], "refused sketch"),
    };
    let refused = parsed.is_none() || !diagnostics.is_empty();
    let listed = |values: Value, what: &str| {
        if refused {
            super::receipt::withheld(&values.to_string(), &[], what)
        } else {
            values
        }
    };
    let keys: Vec<&str> = answer.questions.iter().map(|q| q.key.as_str()).collect();
    json!({
        "round": round,
        "phase": "sketch",
        "sketch_sha256": super::knowledge::sha256(&record.to_string()),
        "proposed_sketch": proposed,
        "tasks": parsed.map_or(0, |s| s.tasks.len()),
        "questions": listed(json!(keys), "refused sketch question keys"),
        "gaps": listed(json!(answer.gaps), "refused sketch gaps"),
        "notes": super::receipt::withheld(&answer.notes, &[], "sketch notes"),
        "diagnostics": diagnostics_record(diagnostics),
    })
}

/// Phase 1 · the sketch, judged structurally, repaired within the budget, from round `first` of
/// the door's one round count (a graph reopened by evidence continues it, never resets it).
/// Returns the rounds spent and the accepted sketch with the answer that carried it.
async fn propose<P: ProviderInferDyn>(
    talk: &mut Talk,
    intent: &str,
    reading: &Reading,
    policy: &AuthoringPolicy,
    (provider, first): (&P, u32),
    out: &mut CompileOutcome,
) -> (u32, Option<(Sketch, SketchAnswer)>) {
    let budget = policy.repairs.min(5);
    let mut round = first;
    while round <= budget {
        let role = if round == 0 {
            "sketch"
        } else {
            "sketch-repair"
        };
        let Some((answer, text)) = call::<SketchAnswer, P>(
            talk,
            round,
            role,
            schema(SKETCH_SCHEMA),
            policy,
            provider,
            out,
        )
        .await
        else {
            return (round + 1, None);
        };
        let record = graph(&answer);
        let (diagnostics, parsed) = match Sketch::from_json(&record) {
            Ok(parsed) => {
                let mut judged = judge_sketch(
                    intent,
                    reading,
                    &parsed,
                    (&talk.allowed, &talk.clarified),
                    talk.observed.as_ref(),
                );
                // A question the request or the observed world already settles is refused before
                // the graph is fixed, so this repair can withdraw it (no fill can).
                let questions = &answer.questions;
                if judged.is_empty()
                    && let Err(refused) = super::native::admitted_questions(
                        intent,
                        "",
                        questions,
                        talk.observed.as_ref(),
                    )
                {
                    judged.push(refused);
                }
                (judged, Some(parsed))
            }
            Err(message) => (
                vec![Diagnostic {
                    kind: "sketch",
                    message,
                }],
                None,
            ),
        };
        talk.rounds.push(sketch_round(
            round,
            &record,
            parsed.as_ref(),
            &answer,
            &diagnostics,
        ));
        round += 1;
        if let Some(parsed) = parsed
            && diagnostics.is_empty()
        {
            talk.messages.push(Message::text(Role::Assistant, text));
            return (round, Some((parsed, answer)));
        }
        if !repair(talk, text, diagnostics, "") {
            break;
        }
    }
    (round, None)
}

/// The graph a sketch answer states, as `Sketch::from_json` reads it: its name, its tasks and,
/// when stated, its named results (omitted and explicit stay distinct).
fn graph(answer: &SketchAnswer) -> Value {
    let mut record = json!({"name": answer.name, "tasks": answer.tasks});
    if let Some(outputs) = &answer.outputs {
        record["outputs"] = outputs.clone();
    }
    record
}

/// Phase 2 · the holes, filled and judged as the whole document they state, repaired within
/// the budget. Returns the accepted answer, its candidate the emitted document, and the fills
/// exactly as accepted (the semantic record replays them).
async fn fill<P: ProviderInferDyn>(
    talk: &mut Talk,
    intent: &str,
    reading: &Reading,
    policy: &AuthoringPolicy,
    provider: &P,
    out: &mut CompileOutcome,
    accepted: &(Sketch, SketchAnswer),
    first_round: u32,
) -> Option<(Answer, Vec<Value>)> {
    let (sketch, answer) = accepted;
    let last_round = policy.repairs.min(5) + 1;
    talk.last = None;
    talk.messages
        .push(Message::text(Role::User, holes_message(sketch)));
    let mut round = first_round;
    while round <= last_round {
        let role = if round == first_round {
            "fill"
        } else {
            "fill-repair"
        };
        let (filling, text) = call::<Filling, P>(
            talk,
            round,
            role,
            schema(FILLS_SCHEMA),
            policy,
            provider,
            out,
        )
        .await?;
        // Every fill is judged against the accepted sketch before a document exists: a refusal
        // is a named diagnostic for the same bounded repair, never a candidate or its digest.
        let (fills, doc) = match validated(sketch, &filling.fills) {
            Ok(valid) => valid,
            Err(diagnostics) => {
                talk.rounds
                    .push(refused_round(round, sketch, &filling, &diagnostics));
                round += 1;
                if !repair(talk, text, diagnostics, FILL_AGAIN) {
                    break;
                }
                continue;
            }
        };
        let candidate = match serde_yaml_bw::to_string(&doc) {
            Ok(candidate) => candidate,
            Err(error) => {
                talk.rounds.push(json!({
                    "round": round,
                    "phase": "fill",
                    "answer": format!("the document is not representable: {error}"),
                    "proposed_fills": proposed_fills(sketch, &filling.fills, false),
                }));
                return None;
            }
        };
        // The sketch door authors creations only (a revision in words goes to the native
        // door): every stated path is opened, none waived.
        let diagnostics = judge(
            intent,
            reading,
            &candidate,
            &answer.questions,
            &talk.allowed,
            &[],
            &talk.clarified,
            talk.observed.as_ref(),
        );
        talk.rounds.push(json!({
            "round": round,
            "phase": "fill",
            "candidate_sha256": super::knowledge::sha256(&candidate),
            "fills": fills.len(),
            "proposed_fills": proposed_fills(sketch, &filling.fills, true),
            "notes": super::receipt::withheld(&filling.notes, &[], "fill notes"),
            "diagnostics": diagnostics_record(&diagnostics),
        }));
        round += 1;
        if diagnostics.is_empty() {
            let answer = Answer {
                candidate,
                questions: answer.questions.clone(),
                gaps: answer.gaps.clone(),
            };
            talk.messages.push(Message::text(Role::Assistant, text));
            return Some((answer, filling.fills));
        }
        talk.refused = Some(candidate);
        if !repair(talk, text, diagnostics, FILL_AGAIN) {
            break;
        }
    }
    None
}

/// The route step of a COLD round whose plan could not keep the request's branches apart.
pub(super) const COMPOSITION: &str = "native: sketch for branches the plan cannot keep apart";
/// The route step of an escalating COLD round that ended without a candidate, or handed the human
/// a machine's problem.
pub(super) const ESCALATED: &str = "native: sketch after the plan";

/// Why a COLD round hands its request to the sketch door.
pub(super) enum Escalation {
    /// Branches the private plan cannot keep apart.
    Composition(Composition),
    /// An escalating policy's plan round ended without a candidate, or with a machine's problem.
    Plan,
}

/// The sketch door after a COLD round paid for its plan: the same request, answers, reading
/// floor and receipt. Within the bound the policy already grants: the sketch door takes one
/// request more than the native door that bound once counted (the sketch, then its fills), so
/// its repair allowance is one less (none left is a budget finding, no request). A policy with
/// no native door names why and assembles nothing. Never source generation.
#[allow(clippy::too_many_arguments)] // the sketch door's own inputs, plus why it opens
pub(super) async fn compose<P: ProviderInferDyn>(
    intent: &str,
    reading: &Reading,
    policy: &AuthoringPolicy,
    seats: (&P, &mut Rehearsals<'_>),
    request: &CompileRequest,
    mut route: Vec<String>,
    mut out: CompileOutcome,
    why: &Escalation,
) -> Result<CompileOutcome, CompileError> {
    let (what, step) = match why {
        Escalation::Composition(composition) => {
            let kinds: Vec<String> = (composition.occurrences.iter())
                .map(|(op, _)| format!("`{}`", op.word()))
                .collect();
            let what = format!(
                "The request composes {} independent branches ({}) that the private plan cannot keep apart",
                kinds.len(),
                kinds.join(", ")
            );
            (what, COMPOSITION)
        }
        Escalation::Plan => (
            "The private plan ended without a candidate the request can stand on".to_owned(),
            ESCALATED,
        ),
    };
    let refusal = if policy.native == NativeMode::Off {
        Some(format!(
            "{what}, and this authoring policy permits no sketch door (native: off). No candidate was assembled."
        ))
    } else if policy.repairs.min(5).checked_sub(1).is_none() {
        Some(format!(
            "{what}; the sketch door needs one request more than the single candidate this policy bounds, and its repair allowance (0) leaves none. No request was sent and no candidate was assembled."
        ))
    } else {
        None
    };
    if let Some(message) = refusal {
        crate::finding(
            &mut out,
            DiagnosticKind::RequiresHuman,
            "authoring_plan",
            message,
        );
        let needs = match why {
            Escalation::Composition(_) => "cold: composition needs the sketch door",
            Escalation::Plan => "cold: escalation needs the sketch door",
        };
        route.push(needs.to_owned());
        super::record_route(&mut out, &route);
        return Ok(out);
    }
    route.push(step.to_owned());
    let bounded = policy
        .clone()
        .with_repairs(policy.repairs.min(5).saturating_sub(1));
    author(intent, reading, &bounded, seats, request, route, out).await
}

/// The question fields a native settlement reads (`apply_native`, `bake`, `ask`).
const QUESTION_KEYS: [&str; 5] = ["key", "label", "answer_type", "why", "options"];

/// The semantic record of the accepted pair (slice C), closed: the basis read before the
/// proposal, the graph and fills exactly as decoded, the settlement fields a native settlement
/// reads (questions through an allowlist, gaps, trigger), the pre-answer assembly (`source`,
/// labelled so: an observation no replay reads) and the final candidate bound to its answers. No
/// `strategy` word, judgment or journal. `None` when the settlement lacks a field it needs.
fn semantic_record(
    basis: Value,
    stated: &Value,
    fills: &[Value],
    settled: &Value,
    out: &CompileOutcome,
    world: Option<&Value>,
) -> Option<Value> {
    let assembled = settled["source"].as_str()?;
    let questions: Vec<Value> = (settled["questions"].as_array()?.iter())
        .map(|q| {
            let kept = (q.as_object().into_iter().flatten())
                .filter(|(key, _)| QUESTION_KEYS.contains(&key.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()));
            Value::Object(kept.collect())
        })
        .collect();
    let sha = super::knowledge::sha256;
    // The trigger in the request's own words (the reader's phrase is normalized): its occurrence
    // in the effective request; none found, no record.
    let trigger = match settled["trigger"].as_str() {
        Some(phrase) => json!(occurrence(basis["effective"].as_str()?, phrase)?),
        None => Value::Null,
    };
    let mut record = json!({
        "semantic_record": 1,
        "lowering": 1,
        "intent_sha256": settled["intent_sha256"].as_str()?,
        "final": {"answers": basis["answers"], "candidate_sha256": out.candidate.as_deref().map(sha)},
        "sketch": stated,
        "fills": fills,
        "settlement": {"questions": questions, "gaps": settled["gaps"].as_array()?,
                       "trigger": trigger},
        "assembly_sha256": sha(assembled),
        "source": assembled,
        "source_is": "pre_answer_assembly",
    });
    record["basis"] = json!({});
    record["basis"]["read"] = basis;
    record["basis"]["world"] = nika_compile_fidelity::observed::basis::keep(world);
    Some(record)
}

/// The words of `text` a lowercase `phrase` was read from: the first occurrence, cut at `text`'s
/// own character boundaries, whose lowercase is the phrase.
fn occurrence<'a>(text: &'a str, phrase: &str) -> Option<&'a str> {
    let most = phrase.chars().count();
    text.char_indices().find_map(|(start, _)| {
        let rest = &text[start..];
        (rest.char_indices().take(most))
            .map(|(at, c)| &rest[..at + c.len_utf8()])
            .find(|words| words.to_lowercase() == phrase)
    })
}

/// At the compile's entry, once the door returned: the caller basis read before any money was
/// blanked joins the semantic record the door just produced (a replayed record keeps its own),
/// which is kept only when the core's own replay reproduces its final binding with no call;
/// otherwise no record, a finding and INCOMPLETE: an answer round compiles afresh, no retry.
pub(super) fn bind_caller(caller: Value, raw: &CompileRequest, out: &mut CompileOutcome) {
    let fresh =
        |r: &&mut Value| r.get("semantic_record").is_some() && r["basis"].get("caller").is_none();
    let Some(record) = out.provenance.plan.as_mut().filter(fresh) else {
        return;
    };
    record["basis"]["caller"] = caller;
    let mut replay = raw.clone();
    replay.plan = Some(record.clone());
    let kept = nika_compile::compile_judged(&replay, &[]).is_ok_and(|replayed| {
        (replayed.provenance.plan.as_ref()).is_some_and(|p| p["final"] == record["final"])
    });
    if !kept {
        withhold_record(out);
    }
}

/// No replay record for an accepted sketch that cannot be recorded or does not replay: the round
/// is INCOMPLETE with a static finding, never READY without its record and never retried (the
/// judge of the whole request is asked only of a READY conclusion); an answer round compiles the
/// request again.
fn withhold_record(out: &mut CompileOutcome) {
    out.provenance.plan = None;
    out.status = crate::CompileStatus::Incomplete;
    super::super::finding(
        out,
        DiagnosticKind::Unknown,
        "recorded_plan",
        "The accepted sketch cannot be kept as a replay record under this request, so nothing is READY: compile the request again.",
    );
}

/// The laws a replayed semantic record must still hold before any judgment is asked of it: the
/// reach of each task (`nika_cap`, which the core cannot read) and every fill judged again as
/// emitted. The core's replay already ran the structural and fill laws and the emission.
pub(super) fn replay_laws(record: &Value) -> Vec<String> {
    let Ok(sketch) = Sketch::from_json(&record["sketch"]) else {
        return vec!["its graph does not decode".to_owned()];
    };
    let mut refused: Vec<Diagnostic> = reach_laws(&sketch);
    if refused.is_empty() {
        let fills = record["fills"].as_array().map_or(&[][..], Vec::as_slice);
        refused = validated(&sketch, fills).err().unwrap_or_default();
    }
    refused.into_iter().map(|d| d.message).collect()
}

/// The sketch door: the two-phase conversation, judged at each phase, settled by the native
/// door's own conclusion (questions asked, answers baked, the record replayable with zero
/// calls) and recorded beside it.
pub(super) async fn author<P: ProviderInferDyn>(
    intent: &str,
    reading: &Reading,
    policy: &AuthoringPolicy,
    (provider, rehearsals): (&P, &mut Rehearsals<'_>),
    request: &CompileRequest,
    mut route: Vec<String>,
    mut out: CompileOutcome,
) -> Result<CompileOutcome, CompileError> {
    // The cold round as the door opened on it: every attempt concludes against the same one.
    let opened = out.clone();
    let _ = cold(&mut out);
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
    let mut system = system_message(&references, &callables);
    system.push_str("\n\n");
    system.push_str(SKETCH);
    // The request's own basis, read before any proposal: no answer of the seat reaches it.
    let basis = nika_compile::surface::semantic::request_basis(intent, request);
    let mut talk = Talk::open(
        system,
        format!("{opening}\n\nAnswer with the SKETCH (call 1), not a file."),
        route,
        allowed,
        request,
    );
    talk.presented = json!(sent);
    // One round count spans the sketch, its fills and every reopening (the last round leaves the
    // fill its turn).
    let last = policy.repairs.min(5);
    let mut first = 0;
    loop {
        let (spent, sketched) = propose(
            &mut talk,
            intent,
            reading,
            policy,
            (provider, first),
            &mut out,
        )
        .await;
        let accepted = match &sketched {
            Some(pair) => {
                fill(
                    &mut talk, intent, reading, policy, provider, &mut out, pair, spent,
                )
                .await
            }
            None => None,
        };
        let mut done = out.clone();
        let settled = (sketched.as_ref(), accepted.as_ref());
        settle(
            (intent, reading, request),
            &talk,
            (&sent, revision),
            basis.clone(),
            settled,
            (&opened, &mut done),
        );
        // Every exhaustion (no accepted pair, spent evidence, a withdrawal) meets one recovery.
        let door = (intent, reading, policy, request);
        let (next, seats) = (next_round(&talk), (provider, &mut *rehearsals));
        let ended = match accepted {
            None => (done, Vec::new()),
            Some(_) => match examine(door, seats, &mut talk, done, (next, last), last + 2).await {
                Step::Done(answer) => return Ok(answer),
                Step::Withdrawn(done, defects) => (done, defects),
                Step::Reopen(mut done, defects) => {
                    if next <= last && reopen(&mut talk, defects.clone(), SKETCH_AGAIN) {
                        // The judge's calls, when it was asked, belong to the door's one journal.
                        (out.provenance.authoring).clone_from(&done.provenance.authoring);
                        first = next;
                        continue;
                    }
                    evidence::refuse(&mut done, EVIDENCE_SPENT.to_owned());
                    (done, defects)
                }
            },
        };
        let seats = (provider, &mut *rehearsals);
        let recovered = recover::after(door, seats, &mut talk, (&opened, &sent), ended);
        return Ok(Box::pin(recovered).await);
    }
}

const EVIDENCE_SPENT: &str =
    "The evidence refused this candidate and the repair allowance is spent: nothing is READY.";

/// Where one settled candidate leads: the door's answer, a reopening from these defects, or a
/// withdrawal past the last round from the defects the whole-request judgment demonstrated.
enum Step {
    Done(CompileOutcome),
    Reopen(CompileOutcome, Vec<Diagnostic>),
    Withdrawn(CompileOutcome, Vec<Diagnostic>),
}

/// One settled candidate faces its evidence (journalled in the attempt's record and the talk),
/// then, when nothing there refuses it, the whole-request judgment. Round `next` is the one a
/// reopening would spend; past `last` a refusal withdraws the candidate. The room admits
/// `attempts` rehearsals in all: one per candidate the door's rounds can produce.
async fn examine<P: ProviderInferDyn>(
    (intent, reading, policy, request): (&str, &Reading, &AuthoringPolicy, &CompileRequest),
    (provider, rehearsals): (&P, &mut Rehearsals<'_>),
    talk: &mut Talk,
    mut done: CompileOutcome,
    (next, last): (u32, u32),
    attempts: u32,
) -> Step {
    let (found, record) = evidence::examined(rehearsals, request, intent, &done, attempts).await;
    if !record.is_null() {
        // The evidence of the candidate the last round produced, in the journal this attempt
        // returns and in the talk a reopened graph continues.
        let entry = json!({"round": next.saturating_sub(1), "evidence": record});
        let rounds = (done.provenance.decision.as_mut())
            .and_then(|decision| decision["native"]["rounds"].as_array_mut());
        if let Some(rounds) = rounds {
            rounds.push(entry.clone());
        }
        talk.rounds.push(entry);
    }
    match found {
        Evidence::Stop(reason) => {
            evidence::refuse(&mut done, reason);
            Step::Done(done)
        }
        Evidence::Defect(defect) => Step::Reopen(done, vec![defect]),
        Evidence::Unoffered | Evidence::Open | Evidence::Holds | Evidence::Unknown => {
            let verdict =
                super::verify::native_verdict(intent, reading, policy, provider, request, done);
            match verdict.await {
                Ok(judged) => Step::Done(judged),
                Err(judged) => {
                    let (judged, verdict) = *judged;
                    let defects = judge_defects(&verdict.defects);
                    if verdict.defects.is_empty() {
                        Step::Done(super::verify::withdrawn(judged, &verdict))
                    } else if next > last {
                        Step::Withdrawn(super::verify::withdrawn(judged, &verdict), defects)
                    } else {
                        Step::Reopen(judged, defects)
                    }
                }
            }
        }
    }
}

/// The round after the journal's last: where a reopened graph continues the door's count.
fn next_round(talk: &Talk) -> u32 {
    (talk.rounds.iter())
        .filter_map(|round| round["round"].as_u64())
        .max()
        .and_then(|round| u32::try_from(round + 1).ok())
        .unwrap_or(0)
}

/// The judge's defects as the repair reads them: the parts of the request the bytes miss.
fn judge_defects(defects: &[String]) -> Vec<Diagnostic> {
    (defects.iter())
        .map(|defect| Diagnostic {
            kind: "semantic_verification",
            message: format!(
                "the judge compared the whole request with the candidate's bytes: it does not carry `{defect}`"
            ),
        })
        .collect()
}

/// Reopen from evidence: the diagnostics go back with the `tail` instruction (the whole sketch
/// again, its holes filled after it; or a recovery's whole source); a repeat is no progress.
fn reopen(talk: &mut Talk, diagnostics: Vec<Diagnostic>, tail: &str) -> bool {
    if talk.last.as_ref() == Some(&diagnostics) {
        talk.route.push("native: no progress".to_owned());
        return false;
    }
    talk.messages.push(Message::text(
        Role::User,
        format!("{}{tail}", repair_message(&diagnostics, &talk.repairs)),
    ));
    talk.last = Some(diagnostics);
    true
}

/// What a reopened graph is asked.
const SKETCH_AGAIN: &str = "\n\nThe workflow these fills made was refused by the evidence above. Answer the whole SKETCH again (call 1 schema), corrected so that it does what the request asks; its holes are filled again after it.";

/// The accepted sketch with the answer that carried it.
type Sketched = (Sketch, SketchAnswer);
/// The accepted fill's answer with the fills exactly as accepted.
type Filled = (Answer, Vec<Value>);

/// One attempt's conclusion on `done`: the native record, the sketch summary, the settlement
/// against the cold round the door opened on, and the semantic record of the accepted pair.
fn settle(
    (intent, reading, request): (&str, &Reading, &CompileRequest),
    talk: &Talk,
    (sent, revision): (&[Value], Option<(&str, &str)>),
    basis: Value,
    (sketched, accepted): (Option<&Sketched>, Option<&Filled>),
    (opened, done): (&CompileOutcome, &mut CompileOutcome),
) {
    let cold = cold(&mut opened.clone());
    let answer = accepted.map(|(answer, _)| answer);
    native::record(done, request, &cold, talk, sent, answer, revision);
    if let Some(decision) = done.provenance.decision.as_mut() {
        decision["native"]["sketch"] = json!({
            "accepted": sketched.is_some(),
            "tasks": sketched.map_or(0, |(s, _)| s.tasks.len()),
            "holes": sketched.map_or(0, |(s, _)| ir::holes(s).len()),
        });
    }
    conclude(intent, reading, request, answer, talk, cold, done);
    // The settlement's record becomes the semantic record: the accepted pair from its producer,
    // the basis read before the proposal; the source it emitted stays an observation.
    if let (Some((_, fills)), Some((_, stated))) = (accepted, sketched)
        && let Some(settled) = done.provenance.plan.take()
    {
        done.provenance.plan = semantic_record(
            basis,
            &graph(stated),
            fills,
            &settled,
            done,
            request.knowledge.as_ref(),
        );
        if done.provenance.plan.is_none() {
            withhold_record(done);
        }
    }
    done.provenance.strategy = Some(Strategy::Native);
}

#[cfg(test)]
mod tests {
    use super::{FILLS_SCHEMA, SKETCH_SCHEMA, schema, semantic_record, withhold_record};
    use serde_json::json;

    #[test]
    fn a_settlement_missing_a_field_builds_no_record_and_the_round_is_withheld() {
        let mut out = nika_compile::surface::initial();
        out.status = crate::CompileStatus::Ready;
        let settled = json!({"intent_sha256": "x", "questions": [], "gaps": [], "trigger": null});
        let basis = json!({"answers": {}});
        let record = semantic_record(
            basis,
            &json!({"name": "x", "tasks": []}),
            &[],
            &settled,
            &out,
            None,
        );
        assert!(record.is_none(), "no source: no record built from defaults");
        out.provenance.plan = Some(json!({"strategy": "native"}));
        withhold_record(&mut out);
        assert_eq!(out.status, crate::CompileStatus::Incomplete);
        assert!(out.provenance.plan.is_none());
        assert!(out.diagnostics.iter().any(|d| d.target == "recorded_plan"));
    }

    #[test]
    fn the_two_answer_schemas_parse_and_close_their_objects() {
        for text in [SKETCH_SCHEMA, FILLS_SCHEMA] {
            let value = schema(text);
            assert_eq!(value["additionalProperties"], false, "{value}");
            assert!(value["required"].is_array(), "{value}");
        }
    }

    /// Run a jq program over `input` with the core, std and json definitions (the language the
    /// builtin runs, without the run-start clock no program here reads): its one output.
    fn jq(program: &str, input: &serde_json::Value) -> Result<serde_json::Value, String> {
        use jaq_core::load::{Arena, File, Loader};
        use jaq_core::{Compiler, Ctx, Vars, data::JustLut};
        use jaq_json::{Val, read};
        let defs = jaq_core::defs()
            .chain(jaq_std::defs())
            .chain(jaq_json::defs());
        let funs = jaq_core::funs()
            .chain(jaq_std::funs())
            .chain(jaq_json::funs());
        let arena = Arena::default();
        let modules = Loader::new(defs)
            .load(
                &arena,
                File {
                    code: program,
                    path: (),
                },
            )
            .map_err(|_| "parse".to_owned())?;
        let filter = Compiler::default()
            .with_funs(funs)
            .compile(modules)
            .map_err(|_| "compile".to_owned())?;
        let bytes = serde_json::to_vec(input).map_err(|e| e.to_string())?;
        let val = read::parse_single(&bytes).map_err(|e| e.to_string())?;
        let ctx = Ctx::<JustLut<Val>>::new(&filter.lut, Vars::new([]));
        let mut outputs = filter.id.run((ctx, val));
        let first = outputs.next().ok_or("no output")?;
        let first = first.map_err(|_| "the program fails on its input".to_owned())?;
        serde_json::from_str(&first.to_string()).map_err(|e| e.to_string())
    }

    /// A program bound to two tables reads both (the input object of its edge names, each its
    /// exact binding), in any edge order, and computes the totals the tables state; the former
    /// first-edge input computes another answer. One edge keeps its whole-value input; none,
    /// no input.
    #[test]
    fn a_program_bound_to_two_tables_reads_both_and_computes_the_stated_totals() {
        use nika_compile_fidelity::sketch::{Sketch, document, fills_from_json};
        let orders = json!([
            {"customer_id": "a", "amount_cents": "100"},
            {"customer_id": "b", "amount_cents": "50"},
            {"customer_id": "a", "amount_cents": "25"}
        ]);
        let customers = json!([
            {"customer_id": "a", "region": "north"},
            {"customer_id": "b", "region": "south"}
        ]);
        let tables = json!({"read_orders": orders, "read_customers": customers});
        // The independent oracle: each region's total over the joined rows.
        let mut expected = std::collections::BTreeMap::new();
        for row in orders.as_array().unwrap() {
            let region = customers
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["customer_id"] == row["customer_id"])
                .unwrap()["region"]
                .clone();
            let cents: i64 = row["amount_cents"].as_str().unwrap().parse().unwrap();
            *expected
                .entry(region.as_str().unwrap().to_owned())
                .or_insert(0) += cents;
        }
        let program = "(.customers | map({key: .customer_id, value: .region}) | from_entries) as $region | .orders | group_by($region[.customer_id]) | map({region: $region[.[0].customer_id], total_cents: (map(.amount_cents | tonumber) | add)})";
        let fills = fills_from_json(&json!({"fills": [
            {"task": "totals", "field": "expression", "value": program}
        ]}))
        .unwrap();
        let emitted = |edges: &[(&str, &str)]| {
            let with: Vec<_> = edges
                .iter()
                .map(|(n, f)| json!({"name": n, "from": f}))
                .collect();
            let sketch = Sketch::from_json(&json!({"name": "regional-totals", "tasks": [
                {"id": "read_orders", "verb": "invoke", "tool": "nika:read", "reads": ["./orders.json"], "purpose": "orders"},
                {"id": "read_customers", "verb": "invoke", "tool": "nika:read", "reads": ["./customers.json"], "purpose": "customers"},
                {"id": "totals", "verb": "invoke", "tool": "nika:jq", "with": with, "purpose": "join and sum"}
            ]}))
            .unwrap();
            document(&sketch, &fills)["tasks"]["totals"].clone()
        };
        // The value a template reads at run: its edge's source table.
        let resolve = |task: &serde_json::Value, template: &str| {
            let name = template
                .trim_start_matches("${{ with.")
                .trim_end_matches(" }}");
            let source = task["with"][name].as_str().unwrap();
            let id = source
                .trim_start_matches("${{ tasks.")
                .trim_end_matches(".output }}");
            tables[id].clone()
        };
        let totals = |task: &serde_json::Value| -> std::collections::BTreeMap<String, i64> {
            let input = &task["invoke"]["args"]["input"];
            let bound: serde_json::Map<String, serde_json::Value> =
                (input.as_object().unwrap().iter())
                    .map(|(k, v)| (k.clone(), resolve(task, v.as_str().unwrap())))
                    .collect();
            let out = jq(program, &serde_json::Value::Object(bound)).unwrap();
            (out.as_array().unwrap().iter())
                .map(|r| {
                    (
                        r["region"].as_str().unwrap().to_owned(),
                        r["total_cents"].as_i64().unwrap(),
                    )
                })
                .collect()
        };
        let two = [("orders", "read_orders"), ("customers", "read_customers")];
        for edges in [two, [two[1], two[0]]] {
            let task = emitted(&edges);
            assert_eq!(
                task["invoke"]["args"]["input"],
                json!({"orders": "${{ with.orders }}", "customers": "${{ with.customers }}"}),
                "{task:#}"
            );
            assert_eq!(task["with"]["orders"], "${{ tasks.read_orders.output }}");
            assert_eq!(
                task["with"]["customers"],
                "${{ tasks.read_customers.output }}"
            );
            assert_eq!(totals(&task), expected, "{task:#}");
        }
        // The former first-edge input: the orders alone, and the program cannot find its regions.
        assert!(
            jq(program, &orders).is_err(),
            "the orders alone carry no regions"
        );
        let one = emitted(&[two[0]]);
        assert_eq!(
            one["invoke"]["args"]["input"], "${{ with.orders }}",
            "{one:#}"
        );
        let none = emitted(&[]);
        assert!(none["invoke"]["args"].get("input").is_none(), "{none:#}");
    }

    /// The trigger's words are cut from the request at its own character boundaries: a
    /// character whose lowercase is longer (`İ`) before the phrase, a mixed case, the phrase
    /// absent.
    #[test]
    fn the_trigger_occurrence_is_the_requests_own_words() {
        let text = "İstanbul : Chaque Lundi matin, copie ./notes.md";
        assert_eq!(
            super::occurrence(text, "chaque lundi matin"),
            Some("Chaque Lundi matin")
        );
        assert_eq!(
            super::occurrence("İİ chaque jour", "chaque jour"),
            Some("chaque jour")
        );
        assert_eq!(super::occurrence(text, "chaque vendredi"), None);
        assert_eq!(super::occurrence("", "chaque jour"), None);
    }
}
