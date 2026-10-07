// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The two layouts of one desk, proved on its state and on its frames: `F4`,
//! the separator keys and the pointer's moves change the view only (no draft
//! is sent, nothing is opened, approved or detached, no reading position
//! moves); every arrangement draws the composer, the object and its chrome
//! inside the screen, once, at the target sizes, without colour and without
//! motion.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Position;
use ratatui::style::{Color, Modifier};

use super::{Desk, Route, composer_route, draw};
use crate::composer::Composer;
use crate::model::{Presentation, Script, UiState, demo_project};
use crate::session::feed::Observed;
use crate::visual::logomark::REVEAL_ENDS;
use crate::workspace::candidate::Proposed;
use crate::workspace::focus::Region;
use crate::workspace::geometry::{
    ASIDE_MIN, Arrangement, CONVERSATION_MIN_ROWS, CONVERSATION_MIN_WIDTH, Layout, OBJECT_MIN_ROWS,
    Separator,
};
use crate::workspace::inspect::Inspected;
use crate::workspace::object::Paint;
use crate::workspace::screen::{self, Switch};
use nika_display::run_story::RunFrame;

/// The minimum and the three target sizes.
const TARGETS: [(u16, u16); 4] = [(60, 16), (80, 24), (120, 40), (180, 48)];
const WIDE: (u16, u16) = (120, 40);
const LARGE: (u16, u16) = (180, 48);
const SMALL: (u16, u16) = (80, 24);
const TINY: (u16, u16) = (59, 20);
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
    desk.route(key(KeyCode::F(4)), WIDE);
    let rows = cells(&desk, Separator::Beneath, WIDE);
    assert_eq!(desk.route(key(KeyCode::Char('+')), WIDE), Route::Repaint);
    assert_eq!(cells(&desk, Separator::Beneath, WIDE), rows - 1);
    for _ in 0..40 {
        desk.route(key(KeyCode::Char('-')), WIDE);
    }
    let geometry = desk.geometry(WIDE).expect("fits");
    assert_eq!(
        geometry.object.height, OBJECT_MIN_ROWS,
        "the object keeps its rows"
    );
    let before = desk.arrangement();
    let ctrl = KeyEvent::new(KeyCode::Char('+'), KeyModifiers::CONTROL);
    assert_eq!(
        desk.route(ctrl, WIDE),
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
    // The Workbench's rule under the object: up gives the conversation rows,
    // never past the object's minimum; down never takes the composer's rows.
    desk.set_layout(Layout::Workbench);
    let bench = desk.geometry(WIDE).expect("fits");
    let rule = bench.handle(Separator::Beneath).expect("beneath");
    let from = Position::new(rule.x + 10, rule.y);
    assert_eq!(desk.press(from, WIDE), Some(Separator::Beneath));
    assert!(desk.drag_to(Position::new(from.x, from.y - 5), WIDE));
    assert_eq!(
        cells(&desk, Separator::Beneath, WIDE),
        bench.conversation.height + 5
    );
    assert!(desk.drag_to(Position::new(from.x, 0), WIDE));
    assert_eq!(
        desk.geometry(WIDE).expect("fits").object.height,
        OBJECT_MIN_ROWS
    );
    assert!(desk.drag_to(Position::new(from.x, 39), WIDE));
    assert_eq!(
        cells(&desk, Separator::Beneath, WIDE),
        CONVERSATION_MIN_ROWS
    );
    // A layout switch lets the separator go.
    desk.toggle_layout();
    assert_eq!(desk.dragging(), None);
}

/// Each layout keeps its own shares and both keep the aside: switching and
/// resizing never rewrite a share, so the same size shows the same cells.
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
    desk.route(key(KeyCode::F(4)), LARGE);
    for _ in 0..3 {
        desk.route(key(KeyCode::Char('-')), LARGE);
    }
    let bench = desk.geometry(LARGE).expect("fits");
    assert_eq!(bench.aside, session.aside, "the project column is shared");
    for size in [SMALL, (60, 16), WIDE, (100, 32)] {
        let geometry = desk.geometry(size).expect("fits");
        assert!(
            geometry.conversation.height >= CONVERSATION_MIN_ROWS,
            "{size:?}"
        );
    }
    assert_eq!(desk.geometry(LARGE), Some(bench));
    desk.route(key(KeyCode::F(4)), LARGE);
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

/// The switch's words for `layout` in the glyph column.
fn switch_words(layout: Layout, ascii: bool) -> String {
    let sep = if ascii { "-" } else { "·" };
    match layout {
        Layout::Session => format!("[Session] Workbench {sep} F4"),
        Layout::Workbench => format!("Session [Workbench] {sep} F4"),
        _ => "unsupported layout in this fixture".to_owned(),
    }
}

