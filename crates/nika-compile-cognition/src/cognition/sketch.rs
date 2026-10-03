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
use super::{AuthoringPolicy, CompileOutcome, CompileRequest, Strategy};
use crate::fidelity::{self, Diagnostic};
use crate::sketch::{self as ir, Fill, Sketch};
use crate::{CompileError, lexicon::Reading};
use nika_kernel::ai::provider::{Message, ProviderInferDyn, Role};
use serde_json::{Value, json};

/// The seat's first answer: the sketch, its business questions, the clauses no task realizes.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SketchAnswer {
    #[serde(default, deserialize_with = "super::nullable_default")]
    name: String,
    #[serde(default, deserialize_with = "super::nullable_default")]
    tasks: Vec<Value>,
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

/// The structural judge of a sketch: the sketch's own laws, then the fidelity laws over the
/// document it states before any hole is filled (the stated paths, the approval, the
/// prohibitions, no invented literal).
fn judge_sketch(
    intent: &str,
    reading: &Reading,
    sketch: &Sketch,
    allowed: &[String],
    clarified: &[String],
) -> Vec<Diagnostic> {
    let mut out: Vec<Diagnostic> = ir::structural_laws(sketch, intent, allowed)
        .into_iter()
        .map(|message| Diagnostic {
            kind: "sketch",
            message,
        })
        .collect();
    if out.is_empty() {
        out.extend(reach_laws(sketch));
    }
    if out.is_empty() {
        let doc = ir::document(sketch, &[]);
        fidelity::laws(
            intent,
            &reading.plan,
            &doc,
            allowed,
            &[],
            clarified,
            &mut out,
        );
    }
    out.dedup();
    out
}

/// The reach laws a graph must hold before any hole is filled, since no fill can repair them:
/// each side a builtin always reaches (`nika_cap::required_fs_directions`, e.g. a chart's write,
/// an edit's read and write) is stated in the task's `reads`/`writes`, and every path the sketch
/// itself derives for the task (the partial projection's arguments) is bound to that reach on
/// each side its effect touches (`nika_cap::unbound_fs_args`). Optional slots and inline data are
/// never required here; a filled argument is judged again at emission.
fn reach_laws(sketch: &Sketch) -> Vec<Diagnostic> {
    let doc = ir::document(sketch, &[]);
    let mut out = Vec::new();
    for task in sketch.tasks.iter().filter(|t| t.verb == ir::Verb::Invoke) {
        let Some(tool) = task.tool.as_deref() else {
            continue;
        };
        let mut push = |message: String| {
            out.push(Diagnostic {
                kind: "sketch",
                message,
            });
        };
        if let Some((read, write)) = nika_cap::required_fs_directions(tool) {
            for (needed, stated, side) in [
                (read, &task.reads, "reads"),
                (write, &task.writes, "writes"),
            ] {
                if needed && stated.is_empty() {
                    push(format!(
                        "`{}` invokes `{tool}`, which always {side} a file: state that path in its `{side}`",
                        task.id
                    ));
                }
            }
        }
        let derived = doc["tasks"][task.id.as_str()]["invoke"].get("args");
        for finding in nika_cap::unbound_fs_args(tool, derived, &task.reads, &task.writes) {
            push(format!("`{}`: {finding}", task.id));
        }
    }
    out
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
        "Fill each hole once, with its value; fill every hole not marked optional; an `args` object never names an argument the sketch states for its task (the path or glob it reaches, its edge input or bound content, its channel); a content template reads every edge its task is bound to; a builtin's file argument is one of the paths its own task states in reads or writes. Answer one JSON object {\"fills\": [{\"task\", \"field\", \"value\"}], \"notes\"}.",
    );
    text
}

/// The task keys a sketch parse reads; any other key of a task is ignored by it.
const TASK_KEYS: &[&str] = &[
    "id", "verb", "tool", "reads", "writes", "hosts", "after", "with", "gated_by", "for_each",
    "purpose",
];

