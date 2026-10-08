// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The laws that end the verifier's work: its repairs when no repair count bounds them
//! ([`progressed`]), and its questions on bytes a judge already rejected, whose record
//! carries each rejection to every round that replays it ([`carry_declined`]).

use nika_compile::CompileOutcome;
use nika_compile::surface::sha256;
use serde_json::{Value, json};

/// Whether a defect set is progress over the sets already repaired from: it names a part never
/// named before, or it narrows the last set (fewer parts, all among the last). Each new part
/// grows a finite set and each narrowing shrinks the last, so the repairs end.
#[must_use]
pub fn progressed(seen: &[Vec<String>], key: &[String]) -> bool {
    let new = key
        .iter()
        .any(|part| !seen.iter().flatten().any(|named| named == part));
    let narrowed = seen
        .last()
        .is_some_and(|last| key.len() < last.len() && key.iter().all(|part| last.contains(part)));
    new || narrowed
}

/// The record of bytes the judge rejected, kept with every rejection of them it carries
/// (`declined`): a round that replays it, whatever host resends it, repeats each with no call
/// (the verifier's replay check) and never asks that judge again on those bytes (R6), however often it
/// replays. A rejection this compile only repeated (carried, or the same bytes again) is kept
/// too, once per judge, context and request. With no rejection to carry (an abstention), the
/// record is dropped, as an abstention is never carried; so is a semantic record, whose closed
/// format holds no rejection: no round replays it to that judge.
pub fn carry_declined(out: &mut CompileOutcome) {
    let sha = out.candidate.as_deref().map(sha256);
    let record = (out.provenance.plan.as_ref()).filter(|r| r.get("semantic_record").is_none());
    let mut declined =
        (record.and_then(|record| record["declined"].as_array().cloned())).unwrap_or_default();
    let attempts = (out.provenance.decision.as_ref())
        .and_then(|decision| decision["semantic_verification"].as_array())
        .map_or(&[][..], Vec::as_slice);
    let key = |a: &Value| [&a["judge"], &a["context_sha256"], &a["request"]].map(Value::clone);
    for attempt in attempts {
        let rejected = attempt["rejected"] == Value::Bool(true)
            && attempt["settled"] == Value::Bool(false)
            && sha
                .as_deref()
                .is_some_and(|sha| attempt["candidate_sha256"] == sha);
        let known = (declined.iter()).any(|kept| {
            kept["candidate_sha256"] == attempt["candidate_sha256"] && key(kept) == key(attempt)
        });
        if rejected && !known {
            declined.push(attempt.clone());
        }
    }
    let carries = record.is_some() && !declined.is_empty();
    match out.provenance.plan.as_mut().and_then(Value::as_object_mut) {
        Some(record) if carries => {
            record.insert("declined".to_owned(), json!(declined));
        }
        _ => out.provenance.plan = None,
    }
}

#[cfg(test)]
mod tests {
    use super::{CompileOutcome, Value, carry_declined, json, progressed, sha256};

    fn set(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| (*p).to_owned()).collect()
    }

    /// An outcome showing `candidate`, its `record` kept and its judges' `attempts` journaled.
    fn shown(candidate: &str, record: Value, attempts: &[Value]) -> CompileOutcome {
        let mut out = nika_compile::initial();
        out.candidate = Some(candidate.to_owned());
        out.provenance.plan = Some(record);
        out.provenance.decision = Some(json!({"semantic_verification": attempts}));
        out
    }

    /// One judge's attempt on `candidate`, under one context and request.
    fn attempt(candidate: &str, judge: &str, rejected: bool, settled: bool) -> Value {
        json!({
            "candidate_sha256": sha256(candidate), "judge": {"seat": judge},
            "context_sha256": "context", "request": "request",
            "rejected": rejected, "settled": settled,
        })
    }

    /// Each rejection of the shown bytes rides the record once, after the rejections it already
    /// carried, however often the law runs; a settled, an unrejected or another bytes' attempt
    /// never does, and the rest of the record is kept as it was.
    #[test]
    fn a_record_carries_each_rejection_of_its_bytes_once() {
        let earlier = attempt("earlier bytes", "j", true, false);
        let rejected = attempt("bytes", "j", true, false);
        let journal = [
            rejected.clone(),
            rejected.clone(),
            attempt("bytes", "k", false, false),
            attempt("bytes", "l", true, true),
            attempt("other bytes", "m", true, false),
        ];
        let record = json!({"plan": 1, "declined": [earlier.clone()]});
        let mut out = shown("bytes", record, &journal);
        carry_declined(&mut out);
        carry_declined(&mut out);
        let kept = out.provenance.plan.unwrap_or_default();
        assert_eq!(kept, json!({"plan": 1, "declined": [earlier, rejected]}));
    }

    /// With no rejection to carry (an abstention) the record is dropped, and so is a semantic
    /// record, whose closed format holds no rejection, even beside one.
    #[test]
    fn a_record_with_no_rejection_or_a_semantic_one_is_dropped() {
        let abstained = shown(
            "bytes",
            json!({"plan": 1}),
            &[attempt("bytes", "j", false, false)],
        );
        let semantic = shown(
            "bytes",
            json!({"semantic_record": 1}),
            &[attempt("bytes", "j", true, false)],
        );
        for mut out in [abstained, semantic] {
            carry_declined(&mut out);
            assert_eq!(out.provenance.plan, None, "{out:#?}");
        }
    }

    /// A new part or a narrowing is progress; the same set, or a reshuffle of parts already
    /// repaired from, is not.
    #[test]
    fn progress_is_a_new_part_or_a_narrowing() {
        let seen = [set(&["a", "b"])];
        assert!(progressed(&seen, &set(&["c"])));
        assert!(progressed(&seen, &set(&["a"])));
        assert!(!progressed(&seen, &set(&["a", "b"])));
        assert!(!progressed(&[set(&["a"]), set(&["b"])], &set(&["a"])));
    }
}
