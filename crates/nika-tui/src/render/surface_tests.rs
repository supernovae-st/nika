// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The continuous surface as drawn: opening it moves no row of a full
//! screen, its band hides every cell of what it covers down to the composer,
//! each painted item row is the item a press there lands on, the selection
//! reads without colour, the typed choice keeps the draft out of view while
//! its own reply takes the line, and only an inline frame grows for a band.

#![allow(clippy::expect_used)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier};

use super::surface::{Hit, hit};
use super::{Panel, band, draw_focus, draw_inline, focus_areas, live_rows, panel_rows};
use crate::composer::Composer;
use crate::composer::chooser::Entry;
use crate::model::{
    Asked, Committed, Kind, Offer, Presentation, Retained, Shape, UiState, Waiting,
};

const LABEL: &str = "Which currency do the amounts use?";

/// A line of the transcript the band hides while a surface is open.
const EARLIER: &str = "an earlier line the band hides";

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// `count` offers with distinct one-cell keys and labels.
fn offers(count: usize) -> Vec<Offer> {
    (0..count)
        .map(|n| Offer::new(format!("opt-{n}"), format!("label {n}")))
        .collect()
}

/// The typed choice of `count` offers.
fn choice(count: usize, mandatory: bool) -> Waiting {
    let shape = Shape::Choice(offers(count));
    let question = Asked::new(LABEL, "one currency", mandatory, shape, "w-1", 1);
    Waiting::asked("const.currency", question)
}

/// A state painting `waiting` in `presentation` at `size`, a transcript line
/// above.
fn state(presentation: Presentation, size: (u16, u16), waiting: Waiting, ascii: bool) -> UiState {
    let mut state = UiState::new(presentation, false, size);
    state.ascii = ascii;
    state.transcript.push(Committed::new(Kind::Reply, EARLIER));
    state.waiting = waiting;
    state
}

/// A composer holding the keys with the Session's commands offered, the
/// draft `draft` typed first, following `waiting`.
fn composer(waiting: &Waiting, draft: &str) -> Composer {
    let mut composer = Composer::new();
    composer.offer(vec![
        Entry::command(
            "/help",
            "What you can ask",
            "any time · reads only",
            "The help card.",
            "help",
        ),
        Entry::command(
            "/status",
            "Where you are",
            "any time · reads only",
            "Root and seat.",
            "model",
        ),
        Entry::command(
            "/intelligence",
            "Choose the AI",
            "this session",
            "The choices again.",
            "model",
        ),
    ]);
    composer.set_focused(true);
    composer.paste(draft);
    composer.follow(waiting);
    composer
}

/// Select the offer at `at` of `waiting`'s choice by keys.
fn select(composer: &mut Composer, waiting: &Waiting, at: usize) {
    composer.offer_key(waiting, key(KeyCode::Home));
    for _ in 0..at {
        composer.offer_key(waiting, key(KeyCode::Down));
    }
    assert_eq!(composer.offer_selected(waiting), Some(at));
}

/// The frame of `state`.
fn draw(state: &UiState, composer: &Composer) -> Buffer {
    let (width, height) = state.size;
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| match state.presentation {
            Presentation::Inline => draw_inline(frame, state, composer),
            _ => draw_focus(frame, state, composer),
        })
        .expect("draw");
    terminal.backend().buffer().clone()
}

/// The band painting reads, from the same rectangles.
fn band_of(state: &UiState, composer: &Composer) -> Option<(Rect, bool)> {
    let area = Rect::new(0, 0, state.size.0, state.size.1);
    if state.presentation == Presentation::Inline {
        band(state, composer, 0, area, Panel::default())
    } else {
        let [transcript, _, live] = focus_areas(area, state, composer);
        band(state, composer, transcript.y, live, Panel::default())
    }
}

fn row(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect::<String>()
        .trim_end()
        .to_owned()
}

/// The offer whose exact row `text` is, with or without the selection mark.
fn painted(text: &str, offers: &[Offer], ascii: bool) -> Option<usize> {
    let (mark, sep) = if ascii {
        ("> ", " - ")
    } else {
        ("› ", " · ")
    };
    offers.iter().position(|offer| {
        let body = format!("{}{sep}{}", offer.key, offer.label);
        text == format!("  {body}") || text == format!("{mark}{body}")
    })
}

