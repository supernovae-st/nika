// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A current decision the restored stacked split cannot show folds the
//! object to its strip: three rows (its title, one row, the continuation
//! cue) above a conversation that takes every row given up, the header, the
//! aside and the pinned row unmoved. One decision per frame, read by the
//! frame, the extent, `F4` and the object's action alike; never side by
//! side, expanded, while the aside holds the keys, for another state or
//! identity, or where the decision fits. Typing and work never move it, and
//! nothing here answers, saves or runs.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nika_session::ProposalId;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

use super::{Desk, Route, draw, layout::QUESTION_CONTEXT};
use crate::composer::Composer;
use crate::model::{
    Asked, Committed, Kind, Offer, Presentation, Shape, UiState, Waiting, demo_project,
};
use crate::render::question;
use crate::visual::logomark::REVEAL_ENDS;
use crate::workspace::cards;
use crate::workspace::cards::review::fixture;
use crate::workspace::focus::Region;
use crate::workspace::geometry::{Arrangement, Geometry, Layout};
use crate::workspace::object::{self, Paint};
use crate::workspace::screen;

/// The words left unsent in the composer.
const DRAFT: &str = "hold this decision";

/// Stacked sizes whose restored split cannot show the fixture's review whole.
const FOLDING: [(u16, u16); 3] = [(80, 24), (80, 30), (99, 30)];

