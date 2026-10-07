// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The passive repair report through its Display owner. Every expected text is
//! the byte-exact report the pre-move host (`fix_ladder`)
//! printed for the same rows; none is produced by the renderer under test.

use super::*;

fn refusals() -> [Vec<Refusal>; 2] {
    [
        vec![Refusal {
            attempted: vec!["w1-map `envelope` → `map`".to_owned()],
            reason: "simple key expect ':'".to_owned(),
        }],
        vec![
            Refusal {
                attempted: vec![],
                reason: "duplicate key « tâche » \"x\" \\".to_owned(),
            },
            Refusal {
                attempted: vec!["a → b".to_owned(), "c ✨ → d".to_owned()],
                reason: "line one\nline two".to_owned(),
            },
        ],
    ]
}

fn repairs() -> Vec<Repair> {
    vec![
        Repair::applied("bare exec: string", "command: argv", "bare-exec"),
        Repair {
            old: "needs: « é »".to_owned(),
            new: "after: { id: success }".to_owned(),
            kind: "needs-after",
            applied: false,
        },
        Repair::applied("tasks: list", "tasks: map keyed by task id", "w1-map"),
    ]
}

#[test]
fn the_report_keeps_the_pre_move_bytes() {
    let plain = Theme::new(false, false, false);
    let [one, two] = refusals();
    assert_eq!(render_refusals(&one, plain), REFUSALS_ONE);
    assert_eq!(render_refusals(&two, plain), REFUSALS_TWO);
    assert_eq!(
        render_refusals(&two, Theme::new(true, false, false)),
        REFUSALS_TWO_COLOR
    );
    let notes = StopNotes(vec![
        "`needs:` is foreign — rewrite it".to_owned(),
        "équipe \"x\" \\ ✨\nsecond".to_owned(),
    ]);
    assert_eq!(render_stops(&notes, plain), STOPS_TWO);
    assert_eq!(summary(&[], 0, plain), SUMMARY_NONE);
    assert_eq!(summary(&repairs()[1..2], 0, plain), SUMMARY_SKIPPED);
    assert_eq!(summary(&repairs(), 2, plain), SUMMARY_MIXED);
    assert_eq!(
        summary(&repairs(), 2, Theme::new(false, true, false)),
        SUMMARY_MIXED_ASCII
    );
}

const REFUSALS_ONE: &str = " ✗ FIX  refused — w1-map `envelope` → `map` · the repaired text does not parse (simple key expect ':') · the file is unchanged\n";

const REFUSALS_TWO: &str = " ✗ FIX  refused —  · the repaired text does not parse (duplicate key « tâche » \"x\" \\) · the file is unchanged\n ✗ FIX  refused — a → b · c ✨ → d · the repaired text does not parse (line one\nline two) · the file is unchanged\n";

const REFUSALS_TWO_COLOR: &str = " \u{1b}[31m✗\u{1b}[0m \u{1b}[1mFIX\u{1b}[0m  refused —  · the repaired text does not parse (duplicate key « tâche » \"x\" \\) · the file is unchanged\n \u{1b}[31m✗\u{1b}[0m \u{1b}[1mFIX\u{1b}[0m  refused — a → b · c ✨ → d · the repaired text does not parse (line one\nline two) · the file is unchanged\n";

const STOPS_TWO: &str =
    " ◼ STOP  `needs:` is foreign — rewrite it\n ◼ STOP  équipe \"x\" \\ ✨\nsecond\n";

const SUMMARY_NONE: &str = " ○ FIX  no machine-applicable repairs (typed rename suggestions only — structural findings stay yours)\n";

const SUMMARY_SKIPPED: &str = " ○ FIX  needs-after `needs: « é »` → `after: { id: success }` skipped — `needs: « é »` is not unique in the file (a blind splice could rewrite the wrong site)\n ○ FIX  no machine-applicable repairs (typed rename suggestions only — structural findings stay yours)\n";

const SUMMARY_MIXED: &str = " ✔ FIX  bare-exec `bare exec: string` → `command: argv`\n ○ FIX  needs-after `needs: « é »` → `after: { id: success }` skipped — `needs: « é »` is not unique in the file (a blind splice could rewrite the wrong site)\n ✔ FIX  w1-map `tasks: list` → `tasks: map keyed by task id`\n ✔ FIX  2 repairs applied · re-audit below\n";

const SUMMARY_MIXED_ASCII: &str = " ✔ FIX  bare-exec `bare exec: string` → `command: argv`\n ○ FIX  needs-after `needs: « é »` → `after: { id: success }` skipped — `needs: « é »` is not unique in the file (a blind splice could rewrite the wrong site)\n ✔ FIX  w1-map `tasks: list` → `tasks: map keyed by task id`\n ✔ FIX  2 repairs applied · re-audit below\n";
