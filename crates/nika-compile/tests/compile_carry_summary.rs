// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A computed summary carried to a webhook is posted as its JSON text (measured on the
//! sealed-v3 lane of 2026-09-22, sv3-42: `nika:notify` refused a message that was the
//! summary object). The plan below is the one the seat recorded on that lane, replayed
//! through the assembler with zero calls.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, compile};
use serde_json::Value;

mod common;

const INTENT: &str = "Lesen Sie ./aquarium/messungen.csv (Spalten becken, ph, temperatur). Schreiben Sie alle Becken, deren ph unter 6,5 oder über 7,5 liegt, mit denselben Spalten nach ./out/auffaellig.csv. Wenn mehr als zwei Becken betroffen sind, senden Sie genau eine POST-Anfrage an http://127.0.0.1:59116/alarm mit der Anzahl der betroffenen Becken; andernfalls senden Sie nichts.";

/// The seat's recorded plan (gpt-5-mini, lane3, r1): a read, a typed filter, a validate
/// over the condition, a write and an automatic send of the count.
const PLAN: &str = r#"{"bindings": [{"literal": "./aquarium/messungen.csv", "role": "path"}, {"literal": "./out/auffaellig.csv", "role": "path"}, {"literal": "http://127.0.0.1:59116/alarm", "role": "url"}], "constraints": ["Lesen Sie ./aquarium/messungen.csv (Spalten becken, ph, temperatur).", "Wenn mehr als zwei Becken betroffen sind, senden Sie genau eine POST-Anfrage an http://127.0.0.1:59116/alarm mit der Anzahl der betroffenen Becken; andernfalls senden Sie nichts."], "effects": [{"evidence": "Schreiben Sie alle Becken, deren ph unter 6,5 oder über 7,5 liegt, mit denselben Spalten nach ./out/auffaellig.csv.", "policy": "automatic", "policy_literal": null, "target": "./out/auffaellig.csv", "verb": "write"}, {"evidence": "genau eine POST-Anfrage an http://127.0.0.1:59116/alarm mit der Anzahl der betroffenen Becken", "policy": "automatic", "policy_literal": null, "target": "genau eine POST-Anfrage an http://127.0.0.1:59116/alarm", "verb": "send"}], "obligations": [], "operations": [{"categories": [], "detail": "Lesen Sie ./aquarium/messungen.csv (Spalten becken, ph, temperatur).", "evidence": "Lesen Sie ./aquarium/messungen.csv (Spalten becken, ph, temperatur).", "op": "read"}, {"categories": [], "detail": "Behalte Zeilen, deren ph unter 6,5 oder über 7,5 liegt; Spalten becken, ph, temperatur. ; Zähle die Anzahl betroffener Becken (aus dem gefilterten Satz). ; Schreiben Sie alle Becken, deren ph unter 6,5 oder über 7,5 liegt, mit denselben Spalten nach ./out/auffaellig.csv.", "evidence": "ph unter 6,5 oder über 7,5", "op": "compute"}, {"categories": [], "detail": "Validiere: wenn die gezählte Anzahl > 2 ist, sende genau eine POST-Anfrage an http://127.0.0.1:59116/alarm mit der Anzahl der betroffenen Becken; andernfalls nichts senden.", "evidence": "Wenn mehr als zwei Becken betroffen sind, senden Sie genau eine POST-Anfrage an http://127.0.0.1:59116/alarm mit der Anzahl der betroffenen Becken; andernfalls senden Sie nichts.", "op": "validate"}], "rules": [{"clauses": [{"comparator": "<", "field": "ph", "value": "6.5", "value_kind": "number"}, {"comparator": ">", "field": "ph", "value": "7.5", "value_kind": "number"}], "fields": ["ph", "becken", "temperatur"], "jq": "[.records[] | select((.ph | tonumber) < 6.5 or (.ph | tonumber) > 7.5)] | map({\"becken\": .becken, \"ph\": .ph, \"temperatur\": .temperatur})", "junction": "or", "lines": false, "shape": {"aggregations": [], "columns": ["becken", "ph", "temperatur"], "derived": [], "descending": false, "distinct": false, "group_by": null, "join_on": null, "limit": null, "renames": [], "sort_by": null}, "summary": false, "synthesized": true, "text": "ph unter 6,5 oder über 7,5"}], "strategy": "cold", "trigger": null, "unknowns": []}"#;

#[test]
fn a_carried_summary_is_posted_as_its_json_text() {
    let plan: Value = serde_json::from_str(PLAN).unwrap();
    let out = compile(
        &CompileRequest::create(INTENT)
            .with_plan(plan)
            .answer("model", r#""mock/echo""#),
    )
    .unwrap();
    let source = out.candidate.as_deref().expect("a candidate is assembled");
    let doc: Value = serde_yaml_bw::from_str(source).unwrap();
    let send = &doc["tasks"]["send"];
    assert_eq!(send["invoke"]["tool"], "nika:notify", "{source}");
    let content = send["with"]["content"].as_str().unwrap_or_default();
    assert!(
        content.contains("tasks.send_text.output"),
        "the message is the summary's JSON text, never the object: {source}"
    );
    let text = &doc["tasks"]["send_text"];
    assert_eq!(text["invoke"]["args"]["expression"], "tojson", "{source}");
    assert!(
        text["with"]["data"]
            .as_str()
            .unwrap_or_default()
            .contains("compute_summary"),
        "{source}"
    );
}