fn paint(ascii: bool) -> Paint {
    Paint {
        ascii,
        color: false,
        elapsed: REVEAL_ENDS,
        reduced_motion: false,
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// A desk whose object is the fixture's candidate, reviewed.
fn reviewing() -> Desk {
    let mut desk = Desk::new();
    desk.view = Some(demo_project());
    desk.proposed(Some(fixture::candidate(fixture::id(), false)));
    desk
}

/// The fixture's conversation at `size`, waiting for consent on its tagged
/// proposal, and a composer holding the draft.
fn waiting(size: (u16, u16)) -> (UiState, Composer) {
    let proposal = Committed::proposal(fixture::id(), fixture::PREVIEW);
    let state = fixture::state(size, proposal, Waiting::Proposal);
    let mut composer = Composer::new();
    composer.paste(DRAFT);
    (state, composer)
}

/// The rows of `desk`'s frame of `state`.
fn rows(desk: &Desk, state: &UiState, composer: &Composer) -> Vec<String> {
    let (width, height) = state.size;
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| assert!(draw(frame, desk, paint(state.ascii), state, composer)))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

/// The words of `rows`, edges dropped and spaces collapsed: what a reader
/// meets, whichever row a fact wraps onto.
fn reading(rows: &[String]) -> String {
    let joined = rows.join(" ").replace(['│', '|'], " ");
    joined.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The restored frame of `desk` at `size`, no decision folding it.
fn restored(desk: &Desk, size: (u16, u16)) -> Geometry {
    let area = Rect::new(0, 0, size.0, size.1);
    Geometry::arranged(area, desk.pins(), &desk.arrangement()).expect("fits")
}

/// A fold keeps whole regions that tile the screen exactly at any origin, a
/// run pinned or not: the object keeps its three rows, the conversation under
/// it takes every row given up, nothing else moves; side by side, nothing
/// folds.
#[test]
fn a_folded_frame_tiles_the_screen_exactly() {
    for (width, height) in [(60, 16), (60, 18), (80, 24), (99, 30), (120, 40)] {
        for (x, y) in [(0, 0), (7, 3)] {
            for pinned in [false, true] {
                let at = format!("{width}x{height}@{x},{y} pinned={pinned}");
                let area = Rect::new(x, y, width, height);
                let session = Arrangement::of(Layout::Session);
                let before = Geometry::arranged(area, pinned, &session).expect("fits");
                let after = screen::folded(before);
                if !before.stacked {
                    assert_eq!(after, before, "{at}: side by side");
                    continue;
                }
                let (object, talk) = (after.object, after.conversation);
                assert_eq!(object.height, 3, "{at}");
                assert_eq!(object.y, before.object.y, "{at}");
                assert_eq!(object.bottom(), talk.y, "{at}: no gap, no overlap");
                assert_eq!(talk.bottom(), before.conversation.bottom(), "{at}");
                assert_eq!((object.x, object.width), (area.x, area.width), "{at}");
                assert_eq!((talk.x, talk.width), (area.x, area.width), "{at}");
                let kept = (before.header, before.aside, before.pinned);
                assert_eq!((after.header, after.aside, after.pinned), kept, "{at}");
                let regions = after.regions();
                let cells: u32 = (regions.iter())
                    .map(|r| u32::from(r.width) * u32::from(r.height))
                    .sum();
                assert_eq!(cells, u32::from(width) * u32::from(height), "{at}");
                for (i, a) in regions.iter().enumerate() {
                    for b in &regions[i + 1..] {
                        assert!(!a.intersects(*b), "{at}: {a:?} {b:?}");
                    }
                }
            }
        }
    }
}

/// At 80x24, 80x30 and 99x30 the review fits only folded: at rest the
/// card's title and standing, its changes and effects, its identity
/// footnote, the `Save?` line with the draft (once) and the hint all show
/// together, in both glyph columns, a run pinned or not.
#[test]
fn the_review_shows_whole_at_rest_above_the_folded_object() {
    let id = fixture::id().to_string();
    for size in FOLDING {
        for (ascii, pinned) in [(false, false), (true, false), (false, true)] {
            let at = format!("{size:?} ascii={ascii} pinned={pinned}");
            let mut desk = reviewing();
            if pinned {
                let run = crate::workspace::pinned::Pinned::new(
                    "demo",
                    "release.nika",
                    "#7",
                    nika_display::state::TaskState::Paused,
                    "waiting",
                );
                desk.view.as_mut().expect("the demo project").pinned = Some(run);
            }
            let (mut state, composer) = waiting(size);
            state.ascii = ascii;
            desk.prepare_for(&state, ascii, false);
            assert!(desk.folds(size), "{at}");
            let geometry = desk.geometry(size).expect("fits");
            assert_eq!(geometry, screen::folded(restored(&desk, size)), "{at}");
            assert_eq!(geometry.object.height, 3, "{at}");
            let shown = rows(&desk, &state, &composer);
            let read = reading(&shown);
            let sep = if ascii { "-" } else { "·" };
            for fact in [
                "Review before saving".to_owned(),
                format!("what a yes answers {sep} not saved"),
                "nothing has run on your files".to_owned(),
                "creates compiled-workflow.nika".to_owned(),
                "when it runs".to_owned(),
                format!("proposal {id} {sep} these bytes 51835c93e564"),
                "yes + Enter: Save".to_owned(),
            ] {
                assert!(read.contains(&fact), "{at}: {fact}\n{shown:#?}");
            }
            let input: Vec<&String> = shown.iter().filter(|row| row.contains(DRAFT)).collect();
            assert_eq!(input.len(), 1, "{at}: the draft once\n{shown:#?}");
            assert!(input[0].contains("Save? "), "{at}\n{shown:#?}");
            assert!(
                !read.contains("Approved") && !read.contains("Applied"),
                "{at}"
            );
        }
    }
}

/// One folded geometry: the frame paints the conversation's title on the
/// row under the strip, the transcript under it, and the extent scrolls the
/// object one row under its title, as the face was rendered for.
#[test]
fn the_frame_and_the_extent_read_the_one_folded_geometry() {
    for size in FOLDING {
        let mut desk = reviewing();
        let (state, composer) = waiting(size);
        desk.prepare_for(&state, false, false);
        let geometry = desk.geometry(size).expect("fits");
        let shown = rows(&desk, &state, &composer);
        let title = usize::from(geometry.conversation.y);
        assert!(
            shown[title].contains("this conversation"),
            "{size:?}\n{shown:#?}"
        );
        let thread = desk.screen(false).thread;
        let [heading, transcript, _, _] =
            screen::panel_areas(&geometry, &state, &composer, &thread);
        assert_eq!(heading.y, geometry.conversation.y, "{size:?}");
        assert_eq!(transcript.y, heading.bottom(), "{size:?}");
        let length = object::length(&desk.screen(false).object);
        let extent = desk.extent(size).expect("fits");
        assert_eq!(
            extent.object_rows,
            object::content_rows(length, 3),
            "{size:?}"
        );
    }
}

/// Nothing folds where the review fits (80x40), side by side, expanded,
/// while the aside holds the keys, outside the workspace, while anything
/// but consent on this candidate waits, or for an aside, draft, untagged or
/// other proposal: the restored frame stays.
#[test]
fn nothing_folds_where_the_decision_fits_or_is_not_current() {
    let unfolded = |desk: &mut Desk, state: &UiState, at: &str| {
        desk.prepare_for(state, false, false);
        assert!(!desk.folds(state.size), "{at}");
        assert_eq!(
            desk.geometry(state.size),
            Some(restored(desk, state.size)),
            "{at}"
        );
    };
    for size in [(80, 40), (120, 40), (180, 48)] {
        let (state, _) = waiting(size);
        unfolded(&mut reviewing(), &state, &format!("{size:?}"));
    }
    let size = (80, 24);
    let legacy = Waiting::Question {
        key: "const.notes".to_owned(),
    };
    for other in [Waiting::Free, Waiting::Gate, Waiting::Choosing, legacy] {
        let (mut state, _) = waiting(size);
        state.waiting = other;
        unfolded(&mut reviewing(), &state, &format!("{:?}", state.waiting));
    }
    for presentation in [Presentation::Inline, Presentation::Focus] {
        let (mut state, _) = waiting(size);
        state.presentation = presentation;
        unfolded(&mut reviewing(), &state, &format!("{presentation:?}"));
    }
    let untagged = Committed::new(Kind::Proposal, fixture::PREVIEW);
    let other = Committed::proposal(ProposalId::of("another preview"), fixture::PREVIEW);
    for (case, block) in [("untagged", untagged), ("another identity", other)] {
        let state = fixture::state(size, block, Waiting::Proposal);
        unfolded(&mut reviewing(), &state, case);
    }
    let aside = fixture::candidate(fixture::id(), true);
    let draft = fixture::candidate(fixture::id(), false).drafted();
    for (case, candidate) in [("aside", aside), ("draft", draft)] {
        let mut desk = reviewing();
        desk.proposed(Some(candidate));
        unfolded(&mut desk, &waiting(size).0, case);
    }
    let (state, _) = waiting(size);
    let mut desk = reviewing();
    assert!(desk.set_layout(Layout::Workbench));
    unfolded(&mut desk, &state, "expanded");
    let mut desk = reviewing();
    desk.prepare_for(&state, false, false);
    assert!(desk.folds(size), "the decision folds");
    // The folded aside stands over the object's restored rows.
    desk.focus.region = Region::Aside;
    assert!(!desk.folds(size), "the aside holds the keys");
    assert_eq!(desk.geometry(size), Some(restored(&desk, size)));
    desk.focus.region = Region::Conversation;
    assert!(desk.folds(size), "the keys left the aside");
}

/// Another size, layout, pin or candidate than the frame decided for reads
/// the restored frame until the next frame decides again.
#[test]
fn a_fold_holds_only_for_the_frame_it_was_decided_for() {
    let size = (80, 24);
    let mut desk = reviewing();
    let (state, _) = waiting(size);
    desk.prepare_for(&state, false, false);
    assert!(desk.folds(size));
    assert!(!desk.folds((80, 25)), "another size");
    assert!(desk.set_layout(Layout::Workbench));
    assert!(!desk.folds(size), "another layout");
    assert!(desk.set_layout(Layout::Session));
    assert!(desk.folds(size), "the same frame again");
    let run = crate::workspace::pinned::Pinned::new(
        "demo",
        "release.nika",
        "#7",
        nika_display::state::TaskState::Paused,
        "waiting",
    );
    desk.view.as_mut().expect("the demo project").pinned = Some(run);
    assert!(!desk.folds(size), "another pin");
    desk.prepare_for(&state, false, false);
    assert!(desk.folds(size), "decided again");
    desk.proposed(None);
    assert!(!desk.folds(size), "another candidate");
}

/// Typing three lines, work and the chooser's absence or presence leave the
/// fold as it was: the demand is measured at rest, never on the live rows.
#[test]
fn typing_and_work_never_move_the_fold() {
    for size in [(60, 16), (80, 24)] {
        let mut desk = reviewing();
        let (mut state, mut composer) = waiting(size);
        desk.prepare_for(&state, false, false);
        let folded = desk.geometry(size);
        assert!(desk.folds(size), "{size:?}");
        composer.paste("\nsecond line\nthird line");
        state.busy = Some("reviewing your reply".to_owned());
        desk.prepare_for(&state, false, false);
        assert_eq!(desk.geometry(size), folded, "{size:?}: busy, three lines");
        state.busy = None;
        desk.prepare_for(&state, false, false);
        assert_eq!(desk.geometry(size), folded, "{size:?}: at rest again");
        let shown = rows(&desk, &state, &composer);
        let lines = shown
            .iter()
            .filter(|row| row.contains("third line"))
            .count();
        assert_eq!(lines, 1, "{size:?}\n{shown:#?}");
    }
}

/// From the strip, the object's action and `F4` expand the object, and
/// restoring returns to the strip while the decision is current: the focus,
/// the face and the candidate stay, each change settles once, and nothing is
/// answered.
#[test]
fn f4_expands_the_strip_and_restores_to_it() {
    for size in [(60, 16), (80, 24)] {
        for ascii in [false, true] {
            let at = format!("{size:?} ascii={ascii}");
            let mut desk = reviewing();
            let (mut state, _) = waiting(size);
            state.ascii = ascii;
            desk.prepare_for(&state, ascii, false);
            let strip = desk.geometry(size).expect("fits");
            assert_eq!(strip.object.height, 3, "{at}");
            assert_eq!(desk.toggled(size), Some(Layout::Workbench), "{at}");
            let shown = desk.screen(ascii);
            let next = desk.toggled(size);
            let action =
                screen::object_action(&shown.object, &strip, desk.focus.region, next, ascii);
            let (cells, words) = action.expect("the strip offers its action");
            assert!(words.starts_with("[+]"), "{at}: {words}");
            let rows = [strip.object.y, strip.object.bottom() - 1];
            assert!(rows.contains(&cells.y), "{at}: {cells:?}");
            assert_eq!(cells.right(), strip.object.right(), "{at}");
            let before = format!("{:?}|{:?}|{:?}", desk.focus, desk.face, desk.candidate);
            assert_eq!(desk.route(key(KeyCode::F(4)), size), Route::Repaint, "{at}");
            desk.prepare_for(&state, ascii, false);
            let grown = desk.geometry(size).expect("fits");
            assert!(grown.object.height > strip.object.height, "{at}: no growth");
            assert!(!desk.folds(size), "{at}: an expansion never folds");
            assert!(desk.take_settled().is_some(), "{at}");
            assert_eq!(desk.take_settled(), None, "{at}: settled once");
            assert_eq!(desk.route(key(KeyCode::F(4)), size), Route::Repaint, "{at}");
            desk.prepare_for(&state, ascii, false);
            assert_eq!(desk.geometry(size), Some(strip), "{at}: back to the strip");
            assert!(desk.take_settled().is_some(), "{at}");
            let after = format!("{:?}|{:?}|{:?}", desk.focus, desk.face, desk.candidate);
            assert_eq!(after, before, "{at}: the view changed only");
            assert_eq!(state.waiting, Waiting::Proposal, "{at}: nothing answered");
        }
    }
}

/// A long typed question folds the object at 60x18 and 80x24: at rest its
/// first words, the line that answers it and where the whole question is
/// read show together. Where a short one folds is measured, not assumed
/// ([`a_homed_question_keeps_three_rows_of_its_exchange_in_view`]).
#[test]
fn the_live_question_keeps_its_first_words_and_the_reader_cue() {
    let witness = "3:q-currency";
    let long: Vec<String> = (1..=10).map(|n| format!("question line {n}")).collect();
    let text = format!("first words of the question\n{}", long.join("\n"));
    let offers = vec![
        Offer::new("eur", "Euro"),
        Offer::new("usd", "US dollar"),
        Offer::new("gbp", "Pound sterling"),
    ];
    let asking = |size, text: &str, shape: Shape| {
        let mut state = UiState::new(Presentation::Workspace, false, size);
        state
            .transcript
            .push(Committed::new(Kind::Human, "sum the amounts"));
        state.transcript.push(Committed::question(witness, text));
        let asked = Asked::new("the currency code", "", true, shape, witness, 3);
        state.waiting = Waiting::asked("const.currency", asked);
        state
    };
    for size in [(60, 18), (80, 24)] {
        for ascii in [false, true] {
            let at = format!("{size:?} ascii={ascii}");
            let mut state = asking(size, text.as_str(), Shape::Choice(offers.clone()));
            state.ascii = ascii;
            let mut desk = Desk::new();
            desk.view = Some(demo_project());
            desk.prepare_for(&state, ascii, false);
            assert!(desk.folds(size), "{at}");
            let shown = rows(&desk, &state, &Composer::new());
            let read = reading(&shown);
            assert!(
                read.contains("first words of the question"),
                "{at}\n{shown:#?}"
            );
            assert!(read.contains("the whole question: F2"), "{at}\n{shown:#?}");
            let prompt = if ascii { "reply >" } else { "reply ›" };
            assert!(read.contains(prompt), "{at}\n{shown:#?}");
        }
    }
}

/// The restored stacked conversation keeps three rows of the exchange that
/// led to a homed question above the question's own row at rest; short of
/// them the object folds to its strip, and folded they hold. The boundary is
/// measured from the shared rest geometry at the shortest workspace and at
/// 80x24, 80x30, 99x30 and 80x40, never pinned; both outcomes occur.
#[test]
fn a_homed_question_keeps_three_rows_of_its_exchange_in_view() {
    let witness = "3:q-currency";
    let mut outcomes = Vec::new();
    for size in [(60, 16), (60, 18), (80, 24), (80, 30), (99, 30), (80, 40)] {
        let mut state = UiState::new(Presentation::Workspace, false, size);
        state
            .transcript
            .push(Committed::new(Kind::Human, "sum the amounts"));
        state
            .transcript
            .push(Committed::question(witness, "the currency code"));
        let asked = Asked::new("the currency code", "", true, Shape::Text, witness, 3);
        state.waiting = Waiting::asked("const.currency", asked);
        let mut desk = Desk::new();
        desk.view = Some(demo_project());
        let thread = desk.screen(false).thread;
        let from = question::asked_block(&state).expect("the question's block");
        let context = (None, Some(from));
        let rest = screen::rest_transcript(&restored(&desk, size), &state, &thread);
        let need = cards::rows_from(&state, rest, context, from) + QUESTION_CONTEXT;
        let folds = need > usize::from(rest.height);
        desk.prepare_for(&state, false, false);
        let at = format!("{size:?}: {need} rows asked of {}", rest.height);
        assert_eq!(desk.folds(size), folds, "{at}");
        if folds {
            let geometry = desk.geometry(size).expect("fits");
            assert_eq!(geometry.object.height, 3, "{at}: the strip");
            let held = screen::rest_transcript(&geometry, &state, &thread);
            let kept = cards::rows_from(&state, held, context, from) + QUESTION_CONTEXT;
            assert!(kept <= usize::from(held.height), "{at}: folded they hold");
        }
        outcomes.push(folds);
    }
    let both = outcomes.contains(&true) && outcomes.contains(&false);
    assert!(both, "{outcomes:?}");
}
