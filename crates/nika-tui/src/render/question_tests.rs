// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The card of a typed question that offers nothing (a text, an exact value)
//! as drawn: it names the shape of the answer, never repeats the question's
//! own words outside its live home, reads in the workspace the first exact
//! words and where the rest is read, gives way to a turn and to the line that
//! answers it, and keeps its rows under a surface. A typed choice has no card:
//! it opens the surface above the line (`surface_tests`).

#![allow(clippy::expect_used)]

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};

use super::question::{Share, WORD_ROWS, Whole, carried, hang, rows, word_cap};
use super::{draw_focus, draw_inline, live_areas, live_rows};
use crate::composer::Composer;
use crate::model::{Asked, Committed, Kind, Offer, Presentation, Shape, UiState, Waiting};
use crate::visual::role;
use nika_display::theme::Role;
use unicode_width::UnicodeWidthStr;

const LABEL: &str = "Which currency do the amounts use?";
const WHY: &str = "the report sums amounts and needs one currency";

fn asked(shape: Shape, mandatory: bool) -> Waiting {
    let question = Asked::new(LABEL, WHY, mandatory, shape, "w-1", 1);
    Waiting::asked("const.currency", question)
}

/// A state painting `waiting` in `presentation` at `size`, the question's
/// own words in the transcript above.
fn state(presentation: Presentation, size: (u16, u16), waiting: Waiting, ascii: bool) -> UiState {
    let mut state = UiState::new(presentation, false, size);
    state.ascii = ascii;
    let words = format!("{LABEL}\n  ({WHY})");
    state.transcript.push(Committed::new(Kind::Question, words));
    state.waiting = waiting;
    state
}

/// A composer holding the keys, following `waiting`.
fn composer(waiting: &Waiting) -> Composer {
    let mut composer = Composer::new();
    composer.set_focused(true);
    composer.follow(waiting);
    composer
}

/// The frame of `state`.
fn frame(state: &UiState, composer: &Composer) -> Buffer {
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

fn row(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect::<String>()
        .trim_end()
        .to_owned()
}

/// The card names the shape of the answer and whether it is required; the
/// question's own words keep their one home in the transcript. A typed
/// choice has no card at all: the surface above the line takes it.
#[test]
fn the_card_names_the_shape_and_never_repeats_the_question() {
    for (shape, mandatory, header) in [
        (Shape::Text, true, "A text answer · required"),
        (Shape::Text, false, "A text answer"),
        (Shape::Literal, false, "An exact value"),
    ] {
        for ascii in [false, true] {
            let state = state(
                Presentation::Inline,
                (80, 12),
                asked(shape.clone(), mandatory),
                ascii,
            );
            let composer = composer(&state.waiting);
            let buffer = frame(&state, &composer);
            let shown: Vec<String> = (0..12).map(|y| row(&buffer, y)).collect();
            let header = if ascii {
                header.replace('·', "-")
            } else {
                header.to_owned()
            };
            assert!(shown.contains(&header), "{header}: {shown:#?}");
            for words in [LABEL, WHY] {
                assert!(!shown.iter().any(|row| row.contains(words)), "{shown:#?}");
            }
        }
    }
    let choice = Shape::Choice(vec![Offer::new("eur", "Euro")]);
    let state = state(Presentation::Focus, (80, 24), asked(choice, true), false);
    assert_eq!(rows(&state, 80), 0, "a typed choice has no card");
}

/// The card gives way while a turn works, and a question kept in prose has
/// none; a surface over it hides it without moving its rows. It never takes
/// the row of the line that answers.
#[test]
fn the_card_gives_way_and_never_takes_the_line() {
    let waiting = asked(Shape::Text, true);
    let mut state = state(Presentation::Focus, (80, 24), waiting.clone(), false);
    let mut composer = composer(&state.waiting);
    assert_eq!(rows(&state, 80), 1, "the header, outside a live home");
    let bare = UiState::new(Presentation::Focus, false, (80, 24));
    assert!(live_rows(&state, &composer, 80, 24) > live_rows(&bare, &composer, 80, 24));
    state.busy = Some("● reading your answer".to_owned());
    assert_eq!(rows(&state, 80), 0, "a turn works");
    state.busy = None;
    composer.toggle_palette();
    assert_eq!(
        rows(&state, 80),
        1,
        "a surface hides the card, its rows stay"
    );
    composer.close_palette();
    state.waiting = Waiting::Question {
        key: "unknown_cost".to_owned(),
    };
    assert_eq!(rows(&state, 80), 0, "a question in prose");
    state.waiting = Waiting::Question {
        key: "const.currency".to_owned(),
    };
    assert_eq!(rows(&state, 80), 0, "the same key in prose");
    state.waiting = waiting;
    for height in [3, 4, 5, 6, 8] {
        let areas = live_areas(&state, &composer, Rect::new(0, 0, 80, height), false);
        assert!(
            areas.input.height >= 1,
            "height {height}: the line keeps its row"
        );
        assert!(areas.card.bottom() <= areas.input.y, "height {height}");
    }
    let areas = live_areas(&state, &composer, Rect::new(0, 0, 80, 6), false);
    let mut terminal = Terminal::new(TestBackend::new(80, 6)).expect("terminal");
    terminal
        .draw(|frame| super::render_live(frame, &state, &composer, frame.area()))
        .expect("draw");
    let header = row(terminal.backend().buffer(), areas.card.y);
    assert_eq!(header, "A text answer · required");
}

/// The Session's words of the typed question, as its block holds them: the
/// label, its reason under an indent, what the compiler could not settle,
/// the reply protocol and how a reply is taken.
const ASKED: &str = "Which currency do the amounts use?\n    (the report sums amounts and needs one currency)\nwhat I could not settle: · the export mixes EUR and USD\nreply on the next line · `cancel` drops this · `why?` explains\nyour reply is taken exactly as you type it";

/// The witness that question was asked as, in this session.
const WITNESS: &str = "3:q-currency";

/// A fitting workspace waiting on the typed question of `shape`, its words
/// tied to it by identity, after the human's request.
fn home(shape: Shape) -> UiState {
    let mut state = UiState::new(Presentation::Workspace, false, (120, 40));
    state
        .transcript
        .push(Committed::new(Kind::Human, "sum the amounts"));
    state.transcript.push(Committed::question(WITNESS, ASKED));
    let asked = Asked::new(LABEL, WHY, true, shape, WITNESS, 3);
    state.waiting = Waiting::asked("const.currency", asked);
    state
}

/// The renderer's words `text` in the glyph column in use.
fn own_twin(text: &str, ascii: bool) -> String {
    if ascii {
        text.replace('·', "-").replace('…', "...")
    } else {
        text.to_owned()
    }
}

/// The frame of the live area `live` of `state`, nothing else painted.
fn live_frame(state: &UiState, composer: &Composer, live: Rect) -> Buffer {
    let backend = TestBackend::new(live.right(), live.bottom());
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| super::render_live(frame, state, composer, live))
        .expect("draw");
    terminal.backend().buffer().clone()
}

