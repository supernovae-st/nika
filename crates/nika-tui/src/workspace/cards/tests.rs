// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The conversation's pieces: each speaker's bubble on its side, bounded
//! decision and refusal cards, one plan that measures exactly what it paints,
//! every wrapped row reachable, the words never rewritten.

use super::diagnostics::tests::{
    ROOT, WARNING_LINE, knowledge_refusal, opening_banner, provider_failure,
};
use super::review::fixture;
use super::*;
use crate::composer::Composer;
use crate::model::{Asked, Presentation, Shape, Waiting};
use crate::workspace::desk::{self, Desk};
use crate::workspace::object::{Object, Paint};
use crate::workspace::text::twins;
use nika_session::ProposalId;
use ratatui::style::Color;
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

/// The rows of `buffer`, one string per row.
fn rows_of(buffer: &Buffer) -> Vec<String> {
    let area = buffer.area;
    (0..area.height)
        .map(|y| (0..area.width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

/// The rows and the buffer of the conversation painted in `area` with
/// `context`.
fn framed(state: &UiState, area: Rect, context: Context<'_>) -> (Vec<String>, Buffer) {
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).expect("terminal");
    terminal
        .draw(|frame| render(frame, state, area, context))
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    (rows_of(&buffer), buffer)
}

/// The rows and the buffer of the conversation painted in `area` with
/// `review`.
fn reviewed(state: &UiState, area: Rect, review: Option<&Review>) -> (Vec<String>, Buffer) {
    framed(state, area, (review, None))
}

/// The rows and the buffer of the conversation painted in `area`.
fn drawn(state: &UiState, area: Rect) -> (Vec<String>, Buffer) {
    reviewed(state, area, None)
}

/// Every scroll position of `state` in `area` painted with `context`, from
/// the latest view to one row past the furthest, and how far that is.
fn windows(state: &mut UiState, area: Rect, context: Context<'_>) -> (usize, Vec<Buffer>) {
    let most = height(state, area, context).saturating_sub(usize::from(area.height));
    let frames = (0..=most + 1)
        .map(|scroll| {
            state.focus_scroll = scroll;
            framed(state, area, context).1
        })
        .collect();
    state.focus_scroll = 0;
    (most, frames)
}

/// What the first row of `frame` declares when it is a card's top border: the
/// rows of the card's words above the first one shown, `0` for the card's own
/// top; `None` for any other row.
fn declared(frame: &Buffer) -> Option<usize> {
    let row: String = (0..frame.area.width)
        .map(|x| frame[(x, 0)].symbol())
        .collect();
    let title = (row.strip_prefix("╭─ ")).or_else(|| row.strip_prefix("+- "))?;
    let count = (title.split_once("↑ ")).or_else(|| title.split_once("^ "));
    Some(count.map_or(0, |(_, rest)| {
        let digits = rest.split(' ').next().unwrap_or_default();
        digits.parse().expect("a whole count")
    }))
}

/// One plan measures and paints `state` with `context`: one more row of
/// scroll moves every painted cell exactly one row down, the latest view
/// ends on the last piece's row of air, and the furthest scroll shows the
/// conversation's first row and stops there. The one declared exception: a
/// card entered part-way names itself and the rows of its words above on its
/// first row shown, one row back declares one fewer (its own top at none),
/// and every other row still moves whole.
fn assert_one_window(state: &mut UiState, area: Rect, context: Context<'_>, case: &str) {
    let (most, frames) = windows(state, area, context);
    assert!(most > 0, "{case}: the conversation scrolls");
    for (scroll, pair) in frames.windows(2).enumerate().take(most) {
        let above = declared(&pair[0]).filter(|above| *above > 0);
        if let Some(above) = above {
            let back = declared(&pair[1]);
            assert_eq!(back, Some(above - 1), "{case}: scroll {scroll}");
        }
        for y in u16::from(above.is_some())..area.height - 1 {
            for x in 0..area.width {
                let moved = pair[1][(x, y + 1)] == pair[0][(x, y)];
                assert!(moved, "{case}: scroll {scroll} at ({x}, {y})");
            }
        }
    }
    let blank = |frame: &Buffer, y: u16| (0..area.width).all(|x| frame[(x, y)].symbol() == " ");
    let ends = blank(&frames[0], area.height - 1) && !blank(&frames[0], area.height - 2);
    assert!(ends, "{case}: the latest view ends on the last piece's air");
    assert!(!blank(&frames[most], 0), "{case}: the first row at the top");
    let stops = frames[most] == frames[most + 1];
    assert!(stops, "{case}: no row above the first");
}

/// The words a reader meets at each scroll position of `state` painted with
/// `review`.
fn readings(state: &mut UiState, area: Rect, review: Option<&Review>) -> Vec<String> {
    let (_, frames) = windows(state, area, (review, None));
    frames.iter().map(|frame| words(&rows_of(frame))).collect()
}

/// Whether `fact`, its spaces aside, is read whole at some scroll position.
fn seen(readings: &[String], fact: &str) -> bool {
    let fact = fact.split_whitespace().collect::<Vec<_>>().join(" ");
    readings.iter().any(|reading| reading.contains(&fact))
}

/// Every row of the whole conversation painted at `width` in a transcript
/// `tall` rows high (which chooses the form), the measured rows exactly: a
/// plan shorter than the transcript stands on its last row, the rows above it
/// blank, and the rows and cells returned start at the plan's first row.
fn painted(state: &UiState, width: u16, tall: u16) -> (Vec<String>, Buffer) {
    let area = Rect::new(0, 0, width, tall);
    let rows = height(state, area, (None, None));
    assert!(rows <= usize::from(tall), "{rows} rows do not fit {tall}");
    let (text, buffer) = drawn(state, area);
    let above = usize::from(tall) - rows;
    let blank = text[..above].iter().all(|row| row.trim().is_empty());
    assert!(blank, "a short plan stands on the last row: {text:#?}");
    let first = u16::try_from(above).expect("a row of the transcript");
    let mut shown = Buffer::empty(Rect::new(0, 0, width, tall - first));
    for y in 0..tall - first {
        for x in 0..width {
            shown[(x, y)] = buffer[(x, y + first)].clone();
        }
    }
    (text[above..].to_vec(), shown)
}

/// A speaker label or a sublabel row.
fn is_label(row: &str) -> bool {
    const NAMES: [&str; 6] = ["You", "Nika", "Activity", "Run", "Result", "Report"];
    let kind = (row.strip_prefix("Nika · ")).or(row.strip_prefix("Nika - "));
    NAMES.contains(&row) || kind.is_some_and(|kind| NAMES.contains(&kind))
}

/// The words a reader meets, without edges, labels or sublabels.
fn words(rows: &[String]) -> String {
    let inner = |row: &String| {
        let row = row.trim();
        let cells: Vec<char> = row.chars().collect();
        let edge = |cell: Option<&char>| matches!(cell, Some('│' | '|'));
        if cells.len() > 2 && edge(cells.first()) && edge(cells.last()) {
            Some(cells[1..cells.len() - 1].iter().collect::<String>())
        } else if row.starts_with(['╭', '╰', '+']) || is_label(row) {
            None
        } else {
            Some(row.to_owned())
        }
    };
    let words: Vec<String> = (rows.iter().filter_map(inner))
        .flat_map(|row| {
            row.split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect();
    words.join(" ")
}

/// Every glyph of `text`, whitespace dropped.
fn glyphs(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

/// A conversation with every kind of piece: the opening banner, the human's
/// lines (one of two lines), Nika's speech, a run grouped under its report
/// and result, decision cards between them, rows long enough to wrap.
fn journey(color: bool, ascii: bool, size: (u16, u16)) -> UiState {
    let mut state = UiState::new(Presentation::Workspace, color, size);
    state.ascii = ascii;
    for (kind, text) in [
        (Kind::Banner, "Nika · demo\n\nWhat do you want to automate?"),
        (Kind::Human, "nika › digest my monday notes"),
        (
            Kind::Question,
            "Which file holds the notes to digest? (const.source_path)",
        ),
        (Kind::Human, "reply › ./notes/lundi.md"),
        (
            Kind::Reply,
            "a reply long enough to wrap over several rows at narrow widths, then its end",
        ),
        (
            Kind::Report,
            "running ./digest-notes.nika once · ceiling $0.25",
        ),
        (Kind::Run, "read     ✓  12 ms"),
        (Kind::Run, "draft    ✓  1.8 s  $0.0021"),
        (
            Kind::Gate,
            "write ./digest.md · 412 bytes · overwrite the existing file?",
        ),
        (Kind::Run, "write    ✓  3 ms"),
        (
            Kind::Result,
            "produced ./digest.md (412 bytes)\n2.1 s · $0.0021 · sealed",
        ),
        (Kind::Human, "thanks\nand a second line of the same message"),
    ] {
        state.transcript.push(Committed::new(kind, text));
    }
    state
}

/// The human's lines stand on the raised surface at the right end, Nika's on
/// the ground at the left, each under its speaker: slabs in a short
/// transcript, quiet outlines in a tall one. Without colour no hue or surface
/// is painted, the ASCII column draws only ASCII, and no block is rewritten.
#[test]
fn the_human_stands_raised_on_the_right_and_nika_on_the_left() {
    let raised = role::surface(true, true).bg.expect("the raised surface");
    // (rows, first column of the human's bubble, last column of its words,
    // first column of Nika's words, rows the human's surface covers)
    for (tall, left, end, nika, surface) in [(12, 17, 38, 0, 2), (20, 15, 37, 2, 3)] {
        for (ascii, color) in [(false, false), (false, true), (true, false), (true, true)] {
            let case = format!("rows {tall} ascii {ascii} color {color}");
            let mut state = UiState::new(Presentation::Workspace, color, (40, tall));
            state.ascii = ascii;
            state
                .transcript
                .push(Committed::new(Kind::Human, "Keep a compact brief."));
            state.transcript.push(Committed::new(
                Kind::Reply,
                "I will keep each update concise.",
            ));
            let before = state.transcript.clone();
            let (rows, buffer) = painted(&state, 40, tall);
            // The row and the first column (cells, one per char here) of `needle`.
            let at = |needle: &str| {
                let row = rows.iter().position(|row| row.contains(needle));
                let row = row.expect("a painted line");
                let byte = rows[row].find(needle).expect("the needle");
                (row, rows[row][..byte].chars().count())
            };
            let (you, start) = at("Keep a compact brief.");
            let (said, nika_start) = at("I will keep");
            assert_eq!(
                start + 20,
                end,
                "{case}: the human's words end at the right\n{rows:#?}"
            );
            assert_eq!(
                nika_start, nika,
                "{case}: Nika's words at the left\n{rows:#?}"
            );
            assert!(said > you, "{case}");
            let top = u16::try_from(you - 1).expect("row");
            for y in top..top + surface {
                for x in 0..40 {
                    let bg = buffer[(x, y)].bg;
                    let lifted = color && x >= left;
                    assert_eq!(bg == raised, lifted, "{case}: ({x}, {y})\n{rows:#?}");
                }
            }
            let gap = top + surface;
            let nika_row = u16::try_from(said).expect("row");
            for y in [gap, nika_row] {
                let ground = (0..40).all(|x| buffer[(x, y)].bg == Color::Reset);
                assert!(ground, "{case}: row {y} on the ground\n{rows:#?}");
            }
            if !color {
                let plain = buffer
                    .content()
                    .iter()
                    .all(|cell| cell.fg == Color::Reset && cell.bg == Color::Reset);
                assert!(plain, "{case}: a hue without colour");
            }
            assert!(!ascii || rows.join("").is_ascii(), "{case}\n{rows:#?}");
            assert_eq!(state.transcript, before, "{case}");
        }
    }
}

/// A speaker's consecutive blocks share one bubble under one label; a quiet
/// sublabel names each change of kind, and a run's task lines share one.
#[test]
fn a_run_reads_under_one_label_and_one_sublabel() {
    for (tall, ascii) in [(14, false), (14, true), (24, false), (24, true)] {
        let case = format!("rows {tall} ascii {ascii}");
        let mut state = UiState::new(Presentation::Workspace, false, (60, tall));
        state.ascii = ascii;
        for (kind, text) in [
            (Kind::Human, "run it"),
            (Kind::Report, "running ./digest-notes.nika once"),
            (Kind::Run, "read     ok  12 ms"),
            (Kind::Run, "draft    ok  1.8 s"),
            (Kind::Run, "write    ok  3 ms"),
            (Kind::Result, "produced ./digest.md (412 bytes)"),
        ] {
            state.transcript.push(Committed::new(kind, text));
        }
        let (rows, _) = painted(&state, 60, tall);
        let bare: Vec<&str> = (rows.iter())
            .map(|row| row.trim().trim_matches(['│', '|']).trim())
            .collect();
        let count = |words: &str| bare.iter().filter(|row| **row == words).count();
        assert_eq!(
            count("Run"),
            1,
            "{case}: one sublabel for the run\n{rows:#?}"
        );
        assert_eq!(count("Result"), 1, "{case}\n{rows:#?}");
        assert_eq!(count("Report"), 0, "{case}: the label names it\n{rows:#?}");
        let sep = if ascii { "-" } else { "·" };
        let label = format!("Nika {sep} Report");
        assert_eq!(
            rows.iter().filter(|row| row.contains(&label)).count(),
            1,
            "{case}\n{rows:#?}"
        );
        let run = bare.iter().position(|row| *row == "Run").expect("sublabel");
        for (next, task) in ["read", "draft", "write"].into_iter().enumerate() {
            assert!(bare[run + 1 + next].starts_with(task), "{case}\n{rows:#?}");
        }
        assert!(!ascii || rows.join("").is_ascii(), "{case}\n{rows:#?}");
    }
}

/// One plan measures and paints: one more row of scroll moves every painted
/// cell exactly one row down, the latest view ends on the last piece's row of
/// air, and the furthest scroll shows the conversation's first row and stops
/// there; in both forms, at every width, with colour and without.
#[test]
fn every_scroll_position_is_one_window_of_the_measured_conversation() {
    // Slabs below sixteen rows or twenty-four columns, outlines from there.
    let slabs = [(6, 6), (12, 8), (24, 15), (80, 8)];
    let outlines = [(24, 16), (44, 29), (68, 37)];
    for (width, tall) in slabs.into_iter().chain(outlines) {
        for (ascii, color) in [(false, true), (true, false)] {
            let case = format!("{width}x{tall} ascii {ascii} color {color}");
            let mut state = journey(color, ascii, (width, tall));
            let area = Rect::new(0, 0, width, tall);
            assert_one_window(&mut state, area, (None, None), &case);
        }
    }
}

/// A bubble is measured in cells: wide glyphs size it, a wide glyph starting
/// in its last column of words spills only into the air beside them, never
/// over the edge, and every glyph of the message stays painted.
#[test]
fn wide_glyphs_size_the_bubble_and_keep_its_edge() {
    let mut short = UiState::new(Presentation::Workspace, false, (24, 16));
    short.transcript.push(Committed::new(Kind::Human, "漢字"));
    let (rows, buffer) = painted(&short, 24, 16);
    let found = rows.iter().position(|row| row.contains('漢'));
    let body = u16::try_from(found.expect("words")).expect("row");
    // Four cells of words, an edge and a cell of air each side, at the right end.
    for (x, cell) in [(16, "│"), (18, "漢"), (20, "字"), (23, "│")] {
        assert_eq!(buffer[(x, body)].symbol(), cell, "{rows:#?}");
    }
    let message = format!("{} 漢\n漢字", "a".repeat(18));
    let mut long = UiState::new(Presentation::Workspace, false, (24, 16));
    long.transcript
        .push(Committed::new(Kind::Human, message.clone()));
    let (rows, buffer) = painted(&long, 24, 16);
    let mut seen = String::new();
    for (y, row) in rows.iter().enumerate() {
        if !row.starts_with('│') {
            continue;
        }
        let y = u16::try_from(y).expect("row");
        assert_eq!(buffer[(23, y)].symbol(), "│", "the edge stays\n{rows:#?}");
        let cells = (1..23).map(|x| buffer[(x, y)].symbol().to_owned());
        seen.extend(cells);
    }
    assert_eq!(glyphs(&seen), glyphs(&message), "{rows:#?}");
}

/// The transcript's rectangle alone chooses slabs or outlines: never the
/// conversation's length, so measuring and painting one rectangle agree.
#[test]
fn the_rectangle_alone_chooses_slabs_or_outlines() {
    assert_eq!(Form::of(Rect::new(0, 0, 40, 15)), Form::Slab);
    assert_eq!(Form::of(Rect::new(0, 0, 40, 16)), Form::Outlined);
    assert_eq!(Form::of(Rect::new(0, 0, 23, 40)), Form::Slab);
    for blocks in [1, 30] {
        let mut state = UiState::new(Presentation::Workspace, false, (40, 16));
        for _ in 0..blocks {
            state
                .transcript
                .push(Committed::new(Kind::Reply, "same words"));
        }
        let (rows, _) = drawn(&state, Rect::new(0, 0, 40, 16));
        let outlined = rows.iter().any(|row| row.starts_with('╰'));
        assert!(outlined, "{blocks} blocks: {rows:#?}");
    }
}

/// While the Session waits for consent, the latest proposal it tagged with
/// the candidate's identity reads as the candidate's typed review: every
/// change, the bytes the faces do not show, what it reads and reaches, its
/// rehearsal, the identity a consent names and where the whole words are read
/// on its bottom border; no lead and no footnote repeat the standing or the
/// bytes. The human's turn stays readable, the cost prose stays with the
/// reader, and the block keeps every byte; at the 80, 120 and 180
/// transcripts, in both glyph columns.
#[test]
fn the_current_proposal_reads_as_its_typed_review() {
    let facts = [
        fixture::CREATES,
        fixture::REPLACES,
        UNSHOWN,
        fixture::READS,
        fixture::REACH,
        "rehearsal · none bound to this identity",
    ];
    for (width, tall) in [(80, 8), (44, 29), (68, 37)] {
        for ascii in [false, true] {
            let case = format!("{width}x{tall} ascii {ascii}");
            let review = fixture::candidate(fixture::id(), false).review(ascii);
            let proposal = Committed::proposal(fixture::id(), fixture::PREVIEW);
            let mut state = fixture::state((width, tall), proposal, Waiting::Proposal);
            state.ascii = ascii;
            let before = state.transcript.clone();
            let area = Rect::new(0, 0, width, tall);
            let read = readings(&mut state, area, review.as_ref());
            for fact in facts {
                let shown = seen(&read, &twins(fact, ascii));
                assert!(shown, "{case}: {fact}\n{read:#?}");
            }
            for gone in ["what a yes answers", "Full proposal", "these bytes"] {
                assert!(!seen(&read, gone), "{case}: {gone}\n{read:#?}");
            }
            let (rows, _) = reviewed(&state, area, review.as_ref());
            let foot = format!("proposal {} · F2: whole words", fixture::id());
            let foot = twins(&foot, ascii);
            let border = rows.iter().find(|row| row.contains(&foot));
            let low = if ascii { "+- " } else { "╰─ " };
            let edged = border.is_some_and(|row| row.starts_with(low));
            assert!(edged, "{case}\n{rows:#?}");
            let turn = seen(&read, "copy the brief into out");
            assert!(turn, "{case}: the human's turn");
            let cost = seen(&read, fixture::COST);
            assert!(!cost, "{case}: the cost prose is the reader's");
            assert_eq!(state.transcript, before, "{case}");
        }
    }
}

/// The current proposal's identity and reader key ride its bottom border one
/// cell in, quiet beside the decision's edges, in the longest whole form the
/// border holds and never cut: the card stays its words plus three rows, and
/// a border too narrow for even the identity stays a plain rule.
#[test]
fn the_reviewed_card_carries_its_identity_on_its_border() {
    let id = fixture::id().to_string();
    for (width, form) in [
        (72, format!("proposal {id} · F2: whole words")),
        (44, format!("proposal {id} · F2: whole words")),
        (43, format!("proposal {id} · F2")),
        (31, format!("proposal {id} · F2")),
        (30, format!("{id} · F2")),
    ] {
        for ascii in [false, true] {
            let at = format!("{width} ascii={ascii}");
            let review = fixture::candidate(fixture::id(), false).review(ascii);
            let proposal = Committed::proposal(fixture::id(), fixture::PREVIEW);
            let mut state = fixture::state((width, 60), proposal, Waiting::Proposal);
            state.ascii = ascii;
            state.color = true;
            state.transcript.remove(0);
            let area = Rect::new(0, 0, width, 60);
            let rows = height(&state, area, (review.as_ref(), None));
            let words = (review.as_ref())
                .map(|review| review.lines(true, ascii, width - 4))
                .unwrap_or_default();
            assert_eq!(rows, words.len() + 3, "{at}: the words plus three rows");
            let (shown, buffer) = reviewed(&state, area, review.as_ref());
            let y = shown.iter().rposition(|row| row.trim() != "");
            let y = y.expect("a border");
            let low = if ascii { "+- " } else { "╰─ " };
            let opens = format!("{low}{} ", twins(&form, ascii));
            assert!(shown[y].starts_with(&opens), "{at}: {}", shown[y]);
            let end = if ascii { "+" } else { "╯" };
            assert!(shown[y].trim_end().ends_with(end), "{at}: {}", shown[y]);
            let row = u16::try_from(y).expect("a row");
            let (edge, quiet) = (role::style(Role::Warn, true), role::style(Role::Dim, true));
            assert_eq!(Some(buffer[(0, row)].fg), edge.fg, "{at}: edge");
            assert_eq!(Some(buffer[(3, row)].fg), quiet.fg, "{at}: a quiet foot");
        }
    }
    let review = fixture::candidate(fixture::id(), false).review(false);
    let proposal = Committed::proposal(fixture::id(), fixture::PREVIEW);
    let state = fixture::state((20, 40), proposal, Waiting::Proposal);
    let (shown, _) = reviewed(&state, Rect::new(0, 0, 20, 40), review.as_ref());
    let y = shown.iter().rposition(|row| row.trim() != "");
    let (y, rule) = (y.expect("a border"), format!("╰{}╯", "─".repeat(18)));
    assert_eq!(shown[y], rule, "too narrow: a plain rule");
}

/// A proposal block the Session no longer waits on is history: its card is
/// titled `Proposal`, quietly, and keeps the Session's whole words; the one
/// the Session waits on keeps the decision's title, reviewed or not.
#[test]
fn a_proposal_no_longer_waited_on_is_titled_as_history() {
    let area = Rect::new(0, 0, 68, 60);
    let proposal = Committed::proposal(fixture::id(), fixture::PREVIEW);
    let title = |state: &UiState, review: Option<&Review>| {
        let (rows, buffer) = reviewed(state, area, review);
        let y = rows.iter().position(|row| row.starts_with("╭─ "));
        let y = y.expect("its title");
        (
            rows[y].clone(),
            buffer[(3, u16::try_from(y).expect("a row"))].fg,
        )
    };
    let review = fixture::candidate(fixture::id(), false).review(false);
    let mut state = fixture::state((68, 60), proposal, Waiting::Proposal);
    state.transcript.remove(0);
    state.color = true;
    let decision = "╭─ Review before saving ─";
    let (waiting, tone) = title(&state, review.as_ref());
    assert!(waiting.starts_with(decision), "{waiting}");
    let (unreviewed, _) = title(&state, None);
    assert!(unreviewed.starts_with(decision), "{unreviewed}");
    state.waiting = Waiting::Free;
    let (saved, quiet) = title(&state, review.as_ref());
    assert!(saved.starts_with("╭─ Proposal ─"), "{saved}");
    assert_eq!(Some(quiet), role::style(Role::Dim, true).fg, "quiet");
    assert_ne!(quiet, tone);
    let read = readings(&mut state, area, review.as_ref());
    assert!(seen(&read, fixture::COST), "its whole words: {read:#?}");
}

/// Anything but the current proposal keeps the Session's whole words:
/// another identity, an untagged or held preview, the Session waiting for
/// anything but consent, an aside or draft candidate, or no candidate.
#[test]
fn every_other_proposal_keeps_the_sessions_words() {
    let id = fixture::id();
    let current = fixture::candidate(id.clone(), false).review(false);
    let tagged = Committed::proposal(id.clone(), fixture::PREVIEW);
    let question = Waiting::Question {
        key: "const.notes".to_owned(),
    };
    let aside = fixture::candidate(id.clone(), true).review(false);
    let draft = fixture::candidate(id.clone(), false)
        .drafted()
        .review(false);
    assert!(aside.is_none() && draft.is_none(), "no review names them");
    let other = Committed::proposal(ProposalId::of("another preview"), fixture::PREVIEW);
    let held = Committed::new(Kind::Proposal, fixture::PREVIEW);
    let cases = [
        (
            "another identity",
            other,
            Waiting::Proposal,
            current.clone(),
        ),
        ("untagged or held", held, Waiting::Proposal, current.clone()),
        ("free", tagged.clone(), Waiting::Free, current.clone()),
        ("question", tagged.clone(), question, current),
        ("aside", tagged.clone(), Waiting::Proposal, aside),
        ("draft", tagged.clone(), Waiting::Proposal, draft),
        ("no candidate", tagged, Waiting::Proposal, None),
    ];
    for (case, block, waiting, review) in cases {
        let mut state = fixture::state((68, 37), block, waiting);
        let read = readings(&mut state, Rect::new(0, 0, 68, 37), review.as_ref());
        let said = seen(&read, fixture::COST);
        assert!(said, "{case}: the Session's words\n{read:#?}");
        let reviewed = seen(&read, UNSHOWN);
        assert!(!reviewed, "{case}");
        let (rows, _) = reviewed_frame(&state, review.as_ref());
        assert!(!rows.contains(" F2: whole words "), "{case}\n{rows}");
    }
}

/// The words only the review says: how many changes the faces do not show.
const UNSHOWN: &str = "1 more change(s) whose bytes these faces do not show";

/// The whole frame of `state`'s conversation at 68x37, read with `review`.
fn reviewed_frame(state: &UiState, review: Option<&Review>) -> (String, Buffer) {
    let (rows, buffer) = reviewed(state, Rect::new(0, 0, 68, 37), review);
    (rows.join("\n"), buffer)
}

/// Only the latest proposal the Session tagged can be reviewed: an older
/// preview of the same identity keeps its words, and a newer proposal of
/// another identity leaves every preview as said.
#[test]
fn only_the_latest_tagged_proposal_is_reviewed() {
    let id = fixture::id();
    let review = fixture::candidate(id.clone(), false).review(false);
    let first = Committed::proposal(id.clone(), fixture::PREVIEW);
    let mut state = fixture::state((68, 37), first, Waiting::Proposal);
    state
        .transcript
        .push(Committed::new(Kind::Human, "nika › propose it again"));
    state
        .transcript
        .push(Committed::proposal(id, fixture::PREVIEW));
    let current = review::summarized(&state, review.as_ref()).map(|(at, _)| at);
    assert_eq!(current, Some(3), "the latest tagged preview");
    let read = readings(&mut state, Rect::new(0, 0, 68, 37), review.as_ref());
    let older = seen(&read, fixture::COST);
    assert!(older, "the older preview keeps its words");
    assert!(seen(&read, UNSHOWN));
    let (rows, _) = reviewed_frame(&state, review.as_ref());
    assert!(rows.contains("╭─ Proposal ─"), "history\n{rows}");
    assert!(rows.contains("╭─ Review before saving ─"), "{rows}");
    let newer = ProposalId::of("a newer preview");
    state
        .transcript
        .push(Committed::proposal(newer, "a newer preview"));
    assert!(review::summarized(&state, review.as_ref()).is_none());
}

/// The reviewed conversation scrolls as one window over exactly the rows its
/// measure counts with the same review, in the 80, 120 and 180 transcripts.
#[test]
fn a_reviewed_conversation_scrolls_as_one_window() {
    for (width, tall) in [(80, 8), (44, 29), (68, 37)] {
        let review = fixture::candidate(fixture::id(), false).review(false);
        let proposal = Committed::proposal(fixture::id(), fixture::PREVIEW);
        let mut state = fixture::state((width, tall), proposal, Waiting::Proposal);
        let earlier = (0..12).map(|n| Committed::new(Kind::Reply, format!("earlier reply {n}")));
        let mut transcript: Vec<Committed> = earlier.collect();
        transcript.append(&mut state.transcript);
        state.transcript = transcript;
        assert!(review::summarized(&state, review.as_ref()).is_some());
        let case = format!("{width}x{tall}");
        let area = Rect::new(0, 0, width, tall);
        assert_one_window(&mut state, area, (review.as_ref(), None), &case);
    }
}

/// The object's title and body as the desk paints them, one row per line.
fn object_text(desk: &Desk) -> String {
    match desk.screen(false).object {
        Object::Workflow { title, body } => (std::iter::once(title).chain(body))
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n"),
        other => format!("{other:?}"),
    }
}

/// The whole workspace frame of `desk` and `state`, its rows joined.
fn capture(desk: &Desk, state: &UiState) -> String {
    let (width, tall) = state.size;
    let mut terminal = Terminal::new(TestBackend::new(width, tall)).expect("terminal");
    let paint = Paint {
        ascii: false,
        color: false,
        elapsed: std::time::Duration::ZERO,
        reduced_motion: true,
    };
    let composer = Composer::new();
    terminal
        .draw(|frame| assert!(desk::draw(frame, desk, paint, state, &composer)))
        .expect("draw");
    rows_of(terminal.backend().buffer()).join("\n")
}

/// The object leaves its facts to the conversation exactly while the
/// conversation reviews the candidate it shows: the reviewed face keeps its
/// title, the witness of its bytes and the face; the same candidate, face
/// and size with no review takes its facts back, the cached face rendered
/// anew each way, and every capture shows a material fact once. An aside
/// candidate keeps its facts whatever waits.
#[test]
fn the_object_leaves_its_facts_to_the_review_alone() {
    let size = (180, 48);
    let mut desk = Desk::new();
    desk.proposed(Some(fixture::candidate(fixture::id(), false)));
    let proposal = Committed::proposal(fixture::id(), fixture::PREVIEW);
    let reviewed = fixture::state(size, proposal.clone(), Waiting::Proposal);
    let free = fixture::state(size, proposal, Waiting::Free);
    for (state, facts) in [(&reviewed, false), (&free, true), (&reviewed, false)] {
        desk.prepare_for(state, false, false);
        let object = object_text(&desk);
        assert_eq!(object.contains("33 lines"), facts, "{object}");
        assert_eq!(object.contains("what a yes answers"), facts, "{object}");
        assert!(object.contains("these bytes 51835c93e564"), "{object}");
        for _ in 0..2 {
            let screen = capture(&desk, state);
            assert_eq!(screen.matches("33 lines").count(), 1, "{screen}");
        }
    }
    let mut aside = Desk::new();
    aside.proposed(Some(fixture::candidate(fixture::id(), true)));
    aside.prepare_for(&reviewed, false, false);
    assert!(object_text(&aside).contains("33 lines"), "the facts stay");
}

/// In a pane too small for cards the current proposal still reads as its
/// review, its identity and reader key on a last row of their own (no border
/// carries them there), the measure counting exactly the lines painted, and
/// the cost prose stays with the reader.
#[test]
fn a_short_split_reads_the_current_proposal_as_its_review() {
    let area = Rect::new(0, 0, 40, 5);
    let review = fixture::candidate(fixture::id(), false).review(false);
    let proposal = Committed::proposal(fixture::id(), fixture::PREVIEW);
    let state = fixture::state((40, 5), proposal, Waiting::Proposal);
    let foot = format!("proposal {} · F2: whole words", fixture::id());
    let facts = (review.as_ref())
        .map(|review| review.lines(false, false, 40))
        .unwrap_or_default();
    let human = card_lines(&Committed::new(Kind::Human, fixture::REQUEST), false, false);
    let expected = content_rows(&[human, facts, vec![Line::raw(foot.clone())]].concat(), 40);
    assert_eq!(height(&state, area, (review.as_ref(), None)), expected);
    let (rows, _) = reviewed(&state, area, review.as_ref());
    let last = rows.last().map(|row| row.trim_end());
    assert_eq!(last, Some(foot.as_str()), "{rows:#?}");
    let read = rows.join(" ");
    assert!(read.contains("rehearsal · none bound"), "{rows:#?}");
    assert!(!read.contains(fixture::COST), "{rows:#?}");
}

/// Short panes still distinguish a sent message from the assistant reply.
#[test]
fn a_compact_conversation_keeps_the_human_speaker() {
    for ascii in [false, true] {
        let area = Rect::new(0, 0, 40, 4);
        let mut state = UiState::new(Presentation::Workspace, false, (40, 4));
        state.ascii = ascii;
        state
            .transcript
            .push(Committed::new(Kind::Human, "Keep a compact brief."));
        state.transcript.push(Committed::new(
            Kind::Reply,
            "I will keep each update concise.",
        ));
        let before = state.transcript.clone();
        assert_eq!(height(&state, area, (None, None)), 2);
        let mut terminal = Terminal::new(TestBackend::new(40, 4)).expect("terminal");
        terminal
            .draw(|frame| render(frame, &state, area, (None, None)))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let rows: Vec<String> = (0..4)
            .map(|y| {
                (0..40)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect();
        // Short of its pane at its live position, the plan stands on its last row.
        assert_eq!(
            rows,
            [
                String::new(),
                String::new(),
                format!("{} Keep a compact brief.", if ascii { ">" } else { "›" }),
                "I will keep each update concise.".to_owned(),
            ]
        );
        assert_eq!(state.transcript, before);
    }
}

/// A short conversation at its live position stands on its last row, in
/// cards, slabs and the compact form alike: the latest exchange touches the
/// decision under it and the empty rows go above it. Its measure is the
/// same, and scrolled back it keeps the offsets it was measured with.
#[test]
fn a_short_conversation_stands_on_its_last_row() {
    for (width, tall) in [(40, 5), (40, 12), (44, 21)] {
        let at = format!("{width}x{tall}");
        let mut state = UiState::new(Presentation::Workspace, false, (120, 40));
        state
            .transcript
            .push(Committed::new(Kind::Human, "Keep a compact brief."));
        state.transcript.push(Committed::new(
            Kind::Reply,
            "I will keep each update concise.",
        ));
        let area = Rect::new(0, 0, width, tall);
        let rows = height(&state, area, (None, None));
        assert!(rows < usize::from(tall), "{at}: a short plan");
        let (shown, _) = drawn(&state, area);
        let above = usize::from(tall) - rows;
        let blank = shown[..above].iter().all(|row| row.trim().is_empty());
        assert!(blank, "{at}: the empty rows above\n{shown:#?}");
        let first = shown[above].trim();
        assert!(!first.is_empty(), "{at}: its first row\n{shown:#?}");
        // The latest words end within the last piece's rows: its edge and air.
        let latest = shown.iter().rposition(|row| row.contains("I will keep"));
        let touches = latest.is_some_and(|row| row + 3 >= rows + above);
        assert!(touches, "{at}\n{shown:#?}");
        state.focus_scroll = 1;
        let measured = height(&state, area, (None, None));
        assert_eq!(measured, rows, "{at}: the measure");
        let (scrolled, _) = drawn(&state, area);
        let top = scrolled[0].trim();
        assert!(!top.is_empty(), "{at}: scrolled\n{scrolled:#?}");
    }
}

/// The knowledge refusal reads as its four sentences at every width, glyph
/// column and colour mode, never as its root or its codes; the measured
/// height ends on the bottom border, and the transcript keeps its words.
#[test]
fn a_recognised_refusal_reads_as_its_summary_at_every_width() {
    let refusal = knowledge_refusal(ROOT);
    for ascii in [false, true] {
        for color in [false, true] {
            for width in [24, 44, 72, 120] {
                let mut state = UiState::new(Presentation::Workspace, color, (width, 40));
                state.ascii = ascii;
                state.transcript.push(Committed::new(
                    Kind::Human,
                    "read notes.md and write digest.md",
                ));
                state
                    .transcript
                    .push(Committed::new(Kind::Refusal, refusal.clone()));
                let (rows, buffer) = painted(&state, width, 40);
                let glyph = if ascii { "x" } else { "✖" };
                let summary = format!(
                    "{glyph} Nika cannot verify the knowledge release named by NIKA_KNOWLEDGE. This authoring request was not sent; no write. Earlier routing may have reached the model. Next: quit and restart Nika with NIKA_KNOWLEDGE unset (built-in knowledge) or NIKA_KNOWLEDGE=off. Details: F2"
                );
                let case = format!("width {width} ascii {ascii} color {color}");
                assert!(words(&rows).ends_with(&summary), "{case}: {rows:#?}");
                let all = rows.join("\n");
                for wall in [ROOT, "ADMISSION_UNTRUSTED", "NIKA_AUTHORING_STRATEGY"] {
                    assert!(!all.contains(wall), "{case}: {wall} in {all}");
                }
                assert!(all.contains("Could not continue"), "{case}: {all}");
                // A card is its title, body and bottom border, then one gap row.
                assert!(rows.len() >= 4, "{case}");
                let (foot, gap) = (&rows[rows.len() - 2], &rows[rows.len() - 1]);
                assert!(foot.starts_with(if ascii { "+" } else { "╰" }), "{case}");
                assert!(gap.trim().is_empty(), "{case}: {gap:?}");
                assert!(!ascii || all.is_ascii(), "{case}: {all}");
                assert!(
                    color
                        || buffer
                            .content()
                            .iter()
                            .all(|cell| cell.fg == ratatui::style::Color::Reset),
                    "{case}"
                );
                assert_eq!(state.transcript[1].text, refusal, "{case}");
            }
        }
    }
}

/// In a split too short for cards, the summary's end is what shows, and
/// the measure counts the same lines.
#[test]
fn a_short_split_shows_the_summary_end() {
    let area = Rect::new(0, 0, 40, 5);
    let mut state = UiState::new(Presentation::Workspace, false, (40, 5));
    let block = Committed::new(Kind::Refusal, knowledge_refusal(ROOT));
    state.transcript.push(block.clone());
    assert_eq!(
        height(&state, area, (None, None)),
        content_rows(&card_lines(&block, false, false), 40)
    );
    let mut terminal = Terminal::new(TestBackend::new(40, 5)).expect("terminal");
    terminal
        .draw(|frame| render(frame, &state, area, (None, None)))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let last: String = (0..40).map(|x| buffer[(x, 4)].symbol()).collect();
    assert_eq!(last.trim_end(), "  Details: F2");
}

/// A provider failure may have sent its call: its card paints the
/// Session's words exactly, the uncertain scope included, and claims
/// nothing more.
#[test]
fn a_provider_failure_card_paints_the_session_words() {
    let block = Committed::new(Kind::Refusal, provider_failure());
    for (color, ascii) in [(false, false), (true, true)] {
        assert_eq!(
            card_lines(&block, color, ascii),
            block_lines(&block, color, ascii)
        );
    }
    let mut state = UiState::new(Presentation::Workspace, false, (60, 40));
    state.transcript.push(block.clone());
    let (rows, _) = painted(&state, 60, 40);
    let words = words(&rows);
    assert!(
        words.starts_with("✖ I couldn't use the authoring seat for this part — "),
        "{words}"
    );
    assert!(
        words.contains("a failed call can still have been sent."),
        "{words}"
    );
    assert!(
        !words.to_lowercase().contains("nothing was sent"),
        "{words}"
    );
    assert_eq!(state.transcript[0], block);
}

/// The opening banner warns in one short line; its other lines, and the
/// block, stay as the Session wrote them. It names itself: its bubble
/// carries no second speaker label.
#[test]
fn the_banner_card_warns_in_one_short_line() {
    let said = opening_banner(ROOT);
    let mut state = UiState::new(Presentation::Workspace, false, (72, 40));
    state
        .transcript
        .push(Committed::new(Kind::Banner, said.clone()));
    let (rows, _) = painted(&state, 72, 40);
    let all = words(&rows);
    let warning = WARNING_LINE
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        all.starts_with("Nika ·") && all.ends_with(&warning),
        "{all}"
    );
    assert!(
        !all.contains(ROOT) && !all.contains("ADMISSION_UNTRUSTED"),
        "{all}"
    );
    assert!(!rows.iter().any(|row| row.contains("─ Nika")), "{rows:#?}");
    assert_eq!(state.transcript[0].text, said);
}

#[test]
fn compact_view_keeps_the_tail_beyond_u16_rows() {
    let area = Rect::new(0, 0, 30, 5);
    let mut state = UiState::new(Presentation::Workspace, false, (30, 5));
    state.transcript.push(Committed::new(
        Kind::Reply,
        "row\n".repeat(70_000) + "final detail",
    ));
    assert!(height(&state, area, (None, None)) > usize::from(u16::MAX));
    let mut terminal = Terminal::new(TestBackend::new(30, 5)).expect("terminal");
    terminal
        .draw(|frame| render(frame, &state, area, (None, None)))
        .expect("compact draws");
    let shown = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect::<String>();
    assert!(shown.contains("final detail"), "{shown}");
}

#[test]
fn complete_cards_keep_both_borders_and_padding_at_every_width() {
    for ascii in [false, true] {
        for width in [6, 12, 22, 44, 72] {
            let mut state = UiState::new(Presentation::Workspace, true, (width, 8));
            state.ascii = ascii;
            state.transcript.push(Committed::new(Kind::Proposal, "ok"));
            let mut terminal = Terminal::new(TestBackend::new(width, 8)).expect("terminal");
            terminal
                .draw(|frame| render(frame, &state, frame.area(), (None, None)))
                .expect("draw");
            let buffer = terminal.backend().buffer();
            // The complete short card is live at the bottom, with one row
            // of air below it. Its borders and padding keep their cells.
            for y in 0..4 {
                for x in 0..width {
                    assert_eq!(buffer[(x, y)].symbol(), " ");
                }
            }
            assert_eq!(
                buffer[(width - 1, 4)].symbol(),
                if ascii { "+" } else { "╮" }
            );
            assert_eq!(
                buffer[(width - 1, 5)].symbol(),
                if ascii { "|" } else { "│" }
            );
            assert_eq!(
                buffer[(width - 1, 6)].symbol(),
                if ascii { "+" } else { "╯" }
            );
            assert_eq!(buffer[(1, 5)].symbol(), " ");
            assert_eq!(buffer[(width - 2, 5)].symbol(), " ");
            assert_eq!(buffer[(2, 5)].symbol(), "o");
            assert_eq!(buffer[(3, 5)].symbol(), "k");
        }
    }
}

#[test]
fn wrapped_message_rows_remain_reachable_without_repeated_frames() {
    let area = Rect::new(0, 0, 20, 6);
    let mut state = UiState::new(Presentation::Workspace, false, (20, 6));
    state.transcript.push(Committed::new(
        Kind::Reply,
        "12345678901234567\n".repeat(8) + "last row",
    ));
    assert_eq!(
        height(&state, area, (None, None)),
        11,
        "17 cells fit in the full 20-cell conversation body"
    );
    let mut terminal = Terminal::new(TestBackend::new(20, 6)).expect("terminal");
    let mut saw_first = false;
    let mut saw_last = false;
    for scroll in 0..=height(&state, area, (None, None)) - usize::from(area.height) {
        state.focus_scroll = scroll;
        terminal
            .draw(|frame| render(frame, &state, area, (None, None)))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let rows: Vec<String> = (0..6)
            .map(|y| (0..20).map(|x| buffer[(x, y)].symbol()).collect())
            .collect();
        saw_first |= rows[0].contains("Nika");
        saw_last |= rows.iter().any(|row| row.contains("last row"));
        assert!(!rows.iter().any(|row| row.contains(['╭', '╰', '│'])));
    }
    assert!(saw_first && saw_last, "both ends stay reachable");
}

#[test]
fn a_clipped_card_keeps_the_end_and_scrolling_reveals_the_previous_turn() {
    let mut state = UiState::new(Presentation::Workspace, true, (35, 12));
    state
        .transcript
        .push(Committed::new(Kind::Human, "earlier intent"));
    state.transcript.push(Committed::new(
        Kind::Reply,
        "a long reply\n".repeat(30) + "the final detail",
    ));
    let mut terminal = Terminal::new(TestBackend::new(35, 12)).expect("test terminal");
    let text = |buffer: &ratatui::buffer::Buffer| {
        (0..12)
            .map(|y| (0..35).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    };
    terminal
        .draw(|frame| render(frame, &state, frame.area(), (None, None)))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    assert!(text(buffer).contains("the final detail"));
    assert!(!text(buffer).contains("earlier intent"));
    assert!(
        buffer
            .content()
            .iter()
            .all(|cell| cell.bg == ratatui::style::Color::Reset),
        "a scrolled reply remains on the conversation surface"
    );
    state.focus_scroll = usize::MAX;
    state.color = false;
    state.ascii = true;
    terminal
        .draw(|frame| render(frame, &state, frame.area(), (None, None)))
        .expect("draw older");
    let buffer = terminal.backend().buffer();
    assert!(text(buffer).contains("earlier intent"));
    assert!(text(buffer).is_ascii());
    assert!(
        buffer
            .content()
            .iter()
            .all(|cell| cell.fg == ratatui::style::Color::Reset)
    );
}

/// The Session's words of a typed question, as its block holds them.
const ASKED: &str = "the currency code\n    (The compiler cannot invent this authoring value.)\nreply on the next line · `cancel` drops this";

/// The witness that question was asked as, in this session.
const WITNESS: &str = "3:q-currency";

/// A workspace of `size` waiting on that typed question, its words in a block
/// tagged `tagged` (untagged when `None`), after the human's request.
fn asking(size: (u16, u16), tagged: Option<&str>) -> UiState {
    let mut state = UiState::new(Presentation::Workspace, false, size);
    state
        .transcript
        .push(Committed::new(Kind::Human, "sum the amounts"));
    state.transcript.push(match tagged {
        Some(witness) => Committed::question(witness, ASKED),
        None => Committed::new(Kind::Question, ASKED),
    });
    let asked = Asked::new("the currency code", "", true, Shape::Text, WITNESS, 3);
    state.waiting = Waiting::asked("const.currency", asked);
    state
}

/// Only the latest question block tied to the typed question waiting now is
/// carried: another epoch, another identity or an untagged block keeps every
/// word, and so do a later question in prose and a later summarized refusal
/// (whose details `F2` reads); and nothing is carried outside the fitting
/// workspace or while anything else waits.
#[test]
fn the_live_question_is_tied_to_its_block_by_identity_alone() {
    use crate::render::question::asked_block;
    let tied = asking((120, 40), Some(WITNESS));
    assert_eq!(asked_block(&tied), Some(1));
    for witness in [Some("4:q-currency"), Some("3:q-other"), None] {
        assert_eq!(
            asked_block(&asking((120, 40), witness)),
            None,
            "{witness:?}"
        );
    }
    let mut noticed = tied.clone();
    noticed
        .transcript
        .push(Committed::new(Kind::Notice, "not taken"));
    assert_eq!(asked_block(&noticed), Some(1), "a notice changes nothing");
    let mut prose = tied.clone();
    prose.transcript.push(Committed::new(Kind::Question, ASKED));
    assert_eq!(asked_block(&prose), None, "a later question in prose");
    let mut refused = tied.clone();
    let refusal = Committed::new(Kind::Refusal, knowledge_refusal(ROOT));
    refused.transcript.push(refusal);
    assert_eq!(asked_block(&refused), None, "a later summarized refusal");
    let mut again = tied.clone();
    again.transcript.push(Committed::question(WITNESS, ASKED));
    assert_eq!(asked_block(&again), Some(2), "the latest occurrence alone");
    for presentation in [Presentation::Inline, Presentation::Focus] {
        let mut elsewhere = tied.clone();
        elsewhere.presentation = presentation;
        assert_eq!(asked_block(&elsewhere), None, "{presentation:?}");
    }
    let mut small = tied.clone();
    small.size = (59, 15);
    assert_eq!(asked_block(&small), None, "the focus view stands in");
    let legacy = Waiting::Question {
        key: "const.currency".to_owned(),
    };
    for waiting in [Waiting::Free, Waiting::Proposal, Waiting::Gate, legacy] {
        let mut other = tied.clone();
        other.waiting = waiting;
        assert_eq!(asked_block(&other), None, "{:?}", other.waiting);
    }
}

/// The carried question reads one quiet row where its block stood, in both
/// forms and glyph columns: its words wait at the answer line, the block is
/// untouched, and the same conversation uncarried keeps the words.
#[test]
fn the_carried_question_reads_one_quiet_row() {
    for (width, tall) in [(44, 29), (68, 37), (40, 5)] {
        for ascii in [false, true] {
            let at = format!("{width}x{tall} ascii {ascii}");
            let mut state = asking((120, 40), Some(WITNESS));
            state.ascii = ascii;
            let before = state.transcript.clone();
            let area = Rect::new(0, 0, width, tall);
            let (rows, _) = framed(&state, area, (None, Some(1)));
            let shown = rows.join("\n");
            let marker = if ascii {
                "v the question waits below"
            } else {
                "↓ the question waits below"
            };
            assert!(shown.contains(marker), "{at}\n{shown}");
            assert!(!shown.contains("cannot invent"), "{at}\n{shown}");
            assert!(!shown.contains("drops this"), "{at}\n{shown}");
            assert_eq!(state.transcript, before, "{at}");
            let carried = height(&state, area, (None, Some(1)));
            let said = height(&state, area, (None, None));
            assert!(carried < said, "{at}: {carried} {said}");
        }
    }
}

/// A conversation carrying the live question scrolls as one window over
/// exactly the rows its measure counts, its last piece the quiet row: twelve
/// earlier turns, each speaker's own bubble, overflow every transcript.
#[test]
fn a_carried_conversation_scrolls_as_one_window() {
    for (width, tall) in [(44, 29), (68, 37), (24, 16)] {
        let mut state = asking((120, 40), Some(WITNESS));
        let earlier = (0..12).flat_map(|n| {
            [
                Committed::new(Kind::Human, format!("earlier request {n}")),
                Committed::new(Kind::Reply, format!("earlier reply {n}")),
            ]
        });
        let mut transcript: Vec<Committed> = earlier.collect();
        transcript.append(&mut state.transcript);
        state.transcript = transcript;
        let carried = crate::render::question::asked_block(&state);
        assert_eq!(carried, Some(25));
        let case = format!("{width}x{tall}");
        let area = Rect::new(0, 0, width, tall);
        assert_one_window(&mut state, area, (None, carried), &case);
    }
}

/// A card entered part-way names itself and the rows of its words above on
/// its first row shown, in both glyph columns and without a hue; its measure
/// is unchanged, the row its heading covers is read one row back, every row
/// is read at some scroll, and the furthest scroll shows its own top.
#[test]
fn a_card_entered_part_way_names_itself_and_the_rows_above() {
    let said: Vec<String> = (1..=12).map(|n| format!("gate row {n}")).collect();
    let area = Rect::new(0, 0, 44, 10);
    for ascii in [false, true] {
        let mut state = UiState::new(Presentation::Workspace, false, (120, 40));
        state.ascii = ascii;
        let gate = Committed::new(Kind::Gate, said.join("\n"));
        state.transcript.push(gate);
        let lines = card_lines(&state.transcript[0], false, ascii);
        let rows = content_rows(&lines, area.width - 4);
        assert_eq!(rows, said.len(), "one row per line");
        assert_eq!(height(&state, area, (None, None)), card_height(rows));
        let (corner, sep, up) = if ascii {
            ("+-", "-", "^")
        } else {
            ("╭─", "·", "↑")
        };
        let (most, frames) = windows(&mut state, area, (None, None));
        assert_eq!(most, card_height(rows) - usize::from(area.height));
        for (scroll, frame) in frames.iter().enumerate().take(most + 1) {
            let shown = rows_of(frame);
            let at = format!("ascii {ascii} scroll {scroll}\n{shown:#?}");
            // The only piece: the rows above the window are the card's own.
            let offset = most - scroll;
            let title = format!("{corner} Approval needed ");
            assert!(shown[0].starts_with(&title), "{at}");
            assert_eq!(declared(frame), Some(offset), "{at}");
            if offset > 0 {
                let plural = if offset == 1 { "" } else { "s" };
                let count = format!("{title}{sep} {up} {offset} row{plural} ");
                assert!(shown[0].starts_with(&count), "{at}");
                // The first row of words shown is the one after the covered row.
                let next = format!("gate row {} ", offset + 1);
                assert!(shown[1].contains(&next), "{at}");
                let covered = format!("gate row {offset} ");
                let back = rows_of(&frames[scroll + 1]);
                assert!(back[1].contains(&covered), "{at}");
            }
            assert!(!ascii || shown.iter().all(|row| row.is_ascii()), "{at}");
            let hued = (frame.content.iter())
                .any(|cell| cell.fg != Color::Reset || cell.bg != Color::Reset);
            assert!(!hued, "{at}");
        }
        for line in &said {
            let read = (frames.iter()).any(|frame| {
                let line = format!("{line} ");
                rows_of(frame).iter().any(|row| row.contains(&line))
            });
            assert!(read, "ascii {ascii}: {line}");
        }
        assert_one_window(&mut state, area, (None, None), &format!("ascii {ascii}"));
    }
}

/// A question in prose (a cost decision) keeps its warning card in the
/// transcript; the carried question's row is quiet, never the warning hue.
#[test]
fn a_cost_question_keeps_its_warning_and_the_carried_row_stays_quiet() {
    let area = Rect::new(0, 0, 44, 29);
    let warn = role::style(Role::Warn, true).fg.unwrap_or(Color::Reset);
    let mut state = asking((120, 40), Some(WITNESS));
    state.color = true;
    let (rows, buffer) = framed(&state, area, (None, Some(1)));
    let y = rows
        .iter()
        .position(|row| row.contains("waits below"))
        .expect("the row");
    let quiet = role::style(Role::Dim, true).fg.unwrap_or(Color::Reset);
    let at = u16::try_from(y).expect("a row");
    assert_eq!(buffer[(2, at)].fg, quiet);
    assert_ne!(buffer[(2, at)].fg, warn);
    state.transcript[1] = Committed::new(Kind::Question, "approve spending $0.20 once?");
    state.waiting = Waiting::Question {
        key: "unknown_cost".to_owned(),
    };
    let (rows, buffer) = framed(&state, area, (None, None));
    let y = rows
        .iter()
        .position(|row| row.contains("Question"))
        .expect("the card");
    let x = rows[y].chars().position(|c| c == 'Q').expect("its title");
    let cell = &buffer[(
        u16::try_from(x).expect("a column"),
        u16::try_from(y).expect("a row"),
    )];
    assert_eq!(cell.fg, warn, "{rows:#?}");
}

/// A run's story folded at its commit under `label`, settled `state`.
fn folded(label: &str, state: &'static str, story: &str) -> Committed {
    let mut block = Committed::new(Kind::Run, story);
    block.run = Some((label.to_owned(), state));
    block
}

/// A run's story folded at its commit reads as one quiet row under the run's
/// sublabel: its label, its settled state and the reader key, in both glyph
/// columns and both forms, none of the story's words painted; the same story
/// untagged keeps every word and offers no key.
#[test]
fn a_folded_run_story_reads_as_one_quiet_row() {
    for (tall, ascii) in [(14, false), (14, true), (24, false), (24, true)] {
        let case = format!("rows {tall} ascii {ascii}");
        let mut state = UiState::new(Presentation::Workspace, true, (60, tall));
        state.ascii = ascii;
        for block in [
            Committed::new(Kind::Human, "run it"),
            Committed::new(Kind::Report, "check · digest.nika · ok"),
            folded(
                "run 0123456789ab",
                "failed",
                "running · digest\n  read_notes · 2 ms\nfailed · 0/1 tasks",
            ),
            Committed::new(Kind::Result, "the run failed: see its journal"),
        ] {
            state.transcript.push(block);
        }
        let (rows, buffer) = painted(&state, 60, tall);
        let sep = if ascii { " - " } else { " · " };
        let fold = format!("run 0123456789ab{sep}failed{sep}F2");
        let at: Vec<usize> = (rows.iter().enumerate())
            .filter(|(_, row)| row.contains(&fold))
            .map(|(y, _)| y)
            .collect();
        assert_eq!(at.len(), 1, "{case}: one row\n{rows:#?}");
        let x = rows[at[0]].find("run 0123").expect("the fold's words");
        let x = u16::try_from(rows[at[0]][..x].chars().count()).expect("a column");
        let y = u16::try_from(at[0]).expect("a row");
        let dim = role::style(Role::Dim, true).fg;
        assert_eq!(buffer[(x, y)].fg, dim.expect("a dim hue"), "{case}: quiet");
        assert!(
            rows.iter()
                .any(|row| row.trim().trim_matches(['│', '|']).trim() == "Run")
        );
        assert!(!rows.join("").contains("read_notes"), "{case}\n{rows:#?}");
        state.transcript[2].run = None;
        let (rows, _) = painted(&state, 60, tall);
        assert!(rows.join("").contains("read_notes"), "{case}\n{rows:#?}");
        assert!(!rows.join("").contains("F2"), "{case}\n{rows:#?}");
    }
}

/// The plan names, on each painted row, the block painted there
/// ([`shown_at`]): at every scroll position, in both forms, inside bubbles
/// and cards alike, one block a row at most, and over the whole window the
/// same blocks, the newest first.
#[test]
fn each_row_names_the_block_painted_on_it() {
    let said = [
        (Kind::Banner, "alpha banner"),
        (Kind::Human, "bravo"),
        (Kind::Reply, "charlie"),
        (Kind::Question, "delta?"),
        (Kind::Human, "echo"),
        (Kind::Report, "foxtrot"),
        (Kind::Run, "golf"),
        (Kind::Result, "india"),
        (Kind::Refusal, "juliet refused"),
        (Kind::Human, "kilo"),
        (Kind::Reply, "lima"),
        (Kind::Reply, "mike"),
    ];
    let tokens = [
        "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "india", "juliet", "kilo",
        "lima", "mike",
    ];
    for area in [Rect::new(0, 0, 40, 12), Rect::new(0, 0, 40, 18)] {
        let mut state = UiState::new(Presentation::Workspace, false, (40, area.height));
        for (kind, text) in said {
            state.transcript.push(Committed::new(kind, text));
        }
        let (most, frames) = windows(&mut state, area, (None, None));
        assert!(most > 0, "{area:?}: the conversation scrolls");
        for (scroll, frame) in frames.iter().enumerate().take(most + 1) {
            state.focus_scroll = scroll;
            let rows = rows_of(frame);
            let mut named = Vec::new();
            for (y, row) in rows.iter().enumerate() {
                let here = shown_at(&state, area, (None, None), y..y + 1);
                assert!(
                    here.len() <= 1,
                    "{area:?} scroll {scroll} row {y}: {here:?}"
                );
                for (block, token) in tokens.iter().enumerate() {
                    if row.contains(token) {
                        assert_eq!(here, [block], "{area:?} scroll {scroll} row {y}: {row}");
                    }
                }
                named.extend(here);
            }
            named.dedup();
            named.reverse();
            let whole = shown_at(&state, area, (None, None), 0..rows.len());
            assert_eq!(whole, named, "{area:?} scroll {scroll}");
        }
    }
}
