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
use crate::workspace::candidate::Proposed;
use crate::workspace::cards::review::{Review, fixture, summarized};
use crate::workspace::inspect::Inspected;

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

#[test]
fn an_armed_exit_stays_visible_under_a_choice_or_command_surface() {
    for presentation in [
        Presentation::Inline,
        Presentation::Focus,
        Presentation::Workspace,
    ] {
        for ascii in [false, true] {
            for commands in [false, true] {
                let waiting = Waiting::Selection {
                    asked: Asked::new(
                        "Choose an AI connection",
                        "",
                        true,
                        Shape::Choice(offers(25)),
                        "intelligence",
                        1,
                    ),
                };
                let mut state = state(presentation, (80, 24), waiting, ascii);
                let mut composer = composer(&state.waiting, "a draft kept apart");
                if commands {
                    composer.toggle_palette();
                }
                state.completion = Some("an older completion notice".into());
                state.interrupt_armed = true;
                let buffer = draw(&state, &composer);
                let footer = row(&buffer, 23);
                assert_eq!(
                    footer,
                    super::own("Ctrl+C again leaves · any key stays", ascii)
                );
                assert_eq!(composer.text(), "a draft kept apart");
                assert_eq!(
                    composer.offer_selected(&state.waiting),
                    Some(0),
                    "as it opened"
                );
                state.interrupt_armed = false;
                let buffer = draw(&state, &composer);
                assert_eq!(row(&buffer, 23), "an older completion notice");
            }
        }
    }
}

/// The offer whose exact row `text` is, with or without the selection mark.
fn painted(text: &str, offers: &[Offer], ascii: bool) -> Option<usize> {
    let (mark, selected) = if ascii {
        ("> ", "(*)")
    } else {
        ("› ", "(●)")
    };
    // A question the author asked numbers its first nine offers: the digit
    // that selects each one stands between the mark and the glyph.
    offers.iter().enumerate().position(|(at, offer)| {
        let digit = if at < 9 {
            format!("{} ", at + 1)
        } else {
            "  ".to_owned()
        };
        [String::new(), digit].iter().any(|digit| {
            text == format!("  {digit}( ) {}", offer.label)
                || text == format!("{mark}{digit}{selected} {}", offer.label)
        })
    })
}

#[test]
fn long_settings_and_proposal_choices_keep_navigation_on_the_visible_page() {
    for size in [(60, 18), (80, 24), (120, 40)] {
        for proposal in [false, true] {
            let asked = Asked::new(
                "Choose a model",
                "",
                true,
                Shape::Choice(offers(28)),
                "composite",
                1,
            );
            let waiting = if proposal {
                Waiting::proposal_choice("candidate", "model", asked)
            } else {
                Waiting::selection(asked)
            };
            let state = state(Presentation::Focus, size, waiting.clone(), true);
            let mut composer = composer(&waiting, "kept draft");
            select(&mut composer, &waiting, 23);
            let buffer = draw(&state, &composer);
            let band = band_of(&state, &composer).expect("active surface");
            // A proposal's choice fills the radio of its selection; a settings list keeps its
            // radio for the value in use and marks the selection by its cursor alone.
            let selected = (0..size.1)
                .find(|y| {
                    let shown = row(&buffer, *y);
                    if proposal {
                        shown.contains("(*) label 23")
                    } else {
                        shown.contains("> ( ) label 23")
                    }
                })
                .expect("the selected model remains visible");
            assert_eq!(
                hit(
                    &state,
                    &composer,
                    band,
                    Position::new(band.0.x + 2, selected),
                    None
                ),
                Some(Hit::Item(23))
            );
            assert_eq!(composer.text(), "kept draft");
            assert_eq!(composer.offer_selected(&waiting), Some(23));
        }
    }
}

#[test]
fn the_intelligence_guide_keeps_its_latest_feedback_visible_above_options() {
    for feedback in [
        "Your message is kept and will resume after your choice.",
        "Unknown provider: no selection changed.",
        "Help: /providers lists connections; /intelligence reopens this guide.",
    ] {
        let waiting = Waiting::selection(Asked::new(
            "Choose an AI connection",
            "",
            true,
            Shape::Choice(offers(18)),
            "guide",
            1,
        ));
        let mut state = state(Presentation::Focus, (80, 24), waiting.clone(), true);
        if let Waiting::Selection { asked } = &mut state.waiting {
            asked.retained = Some(Retained::new(
                Some("the earlier request".into()),
                Vec::new(),
            ));
        }
        state.transcript.push(Committed::new(Kind::Reply, feedback));
        let composer = composer(&waiting, "kept draft");
        let buffer = draw(&state, &composer);
        let screen = (0..24)
            .map(|y| row(&buffer, y))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(screen.contains(feedback), "{screen}");
        assert!(screen.contains("( ) label 0"), "{screen}");
    }
}

