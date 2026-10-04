// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The engine's native output conventions, as the deterministic compile emits them.
use super::*;

/// The engine's output conventions state the written-total law the compiler emits (R4 A11,
/// E36: a judge held « write the sum to ./out/result.json » against the object the compiler
/// writes). Measured on emitted candidates: a total over every row goes to a structured file as
/// the compute's object and to a prose file as its value alone; an explicitly requested object is
/// that object, and an explicitly requested bare number is never silently wrapped (the
/// deterministic compile leaves it unread). The conventions say so, a requested shape
/// overriding, with no other wrapper, key or field.
#[test]
fn the_output_conventions_state_the_written_total_law() {
    let observed = json!({"observed": [{"path": "./data/input.csv", "state": "observed", "complete": false, "kind": "csv", "columns": ["id", "item", "status", "qty"]}]});
    let compile = |text: String| {
        nika_compile::compile(&CompileRequest::create(text).with_knowledge(observed.clone()))
            .unwrap()
    };
    let written = |write: &str| {
        let out = compile(format!("read ./data/input.csv, the total of qty, {write}"));
        assert_eq!(out.status, CompileStatus::Ready, "{write}: {out:#?}");
        let doc: Value = serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap();
        doc["tasks"]["write_output"]["with"]["content"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let object = "${{ tasks.compute.output }}";
    assert_eq!(written("write it to ./out/result.json"), object);
    assert_eq!(
        written("write it to ./out/result.md"),
        "${{ tasks.compute.output.total }}"
    );
    let requested = "write it as an object with a total field to ./out/result.json";
    assert_eq!(written(requested), object);
    let bare = compile(
        "read ./data/input.csv, the total of qty, write only the number to ./out/result.json"
            .to_owned(),
    );
    assert_ne!(bare.status, CompileStatus::Ready, "{bare:#?}");
    let conventions = include_str!("../../assets/native_output_conventions.md");
    for statement in [
        "A total over every row is written as the engine's compute returns it",
        "A total the engine types (a named total)",
        "to a structured file (json, csv, yaml, toml), the object with one field per named total",
        "to a prose file (md, txt or any other destination), the value alone when it is the only total",
        "several totals keep the object",
        "A shape the request names overrides both",
        "Add no other wrapper, key or field",
    ] {
        assert!(conventions.contains(statement), "{statement}");
    }
    assert!(!conventions.contains("add no wrapper, key or field the request did not ask for"));
}