/// How many keys of `object` are not in `known`: data the parse never read.
fn ignored_keys(object: &Value, known: &[&str]) -> usize {
    object.as_object().map_or(0, |map| {
        map.keys().filter(|k| !known.contains(&k.as_str())).count()
    })
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
        })).collect::<Vec<_>>(),
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
        "notes": filling.notes,
        "diagnostics": diagnostics_record(diagnostics),
    })
}

/// The tail of a fill repair: the sketch stays as accepted, only the named holes are filled again.
const FILL_AGAIN: &str = "\nFill the named holes again (the sketch stays as accepted); answer the same {\"fills\", \"notes\"} object.";

/// The fills as the compiler consumes them and the complete document they state, or every
/// refusal before any document exists: the fill laws of the accepted sketch, then each invoke's
/// builtin contract (`nika_cap`) over the arguments it would carry. A refusal never repeats a
/// value a fill proposed.
fn validated(sketch: &Sketch, raw: &[Value]) -> Result<(Vec<Fill>, Value), Vec<Diagnostic>> {
    let refused = |messages: Vec<String>| -> Vec<Diagnostic> {
        messages
            .into_iter()
            .map(|message| Diagnostic {
                kind: "fill",
                message,
            })
            .collect()
    };
    let fills = ir::fills_from_json(&json!({"fills": raw})).map_err(|m| refused(vec![m]))?;
    let doc = ir::complete_document(sketch, &fills).map_err(refused)?;
    let mut findings = Vec::new();
    for (id, node) in doc["tasks"].as_object().into_iter().flatten() {
        let Some(tool) = node["invoke"]["tool"].as_str() else {
            continue;
        };
        let args = node["invoke"].get("args");
        for finding in nika_cap::builtin_shape_findings(tool, args) {
            findings.push(format!(
                "task `{id}` (`{tool}`): {}",
                redacted(&finding, &fills)
            ));
        }
        // Each filesystem argument the builtin contract names is bound to THIS task's stated
        // reach, never to the union of permits (`nika_cap::unbound_fs_args`, the effect owner).
        if let Some(task) = sketch.tasks.iter().find(|t| &t.id == id) {
            for finding in nika_cap::unbound_fs_args(tool, args, &task.reads, &task.writes) {
                findings.push(format!("task `{id}`: {finding}"));
            }
        }
    }
    if findings.is_empty() {
        Ok((fills, doc))
    } else {
        Err(refused(findings))
    }
}

/// A contract finding with every string a fill proposed (four characters or more) replaced, so
/// the record names the broken rule without repeating the refused value.
fn redacted(message: &str, fills: &[Fill]) -> String {
    fn leaves(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::String(text) if text.chars().count() >= 4 => out.push(text.clone()),
            Value::Array(items) => items.iter().for_each(|v| leaves(v, out)),
            Value::Object(map) => map.values().for_each(|v| leaves(v, out)),
            _ => {}
        }
    }
    let mut texts = Vec::new();
    for fill in fills {
        leaves(&fill.value, &mut texts);
    }
    texts.sort_by_key(|t| std::cmp::Reverse(t.len()));
    let mut out = message.to_owned();
    for text in texts {
        out = out.replace(&text, "<proposed value>");
    }
    out
}

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