/// The intelligence list reads its own guide in its head, where the conversation keeps none
/// of it: an older reply of the conversation never stands for it (live gauntlet r02 to r09:
/// « Choose a connection below. No selection has changed. » stayed in the history).
#[test]
fn the_intelligence_list_reads_its_guide_in_its_head() {
    const GUIDE: &str = "Choose a connection below. No selection has changed.";
    let mut asked = Asked::new(
        "Choose a connection",
        "",
        false,
        Shape::Choice(offers(4)),
        "guide",
        1,
    );
    asked.guide = Some(GUIDE.to_owned());
    let waiting = Waiting::selection(asked);
    let mut state = state(Presentation::Focus, (80, 24), waiting.clone(), true);
    state.transcript.push(Committed::new(
        Kind::Reply,
        "An older reply of the conversation.",
    ));
    let composer = composer(&waiting, "");
    let buffer = draw(&state, &composer);
    let rows: Vec<String> = (0..24).map(|y| row(&buffer, y)).collect();
    let guided = rows.iter().position(|row| row.contains(GUIDE));
    let first = rows.iter().position(|row| row.contains("label 0"));
    assert!(
        guided
            .zip(first)
            .is_some_and(|(guided, first)| guided < first),
        "the guide above the rows: {rows:#?}"
    );
}

