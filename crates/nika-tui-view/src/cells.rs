// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Cells: the one place a viewer measures, cleans, cuts and wraps text.
//!
//! Widths are counted grapheme by grapheme with the table the buffer uses,
//! so a cluster the terminal draws in one piece (an emoji sequence, a
//! letter and its combining marks) is measured in one piece and never
//! split. A control character never reaches a cell: a tab becomes spaces,
//! a C0 control its picture (its caret form in the ASCII column), a C1
//! control or a bidirectional override its code point in brackets. An
//! escape sequence in a file cannot drive the terminal, and a hidden
//! direction override cannot reorder what the human reads.

use nika_display::theme::Role;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::mask::Lines;
use super::{Canvas, Format, Note, Rendered};
use crate::role;

/// The tab stop a tab expands to.
const TAB: usize = 4;

/// The byte-order mark some UTF-8 files start with.
const BOM: &[u8] = b"\xEF\xBB\xBF";

/// The mark a line ends with when it continues past the edge.
pub(crate) const fn cut_mark(ascii: bool) -> &'static str {
    if ascii { "..." } else { "…" }
}

/// The separator between facts.
pub(crate) const fn sep(ascii: bool) -> &'static str {
    if ascii { " - " } else { " · " }
}

/// The mark one invalid UTF-8 sequence shows as.
pub(crate) const fn invalid_mark(ascii: bool) -> &'static str {
    if ascii { "?" } else { "\u{FFFD}" }
}

