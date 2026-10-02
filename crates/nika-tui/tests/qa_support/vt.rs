// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A small VT screen: what a terminal shows after the bytes a process wrote.
//!
//! The QA proofs read the screen the human reads, not escape fragments: a
//! word the diff renderer splits over cursor moves is still one word here,
//! and a line that was painted and later erased is gone. The screen answers
//! the two queries the renderer asks at entry and after every resize (device
//! attributes, cursor position) from its own state, the way a terminal does,
//! so an inline viewport lands where it would land in xterm.
//!
//! Scope: what crossterm and ratatui emit (cursor addressing, erase, scroll
//! regions with `SU`/`SD`, `SGR`, DEC private modes with the alternate
//! screen, OSC titles) and the C0 controls a shell prints. A wide glyph takes
//! two cells; a combining mark joins the cell before it. A resize keeps the
//! cursor's line on screen (the lines above it go to the history), cuts or
//! pads the columns and never reflows, like xterm. Lines leave the main screen
//! for the history when a scroll region that starts at the top row scrolls up
//! (xterm and tmux keep them the same way).

use std::collections::{BTreeMap, BTreeSet};

use unicode_width::{UnicodeWidthChar as _, UnicodeWidthStr as _};

/// A blank cell. The tail of a wide glyph is the empty string.
const BLANK: &str = " ";
/// The primary device attributes this terminal reports (a VT220 with ANSI
/// colour, no keyboard protocol): the answer crossterm's probe waits for.
const ATTRIBUTES: &[u8] = b"\x1b[?62;22c";

#[derive(Debug, Default)]
struct Sequence {
    params: String,
    inter: String,
}

#[derive(Debug)]
enum State {
    Ground,
    Escape,
    Charset,
    Csi(Sequence),
    Osc(Vec<u8>),
    OscEscape(Vec<u8>),
}

type Grid = Vec<Vec<String>>;

/// The screen, its history and what the process asked of it.
#[derive(Debug)]
pub(crate) struct Screen {
    cols: usize,
    rows: usize,
    main: Grid,
    alt: Grid,
    on_alt: bool,
    row: usize,
    col: usize,
    wrap_pending: bool,
    saved: (usize, usize),
    top: usize,
    bottom: usize,
    history: Vec<String>,
    replies: Vec<Vec<u8>>,
    modes: BTreeMap<u16, bool>,
    hues: BTreeSet<String>,
    weights: usize,
    bells: usize,
    cursor_reports: usize,
    beyond: usize,
    title: Option<String>,
    state: State,
    utf8: Vec<u8>,
}

fn blank_row(cols: usize) -> Vec<String> {
    vec![BLANK.to_owned(); cols]
}

fn joined(line: &[String]) -> String {
    line.concat().trim_end().to_owned()
}

impl Screen {
    /// An empty screen of `cols` × `rows`, the cursor at the top left.
    pub(crate) fn new(cols: u16, rows: u16) -> Self {
        let (cols, rows) = (usize::from(cols.max(1)), usize::from(rows.max(1)));
        Self {
            cols,
            rows,
            main: vec![blank_row(cols); rows],
            alt: vec![blank_row(cols); rows],
            on_alt: false,
            row: 0,
            col: 0,
            wrap_pending: false,
            saved: (0, 0),
            top: 0,
            bottom: rows - 1,
            history: Vec::new(),
            replies: Vec::new(),
            modes: BTreeMap::new(),
            hues: BTreeSet::new(),
            weights: 0,
            bells: 0,
            cursor_reports: 0,
            beyond: 0,
            title: None,
            state: State::Ground,
            utf8: Vec::new(),
        }
    }

    /// Put the cursor on the last row, where a shell prompt leaves it after
    /// some history: an inline viewport then opens at the bottom.
    pub(crate) fn park_at_bottom(&mut self) {
        self.row = self.rows - 1;
        self.col = 0;
    }