#[test]
fn grouped_choices_show_the_active_question_next_to_its_offers() {
    const DETAIL: &str = "Quel niveau de détail ?";
    const CONTENT: &str = "Quels éléments inclure ? (plusieurs possibles)";
    let whole = format!("{DETAIL}\nUn choix pour la longueur du rapport.\n{CONTENT}");
    for size in [(80, 24), (120, 40)] {
        for multiple in [false, true] {
            let (label, shape, first) = if multiple {
                (
                    CONTENT,
                    Shape::Multiple(vec![
                        Offer::new("revenue", "Chiffre d'affaires"),
                        Offer::new("clients", "Classement clients"),
                    ]),
                    "[ ] Chiffre d'affaires",
                )
            } else {
                (
                    DETAIL,
                    Shape::Choice(vec![
                        Offer::new("short", "Court"),
                        Offer::new("detailed", "Détaillé"),
                    ]),
                    "› 1 (●) Court",
                )
            };
            let witness = if multiple {
                "group:content"
            } else {
                "group:detail"
            };
            let asked = Asked::new(label, "", false, shape, witness, 1).retaining(Retained::new(
                Some("Prepare the sales report".into()),
                Vec::new(),
            ));
            let waiting = Waiting::asked("group", asked);
            let mut state = state(Presentation::Workspace, size, waiting.clone(), false);
            state.transcript.push(Committed::question(witness, &whole));
            let composer = composer(&waiting, "kept draft");
            let buffer = draw(&state, &composer);
            let shown: Vec<_> = (0..size.1).map(|y| row(&buffer, y)).collect();
            let first_offer = shown
                .iter()
                .position(|row| row.contains(first))
                .expect("the first active offer is visible");
            assert_eq!(
                shown[..first_offer]
                    .iter()
                    .rfind(|row| !row.trim().is_empty())
                    .expect("the question precedes its offers")
                    .trim(),
                label,
                "the active question belongs directly above its offers: {size:?}\n{shown:#?}"
            );
            assert!(
                shown
                    .iter()
                    .any(|row| row.contains("Request · Prepare the sales report"))
            );
            let full = super::question::asked_block(&state).expect("F2 keeps the full group");
            assert_eq!(state.transcript[full].text, whole);
            assert_eq!(composer.text(), "kept draft");
        }
    }
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
                    let landed = hit(&state, &composer, band, press, None);
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
        let mark = if ascii {
            "> 2 (*) label 1"
        } else {
            "› 2 (●) label 1"
        };
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

#[test]
fn multiple_checkboxes_and_explicit_submit_share_paint_and_hit_geometry() {
    for size in [(60, 18), (80, 24), (120, 40)] {
        for ascii in [false, true] {
            let asked = Asked::new(LABEL, "", true, Shape::Multiple(offers(9)), "m1", 1);
            let state = state(
                Presentation::Focus,
                size,
                Waiting::asked("sources", asked),
                ascii,
            );
            let mut composer = composer(&state.waiting, "ordinary draft");
            assert!(composer.select_offer(&state.waiting, 0));
            assert!(composer.select_offer(&state.waiting, 2));
            let buffer = draw(&state, &composer);
            let rows = (0..size.1).map(|y| row(&buffer, y)).collect::<Vec<_>>();
            assert!(
                rows.iter().any(|row| row.contains("[x] label 0")),
                "{rows:?}"
            );
            assert!(
                rows.iter().any(|row| row.contains("[ ] label 1")),
                "{rows:?}"
            );
            assert!(
                rows.iter().any(|row| row.contains("[x] label 2")),
                "{rows:?}"
            );
            assert!(!rows.iter().any(|row| row.contains("ordinary draft")));
            assert_eq!(
                composer.offer_key(&state.waiting, key(KeyCode::Tab)),
                crate::composer::answer::Offered::Moved
            );
            let buffer = draw(&state, &composer);
            let band = band_of(&state, &composer).expect("band");
            let y = (band.0.y..band.0.bottom())
                .find(|y| row(&buffer, *y).contains("Submit"))
                .expect("Submit remains reachable");
            assert!(row(&buffer, y).contains("Enter validates the checked answers"));
            let title = rows
                .iter()
                .find(|row| row.contains("Question"))
                .expect("title");
            assert!(title.contains("2 of 9 selected"), "{title}");
            assert_eq!(
                hit(
                    &state,
                    &composer,
                    band,
                    Position::new(band.0.x + 1, y),
                    None
                ),
                Some(Hit::Item(9))
            );
            assert_eq!(composer.text(), "ordinary draft");
            for y in 0..size.1 {
                if ascii {
                    assert!(row(&buffer, y).is_ascii());
                }
                for x in 0..size.0 {
                    assert_eq!(
                        (buffer[(x, y)].fg, buffer[(x, y)].bg),
                        (Color::Reset, Color::Reset)
                    );
                }
            }
        }
    }
}

#[test]
fn a_native_cost_decision_keeps_amount_bounds_and_its_full_context_at_eighty_columns() {
    let words = "Fresh Run cost decision · deepseek/fixture
USD cost is unknown; a charge is possible.
At most 1 request; at most 8192 output tokens and 120 seconds.
Overrides only the shown $0.25 default; no hard cap is overridden.
The complete route and input witness remain in this full question.
Continue once? yes / no / details";
    for ascii in [false, true] {
        let mut asked = Asked::new(
            "Continue this run once?",
            "",
            true,
            Shape::Choice(vec![
                Offer::new("yes", "Run once"),
                Offer::new("no", "Cancel this run"),
                Offer::new("details", "Read the full cost evidence"),
            ]),
            "action:9",
            9,
        );
        asked.free_text = false;
        let mut state = state(
            Presentation::Workspace,
            (80, 24),
            Waiting::Action {
                key: "run_cost".into(),
                asked,
            },
            ascii,
        );
        state.transcript.push(Committed::new(Kind::Question, words));
        let composer = composer(&state.waiting, "a draft kept apart");
        let buffer = draw(&state, &composer);
        let text = (0..24).map(|y| row(&buffer, y)).collect::<Vec<_>>().join(
            "
",
        );
        for fact in [
            "USD cost is unknown",
            "8192 output tokens",
            "120 seconds",
            "$0.25",
            "Run once",
            "Cancel this run",
            "Read the full cost evidence",
        ] {
            assert!(text.contains(fact), "{fact}:\n{text}");
        }
        assert_eq!(composer.offer_selected(&state.waiting), None);
        assert_eq!(composer.text(), "a draft kept apart");
        let index = crate::render::question::asked_block(&state).expect("F2 context");
        assert_eq!(
            state.transcript[index].text, words,
            "the F2 reader receives the exact owner block"
        );
    }
}

#[test]
fn finite_gate_and_judgment_heads_and_f2_read_the_same_current_owner_context() {
    for (key, kind, current) in [
        (
            "gate_choice",
            Kind::Gate,
            "Current gate: choose the output destination",
        ),
        (
            "judgment",
            Kind::Reply,
            "Qualification incomplete: the candidate remains unjudged",
        ),
    ] {
        let asked = Asked::new(
            "Choose an action",
            "",
            true,
            Shape::Choice(vec![Offer::new("retry", "First action")]),
            "action:context",
            22,
        );
        let mut state = state(
            Presentation::Focus,
            (80, 24),
            Waiting::Action {
                key: key.into(),
                asked,
            },
            false,
        );
        state
            .transcript
            .push(Committed::new(Kind::Question, "Obsolete currency question"));
        state
            .transcript
            .push(Committed::new(Kind::Gate, "Obsolete gate question"));
        state.transcript.push(Committed::new(kind, current));
        let expected = state.transcript.len() - 1;
        // A later block of another kind cannot become this decision's context.
        state.transcript.push(Committed::new(
            Kind::Question,
            "Other question unrelated to this decision",
        ));
        let composer = composer(&state.waiting, "retained draft");
        let buffer = draw(&state, &composer);
        let (band, _) = band_of(&state, &composer).expect("native decision band");
        let text = (band.y..band.bottom())
            .map(|y| row(&buffer, y))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains(current), "{key}: {text}");
        assert!(
            !text.contains("Obsolete") && !text.contains("Other question"),
            "{text}"
        );
        state.presentation = Presentation::Workspace;
        assert_eq!(crate::render::question::asked_block(&state), Some(expected));
        assert_eq!(state.transcript[expected].text, current);
        assert_eq!(composer.offer_selected(&state.waiting), None);
        assert_eq!(composer.text(), "retained draft");
    }
}