/// Opening a surface (the palette, the slash list, a typed choice, or the
/// palette over that choice) and closing it again moves no row of a full
/// screen: the live area keeps its rows boxed or plain at every qualified
/// size, so the transcript keeps its rows and its reading position.
#[test]
fn opening_any_surface_moves_no_row_of_a_full_screen() {
    for presentation in [Presentation::Focus, Presentation::Workspace] {
        for (width, height) in [(80, 24), (120, 40), (180, 48)] {
            for boxed in [false, true] {
                let at = format!("{presentation:?} {width}x{height} boxed={boxed}");
                let panel = Panel {
                    boxed,
                    consent: None,
                };
                let rest = state(presentation, (width, height), Waiting::Free, false);
                let mut composer = composer(&Waiting::Free, "first line\nsecond line");
                let closed = panel_rows(&rest, &composer, width, height, panel);
                composer.toggle_palette();
                assert_eq!(
                    panel_rows(&rest, &composer, width, height, panel),
                    closed,
                    "{at}"
                );
                composer.close_palette();
                let mut slash = self::composer(&Waiting::Free, "/st");
                assert!(slash.listing().is_some(), "{at}: the slash list");
                let open = panel_rows(&rest, &slash, width, height, panel);
                slash.choose(key(KeyCode::Esc));
                assert!(slash.listing().is_none(), "{at}");
                assert_eq!(
                    panel_rows(&rest, &slash, width, height, panel),
                    open,
                    "{at}"
                );
                let asked = state(presentation, (width, height), choice(9, true), false);
                let mut held = self::composer(&asked.waiting, "first line\nsecond line");
                let choosing = panel_rows(&asked, &held, width, height, panel);
                held.toggle_palette();
                assert_eq!(
                    panel_rows(&asked, &held, width, height, panel),
                    choosing,
                    "{at}"
                );
            }
        }
    }
    let rest = state(Presentation::Focus, (80, 24), Waiting::Free, false);
    let mut composer = composer(&Waiting::Free, "a draft");
    let area = Rect::new(0, 0, 80, 24);
    let closed = focus_areas(area, &rest, &composer);
    composer.toggle_palette();
    assert_eq!(
        focus_areas(area, &rest, &composer),
        closed,
        "the focus view's rows stay"
    );
}

/// The band covers the transcript and the rows above the line, down to the
/// composer, and no cell of what it hides stays: the transcript and the
/// draft are out of view while the palette is open, back once it closes.
#[test]
fn the_band_hides_every_cell_down_to_the_composer() {
    let shown = |buffer: &Buffer| (0..24).map(|y| row(buffer, y)).collect::<Vec<_>>();
    let state = state(Presentation::Focus, (80, 24), Waiting::Free, false);
    let mut composer = composer(&Waiting::Free, "my draft stays hidden");
    let rest = shown(&draw(&state, &composer));
    assert!(rest.iter().any(|row| row.contains(EARLIER)), "{rest:#?}");
    assert!(band_of(&state, &composer).is_none(), "no surface open");
    composer.toggle_palette();
    let (band, framed) = band_of(&state, &composer).expect("the band");
    assert!(!framed, "a plain line under a plain band");
    let open = shown(&draw(&state, &composer));
    for words in [EARLIER, "my draft stays hidden"] {
        assert!(
            !open.iter().any(|row| row.contains(words)),
            "{words}: {open:#?}"
        );
    }
    assert_eq!(band.y, 0, "from the transcript's first row");
    let title = &open[usize::from(band.y)];
    assert!(title.starts_with("Commands · choose an action"), "{title}");
    assert!(title.ends_with("Esc"), "{title}");
    assert_eq!(open[usize::from(band.bottom())], "commands ›");
    assert!(
        open[23].starts_with("↑↓ choose · Enter runs"),
        "{}",
        open[23]
    );
    composer.close_palette();
    assert_eq!(shown(&draw(&state, &composer)), rest, "back as it was");
}

