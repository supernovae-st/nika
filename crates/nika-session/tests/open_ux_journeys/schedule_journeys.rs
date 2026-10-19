// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What the person sees of a schedule on the agent route, what saving and activating declare,
//! and which schedules the Session refuses to show.

use serde_json::{Value, json};

use crate::peer::{Kind, Seen};
use crate::scenarios::{COMMENTED, DAILY, WHEN, ZONE};
use crate::{Journey, candidate, outcome_kind, proposal, provenance, run, work};

/// The state a judge's request shows it: the JSON its user turn holds after `STATE:`, up to the
/// options.
fn judged_state(seen: &Seen) -> Option<Value> {
    let said = (seen.body["messages"].as_array().into_iter().flatten())
        .filter(|message| message["role"] == "user")
        .find_map(|message| message["content"].as_str()?.strip_prefix("STATE:\n"))?;
    let (state, _) = said.split_once("\n\nOPTIONS:\n")?;
    serde_json::from_str(state).ok()
}

/// Whether the Session answered one of the author's calls with a refusal that states `law`.
fn refused_with(journey: &Journey, law: &str) -> bool {
    journey.agent.iter().any(|seen| seen.last().contains(law))
}

/// How many of the journey's first `steps` asked the person a question.
fn asked(journey: &Journey, steps: usize) -> usize {
    (0..steps)
        .filter(|at| outcome_kind(journey.step(*at)) == "question")
        .count()
}

/// A recurring request on the agent route shows its schedule with the proposal: the person's
/// words, the cron fields they state, and the zone as a choice whose default is theirs.
#[test]
fn a_recurring_request_shows_its_schedule_and_the_zone_as_a_default() {
    let journey = run("schedule_saved");
    let proposed = journey.step(0);
    assert_eq!(outcome_kind(proposed), "proposal", "{proposed:#}");
    let card = proposed["outcome"]["text"].as_str().unwrap_or_default();
    for shown in [
        format!("(« {WHEN} »)"),
        format!("proposed cron `0 9 * * 1` · zone {ZONE}"),
        "a schedule to activate AFTER saving".to_owned(),
        format!("`local` {ZONE}, le fuseau de cette machine (default): {ZONE}"),
    ] {
        assert!(card.contains(&shown), "{shown}: {card}");
    }
}

/// Saving a scheduled proposal saves the workflow and declares nothing: no `nika.yaml`, and the
/// report offers « activate » for the words the person used.
#[test]
fn saving_a_scheduled_proposal_declares_nothing() {
    let journey = run("schedule_saved");
    let saved = journey.step(1);
    assert!(work(saved)["saved"]["workflow"].is_string(), "{saved:#}");
    let said = saved["outcome"]["text"].as_str().unwrap_or_default();
    let offered = format!("dites « active-le » pour déclarer « {WHEN} »");
    assert!(said.contains(&offered), "{said}");
    let files = journey.report["files"].to_string();
    assert!(!files.contains("nika.yaml"), "{files}");
}

/// « activate » after an agent-led save asks only what the declaration still needs (the zone was
/// chosen with the proposal), then declares the beat in `nika.yaml`, which the readers `nika arm`
/// and `nika serve` load it with accept: the project shape and the cadence grammar, its laws, and
/// their agreement on the beat count.
#[test]
fn activating_after_an_agent_led_save_declares_a_beat_nika_arm_reads() {
    let journey = run("schedule_activated");
    let asked = journey.step(2);
    let first = asked["outcome"]["key"].as_str();
    assert_eq!(first, Some("project.missed"), "{asked:#}");
    let declared = journey.step(5);
    let said = declared["outcome"]["text"].as_str().unwrap_or_default();
    assert!(
        said.contains("Déclaré dans `nika.yaml` · pas actif"),
        "{said}"
    );
    let root = journey.dir.path().join("project");
    let text = std::fs::read_to_string(root.join("nika.yaml")).unwrap_or_default();
    let cadence = format!("cadence: \"TZ={ZONE} 0 9 * * 1\"");
    assert!(text.contains(&cadence), "{text}");
    assert!(text.contains("workflow: weekly-copy.nika"), "{text}");
    let (_, project) = (nika_vocab::project::discover(&root).unwrap()).expect("a project file");
    let registry = nika_cadence::parse_registry(&text).unwrap();
    assert_eq!(nika_cadence::validate(&registry).count(), 0, "{text}");
    assert_eq!(
        (project.arm().len(), registry.beat_count()),
        (1, 1),
        "{text}"
    );
}