#[test]
fn the_gate_prompt_precedes_long_run_metadata_while_f2_keeps_every_word() {
    for (key, prompt, offers) in [
        (
            "gate_choice",
            "Choose the exact report label",
            vec![
                Offer::new("\"true\"", "true"),
                Offer::new("\"1\"", "1"),
                Offer::new("\"  spaced label  \"", "  spaced label  "),
                Offer::new("\"/quit\"", "/quit"),
            ],
        ),
        (
            "gate",
            "Write the final report?",
            vec![
                Offer::new("yes", "Approve this step"),
                Offer::new("no", "Refuse this step"),
            ],
        ),
    ] {
        let context = format!(
            "run observed: exit 4\nPaused for a human answer\nResume the exact trace with its answer\nTrace: .nika/traces/a-long-run-identity.ndjson\nWorkflow: report.nika\nEarlier tasks and evidence remain recorded\n{prompt}\nDownstream: record writes the selected label"
        );
        let labels = offers
            .iter()
            .map(|offer| offer.label.trim().to_owned())
            .collect::<Vec<_>>();
        let mut asked = Asked::new(prompt, "", true, Shape::Choice(offers), "action:prompt", 23);
        asked.free_text = false;
        let mut state = state(
            Presentation::Focus,
            (80, 24),
            Waiting::Action {
                key: key.into(),
                asked,
            },
            false,
        );
        state.transcript.push(Committed::new(Kind::Gate, &context));
        let composer = composer(&state.waiting, "retained draft");
        let buffer = draw(&state, &composer);
        let (band, _) = band_of(&state, &composer).expect("native gate band");
        let rows = (band.y..band.bottom())
            .map(|y| row(&buffer, y))
            .collect::<Vec<_>>();
        let prompt_row = rows
            .iter()
            .position(|line| line.contains(prompt))
            .expect("visible prompt");
        let metadata_row = rows
            .iter()
            .position(|line| line.contains("run observed"))
            .expect("metadata");
        assert!(
            prompt_row < metadata_row,
            "the exact question comes first: {rows:?}"
        );
        let text = rows.join("\n");
        for label in labels {
            assert!(text.contains(&label), "{label}: {text}");
        }
        assert_eq!(composer.offer_selected(&state.waiting), None);
        state.presentation = Presentation::Workspace;
        let index = crate::render::question::asked_block(&state).expect("F2 gate");
        assert_eq!(state.transcript[index].text, context);
    }
}

/// The real stacked workspace geometry: an existing run object and pinned run,
/// with only eight rows left for the native decision above the fixed composer.
fn gate_workspace_frame(state: &UiState, composer: &Composer) -> (Buffer, (Rect, bool)) {
    use crate::workspace::{
        aside::{Aside, Tab},
        conversation::Thread,
        focus::Focus,
        geometry::Geometry,
        header::Place,
        object::{Object, Paint},
        pinned::Pinned,
        screen::{self, Screen},
    };
    let screen = Screen::new(
        Place::on("local").with_project("reports", "/project"),
        Aside::new("reports", Tab::Nika, Vec::new(), true),
        Object::Shown {
            icon: crate::visual::icon::Icon::Run,
            name: "run paused".into(),
            lines: [
                "choose · paused",
                "invoke · nika:prompt",
                "",
                "record · scheduled",
                "invoke · nika:write",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        },
        Thread::new("reports", "this conversation"),
    )
    .pinning(Pinned::new(
        "reports",
        "choice.nika",
        "run",
        nika_display::state::TaskState::Paused,
        "a gate asks you",
    ));
    let paint = Paint {
        ascii: state.ascii,
        color: false,
        elapsed: std::time::Duration::ZERO,
        reduced_motion: true,
    };
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
    terminal
        .draw(|frame| {
            assert!(screen::draw(
                frame,
                &screen,
                paint,
                &Focus::composing(),
                state,
                composer
            ));
        })
        .expect("draw workspace");
    let geometry = Geometry::of(Rect::new(0, 0, 80, 24), true).expect("stacked workspace");
    let band = screen::band(&geometry, state, composer, &screen).expect("gate band");
    assert_eq!(
        band.0.height, 8,
        "the real compact conversation, not the full-screen focus view"
    );
    (terminal.backend().buffer().clone(), band)
}

fn compact_gate_offers(key: &str) -> Vec<Offer> {
    if key == "gate_choice" {
        vec![
            Offer::new("\"true\"", "true"),
            Offer::new("\"1\"", "1"),
            Offer::new("\"  spaced label  \"", "  spaced label  "),
            Offer::new("\"/quit\"", "/quit"),
        ]
    } else {
        vec![
            Offer::new("yes", "Approve this step"),
            Offer::new("no", "Refuse this step"),
        ]
    }
}

#[test]
fn the_compact_workspace_keeps_the_gate_question_and_every_offer_reachable() {
    const PROMPT: &str = "Choose the exact report label";
    for key in ["gate_choice", "gate"] {
        for (long, ascii) in [(false, false), (false, true), (true, false), (true, true)] {
            let prompt = if long {
                format!(
                    "{PROMPT}: {}",
                    "retain the selected report label and its exact spelling. ".repeat(6)
                )
            } else {
                PROMPT.to_owned()
            };
            let offers = compact_gate_offers(key);
            let labels: Vec<_> = offers.iter().map(|offer| offer.label.clone()).collect();
            let mut asked = Asked::new(
                &prompt,
                "",
                true,
                Shape::Choice(offers),
                "action:compact",
                24,
            );
            asked.free_text = false;
            let mut state = state(
                Presentation::Workspace,
                (80, 24),
                Waiting::Action {
                    key: key.into(),
                    asked,
                },
                ascii,
            );
            let context = format!(
                "run observed: exit 4\nPaused for a human answer\nResume the exact trace with its answer\nTrace: .nika/traces/run.ndjson\nWorkflow: choice.nika\nEarlier tasks remain recorded\n{prompt}\nDownstream: record writes the selected label"
            );
            state.transcript.push(Committed::new(Kind::Gate, &context));
            let mut composer = composer(&state.waiting, "retained draft");
            let (buffer, band) = gate_workspace_frame(&state, &composer);
            let first = row(&buffer, band.0.y + 1);
            assert!(
                first.starts_with(PROMPT),
                "active question precedes metadata: {first}"
            );
            let text = (band.0.y..band.0.bottom())
                .map(|y| row(&buffer, y))
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                text.contains("F2"),
                "full question stays accessible: {text}"
            );
            for (index, label) in labels.iter().enumerate() {
                let y = (band.0.y..band.0.bottom())
                    // A decision's act wears the digit that selects it.
                    .find(|y| row(&buffer, *y) == format!("  {} ( ) {label}", index + 1).trim_end())
                    .expect("every exact offer remains visible");
                assert_eq!(
                    hit(
                        &state,
                        &composer,
                        band,
                        Position::new(band.0.x + 2, y),
                        None
                    ),
                    Some(Hit::Item(index))
                );
            }
            assert_eq!(composer.offer_selected(&state.waiting), None);
            select(&mut composer, &state.waiting, labels.len() - 1);
            assert_eq!(
                composer.offer_selected(&state.waiting),
                Some(labels.len() - 1)
            );
            assert_eq!(composer.text(), "retained draft");
            let owner = super::question::asked_block(&state).expect("F2 keeps the gate context");
            assert_eq!(state.transcript[owner].text, context);
        }
    }
}