/// The text of row `y` of `buffer`, from column `x`.
fn row_from(buffer: &Buffer, x: u16, y: u16) -> String {
    (x..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect::<String>()
        .trim_end()
        .to_owned()
}

/// In the workspace the card is the question's live home: the accent title
/// with quiet facts, then the first exact rows of the Session's words, wrapped
/// by cells under their own indent, and where the whole question is read when
/// they do not all show. The line that answers keeps its row first, and the
/// card asks for exactly the rows it paints.
#[test]
fn the_live_home_reads_the_first_exact_words_and_where_the_rest_is_read() {
    for (width, height) in [(44, 18), (60, 6), (80, 9), (68, 30), (40, 5)] {
        for ascii in [false, true] {
            let at = format!("{width}x{height} ascii={ascii}");
            let mut state = home(Shape::Text);
            state.ascii = ascii;
            let composer = composer(&state.waiting);
            let live = Rect::new(0, 0, width, height);
            let areas = live_areas(&state, &composer, live, false);
            let card = areas.card;
            assert!(
                areas.input.height >= 1 && card.bottom() <= areas.input.y,
                "{at}"
            );
            assert_eq!(carried(&state, &composer, live, false), Some(1), "{at}");
            let buffer = live_frame(&state, &composer, live);
            let facts = own_twin("Question · A text answer · required", ascii);
            assert_eq!(row_from(&buffer, 0, card.y), facts, "{at}");
            let words = hang(ASKED, usize::from(width));
            let shown: Vec<String> = (card.y + 1..card.bottom())
                .map(|y| row_from(&buffer, 0, y))
                .collect();
            let tall = usize::from(card.height);
            let share = Share::of(tall, words.len(), word_cap(&state)).expect("a home");
            match share.whole {
                Whole::Shown => assert_eq!(shown, words, "{at}"),
                Whole::Row => {
                    assert_eq!(shown[..share.words], words[..share.words], "{at}");
                    let cue = own_twin("… the whole question: F2", ascii);
                    assert_eq!(shown[share.words], cue, "{at}");
                }
                Whole::Inline => {
                    let cue = own_twin(" … F2", ascii);
                    let kept = shown[0].strip_suffix(&cue).expect("the cue");
                    assert!(
                        !kept.is_empty() && words[0].starts_with(kept),
                        "{at}: {kept}"
                    );
                }
            }
        }
    }
    let state = home(Shape::Text);
    let composer = composer(&state.waiting);
    let roomy = Rect::new(0, 0, 80, 30);
    let asked = rows(&state, 80);
    assert_eq!(
        live_areas(&state, &composer, roomy, false).card.height,
        asked
    );
}

/// The live home's title wears the accent with colour and the weight without;
/// its facts stay quiet, never the warning hue a gate or a cost keeps; and no
/// hue or background is painted without colour.
#[test]
fn the_live_home_titles_in_the_accent_and_keeps_its_facts_quiet() {
    for color in [true, false] {
        let mut state = home(Shape::Text);
        state.color = color;
        let composer = composer(&state.waiting);
        let live = Rect::new(0, 0, 44, 18);
        let buffer = live_frame(&state, &composer, live);
        let y = live_areas(&state, &composer, live, false).card.y;
        let (title, facts) = (&buffer[(0, y)], &buffer[(13, y)]);
        if color {
            let accent = role::style(Role::Accent, true).fg.unwrap_or(Color::Reset);
            let warn = role::style(Role::Warn, true).fg.unwrap_or(Color::Reset);
            assert_eq!(title.fg, accent);
            assert_ne!(title.fg, warn);
            assert_ne!(facts.fg, warn);
        } else {
            assert!(title.modifier.contains(Modifier::BOLD));
            for x in 0..44 {
                let cell = &buffer[(x, y)];
                assert_eq!((cell.fg, cell.bg), (Color::Reset, Color::Reset), "x {x}");
            }
        }
    }
}

/// The card carries the words only while it paints them: never while a turn
/// works (the reader still opens them), nor with too few rows for its title
/// and first word row, nor outside the workspace. A surface over it, and a
/// scrolled reading, change nothing: the transcript keeps its rows.
#[test]
fn the_card_carries_the_words_only_while_it_paints_them() {
    let live = Rect::new(0, 0, 44, 18);
    let mut state = home(Shape::Text);
    let mut composer = composer(&state.waiting);
    assert_eq!(carried(&state, &composer, live, false), Some(1));
    state.busy = Some("● reading your answer".to_owned());
    assert_eq!(
        carried(&state, &composer, live, false),
        None,
        "a turn works"
    );
    assert_eq!(
        super::question::asked_block(&state),
        Some(1),
        "the reader keeps it"
    );
    state.busy = None;
    composer.toggle_palette();
    assert_eq!(
        carried(&state, &composer, live, false),
        Some(1),
        "a surface over the card keeps the transcript's rows"
    );
    composer.close_palette();
    for height in [1, 2, 3, 4] {
        let short = Rect::new(0, 0, 44, height);
        assert_eq!(
            carried(&state, &composer, short, false),
            None,
            "{height} rows"
        );
    }
    assert_eq!(carried(&state, &composer, live, false), Some(1));
    let mut scrolled = home(Shape::Text);
    scrolled.focus_scroll = 7;
    assert_eq!(carried(&scrolled, &composer, live, false), Some(1));
    state.presentation = Presentation::Focus;
    assert_eq!(
        carried(&state, &composer, live, false),
        None,
        "the focus view"
    );
}

/// The title comes first, then the first word row; the words take the rest,
/// every row the cap holds and past it a bounded prefix that says where the
/// whole question is read; that row never hides a single word row; a card too
/// short for that carries none.
#[test]
fn the_card_shares_its_rows_words_first_and_bounded() {
    let share = |words, whole| Some(Share { words, whole });
    let cap = WORD_ROWS;
    assert_eq!(Share::of(1, 3, cap), None, "the title alone");
    assert_eq!(Share::of(4, 0, cap), None, "no words");
    assert_eq!(Share::of(2, 1, cap), share(1, Whole::Shown));
    assert_eq!(Share::of(2, 4, cap), share(1, Whole::Inline));
    assert_eq!(Share::of(3, 4, cap), share(1, Whole::Row));
    assert_eq!(Share::of(5, 4, cap), share(4, Whole::Shown));
    assert_eq!(Share::of(9, 9, cap), share(WORD_ROWS, Whole::Row));
    // One row past the cap is read where it stands: the cue would take it.
    assert_eq!(Share::of(8, 7, cap), share(7, Whole::Shown));
    assert_eq!(Share::of(8, 8, cap), share(WORD_ROWS, Whole::Row));
    // A taller frame's cap holds more: nine rows whole under a cap of ten.
    assert_eq!(Share::of(11, 9, 10), share(9, Whole::Shown));
    assert_eq!(Share::of(9, 9, 10), share(7, Whole::Row));
    for cap in [WORD_ROWS, 10] {
        for rows in 0..20 {
            for words in 0..14 {
                let Some(share) = Share::of(rows, words, cap) else {
                    continue;
                };
                let at = format!("{rows} {words} cap {cap}: {share:?}");
                let cue = usize::from(share.whole == Whole::Row);
                assert!(1 + share.words + cue <= rows, "{at}");
                assert!((1..=cap + 1).contains(&share.words), "{at}");
                assert_eq!(share.whole == Whole::Shown, share.words == words, "{at}");
                if share.whole == Whole::Row {
                    assert!(words >= share.words + 2, "{at}: the cue hides one row");
                }
            }
        }
    }
}

/// The captured currency question, as its block holds it: the label, why
/// it is asked, the reply protocol, and how a reply is taken.
const CURRENCY: &str = "the currency code\n  (The compiler cannot invent this authoring value.)\n  reply on the next line · `cancel` drops this · `why?` explains\n  no intelligence reads this reply: say the value alone (one word, a number, a path), or put a longer value in quotes — it is taken exactly as you type it";

/// The frame's cap lets the card read every word it has room for: at 60x18
/// the row one past the fewest stands where the cue would, at 120x40 a
/// quarter of the frame holds all nine rows, and at 80x24 and 180x48 the
/// words already fit. The rule of how a reply is taken stays in the card,
/// never behind `F2` while the rows exist; the transcript keeps its row.
#[test]
fn the_card_reads_every_word_its_frame_has_room_for() {
    for (size, width, height, count) in [
        ((60, 18), 60, 11, 7),
        ((80, 24), 80, 9, 5),
        ((120, 40), 44, 17, 9),
        ((180, 48), 68, 14, 6),
    ] {
        let at = format!("{size:?}");
        let mut state = home(Shape::Literal);
        state.size = size;
        state.transcript[1] = Committed::question(WITNESS, CURRENCY);
        let composer = composer(&state.waiting);
        let live = Rect::new(0, 0, width, height);
        let words = hang(CURRENCY, usize::from(width));
        assert_eq!(words.len(), count, "{at}: {words:#?}");
        let areas = live_areas(&state, &composer, live, false);
        let card = areas.card;
        assert_eq!(usize::from(card.height), 1 + count, "{at}: every word row");
        assert!(
            areas.input.height >= 1 && card.bottom() <= areas.input.y,
            "{at}"
        );
        assert_eq!(carried(&state, &composer, live, false), Some(1), "{at}");
        let buffer = live_frame(&state, &composer, live);
        let shown: Vec<String> = (card.y + 1..card.bottom())
            .map(|y| row_from(&buffer, 0, y))
            .collect();
        assert_eq!(shown, words, "{at}: the Session's exact words, whole");
        let last = shown.last().map_or("", String::as_str);
        assert!(last.ends_with("as you type it"), "{at}: the reply rule");
        let cue = shown.iter().any(|row| row.contains("the whole question"));
        assert!(!cue, "{at}: no reader row while the words show");
    }
}

/// The words wrap by cells and keep every glyph in order: a break takes the
/// space it falls on, a word wider than a row continues on the next, blank
/// lines stay, and every continuation hangs under its line's own indent.
#[test]
fn the_words_wrap_by_cells_under_their_own_indent_keeping_every_glyph() {
    let text = "the currency code\n    (The compiler cannot invent this authoring value.)\n\nunbreakable-identifier-wider-than-a-row ok\n  日本語のプロジェクト  ";
    let glyphs = |text: &str| {
        text.chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
    };
    for width in [8, 12, 20, 33, 44, 80] {
        let rows = hang(text, width);
        assert_eq!(glyphs(&rows.concat()), glyphs(text), "{width}");
        for row in &rows {
            assert!(row.width() <= width, "{width}: {row:?}");
        }
    }
    assert_eq!(hang("the currency code", 44), ["the currency code"]);
    assert_eq!(hang("a b\n\nc", 10), ["a b", "", "c"]);
    assert_eq!(
        hang("    (The compiler cannot invent this authoring value.)", 24),
        [
            "    (The compiler cannot",
            "    invent this",
            "    authoring value.)"
        ]
    );
    assert!(hang("anything", 0).is_empty());
}