/// `n` as a `u64` (saturating on a platform wider than 64 bits).
pub(crate) fn wide(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

/// A byte count in words, decimal units: `12 bytes`, `3.4 KB`, `1.0 MB`.
pub(crate) fn size_words(n: usize) -> String {
    size_words_u64(wide(n))
}

/// [`size_words`] for a declared 64-bit size.
#[allow(clippy::cast_precision_loss)] // a display magnitude with one decimal
pub(crate) fn size_words_u64(n: u64) -> String {
    match n {
        1 => "1 byte".to_owned(),
        n if n < 1_000 => format!("{n} bytes"),
        n if n < 1_000_000 => format!("{:.1} KB", n as f64 / 1e3),
        n if n < 1_000_000_000 => format!("{:.1} MB", n as f64 / 1e6),
        n => format!("{:.1} GB", n as f64 / 1e9),
    }
}

/// Whether every byte of `text` is printable ASCII (one cell each).
fn printable_ascii(text: &str) -> bool {
    text.bytes().all(|b| (0x20..0x7f).contains(&b))
}

/// Visit the graphemes of `text` with their widths in cells, until
/// `visit` returns false.
pub(crate) fn each_grapheme(text: &str, mut visit: impl FnMut(&str, usize) -> bool) {
    let span = Span::raw(text);
    for grapheme in span.styled_graphemes(Style::default()) {
        if !visit(grapheme.symbol, grapheme.symbol.width()) {
            break;
        }
    }
}

/// The width of `text` in cells, measured grapheme by grapheme.
pub(crate) fn width(text: &str) -> usize {
    if printable_ascii(text) {
        return text.len();
    }
    let mut total = 0;
    each_grapheme(text, |_, w| {
        total += w;
        true
    });
    total
}

/// The visible form of a control character, `None` for any other.
fn visible(c: char, ascii: bool) -> Option<String> {
    let code = u32::from(c);
    match code {
        0x00..=0x1F if ascii => {
            let letter = char::from_u32(code + 0x40)?;
            Some(format!("^{letter}"))
        }
        0x00..=0x1F => char::from_u32(0x2400 + code).map(String::from),
        0x7F if ascii => Some("^?".to_owned()),
        0x7F => Some("\u{2421}".to_owned()),
        0x80..=0x9F | 0x202A..=0x202E | 0x2066..=0x2069 => Some(format!("<U+{code:04X}>")),
        _ => None,
    }
}

/// One line of text with every control character made visible, and how
/// many were marked (a tab is layout: it expands, uncounted).
pub(crate) fn clean(text: &str, ascii: bool) -> (String, usize) {
    if printable_ascii(text) {
        return (text.to_owned(), 0);
    }
    let mut out = String::with_capacity(text.len());
    let mut column = 0;
    let mut marked = 0;
    for c in text.chars() {
        if c == '\t' {
            let pad = TAB - column % TAB;
            out.extend(std::iter::repeat_n(' ', pad));
            column += pad;
        } else if let Some(mark) = visible(c, ascii) {
            column += mark.chars().count();
            out.push_str(&mark);
            marked += 1;
        } else {
            out.push(c);
            column += c.width().unwrap_or(0);
        }
    }
    (out, marked)
}

/// Text decoded from at most `max` bytes of the input.
pub(crate) struct Decoded {
    /// The text, each invalid sequence replaced by one mark.
    pub(crate) text: String,
    /// Invalid sequences met.
    pub(crate) invalid: usize,
    /// Input bytes consumed (all of them unless the bound cut the input).
    pub(crate) read: usize,
}

/// Decode `bytes` as UTF-8: a leading byte-order mark dropped, the bound
/// backed off to a character boundary, each invalid sequence one mark.
pub(crate) fn decode(bytes: &[u8], max: usize, ascii: bool) -> Decoded {
    let body = bytes.strip_prefix(BOM).unwrap_or(bytes);
    let skipped = bytes.len() - body.len();
    let mut end = body.len().min(max);
    let mut back = 0;
    while end > 0 && end < body.len() && back < 3 && body[end] & 0xC0 == 0x80 {
        end -= 1;
        back += 1;
    }
    let mut text = String::with_capacity(end);
    let mut invalid = 0;
    for chunk in body[..end].utf8_chunks() {
        text.push_str(chunk.valid());
        if !chunk.invalid().is_empty() {
            invalid += 1;
            text.push_str(invalid_mark(ascii));
        }
    }
    Decoded {
        text,
        invalid,
        read: skipped + end,
    }
}

/// The largest index at most `at` that is a character boundary of `text`.
pub(crate) fn floor_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The lines of `text` (split on `\n`, a trailing `\r` dropped, one final
/// newline ignored), each cut to at most `max` bytes on a character
/// boundary, with whether it was cut.
pub(crate) fn lines(text: &str, max: usize) -> impl Iterator<Item = (&str, bool)> {
    let text = text.strip_suffix('\n').unwrap_or(text);
    text.split('\n').map(move |line| {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.len() <= max {
            (line, false)
        } else {
            (&line[..floor_boundary(line, max)], true)
        }
    })
}

/// Words written for the Unicode column (their separators, cut marks,
/// arrows and multiplication signs) in the glyph column in use.
pub(crate) fn dots(text: &str, ascii: bool) -> String {
    if !ascii || text.is_ascii() {
        return text.to_owned();
    }
    text.replace('·', "-")
        .replace('…', "...")
        .replace('→', "->")
        .replace('×', "x")
        .replace(['—', '–'], "-")
}

/// `n` and its noun, singular for one (`1 task`, `3 tasks`).
pub(crate) fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// A span in a role's style.
pub(crate) fn paint(text: impl Into<String>, role: Role, color: bool) -> Span<'static> {
    Span::styled(text.into(), role::style(role, color))
}

/// A span in the default style.
pub(crate) fn plain(text: impl Into<String>) -> Span<'static> {
    Span::raw(text.into())
}