/// Every row the frame paints as an offer is the offer a press on that row
/// lands on, any other row of the band takes the press and nothing outside
/// it does: at the qualified sizes, inline and in the focus view, in both
/// glyph columns, on every page, the selection in view.
#[test]
fn every_painted_offer_row_is_the_offer_a_press_lands_on() {
    let all = offers(9);
    let cases = [
        (Presentation::Focus, (80, 24)),
        (Presentation::Focus, (60, 18)),
        (Presentation::Focus, (120, 40)),
        (Presentation::Inline, (80, 12)),
        (Presentation::Inline, (60, 12)),
    ];
    for (presentation, size) in cases {
        for ascii in [false, true] {
            for selected in [None, Some(0), Some(4), Some(5), Some(8)] {
                let at = format!("{presentation:?} {size:?} ascii={ascii} {selected:?}");
                let state = state(presentation, size, choice(9, true), ascii);
                let mut composer = composer(&state.waiting, "");
                if let Some(at) = selected {
                    select(&mut composer, &state.waiting, at);
                }
                let buffer = draw(&state, &composer);
                let band = band_of(&state, &composer).expect("the band");
                let mut hits = Vec::new();
                for y in 0..size.1 {
                    let text = row(&buffer, y);
                    let press = Position::new(band.0.x + 1, y);
                    let landed = hit(&state, &composer, band, press);
                    let item = match landed {
                        Some(Hit::Item(index)) => Some(index),
                        Some(Hit::Band) | None => None,
                    };
                    assert_eq!(item, painted(&text, &all, ascii), "{at} row {y}: {text:?}");
                    assert_eq!(landed.is_some(), band.0.contains(press), "{at} row {y}");
                    hits.extend(item);
                }
                assert!(!hits.is_empty(), "{at}: no offer painted");
                let first = hits[0];
                assert_eq!(
                    hits,
                    (first..first + hits.len()).collect::<Vec<_>>(),
                    "{at}"
                );
                if let Some(selected) = selected {
                    assert!(hits.contains(&selected), "{at}: the selection is in view");
                }
                if ascii {
                    for y in 0..size.1 {
                        assert!(row(&buffer, y).is_ascii(), "{at} row {y}");
                    }
                }
            }
        }
    }
}

/// The selection is a mark and reverse video while the offers take keys,
/// with no hue when colour is off; without the keys the mark stays and the
/// reverse goes. Words in the ordinary draft never make the offers inert.
#[test]
fn the_selection_reads_without_colour_and_fills_only_while_armed() {
    for ascii in [false, true] {
        let state = state(Presentation::Inline, (80, 12), choice(3, false), ascii);
        let mut composer = composer(&state.waiting, "words waiting in my draft");
        select(&mut composer, &state.waiting, 1);
        let buffer = draw(&state, &composer);
        let mark = if ascii { "> opt-1" } else { "› opt-1" };
        let y = (0..12)
            .find(|y| row(&buffer, *y).starts_with(mark))
            .expect("the selected row");
        for x in 0..7 {
            let cell = &buffer[(x, y)];
            assert!(cell.modifier.contains(Modifier::REVERSED), "x {x}");
            assert_eq!((cell.fg, cell.bg), (Color::Reset, Color::Reset), "x {x}");
        }
        for other in (0..12).filter(|other| *other != y) {
            let reversed =
                (0..80).any(|x| buffer[(x, other)].modifier.contains(Modifier::REVERSED));
            let text = row(&buffer, other);
            // The own reply's field shows its cursor on the line.
            assert!(!reversed || text.contains("reply"), "row {other}: {text}");
        }
        composer.set_focused(false);
        let buffer = draw(&state, &composer);
        let y = (0..12)
            .find(|y| row(&buffer, *y).starts_with(mark))
            .expect("the mark stays");
        assert!(
            (0..7).all(|x| !buffer[(x, y)].modifier.contains(Modifier::REVERSED)),
            "without the keys the selection is not filled"
        );
    }
}

/// While a typed choice holds the line, the ordinary draft waits out of view,
/// untouched, and the line shows the own reply's field: its placeholder, then
/// what is typed there. The title names the question and its facts, and the
/// head reads the question's own words.
#[test]
fn the_typed_choice_holds_the_line_and_keeps_the_draft_out_of_view() {
    let state = state(Presentation::Focus, (80, 24), choice(3, true), false);
    let mut composer = composer(&state.waiting, "my unsent draft");
    let buffer = draw(&state, &composer);
    let shown: Vec<String> = (0..24).map(|y| row(&buffer, y)).collect();
    assert!(
        !shown.iter().any(|row| row.contains("my unsent draft")),
        "{shown:#?}"
    );
    assert!(!shown.iter().any(|row| row.contains(EARLIER)), "{shown:#?}");
    let (band, _) = band_of(&state, &composer).expect("the band");
    assert_eq!(shown[usize::from(band.y)], "Question · required");
    assert_eq!(shown[usize::from(band.y) + 1], LABEL);
    let line = usize::from(band.bottom());
    assert!(shown[line].starts_with("reply ›"), "{}", shown[line]);
    assert!(
        shown[line].contains("type your own reply"),
        "{}",
        shown[line]
    );
    for c in "dollars".chars() {
        assert!(composer.own_key(key(KeyCode::Char(c))));
    }
    let buffer = draw(&state, &composer);
    let typed: Vec<String> = (0..24).map(|y| row(&buffer, y)).collect();
    assert_eq!(typed[line], "reply › dollars");
    assert_eq!(
        composer.text(),
        "my unsent draft",
        "the draft never changed"
    );
    assert_eq!(composer.own_reply(), "dollars");
}

