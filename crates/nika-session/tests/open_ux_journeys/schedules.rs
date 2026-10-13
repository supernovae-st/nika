// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The schedule journeys: a recurring request on the agent route carries its schedule to the
//! proposal, its zone shown as a choice whose default is the person's own; saving declares
//! nothing, and « activate » declares the beat in `nika.yaml`.

use serde_json::{Value, json};

use super::{Act, Scenario};
use crate::peer::{Script, Step, call, call_saying};

/// A weekly request: the words that say when, then the work.
const WEEKLY: &str = "chaque lundi à 9h, lis ./notes/semaine.md et écris-le dans ./out/semaine.md";
/// The words of `WEEKLY` that say when.
pub(crate) const WHEN: &str = "chaque lundi à 9h";
/// The person's zone: the child runs on a machine at UTC+4, and the Session names it.
pub(crate) const ZONE: &str = "Asia/Dubai";
/// A request whose words state no hour.
const MORNINGS: &str = "chaque matin, lis ./notes/semaine.md et écris-le dans ./out/semaine.md";
/// What the author says with its proposal: nothing of the schedule, which the card shows.
const SHOWN: &str = "Voici le workflow : il lit les notes de la semaine et les écrit.";
/// The notes the workflow reads.
const NOTES: &str = "lundi : relire le plan\n";

/// The copy as the author writes it, for one run: one read, one write, the paths the person
/// typed. The workflow holds no schedule.
const COPY: &str = r#"nika: weekly-copy
permits:
  tools: ["nika:read", "nika:write"]
  fs:
    read: ["./notes/semaine.md"]
    write: ["./out/semaine.md"]
tasks:
  read_notes:
    invoke: { tool: "nika:read", args: { path: "./notes/semaine.md" } }
  copy:
    with: { text: "${{ tasks.read_notes.output }}" }
    invoke:
      tool: "nika:write"
      args:
        path: "./out/semaine.md"
        content: "${{ with.text }}"
        overwrite: true
        create_dirs: true
"#;

/// The copy written as a candidate: the person typed both paths, so it states no selection.
fn copy() -> Value {
    json!({"source": COPY, "resolutions": [],
        "summary": "lit ./notes/semaine.md et l'écrit dans ./out/semaine.md"})
}

/// The zone a schedule follows, shown as a choice: the person's zone recommended.
fn zone_choice(when: &str) -> Value {
    let question = format!("Quel fuseau horaire pour « {when} » ?");
    json!({"key": "timezone", "question": question, "role": "value",
        "options": [
            {"key": "local", "label": "Asia/Dubai, le fuseau de cette machine",
             "recommended": true, "values": [{"role": "value", "value": ZONE}]},
            {"key": "paris", "label": "Europe/Paris",
             "values": [{"role": "value", "value": "Europe/Paris"}]}]})
}

/// `propose` with the schedule the words state, its zone shown as a choice.
fn scheduled(when: &str) -> Value {
    json!({"schedule": {"message": "u1", "excerpt": when, "timezone": ZONE},
        "open": [zone_choice(when)]})
}

/// One journey on the notes: the person's acts, the author's steps.
fn journey(acts: Vec<Act>, agent: Vec<Step>) -> Scenario {
    Scenario {
        files: vec![("notes/semaine.md", NOTES)],
        history: false,
        preparing: false,
        acts,
        script: Script {
            agent,
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// The copy written, then proposed with the schedule.
fn proposed() -> Vec<Step> {
    vec![
        call("candidate_write", copy()),
        call_saying(SHOWN, "propose", scheduled(WHEN)),
    ]
}

/// The weekly request proposed with its schedule, then saved with « oui ».
pub(super) fn schedule_saved() -> Scenario {
    journey(vec![Act::Turn(WEEKLY), Act::Submit("oui")], proposed())
}

/// Saved, then « activate »: the missed policy and the ceiling answered, the declaration
/// consented.
pub(super) fn schedule_activated() -> Scenario {
    let acts = vec![
        Act::Turn(WEEKLY),
        Act::Submit("oui"),
        Act::Turn("activate"),
        Act::Turn("1"),
        Act::Turn("0.20"),
        Act::Submit("oui"),
    ];
    journey(acts, proposed())
}

/// The schedule proposed in a zone no choice shows and the person never named, then with the
/// zone as a choice.
pub(super) fn schedule_zone_unseen() -> Scenario {
    let unseen = json!({"schedule": {"message": "u1", "excerpt": WHEN, "timezone": ZONE}});
    let agent = vec![
        call("candidate_write", copy()),
        call("propose", unseen),
        call_saying(SHOWN, "propose", scheduled(WHEN)),
    ];
    journey(vec![Act::Turn(WEEKLY)], agent)
}

/// Words that state no hour, proposed as a schedule; the author then asks the hour.
pub(super) fn schedule_without_hour() -> Scenario {
    let asked =
        json!({"questions": [{"key": "hour", "question": "À quelle heure, chaque matin ?"}]});
    let agent = vec![
        call("candidate_write", copy()),
        call("propose", scheduled("chaque matin")),
        call("ask", asked),
    ];
    journey(vec![Act::Turn(MORNINGS)], agent)
}
