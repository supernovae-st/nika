// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(clippy::unwrap_used)]

use super::*;
use serde_json::json;

const SOURCE: &str = "nika: hello\nsteps:\n  - id: hi\n    exec: printf hello\n";

#[test]
fn presentation_reaches_the_document_door_without_becoming_an_answer() {
    let seat = AuthoringSeat::Deterministic { why: None };
    let context = AuthoringContext::default();
    let answers = BTreeMap::new();
    let presentation = json!({"kind":"proposal", "pending_choices":[{
        "id":"format-one", "key":"format", "question":"Choose a format", "state":"open",
        "options":[{"key":"text", "label":"Plain text", "recommended":true, "values":[]},
                   {"key":"json", "label":"JSON", "recommended":false, "values":[]}]}], "selected":null});
    let out = verify_presented_in(
        &seat,
        &context,
        ("print hello", &answers, None),
        (SOURCE, (&[], &[]), &presentation),
        vec![],
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        out.provenance.decision.as_ref().unwrap()["presentation"],
        presentation
    );
    assert_ne!(out.status, nika_onboard::compile::CompileStatus::Ready);
    assert_eq!(answers, BTreeMap::new());
}

#[test]
fn legacy_verification_has_no_presentation() {
    let seat = AuthoringSeat::Deterministic { why: None };
    let context = AuthoringContext::default();
    let answers = BTreeMap::new();
    let out = verify_in(
        &seat,
        &context,
        ("print hello", &answers, None),
        (SOURCE, (&[], &[])),
        vec![],
        None,
        None,
    )
    .unwrap();
    assert!(
        out.provenance
            .decision
            .as_ref()
            .is_none_or(|decision| decision.get("presentation").is_none())
    );
    assert_ne!(out.status, nika_onboard::compile::CompileStatus::Ready);
}