/// The question the Session asks of a reviewed proposal.
const SAVE_LABEL: &str = "What should happen to this reviewed proposal?";

/// The save decision on a reviewed proposal as the Session asks it: its four offers and typed
/// words that describe a change; it names no request (the card reads the proposal's own).
fn save_decision() -> Waiting {
    let offers = vec![
        Offer::new("yes", "Save workflow"),
        Offer::new("save & run", "Save and run once"),
        Offer::new("no", "Discard this proposal"),
        Offer::new("/show", "Inspect the exact changes"),
    ];
    let mut asked = Asked::new(
        SAVE_LABEL,
        "",
        true,
        Shape::Choice(offers),
        "action:save",
        7,
    );
    asked.free_text = true;
    Waiting::Action {
        key: "save".into(),
        asked,
    }
}

/// A one-change candidate as the save journeys propose one: what it creates, what it reads when
/// it runs, a reach that leaves this machine, no rehearsal bound.
fn single_change_review() -> Review {
    let look = Inspected::unjudged(
        "compiled-workflow.nika",
        "51835c93e564aaaabbbbccccddddeeeeffff0000".to_owned(),
        "nika: copy\n".to_owned(),
    );
    Proposed::new(fixture::id(), false, look)
        .changing(vec![fixture::CREATES.to_owned()])
        .reaching(Some(vec![fixture::READS.to_owned()]))
        .declaring(vec![(fixture::REACH.to_owned(), true)])
        .review(false)
        .expect("a review a consent can name")
}

/// The real workspace of `state`, its conversation reviewing `review`, a run pinned or not: the
/// frame and the band the conversation leaves the decision.
fn consent_frame(
    state: &UiState,
    composer: &Composer,
    review: &Review,
    pinned: bool,
) -> (Buffer, (Rect, bool)) {
    use crate::workspace::{
        aside::{Aside, Tab},
        conversation::Thread,
        focus::Focus,
        geometry::Geometry,
        header::Place,
        object::{Object, Paint},
        pinned::Pinned,
        screen::{self, Screen},
    };
    let screen = Screen::new(
        Place::on("local").with_project("reports", "/project"),
        Aside::new("reports", Tab::Nika, Vec::new(), true),
        Object::Shown {
            icon: crate::visual::icon::Icon::Run,
            name: "compiled-workflow.nika".into(),
            lines: vec!["read · ./notes/brief.md".to_owned()],
        },
        Thread::new("reports", "this conversation"),
    )
    .reviewing(Some(review.clone()));
    let screen = if pinned {
        screen.pinning(Pinned::new(
            "reports",
            "choice.nika",
            "run",
            nika_display::state::TaskState::Paused,
            "a gate asks you",
        ))
    } else {
        screen
    };
    let paint = Paint {
        ascii: state.ascii,
        color: false,
        elapsed: std::time::Duration::ZERO,
        reduced_motion: true,
    };
    let (width, tall) = state.size;
    let mut terminal = Terminal::new(TestBackend::new(width, tall)).expect("terminal");
    terminal
        .draw(|frame| {
            assert!(screen::draw(
                frame,
                &screen,
                paint,
                &Focus::composing(),
                state,
                composer
            ));
        })
        .expect("draw workspace");
    let geometry = Geometry::of(Rect::new(0, 0, width, tall), pinned).expect("a workspace");
    let band = screen::band(&geometry, state, composer, &screen).expect("the decision band");
    (terminal.backend().buffer().clone(), band)
}

