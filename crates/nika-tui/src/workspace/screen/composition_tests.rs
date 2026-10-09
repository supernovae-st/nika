// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The approved composition of a workspace frame (the October 8 reference):
//! one quiet header row with the brand, the project, its host and the
//! preparation intelligence at its right end (the folded regions first on a
//! narrow terminal), a quiet conversation title with no « Prepare with »
//! paragraph under it, no row for an empty attachment context, and one
//! bounded composer under its caption where the panel has the rows for it,
//! the plain composer elsewhere.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use unicode_width::UnicodeWidthStr;

use super::*;
use crate::model::{Asked, Committed, Kind, Presentation, Shape, Waiting};
use crate::visual::icon::Icon;
use crate::visual::logomark::REVEAL_ENDS;
use crate::workspace::aside::{Entry, Tab};
use crate::workspace::header::Manifest;

/// The selection the header names, as the Session projects it.
const SEAT: &str = "deepseek/chosen - deepseek API, metered";
/// A draft left unsent in the composer.
const DRAFT: &str = "keep this exact draft";
/// What the folded header names while the conversation holds the keys.
const NAMES: &str = "Project [Conversation] Object";

/// The studio project, its conversation and an opened workflow, `seat`
/// selected for preparation (or nothing selected), no run pinned.
fn studio(seat: Option<&str>) -> Screen {
    let place = Place::on("local")
        .with_project("studio", "~/Projects/studio")
        .observed(true, true);
    let aside = Aside::new(
        "studio",
        Tab::Nika,
        vec![
            Entry::new(Icon::Conversation, "this conversation").opened(),
            Entry::new(Icon::Workflow, "release.nika"),
        ],
        true,
    );
    let mut thread = Thread::new("studio", "this conversation").viewing("release.nika");
    thread.intelligence = seat.map(str::to_owned);
    let object = Object::Shown {
        icon: Icon::Workflow,
        name: "release.nika".to_owned(),
        lines: vec!["nika: release".to_owned()],
    };
    Screen::new(place, aside, object, thread)
}

/// A workspace at `size` waiting as `waiting`, after a short exchange.
fn state(size: (u16, u16), ascii: bool, color: bool, waiting: Waiting) -> UiState {
    let mut state = UiState::new(Presentation::Workspace, color, size);
    state.ascii = ascii;
    state.waiting = waiting;
    state
        .transcript
        .push(Committed::new(Kind::Human, "draft a weekly digest"));
    state.transcript.push(Committed::new(
        Kind::Question,
        "Which file holds the notes to digest?",
    ));
    state
}

/// The workspace's composer holding `draft`.
fn composer(draft: &str, ascii: bool) -> Composer {
    let mut composer = Composer::new();
    composer.set_placeholder(conversation::invitation(ascii));
    composer.paste(draft);
    composer
}

fn paint(ascii: bool, color: bool) -> Paint {
    Paint {
        ascii,
        color,
        elapsed: REVEAL_ENDS,
        reduced_motion: false,
    }
}

/// The object restored (the arrangement a workspace opens in).
fn restored() -> Arrangement {
    Arrangement::of(geometry::Layout::Session)
}

/// The text of the cells `from..to` of row `y`, a wide glyph once.
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

/// One frame of `screen`: its rows and its buffer.
fn frame_of(
    screen: &Screen,
    state: &UiState,
    composer: &Composer,
    paint: Paint,
    arrangement: Arrangement,
) -> (Vec<String>, Buffer) {
    let (width, height) = state.size;
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    let chrome = Chrome {
        arrangement,
        dragging: None,
        folded: false,
    };
    terminal
        .draw(|frame| {
            let focus = Focus::composing();
            assert!(draw_in(
                frame, screen, paint, &focus, state, composer, chrome
            ));
        })
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    let rows = (0..height)
        .map(|y| row_text(&buffer, y, 0, width))
        .collect();
    (rows, buffer)
}