/// `spans` fitted to `width` cells: kept whole when they fit and nothing
/// follows (`more` false), else cut at a grapheme boundary and ended by
/// the cut mark (dim). Returns the line and whether it was cut.
pub(crate) fn fit(
    spans: Vec<Span<'static>>,
    width: usize,
    more: bool,
    canvas: Canvas,
) -> (Line<'static>, bool) {
    let total: usize = spans.iter().map(|s| self::width(&s.content)).sum();
    if total <= width && !more {
        return (Line::from(spans), false);
    }
    let mark = cut_mark(canvas.ascii);
    let Some(room) = width.checked_sub(self::width(mark)) else {
        return (Line::default(), true);
    };
    let mut out: Vec<Span<'static>> = Vec::with_capacity(spans.len() + 1);
    let mut used = 0;
    for span in spans {
        let mut kept = String::new();
        let mut stopped = false;
        each_grapheme(&span.content, |grapheme, w| {
            if used + w > room {
                stopped = true;
                return false;
            }
            used += w;
            kept.push_str(grapheme);
            true
        });
        if !kept.is_empty() {
            out.push(Span::styled(kept, span.style));
        }
        if stopped {
            break;
        }
    }
    out.push(paint(mark, Role::Dim, canvas.color));
    (Line::from(out), true)
}

/// One grapheme of a paragraph being wrapped: the span it belongs to, its
/// byte range in that span, and its width. Ranges, not copies: a paragraph
/// allocates once per row, never once per grapheme.
#[derive(Clone, Copy)]
struct Cell {
    span: usize,
    start: usize,
    end: usize,
    width: usize,
}