/// The rows of `band` in `buffer`, its own cells only.
fn band_rows(buffer: &Buffer, (band, _): (Rect, bool)) -> Vec<String> {
    (band.y..band.bottom())
        .map(|y| {
            (band.x..band.right())
                .map(|x| buffer[(x, y)].symbol())
                .collect()
        })
        .collect()
}

/// A decision that comes back after the person wrote beside it keeps what Nika said since in
/// view: the band covers the conversation, so its head opens on Nika's reply, then says the
/// proposal is the one from before that message, above the facts a consent decides. Live
/// gauntlet r09 B1: the reply sat behind the card, found only by scrolling once the run was
/// over.
#[test]
fn the_card_keeps_what_nika_said_since_the_persons_line_in_view() {
    let review = (fixture::candidate(fixture::id(), false).review(false))
        .expect("a review a consent can name");
    let reply = "Je ne peux pas vous montrer ce workflow tel quel.";
    for size in [(120, 36), (80, 24)] {
        let mut state = state(Presentation::Workspace, size, save_decision(), false);
        state
            .transcript
            .push(Committed::proposal(fixture::id(), fixture::PREVIEW));
        state.transcript.push(Committed::new(
            Kind::Human,
            "Je veux un budget max de 0,10 $ par run.",
        ));
        state.transcript.push(Committed::new(Kind::Reply, reply));
        let composer = composer(&state.waiting, "");
        let (buffer, band) = consent_frame(&state, &composer, &review, false);
        let text = band_rows(&buffer, band).join("\n");
        // The words as read, across the rows they wrap over.
        let read = (text.split_whitespace())
            .filter(|word| *word != "│")
            .collect::<Vec<_>>()
            .join(" ");
        assert!(read.contains(reply), "{size:?}\n{text}");
        assert!(
            read.contains("this proposal is unchanged since your last message"),
            "{size:?}\n{text}"
        );
        assert!(
            read.contains("Save workflow"),
            "{size:?}: the acts stay\n{text}"
        );
        // In the person's language, the card says so in French.
        state.lang = crate::lang::Lang::Fr;
        let (buffer, band) = consent_frame(&state, &composer, &review, false);
        let text = band_rows(&buffer, band).join("\n");
        let read = (text.split_whitespace())
            .filter(|word| *word != "│")
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            read.contains("cette proposition n'a pas changé depuis votre dernier message"),
            "{size:?}\n{text}"
        );
    }
}

/// An act the decision offers, picked, asked for no change: « Inspect the exact changes » right
/// after the proposal never makes the card say it is unchanged since the person's message.
#[test]
fn a_picked_act_is_no_message_the_proposal_stayed_unchanged_for() {
    let review = (fixture::candidate(fixture::id(), false).review(false))
        .expect("a review a consent can name");
    let mut state = state(Presentation::Workspace, (120, 36), save_decision(), false);
    state
        .transcript
        .push(Committed::proposal(fixture::id(), fixture::PREVIEW));
    state
        .transcript
        .push(Committed::new(Kind::Human, "Inspect the exact changes"));
    let composer = composer(&state.waiting, "");
    let (buffer, band) = consent_frame(&state, &composer, &review, false);
    let text = band_rows(&buffer, band).join("\n");
    assert!(!text.contains("unchanged since"), "{text}");
}

/// A long reply keeps its first row and its latest words above the card, its Markdown marks
/// read as the words they carry: the question that ends it is what the person answers.
#[test]
fn a_long_reply_keeps_its_latest_words_above_the_card() {
    let review = (fixture::candidate(fixture::id(), false).review(false))
        .expect("a review a consent can name");
    let reply = "**Je ne peux pas** vous montrer ce workflow tel quel.\n\n\
        Le moteur ne connaît pas de budget en dollars appliqué à l'exécution, mais il borne \
        chaque appel par un nombre de jetons, ce qui revient au même pour un modèle connu.\n\n\
        Si cette garantie vous suffit, dites-le et je vous montre le workflow ainsi.";
    for size in [(120, 36), (80, 24)] {
        let mut state = state(Presentation::Workspace, size, save_decision(), false);
        state
            .transcript
            .push(Committed::proposal(fixture::id(), fixture::PREVIEW));
        state.transcript.push(Committed::new(
            Kind::Human,
            "Je veux un budget max de 0,10 $ par run.",
        ));
        state.transcript.push(Committed::new(Kind::Reply, reply));
        let composer = composer(&state.waiting, "");
        let (buffer, band) = consent_frame(&state, &composer, &review, false);
        let text = band_rows(&buffer, band).join("\n");
        let read = (text.split_whitespace())
            .filter(|word| *word != "│")
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            read.contains("Je ne peux pas"),
            "{size:?}: its first row\n{text}"
        );
        assert!(
            read.contains("je vous montre le workflow ainsi."),
            "{size:?}: its latest words\n{text}"
        );
        assert!(!text.contains("**"), "{size:?}: no Markdown mark\n{text}");
        assert!(
            read.contains("this proposal is unchanged since your last message"),
            "{size:?}: the note stays whole\n{text}"
        );
        assert!(
            read.contains("Save workflow"),
            "{size:?}: the acts stay\n{text}"
        );
    }
}

