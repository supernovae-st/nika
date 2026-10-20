// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(clippy::unwrap_used)]

use super::*;

fn outcome(source: &str) -> CompileOutcome {
    let mut out = nika_compile::initial();
    out.status = CompileStatus::Ready;
    out.candidate = Some(source.into());
    out.provenance.decision = Some(json!({"semantic_verification": [
        {"candidate_sha256": nika_compile::surface::sha256(source), "attempt": 1,
         "request": "write the report", "context_sha256": "a".repeat(64),
         "judge": {"kind": "decision_seat", "seat": "fixture/judge"},
         "attempted": 1, "returned": 1, "consumed": 0, "settled": false,
         "unknown": ["output"], "questions": []},
        {"candidate_sha256": nika_compile::surface::sha256(source), "attempt": 2,
         "request": "write the report", "context_sha256": "a".repeat(64),
         "judge": {"kind": "decision_seat", "seat": "fixture/judge"},
         "attempted": 1, "returned": 1, "consumed": 1, "settled": true,
         "settled_by": "whole", "questions": [{"key": "whole", "read_back": false}],
         "usage": {"input_tokens": 73, "output_tokens": 4}}
    ]}));
    out
}

#[test]
fn original_attempts_survive_projection_and_cache_without_recounting() {
    let source = "nika: report\n";
    let out = outcome(source);
    let judged = Judged::ready("scope".into(), source, &out);
    let first = judged.projection(4).unwrap();
    assert_eq!(first["schema"], "nika/semantic-verification@1");
    assert_eq!(
        first["candidate_sha256"],
        nika_compile::surface::sha256(source)
    );
    assert_eq!(first["candidate_revision"], 4);
    assert_eq!(first["reused"], false);
    assert_eq!(
        first["records"],
        out.provenance.decision.as_ref().unwrap()["semantic_verification"]
    );
    let cached = judged.cached().projection(4).unwrap();
    let mut expected = first;
    expected["reused"] = json!(true);
    assert_eq!(cached, expected);
    assert!(
        cached.get("revision").is_none(),
        "no invented document base hash"
    );
}

#[test]
fn no_ready_records_means_no_projection() {
    let source = "nika: report\n";
    assert_eq!(Judged::bare("scope".into(), source).projection(1), None);
    let mut out = outcome(source);
    out.status = CompileStatus::Incomplete;
    assert_eq!(
        Judged::ready("scope".into(), source, &out).projection(1),
        None
    );
    out.status = CompileStatus::Ready;
    out.provenance.decision = Some(json!({"semantic_verification": ["not a record"]}));
    assert_eq!(
        Judged::ready("scope".into(), source, &out).projection(1),
        None
    );
}

#[test]
fn shown_evidence_follows_only_its_exact_revision_bytes_and_scope() {
    let source = "nika: report\n";
    let judged = Judged::ready("scope".into(), source, &outcome(source));
    let shown = Shown::new(4, judged.clone());
    assert_eq!(shown.repeated(4, source, "scope"), Some(judged.cached()));
    assert_eq!(shown.repeated(5, source, "scope"), None);
    assert_eq!(shown.repeated(4, "nika: other\n", "scope"), None);
    assert_eq!(shown.repeated(4, source, "other scope"), None);
}
