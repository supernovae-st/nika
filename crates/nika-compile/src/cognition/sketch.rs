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
    self, Answer, Prelude, Question, Talk, cold, conclude, decode, floor_refuses, judge, prelude,
    repair_message, system_message,
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

/// The sketch instruction in the embedded pack, read beside the card.
const SKETCH_PATH: &str = "stdlib/authoring-sketch-v0.1.md";

/// The two answer schemas, embedded beside the crate (the `tests` prove they parse).
const SKETCH_SCHEMA: &str = include_str!("../../assets/sketch_schema.json");
const FILLS_SCHEMA: &str = include_str!("../../assets/fills_schema.json");

fn schema(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|_| json!({"type": "object"}))
}

/// One call under the schema: the seat's text decoded as `T`, or the end of the talk (a
/// failed call is journaled here; the decoder journals the rest).
async fn call<T: serde::de::DeserializeOwned, P: ProviderInferDyn>(
    talk: &mut Talk,
    round: u32,
    role: &'static str,
    schema: Value,
    policy: &AuthoringPolicy,
    provider: &P,
    out: &mut CompileOutcome,
) -> Option<(T, String)> {
    let Some(response) =
        super::call_with_schema(policy, provider, role, talk.messages.clone(), schema, out).await
    else {
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
) -> Vec<Diagnostic> {
    let mut out: Vec<Diagnostic> = ir::structural_laws(sketch, intent, allowed)
        .into_iter()
        .map(|message| Diagnostic {
            kind: "sketch",
            message,
        })
        .collect();
    if out.is_empty() {
        let doc = ir::document(sketch, &[]);
        fidelity::laws(intent, &reading.plan, &doc, allowed, &[], &mut out);
    }
    out.dedup();
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
        "Answer one JSON object {\"fills\": [{\"task\", \"field\", \"value\"}], \"notes\"}.",
    );
    text
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
                judge_sketch(intent, reading, &parsed, &talk.allowed),
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
        talk.rounds.push(json!({
            "round": round,
            "phase": "sketch",
            "sketch_sha256": super::knowledge::sha256(&record.to_string()),
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
        let fills: Vec<Fill> = match ir::fills_from_json(&json!({"fills": filling.fills})) {
            Ok(fills) => fills,
            Err(message) => {
                talk.rounds
                    .push(json!({"round": round, "phase": "fill", "answer": message}));
                return None;
            }
        };
        let candidate = match serde_yaml_bw::to_string(&ir::document(sketch, &fills)) {
            Ok(candidate) => candidate,
            Err(error) => {
                talk.rounds.push(json!({
                    "round": round,
                    "phase": "fill",
                    "answer": format!("the document is not representable: {error}"),
                }));
                return None;
            }
        };
        let diagnostics = judge(
            intent,
            reading,
            &candidate,
            &answer.questions,
            &talk.allowed,
            talk.observed.as_ref(),
        );
        talk.rounds.push(json!({
            "round": round,
            "phase": "fill",
            "candidate_sha256": super::knowledge::sha256(&candidate),
            "fills": fills.len(),
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
            });
        }
        talk.refused = Some(candidate);
        let tail = "\nFill the named holes again (the sketch stays as accepted); answer the same {\"fills\", \"notes\"} object.";
        if !repair(talk, text, diagnostics, tail) {
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
    system.push_str(nika_pack::doc(SKETCH_PATH).unwrap_or_default());
    let mut talk = Talk::open(
        system,
        format!("{opening}\n\nAnswer with the SKETCH (call 1), not a file."),
        route,
        allowed,
        request,
    );
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