/// The save decision keeps what its consent decides in view at every size the save journeys
/// read: the band over the conversation names the identity a yes answers, whole beside its
/// reader key, the standing, what the proposal creates and what it reaches when it runs; its
/// declared reach and its rehearsal where the band holds every fact; the question only where
/// every fact holds first, and never a request the decision does not answer. Its first offers
/// stay listed under them.
#[test]
fn the_save_decision_band_keeps_the_proposal_in_view_at_the_journeys_sizes() {
    let review = (fixture::candidate(fixture::id(), false).review(false))
        .expect("a review a consent can name");
    let foot = format!("{} · F2", fixture::PATH);
    // The size, whether every fact shows, whether the question shows.
    for (size, every, lead) in [
        ((120, 36), true, true),
        ((80, 24), false, false),
        ((80, 30), true, false),
        ((99, 30), true, false),
        ((80, 40), true, true),
        ((120, 40), true, true),
    ] {
        let mut state = state(Presentation::Workspace, size, save_decision(), false);
        state
            .transcript
            .push(Committed::proposal(fixture::id(), fixture::PREVIEW));
        let composer = composer(&state.waiting, "");
        let (buffer, band) = consent_frame(&state, &composer, &review, false);
        let text = band_rows(&buffer, band).join("\n");
        let mut kept = vec![
            foot.as_str(),
            "Not saved yet",
            "creates compiled-workflow.nika",
            "when it runs",
            "Save workflow",
            "Discard this proposal",
        ];
        if every {
            kept.extend(["reaches, as declared", "rehearsal"]);
        }
        for fact in kept {
            assert!(text.contains(fact), "{size:?}: {fact}\n{text}");
        }
        let asks = "What should happen";
        assert_eq!(text.contains(asks), lead, "{size:?}: {asks}\n{text}");
        assert!(
            !text.contains("Request · "),
            "{size:?}: no stale request\n{text}"
        );
    }
}

/// The compact workspace keeps what a consent decides in view: with a run pinned at 80x24 the
/// decision has eight rows, and its head yields the question before the identity
/// a yes names (whole beside its reader key), the standing, what the proposal creates and what it
/// reaches when it runs; at 60x18 the identity, the standing and the change; at 120x40 every fact
/// with the question. On every page each offer is painted where a press lands on
/// it with the same consent, the draft stays, and F2 reads the current proposal's own words.
#[test]
fn the_compact_workspace_keeps_the_consents_identity_effects_and_every_offer() {
    let review = single_change_review();
    let foot = format!("{} · F2", fixture::PATH);
    let labels = [
        "Save workflow",
        "Save and run once",
        "Discard this proposal",
        "Inspect the exact changes",
    ];
    let compact = [
        foot.as_str(),
        "Not saved yet",
        "creates compiled-workflow.nika",
        "when it runs",
        "reaches, as declared",
    ];
    let small = [
        foot.as_str(),
        "Not saved yet",
        "creates compiled-workflow.nika",
    ];
    let wide = [
        "What should happen",
        foot.as_str(),
        "Not saved yet",
        "creates compiled-workflow.nika",
        "when it runs",
        "reaches, as declared",
        "rehearsal",
    ];
    for (size, pinned, kept) in [
        ((80, 24), true, &compact[..]),
        ((60, 18), false, &small[..]),
        ((120, 40), true, &wide[..]),
    ] {
        let mut state = state(Presentation::Workspace, size, save_decision(), false);
        state
            .transcript
            .push(Committed::proposal(fixture::id(), fixture::PREVIEW));
        let mut composer = composer(&state.waiting, "hold for review");
        let (buffer, band) = consent_frame(&state, &composer, &review, pinned);
        if size == (80, 24) {
            assert_eq!(
                band.0.height, 8,
                "the compact conversation with a run pinned"
            );
        }
        let text = band_rows(&buffer, band).join("\n");
        for fact in kept {
            assert!(text.contains(fact), "{size:?}: {fact}\n{text}");
        }
        if size != (120, 40) {
            assert!(
                !text.contains("What should happen"),
                "{size:?}: the question yields first\n{text}"
            );
        }
        let consent = crate::workspace::cards::review::consent(&state, Some(&review));
        for (index, label) in labels.iter().enumerate() {
            select(&mut composer, &state.waiting, index);
            let (buffer, band) = consent_frame(&state, &composer, &review, pinned);
            let at = (band_rows(&buffer, band).iter())
                .position(|row| row.contains(label))
                .expect("each offer painted on its page");
            let y = band.0.y + u16::try_from(at).expect("a band row");
            assert_eq!(
                hit(
                    &state,
                    &composer,
                    band,
                    Position::new(band.0.x + 2, y),
                    consent
                ),
                Some(Hit::Item(index)),
                "{size:?}: {label}"
            );
        }
        assert_eq!(composer.text(), "hold for review");
        assert_eq!(
            super::question::asked_block(&state),
            None,
            "save reads the proposal"
        );
        let (at, read) = summarized(&state, Some(&review)).expect("F2 reads the current proposal");
        assert_eq!(state.transcript[at].text, fixture::PREVIEW);
        assert_eq!(read.id(), &fixture::id());
    }
}

