// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The pure half of a semantic record's replay (slice C): the closed format the sketch door's
//! record keeps, decoded and judged again by the sketch laws, its graph and fills completed into
//! the document they state. A reconstruction only: no core type, no I/O, no lowering to bytes
//! (the compile core serializes the document and binds its identity), never READY and never an
//! authority. The request basis, the caller, the answers, the grants and the current judgment
//! are projected by the core and the cognition that read them; `intent` and `allowed` come from the
//! caller's admission, never from the record.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::{Sketch, complete_document, fills_from_json, structural_laws_observed};
use crate::behavior::{Contract, Presence, Requirement, contract_of_request};

/// Whether `now`, this request's caller basis, is the one a record's `stored` caller basis
/// states: the same words, original, money spans and operator money, each initial answer
/// unchanged in `current`, and no clarification the record never had (a replacement is a new
/// basis). An initial answer map that is not one of strings matches nothing.
#[must_use]
pub fn same_caller(stored: &Value, now: &Value, current: &BTreeMap<String, String>) -> bool {
    let initial = (stored["answers"].as_object()).filter(|map| map.values().all(Value::is_string));
    ["input", "original_intent", "money", "stated_money"]
        .iter()
        .all(|key| stored[*key] == now[*key])
        && initial.is_some_and(|initial| {
            (initial.iter()).all(|(k, v)| current.get(k).map(String::as_str) == v.as_str())
                && (initial.contains_key("intent.clarification")
                    || !current.contains_key("intent.clarification"))
        })
}

/// The record's format and lowering versions this compiler replays.
const FORMAT: u64 = 1;
const LOWERING: u64 = 1;
/// Every key a record may hold, and the question fields a native settlement reads.
const RECORD_KEYS: &str = "semantic_record lowering intent_sha256 basis sketch fills settlement \
    assembly_sha256 final source source_is";
const QUESTION_KEYS: &str = "key label answer_type why options";
/// The answer types a native settlement reads, and the bounds of a choice's options.
const ANSWER_TYPES: [&str; 4] = ["", "text", "literal", "choice"];
const MAX_OPTIONS: usize = 64;
const MAX_OPTION_BYTES: usize = 256;

/// Whether `value` is an object holding only `keys`.
fn closed(value: &Value, keys: &str) -> bool {
    (value.as_object()).is_some_and(|map| {
        map.keys()
            .all(|key| keys.split_whitespace().any(|k| k == key))
    })
}

/// Whether `value` is an object of string values: an answer map.
fn answer_map(value: &Value) -> bool {
    (value.as_object()).is_some_and(|map| map.values().all(Value::is_string))
}

/// Whether every field of the record is the one its format states: closed at each level, every
/// required field present and of its type (no absent or mistyped field read as empty).
fn well_formed(record: &Value) -> bool {
    let (basis, settlement, last) = (&record["basis"], &record["settlement"], &record["final"]);
    let bounded = |v: &Value| v.as_str().is_some_and(|t| t.len() <= MAX_OPTION_BYTES);
    let option = |o: &Value| closed(o, "key label") && bounded(&o["key"]) && bounded(&o["label"]);
    let question = |q: &Value| {
        closed(q, QUESTION_KEYS)
            && q["key"].is_string()
            && ["label", "answer_type", "why"]
                .iter()
                .all(|field| q.get(*field).is_none_or(Value::is_string))
            && ANSWER_TYPES.contains(&q["answer_type"].as_str().unwrap_or_default())
            && q.get("options").is_none_or(|options| {
                (options.as_array())
                    .is_some_and(|os| os.len() <= MAX_OPTIONS && os.iter().all(option))
            })
    };
    closed(record, RECORD_KEYS)
        && ["intent_sha256", "assembly_sha256", "source"]
            .iter()
            .all(|field| record[*field].is_string())
        && record["source_is"] == "pre_answer_assembly"
        && closed(&record["sketch"], "name tasks outputs")
        && record["fills"].is_array()
        && closed(settlement, "questions gaps trigger")
        && (settlement["questions"].as_array()).is_some_and(|qs| qs.iter().all(question))
        && (settlement["gaps"].as_array()).is_some_and(|gs| gs.iter().all(Value::is_string))
        && (settlement.get("trigger")).is_some_and(|t| t.is_null() || t.is_string())
        && closed(last, "answers candidate_sha256")
        && answer_map(&last["answers"])
        && (last.get("candidate_sha256")).is_some_and(|c| c.is_null() || c.is_string())
        && closed(basis, "read caller")
        && answer_map(&basis["read"]["answers"])
        && basis["caller"].is_object()
}

