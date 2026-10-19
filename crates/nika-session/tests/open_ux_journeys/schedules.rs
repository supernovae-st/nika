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
pub(super) fn zone_choice(when: &str) -> Value {
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

/// Declared as `schedule_activated`, then asked whether it is active, which the Session answers
/// from `nika.yaml` (live gauntlet r08, 2026-10-17: the model said « pas actif » right after the
/// install); the next line that reaches the author carries what Nika declared (live gauntlet
/// r02/r08, 2026-10-16: « rien n'est déclaré »).
pub(super) fn schedule_declared_then_asked() -> Scenario {
    let acts = vec![
        Act::Turn(WEEKLY),
        Act::Submit("oui"),
        Act::Turn("activate"),
        Act::Turn("1"),
        Act::Turn("0.20"),
        Act::Submit("oui"),
        Act::Turn("c'est actif maintenant ?"),
        Act::Turn("Merci. Et il copie quoi exactement ?"),
    ];
    let mut agent = proposed();
    agent.push(crate::peer::say(
        "Il copie notes/semaine.md vers la copie de la semaine.",
    ));
    journey(acts, agent)
}

/// The copy request of live gauntlet r09 (2026-10-17, NIK-199), with this project's paths: it
/// asks for a new workflow, and no word of it says save or run.
pub(crate) const COPY_REQUEST: &str = "Laisse tomber cette modification. Crée-moi un nouveau workflow weekly-copy.nika, identique à l'autre mais qui lit ./notes/semaine.md et l'écrit dans ./out/semaine.md.";

/// The author claims the copy request authorizes the save, and the consent reading would agree
/// (NIK-199): the proposal still waits for the person's yes.
pub(super) fn copy_request_claimed_as_consent() -> Scenario {
    let agent = vec![
        call("candidate_write", copy()),
        call_saying(
            "C'est enregistré.",
            "propose",
            json!({"acts": ["save"],
                "authorized_by": {"message": "u1", "excerpt": "Crée-moi un nouveau workflow"}}),
        ),
    ];
    let mut scenario = journey(vec![Act::Turn(COPY_REQUEST)], agent);
    scenario.script.picks = vec!["change_then_save".to_owned()];
    scenario
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

/// The label of the cadence the person picks from a list.
pub(crate) const DAILY: &str = "Tous les jours à 9h";

/// The cadence asked as a list with no default, free words beside it.
fn cadence_asked() -> Value {
    json!({"questions": [{"key": "cadence", "question": "À quelle fréquence ?", "free_text": true,
        "options": [
            {"key": "daily", "label": DAILY},
            {"key": "weekdays", "label": "Du lundi au vendredi à 9h"}]}]})
}

/// Words that state no hour; the author asks the cadence as a list, the person picks `daily`
/// from it (a native list sends its key as their line), and the author proposes the schedule
/// cited from that pick's label; « oui » saves it.
pub(super) fn schedule_cadence_picked() -> Scenario {
    let picked = json!({"schedule": {"message": "u2", "excerpt": DAILY, "timezone": ZONE},
        "open": [zone_choice(DAILY)]});
    let agent = vec![
        call("ask", cadence_asked()),
        call("candidate_write", copy()),
        call_saying(SHOWN, "propose", picked),
    ];
    let acts = vec![
        Act::Turn(MORNINGS),
        Act::AnswerQuestionOf(0, "daily"),
        Act::Submit("oui"),
    ];
    journey(acts, agent)
}

/// The weekly request proposed with its zone as a choice; the person picks the zone's default
/// from that choice, which the proposal shown keeps with no author; « oui » saves it.
pub(super) fn schedule_zone_picked() -> Scenario {
    let acts = vec![
        Act::Turn(WEEKLY),
        Act::AnswerProposalChoiceOf(0, 0, "local"),
        Act::Submit("oui"),
    ];
    journey(acts, proposed())
}

/// What the person asks of a shown proposal: a change that leaves its schedule as it is.
pub(crate) const COMMENT: &str = "ajoute un commentaire en haut du workflow";
/// The comment the revision adds.
pub(crate) const COMMENTED: &str = "# copie hebdomadaire des notes";

/// The copy revised as `COMMENT` asks; its bytes never hold the zone.
fn commented() -> Value {
    json!({"source": format!("{COMMENTED}\n{COPY}"), "resolutions": [],
        "summary": "un commentaire en haut du workflow"})
}

/// The zone picked as in `schedule_zone_picked`; then the person asks a change, the author
/// revises the copy and proposes it with the same schedule, and « oui » saves the revision.
pub(super) fn schedule_zone_revised() -> Scenario {
    let again = json!({"schedule": {"message": "u1", "excerpt": WHEN, "timezone": ZONE}});
    let mut agent = proposed();
    agent.extend([
        call("candidate_write", commented()),
        call_saying(SHOWN, "propose", again),
    ]);
    let acts = vec![
        Act::Turn(WEEKLY),
        Act::AnswerProposalChoiceOf(0, 0, "local"),
        Act::Submit(COMMENT),
        Act::Submit("oui"),
    ];
    journey(acts, agent)
}

/// The weekly request saved with its schedule; the person then asks a change, the author
/// proposes the revision with no schedule of its own, « oui » saves it, and « activate »
/// follows (live gauntlet r02 B10, 2026-10-17: after a repair, the zone was asked again).
pub(super) fn schedule_saved_then_revised() -> Scenario {
    let mut agent = proposed();
    agent.extend([
        call("candidate_write", commented()),
        call_saying(SHOWN, "propose", json!({})),
    ]);
    let acts = vec![
        Act::Turn(WEEKLY),
        Act::Submit("oui"),
        Act::Turn(COMMENT),
        Act::Submit("oui"),
        Act::Turn("activate"),
    ];
    journey(acts, agent)
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