/// Where even its shortest form is wider than the band, the name of the proposal a consent
/// answers wraps on rows of its own: it reads whole across them, never cropped.
#[test]
fn a_narrow_consent_band_wraps_the_proposals_name_whole() {
    let review = single_change_review();
    let mut state = state(Presentation::Workspace, (80, 24), save_decision(), false);
    state
        .transcript
        .push(Committed::proposal(fixture::id(), fixture::PREVIEW));
    let composer = composer(&state.waiting, "");
    let band = (Rect::new(0, 0, 10, 24), false);
    let mut buffer = Buffer::empty(band.0);
    super::surface::render(&state, &composer, band, &mut buffer, Some(&review));
    let read: String = (0..24).map(|y| row(&buffer, y)).collect();
    assert!(read.contains(fixture::PATH), "{read}");
}

/// What the Session said above a proposal (how it read an answer, a model it chose for the
/// person) reads above the decision's question: whole where the band holds it, and where it is
/// short one row of it, cut with a visible mark, before the last facts, the identity, the standing
/// and the change kept. A proposal without a change set's first line shows no note.
#[test]
fn the_decision_reads_what_the_session_said_above_the_proposal() {
    let review = single_change_review();
    let read = "read your answer as « 09:00 » (from « tout les matins a 9h » — say the value \
                alone if that is not it)";
    let chose = "Nika chose « anthropic/claude-sonnet-5-5 », the model offered here, for \
                 « choisis toi-même » — name another model to change it";
    let words = format!(
        "{read}\n{chose}\nproposed change · copy the brief\n  creates `compiled-workflow.nika` \
         (33 lines)"
    );
    let foot = format!("{} · F2", fixture::PATH);
    for (size, pinned, whole) in [
        ((180, 48), false, true),
        ((120, 40), false, true),
        ((80, 24), true, false),
    ] {
        let mut state = state(Presentation::Workspace, size, save_decision(), false);
        state
            .transcript
            .push(Committed::proposal(fixture::id(), words.clone()));
        let composer = composer(&state.waiting, "");
        let (buffer, band) = consent_frame(&state, &composer, &review, pinned);
        let rows = band_rows(&buffer, band);
        let text = rows.join("\n");
        let at = |said: &str| (rows.iter()).position(|row| row.contains(said));
        let found = |said: &str| {
            let row = at(said);
            assert!(row.is_some(), "{size:?}: {said}\n{text}");
            row.unwrap_or_default()
        };
        // The band is the conversation's column: a note wraps on its words.
        let first = found("read your answer as");
        let identity = found(&foot);
        assert!(first < identity, "{size:?}: above the card\n{text}");
        for fact in ["Not saved yet", "creates compiled-workflow.nika"] {
            assert!(text.contains(fact), "{size:?}: {fact}\n{text}");
        }
        if whole {
            let chosen = found("Nika chose «");
            let question = found("What should happen");
            assert!(chosen < question, "{size:?}: the notes first\n{text}");
            assert!(text.contains("name another model"), "{text}");
        } else {
            assert!(
                rows.get(first)
                    .is_some_and(|row| row.trim_end().ends_with('…')),
                "{size:?}: a cut note marks it\n{text}"
            );
        }
    }
    let mut plain = state(Presentation::Workspace, (120, 40), save_decision(), false);
    plain
        .transcript
        .push(Committed::proposal(fixture::id(), fixture::PREVIEW));
    let composer = composer(&plain.waiting, "");
    let (buffer, band) = consent_frame(&plain, &composer, &review, false);
    let text = band_rows(&buffer, band).join("\n");
    assert!(!text.contains("digest-notes"), "no note told apart\n{text}");
    // The Session's own review of the proposal is the proposal, never a note above it.
    let reviewed = format!(
        "{read}\nNika proposes `compiled-workflow.nika`:\nDoes\n1. read_source · reads a file\n\
         Runs\nwhen you ask (« run it ») · no schedule was asked\nproposed change · copy the brief"
    );
    let mut state = state(Presentation::Workspace, (120, 40), save_decision(), false);
    state
        .transcript
        .push(Committed::proposal(fixture::id(), reviewed));
    let (buffer, band) = consent_frame(&state, &composer, &review, false);
    let text = band_rows(&buffer, band).join("\n");
    assert!(text.contains("read your answer as"), "{text}");
    for review in [
        "Nika proposes",
        "read_source · reads a file",
        "no schedule was asked",
    ] {
        assert!(
            !text.contains(review),
            "{review} is the proposal's own\n{text}"
        );
    }
    assert!(text.contains("What should happen"), "{text}");
}

mod choices;
mod expiry;
mod filter;
mod rich;
mod wrapped;