/// Both layouts at the minimum and the target sizes, in both glyph columns:
/// the composer keeps the draft on screen, the object and the header are
/// drawn, the layout in view is named once (at the header's end), and a run
/// stays pinned.
#[test]
fn both_layouts_draw_the_composer_the_object_and_one_switch_at_the_target_sizes() {
    for size in TARGETS {
        for layout in Layout::ALL {
            for ascii in [false, true] {
                let mut desk = with_run();
                desk.set_layout(layout);
                desk.prepare(size, ascii, false);
                let (state, composer) = talking(size, ascii);
                let (rows, _) = frame(&desk, size, &state, &composer, paint(ascii, false));
                let at = format!("{size:?} {layout:?} ascii={ascii}");
                let text = rows.join("\n");
                assert!(
                    text.contains(DRAFT),
                    "{at}: the draft left the screen\n{text}"
                );
                assert!(rows[0].contains("demo"), "{at}: {}", rows[0]);
                let words = switch_words(layout, ascii);
                assert!(rows[0].trim_end().ends_with(&words), "{at}: {}", rows[0]);
                assert_eq!(
                    text.matches("Workbench").count(),
                    1,
                    "{at}: named once\n{text}"
                );
                assert!(text.contains("long.nika"), "{at}: the run's object\n{text}");
                let last = rows.last().expect("rows");
                assert!(last.contains("long.nika"), "{at}: the pinned run: {last}");
                if ascii {
                    assert!(rows[0].is_ascii(), "{at}: {}", rows[0]);
                }
            }
        }
    }
}

/// Without colour the switch carries the layout in view by its brackets and
/// its weight, never by a hue; each word asks what it says.
#[test]
fn the_switch_names_its_layout_without_colour_and_each_word_asks_its_layout() {
    let mut desk = demo();
    let (state, composer) = talking(WIDE, false);
    for layout in Layout::ALL {
        desk.set_layout(layout);
        let (rows, buffer) = frame(&desk, WIDE, &state, &composer, paint(false, false));
        let header = desk.geometry(WIDE).expect("fits").header;
        let place = desk.screen(false).place;
        let area = screen::switch_area(&place, header, false).expect("room for the switch");
        assert_eq!(area.right(), header.right());
        for x in area.x..area.right() {
            let cell = &buffer[(x, area.y)];
            assert_eq!(
                (cell.fg, cell.bg),
                (Color::Reset, Color::Reset),
                "a hue at {x}"
            );
        }
        let words = switch_words(layout, false);
        let start = rows[0].find(&words).expect("the switch");
        let bracket = rows[0][start..].find('[').expect("a bracket");
        let bold = u16::try_from(rows[0][..start + bracket].chars().count()).expect("x");
        assert!(buffer[(bold + 1, area.y)].modifier.contains(Modifier::BOLD));
        let ask = |x: u16| screen::switch_at(&place, header, layout, false, x);
        assert!(matches!(layout, Layout::Session | Layout::Workbench));
        let session_at = if layout == Layout::Session {
            area.x + 1
        } else {
            area.x
        };
        assert_eq!(ask(session_at), Some(Switch::To(Layout::Session)));
        assert_eq!(ask(area.x + 12), Some(Switch::To(Layout::Workbench)));
        assert_eq!(ask(area.right() - 1), Some(Switch::Toggle));
        assert_eq!(ask(area.right() - 3), None, "the separator asks nothing");
        assert_eq!(ask(area.x - 1), None);
    }
}

/// The separator the pointer holds is drawn reversed until it is let go.
#[test]
fn a_held_separator_is_reversed_until_it_is_let_go() {
    for layout in Layout::ALL {
        let mut desk = demo();
        desk.set_layout(layout);
        let (state, composer) = talking(WIDE, false);
        let geometry = desk.geometry(WIDE).expect("fits");
        assert!(matches!(layout, Layout::Session | Layout::Workbench));
        let separator = if layout == Layout::Session {
            Separator::Beside
        } else {
            Separator::Beneath
        };
        let handle = geometry.handle(separator).expect("shown");
        desk.press(Position::new(handle.x, handle.y), WIDE);
        let (_, held) = frame(&desk, WIDE, &state, &composer, paint(false, false));
        let reversed = |buffer: &Buffer| {
            (handle.y..handle.bottom()).all(|y| {
                (handle.x..handle.right())
                    .all(|x| buffer[(x, y)].modifier.contains(Modifier::REVERSED))
            })
        };
        assert!(reversed(&held), "{layout:?}");
        desk.release();
        let (_, free) = frame(&desk, WIDE, &state, &composer, paint(false, false));
        assert!(!reversed(&free), "{layout:?}");
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
            let seat = desk.screen(ascii).thread.intelligence;
            let transcript = screen::panel_areas(&geometry, &state, &composer, seat.as_deref())[1];
            let area = screen::latest_area(transcript, ascii).expect("room for the marker");
            let (rows, buffer) = frame(&desk, LARGE, &state, &composer, paint(ascii, false));
            assert_eq!(rows.join("\n").matches(marker).count(), 1, "{layout:?}");
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