/// A request to create or copy a workflow is proposed first, whatever the author claims of it
/// and a reading makes of it: the proposal waits for the person's yes, nothing is written and
/// nothing is said as saved (NIK-199, live gauntlet r09, 2026-10-17).
#[test]
fn a_copy_request_is_proposed_before_anything_is_written() {
    let journey = run("copy_request_claimed_as_consent");
    let shown = journey.step(0);
    assert_eq!(outcome_kind(shown), "proposal", "{shown:#}");
    assert_eq!(
        journey.report["files"],
        json!(["notes/semaine.md"]),
        "nothing written beside the notes"
    );
    assert!(work(shown)["saved"].is_null(), "{shown:#}");
    let said = shown.to_string();
    assert!(
        !said.contains("applied · wrote") && !said.contains("Enregistré"),
        "{said}"
    );
    let root = journey.dir.path().join("project");
    assert!(
        !root.join("weekly-copy.nika").exists(),
        "the workflow is not on disk"
    );
}

/// Asked after the declaration whether the schedule is active, the Session answers from the
/// project file, and the author reads what Nika declared with the person's next line: the
/// project file's own words, never its memory.
#[test]
fn the_author_reads_the_declaration_nika_saved() {
    let journey = run("schedule_declared_then_asked");
    let answered = journey.step(6);
    let said = answered["outcome"]["text"].as_str().unwrap_or_default();
    assert!(
        said.starts_with("Pas encore : `weekly-copy.nika` est déclaré dans `nika.yaml`"),
        "{answered:#}"
    );
    assert!(
        !(journey.agent.iter()).any(|seen| seen.last().contains("c'est actif maintenant")),
        "the Session answered it, never the author"
    );
    let asked = (journey.agent.iter())
        .find(|seen| seen.last().contains("il copie quoi exactement"))
        .expect("the next line reached the author");
    let last = asked.last();
    assert!(
        last.contains("saved the schedule declaration")
            && last.contains("`weekly-copy.nika`")
            && last.contains(&format!("TZ={ZONE} 0 9 * * 1"))
            && last.contains("Declared is not active"),
        "{last}"
    );
}

/// A schedule in a zone the person would never see is refused: their words do not name it and
/// no choice of the proposal offers it. Shown as a choice, the same schedule is proposed.
#[test]
fn a_schedule_zone_the_person_never_saw_is_refused() {
    let journey = run("schedule_zone_unseen");
    let refused = journey.agent.get(2).map(Seen::last).unwrap_or_default();
    let law = "is neither in the person's words nor the default of a choice";
    assert!(refused.contains(law), "{refused}");
    assert_eq!(outcome_kind(journey.step(0)), "proposal");
}

/// Words that state no hour are no schedule: the proposal is refused with why, and the author
/// asks when instead of choosing an hour.
#[test]
fn a_schedule_without_its_hour_is_asked_never_guessed() {
    let journey = run("schedule_without_hour");
    let refused = journey.agent.get(2).map(Seen::last).unwrap_or_default();
    assert!(refused.contains("is not a complete schedule"), "{refused}");
    assert_eq!(outcome_kind(journey.step(0)), "question");
}

/// A cadence picked from a list is cited from the label the person picked: the proposal shows its
/// schedule at once, the cadence is asked once, and « oui » saves the workflow.
#[test]
fn a_cadence_picked_from_a_list_is_proposed_from_its_label() {
    let journey = run("schedule_cadence_picked");
    assert_eq!(outcome_kind(journey.step(0)), "question");
    let proposed = journey.step(1);
    assert_eq!(outcome_kind(proposed), "proposal", "{proposed:#}");
    let card = proposed["outcome"]["text"].as_str().unwrap_or_default();
    for shown in [
        format!("(« {DAILY} »)"),
        format!("proposed cron `0 9 * * *` · zone {ZONE}"),
    ] {
        assert!(card.contains(&shown), "{shown}: {card}");
    }
    assert!(!refused_with(&journey, "cite the person's line"));
    let saved = journey.step(2);
    assert!(work(saved)["saved"]["workflow"].is_string(), "{saved:#}");
    let said = saved["outcome"]["text"].as_str().unwrap_or_default();
    let offered = format!("dites « active-le » pour déclarer « {DAILY} »");
    assert!(said.contains(&offered), "{said}");
    assert_eq!(asked(&journey, 3), 1, "the cadence is asked once");
}

