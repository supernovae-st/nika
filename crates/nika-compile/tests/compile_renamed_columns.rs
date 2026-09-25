// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A renamed column keeps its place in the written CSV (sealed sv3-41 on the lane of
//! 2026-09-22: « benenne die Spalte `kwh` in `energie_kwh` um (sonst nichts ändern) » wrote
//! `anlage,datum,energie_kwh`, the sorted keys, where the source reads `datum,anlage,kwh`).
//! The plan below is the one the seat recorded on that lane, replayed with zero calls: the
//! source's header order is mapped through the rename before the CSV stage reads it.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, compile};
use serde_json::Value;

mod common;

const INTENT: &str = "Lies ./solar/ertrag.csv, benenne die Spalte kwh in energie_kwh um (sonst nichts ändern) und schreib das Ergebnis nach ./out/ertrag-umbenannt.csv";

const PLAN: &str = r#"{"bindings": [{"literal": "./solar/ertrag.csv", "role": "path"}, {"literal": "./out/ertrag-umbenannt.csv", "role": "path"}], "constraints": ["Lies ./solar/ertrag.csv, benenne die Spalte kwh in energie_kwh um (sonst nichts ändern) und schreib das Ergebnis nach ./out/ertrag-umbenannt.csv"], "effects": [{"evidence": "schreib das Ergebnis nach ./out/ertrag-umbenannt.csv", "policy": "automatic", "policy_literal": null, "target": "schreib das Ergebnis nach ./out/ertrag-umbenannt.csv", "verb": "write"}], "obligations": [], "operations": [{"categories": [], "detail": "Lies ./solar/ertrag.csv", "evidence": "./solar/ertrag.csv", "op": "read"}, {"categories": [], "detail": "benenne die Spalte kwh in energie_kwh um (sonst nichts ändern)", "evidence": "benenne die Spalte kwh in energie_kwh um (sonst nichts ändern)", "op": "compute"}], "rules": [{"clauses": [], "fields": [], "jq": ".records | map(with_entries(if .key == \"kwh\" then .key = \"energie_kwh\" else . end))", "junction": "and", "lines": false, "shape": {"aggregations": [], "columns": [], "derived": [], "descending": false, "distinct": false, "group_by": null, "join_on": null, "limit": null, "renames": [{"from": "kwh", "to": "energie_kwh"}], "sort_by": null}, "summary": false, "synthesized": true, "text": "benenne die Spalte kwh in energie_kwh um (sonst nichts ändern)"}], "strategy": "cold", "trigger": null, "unknowns": []}"#;

#[test]
fn a_renamed_column_keeps_its_place_in_the_written_csv() {
    let plan: Value = serde_json::from_str(PLAN).unwrap();
    let out = compile(&CompileRequest::create(INTENT).with_plan(plan)).unwrap();
    assert!(
        out.candidate.is_some(),
        "a candidate is assembled: {out:#?}"
    );
    let source = out.candidate.as_deref().unwrap();
    assert!(!source.contains("infer:"), "no model call: {source}");
    let doc: Value = serde_yaml_bw::from_str(source).unwrap();
    let columns = &doc["tasks"]["ertrag_umbenannt_columns"];
    assert_eq!(columns["invoke"]["tool"], "nika:jq", "{source}");
    let expression = columns["invoke"]["args"]["expression"]
        .as_str()
        .unwrap_or_default();
    assert!(
        expression.contains("\"kwh\"") && expression.contains("\"energie_kwh\""),
        "the header order is mapped through the rename: {source}"
    );
    assert!(
        columns["with"]["columns"]
            .as_str()
            .unwrap_or_default()
            .contains("source_columns"),
        "{source}"
    );
    let csv = &doc["tasks"]["ertrag_umbenannt_csv"];
    assert!(
        csv["with"]["columns"]
            .as_str()
            .unwrap_or_default()
            .contains("ertrag_umbenannt_columns"),
        "the CSV stage reads the mapped columns: {source}"
    );
}
