// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The forensic summary of one cognition compile (`decision.forensic`): which door answered and
//! why, who authored the candidate's bytes, the request apart from any semantic proposal, what
//! the model proposed where a proposal exists, the Foundry references actually shown, the calls
//! with their failures and unknown usage, and which evidence is bound to the final candidate.
//!
//! Projected AFTER the compile from the journals the doors already keep (the route, the
//! authoring receipt, the native rounds, the rehearsal and verification records): it makes no
//! call, changes no status, question, diagnostic or candidate, and grants nothing. A fact no
//! journal holds is `UNKNOWN` or `NOT_CAPTURED`, never reconstructed. Written only when the caller
//! offered a seat or a provider (the existing opt-in record); a seatless compile is unchanged.

use serde_json::{Map, Value, json};

use super::knowledge;
use crate::types::{EditChange, Input};
use crate::{CompileOutcome, CompileRequest, CompileStatus, Strategy};

/// The generation of this summary. Only a change of a field's meaning bumps it.
const VERSION: u32 = 1;

/// The route step of the policy that sends CREATE straight to the native source door.
pub(super) const NATIVE_ONLY: &str = "native: only";
/// The route step of the policy that sends CREATE straight to the sketch door.
pub(super) const NATIVE_SKETCH: &str = "native: sketch";
/// The route step of an escalating policy whose request carries attached Foundry references.
pub(super) const NATIVE_INFORMED: &str = "native: informed generation";
/// The route step of a plan round that escalated to the native source door.
pub(super) const NATIVE_ESCALATED: &str = "native: escalated";
/// The route step of a revision the constant door could not settle.
pub(super) const EDIT_NATIVE: &str =
    "edit: the constant door could not settle the change; the seat revises the base";

/// What this summary cannot observe at all in this version: named, never inferred.
const NOT_CAPTURED: &[&str] = &[
    "transform_program_proposals: the verified-transform calls keep their journal entries, not their decoded programs",
    "decision_seat_usage: a caller's seat reports usage in its own receipt, outside this compile",
    "semantic_judge_candidate_binding: verification records do not name the candidate digest they judged",
    "behavioral_satisfaction: no behavioral judge compares observations with the request's obligations",
];

