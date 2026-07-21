// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `workflow_started` prologue tests — the evidence-pack fields
//! (A5): the workflow's semantic hash · the declared boundary as JSON ·
//! the composer-selected sandbox backend, journaled so the pack reads
//! the run's identity + boundary + confinement from the journal's OWN
//! bytes. Split from `tests.rs` (that file rides the 1,500-LOC ceiling).

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use crate::*;

/// Run the prologue over one yaml fixture and return the started
/// event's string fields as (key, value) pairs.
fn started_fields(yaml: &str, sandbox: Option<&str>) -> Vec<(String, String)> {
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    emit_prologue(&wf, "pay", None, None, sandbox, &mut stamper, &mut sink);
    let started = &sink.events()[0];
    started
        .fields
        .iter()
        .filter_map(|f| match &f.value {
            nika_types::resource::Value::String(s) => Some((f.key.clone(), s.clone())),
            _ => None,
        })
        .collect()
}

fn get<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// The A5 evidence fields ride the prologue: the semantic hash (the
/// proof layer's OWN projection — byte-equal to the seal's), the
/// declared boundary as spec-wire JSON, and the sandbox backend name.
#[test]
fn prologue_journals_the_evidence_fields() {
    let yaml = "nika: v1\nworkflow:\n  id: pay\npermits:\n  fs: { read: [\"./in/**\"], write: [\"./out/**\"] }\n  exec: [\"echo\"]\ntasks:\n  t:\n    exec: { command: [\"echo\", \"x\"] }\n";
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let fields = started_fields(yaml, Some("seatbelt"));
    let sem = crate::proof::ir::semantic_ir_hash(&wf).expect("projectable");
    assert_eq!(get(&fields, "semantic_hash"), Some(sem.as_hex()));
    assert_eq!(get(&fields, "sandbox"), Some("seatbelt"));
    let permits: serde_json::Value =
        serde_json::from_str(get(&fields, "permits_json").expect("the boundary is journaled"))
            .expect("permits_json is valid JSON");
    assert_eq!(permits["fs"]["write"], serde_json::json!(["./out/**"]));
    assert_eq!(permits["exec"], serde_json::json!(["echo"]));
}

/// Absent facts stay absent (never an invented claim): no `permits:`
/// block → no `permits_json`; an unnamed backend → no `sandbox`. The
/// semantic hash rides regardless — it is the workflow's identity.
#[test]
fn prologue_omits_absent_boundary_and_backend() {
    let yaml = "nika: v1\nworkflow:\n  id: floor\ntasks:\n  t:\n    exec: { command: [\"echo\", \"x\"] }\n";
    let fields = started_fields(yaml, None);
    assert_eq!(
        get(&fields, "permits_json"),
        None,
        "no boundary = nothing journaled"
    );
    assert_eq!(
        get(&fields, "sandbox"),
        None,
        "an unnamed backend stays absent"
    );
    assert!(
        get(&fields, "semantic_hash").is_some(),
        "the identity is always journaled"
    );
}
