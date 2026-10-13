// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Before an object exists, an exchange gets the welcome's space without
//! changing the human's arrangement. The frame and its hit regions agree.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::{Position, Rect};

use super::{Desk, Route, draw};
use crate::composer::Composer;
use crate::model::{Committed, Kind, Presentation, UiState, demo_project};
use crate::visual::logomark::REVEAL_ENDS;
use crate::workspace::cards::review::fixture;
use crate::workspace::focus::Region;
use crate::workspace::geometry::{Arrangement, Geometry, Layout, Separator};
use crate::workspace::object::Paint;
use crate::workspace::screen;

fn original(desk: &Desk, size: (u16, u16)) -> Geometry {
    Geometry::arranged(
        Rect::new(0, 0, size.0, size.1),
        desk.pins(),
        &desk.arrangement(),
    )
    .expect("fits")
}

#[test]
fn the_first_exchange_takes_the_welcome_space_until_an_object_arrives() {
    for size in [(80, 24), (120, 40), (180, 48)] {
        for layout in [Layout::Session, Layout::Workbench] {
            let mut desk = Desk::new();
            desk.view = Some(demo_project());
            let chosen = Arrangement::of(layout)
                .with_aside_width(Some(240))
                .with_conversation_width(Some(580))
                .with_conversation_height(Some(350));
            desk.arrange(chosen);
            let mut state = UiState::new(Presentation::Workspace, false, size);
            desk.prepare_for(&state, false, false);
            let before = original(&desk, size);
            assert_eq!(desk.geometry(size), Some(before), "opening still welcomes");
            state.transcript.push(Committed::new(
                Kind::Human,
                "Read sales.csv and total the paid orders.",
            ));
            state.busy = Some("reading sales.csv".to_owned());
            desk.prepare_for(&state, false, false);
            let after = desk.geometry(size).expect("fits");
            assert_eq!(after.conversation, before.conversation.union(before.object));
            assert!(after.object.is_empty());
            assert_eq!(
                (after.header, after.aside, after.pinned),
                (before.header, before.aside, before.pinned)
            );
            let formerly_object = Position::new(before.object.x + 1, before.object.y + 1);
            assert!(after.conversation.contains(formerly_object));
            assert!(!after.object.contains(formerly_object));
            assert_eq!(after.handle(Separator::Beside), None);
            assert_eq!(after.handle(Separator::Beneath), None);
            assert_eq!(
                after.handle(Separator::Aside),
                before.handle(Separator::Aside)
            );
            assert_eq!(
                desk.route(KeyEvent::new(KeyCode::F(4), KeyModifiers::NONE), size),
                Route::Nothing
            );
            assert_eq!(desk.arrangement(), chosen);
            assert_eq!(desk.take_settled(), None);
            state.busy = None;
            desk.prepare_for(&state, false, false);
            assert_eq!(
                desk.geometry(size),
                Some(after),
                "the reply does not restore empty onboarding"
            );
            desk.proposed(Some(fixture::candidate(fixture::id(), false)));
            assert_eq!(
                desk.geometry(size),
                Some(before),
                "the first object restores the chosen shares immediately"
            );
            assert_eq!(desk.arrangement(), chosen);
            assert_eq!(desk.take_settled(), None);
        }
    }
}

#[test]
fn an_expanded_exchange_paints_the_same_cells_as_its_geometry() {
    for (size, ascii) in [((80, 24), false), ((120, 40), false), ((120, 40), true)] {
        let mut desk = Desk::new();
        desk.view = Some(demo_project());
        let mut state = UiState::new(Presentation::Workspace, false, size);
        state.ascii = ascii;
        state.transcript.push(Committed::new(
            Kind::Human,
            "Read sales.csv and total the paid orders.",
        ));
        let mut composer = Composer::new();
        composer.paste("Keep this draft");
        desk.prepare_for(&state, ascii, false);
        let geometry = desk.geometry(size).expect("fits");
        let shown = desk.screen(ascii);
        let title = screen::panel_areas(&geometry, &state, &composer, &shown)[0];
        let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).expect("terminal");
        terminal
            .draw(|frame| {
                assert!(draw(
                    frame,
                    &desk,
                    Paint {
                        ascii,
                        color: false,
                        elapsed: REVEAL_ENDS,
                        reduced_motion: true
                    },
                    &state,
                    &composer
                ));
            })
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let row = |y| {
            (0..size.0)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        };
        assert!(row(title.y).contains(&shown.thread.name));
        let words = (0..size.1).map(row).collect::<Vec<_>>().join("\n");
        assert!(
            words.contains("Read sales.csv and total the paid orders."),
            "{words}"
        );
        assert!(words.contains("Keep this draft"), "{words}");
        assert!(
            !words.contains("1 Describe"),
            "the welcome does not paint over the exchange"
        );
        assert_eq!(composer.text(), "Keep this draft");
        assert_eq!(desk.focus.region, Region::Conversation);
        assert_eq!(state.focus_scroll, 0);
        assert_eq!(desk.take_settled(), None);
    }
}

#[test]
fn explicit_navigation_can_still_visit_the_welcome_and_folded_project() {
    let size = (80, 24);
    let mut desk = Desk::new();
    let mut state = UiState::new(Presentation::Workspace, false, size);
    state
        .transcript
        .push(Committed::new(Kind::Human, "Read sales.csv"));
    desk.prepare_for(&state, false, false);
    let before = original(&desk, size);
    let expanded = desk.geometry(size).expect("fits");
    for region in [Region::Object, Region::Aside] {
        desk.focus.region = region;
        assert_eq!(desk.geometry(size), Some(before));
        assert_eq!(
            desk.route(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), size),
            Route::Repaint
        );
        assert_eq!(desk.focus.region, Region::Conversation);
        assert_eq!(desk.geometry(size), Some(expanded));
        assert_eq!(desk.take_settled(), None);
    }
}

#[test]
fn a_separator_that_disappears_cannot_change_the_kept_shares() {
    let size = (120, 40);
    let mut desk = Desk::new();
    let mut state = UiState::new(Presentation::Workspace, false, size);
    desk.prepare_for(&state, false, false);
    let before = desk.arrangement();
    let cells = desk
        .geometry(size)
        .expect("fits")
        .handle(Separator::Beside)
        .expect("separator");
    let at = Position::new(cells.x, cells.y);
    assert_eq!(desk.press(at, size), Some(Separator::Beside));
    state
        .transcript
        .push(Committed::new(Kind::Human, "Read sales.csv"));
    desk.prepare_for(&state, false, false);
    assert!(!desk.drag_to(Position::new(at.x + 10, at.y), size));
    assert_eq!(desk.dragging(), None);
    assert_eq!(desk.arrangement(), before);
    assert_eq!(desk.take_settled(), None);
}