/// Write `decision.forensic` when the caller offered cognition and the compile kept a record.
pub(super) fn record(request: &CompileRequest, offered: bool, out: &mut CompileOutcome) {
    if !offered || (out.provenance.decision.is_none() && out.provenance.authoring.is_none()) {
        return;
    }
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    let route: Vec<String> = decision["route"]
        .as_array()
        .map(|steps| {
            steps
                .iter()
                .filter_map(|s| s.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let summary = json!({
        "version": VERSION,
        "identity": identity(request),
        "intent": intent(request),
        "door": door(request, out, &decision, &route),
        "doors_tried": tried(&route),
        "proposal": proposal(request, out, &decision),
        "universe": universe(&decision),
        "foundry": foundry(request, out),
        "calls": calls(out, &decision),
        "evidence": evidence(out, &decision),
        "attempts": attempts(&decision),
        "not_captured": NOT_CAPTURED,
    });
    decision["forensic"] = summary;
    out.provenance.decision = Some(decision);
}

/// The engine, pack, spec, card and Foundry snapshot this compile read.
fn identity(request: &CompileRequest) -> Value {
    let mut identity = knowledge::identity();
    identity["foundry"] = request.authoring_knowledge.as_ref().map_or(
        Value::Null,
        |pack| json!({"identity": pack.identity, "selection": pack.selection}),
    );
    identity
}

/// The request as the caller stated it, by digest, apart from anything a model proposed.
fn intent(request: &CompileRequest) -> Value {
    match &request.input {
        Input::Create(text) => {
            let clarification = request
                .answers
                .get("intent.clarification")
                .and_then(|raw| serde_json::from_str::<String>(raw).ok())
                .filter(|text| !text.trim().is_empty());
            json!({
                "kind": "create",
                "original_sha256": crate::intent_sha256(text),
                "effective_sha256": crate::intent_sha256(clarification.as_deref().unwrap_or(text)),
                "replaced_by_clarification": clarification.is_some(),
            })
        }
        Input::Edit { source, change } => json!({
            "kind": "edit",
            "base_sha256": knowledge::sha256(source),
            "change": match change {
                EditChange::Text(words) => json!({"kind": "text", "sha256": knowledge::sha256(words)}),
                _ => json!({"kind": "constant"}),
            },
            "original_intent_sha256": request.original_intent.as_deref().map(crate::intent_sha256),
        }),
        _ => json!({"kind": "UNKNOWN"}),
    }
}

/// The door that settled the outcome, why it opened, and who wrote the candidate's bytes.
fn door(
    request: &CompileRequest,
    out: &CompileOutcome,
    decision: &Value,
    route: &[String],
) -> Value {
    let has = |step: &str| route.iter().any(|s| s == step);
    // The sketch door's own route steps name it before any record (a floor refusal has none):
    // the policy door before HOT, or a COLD round's composition the plan could not keep apart.
    // A composition stopped before any sketch call (native off, no repair allowance) has neither
    // step and stays the plan round's own record.
    let composition = has(super::sketch::COMPOSITION);
    let sketch = decision["native"].get("sketch").is_some() || has(NATIVE_SKETCH) || composition;
    let (name, reason) = match out.provenance.strategy {
        _ if request.plan.is_some() => ("replay", "answer_round_replays_a_recorded_plan"),
        Some(Strategy::Skeleton) => ("skeleton", "exact_skeleton_name"),
        Some(Strategy::Support) => ("support", "exact_support_grammar"),
        Some(Strategy::Hot) => ("hot", "every_clause_read_and_admitted"),
        Some(Strategy::Warm) => ("warm", "finite_ambiguity_settled_by_the_seat"),
        Some(Strategy::Cold) => ("cold_plan", "hot_rejected_and_warm_not_settling"),
        Some(Strategy::Native) => {
            let reason = if has(EDIT_NATIVE) {
                "nonconstant_revision"
            } else if has(NATIVE_SKETCH) {
                "policy_sketch_before_hot"
            } else if composition {
                "plan_composition_requires_sketch"
            } else if has(NATIVE_ONLY) {
                "policy_native_only_before_hot"
            } else if has(NATIVE_INFORMED) {
                "escalate_with_attached_foundry_references"
            } else if has(NATIVE_ESCALATED) {
                "plan_round_escalated"
            } else {
                "UNKNOWN"
            };
            (if sketch { "sketch" } else { "native_source" }, reason)
        }
        _ if route.iter().any(|s| s == "needs cognition") => ("none", "needs_cognition"),
        _ if route.iter().any(|s| s.starts_with("cold: ")) => {
            ("none", "cold_plan_without_candidate")
        }
        _ => ("none", "UNKNOWN"),
    };
    let owner = match (
        out.candidate.is_some() || out.provenance.plan.is_some(),
        name,
    ) {
        (false, _) | (_, "none") => "none",
        (_, "native_source") => "model",
        (_, "sketch") => "compiler_from_model_sketch_and_fills",
        // A sketch concludes under the native strategy word: its record cannot say whether a
        // model or the compiler wrote the replayed bytes.
        (_, "replay") => match request.plan.as_ref().and_then(|p| p["strategy"].as_str()) {
            Some("native") | None => "UNKNOWN",
            Some(_) => "deterministic_assembler",
        },
        _ => "deterministic_assembler",
    };
    json!({"name": name, "reason": reason, "source_owner": owner})
}

/// What the route says of each earlier door: tried and its verdict, or never tried.
fn tried(route: &[String]) -> Value {
    let hot = if route.iter().any(|s| s == "hot") {
        json!("admitted")
    } else if let Some(why) = route.iter().find_map(|s| s.strip_prefix("hot rejected: ")) {
        json!({"rejected": why})
    } else {
        json!("not_tried")
    };
    let warm = if route.iter().any(|s| s == "warm") {
        "settled"
    } else if route.iter().any(|s| s == "warm: none") {
        "none_or_failed"
    } else {
        "not_tried"
    };
    let cold = route
        .iter()
        .find(|s| s.starts_with("cold: ") && s.ends_with("sample(s)"))
        .map_or_else(|| json!("not_tried"), |step| json!({"called": step}));
    json!({"hot": hot, "warm": warm, "cold_plan": cold})
}

/// What the model proposed, where a proposal exists, and where its exact payload is kept.
fn proposal(request: &CompileRequest, out: &CompileOutcome, decision: &Value) -> Value {
    let plan_sha = |plan: &Value| knowledge::sha256(&plan.to_string());
    if request.plan.is_some() {
        return json!({"state": "NOT_CAPTURED", "why": "an answer round replays a recorded plan; this invocation proposed nothing"});
    }
    match out.provenance.strategy {
        Some(Strategy::Native) if decision["native"].get("sketch").is_some() => json!({
            "author": "model",
            "kind": "sketch_and_fills",
            "state": "captured",
            "payload": "decision.native.rounds[*].proposed_sketch | proposed_fills",
        }),
        Some(Strategy::Native) => json!({
            "author": "model",
            "kind": "source",
            "state": "NOT_CAPTURED",
            "why": "source-direct generation: the model wrote the candidate itself and no private semantic plan exists",
            "payload": "decision.native.rounds[*].candidate",
        }),
        Some(Strategy::Cold) => json!({
            "author": "model",
            "kind": "plan",
            "state": "captured",
            "payload": "provenance.authoring.context[*].proposed",
            "assembled_plan_sha256": out.provenance.plan.as_ref().map(plan_sha),
        }),
        Some(Strategy::Hot | Strategy::Warm) => json!({
            "author": if out.provenance.strategy == Some(Strategy::Warm) { "reader_and_decision_seat" } else { "reader" },
            "kind": "plan",
            "state": "deterministic",
            "assembled_plan_sha256": out.provenance.plan.as_ref().map(plan_sha),
        }),
        _ if out.provenance.authoring.is_some() => json!({
            "state": "see_calls",
            "payload": "provenance.authoring.context[*].proposed",
        }),
        _ => json!({"state": "none"}),
    }
}

/// The finite candidate universe a route built and where it is kept, or that this route built
/// none: COLD's composed plans, WARM's offered readings. Never reconstructed from another record.
fn universe(decision: &Value) -> Value {
    if let Some(candidates) = decision["candidates"].as_array() {
        return json!({
            "state": "recorded",
            "kind": "composed_plans",
            "path": "provenance.decision.candidates",
            "count": candidates.len(),
            "feasible_count": decision["feasible_count"],
            "selected_candidate": decision["selected_candidate"],
            "seat_choice_path": decision.get("warm_after_cold").map(|_| "provenance.decision.warm_after_cold"),
        });
    }
    if let Some(questions) = decision["questions"].as_array() {
        return json!({
            "state": "recorded",
            "kind": "seat_readings",
            "path": "provenance.decision.questions[*].options",
            "count": questions.len(),
        });
    }
    json!({"state": "not_applicable", "why": "this route built no finite candidate universe"})
}

/// How far one journaled call got: answered (a response came back, so its messages were
/// delivered), refused by a local admission before any byte left, or unknown (a provider failure
/// or a timeout may or may not have delivered it). Journaling a call proves only its preparation.
fn delivery(call: &Value) -> &'static str {
    if call["response"].is_object() {
        "answered"
    } else if call["result"]["failure_kind"] == "admission_refused" {
        "not_sent"
    } else {
        "unknown"
    }
}

/// The references attached to the request, those the prepared calls carried, and of those the
/// ones a model confirmably received (an answered call), apart from an undelivered or unknown
/// delivery.
fn foundry(request: &CompileRequest, out: &CompileOutcome) -> Value {
    let attached: Vec<Value> = request
        .authoring_knowledge
        .as_ref()
        .map(|pack| {
            pack.references
                .iter()
                .map(|r| json!({"id": r.id, "kind": r.kind, "bytes": r.text.len(), "sha256": knowledge::sha256(&r.text)}))
                .collect()
        })
        .unwrap_or_default();
    let (mut prepared, mut presented, mut unknown): (Vec<String>, Vec<String>, Vec<String>) =
        (Vec::new(), Vec::new(), Vec::new());
    let push = |list: &mut Vec<String>, id: &str| {
        if !list.iter().any(|p| p == id) {
            list.push(id.to_owned());
        }
    };
    for call in out
        .provenance
        .authoring
        .iter()
        .flat_map(|r| r.context.iter())
    {
        let state = delivery(call);
        let ids = call["references"].as_array().into_iter().flatten();
        for id in ids.filter_map(|r| r["id"].as_str()) {
            push(&mut prepared, id);
            match state {
                "answered" => push(&mut presented, id),
                "unknown" => push(&mut unknown, id),
                _ => {}
            }
        }
    }
    unknown.retain(|id| !presented.contains(id));
    let unseen: Vec<&str> = attached
        .iter()
        .filter_map(|r| r["id"].as_str())
        .filter(|id| !presented.iter().any(|p| p == id))
        .collect();
    json!({
        "attached": attached,
        "prepared": prepared,
        "presented": presented,
        "delivery_unknown": unknown,
        "attached_not_presented": unseen,
    })
}

/// The generative calls by role, their failures and whether their usage is complete; the
/// decision-seat records the compile kept.
fn calls(out: &CompileOutcome, decision: &Value) -> Value {
    let generative = out.provenance.authoring.as_ref().map_or_else(
        || json!({"count": 0, "usage": "none"}),
        |receipt| {
            let (mut roles, mut deliveries) = (Map::new(), Map::new());
            let mut failed = 0;
            for call in &receipt.context {
                for (map, key) in [
                    (&mut roles, call["call"].as_str().unwrap_or("UNKNOWN")),
                    (&mut deliveries, delivery(call)),
                ] {
                    let n = map.get(key).and_then(Value::as_u64).unwrap_or(0);
                    map.insert(key.to_owned(), json!(n + 1));
                }
                failed += usize::from(call["result"].get("failure_kind").is_some());
            }
            let complete = crate::authority::usage_complete(&receipt.context);
            json!({
                "count": receipt.calls,
                "by_role": roles,
                "delivery": deliveries,
                "failed": failed,
                "usage": if complete { "complete" } else { "incomplete" },
                "input_tokens": receipt.input_tokens,
                "output_tokens": receipt.output_tokens,
            })
        },
    );
    let warm = decision["questions"].as_array().map_or(0, Vec::len);
    let verification: Vec<Value> = decision["semantic_verification"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|a| json!({"attempt": a["attempt"], "judge": a["judge"]["kind"], "attempted": a["attempted"], "returned": a["returned"]}))
        .collect();
    json!({
        "generative": generative,
        "decision_seat": {
            "warm_questions": warm,
            "cold_choice": decision.get("warm_after_cold").is_some(),
            "verification": verification,
        },
    })
}

/// The final candidate's identity and which evidence is bound to those exact bytes.
fn evidence(out: &CompileOutcome, decision: &Value) -> Value {
    let candidate = out.candidate.as_deref().map(knowledge::sha256);
    let check = match &out.check_preview {
        None => "not_run",
        Some(preview) if preview.report.is_clean() => "clean",
        Some(_) => "findings",
    };
    let rehearsal = match decision.get("rehearsal") {
        None => json!({"state": "not_offered"}),
        Some(record) => {
            let reports = record["reports"].as_array().cloned().unwrap_or_default();
            let bound = candidate.as_ref().and_then(|sha| {
                reports
                    .iter()
                    .rev()
                    .find(|r| r["candidate_sha256"].as_str() == Some(sha))
            });
            match bound {
                // A host that never attempted the run observed nothing, even for these bytes.
                Some(report) => json!({
                    "state": if report["attempt"] == "never_attempted" { "not_run" } else { "observed" },
                    "bound_to_candidate": true,
                    "outcome": report["outcome"]["kind"],
                    "attempt": report["attempt"],
                }),
                None if reports.is_empty() => json!({"state": "not_run"}),
                None => {
                    json!({"state": "UNKNOWN", "bound_to_candidate": false, "reports": reports.len()})
                }
            }
        }
    };
    let semantic = decision["semantic_verification"].as_array().map_or_else(
        || json!({"state": "not_run"}),
        |attempts| {
            let last = attempts.last().cloned().unwrap_or(Value::Null);
            json!({
                "state": "recorded",
                "attempts": attempts.len(),
                "last_defects": last["defects"].as_array().map_or(0, Vec::len),
                "last_unknown": last["unknown"].as_array().map_or(0, Vec::len),
                "candidate_binding": "NOT_CAPTURED",
            })
        },
    );
    json!({
        "status": match out.status {
            CompileStatus::Ready => "ready",
            CompileStatus::Incomplete => "incomplete",
            CompileStatus::Refused => "refused",
            _ => "UNKNOWN",
        },
        "candidate_sha256": candidate,
        "check": check,
        "rehearsal": rehearsal,
        "semantic_judge": semantic,
        "behavioral_judge": {"state": "not_run", "why": "rehearsal observes execution; no behavioral judge ran in this compile"},
        "satisfaction": "UNKNOWN",
    })
}

/// The attempts the native or sketch conversation journaled, by identity: each round's phase,
/// the digest of what it proposed and how many diagnostics refused it.
fn attempts(decision: &Value) -> Value {
    let rounds = decision["native"]["rounds"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    json!(
        rounds
            .iter()
            .map(|r| json!({
                "round": r["round"],
                "phase": r.get("phase").cloned().unwrap_or_else(|| json!("native")),
                "call": r.get("call"),
                "candidate_sha256": r.get("candidate_sha256"),
                "sketch_sha256": r.get("sketch_sha256"),
                "diagnostics": r["diagnostics"].as_array().map(Vec::len),
            }))
            .collect::<Vec<_>>()
    )
}