/// The conversation's rectangles, as painting and scrolling share them.
fn areas(geometry: &Geometry, state: &UiState, composer: &Composer, thread: &Thread) -> [Rect; 4] {
    panel_areas(geometry, state, composer, thread)
}

/// What the conversation region shows, row by row.
fn conversation_text(buffer: &Buffer, geometry: &Geometry) -> String {
    let talk = geometry.conversation;
    (talk.y..talk.bottom())
        .map(|y| row_text(buffer, y, talk.x, talk.right()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The rows of `area`, as painted.
fn rows_of(buffer: &Buffer, area: Rect) -> Vec<String> {
    (area.y..area.bottom())
        .map(|y| row_text(buffer, y, area.x, area.right()))
        .collect()
}

/// From 120 columns the header is one quiet row: the brand, the project and
/// its host, then the preparation intelligence at the row's right end. A
/// two-row header's second row is a thin separator, never the location or a
/// status wall (`/status` keeps them); no chevron offers a menu the
/// workspace does not open, and the intelligence has this one home.
#[test]
fn the_header_is_one_quiet_row_ending_with_the_preparation_intelligence() {
    for (width, height) in [(120, 40), (180, 48), (120, 24)] {
        for ascii in [false, true] {
            let at = format!("{width}x{height} ascii={ascii}");
            let screen = studio(Some(SEAT));
            let state = state((width, height), ascii, false, Waiting::Free);
            let composer = composer(DRAFT, ascii);
            let (rows, _) = frame_of(&screen, &state, &composer, paint(ascii, false), restored());
            let brand = if ascii { "NIKA" } else { "◆ NIKA" };
            assert!(rows[0].starts_with(brand), "{at}: {}", rows[0]);
            assert!(
                rows[0].contains("studio") && rows[0].contains("local project"),
                "{at}: {}",
                rows[0]
            );
            // The words end one blank cell before the edge.
            let words = format!("Prepare with: {SEAT} ");
            assert!(rows[0].ends_with(&words), "{at}: {}", rows[0]);
            let shown = rows.join("\n");
            assert_eq!(shown.matches("Prepare with").count(), 1, "{at}\n{shown}");
            assert!(!shown.contains("~/Projects/studio"), "{at}\n{shown}");
            assert!(!rows[0].contains(['⌄', '▾']), "{at}: {}", rows[0]);
            if height >= 30 {
                let rule = if ascii { "-" } else { "─" };
                assert_eq!(
                    rows[1],
                    rule.repeat(usize::from(width)),
                    "{at}: a separator"
                );
            }
        }
    }
}

/// While the project list is folded the header's first row ends with the
/// three regions' names, exactly where a press finds them; the intelligence
/// keeps its one home left of them, on the second row where the first has no
/// room for it whole, else cut with the cut mark beside the `/status`
/// details command. Nothing selected is still said: « not selected ».
#[test]
fn folded_navigation_comes_first_and_the_intelligence_keeps_one_home() {
    for (width, height) in [(60, 16), (80, 24), (99, 30), (100, 32)] {
        for seat in [Some(SEAT), None] {
            for ascii in [false, true] {
                let at = format!("{width}x{height} {seat:?} ascii={ascii}");
                let screen = studio(seat);
                let state = state((width, height), ascii, false, Waiting::Free);
                let composer = composer(DRAFT, ascii);
                let (rows, buffer) =
                    frame_of(&screen, &state, &composer, paint(ascii, false), restored());
                assert!(rows[0].ends_with(NAMES), "{at}: {}", rows[0]);
                let geometry = Geometry::of(Rect::new(0, 0, width, height), false).expect("fits");
                let names = regions_area(&screen.place, geometry.header, ascii).expect("names");
                let painted = row_text(&buffer, names.y, names.x, names.right());
                assert_eq!(painted, NAMES, "{at}: paint and press read one answer");
                let header = rows[..usize::from(geometry.header.height)].join("\n");
                let talk = conversation_text(&buffer, &geometry);
                assert!(!talk.contains("Prepare with"), "{at}\n{talk}");
                assert!(!talk.contains("not selected"), "{at}\n{talk}");
                let Some(seat) = seat else {
                    assert!(header.contains("not selected"), "{at}\n{header}");
                    continue;
                };
                let whole = format!("Prepare with: {seat}");
                if height >= 30 {
                    assert!(header.contains(&whole), "{at}\n{header}");
                } else {
                    let cut = header.contains(&whole) || header.contains("/status");
                    assert!(cut, "{at}\n{header}");
                }
                let mark = if ascii { "..." } else { "…" };
                if !header.contains(&whole)
                    && let Some((_, rest)) = header.split_once("Prepare with: ")
                {
                    let kept = rest.split(mark).next().unwrap_or_default();
                    assert!(seat.starts_with(kept), "{at}: {kept}");
                }
            }
        }
    }
}

/// The header's words keep one blank cell before the edge, the project and a
/// refused `nika.yaml` included, and the seat they end with wears the
/// header's own ink: a fact, never the hue of a control it is not.
#[test]
fn the_header_keeps_air_before_the_edge_and_the_seat_wears_its_ink() {
    for (width, height) in [(120, 40), (180, 48)] {
        let at = format!("{width}x{height}");
        let mut screen = studio(Some(SEAT));
        screen.place = screen.place.clone().governed(Manifest::Refused);
        let state = state((width, height), false, true, Waiting::Free);
        let composer = composer(DRAFT, false);
        let (rows, buffer) = frame_of(&screen, &state, &composer, paint(false, true), restored());
        assert!(rows[0].ends_with(&format!("{SEAT} ")), "{at}: {}", rows[0]);
        for words in ["studio", "nika.yaml refused", "Prepare with: "] {
            assert!(rows[0].contains(words), "{at}: {}", rows[0]);
        }
        let seat = width - 1 - u16::try_from(SEAT.width()).expect("cells");
        for x in seat..width - 1 {
            assert_eq!(
                Some(buffer[(x, 0)].fg),
                role::surface(true, true).fg,
                "{at}: {x}"
            );
        }
    }
}

/// A one-row header too narrow for the whole seat never shows it cut inside
/// a word nor without its label: its first whole words under « Prepare
/// with » beside `/status`, else `/status` alone, which says it whole.
#[test]
fn a_narrow_header_cuts_the_seat_between_words_under_its_label() {
    let seat = "none, the engine facts answer";
    for (width, height) in [(80, 24), (60, 16)] {
        for name in ["studio", "nika-ws-colored-graph-isb5ZE"] {
            for ascii in [false, true] {
                let at = format!("{width}x{height} {name} ascii={ascii}");
                let mut screen = studio(Some(seat));
                screen.place = Place::on("local")
                    .with_project(name, format!("~/Projects/{name}"))
                    .observed(true, true);
                let state = state((width, height), ascii, false, Waiting::Free);
                let composer = composer(DRAFT, ascii);
                let (rows, _) =
                    frame_of(&screen, &state, &composer, paint(ascii, false), restored());
                let mark = if ascii { "..." } else { "…" };
                if let Some((_, rest)) = rows[0].split_once("Prepare with: ") {
                    let kept = rest.split(mark).next().unwrap_or_default();
                    let after = seat.strip_prefix(kept).unwrap_or_default();
                    assert!(after.starts_with(' '), "{at}: a cut word: {}", rows[0]);
                    assert!(rest.contains("/status"), "{at}: {}", rows[0]);
                } else {
                    assert!(rows[0].contains("/status"), "{at}: {}", rows[0]);
                    assert!(!rows[0].contains("none"), "{at}: an unlabelled seat");
                }
            }
        }
    }
}

/// Beside the object the conversation's title is quiet: its own name, then
/// a thin rule across the panel, and no « Prepare with » paragraph repeats
/// the header. Separators keep the quiet border weight (a dim weight without
/// colour): the panel's edge and the project's edge never wear the accent
/// nor the brighter secondary text, whichever region holds the keys.
#[test]
fn the_conversation_title_is_quiet_and_separators_stay_dim() {
    let dim = role::border(true).fg;
    for (width, height) in [(100, 32), (120, 40), (180, 48)] {
        for region in [Region::Conversation, Region::Object, Region::Aside] {
            let at = format!("{width}x{height} {region:?}");
            let screen = studio(Some(SEAT));
            let state = state((width, height), false, true, Waiting::Free);
            let composer = composer(DRAFT, false);
            let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
            let mut focus = Focus::composing();
            focus.region = region;
            let chrome = Chrome {
                arrangement: restored(),
                dragging: None,
                folded: false,
            };
            terminal
                .draw(|frame| {
                    let paint = paint(false, true);
                    assert!(draw_in(
                        frame, &screen, paint, &focus, &state, &composer, chrome
                    ));
                })
                .expect("draw");
            let buffer = terminal.backend().buffer().clone();
            let geometry = Geometry::of(Rect::new(0, 0, width, height), false).expect("fits");
            let [title, ..] = areas(&geometry, &state, &composer, &screen.thread);
            let heading = row_text(&buffer, title.y, title.x, title.right());
            assert!(
                heading.starts_with("◌ this conversation"),
                "{at}: {heading}"
            );
            let rule = row_text(&buffer, title.y + 1, title.x, title.right());
            assert_eq!(rule, "─".repeat(usize::from(title.width)), "{at}: {rule}");
            let talk = conversation_text(&buffer, &geometry);
            assert!(!talk.contains("Prepare with"), "{at}\n{talk}");
            let edges = [Some(geometry.conversation.right() - 1)]
                .into_iter()
                .chain([geometry.aside.map(|aside| aside.right() - 1)]);
            for x in edges.flatten() {
                for y in geometry.conversation.y..geometry.conversation.bottom() {
                    assert_eq!(Some(buffer[(x, y)].fg), dim, "{at}: ({x}, {y})");
                }
            }
        }
    }
}

/// Nothing attached takes no row: the transcript reaches the composer. An
/// actual attachment keeps its row, apart from the object only viewed.
#[test]
fn an_empty_attachment_context_takes_no_row() {
    let size = (120, 40);
    let mut screen = studio(Some(SEAT));
    let state = state(size, false, false, Waiting::Free);
    let composer = composer(DRAFT, false);
    let geometry = Geometry::of(Rect::new(0, 0, size.0, size.1), false).expect("fits");
    let [_, transcript, context, live] = areas(&geometry, &state, &composer, &screen.thread);
    assert_eq!(context.height, 0, "nothing attached takes no row");
    assert_eq!(
        transcript.bottom(),
        live.y,
        "the transcript reaches the composer"
    );
    screen.thread = screen.thread.clone().attaching("notes.md");
    let [transcript, context, live] = {
        let [_, transcript, context, live] = areas(&geometry, &state, &composer, &screen.thread);
        [transcript, context, live]
    };
    assert_eq!(context.height, 1, "an attachment keeps its row");
    assert_eq!((transcript.bottom(), context.bottom()), (context.y, live.y));
    let (_, buffer) = frame_of(&screen, &state, &composer, paint(false, false), restored());
    let row = row_text(&buffer, context.y, context.x, context.right());
    assert!(row.starts_with("Attached: notes.md"), "{row}");
    assert!(row.contains("viewing release.nika"), "{row}");
}

/// Where the panel has the rows, the composer is one bordered box under a
/// slim caption that says what the next line is (« Your message », or
/// « Your answer » while a question waits). The prompt keeps its meaning
/// inside the box, the draft beside it, and at a free prompt the hint
/// under the box says how `Enter` sends.
#[test]
fn the_composer_is_one_box_under_its_caption_where_the_panel_has_rows() {
    for (width, height) in [(100, 32), (120, 40), (180, 48)] {
        for ascii in [false, true] {
            let question = Waiting::Question {
                key: "const.notes".to_owned(),
            };
            for (waiting, caption, prompt) in [
                (Waiting::Free, "Your message", "nika › "),
                (question, "Your answer", "reply › "),
            ] {
                let at = format!("{width}x{height} ascii={ascii} {caption}");
                let free = waiting == Waiting::Free;
                let screen = studio(Some(SEAT));
                let state = state((width, height), ascii, false, waiting);
                let composer = composer(DRAFT, ascii);
                let (_, buffer) =
                    frame_of(&screen, &state, &composer, paint(ascii, false), restored());
                let geometry = Geometry::of(Rect::new(0, 0, width, height), false).expect("fits");
                let [_, _, _, live] = areas(&geometry, &state, &composer, &screen.thread);
                let lines = rows_of(&buffer, live);
                let top = lines.iter().position(|line| line.trim_end() == caption);
                let top = top.expect(caption);
                let (left, right, side, low_left, low_right) = if ascii {
                    ("+", "+", "|", "+", "+")
                } else {
                    ("╭", "╮", "│", "╰", "╯")
                };
                let upper = &lines[top + 1];
                assert!(
                    upper.starts_with(left) && upper.ends_with(right),
                    "{at}: {upper}"
                );
                let input = &lines[top + 2];
                assert!(
                    input.starts_with(side) && input.ends_with(side),
                    "{at}: {input}"
                );
                let inside = input.trim_start_matches(side).trim_end_matches(side).trim();
                let prompt = if ascii {
                    prompt.replace('›', ">")
                } else {
                    prompt.to_owned()
                };
                assert_eq!(inside, format!("{prompt}{DRAFT}"), "{at}");
                let lower = &lines[top + 4];
                let closed = lower.starts_with(low_left) && lower.ends_with(low_right);
                assert!(closed, "{at}: {lower}");
                let under = lines[top + 5..].join(" ");
                if free {
                    assert!(under.contains("Enter send"), "{at}: {under}");
                }
            }
        }
    }
}

/// A long draft wraps inside the box, every row between its sides, and the
/// cursor (a reversed cell) stands in those same cells, never on the border:
/// sizing, wrapping, painting and the cursor read one rectangle.
#[test]
fn a_long_draft_wraps_inside_the_box_and_the_cursor_stays_in_its_cells() {
    let long =
        "ask the digest to group the notes by project, keep every link, and cite each file it read";
    let draft = format!("{long}\nsecond line");
    for (width, height) in [(120, 40), (180, 48)] {
        for ascii in [false, true] {
            let at = format!("{width}x{height} ascii={ascii}");
            let screen = studio(Some(SEAT));
            let state = state((width, height), ascii, false, Waiting::Free);
            let composer = composer(&draft, ascii);
            let (_, buffer) = frame_of(&screen, &state, &composer, paint(ascii, false), restored());
            let geometry = Geometry::of(Rect::new(0, 0, width, height), false).expect("fits");
            let [_, _, _, live] = areas(&geometry, &state, &composer, &screen.thread);
            let lines = rows_of(&buffer, live);
            let top = lines
                .iter()
                .position(|line| line.trim_end() == "Your message");
            let top = top.expect("the caption");
            let low_left = if ascii { "+" } else { "╰" };
            let side = if ascii { "|" } else { "│" };
            let lower = lines.iter().rposition(|line| line.starts_with(low_left));
            let lower = lower.expect("the box's bottom");
            let inner = &lines[top + 2..lower];
            assert!(inner.len() >= 3, "{at}: {inner:#?}");
            for line in inner {
                assert!(
                    line.starts_with(side) && line.ends_with(side),
                    "{at}: {line}"
                );
            }
            let words = inner
                .iter()
                .map(|line| line.trim_start_matches(side).trim_end_matches(side).trim())
                .collect::<Vec<_>>()
                .join(" ");
            let prompt = if ascii { "nika > " } else { "nika › " };
            let expected = format!("{prompt}{long} second line");
            let spaced = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
            assert_eq!(spaced(&words), spaced(&expected), "{at}");
            let first = live.y + u16::try_from(top + 2).expect("row");
            let last = live.y + u16::try_from(lower).expect("row");
            let cursor = (first..last).find_map(|y| {
                (live.x..live.right())
                    .find(|x| buffer[(*x, y)].modifier.contains(Modifier::REVERSED))
                    .map(|x| (x, y))
            });
            let (x, y) = cursor.expect("the cursor is inside the box");
            assert!(x > live.x && x + 1 < live.right(), "{at}: ({x}, {y})");
            assert_eq!(y + 1, last, "{at}: the cursor ends the draft");
        }
    }
}

/// Where the panel is short the composer stays plain: no caption and no box,
/// the same prompt and draft, the question it answers still in view.
#[test]
fn a_short_panel_keeps_the_plain_composer_and_the_question() {
    for (width, height) in [(60, 16), (80, 24)] {
        for ascii in [false, true] {
            let at = format!("{width}x{height} ascii={ascii}");
            let screen = studio(Some(SEAT));
            let waiting = Waiting::Question {
                key: "const.notes".to_owned(),
            };
            let state = state((width, height), ascii, false, waiting);
            let composer = composer(DRAFT, ascii);
            let (_, buffer) = frame_of(&screen, &state, &composer, paint(ascii, false), restored());
            let geometry = Geometry::of(Rect::new(0, 0, width, height), false).expect("fits");
            let talk = conversation_text(&buffer, &geometry);
            assert!(talk.contains("Which file holds the notes"), "{at}\n{talk}");
            let prompt = if ascii { "reply > " } else { "reply › " };
            assert!(talk.contains(&format!("{prompt}{DRAFT}")), "{at}\n{talk}");
            // The question keeps its own card; the composer has no caption.
            assert!(!talk.contains("Your answer"), "{at}\n{talk}");
        }
    }
}

/// The unsent draft is the same text, once, through resizes and through
/// the object's expansion: nothing is cut, doubled or sent.
#[test]
fn the_draft_stays_the_same_text_across_resizes_and_expansion() {
    let draft = "keep this\nexact draft";
    for arrangement in [
        restored(),
        restored().with_layout(geometry::Layout::Workbench),
    ] {
        for (width, height) in [(120, 40), (80, 24), (180, 48), (60, 16)] {
            let at = format!("{width}x{height} {:?}", arrangement.layout);
            let screen = studio(Some(SEAT));
            let state = state((width, height), false, false, Waiting::Free);
            let composer = composer(draft, false);
            let (rows, _) = frame_of(&screen, &state, &composer, paint(false, false), arrangement);
            let shown = rows.join("\n");
            assert_eq!(shown.matches("keep this").count(), 1, "{at}\n{shown}");
            assert_eq!(shown.matches("exact draft").count(), 1, "{at}\n{shown}");
        }
    }
}

/// The panel paints the current proposal as the screen's review at every
/// qualified size: the latest view ends on where the whole words are read,
/// the cost prose stays with the reader, the human's request stays in view
/// where the transcript holds both, and the draft stays; a screen with no
/// review paints the Session's words.
#[test]
fn the_panel_paints_the_current_proposal_as_its_review() {
    use crate::workspace::cards::review::fixture;
    for (width, height) in [(80, 24), (120, 40), (180, 48)] {
        let at = format!("{width}x{height}");
        let proposal = Committed::proposal(fixture::id(), fixture::PREVIEW);
        let state = fixture::state((width, height), proposal, Waiting::Proposal);
        let composer = composer(DRAFT, false);
        let review = fixture::candidate(fixture::id(), false).review(false);
        let screen = studio(Some(SEAT)).reviewing(review);
        let (rows, _) = frame_of(&screen, &state, &composer, paint(false, false), restored());
        let shown = rows.join("\n");
        let reviewed = shown.contains("Full proposal, in the Session's words");
        assert!(reviewed, "{at}\n{shown}");
        assert!(!shown.contains(fixture::COST), "{at}\n{shown}");
        let request = shown.contains("copy the brief into out");
        assert!(width == 80 || request, "{at}\n{shown}");
        assert_eq!(shown.matches(DRAFT).count(), 1, "{at}\n{shown}");
        let plain = studio(Some(SEAT));
        let (rows, _) = frame_of(&plain, &state, &composer, paint(false, false), restored());
        let said = rows.join("\n");
        let reviewed = said.contains("Full proposal, in the Session's words");
        assert!(!reviewed, "{at}\n{said}");
    }
}

/// The panel at every qualified size carries the question's first exact
/// words in its live card, above the line that answers it, and its block reads
/// one quiet row: the words are painted once, never as their tail alone, and a
/// question in prose keeps its whole card.
#[test]
fn the_panel_carries_the_first_question_words_at_every_size() {
    let words = "Which file holds the notes to digest?\n    (The compiler cannot invent this authoring value.)\nreply on the next line · `cancel` drops this · `why?` explains";
    let label = "Which file holds the notes";
    for (width, height) in [(60, 18), (80, 24), (120, 40), (180, 48)] {
        let at = format!("{width}x{height}");
        let asked = Asked::new("Which file?", "", true, Shape::Text, "3:q", 3);
        let waiting = Waiting::asked("const.notes", asked);
        let mut state = state((width, height), false, false, waiting);
        state.transcript[1] = Committed::question("3:q", words);
        let composer = composer("", false);
        let screen = studio(Some(SEAT));
        let (rows, _) = frame_of(&screen, &state, &composer, paint(false, false), restored());
        let shown = rows.join("\n");
        assert_eq!(shown.matches(label).count(), 1, "{at}\n{shown}");
        assert!(shown.contains("the question waits below"), "{at}\n{shown}");
        let first = rows.iter().position(|row| row.contains(label));
        let line = rows.iter().position(|row| row.contains("reply ›"));
        assert!(
            first.zip(line).is_some_and(|(first, line)| first < line),
            "{at}\n{shown}"
        );
        let mut prose = state.clone();
        prose.transcript[1] = Committed::new(Kind::Question, words);
        let (rows, _) = frame_of(&screen, &prose, &composer, paint(false, false), restored());
        assert!(!rows.join("\n").contains("waits below"), "{at}");
    }
}

/// Where the title fills the object's first row, its action stands at the
/// right end of the continuation cue's row, one blank cell after the cue's
/// widest words (the cue the object paints scrolled between its ends): the
/// folded strip's frame shows the cue and the action whole, neither over the
/// other, at the cells the pointer reads.
#[test]
fn a_full_title_row_leaves_the_action_to_the_cue_row() {
    for ascii in [false, true] {
        let at = format!("ascii={ascii}");
        let body: Vec<Line<'static>> = (0..20).map(|n| Line::raw(format!("line {n}"))).collect();
        let shown = Object::Workflow {
            title: Line::raw("x".repeat(55)),
            body,
        };
        let widest = object::lines_from(&shown, 60, 3, paint(ascii, false), 5);
        assert_eq!(widest.last().map(Line::width), Some(CUE_CELLS), "{at}");
        let size = (60, 16);
        let geometry = folded(Geometry::of(Rect::new(0, 0, 60, 16), false).expect("fits"));
        let next = Some(geometry::Layout::Workbench);
        let action = object_action(&shown, &geometry, Region::Conversation, next, ascii);
        let (cells, words) = action.expect("the cue's row offers it");
        let (expand, below) = if ascii {
            ("[+] Expand - F4", "v More below - scroll")
        } else {
            ("[+] Expand · F4", "↓ More below · scroll")
        };
        assert_eq!(words, expand, "{at}");
        assert_eq!(cells.y, geometry.object.bottom() - 1, "{at}");
        assert_eq!(cells.right(), geometry.object.right(), "{at}");
        assert!(usize::from(cells.x - geometry.object.x) > CUE_CELLS, "{at}");
        let mut screen = studio(None);
        screen.object = shown;
        let state = state(size, ascii, false, Waiting::Free);
        let chrome = Chrome {
            arrangement: restored(),
            dragging: None,
            folded: true,
        };
        let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).expect("terminal");
        terminal
            .draw(|frame| {
                let focus = Focus::composing();
                let composer = composer("", ascii);
                let paint = paint(ascii, false);
                assert!(draw_in(
                    frame, &screen, paint, &focus, &state, &composer, chrome
                ));
            })
            .expect("draw");
        let row = row_text(terminal.backend().buffer(), cells.y, 0, size.0);
        assert!(row.starts_with(below), "{at}: {row}");
        assert!(row.ends_with(expand), "{at}: {row}");
        let title = row_text(terminal.backend().buffer(), geometry.object.y, 0, size.0);
        assert!(
            title.starts_with(&"x".repeat(55)),
            "{at}: the title kept whole"
        );
    }
}

/// Where the title fills the object's first row and no continuation cue
/// stands, a last row the object's lines leave free (strictly fewer body
/// lines than rows under the title) carries the action at its right end:
/// the frame paints it whole on that blank row, at the cells the pointer
/// reads. With exactly as many lines as rows the last line keeps its row,
/// nothing covers it, and `F4` alone acts.
#[test]
fn a_free_last_row_carries_the_action_the_title_row_cannot() {
    // At 16 rows the restored Object already fills the available growth.
    // Use an actual expandable frame, as drawing and the pointer do.
    let size = (60, 18);
    let area = Rect::new(0, 0, size.0, size.1);
    let geometry = Geometry::of(area, false).expect("fits");
    let next = toggled(area, false, &restored(), false);
    assert_eq!(next, Some(geometry::Layout::Workbench));
    assert_eq!(
        toggled(Rect::new(0, 0, 60, 16), false, &restored(), false),
        None
    );
    let under = usize::from(geometry.object.height - 1);
    for ascii in [false, true] {
        let expand = if ascii {
            "[+] Expand - F4"
        } else {
            "[+] Expand · F4"
        };
        for (lines, offered) in [(under - 1, true), (under, false)] {
            let at = format!("ascii={ascii} lines={lines}");
            let body = (0..lines).map(|n| Line::raw(format!("line {n}")));
            let shown = Object::Workflow {
                title: Line::raw("x".repeat(55)),
                body: body.collect(),
            };
            let action = object_action(&shown, &geometry, Region::Conversation, next, ascii);
            assert_eq!(action.is_some(), offered, "{at}");
            let mut screen = studio(None);
            screen.object = shown;
            let state = state(size, ascii, false, Waiting::Free);
            let composer = composer("", ascii);
            let (rows, _) = frame_of(&screen, &state, &composer, paint(ascii, false), restored());
            let last = &rows[usize::from(geometry.object.bottom() - 1)];
            if let Some((cells, words)) = action {
                assert_eq!(words, expand, "{at}");
                assert_eq!(cells.y, geometry.object.bottom() - 1, "{at}");
                assert_eq!(cells.right(), geometry.object.right(), "{at}");
                assert!(last.ends_with(expand), "{at}: {last}");
                let before = last.strip_suffix(expand).unwrap_or(last);
                assert!(before.trim().is_empty(), "{at}: nothing covered: {last}");
            } else {
                let kept = format!("line {}", lines - 1);
                assert!(last.starts_with(&kept), "{at}: {last}");
                assert!(!last.contains("[+]"), "{at}: {last}");
            }
            let title = &rows[usize::from(geometry.object.y)];
            assert!(title.starts_with(&"x".repeat(55)), "{at}: the title whole");
        }
    }
}