/// Only an inline frame grows for a surface's band; a full screen keeps its
/// rows. The inline frame never takes more than all but two rows.
#[test]
fn only_an_inline_frame_grows_for_its_band() {
    for presentation in [Presentation::Inline, Presentation::Focus] {
        let rest = state(presentation, (80, 24), Waiting::Free, false);
        let mut composer = composer(&Waiting::Free, "");
        let closed = live_rows(&rest, &composer, 80, 24);
        composer.toggle_palette();
        let open = live_rows(&rest, &composer, 80, 24);
        if presentation == Presentation::Inline {
            assert!(open > closed, "{open} > {closed}");
            for height in [6, 8, 10] {
                assert!(live_rows(&rest, &composer, 80, height) <= height - 2);
            }
        } else {
            assert_eq!(open, closed);
        }
    }
}

/// Beside a typed choice the surface shows what the Session keeps of the
/// request, above the question: its goal on one quiet row, the questions
/// still open on the next, both whole; none when it keeps nothing.
#[test]
fn a_typed_choice_shows_the_request_the_session_keeps() {
    let kept = Retained::new(
        Some("sum the amounts\nof the monday export".to_owned()),
        vec!["which currency".to_owned(), "keep the header".to_owned()],
    );
    let shape = Shape::Choice(offers(3));
    let asked = Asked::new(LABEL, "one currency", true, shape, "w-1", 1).retaining(kept);
    let waiting = Waiting::asked("const.currency", asked);
    let state = state(Presentation::Focus, (80, 24), waiting, false);
    let composer = composer(&state.waiting, "");
    let buffer = draw(&state, &composer);
    let (band, _) = band_of(&state, &composer).expect("the band");
    let top = usize::from(band.y);
    let shown: Vec<String> = (0..24).map(|y| row(&buffer, y)).collect();
    assert_eq!(shown[top], "Question · required");
    assert_eq!(
        shown[top + 1],
        "Request · sum the amounts of the monday export"
    );
    assert_eq!(shown[top + 2], "Open · which currency · keep the header");
    assert_eq!(shown[top + 3], LABEL);
    let bare = self::state(Presentation::Focus, (80, 24), choice(3, true), false);
    let buffer = draw(&bare, &composer);
    assert_eq!(
        row(&buffer, band.y + 1),
        LABEL,
        "nothing kept, nothing shown"
    );
}

/// A line the Session holds for its knowledge opens the same surface:
/// `Knowledge`, the held line as the request kept, the refusal, then its two
/// exact acts as offers.
#[test]
fn a_held_line_opens_the_surface_with_its_two_acts() {
    let offers = vec![
        Offer::new("/knowledge embedded", "use the knowledge built into Nika"),
        Offer::new("cancel", "drop your message"),
    ];
    let kept = Retained::new(Some("sum the amounts".to_owned()), Vec::new());
    let asked = Asked::new(
        "Not admitted (CODE): why",
        "",
        false,
        Shape::Choice(offers),
        "7:k",
        7,
    )
    .retaining(kept);
    let state = state(
        Presentation::Focus,
        (80, 24),
        Waiting::knowledge(asked),
        false,
    );
    let composer = composer(&state.waiting, "");
    let buffer = draw(&state, &composer);
    let (band, _) = band_of(&state, &composer).expect("the band");
    let top = usize::from(band.y);
    let shown: Vec<String> = (0..24).map(|y| row(&buffer, y)).collect();
    assert_eq!(shown[top], "Knowledge · your message waits");
    assert_eq!(shown[top + 1], "Request · sum the amounts");
    assert_eq!(shown[top + 2], "Not admitted (CODE): why");
    // The key column holds 16 cells: `cancel` pads to it, the longer key
    // pushes its label along.
    let act = |key: &str, does: &str| {
        (shown.iter()).any(|line| line.trim_start().starts_with(key) && line.contains(does))
    };
    assert!(
        act("/knowledge embedded", "· use the knowledge built into Nika"),
        "{shown:#?}"
    );
    assert!(act("cancel", "· drop your message"), "{shown:#?}");
}