/// A zone the person picked from a proposal's choice is theirs: proposed again without that
/// choice, the schedule is shown at once, the zone is never asked again, and « oui » saves it.
#[test]
fn a_zone_picked_from_a_proposal_is_never_asked_again() {
    let journey = run("schedule_zone_picked");
    let shown = journey.step(0);
    let card = shown["outcome"]["text"].as_str().unwrap_or_default();
    let cron = format!("proposed cron `0 9 * * 1` · zone {ZONE}");
    assert!(card.contains(&cron), "{card}");
    // The default the candidate carries is kept with no author: the proposal shown waits on.
    let again = journey.step(1);
    assert_eq!(outcome_kind(again), "facts", "{again:#}");
    assert_eq!(again["shown"]["proposal"], shown["shown"]["proposal"]);
    assert_eq!(work(again).get("open_choices"), None, "{again:#}");
    assert_eq!(journey.agent.len(), 2, "no author call after the proposal");
    assert_eq!(provenance(again, ZONE), ("offered".into(), "u2".into()));
    for law in [
        "is neither in the person's words",
        "already settled by the person",
    ] {
        assert!(!refused_with(&journey, law), "{law}");
    }
    assert_eq!(asked(&journey, 3), 0, "the zone is never asked");
    let saved = journey.step(2);
    assert!(work(saved)["saved"]["workflow"].is_string(), "{saved:#}");
}

/// A change the person asks after picking the zone keeps the zone beside the revision: the
/// revised proposal is shown with the same zone, nothing they saw bound is dropped, the zone is
/// never asked again, and « oui » saves the revision.
#[test]
fn a_revision_keeps_the_zone_the_person_picked() {
    let journey = run("schedule_zone_revised");
    let revised = journey.step(2);
    assert_eq!(outcome_kind(revised), "proposal", "{revised:#}");
    assert_ne!(
        proposal(revised),
        proposal(journey.step(1)),
        "a new revision is shown"
    );
    assert!(candidate(revised).starts_with(COMMENTED), "{revised:#}");
    let card = revised["outcome"]["text"].as_str().unwrap_or_default();
    let cron = format!("proposed cron `0 9 * * 1` · zone {ZONE}");
    assert!(card.contains(&cron), "{card}");
    assert!(
        !card.contains("open choices"),
        "the zone is not shown again: {card}"
    );
    assert_eq!(provenance(revised, ZONE), ("offered".into(), "u2".into()));
    for law in [
        "no longer in the candidate",
        "is neither in the person's words",
        "already settled by the person",
    ] {
        assert!(!refused_with(&journey, law), "{law}");
    }
    assert_eq!(asked(&journey, 4), 0, "the zone is never asked");
    let saved = journey.step(3);
    assert!(work(saved)["saved"]["workflow"].is_string(), "{saved:#}");
}

/// A revision saved with no schedule of its own keeps the one asked for the saved workflow, its
/// zone included: « activate » then asks only what the declaration still needs (live gauntlet
/// r02 B10, 2026-10-17: after a repair, the zone was asked again).
#[test]
fn a_saved_schedule_survives_a_revision_saved_without_one() {
    let journey = run("schedule_saved_then_revised");
    let revised = journey.step(3);
    assert!(
        work(revised)["saved"]["workflow"].is_string(),
        "{revised:#}"
    );
    let root = journey.dir.path().join("project");
    let saved = std::fs::read_to_string(root.join("weekly-copy.nika")).unwrap_or_default();
    assert!(
        saved.starts_with(COMMENTED),
        "the revision is saved: {saved}"
    );
    let asked = journey.step(4);
    assert_eq!(outcome_kind(asked), "question", "{asked:#}");
    assert_eq!(
        asked["outcome"]["key"].as_str(),
        Some("project.missed"),
        "{asked:#}"
    );
}

/// The schedule a proposal carries reaches its verification beside the bytes (#1720): every
/// question the judge reads shows it as the propose door accepted it (the line, the words, the
/// zone), the verification passes, the proposal is shown, and the candidate's bytes hold no
/// schedule and no comment standing for one.
#[test]
fn the_verification_reads_the_schedule_beside_the_candidate() {
    let journey = run("schedule_saved");
    let proposed = journey.step(0);
    assert_eq!(outcome_kind(proposed), "proposal", "{proposed:#}");
    let bytes = candidate(proposed);
    assert!(!bytes.is_empty() && !bytes.contains('#'), "{bytes}");
    assert!(
        !bytes.contains("lundi") && !bytes.contains("cron"),
        "{bytes}"
    );
    let states: Vec<Value> = (journey.seen.iter())
        .filter(|seen| seen.kind == Kind::Judge)
        .filter_map(judged_state)
        .collect();
    assert!(!states.is_empty(), "the scripted judge was asked");
    let beside = json!({"message": "u1", "excerpt": WHEN, "zone": ZONE});
    for state in &states {
        assert_eq!(state["presentation"]["schedule"], beside, "{state:#}");
    }
}
