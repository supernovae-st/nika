// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The object restored and expanded on one desk (the internal v1 `Session`
//! and `Workbench` layouts), proved on its state and on its frames: `F4`, the
//! object's own action, the separator keys and the pointer's moves change the
//! view only (no draft is sent, nothing is opened, approved or detached, no
//! reading position moves); every arrangement draws the composer, the object
//! and its chrome inside the screen, once, at the target sizes, without colour
//! and without motion. Expansion is offered only where it enlarges the
//! object; a kept expansion always restores.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nika_display::state::TaskState;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier};
use unicode_width::UnicodeWidthStr;

use super::{Desk, Route, composer_route, draw};
use crate::composer::Composer;
use crate::model::{Presentation, Script, UiState, demo_project};
use crate::session::feed::Observed;
use crate::visual::logomark::REVEAL_ENDS;
use crate::workspace::candidate::Proposed;
use crate::workspace::focus::Region;
use crate::workspace::geometry::{
    ASIDE_MIN, ASIDE_MIN_WIDTH, Arrangement, CONVERSATION_MIN_ROWS, CONVERSATION_MIN_WIDTH,
    Geometry, Layout, OBJECT_MIN_ROWS, SIDE_BY_SIDE_MIN_WIDTH, Separator,
};
use crate::workspace::inspect::Inspected;
use crate::workspace::object::Paint;
use crate::workspace::pinned::Pinned;
use crate::workspace::project::{ProjectView, Target};
use crate::workspace::screen;
use nika_display::run_story::RunFrame;

/// The minimum and the three target sizes.
const TARGETS: [(u16, u16); 4] = [(60, 16), (80, 24), (120, 40), (180, 48)];
const WIDE: (u16, u16) = (120, 40);
const LARGE: (u16, u16) = (180, 48);
const SMALL: (u16, u16) = (80, 24);
const TINY: (u16, u16) = (59, 20);
/// Narrower than side by side and tall: the expanded object stands above
/// the conversation, whose title row is the separator.
const UNDER: (u16, u16) = (99, 40);
/// Short terminals around the limits of expansion: below 100 columns the
/// conversation under the object can give rows only from 17 rows between
/// the header and the pinned row; from 100 columns it gives columns beside.
const SHORT: [(u16, u16); 15] = [
    (60, 16),
    (60, 17),
    (60, 18),
    (80, 16),
    (80, 17),
    (80, 18),
    (99, 16),
    (99, 17),
    (100, 16),
    (100, 17),
    (120, 16),
    (120, 17),
    (120, 18),
    (180, 16),
    (180, 17),
];
const DRAFT: &str = "yes, save it";

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn demo() -> Desk {
    let mut desk = Desk::new();
    desk.view = Some(demo_project());
    desk
}

/// A candidate under review, as a conversation lends it.
fn candidate() -> Proposed {
    let look = Inspected::unjudged("release.nika", "c-bytes".to_owned(), "nika: x\n".to_owned());
    Proposed::new(nika_session::ProposalId::of("C"), false, look)
}

