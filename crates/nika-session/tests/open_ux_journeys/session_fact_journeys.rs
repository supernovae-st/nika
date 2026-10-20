// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What the Session says of its own work (QA gauntlet q09).

use crate::scenarios::{DETAILS, HOW_MUCH};
use crate::{outcome_kind, run};

/// `/details` after a Save and a run says what the conversation built, the workflow it saved and
/// the run's trace, never « no workflow was read in this session yet ».
#[test]
fn details_after_a_save_and_a_run_say_what_was_built_and_run() {
    let journey = run("details_after_a_run");
    let saved = journey.step(1)["outcome"]["text"]
        .as_str()
        .unwrap_or_default();
    assert!(
        saved.starts_with("appliqué · écrit `news-digest.nika`"),
        "{saved}"
    );
    assert_eq!(
        outcome_kind(journey.step(2)),
        "run_requested",
        "{:#}",
        journey.step(2)
    );
    let details = journey.step(4);
    assert_eq!(details["line"], DETAILS);
    let said = details["outcome"]["text"].as_str().unwrap_or_default();
    assert!(!said.contains("no workflow was read"), "{said}");
    assert!(said.contains("built by the conversation"), "{said}");
    assert!(said.contains("last saved: `news-digest.nika`"), "{said}");
    assert!(said.contains("Preparation:"), "{said}");
    assert!(said.contains("last run: trace `"), "{said}");
}

/// The author reads the Session's spend so far with the person's line, as `/status` says it, so
/// a question about what preparing cost is answered from the Session's own count, once any
/// model request was observed.
#[test]
fn the_author_reads_the_sessions_spend_with_each_line() {
    let journey = run("spend_asked");
    assert_eq!(journey.agent.len(), 3, "two calls, then the answer");
    let asked = journey.agent[2].last();
    assert!(asked.contains(HOW_MUCH), "{asked}");
    let fact = "The Session's spend so far, as the person's `/status` shows it: Preparation: ";
    assert!(asked.contains(fact), "{asked}");
    assert!(asked.contains("API requests observed"), "{asked}");
    // Before any model request, the spend says nothing worth a line.
    let first = journey.agent[0].last();
    assert!(!first.contains(fact), "nothing spent yet: {first}");
}