/// Phase 1 · the sketch, judged structurally, repaired within the budget. Returns the rounds
/// spent and the accepted sketch with the answer that carried it.
async fn propose<P: ProviderInferDyn>(
    talk: &mut Talk,
    intent: &str,
    reading: &Reading,
    policy: &AuthoringPolicy,
    provider: &P,
    out: &mut CompileOutcome,
) -> (u32, Option<(Sketch, SketchAnswer)>) {
    let budget = policy.repairs.min(5);
    let mut round = 0;
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
        let record = json!({"name": answer.name, "tasks": answer.tasks});
        let (diagnostics, parsed) = match Sketch::from_json(&record) {
            Ok(parsed) => (
                judge_sketch(intent, reading, &parsed, &talk.allowed, &talk.clarified),
                Some(parsed),
            ),
            Err(message) => (
                vec![Diagnostic {
                    kind: "sketch",
                    message,
                }],
                None,
            ),
        };
        // The graph the laws accepted, as parsed; a refused one by digest, shape and reason.
        let proposed = match &parsed {
            Some(sketch) if diagnostics.is_empty() => consumed_sketch(sketch, &record),
            _ => {
                super::receipt::withheld(&record.to_string(), &["name", "tasks"], "refused sketch")
            }
        };
        talk.rounds.push(json!({
            "round": round,
            "phase": "sketch",
            "sketch_sha256": super::knowledge::sha256(&record.to_string()),
            "proposed_sketch": proposed,
            "tasks": parsed.as_ref().map_or(0, |s| s.tasks.len()),
            "questions": answer.questions.iter().map(|q| q.key.clone()).collect::<Vec<_>>(),
            "gaps": answer.gaps.clone(),
            "notes": answer.notes.clone(),
            "diagnostics": diagnostics_record(&diagnostics),
        }));
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

/// Phase 2 · the holes, filled and judged as the whole document they state, repaired within
/// the budget. Returns the accepted answer, its candidate the emitted document.
async fn fill<P: ProviderInferDyn>(
    talk: &mut Talk,
    intent: &str,
    reading: &Reading,
    policy: &AuthoringPolicy,
    provider: &P,
    out: &mut CompileOutcome,
    accepted: &(Sketch, SketchAnswer),
    first_round: u32,
) -> Option<Answer> {
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
            "notes": filling.notes.clone(),
            "diagnostics": diagnostics_record(&diagnostics),
        }));
        round += 1;
        if diagnostics.is_empty() {
            return Some(Answer {
                candidate,
                questions: answer.questions.clone(),
                gaps: answer.gaps.clone(),
                notes: answer.notes.clone(),
                dual: None,
            });
        }
        talk.refused = Some(candidate);
        if !repair(talk, text, diagnostics, FILL_AGAIN) {
            break;
        }
    }
    None
}

/// The sketch door: the two-phase conversation, judged at each phase, settled by the native
/// door's own conclusion (questions asked, answers baked, the record replayable with zero
/// calls) and recorded beside it.
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
    let mut system = system_message(&references, &callables);
    system.push_str("\n\n");
    system.push_str(SKETCH);
    let mut talk = Talk::open(
        system,
        format!("{opening}\n\nAnswer with the SKETCH (call 1), not a file."),
        route,
        allowed,
        request,
    );
    talk.presented = json!(sent);
    let (spent, sketched) = propose(&mut talk, intent, reading, policy, provider, &mut out).await;
    let mut accepted = None;
    if let Some(pair) = &sketched {
        accepted = fill(
            &mut talk, intent, reading, policy, provider, &mut out, pair, spent,
        )
        .await;
    }
    native::record(
        &mut out,
        request,
        &cold,
        &talk,
        &sent,
        accepted.as_ref(),
        revision,
    );
    if let Some(decision) = out.provenance.decision.as_mut() {
        decision["native"]["sketch"] = json!({
            "accepted": sketched.is_some(),
            "tasks": sketched.as_ref().map_or(0, |(s, _)| s.tasks.len()),
            "holes": sketched.as_ref().map_or(0, |(s, _)| ir::holes(s).len()),
        });
    }
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
    if accepted.is_some() {
        out = super::verify::judged_native(intent, reading, policy, provider, request, out).await;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{FILLS_SCHEMA, SKETCH_SCHEMA, schema};

    #[test]
    fn the_two_answer_schemas_parse_and_close_their_objects() {
        for text in [SKETCH_SCHEMA, FILLS_SCHEMA] {
            let value = schema(text);
            assert_eq!(value["additionalProperties"], false, "{value}");
            assert!(value["required"].is_array(), "{value}");
        }
    }
}