/// A run asked and seen through its first frames, a task picked in its list,
/// a candidate proposed beside it: the object holds the keys.
fn with_run() -> Desk {
    let exec = r#""execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"}"#;
    let frame = |n: u32, kind: &str, fields: &str| {
        let line = format!(
            r#"{{"correlation":null,{exec},"fields":[{fields}],"id":{{"uuid":"01a0ef11-03a7-74fb-bba0-{n:012x}"}},"kind":"{kind}","run":null,"timestamp":{n}}}"#
        );
        Observed::Frame(RunFrame::decode(&line).expect("frame"))
    };
    let mut seen = vec![
        Observed::Asked {
            workflow: "long.nika".to_owned(),
            resume: false,
            typed: true,
            look: None,
        },
        frame(1, "workflow_started", ""),
    ];
    for n in 0..12 {
        let task = format!(r#"{{"key":"task","value":"step_{n:02}"}}"#);
        seen.push(frame(2 + n, "task_scheduled", &task));
    }
    let mut desk = demo();
    desk.observe(seen.into_iter());
    desk.focus.region = Region::Object;
    assert_eq!(desk.route(key(KeyCode::Down), WIDE), Route::Repaint);
    desk.proposed(Some(candidate()));
    desk.opened = Some(crate::workspace::project::Target::Live);
    desk
}

/// Everything a view change must leave as it was: the project, what is
/// opened and its faces, the candidate, every run leg, the task picked, the
/// focus with its selection and object scroll, the looks asked.
fn kept(desk: &Desk) -> String {
    format!(
        "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
        desk.view,
        desk.focus,
        desk.opened,
        desk.look,
        desk.face,
        desk.wants_look,
        desk.candidate,
        desk.live,
        desk.past,
        desk.run_face,
        desk.pick,
    )
}

/// The cells `separator` sizes on a frame of `size`.
fn cells(desk: &Desk, separator: Separator, size: (u16, u16)) -> u16 {
    desk.geometry(size).expect("fits").extent_of(separator)
}

/// A listed workflow opened as the object in view, with a run pinned or not.
fn opened(pinned: bool) -> Desk {
    let mut desk = demo();
    desk.opened = Some(Target::Workflow("release.nika".to_owned()));
    if pinned {
        let run = Pinned::new("demo", "release.nika", "#7", TaskState::Paused, "waiting");
        desk.view.as_mut().expect("the demo project").pinned = Some(run);
    }
    desk
}

/// Nothing opened: the welcome is the object, with a project lent or none,
/// with a run leg pinned (observed, never opened) or none.
fn welcoming(project: bool, pinned: bool) -> Desk {
    let mut desk = if project { demo() } else { Desk::new() };
    if pinned {
        desk.observe(std::iter::once(Observed::Asked {
            workflow: "long.nika".to_owned(),
            resume: false,
            typed: true,
            look: None,
        }));
        desk.opened = None;
    }
    desk
}

/// Whether expanding `desk`'s object on a frame of `size`, its shares as
/// chosen, gives it strictly more cells than restored: the only expansion
/// the object's action and `F4` may offer.
fn grows(desk: &Desk, size: (u16, u16)) -> bool {
    let area = Rect::new(0, 0, size.0, size.1);
    let object = |layout| {
        let arrangement = desk.arrangement().with_layout(layout);
        Geometry::arranged(area, desk.pins(), &arrangement).map(|g| area_of(g.object))
    };
    match (object(Layout::Session), object(Layout::Workbench)) {
        (Some(restored), Some(expanded)) => expanded > restored,
        _ => false,
    }
}

/// The text of the cells `from..to` of row `y`, a wide glyph once: its
/// width is the number of cells.
fn row_text(buffer: &Buffer, y: u16, from: u16, to: u16) -> String {
    let mut text = String::new();
    let mut x = from;
    while x < to {
        let symbol = buffer[(x, y)].symbol();
        text.push_str(symbol);
        x += u16::try_from(symbol.width().max(1)).expect("a cell's width");
    }
    text
}

/// `F4` shows the other layout from every region, with any modifier, and
/// nothing else changes; where no layout is drawn it changes nothing at all.
#[test]
fn f4_shows_the_other_layout_from_every_region_and_changes_nothing_else() {
    for region in [Region::Conversation, Region::Aside, Region::Object] {
        let mut desk = with_run();
        desk.focus.region = region;
        desk.focus.selected = 1;
        desk.focus.scroll = 2;
        let before = kept(&desk);
        assert_eq!(
            desk.route(key(KeyCode::F(4)), WIDE),
            Route::Repaint,
            "{region:?}"
        );
        assert_eq!(desk.arrangement().layout, Layout::Workbench);
        assert_eq!(
            kept(&desk),
            before,
            "{region:?}: a layout switch is a view change"
        );
        let shifted = KeyEvent::new(KeyCode::F(4), KeyModifiers::SHIFT);
        assert_eq!(desk.route(shifted, WIDE), Route::Repaint);
        assert_eq!(desk.arrangement().layout, Layout::Session);
        assert_eq!(kept(&desk), before);
    }
    let mut desk = demo();
    assert_eq!(desk.route(key(KeyCode::F(4)), TINY), Route::Nothing);
    assert_eq!(
        desk.arrangement().layout,
        Layout::Session,
        "no layout drawn, none switched"
    );
    assert_eq!(composer_route(key(KeyCode::F(4))), Route::Nothing);
    assert_eq!(desk.take_settled(), None);
}

/// `+` and `-` move the separator of the region holding the keys by two
/// columns (or one row) within its bounds; `0` gives back the automatic
/// share; the composer's region types them; a narrow Session has none.
#[test]
fn separator_keys_move_the_focused_regions_separator_within_bounds() {
    let mut desk = demo();
    desk.focus.region = Region::Aside;
    let start = cells(&desk, Separator::Aside, WIDE);
    assert_eq!(desk.route(key(KeyCode::Char('+')), WIDE), Route::Repaint);
    assert_eq!(cells(&desk, Separator::Aside, WIDE), start + 2);
    assert_eq!(desk.route(key(KeyCode::Char('-')), WIDE), Route::Repaint);
    assert_eq!(cells(&desk, Separator::Aside, WIDE), start);
    for _ in 0..20 {
        desk.route(key(KeyCode::Char('_')), WIDE);
    }
    assert_eq!(cells(&desk, Separator::Aside, WIDE), ASIDE_MIN);
    assert_eq!(
        desk.route(key(KeyCode::Char('-')), WIDE),
        Route::Nothing,
        "a bound holds"
    );
    assert_eq!(desk.route(key(KeyCode::Char('0')), WIDE), Route::Repaint);
    assert_eq!(
        cells(&desk, Separator::Aside, WIDE),
        start,
        "automatic again"
    );
    assert_eq!(desk.route(key(KeyCode::Char('0')), WIDE), Route::Nothing);
    // The object grows as the conversation beside it, then under it, yields.
    desk.focus.region = Region::Object;
    let talk = cells(&desk, Separator::Beside, WIDE);
    assert_eq!(desk.route(key(KeyCode::Char('=')), WIDE), Route::Repaint);
    assert_eq!(cells(&desk, Separator::Beside, WIDE), talk - 2);
    assert_eq!(desk.route(key(KeyCode::F(4)), WIDE), Route::Repaint);
    // Beside the expanded object the conversation is at its narrowest: `+`
    // and `0` move nothing and keep the expansion; `-` gives the
    // conversation a step back, restoring the object at that width.
    let expanded = desk.arrangement();
    for c in ['+', '=', '0'] {
        assert_eq!(
            desk.route(key(KeyCode::Char(c)), WIDE),
            Route::Nothing,
            "{c}"
        );
    }
    assert_eq!(desk.arrangement(), expanded);
    assert_eq!(desk.route(key(KeyCode::Char('-')), WIDE), Route::Repaint);
    assert_eq!(desk.arrangement().layout, Layout::Session);
    assert_eq!(desk.arrangement().aside_width, expanded.aside_width);
    assert_eq!(
        cells(&desk, Separator::Beside, WIDE),
        CONVERSATION_MIN_WIDTH + 2
    );
    // Under the expanded object (a narrower terminal) its rows move.
    assert_eq!(desk.route(key(KeyCode::F(4)), UNDER), Route::Repaint);
    let rows = cells(&desk, Separator::Beneath, UNDER);
    assert_eq!(desk.route(key(KeyCode::Char('+')), UNDER), Route::Repaint);
    assert_eq!(cells(&desk, Separator::Beneath, UNDER), rows - 1);
    for _ in 0..40 {
        desk.route(key(KeyCode::Char('-')), UNDER);
    }
    let geometry = desk.geometry(UNDER).expect("fits");
    assert_eq!(
        geometry.object.height, OBJECT_MIN_ROWS,
        "the object keeps its rows"
    );
    let before = desk.arrangement();
    let ctrl = KeyEvent::new(KeyCode::Char('+'), KeyModifiers::CONTROL);
    assert_eq!(
        desk.route(ctrl, UNDER),
        Route::Nothing,
        "a modified key moves nothing"
    );
    assert_eq!(desk.arrangement(), before);
    desk.focus.region = Region::Conversation;
    for c in ['+', '-', '0', '=', '_'] {
        assert_eq!(
            desk.route(key(KeyCode::Char(c)), WIDE),
            Route::Compose,
            "{c}"
        );
    }
    assert_eq!(desk.arrangement(), before, "typed text moves no separator");
    let mut narrow = demo();
    narrow.focus.region = Region::Object;
    assert_eq!(narrow.route(key(KeyCode::Char('+')), SMALL), Route::Nothing);
    assert_eq!(narrow.arrangement(), Arrangement::of(Layout::Session));
}

/// The pointer holds a separator from its press to its release: the region
/// follows the motion within its bounds, the keys stay where they were, and
/// the arrangement settles (to be kept) only once the button is up.
#[test]
fn a_drag_follows_the_pointer_within_bounds_and_settles_on_release() {
    let mut desk = demo();
    desk.focus.region = Region::Object;
    let geometry = desk.geometry(WIDE).expect("fits");
    let rule = geometry.handle(Separator::Beside).expect("beside");
    let from = Position::new(rule.x + 1, rule.y + 4);
    assert_eq!(desk.press(from, WIDE), Some(Separator::Beside));
    assert_eq!(desk.dragging(), Some(Separator::Beside));
    assert!(desk.drag_to(Position::new(from.x + 9, from.y + 7), WIDE));
    let moved = desk.geometry(WIDE).expect("fits");
    assert_eq!(moved.conversation.width, geometry.conversation.width + 9);
    assert_eq!(
        moved.object.right(),
        geometry.object.right(),
        "the object yields"
    );
    assert_eq!(
        desk.take_settled(),
        None,
        "nothing is kept while the button is held"
    );
    assert!(
        !desk.drag_to(Position::new(from.x + 9, from.y), WIDE),
        "a row is no column"
    );
    assert!(desk.drag_to(Position::new(0, from.y), WIDE));
    assert_eq!(
        cells(&desk, Separator::Beside, WIDE),
        CONVERSATION_MIN_WIDTH
    );
    assert!(desk.release());
    assert_eq!(
        desk.focus.region,
        Region::Object,
        "a separator never takes the keys"
    );
    assert_eq!(
        desk.take_settled(),
        Some(desk.arrangement()),
        "settled once released"
    );
    assert_eq!(desk.take_settled(), None, "kept once");
    assert!(!desk.release());
    assert!(
        !desk.drag_to(Position::new(80, 10), WIDE),
        "no button, no move"
    );
    assert_eq!(desk.press(Position::new(rule.x - 3, rule.y), WIDE), None);
    assert_eq!(desk.dragging(), None);
    // The rule under the expanded object (a narrower terminal): up gives the
    // conversation rows, never past the object's minimum; down never takes
    // the composer's rows.
    desk.set_layout(Layout::Workbench);
    let bench = desk.geometry(UNDER).expect("fits");
    let rule = bench.handle(Separator::Beneath).expect("beneath");
    let from = Position::new(rule.x + 10, rule.y);
    assert_eq!(desk.press(from, UNDER), Some(Separator::Beneath));
    assert!(desk.drag_to(Position::new(from.x, from.y - 5), UNDER));
    assert_eq!(
        cells(&desk, Separator::Beneath, UNDER),
        bench.conversation.height + 5
    );
    assert!(desk.drag_to(Position::new(from.x, 0), UNDER));
    assert_eq!(
        desk.geometry(UNDER).expect("fits").object.height,
        OBJECT_MIN_ROWS
    );
    assert!(desk.drag_to(Position::new(from.x, 39), UNDER));
    assert_eq!(
        cells(&desk, Separator::Beneath, UNDER),
        CONVERSATION_MIN_ROWS
    );
    // Restoring the object lets the separator go.
    desk.set_layout(Layout::Session);
    assert_eq!(desk.dragging(), None);
}

/// Each arrangement keeps its own shares and both keep the aside: expanding,
/// restoring and resizing never rewrite a share, so the same size shows the
/// same cells. Beside the expanded object the conversation is already at
/// its narrowest; under it (a narrower terminal) its rows move and are kept.
#[test]
fn each_layout_keeps_its_shares_across_switches_and_resizes() {
    let mut desk = demo();
    desk.focus.region = Region::Aside;
    for _ in 0..3 {
        desk.route(key(KeyCode::Char('+')), LARGE);
    }
    desk.focus.region = Region::Object;
    for _ in 0..4 {
        desk.route(key(KeyCode::Char('+')), LARGE);
    }
    let session = desk.geometry(LARGE).expect("fits");
    assert_eq!(desk.route(key(KeyCode::F(4)), LARGE), Route::Repaint);
    let bench = desk.geometry(LARGE).expect("fits");
    for c in ['+', '0'] {
        assert_eq!(
            desk.route(key(KeyCode::Char(c)), LARGE),
            Route::Nothing,
            "{c}: beside the expanded object, already at its narrowest"
        );
    }
    let rows = cells(&desk, Separator::Beneath, SMALL);
    for _ in 0..3 {
        desk.route(key(KeyCode::Char('-')), SMALL);
    }
    let under = desk.geometry(SMALL).expect("fits");
    assert_eq!(under.extent_of(Separator::Beneath), rows + 3);
    assert_eq!(bench.aside, session.aside, "the project column is shared");
    for size in [SMALL, (60, 16), WIDE, (100, 32)] {
        let geometry = desk.geometry(size).expect("fits");
        assert!(
            geometry.conversation.height >= CONVERSATION_MIN_ROWS,
            "{size:?}"
        );
    }
    assert_eq!(desk.geometry(LARGE), Some(bench));
    assert_eq!(desk.geometry(SMALL), Some(under));
    assert_eq!(desk.route(key(KeyCode::F(4)), LARGE), Route::Repaint);
    assert_eq!(
        desk.geometry(LARGE),
        Some(session),
        "Session returns exactly"
    );
}

/// A kept arrangement is shown as kept (bounded on the way in) and is not
/// handed back to be kept again; the next change is.
#[test]
fn arrange_shows_a_kept_arrangement_without_asking_to_keep_it_again() {
    let mut desk = demo();
    let mut kept = Arrangement::of(Layout::Workbench).with_conversation_height(Some(400));
    kept.aside_width = Some(5_000);
    desk.arrange(kept);
    assert_eq!(desk.arrangement().layout, Layout::Workbench);
    assert_eq!(
        desk.arrangement().aside_width,
        Some(1000),
        "bounded on the way in"
    );
    assert_eq!(desk.take_settled(), None);
    desk.route(key(KeyCode::F(4)), WIDE);
    assert_eq!(desk.take_settled().map(|a| a.layout), Some(Layout::Session));
    assert_eq!(desk.take_settled(), None);
    assert!(!desk.set_layout(Layout::Session), "the layout in view");
    assert_eq!(desk.take_settled(), None);
}

/// Every view change, at every size, in every region: the project, the
/// opened object and its faces, the candidate, the run leg and its picked
/// task, the selection and the object scroll stay exactly as they were.
#[test]
fn view_changes_never_touch_the_run_the_candidate_or_the_selection() {
    for size in [SMALL, WIDE, LARGE] {
        for region in [Region::Conversation, Region::Aside, Region::Object] {
            let mut desk = with_run();
            desk.focus.region = region;
            let before = kept(&desk);
            for code in [
                KeyCode::F(4),
                KeyCode::Char('+'),
                KeyCode::Char('-'),
                KeyCode::F(4),
                KeyCode::Char('0'),
            ] {
                let route = desk.route(key(code), size);
                if region == Region::Conversation && code != KeyCode::F(4) {
                    assert_eq!(route, Route::Compose, "the composer types {code:?}");
                } else {
                    assert!(
                        matches!(route, Route::Repaint | Route::Nothing),
                        "{route:?}"
                    );
                }
            }
            for layout in Layout::ALL {
                desk.set_layout(layout);
                for separator in Separator::ALL {
                    // Each move shifts the separators after it: read them anew.
                    let geometry = desk.geometry(size).expect("fits");
                    let Some(handle) = geometry.handle(separator) else {
                        continue;
                    };
                    let from = Position::new(handle.x, handle.y);
                    assert_eq!(desk.press(from, size), Some(separator));
                    desk.drag_to(Position::new(from.x + 5, from.y + 2), size);
                    desk.drag_to(Position::new(from.x.saturating_sub(7), 0), size);
                    assert!(desk.release());
                }
            }
            assert_eq!(kept(&desk), before, "{size:?} {region:?}");
        }
    }
}

fn paint(ascii: bool, color: bool) -> Paint {
    Paint {
        ascii,
        color,
        elapsed: REVEAL_ENDS,
        reduced_motion: false,
    }
}

/// A workspace that asked its first question, with a draft in the composer.
fn talking(size: (u16, u16), ascii: bool) -> (UiState, Composer) {
    let mut state = UiState::new(Presentation::Workspace, false, size);
    state.ascii = ascii;
    let mut script = Script::demo();
    for beat in script.open() {
        state.apply(beat);
    }
    for beat in script.submit("digest my notes") {
        state.apply(beat);
    }
    let mut composer = Composer::new();
    composer.paste(DRAFT);
    (state, composer)
}

/// One frame of `desk`: its rows and its buffer.
fn frame(
    desk: &Desk,
    size: (u16, u16),
    state: &UiState,
    composer: &Composer,
    paint: Paint,
) -> (Vec<String>, Buffer) {
    let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).expect("terminal");
    terminal
        .draw(|f| assert!(draw(f, desk, paint, state, composer), "{size:?} fits"))
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    let rows = (0..size.1)
        .map(|y| (0..size.0).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    (rows, buffer)
}

/// Both the restored and the expanded object at the minimum and the target
/// sizes, in both glyph columns: the composer keeps the draft on screen, the
/// header names the project, the run stays pinned, and the object's action
/// takes the form that fits: `[+]` only where expanding enlarges the object
/// (elsewhere `F4` keeps the arrangement), `[-]` on every kept expansion; no
/// frame names a mode.
#[test]
fn every_target_keeps_the_draft_the_object_and_its_action() {
    for size in TARGETS {
        for ascii in [false, true] {
            let mut desk = with_run();
            let (state, composer) = talking(size, ascii);
            for expanded in [false, true] {
                let at = format!("{size:?} ascii={ascii} expanded={expanded}");
                desk.prepare(size, ascii, false);
                let geometry = desk.geometry(size).expect("fits");
                let (rows, buffer) = frame(&desk, size, &state, &composer, paint(ascii, false));
                let text = rows.join("\n");
                assert!(text.contains(DRAFT), "{at}: the draft left\n{text}");
                assert!(rows[0].contains("demo"), "{at}: {}", rows[0]);
                assert!(!text.contains("Workbench"), "{at}\n{text}");
                assert!(rows.last().expect("rows").contains("long.nika"), "{at}");
                if ascii {
                    assert!(rows[0].is_ascii(), "{at}: {}", rows[0]);
                }
                let title = title_cells(&desk, size, ascii);
                let body = screen::object_body(&geometry).width;
                let fitting = fitting_action(title, body, expanded, ascii);
                let offered = expanded || grows(&desk, size);
                let shown = painted_action(&buffer, geometry.object, expanded, ascii);
                assert_eq!(shown, fitting.filter(|_| offered), "{at}\n{text}");
                let route = desk.route(key(KeyCode::F(4)), size);
                if offered {
                    assert_eq!(route, Route::Repaint, "{at}");
                } else {
                    // No room to grow: F4 keeps the arrangement. A kept
                    // expansion (an earlier size, a kept preference) is shown next.
                    assert_eq!(route, Route::Nothing, "{at}");
                    desk.arrange(desk.arrangement().with_layout(Layout::Workbench));
                }
            }
        }
    }
}

/// Beside the expanded object its separator stays operable: dragged toward
/// a narrower conversation it moves nothing and keeps the expansion; dragged
/// toward a wider one it restores the object at the width the pointer
/// leaves, the keys where they were, settled once released. Expanded again,
/// the object restores to that width, not the earlier one.
#[test]
fn the_separator_beside_the_expanded_object_restores_it_where_it_is_dragged() {
    let mut desk = demo();
    desk.focus.region = Region::Object;
    let restored = desk.geometry(WIDE).expect("fits");
    assert_eq!(desk.route(key(KeyCode::F(4)), WIDE), Route::Repaint);
    assert!(desk.take_settled().is_some());
    let expanded = desk.arrangement();
    let beside = desk.geometry(WIDE).expect("fits");
    let rule = beside
        .handle(Separator::Beside)
        .expect("an operable separator");
    let from = Position::new(rule.x + 1, rule.y + 4);
    assert_eq!(desk.press(from, WIDE), Some(Separator::Beside));
    assert!(
        !desk.drag_to(Position::new(from.x - 5, from.y), WIDE),
        "no narrower than the narrowest"
    );
    assert_eq!(desk.arrangement(), expanded);
    assert!(desk.drag_to(Position::new(from.x + 6, from.y), WIDE));
    assert_eq!(desk.arrangement().layout, Layout::Session);
    assert_eq!(
        cells(&desk, Separator::Beside, WIDE),
        CONVERSATION_MIN_WIDTH + 6
    );
    assert_eq!(desk.take_settled(), None, "nothing is kept while held");
    assert!(desk.release());
    assert_eq!(desk.focus.region, Region::Object, "the keys stay");
    assert_eq!(desk.take_settled(), Some(desk.arrangement()));
    assert_eq!(desk.route(key(KeyCode::F(4)), WIDE), Route::Repaint);
    assert_eq!(
        cells(&desk, Separator::Beside, WIDE),
        CONVERSATION_MIN_WIDTH
    );
    assert_eq!(desk.route(key(KeyCode::F(4)), WIDE), Route::Repaint);
    assert_eq!(
        cells(&desk, Separator::Beside, WIDE),
        CONVERSATION_MIN_WIDTH + 6
    );
    assert_ne!(
        desk.geometry(WIDE),
        Some(restored),
        "the width the pointer chose"
    );
}

/// The separator the pointer holds is drawn reversed until it is let go:
/// beside the restored object and the expanded one, under the expanded
/// one, at the aside's edge.
#[test]
fn a_held_separator_is_reversed_until_it_is_let_go() {
    for (layout, size, separator) in [
        (Layout::Session, WIDE, Separator::Beside),
        (Layout::Workbench, WIDE, Separator::Beside),
        (Layout::Workbench, UNDER, Separator::Beneath),
        (Layout::Workbench, WIDE, Separator::Aside),
    ] {
        let mut desk = demo();
        desk.set_layout(layout);
        let (state, composer) = talking(size, false);
        let geometry = desk.geometry(size).expect("fits");
        let handle = geometry.handle(separator).expect("shown");
        desk.press(Position::new(handle.x, handle.y), size);
        let (_, held) = frame(&desk, size, &state, &composer, paint(false, false));
        let reversed = |buffer: &Buffer| {
            (handle.y..handle.bottom()).all(|y| {
                (handle.x..handle.right())
                    .all(|x| buffer[(x, y)].modifier.contains(Modifier::REVERSED))
            })
        };
        assert!(reversed(&held), "{layout:?} {separator:?}");
        desk.release();
        let (_, free) = frame(&desk, size, &state, &composer, paint(false, false));
        assert!(!reversed(&free), "{layout:?} {separator:?}");
    }
}

/// A transcript scrolled back shows one marker back to its latest row, at
/// its last row's end; at the latest row there is none. The scroll is the
/// reader's: a layout switch neither resets nor moves it.
#[test]
fn a_scrolled_transcript_shows_one_way_back_in_both_layouts() {
    for layout in Layout::ALL {
        for ascii in [false, true] {
            let mut desk = demo();
            desk.set_layout(layout);
            let (mut state, composer) = talking(LARGE, ascii);
            state.transcript.push(crate::model::Committed::new(
                crate::model::Kind::Reply,
                (0..60)
                    .map(|n| format!("reply line {n:02}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ));
            let marker = if ascii { " v latest " } else { " ↓ latest " };
            let (rows, _) = frame(&desk, LARGE, &state, &composer, paint(ascii, false));
            assert!(
                !rows.join("\n").contains(marker),
                "{layout:?}: none at the latest row"
            );
            state.focus_scroll = 4;
            let geometry = desk.geometry(LARGE).expect("fits");
            let thread = desk.screen(ascii).thread;
            let transcript = screen::panel_areas(&geometry, &state, &composer, &thread)[1];
            let area = screen::latest_area(transcript, ascii).expect("room for the marker");
            let (rows, buffer) = frame(&desk, LARGE, &state, &composer, paint(ascii, false));
            assert_eq!(rows.join("\n").matches(marker).count(), 1, "{layout:?}");
            assert!(
                rows.join("\n").matches("End: latest").count() <= 1,
                "{layout:?}: one keyboard cue beside the clickable marker\n{}",
                rows.join("\n")
            );
            let shown: String = (area.x..area.right())
                .map(|x| buffer[(x, area.y)].symbol())
                .collect();
            assert_eq!(shown, marker);
            assert!(
                buffer[(area.x, area.y)]
                    .modifier
                    .contains(Modifier::REVERSED)
            );
            assert_eq!(desk.route(key(KeyCode::F(4)), LARGE), Route::Repaint);
            assert_eq!(state.focus_scroll, 4, "the reading position stays");
        }
    }
}

/// Reduced motion draws both layouts final at once: the welcome mark at its
/// first frame is the same as at the end of its reveal.
#[test]
fn reduced_motion_draws_both_layouts_final_at_once() {
    for layout in Layout::ALL {
        let mut desk = demo();
        desk.set_layout(layout);
        let (state, composer) = talking(WIDE, false);
        let still = Paint {
            elapsed: std::time::Duration::ZERO,
            reduced_motion: true,
            ..paint(false, false)
        };
        let (first, _) = frame(&desk, WIDE, &state, &composer, still);
        let (last, _) = frame(&desk, WIDE, &state, &composer, paint(false, false));
        assert_eq!(first, last, "{layout:?}");
    }
}

/// The words of the object's own action, longest first, in the glyph
/// column: `[+]` expands the object, `[-]` restores it, and the compact form
/// keeps only the key.
fn action_forms(expanded: bool, ascii: bool) -> [&'static str; 2] {
    match (expanded, ascii) {
        (false, false) => ["[+] Expand · F4", "[+] F4"],
        (false, true) => ["[+] Expand - F4", "[+] F4"],
        (true, false) => ["[-] Restore · F4", "[-] F4"],
        (true, true) => ["[-] Restore - F4", "[-] F4"],
    }
}

/// The form of the action painted at the right end of the object's title
/// row, if one is.
fn painted_action(
    buffer: &Buffer,
    object: Rect,
    expanded: bool,
    ascii: bool,
) -> Option<&'static str> {
    action_forms(expanded, ascii).into_iter().find(|form| {
        let cells = u16::try_from(form.chars().count()).expect("cells");
        let start = object.right().saturating_sub(cells);
        let shown: String = (start..object.right())
            .map(|x| buffer[(x, object.y)].symbol())
            .collect();
        cells <= object.width && shown == *form
    })
}

/// The cells the object's title takes on `desk`'s frame of `size`, as the
/// object region paints it (cut at its body's edge).
fn title_cells(desk: &Desk, size: (u16, u16), ascii: bool) -> usize {
    let width = screen::object_body(&desk.geometry(size).expect("fits")).width;
    let object = desk.screen(ascii).object;
    let title = crate::workspace::object::lines(&object, width, 1, paint(ascii, false));
    title
        .first()
        .map_or(0, ratatui::text::Line::width)
        .min(usize::from(width))
}

/// The action the object offers after a title `title` cells wide in a
/// region `width` cells wide: the longest form leaving one blank cell.
fn fitting_action(title: usize, width: u16, expanded: bool, ascii: bool) -> Option<&'static str> {
    action_forms(expanded, ascii)
        .into_iter()
        .find(|form| title + 1 + form.chars().count() <= usize::from(width))
}

/// A region's cells.
fn area_of(rect: Rect) -> u32 {
    u32::from(rect.width) * u32::from(rect.height)
}

/// The object's own title row offers one action, never a mode: the longest
/// form that fits after the title, none where none fits (`F4` still acts).
/// `F4` grows the object while the conversation keeps its composer and the
/// draft, then gives back exactly the frame before; each press changes the
/// view only. No frame names Session or Workbench, and the action reads
/// without colour, in both glyph columns.
#[test]
fn one_object_action_expands_and_restores_the_object_at_the_target_sizes() {
    let mut offered = Vec::new();
    for size in [SMALL, WIDE, LARGE] {
        for ascii in [false, true] {
            let mut desk = with_run();
            let (state, composer) = talking(size, ascii);
            let restored = desk.geometry(size).expect("fits");
            for expanded in [false, true] {
                let at = format!("{size:?} ascii={ascii} expanded={expanded}");
                desk.prepare(size, ascii, false);
                let geometry = desk.geometry(size).expect("fits");
                let (rows, buffer) = frame(&desk, size, &state, &composer, paint(ascii, false));
                let text = rows.join("\n");
                assert!(
                    !text.contains("Workbench") && !text.contains("[Session]"),
                    "{at}: a mode is named\n{text}"
                );
                let title = title_cells(&desk, size, ascii);
                let body = screen::object_body(&geometry).width;
                let fitting = fitting_action(title, body, expanded, ascii);
                let shown = painted_action(&buffer, geometry.object, expanded, ascii);
                assert_eq!(shown, fitting, "{at}\n{text}");
                offered.push(shown.is_some());
                let object = geometry.object;
                if let Some(form) = shown {
                    let cells = u16::try_from(form.chars().count()).expect("cells");
                    let cell = &buffer[(object.right() - cells, object.y)];
                    assert_eq!((cell.fg, cell.bg), (Color::Reset, Color::Reset), "{at}");
                    assert!(
                        cell.modifier.contains(Modifier::DIM),
                        "{at}: secondary action is not quiet"
                    );
                }
                let draft = rows.iter().position(|row| row.contains(DRAFT));
                let draft = u16::try_from(draft.expect("the draft on screen")).expect("row");
                let talk = geometry.conversation;
                assert!(
                    (talk.y..talk.bottom()).contains(&draft),
                    "{at}: the draft left the conversation\n{text}"
                );
                assert!(rows[0].contains("demo"), "{at}: {}", rows[0]);
                assert!(rows.last().expect("rows").contains("long.nika"), "{at}");
                if ascii {
                    let object_row: String = (object.x..object.right())
                        .map(|x| buffer[(x, object.y)].symbol())
                        .collect();
                    assert!(
                        rows[0].is_ascii() && object_row.is_ascii(),
                        "{at}: {object_row}"
                    );
                }
                if expanded {
                    assert!(
                        area_of(object) > area_of(restored.object),
                        "{at}: no growth"
                    );
                    assert!(talk.height >= CONVERSATION_MIN_ROWS, "{at}");
                    assert_eq!(geometry.aside, restored.aside, "{at}: the project moved");
                    if size.0 >= SIDE_BY_SIDE_MIN_WIDTH {
                        // The conversation stays beside the wider object, every row kept.
                        assert!(!geometry.stacked, "{at}: the conversation moved under");
                        assert_eq!(talk.x, restored.conversation.x, "{at}");
                        assert_eq!(
                            (talk.y, talk.height),
                            (restored.conversation.y, restored.conversation.height),
                            "{at}"
                        );
                        assert!(object.width > restored.object.width, "{at}");
                    }
                } else {
                    assert_eq!(geometry, restored, "{at}: not the restored frame");
                }
                let before = kept(&desk);
                assert_eq!(desk.route(key(KeyCode::F(4)), size), Route::Repaint, "{at}");
                assert_eq!(kept(&desk), before, "{at}: F4 changes the view only");
            }
            assert_eq!(
                desk.geometry(size),
                Some(restored),
                "{size:?}: not restored"
            );
        }
    }
    assert!(offered.contains(&true), "no action was drawn: {offered:?}");
    assert!(
        offered.contains(&false),
        "no action was omitted: {offered:?}"
    );
}

/// The action is the longest form that fits after the title with one blank
/// cell between them: the compact form exactly where the long one cannot
/// fit, and no action (nothing to press) one cell narrower. No form ever
/// covers a cell of the title.
#[test]
fn the_object_action_takes_the_longest_form_that_fits_after_its_title() {
    let area = Rect::new(0, 0, WIDE.0, WIDE.1);
    for ascii in [false, true] {
        let mut desk = with_run();
        desk.prepare(WIDE, ascii, false);
        let title = title_cells(&desk, WIDE, ascii);
        let [_, compact] = action_forms(false, ascii);
        let (state, composer) = talking(WIDE, ascii);
        let start = desk.geometry(WIDE).expect("fits");
        let work = start.object.right() - start.conversation.x;
        // The air after the separator is the object's, not its body's.
        let air = start.object.width - screen::object_body(&start).width;
        let fits = title + 1 + compact.chars().count();
        for (width, expected) in [(fits, Some(compact)), (fits - 1, None)] {
            let talk = work - air - u16::try_from(width).expect("cells");
            let moved = desk
                .arrangement()
                .moved(Separator::Beside, talk, area, desk.pins());
            desk.arrange(moved);
            desk.prepare(WIDE, ascii, false);
            let geometry = desk.geometry(WIDE).expect("fits");
            let at = format!("ascii={ascii} width={width}");
            let body = screen::object_body(&geometry);
            assert_eq!(usize::from(body.width), width, "{at}");
            assert_eq!(
                title_cells(&desk, WIDE, ascii),
                title,
                "{at}: the title moved"
            );
            let (_, buffer) = frame(&desk, WIDE, &state, &composer, paint(ascii, false));
            let shown = painted_action(&buffer, geometry.object, false, ascii);
            assert_eq!(shown, expected, "{at}");
            let gap = body.x + u16::try_from(title).expect("cells");
            let blank = buffer[(gap, body.y)].symbol();
            assert_eq!(blank, " ", "{at}: the title is covered");
        }
    }
}

/// While the folded project list stands over the object, the object's
/// action is neither drawn nor offered; it returns with the object.
#[test]
fn the_folded_project_list_withdraws_the_object_action() {
    let (state, composer) = talking(SMALL, false);
    let mut desk = with_run();
    desk.prepare(SMALL, false, false);
    let geometry = desk.geometry(SMALL).expect("fits");
    assert_eq!(geometry.aside, None, "folded at 80 columns");
    for (region, offered) in [
        (Region::Object, true),
        (Region::Aside, false),
        (Region::Conversation, true),
    ] {
        desk.focus.region = region;
        let (rows, buffer) = frame(&desk, SMALL, &state, &composer, paint(false, false));
        assert_eq!(
            painted_action(&buffer, geometry.object, false, false).is_some(),
            offered,
            "{region:?}\n{}",
            rows.join("\n")
        );
    }
}

/// The welcome has no title: wherever the object offers its action, the
/// welcome gives it its first row. Nothing of the welcome is drawn on that
/// row, the action stands at its right end in its longest form, and the mark
/// and the words below are what the welcome's own fitting renderer lays out
/// in the rows left, with or without a project or a pinned run, restored and
/// expanded, in both glyph columns. Where no action is offered the welcome
/// keeps every row.
#[test]
fn the_welcome_gives_its_first_row_to_the_object_action() {
    let mut offered = 0;
    for size in [(100, 32), (119, 33), (60, 16), SMALL, WIDE, LARGE] {
        for (project, pinned) in [(false, false), (false, true), (true, false), (true, true)] {
            for ascii in [false, true] {
                let mut desk = welcoming(project, pinned);
                let (state, composer) = talking(size, ascii);
                for expanded in [false, true] {
                    let at = format!(
                        "{size:?} project={project} pinned={pinned} ascii={ascii} expanded={expanded}"
                    );
                    let object = screen::object_body(&desk.geometry(size).expect("fits"));
                    let (rows, buffer) = frame(&desk, size, &state, &composer, paint(ascii, false));
                    let shown = painted_action(&buffer, object, expanded, ascii);
                    let reserved = expanded || grows(&desk, size);
                    if [(100, 32), (119, 33)].contains(&size) {
                        assert!(reserved, "{at}: the object grows here");
                    }
                    let form = action_forms(expanded, ascii)[0];
                    assert_eq!(shown, reserved.then_some(form), "{at}\n{}", rows.join("\n"));
                    let welcome = desk.screen(ascii).object;
                    let below = if reserved {
                        offered += 1;
                        let cells = u16::try_from(form.chars().count()).expect("cells");
                        for x in object.x..object.right() - cells {
                            let cell = buffer[(x, object.y)].symbol();
                            assert_eq!(cell, " ", "{at}: the welcome on the action's row at {x}");
                        }
                        Rect::new(object.x, object.y + 1, object.width, object.height - 1)
                    } else {
                        object
                    };
                    let mut alone = Buffer::empty(below);
                    crate::workspace::object::render(
                        &welcome,
                        below,
                        &mut alone,
                        paint(ascii, false),
                    );
                    for y in below.y..below.bottom() {
                        let painted = row_text(&buffer, y, below.x, below.right());
                        let fitted = row_text(&alone, y, below.x, below.right());
                        assert_eq!(painted, fitted, "{at}: row {y}");
                    }
                    let route = desk.route(key(KeyCode::F(4)), size);
                    assert_eq!(route == Route::Repaint, reserved, "{at}");
                    if !reserved {
                        desk.arrange(desk.arrangement().with_layout(Layout::Workbench));
                    }
                }
            }
        }
    }
    assert!(offered > 0, "the welcome never offered its action");
}

/// While the project list is folded the header names the three regions,
/// the one holding the keys in brackets and strong (a weight, never a hue),
/// in both glyph columns; from 120 columns, where all three are drawn, it
/// names none.
#[test]
fn the_folded_header_names_the_three_regions_and_the_one_with_the_keys() {
    for size in [(60, 16), SMALL, (100, 32), WIDE, LARGE] {
        for ascii in [false, true] {
            let mut desk = demo();
            let (state, composer) = talking(size, ascii);
            let folded = size.0 < ASIDE_MIN_WIDTH;
            for (region, words) in [
                (Region::Conversation, "Project [Conversation] Object"),
                (Region::Object, "Project Conversation [Object]"),
                (Region::Aside, "[Project] Conversation Object"),
            ] {
                desk.focus.region = region;
                let (rows, buffer) = frame(&desk, size, &state, &composer, paint(ascii, false));
                let at = format!("{size:?} ascii={ascii} {region:?}");
                let aside = desk.geometry(size).expect("fits").aside;
                assert_eq!(aside.is_none(), folded, "{at}");
                assert_eq!(rows[0].ends_with(words), folded, "{at}: {}", rows[0]);
                assert_eq!(
                    rows[0].contains("Conversation"),
                    folded,
                    "{at}: {}",
                    rows[0]
                );
                assert!(rows[0].contains("demo"), "{at}: {}", rows[0]);
                assert!(!rows[0].contains("Workbench"), "{at}: {}", rows[0]);
                if folded {
                    let open = words.find('[').expect("a bracket");
                    let x = size.0 - u16::try_from(words.len() - open).expect("cells");
                    let cell = &buffer[(x + 1, 0)];
                    assert!(cell.modifier.contains(Modifier::BOLD), "{at}");
                    assert_eq!((cell.fg, cell.bg), (Color::Reset, Color::Reset), "{at}");
                }
                if ascii {
                    assert!(rows[0].is_ascii(), "{at}: {}", rows[0]);
                }
            }
        }
    }
}

/// On the short terminals and at the target sizes, with and without a
/// pinned run: the object offers `[+]` exactly where expanding gives it
/// strictly more cells, and there `F4` grows it; elsewhere `F4` keeps the
/// arrangement as it is (nothing to settle). A kept expansion always offers
/// `[-]`, and `F4` gives back the exact restored frame. Every press is a
/// view change only.
#[test]
fn expand_is_offered_only_where_the_object_grows_and_restore_always_returns() {
    let (mut offered, mut withheld) = (0, 0);
    for size in SHORT.into_iter().chain([SMALL, WIDE, LARGE]) {
        for pinned in [false, true] {
            for ascii in [false, true] {
                let at = format!("{size:?} pinned={pinned} ascii={ascii}");
                let mut desk = opened(pinned);
                let (state, composer) = talking(size, ascii);
                desk.prepare(size, ascii, false);
                let restored = desk.geometry(size).expect("fits");
                let growth = grows(&desk, size);
                if [SMALL, WIDE, LARGE].contains(&size) {
                    assert!(growth, "{at}: expansion must enlarge the object here");
                }
                let (rows, buffer) = frame(&desk, size, &state, &composer, paint(ascii, false));
                let title = title_cells(&desk, size, ascii);
                let fitting = fitting_action(title, restored.object.width, false, ascii);
                let shown = painted_action(&buffer, restored.object, false, ascii);
                assert_eq!(
                    shown,
                    fitting.filter(|_| growth),
                    "{at}: an Expand that does not enlarge\n{}",
                    rows.join("\n")
                );
                if [SMALL, WIDE, LARGE].contains(&size) {
                    assert_eq!(shown, Some(action_forms(false, ascii)[0]), "{at}");
                }
                let before = (desk.arrangement(), kept(&desk));
                let route = desk.route(key(KeyCode::F(4)), size);
                if growth {
                    offered += 1;
                    assert_eq!(route, Route::Repaint, "{at}");
                    assert_eq!(desk.arrangement().layout, Layout::Workbench, "{at}");
                    let expanded = desk.geometry(size).expect("fits").object;
                    assert!(area_of(expanded) > area_of(restored.object), "{at}");
                } else {
                    withheld += 1;
                    assert_eq!(route, Route::Nothing, "{at}: F4 changed the arrangement");
                    assert_eq!(desk.arrangement(), before.0, "{at}");
                    assert_eq!(desk.take_settled(), None, "{at}: nothing to keep");
                    // A kept expansion (an earlier size, a kept preference).
                    desk.arrange(desk.arrangement().with_layout(Layout::Workbench));
                }
                assert_eq!(kept(&desk), before.1, "{at}: a view change only");
                desk.prepare(size, ascii, false);
                let expanded = desk.geometry(size).expect("fits");
                let (_, buffer) = frame(&desk, size, &state, &composer, paint(ascii, false));
                let title = title_cells(&desk, size, ascii);
                let fitting = fitting_action(title, expanded.object.width, true, ascii);
                assert!(fitting.is_some(), "{at}: a restore form fits");
                let shown = painted_action(&buffer, expanded.object, true, ascii);
                assert_eq!(
                    shown, fitting,
                    "{at}: the kept expansion offers its restore"
                );
                assert_eq!(desk.route(key(KeyCode::F(4)), size), Route::Repaint, "{at}");
                assert_eq!(desk.arrangement(), before.0, "{at}: restored exactly");
                assert_eq!(desk.geometry(size), Some(restored), "{at}");
                assert_eq!(kept(&desk), before.1, "{at}");
            }
        }
    }
    assert!(
        offered > 0 && withheld > 0,
        "both sides proved: {offered} offered, {withheld} withheld"
    );
}

/// While the project list is folded, a long project name never takes the
/// three regions' names from the header's first row: the place before them
/// gives way (the end of the name, marked as cut), never the names, with
/// one-row and two-row headers, wide glyphs and in both glyph columns. The
/// header, the project's one home, names it whole where the row has room.
#[test]
fn a_long_project_name_keeps_the_folded_regions_named() {
    let names = [
        "customer-onboarding-automation",
        "nika-tui-ux-wave-20261007",
        "顧客オンボーディング-automation",
    ];
    for size in [(60, 16), (60, 30), (80, 24), (99, 30)] {
        for name in names {
            for ascii in [false, true] {
                let mut desk = Desk::new();
                desk.view = Some(ProjectView::new(
                    "local",
                    name,
                    format!("~/Projects/{name}"),
                ));
                let (state, composer) = talking(size, ascii);
                let cut = if ascii { "..." } else { "…" };
                for (region, words) in [
                    (Region::Conversation, "Project [Conversation] Object"),
                    (Region::Object, "Project Conversation [Object]"),
                    (Region::Aside, "[Project] Conversation Object"),
                ] {
                    desk.focus.region = region;
                    let at = format!("{size:?} {name} ascii={ascii} {region:?}");
                    let (_, buffer) = frame(&desk, size, &state, &composer, paint(ascii, false));
                    let first = row_text(&buffer, 0, 0, size.0);
                    assert_eq!(first.width(), usize::from(size.0), "{at}: {first}");
                    assert!(first.ends_with(words), "{at}: {first}");
                    let place = first.strip_suffix(words).expect("the names");
                    assert!(place.ends_with(' '), "{at}: the place runs into the names");
                    let start: String = name.chars().take(4).collect();
                    assert!(place.contains(&start), "{at}: {place}");
                    if !place.contains(name) {
                        assert!(place.contains(cut), "{at}: a cut name is marked: {place}");
                    }
                    if ascii && name.is_ascii() {
                        assert!(first.is_ascii(), "{at}: {first}");
                    }
                }
                // The header is the project's one home: whole where the row has room.
                let wide = (180, 48);
                let (state, composer) = talking(wide, ascii);
                let (_, buffer) = frame(&desk, wide, &state, &composer, paint(ascii, false));
                let first = row_text(&buffer, 0, 0, wide.0);
                assert!(first.contains(name), "{name} ascii={ascii}: {first}");
            }
        }
    }
}