    /// Read what the process wrote.
    pub(crate) fn feed(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.byte(byte);
        }
    }

    /// The answers owed to the process (device attributes, cursor
    /// reports), in the order it asked; the caller writes them back.
    pub(crate) fn take_replies(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.replies)
    }

    /// The terminal was resized: keep the cursor's line on screen, send the
    /// rows above it to the history, cut or pad the columns.
    pub(crate) fn resize(&mut self, cols: u16, rows: u16) {
        let (cols, rows) = (usize::from(cols.max(1)), usize::from(rows.max(1)));
        let lost = (self.row + 1).saturating_sub(rows);
        for _ in 0..lost {
            let line = self.grid_mut().remove(0);
            if !self.on_alt {
                self.history.push(joined(&line));
            }
        }
        self.row -= lost;
        for grid in [&mut self.main, &mut self.alt] {
            grid.truncate(rows);
            grid.resize(rows, blank_row(cols));
            for line in &mut *grid {
                line.truncate(cols);
                if line.last().is_some_and(|cell| cell.width() > 1) {
                    // A wide glyph cut in half by the new edge is gone.
                    if let Some(cell) = line.last_mut() {
                        BLANK.clone_into(cell);
                    }
                }
                line.resize(cols, BLANK.to_owned());
            }
        }
        self.cols = cols;
        self.rows = rows;
        self.top = 0;
        self.bottom = rows - 1;
        self.row = self.row.min(rows - 1);
        self.col = self.col.min(cols - 1);
        self.saved = (self.saved.0.min(rows - 1), self.saved.1.min(cols - 1));
        self.wrap_pending = false;
    }

    /// The visible rows, trailing blanks trimmed.
    pub(crate) fn lines(&self) -> Vec<String> {
        self.grid().iter().map(|line| joined(line)).collect()
    }

    /// The visible rows, one per line.
    pub(crate) fn text(&self) -> String {
        self.lines().join("\n")
    }

    /// A visible row contains `needle`.
    pub(crate) fn contains(&self, needle: &str) -> bool {
        self.lines().iter().any(|line| line.contains(needle))
    }

    /// The first visible row containing `needle`.
    pub(crate) fn row_of(&self, needle: &str) -> Option<usize> {
        self.lines().iter().position(|line| line.contains(needle))
    }

    /// The first visible row that starts with `head` (a prompt, a rule).
    pub(crate) fn row_starting(&self, head: &str) -> Option<usize> {
        self.lines().iter().position(|line| line.starts_with(head))
    }

    /// The lines that left the main screen upwards, oldest first.
    pub(crate) fn history(&self) -> &[String] {
        &self.history
    }

    /// The main screen's whole story: its history, then its rows (the
    /// scrollback a human scrolls through after an inline session).
    pub(crate) fn transcript(&self) -> Vec<String> {
        let mut lines = self.history.clone();
        lines.extend(self.main.iter().map(|line| joined(line)));
        lines
    }

    /// `needle` is somewhere the human can find it: the visible rows or the
    /// main screen's history.
    pub(crate) fn seen(&self, needle: &str) -> bool {
        self.contains(needle) || self.transcript().iter().any(|line| line.contains(needle))
    }

    /// Where the cursor stands (row, column), from zero.
    pub(crate) fn cursor(&self) -> (usize, usize) {
        (self.row, self.col)
    }

    /// The size (columns, rows).
    pub(crate) fn size(&self) -> (usize, usize) {
        (self.cols, self.rows)
    }

    /// The alternate screen is shown.
    pub(crate) fn on_alt(&self) -> bool {
        self.on_alt
    }

    /// The last state a DEC private mode was set to (`None`: never touched).
    pub(crate) fn mode(&self, mode: u16) -> Option<bool> {
        self.modes.get(&mode).copied()
    }

    /// Every colour an `SGR` set (`38;5;3`, `31`, …): empty means no hue.
    pub(crate) fn hues(&self) -> &BTreeSet<String> {
        &self.hues
    }

    /// How many times a weight (bold, dim) was set.
    pub(crate) fn weights(&self) -> usize {
        self.weights
    }

    /// How many bells rang.
    pub(crate) fn bells(&self) -> usize {
        self.bells
    }

    /// How many cursor-position reports the process asked for.
    pub(crate) fn cursor_reports(&self) -> usize {
        self.cursor_reports
    }

    /// How many absolute moves aimed past the screen (a layout that did not
    /// follow the size).
    pub(crate) fn beyond(&self) -> usize {
        self.beyond
    }

    /// Forget the moves past the screen counted so far: a frame drawn for the
    /// old size can still be in flight when a resize lands (a real terminal
    /// clamps it the same way), so a resize proof judges addressing only
    /// after the process has settled at the final size.
    pub(crate) fn clear_beyond(&mut self) {
        self.beyond = 0;
    }

    /// The last title the process set.
    pub(crate) fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    fn grid(&self) -> &Grid {
        if self.on_alt { &self.alt } else { &self.main }
    }

    fn grid_mut(&mut self) -> &mut Grid {
        if self.on_alt {
            &mut self.alt
        } else {
            &mut self.main
        }
    }

    fn byte(&mut self, byte: u8) {
        match std::mem::replace(&mut self.state, State::Ground) {
            State::Ground => self.ground(byte),
            State::Escape => self.escape(byte),
            State::Charset => {}
            State::Csi(mut seq) => match byte {
                0x30..=0x3f => {
                    seq.params.push(char::from(byte));
                    self.state = State::Csi(seq);
                }
                0x20..=0x2f => {
                    seq.inter.push(char::from(byte));
                    self.state = State::Csi(seq);
                }
                0x40..=0x7e => self.csi(&seq, char::from(byte)),
                0x1b => self.state = State::Escape,
                _ => self.state = State::Csi(seq),
            },
            State::Osc(mut body) => match byte {
                0x07 => self.osc(&body),
                0x1b => self.state = State::OscEscape(body),
                _ => {
                    if body.len() < 4096 {
                        body.push(byte);
                    }
                    self.state = State::Osc(body);
                }
            },
            State::OscEscape(body) => {
                self.osc(&body);
                if byte != b'\\' {
                    self.escape(byte);
                }
            }
        }
    }

    fn ground(&mut self, byte: u8) {
        if byte >= 0x80 {
            self.utf8_byte(byte);
            return;
        }
        self.flush_utf8();
        match byte {
            0x1b => self.state = State::Escape,
            0x07 => self.bells += 1,
            0x08 => {
                self.wrap_pending = false;
                self.col = self.col.saturating_sub(1);
            }
            0x09 => {
                self.wrap_pending = false;
                self.col = ((self.col / 8 + 1) * 8).min(self.cols - 1);
            }
            0x0a..=0x0c => self.linefeed(),
            0x0d => {
                self.wrap_pending = false;
                self.col = 0;
            }
            0x20..=0x7e => self.print(char::from(byte)),
            _ => {}
        }
    }

    fn utf8_byte(&mut self, byte: u8) {
        self.utf8.push(byte);
        match std::str::from_utf8(&self.utf8) {
            Ok(text) => {
                let chars: Vec<char> = text.chars().collect();
                self.utf8.clear();
                for ch in chars {
                    self.print(ch);
                }
            }
            Err(error) if error.error_len().is_some() || self.utf8.len() >= 4 => {
                self.utf8.clear();
                self.print('\u{fffd}');
            }
            Err(_) => {}
        }
    }

    fn flush_utf8(&mut self) {
        if !self.utf8.is_empty() {
            self.utf8.clear();
            self.print('\u{fffd}');
        }
    }

    fn escape(&mut self, byte: u8) {
        match byte {
            b'[' => self.state = State::Csi(Sequence::default()),
            b']' => self.state = State::Osc(Vec::new()),
            b'(' | b')' | b'*' | b'+' => self.state = State::Charset,
            b'7' => self.saved = (self.row, self.col),
            b'8' => self.restore_cursor(),
            b'D' => self.linefeed(),
            b'E' => {
                self.col = 0;
                self.linefeed();
            }
            b'M' => self.reverse_index(),
            b'c' => self.reset(),
            _ => {}
        }
    }

    fn osc(&mut self, body: &[u8]) {
        let text = String::from_utf8_lossy(body);
        if let Some(title) = text.strip_prefix("0;").or_else(|| text.strip_prefix("2;")) {
            self.title = Some(title.to_owned());
        }
    }

    fn csi(&mut self, seq: &Sequence, fin: char) {
        let (private, body) = match seq.params.chars().next() {
            Some(mark @ ('?' | '>' | '<' | '=')) => (Some(mark), &seq.params[1..]),
            _ => (None, seq.params.as_str()),
        };
        if private.is_none() && seq.inter.is_empty() && fin == 'm' {
            self.sgr(body);
            return;
        }
        let args: Vec<usize> = body
            .split(';')
            .map(|arg| {
                arg.split(':')
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0)
            })
            .collect();
        match (private, seq.inter.is_empty()) {
            (None, true) => self.csi_plain(&args, fin),
            (Some('?'), true) => self.csi_private(&args, fin),
            // Keyboard-protocol push/pop, secondary attributes, the cursor
            // shape (`CSI Ps SP q`): nothing a screen shows.
            _ => {}
        }
    }

    fn arg(args: &[usize], index: usize, default: usize) -> usize {
        args.get(index)
            .copied()
            .filter(|value| *value != 0)
            .unwrap_or(default)
    }

    fn csi_plain(&mut self, args: &[usize], fin: char) {
        let n = Self::arg(args, 0, 1);
        let first = args.first().copied().unwrap_or(0);
        match fin {
            'A' => self.move_to(self.row.saturating_sub(n), self.col),
            'B' | 'e' => self.move_to(self.row + n, self.col),
            'C' | 'a' => self.move_to(self.row, self.col + n),
            'D' => self.move_to(self.row, self.col.saturating_sub(n)),
            'E' => self.move_to(self.row + n, 0),
            'F' => self.move_to(self.row.saturating_sub(n), 0),
            'G' | '`' => self.address(self.row, n - 1),
            'd' => self.address(n - 1, self.col),
            'H' | 'f' => self.address(n - 1, Self::arg(args, 1, 1) - 1),
            'J' => self.erase_display(first),
            'K' => self.erase_line(first),
            'L' => self.insert_lines(n),
            'M' => self.delete_lines(n),
            '@' => self.insert_chars(n),
            'P' => self.delete_chars(n),
            'X' => self.clear_cells(self.row, self.col, self.col + n),
            'S' => self.scroll_up(n),
            'T' => self.scroll_down(n),
            'r' => self.set_region(Self::arg(args, 0, 1), Self::arg(args, 1, self.rows)),
            'n' => self.status_report(first),
            'c' if first == 0 => self.replies.push(ATTRIBUTES.to_vec()),
            's' => self.saved = (self.row, self.col),
            'u' => self.restore_cursor(),
            _ => {}
        }
    }

    fn csi_private(&mut self, args: &[usize], fin: char) {
        // `CSI ? u` asks for the keyboard protocol: this terminal has none
        // and stays silent, as a terminal without it does.
        let on = match fin {
            'h' => true,
            'l' => false,
            _ => return,
        };
        for &mode in args {
            let Ok(mode) = u16::try_from(mode) else {
                continue;
            };
            match mode {
                1049 => self.alternate(on, true),
                47 | 1047 => self.alternate(on, false),
                _ => {}
            }
            self.modes.insert(mode, on);
        }
    }

    fn sgr(&mut self, body: &str) {
        let tokens: Vec<&str> = body.split(';').collect();
        let mut index = 0;
        while let Some(token) = tokens.get(index) {
            let code: usize = token
                .split(':')
                .next()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            match code {
                1 | 2 => self.weights += 1,
                // An extended colour spreads over the next arguments
                // (`38;5;n`, `38;2;r;g;b`) unless it uses colons.
                38 | 48 | 58 if !token.contains(':') => {
                    let take = match tokens.get(index + 1) {
                        Some(&"5") => 2,
                        Some(&"2") => 4,
                        _ => 0,
                    };
                    let end = (index + 1 + take).min(tokens.len());
                    self.hues.insert(tokens[index..end].join(";"));
                    index = end - 1;
                }
                30..=38 | 40..=48 | 58 | 90..=97 | 100..=107 => {
                    self.hues.insert((*token).to_owned());
                }
                _ => {}
            }
            index += 1;
        }
    }

    fn status_report(&mut self, kind: usize) {
        match kind {
            6 => {
                self.cursor_reports += 1;
                let reply = format!("\x1b[{};{}R", self.row + 1, self.col + 1);
                self.replies.push(reply.into_bytes());
            }
            5 => self.replies.push(b"\x1b[0n".to_vec()),
            _ => {}
        }
    }

    /// An absolute move: counted when it aims past the screen, then clamped.
    fn address(&mut self, row: usize, col: usize) {
        if row >= self.rows || col >= self.cols {
            self.beyond += 1;
        }
        self.move_to(row, col);
    }

    fn move_to(&mut self, row: usize, col: usize) {
        self.row = row.min(self.rows - 1);
        self.col = col.min(self.cols - 1);
        self.wrap_pending = false;
    }

    fn restore_cursor(&mut self) {
        let (row, col) = self.saved;
        self.move_to(row, col);
    }

    fn alternate(&mut self, on: bool, save: bool) {
        if on == self.on_alt {
            return;
        }
        if on {
            if save {
                self.saved = (self.row, self.col);
            }
            self.alt = vec![blank_row(self.cols); self.rows];
            self.on_alt = true;
        } else {
            self.on_alt = false;
            if save {
                self.restore_cursor();
            }
        }
        self.top = 0;
        self.bottom = self.rows - 1;
        self.wrap_pending = false;
    }

    fn reset(&mut self) {
        self.main = vec![blank_row(self.cols); self.rows];
        self.alt = vec![blank_row(self.cols); self.rows];
        self.on_alt = false;
        self.modes.clear();
        self.top = 0;
        self.bottom = self.rows - 1;
        self.move_to(0, 0);
    }

    fn set_region(&mut self, top: usize, bottom: usize) {
        let (top, bottom) = (top - 1, bottom.min(self.rows) - 1);
        if top < bottom {
            self.top = top;
            self.bottom = bottom;
        } else {
            self.top = 0;
            self.bottom = self.rows - 1;
        }
        self.move_to(0, 0);
    }

    fn linefeed(&mut self) {
        self.wrap_pending = false;
        if self.row == self.bottom {
            self.scroll_up(1);
        } else if self.row + 1 < self.rows {
            self.row += 1;
        }
    }

    fn reverse_index(&mut self) {
        self.wrap_pending = false;
        if self.row == self.top {
            self.scroll_down(1);
        } else {
            self.row = self.row.saturating_sub(1);
        }
    }

    fn scroll_up(&mut self, n: usize) {
        let (top, bottom, cols) = (self.top, self.bottom, self.cols);
        for _ in 0..n.min(bottom + 1 - top) {
            let line = self.grid_mut().remove(top);
            self.grid_mut().insert(bottom, blank_row(cols));
            if top == 0 && !self.on_alt {
                self.history.push(joined(&line));
            }
        }
    }

    fn scroll_down(&mut self, n: usize) {
        let (top, bottom, cols) = (self.top, self.bottom, self.cols);
        for _ in 0..n.min(bottom + 1 - top) {
            self.grid_mut().remove(bottom);
            self.grid_mut().insert(top, blank_row(cols));
        }
    }

    fn insert_lines(&mut self, n: usize) {
        let (row, bottom, cols) = (self.row, self.bottom, self.cols);
        if row < self.top || row > bottom {
            return;
        }
        for _ in 0..n.min(bottom + 1 - row) {
            self.grid_mut().remove(bottom);
            self.grid_mut().insert(row, blank_row(cols));
        }
        self.col = 0;
    }

    fn delete_lines(&mut self, n: usize) {
        let (row, bottom, cols) = (self.row, self.bottom, self.cols);
        if row < self.top || row > bottom {
            return;
        }
        for _ in 0..n.min(bottom + 1 - row) {
            self.grid_mut().remove(row);
            self.grid_mut().insert(bottom, blank_row(cols));
        }
        self.col = 0;
    }

    fn insert_chars(&mut self, n: usize) {
        let (row, col, width) = (self.row, self.col, self.cols);
        let line = &mut self.grid_mut()[row];
        for _ in 0..n.min(width - col) {
            line.insert(col, BLANK.to_owned());
        }
        line.truncate(width);
    }

    fn delete_chars(&mut self, n: usize) {
        let (row, col, width) = (self.row, self.col, self.cols);
        let line = &mut self.grid_mut()[row];
        for _ in 0..n.min(width - col) {
            line.remove(col);
        }
        line.resize(width, BLANK.to_owned());
    }

    fn erase_display(&mut self, kind: usize) {
        let (row, col, height, width) = (self.row, self.col, self.rows, self.cols);
        match kind {
            0 => {
                self.clear_cells(row, col, width);
                for below in row + 1..height {
                    self.clear_cells(below, 0, width);
                }
            }
            1 => {
                for above in 0..row {
                    self.clear_cells(above, 0, width);
                }
                self.clear_cells(row, 0, col + 1);
            }
            2 => {
                for any in 0..height {
                    self.clear_cells(any, 0, width);
                }
            }
            3 => self.history.clear(),
            _ => {}
        }
    }

    fn erase_line(&mut self, kind: usize) {
        let (row, col, width) = (self.row, self.col, self.cols);
        match kind {
            0 => self.clear_cells(row, col, width),
            1 => self.clear_cells(row, 0, col + 1),
            2 => self.clear_cells(row, 0, width),
            _ => {}
        }
    }

    fn clear_cells(&mut self, row: usize, from: usize, to: usize) {
        for col in from..to.min(self.cols) {
            self.put(row, col, BLANK.to_owned());
        }
    }

    fn print(&mut self, ch: char) {
        let width = ch.width().unwrap_or(0);
        if width == 0 {
            let (row, col) = (
                self.row,
                if self.wrap_pending {
                    self.col
                } else {
                    self.col.saturating_sub(1)
                },
            );
            if let Some(cell) = self.grid_mut().get_mut(row).and_then(|l| l.get_mut(col)) {
                cell.push(ch);
            }
            return;
        }
        if self.wrap_pending || self.col + width > self.cols {
            self.col = 0;
            self.linefeed();
        }
        let (row, col) = (self.row, self.col);
        self.put(row, col, ch.to_string());
        if width == 2 {
            self.put(row, col + 1, String::new());
        }
        if col + width >= self.cols {
            self.col = self.cols - 1;
            self.wrap_pending = true;
        } else {
            self.col = col + width;
        }
    }

    /// Write one cell, blanking the other half of a wide glyph it breaks.
    fn put(&mut self, row: usize, col: usize, text: String) {
        let width = self.cols;
        let Some(line) = self.grid_mut().get_mut(row) else {
            return;
        };
        if col >= width {
            return;
        }
        if col > 0 && line[col].is_empty() {
            BLANK.clone_into(&mut line[col - 1]);
        }
        if col + 1 < width && line[col + 1].is_empty() {
            BLANK.clone_into(&mut line[col + 1]);
        }
        line[col] = text;
    }
}
