// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What the person sees of a schedule on the agent route, what saving and activating declare,
//! and which schedules the Session refuses to show.

use crate::peer::Seen;
use crate::scenarios::{WHEN, ZONE};
use crate::{outcome_kind, run, work};

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
    let offered = format!("say « activate » to declare « {WHEN} »");
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
        said.contains("Declared in `nika.yaml` · not active"),
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