/// The graphemes of every span as cells (printable ASCII byte by byte,
/// anything else through the grapheme table).
fn graphemes_of(spans: &[Span<'static>]) -> Vec<Cell> {
    let mut cells = Vec::new();
    for (span, piece) in spans.iter().enumerate() {
        let content: &str = &piece.content;
        if printable_ascii(content) {
            cells.extend((0..content.len()).map(|start| Cell {
                span,
                start,
                end: start + 1,
                width: 1,
            }));
            continue;
        }
        let mut at = 0;
        each_grapheme(content, |grapheme, width| {
            let start = content[at..]
                .find(grapheme)
                .map_or(at, |offset| at + offset);
            let end = start + grapheme.len();
            cells.push(Cell {
                span,
                start,
                end,
                width,
            });
            at = end;
            true
        });
    }
    cells
}

/// Consecutive cells of one span merged back into spans.
fn join(spans: &[Span<'static>], cells: &[Cell]) -> Vec<Span<'static>> {
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut run: Option<(usize, usize, usize)> = None;
    let close = |run: (usize, usize, usize), out: &mut Vec<Span<'static>>| {
        let (span, start, end) = run;
        let piece = &spans[span];
        out.push(Span::styled(
            piece.content[start..end].to_owned(),
            piece.style,
        ));
    };
    for cell in cells {
        run = match run {
            Some((span, start, end)) if span == cell.span && end == cell.start => {
                Some((span, start, cell.end))
            }
            Some(open) => {
                close(open, &mut out);
                Some((cell.span, cell.start, cell.end))
            }
            None => Some((cell.span, cell.start, cell.end)),
        };
    }
    if let Some(open) = run {
        close(open, &mut out);
    }
    out
}

/// `spans` wrapped to `width` cells at spaces (a word wider than the width
/// breaks between graphemes), at most `rows` rows. Returns the rows and
/// whether text was left over.
pub(crate) fn wrap(
    spans: &[Span<'static>],
    width: usize,
    rows: usize,
) -> (Vec<Vec<Span<'static>>>, bool) {
    let width = width.max(1);
    let cells = graphemes_of(spans);
    let blank = |cell: &Cell| &spans[cell.span].content[cell.start..cell.end] == " ";
    let mut out = Vec::new();
    let mut at = 0;
    while at < cells.len() {
        if out.len() == rows {
            return (out, true);
        }
        let start = at;
        let mut used = 0;
        let mut space = None;
        while at < cells.len() && used + cells[at].width <= width {
            if blank(&cells[at]) {
                space = Some(at);
            }
            used += cells[at].width;
            at += 1;
        }
        if at < cells.len() {
            if let Some(space) = space.filter(|s| *s > start) {
                at = space;
            }
            if at == start {
                at += 1;
            }
        }
        out.push(join(spans, &cells[start..at]));
        while at < cells.len() && blank(&cells[at]) {
            at += 1;
        }
    }
    (out, false)
}

/// The lines a view produces, bounded by its canvas: each fitted to the
/// width, their count capped, every cut counted for the notes.
pub(crate) struct Body {
    canvas: Canvas,
    lines: Vec<Line<'static>>,
    cut: usize,
    full: bool,
}

impl Body {
    /// An empty body for `canvas`.
    pub(crate) fn new(canvas: Canvas) -> Self {
        Self {
            canvas,
            lines: Vec::new(),
            cut: 0,
            full: false,
        }
    }

    /// The canvas width in cells.
    pub(crate) fn width(&self) -> usize {
        usize::from(self.canvas.width)
    }

    /// The canvas the body fits.
    pub(crate) fn canvas(&self) -> Canvas {
        self.canvas
    }

    /// Whether no further line will be kept.
    pub(crate) fn is_full(&self) -> bool {
        self.full || self.lines.len() >= self.canvas.limits.lines
    }

    /// Lines kept so far.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.lines.len()
    }

    /// Record that content followed the last line kept.
    pub(crate) fn overflow(&mut self) {
        self.full = true;
    }

    /// Keep one line, cut at the width (`more`: the line continues past
    /// what is given). False once the body is full: the line was dropped.
    pub(crate) fn push(&mut self, spans: Vec<Span<'static>>, more: bool) -> bool {
        if self.lines.len() >= self.canvas.limits.lines {
            self.full = true;
            return false;
        }
        let (line, cut) = fit(spans, self.width(), more, self.canvas);
        self.cut += usize::from(cut);
        self.lines.push(line);
        true
    }

    /// Keep a paragraph wrapped to the width: `first` leads its first row,
    /// `rest` the others (both the same width). False once full.
    pub(crate) fn wrap(
        &mut self,
        spans: &[Span<'static>],
        first: &[Span<'static>],
        rest: &[Span<'static>],
        more: bool,
    ) -> bool {
        let lead: usize = first.iter().map(|s| width(&s.content)).sum();
        let room = self.canvas.limits.lines.saturating_sub(self.lines.len());
        if room == 0 {
            self.full = true;
            return false;
        }
        let (rows, left) = wrap(spans, self.width().saturating_sub(lead), room);
        let count = rows.len().max(1);
        let mut rows = rows.into_iter();
        for index in 0..count {
            let mut line: Vec<Span<'static>> = if index == 0 {
                first.to_vec()
            } else {
                rest.to_vec()
            };
            line.extend(rows.next().unwrap_or_default());
            let last = index + 1 == count;
            self.push(line, last && more && !left);
        }
        if left {
            self.full = true;
        }
        !left
    }

    /// The lines, the cuts reported into `notes`.
    pub(crate) fn finish(self, notes: &mut Vec<Note>) -> Vec<Line<'static>> {
        if self.cut > 0 {
            notes.push(Note::WidthCut {
                lines: self.cut,
                width: self.canvas.width,
            });
        }
        if self.full {
            notes.push(Note::LinesCut {
                shown: self.lines.len(),
            });
        }
        self.lines
    }
}

/// What a decoder writes: the body, the facts under the title, the notes,
/// and the format it ended up showing (a fallback changes it).
pub(crate) struct Sheet {
    /// The body lines.
    pub(crate) body: Body,
    /// The facts under the title.
    pub(crate) facts: Vec<String>,
    /// What was cut, fell back or disagreed.
    pub(crate) notes: Vec<Note>,
    /// The format shown.
    pub(crate) format: Format,
}

impl Sheet {
    /// An empty sheet for `format` on `canvas`.
    pub(crate) fn new(canvas: Canvas, format: Format) -> Self {
        Self {
            body: Body::new(canvas),
            facts: Vec::new(),
            notes: Vec::new(),
            format,
        }
    }

    /// The rendered object titled `title`: the body's cuts join the notes.
    pub(crate) fn finish(self, title: String) -> Rendered {
        let mut notes = self.notes;
        let lines = self.body.finish(&mut notes);
        Rendered {
            title,
            format: self.format,
            facts: self.facts,
            lines,
            notes,
        }
    }
}

/// What a line-oriented decoder counts for its notes (control characters
/// marked, secret-looking values masked), and the structure the lines of a
/// protected text share: a block, a multi-line string, a bracket or a
/// parent key naming a credential masks what it holds to its last line.
#[derive(Debug, Default)]
pub(crate) struct Tally {
    controls: usize,
    masked: usize,
    lines: Lines,
}

impl Tally {
    /// One raw line made safe to draw: controls marked, and secret-looking
    /// values masked when the object is protected.
    pub(crate) fn line(&mut self, raw: &str, canvas: Canvas, protected: bool) -> String {
        self.mark(raw, false, canvas, protected)
    }

    /// One raw line of a diff: its first column (`+`, `-` or a space) says
    /// what changed and stays; the rest is masked as the line it changes.
    pub(crate) fn changed(&mut self, raw: &str, canvas: Canvas, protected: bool) -> String {
        self.mark(raw, true, canvas, protected)
    }

    fn mark(&mut self, raw: &str, diff: bool, canvas: Canvas, protected: bool) -> String {
        let (text, controls) = clean(raw, canvas.ascii);
        self.controls += controls;
        if !protected {
            return text;
        }
        let lead = usize::from(diff && text.starts_with(['+', '-', ' ']));
        let (rest, hidden) = self.lines.line(&text[lead..], canvas.ascii);
        self.masked += hidden;
        format!("{}{rest}", &text[..lead])
    }

    /// Record values masked elsewhere (a table cell, a JSON value).
    pub(crate) fn masked(&mut self, count: usize) {
        self.masked += count;
    }

    /// The counts as notes.
    pub(crate) fn report(self, notes: &mut Vec<Note>) {
        if self.controls > 0 {
            notes.push(Note::Controls {
                count: self.controls,
            });
        }
        if self.masked > 0 {
            notes.push(Note::Masked { count: self.masked });
        }
    }
}

/// `text` cut to `width` cells with the cut mark, then padded to exactly
/// `width` (on the left when `right`).
pub(crate) fn cell(text: &str, width: usize, right: bool, ascii: bool) -> String {
    let mut out = String::new();
    let mut used = 0;
    if self::width(text) <= width {
        out.push_str(text);
        used = self::width(text);
    } else {
        let mark = cut_mark(ascii);
        let room = width.saturating_sub(self::width(mark));
        each_grapheme(text, |grapheme, w| {
            if used + w > room {
                return false;
            }
            used += w;
            out.push_str(grapheme);
            true
        });
        if self::width(mark) <= width {
            out.push_str(mark);
            used += self::width(mark);
        }
    }
    let pad = " ".repeat(width.saturating_sub(used));
    if right {
        format!("{pad}{out}")
    } else {
        format!("{out}{pad}")
    }
}

/// One labelled row of a facts table: the label dim, the value in its tone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Row {
    /// What the row states.
    pub(crate) label: &'static str,
    /// The value, in words.
    pub(crate) value: String,
    /// The value's role, `None` for the default ink.
    pub(crate) tone: Option<Role>,
}

impl Row {
    /// A row in the default ink.
    pub(crate) fn new(label: &'static str, value: String) -> Self {
        Self {
            label,
            value,
            tone: None,
        }
    }

    /// A row whose value wears `tone`.
    pub(crate) fn toned(label: &'static str, value: String, tone: Role) -> Self {
        Self {
            label,
            value,
            tone: Some(tone),
        }
    }
}

/// Keep a facts table: labels padded to one column, values wrapped beside
/// them, every value cleaned of control characters.
pub(crate) fn rows(body: &mut Body, table: &[Row]) {
    let canvas = body.canvas();
    let pad = table.iter().map(|r| r.label.len()).max().unwrap_or(0) + 2;
    let hang = [plain(" ".repeat(pad))];
    for row in table {
        let value = dots(&clean(&row.value, canvas.ascii).0, canvas.ascii);
        let value = match row.tone {
            Some(tone) => paint(value, tone, canvas.color),
            None => plain(value),
        };
        let label = [paint(
            format!("{:<pad$}", row.label),
            Role::Dim,
            canvas.color,
        )];
        if !body.wrap(&[value], &label, &hang, false) {
            return;
        }
    }
}

/// The rows under a title: the facts wrapped to the width (dim), then one
/// row per note, a disagreement marked `!` (amber) and a bound kept `~`.
/// Every fact and note is cleaned first: a control character or a
/// bidirectional mark that reached one shows as a visible mark.
pub(crate) fn head(facts: &[String], notes: &[Note], canvas: Canvas) -> Vec<Line<'static>> {
    let width = usize::from(canvas.width);
    let words = |text: &str| dots(&clean(text, canvas.ascii).0, canvas.ascii);
    let mut out = Vec::new();
    if !facts.is_empty() {
        let joined = words(&facts.join(sep(canvas.ascii)));
        let (rows, _) = wrap(&[paint(joined, Role::Dim, canvas.color)], width, 4);
        out.extend(rows.into_iter().map(|row| fit(row, width, false, canvas).0));
    }
    for note in notes {
        let (mark, tone) = if note.is_warning() {
            ("! ", Role::Warn)
        } else {
            ("~ ", Role::Dim)
        };
        let text = paint(words(&note.text(canvas.ascii)), tone, canvas.color);
        let (rows, _) = wrap(&[text], width.saturating_sub(2), 3);
        for (index, row) in rows.into_iter().enumerate() {
            let lead = if index == 0 {
                paint(mark, tone, canvas.color)
            } else {
                plain("  ")
            };
            let mut spans = vec![lead];
            spans.extend(row);
            out.push(fit(spans, width, false, canvas).0);
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn canvas(width: u16, ascii: bool) -> Canvas {
        Canvas::new(width, ascii, false)
    }

    fn text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn widths_count_graphemes_as_the_buffer_draws_them() {
        assert_eq!(width("abc"), 3);
        assert_eq!(width("日本"), 4);
        assert_eq!(width("e\u{301}"), 1, "a combining acute rides its letter");
        assert_eq!(width("👩‍👩‍👧"), 2, "one family emoji, one cluster");
    }

    #[test]
    fn a_control_character_never_reaches_a_cell() {
        let (line, marked) = clean("a\x1b[31mb\x07\u{202E}c\x7f", false);
        assert_eq!(line, "a\u{241B}[31mb\u{2407}<U+202E>c\u{2421}");
        assert_eq!(marked, 4);
        let (ascii, _) = clean("a\x1bb\x7f", true);
        assert_eq!(ascii, "a^[b^?");
        let (tabbed, marked) = clean("a\tbc\td", true);
        assert_eq!((tabbed.as_str(), marked), ("a   bc  d", 0));
        assert!(!clean("\u{9b}x", false).0.contains('\u{9b}'));
    }

    #[test]
    fn decoding_backs_off_to_a_boundary_and_counts_invalid_sequences() {
        let bytes = "aé".as_bytes();
        let cut = decode(bytes, 2, false);
        assert_eq!((cut.text.as_str(), cut.read, cut.invalid), ("a", 1, 0));
        let junk = decode(b"\xEF\xBB\xBFok\xff\xfe!", 64, true);
        assert_eq!((junk.text.as_str(), junk.invalid), ("ok??!", 2));
        assert_eq!(junk.read, 8);
    }

    #[test]
    fn lines_are_cut_on_a_boundary_and_flagged() {
        let got: Vec<(&str, bool)> = lines("ab\r\nécole\n", 3).collect();
        assert_eq!(got, [("ab", false), ("éc", true)]);
    }

    #[test]
    fn fitting_never_passes_the_width_and_marks_the_cut() {
        for width_cells in 0..14_u16 {
            let canvas = canvas(width_cells, false);
            for sample in ["plain words that go on", "日本語のテキストです", "👩‍👩‍👧👩‍👩‍👧👩‍👩‍👧 x"]
            {
                let (line, _) = fit(vec![plain(sample)], usize::from(width_cells), false, canvas);
                assert!(
                    width(&text(&line)) <= usize::from(width_cells),
                    "{sample} @ {width_cells}"
                );
            }
        }
        let (line, cut) = fit(vec![plain("abcdef")], 4, false, canvas(4, true));
        assert_eq!((text(&line).as_str(), cut), ("a...", true));
        let (line, cut) = fit(vec![plain("ab")], 4, true, canvas(4, false));
        assert_eq!((text(&line).as_str(), cut), ("ab…", true));
    }

    #[test]
    fn wrapping_breaks_at_spaces_and_bounds_the_rows() {
        let (rows, left) = wrap(&[plain("one two three four")], 8, 9);
        let rows: Vec<String> = rows
            .iter()
            .map(|r| r.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert_eq!(rows, ["one two", "three", "four"]);
        assert!(!left);
        let (rows, left) = wrap(&[plain("abcdefghij")], 4, 2);
        assert_eq!(rows.len(), 2);
        assert!(left);
    }

    #[test]
    fn a_body_counts_its_cuts_into_notes() {
        let limits = super::super::Limits::new(64, 2, 64);
        let mut body = Body::new(canvas(5, false).with_limits(limits));
        assert!(body.push(vec![plain("abcdefgh")], false));
        assert!(body.push(vec![plain("ok")], false));
        assert!(!body.push(vec![plain("dropped")], false));
        let mut notes = Vec::new();
        let lines = body.finish(&mut notes);
        assert_eq!(lines.len(), 2);
        assert_eq!(
            notes,
            [
                Note::WidthCut { lines: 1, width: 5 },
                Note::LinesCut { shown: 2 }
            ]
        );
    }

    #[test]
    fn counts_and_notes_agree_in_number() {
        assert_eq!(count(1, "human gate"), "1 human gate");
        assert_eq!(count(0, "task"), "0 tasks");
        assert_eq!(count(3, "code block"), "3 code blocks");
        let one = [
            Note::LinesCut { shown: 1 },
            Note::WidthCut {
                lines: 1,
                width: 80,
            },
            Note::InvalidUtf8 { sequences: 1 },
            Note::Controls { count: 1 },
            Note::Masked { count: 1 },
            Note::DepthCapped { depth: 1 },
        ];
        let words: Vec<String> = one.iter().map(|n| n.text(true)).collect();
        assert_eq!(
            words,
            [
                "the first 1 line shown; more follow",
                "1 line wider than 80 columns, cut at the edge",
                "1 byte sequence not UTF-8, one ? per sequence",
                "1 control character made visible",
                "1 value masked: secret-looking in a protected object",
                "nesting deeper than 1 level keeps the last indent",
            ]
        );
        assert_eq!(
            Note::Masked { count: 4 }.text(false),
            "4 values masked: secret-looking in a protected object"
        );
    }

    #[test]
    fn the_ascii_column_transliterates_the_engine_punctuation() {
        let engine = "a · b … c → d × e — f – g";
        assert_eq!(dots(engine, true), "a - b ... c -> d x e - f - g");
        assert_eq!(dots(engine, false), engine, "the Unicode column keeps it");
    }

    #[test]
    fn sizes_read_as_words() {
        assert_eq!(size_words(1), "1 byte");
        assert_eq!(size_words(999), "999 bytes");
        assert_eq!(size_words(1_500), "1.5 KB");
        assert_eq!(size_words(1_048_576), "1.0 MB");
    }
}
