// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::{Action, Missing, Selected, next};
use crate::NativeMode;

fn selected(author: bool, seat: bool, native: NativeMode) -> Selected {
    Selected {
        author,
        seat,
        native,
    }
}

const WHOLE: Missing = Missing {
    composed: true,
    choices: false,
    untyped: 0,
    settled: false,
};
const CHOICES: Missing = Missing {
    composed: false,
    choices: true,
    untyped: 0,
    settled: false,
};
const UNTYPED: Missing = Missing {
    composed: false,
    choices: false,
    untyped: 2,
    settled: false,
};
const TYPED: Missing = Missing {
    composed: false,
    choices: false,
    untyped: 0,
    settled: false,
};

#[test]
fn a_whole_reading_is_checked_whenever_a_judge_is_selected_never_ready_on_words_alone() {
    for (author, seat) in [(true, false), (false, true), (true, true)] {
        let picked = next(WHOLE, selected(author, seat, NativeMode::Escalate));
        assert_eq!(picked, Action::Check, "author {author} seat {seat}");
    }
    // No intelligence selected: the deterministic assembly, as a compile without cognition.
    assert_eq!(
        next(WHOLE, selected(false, false, NativeMode::Off)),
        Action::Assemble
    );
}

#[test]
fn open_readings_go_to_the_seat_once_then_to_a_composer() {
    let both = selected(true, true, NativeMode::Escalate);
    assert_eq!(next(CHOICES, both), Action::Settle);
    let settled = Missing {
        settled: true,
        untyped: 1,
        ..CHOICES
    };
    assert_eq!(
        next(settled, both),
        Action::Plan,
        "the seat left a clause open"
    );
    let settled_typed = Missing {
        settled: true,
        ..CHOICES
    };
    assert_eq!(next(settled_typed, both), Action::Plan);
    // No seat: the author composes at once; nothing at all: what is missing is named.
    assert_eq!(
        next(CHOICES, selected(true, false, NativeMode::Escalate)),
        Action::Plan
    );
    assert_eq!(
        next(CHOICES, selected(false, false, NativeMode::Off)),
        Action::Ask
    );
}

#[test]
fn the_author_composes_through_the_plan_and_the_sketch_policy_names_its_composer() {
    let escalate = selected(true, true, NativeMode::Escalate);
    assert_eq!(next(UNTYPED, escalate), Action::Plan);
    assert_eq!(next(TYPED, escalate), Action::Plan);
    assert_eq!(
        next(UNTYPED, selected(true, false, NativeMode::Off)),
        Action::Plan
    );
    // The sketch policy is the caller's explicit composer, even over a whole reading.
    assert_eq!(
        next(WHOLE, selected(true, false, NativeMode::Sketch)),
        Action::Sketch
    );
    // A seat without an author cannot compose.
    assert_eq!(
        next(UNTYPED, selected(false, true, NativeMode::Escalate)),
        Action::Ask
    );
}