/// A semantic record's graph and fills completed again under `intent` and the values the round
/// allows: `{document, questions, gaps, trigger}`, the document they state and the settlement a
/// native settlement reads: the questions bound to the document's open placeholders under static
/// wording, each gap at its position and the trigger, their words kept only when they are
/// `intent`'s own (no identity binds them). A reconstruction, not a judgment: READY and every
/// authority stay with the caller.
///
/// # Errors
/// The static reason the record does not replay: not closed or mistyped, another format or
/// lowering, a graph or fills the laws refuse, questions that are not exactly the open
/// placeholders. Never a record value or key name.
pub fn replayed(record: &Value, intent: &str, allowed: &[String]) -> Result<Value, &'static str> {
    replayed_observed(record, intent, allowed, None)
}

/// The same reconstruction under the replaying request's own observation of the stated files
/// (never one a record carries): a read may reach the one file it places under a stated bare
/// name, as the sketch laws admit it.
///
/// # Errors
/// As [`replayed`].
pub fn replayed_observed(
    record: &Value,
    intent: &str,
    allowed: &[String],
    observed: Option<&Value>,
) -> Result<Value, &'static str> {
    if !well_formed(record) {
        return Err("it is not a closed record of this format");
    }
    if record["semantic_record"] != json!(FORMAT) || record["lowering"] != json!(LOWERING) {
        return Err("its format or lowering is not one this compiler replays");
    }
    let sketch = Sketch::from_json(&record["sketch"]).map_err(|_| "its graph is refused")?;
    let fills = fills_from_json(&json!({"fills": record["fills"]}));
    let document = (fills.ok())
        .filter(|_| structural_laws_observed(&sketch, intent, allowed, observed).is_empty())
        .and_then(|fills| complete_document(&sketch, &fills).ok())
        .ok_or("its graph or fills are refused by the laws")?;
    let settlement = &record["settlement"];
    let questions = settled_questions(&settlement["questions"], &document)?;
    let gaps: Vec<Value> = (settlement["gaps"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate())
    .map(|(n, gap)| match verbatim(gap, intent) {
        Some(words) => json!(words),
        None => json!(format!("the clause the record names as gap {}", n + 1)),
    })
    .collect();
    let trigger = match &settlement["trigger"] {
        Value::Null
            if nika_compile_reader::lexicon::read(intent)
                .plan
                .trigger
                .is_some() =>
        {
            return Err("its trigger omits the cadence the request states");
        }
        Value::Null => None,
        stated => {
            Some(verbatim(stated, intent).ok_or("its trigger is not the request's own words")?)
        }
    };
    Ok(json!({"document": document, "questions": questions, "gaps": gaps, "trigger": trigger}))
}

/// `value`'s words when they are, trimmed, verbatim text of `intent`: no identity binds what a
/// record states there, so only the request's own words are repeated.
fn verbatim<'a>(value: &'a Value, intent: &str) -> Option<&'a str> {
    value
        .as_str()
        .map(str::trim)
        .filter(|words| !words.is_empty() && intent.contains(words))
}

/// The record's questions, bound to the rebuilt document: their keys are exactly its open
/// placeholders (each `const.<slug>` the fills declared empty, answered or not), with no extra,
/// omitted or repeated key, so no question can reach a value the graph fixed or a path that is
/// not a const. The seat's wording is not repeated: each question keeps its key, its answer
/// type and a choice's bounded options, under a static label and reason.
fn settled_questions(questions: &Value, document: &Value) -> Result<Vec<Value>, &'static str> {
    let open: std::collections::BTreeSet<String> = (document["const"].as_object())
        .into_iter()
        .flatten()
        .filter(|(_, value)| value.as_str() == Some(""))
        .map(|(slug, _)| format!("const.{slug}"))
        .collect();
    let questions = questions.as_array().map(Vec::as_slice).unwrap_or_default();
    let keys: std::collections::BTreeSet<&str> =
        questions.iter().filter_map(|q| q["key"].as_str()).collect();
    if keys.len() != questions.len() || keys.iter().copied().ne(open.iter().map(String::as_str)) {
        return Err("its questions are not exactly the candidate's open placeholders");
    }
    Ok(questions
        .iter()
        .map(|q| {
            let key = q["key"].as_str().unwrap_or_default();
            let mut kept = json!({
                "key": key,
                "label": format!("The value of `{key}` in this candidate."),
                "answer_type": q["answer_type"].as_str().unwrap_or_default(),
                "why": "A value the request leaves open: your answer is written into this const of the candidate.",
            });
            if let Some(options) = q.get("options") {
                kept["options"] = options.clone();
            }
            kept
        })
        .collect())
}

