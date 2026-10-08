// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The laws that end the verifier's work: its repairs when no repair count bounds them
//! ([`progressed`]), and its questions on bytes a judge already rejected, whose record
//! carries each rejection to every round that replays it ([`carry_declined`]); and the law of
//! a gap an author first declares after a refusal ([`gaps_after_refusal`]).

use nika_compile::CompileOutcome;
use nika_compile::surface::sha256;
use nika_compile_fidelity::fidelity::Diagnostic;
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

/// What a gap first declared after a refusal is told, before the refusal it followed.
const AFTER_REFUSAL: &str = "is declared a gap only after the document was refused. The request states it: keep it and repair it the way the refusal names, rather than drop the obligation; declare it a gap again only when no remedy the refusal names applies. The refusal:";

/// The gaps a repair round's answer declares for the first time after a `refusal` (R7): each one
/// told back as a finding that carries the refusal it followed, whose findings name the remedies
/// the engine supports (an exact grant, an address composed from stated words), so a stated
/// obligation is repaired rather than dropped. A gap `honest` holds (declared before any
/// refusal) stays the author's. The caller tells them once: declared again, a gap is accepted
/// and surfaced. Empty with no refusal or no new gap.
#[must_use]
pub fn gaps_after_refusal(
    gaps: &[String],
    honest: &[String],
    refusal: &[Diagnostic],
) -> Vec<Diagnostic> {
    if refusal.is_empty() {
        return Vec::new();
    }
    let refused: Vec<String> = (refusal.iter())
        .map(|d| format!("[{}] {}", d.kind, d.message))
        .collect();
    let refused = refused.join(" ");
    (gaps.iter())
        .filter(|gap| !honest.contains(gap))
        .map(|gap| Diagnostic {
            kind: "document_gap",
            message: format!("« {gap} » {AFTER_REFUSAL} {refused}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        CompileOutcome, Diagnostic, Value, carry_declined, gaps_after_refusal, json, progressed,
        sha256,
    };

    /// A gap first declared after a refusal is told back with that refusal, its remedy named;
    /// a gap declared before any refusal, or any gap with no refusal, is the author's.
    #[test]
    fn a_gap_first_declared_after_a_refusal_is_told_with_the_refusal() {
        let refusal = [Diagnostic {
            kind: "check",
            message: "NIKA-SEC-005: host 127.0.0.1 is refused — fix: grant 127.0.0.1".to_owned(),
        }];
        let gaps = ["the POST".to_owned(), "the sink".to_owned()];
        let told = gaps_after_refusal(&gaps, &["the sink".to_owned()], &refusal);
        assert_eq!(told.len(), 1, "{told:#?}");
        assert_eq!(told[0].kind, "document_gap");
        assert!(
            told[0]
                .message
                .starts_with("« the POST » is declared a gap only after")
        );
        assert!(
            told[0]
                .message
                .ends_with(&format!("[check] {}", refusal[0].message))
        );
        assert!(gaps_after_refusal(&gaps, &[], &[]).is_empty());
        assert!(gaps_after_refusal(&[], &[], &refusal).is_empty());
    }

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
