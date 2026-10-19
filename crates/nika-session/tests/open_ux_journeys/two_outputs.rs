// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Two outputs, one whose start the person's words leave open: a report in `notes/`, and a
//! payload `out/notification.json` whose name may start at `payload` (no folder the request names
//! holds a reading). The author asks where the payload's name starts; the person's pick settles it,
//! as does the path they type in their own words, and that answer settles no other name.

use serde_json::{Value, json};

use super::{Act, Scenario, resolution};
use crate::peer::{Script, call, call_saying, say};

/// The request: the report in `notes/`, the payload's start left open.
const REQUEST: &str = "Lis ./world/source.json et écris-le dans notes/report.json, puis prépare un payload out/notification.json contenant le canal stock.";
/// The same request with a second name sharing the payload's path, whose start stays open too.
const TWO_NAMES: &str = "Lis ./world/source.json et écris-le dans notes/report.json, puis prépare un payload out/notification.json contenant le canal stock. Il faudra relire out/notification.json avant l'envoi.";
/// The second name, which the payload's answer never settles.
pub(crate) const RELIRE: &str = "faudra relire out/notification.json";
/// The report, as the person's words name it.
pub(crate) const REPORT: &str = "notes/report.json";
/// The payload's open name, and its reading of the path alone.
pub(crate) const PAYLOAD_NAME: &str = "payload out/notification.json";
pub(crate) const PAYLOAD_PATH: &str = "out/notification.json";
/// The question about where the payload's name starts, quoting it.
const WHERE: &str = "Où commence le nom du fichier dans « payload out/notification.json » ?";
/// The source the workflow reads.
const SOURCE: &str = "{\"items\": []}\n";

/// The report and the payload, as the author writes them.
const BOTH: &str = r#"nika: stock-payload
permits:
  tools: ["nika:read", "nika:write"]
  fs:
    read: ["./world/source.json"]
    write: ["./notes/report.json", "./out/notification.json"]
tasks:
  read_source:
    invoke: { tool: "nika:read", args: { path: "./world/source.json" } }
  report:
    with: { text: "${{ tasks.read_source.output }}" }
    invoke:
      tool: "nika:write"
      args:
        path: "./notes/report.json"
        content: "${{ with.text }}"
        overwrite: true
        create_dirs: true
  payload:
    invoke:
      tool: "nika:write"
      args:
        path: "./out/notification.json"
        content: "{\"channel\": \"stock\"}"
        overwrite: true
        create_dirs: true
"#;

/// The candidate, the report bound from the person's words: a value they typed.
fn written() -> Value {
    let report = resolution(REPORT, "answered", "output_path", "u1", REPORT);
    json!({"source": BOTH, "resolutions": [report], "summary": "le rapport et le payload"})
}

/// The author's question about where the payload's name starts: its readings, no default.
fn where_asked() -> Value {
    let option = |key: &str, reading: &str| json!({"key": key, "label": reading, "values": [{"role": "value", "value": reading}]});
    let options = [
        option("reading-1", PAYLOAD_NAME),
        option("reading-2", PAYLOAD_PATH),
    ];
    json!({"questions": [{"key": "notification_path", "question": WHERE, "options": options}]})
}

/// The author's question about where the payload's name starts, free text: no offer to pick.
fn where_typed() -> Value {
    json!({"questions": [{"key": "notification_path", "question": WHERE, "free_text": true,
        "options": []}]})
}

/// The author binds the path the person typed, from their line, under `key`.
fn typed_bound(key: &str) -> Value {
    json!({"answered": [{"key": key, "value": PAYLOAD_PATH, "message": "u2",
        "excerpt": PAYLOAD_PATH}]})
}

/// The author writes both outputs and asks where the payload's name starts, free text; the person
/// types the path alone, the author binds it from their line (as `bound`) and proposes.
fn typed_for(request: &'static str, bound: Value) -> Scenario {
    Scenario {
        files: vec![("world/source.json", SOURCE)],
        history: false,
        preparing: false,
        acts: vec![Act::Turn(request), Act::Submit(PAYLOAD_PATH)],
        script: Script {
            agent: vec![
                call("candidate_write", written()),
                call("ask", where_typed()),
                call("ask", bound),
                call_saying("Voici le workflow.", "propose", json!({})),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// The payload's question answered in the person's own words.
pub(super) fn two_outputs_typed() -> Scenario {
    typed_for(REQUEST, typed_bound("notification_path"))
}

/// The same answer, with a second name sharing the payload's path.
pub(super) fn two_outputs_typed_elsewhere() -> Scenario {
    typed_for(TWO_NAMES, typed_bound("notification_path"))
}

/// The same reply, bound by the author under another key than the question it answered.
pub(super) fn two_outputs_typed_rebound() -> Scenario {
    typed_for(TWO_NAMES, typed_bound("relire_path"))
}

/// The payload's question answered in the person's own words; then, nothing shown, the person
/// writes the same path again as an ordinary line, no question's reply, and the author proposes.
pub(super) fn two_outputs_typed_restated() -> Scenario {
    Scenario {
        files: vec![("world/source.json", SOURCE)],
        history: false,
        preparing: false,
        acts: vec![
            Act::Turn(TWO_NAMES),
            Act::Submit(PAYLOAD_PATH),
            Act::Turn(PAYLOAD_PATH),
        ],
        script: Script {
            agent: vec![
                call("candidate_write", written()),
                call("ask", where_typed()),
                call("ask", typed_bound("notification_path")),
                say("Noté."),
                // Asked once to end with a proposal, the author ends the turn in words again.
                say("Je propose le workflow dès que tout est réglé."),
                call_saying("Voici le workflow.", "propose", json!({})),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// The author writes both outputs and asks where the payload's name starts; the person picks the
/// path alone, and the author proposes.
pub(super) fn two_outputs_answered() -> Scenario {
    Scenario {
        files: vec![("world/source.json", SOURCE)],
        history: false,
        preparing: false,
        acts: vec![Act::Turn(REQUEST), Act::Submit("le chemin seul")],
        script: Script {
            agent: vec![
                call("candidate_write", written()),
                call("ask", where_asked()),
                call_saying("Voici le workflow.", "propose", json!({})),
            ],
            replies: Vec::new(),
            picks: vec!["reading-2".to_owned()],
            verdicts: Vec::new(),
        },
    }
}