/// An answer map: each answer key to its literal.
type Answers = BTreeMap<String, String>;

/// An answer map of strings; anything else is no answer map at all (never read as empty).
fn answers(value: &Value) -> Option<Answers> {
    (value.as_object()?.iter())
        .map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
        .collect()
}

/// The answers a record binds, held to A0 ⊆ Ak ⊆ Ac: its initial answers (`basis.read`, A0)
/// unchanged within the answers its final candidate binds (`final`, Ak), and those exactly this
/// round's (`current`, Ac). `(A0, Ak)`; a record cannot drop or change an answer it began with.
///
/// # Errors
/// The static reason the record's answers do not bind this round.
pub fn bound_answers(
    record: &Value,
    current: &BTreeMap<String, String>,
) -> Result<(Answers, Answers), &'static str> {
    let (Some(initial), Some(bound)) = (
        answers(&record["basis"]["read"]["answers"]),
        answers(&record["final"]["answers"]),
    ) else {
        return Err("it is not a closed record of this format");
    };
    if initial.iter().any(|(k, v)| bound.get(k) != Some(v)) {
        return Err("its bound answers drop or change an initial answer");
    }
    if bound.iter().any(|(k, v)| current.get(k) != Some(v)) {
        return Err("an answer it was assembled with is not this round's");
    }
    Ok((initial, bound))
}

/// The part of a request basis the reader and the behavior contract give, before any proposal:
/// the effective words and initial answers, every clause occurrence in order (repeats kept),
/// the reader's floor and the partial contract. The compile core adds the world's identity and
/// the obligation ledger it owns.
#[must_use]
pub fn read_basis(intent: &str, initial: &BTreeMap<String, String>) -> Value {
    let reading = nika_compile_reader::lexicon::read(intent);
    let contract = contract_of_request(intent, initial);
    json!({"effective": intent, "answers": initial, "seen": reading.seen,
           "floor": reading.plan.unknowns, "contract": contract_projection(&contract)})
}

/// The behavior contract as a request basis keeps it: for each obligation its identity, target,
/// presence kind, the reason of an unsupported requirement and the request's own words; and the
/// sources it names. Partial by construction: what the request proves stays recomputable from
/// the request, never claimed by a record.
#[must_use]
pub fn contract_projection(contract: &Contract) -> Value {
    let obligations: Vec<Value> = (contract.obligations.iter())
        .map(|o| {
            let presence = match &o.presence {
                Presence::Required => "required",
                Presence::Unproven => "unproven",
                Presence::Forbidden => "forbidden",
                Presence::Approval => "approval",
                Presence::Undecided => "undecided",
                Presence::When(_) | Presence::OnlyWhen(_) => "conditional",
            };
            let unsupported = match &o.requirement {
                Requirement::Unsupported(why) => Some(why.as_str()),
                _ => None,
            };
            json!([
                o.id,
                o.target.as_ref().map(|t| &t.path),
                presence,
                unsupported,
                o.evidence
            ])
        })
        .collect();
    json!({"obligations": obligations, "sources": contract.sources})
}

#[cfg(test)]
mod caller_tests {
    use super::*;

    #[test]
    fn replay_keeps_initial_answers_and_refuses_a_new_clarification_or_budget() {
        let stored = json!({"input": "copy", "original_intent": "old", "money": [1],
            "stated_money": null, "answers": {"destination": "a.txt"}});
        let mut current = BTreeMap::from([("destination".into(), "a.txt".into())]);
        assert!(same_caller(&stored, &stored, &current));
        current.insert("revision.path".into(), "b.txt".into());
        assert!(same_caller(&stored, &stored, &current));
        current.insert("intent.clarification".into(), "another request".into());
        assert!(!same_caller(&stored, &stored, &current));
        current.remove("intent.clarification");
        current.insert("destination".into(), "changed.txt".into());
        assert!(!same_caller(&stored, &stored, &current));
        current.insert("destination".into(), "a.txt".into());
        let mut more_money = stored.clone();
        more_money["money"] = json!([2]);
        assert!(!same_caller(&stored, &more_money, &current));
        let mut invalid = stored.clone();
        invalid["answers"]["destination"] = json!(42);
        assert!(!same_caller(&invalid, &invalid, &current));
    }
}
