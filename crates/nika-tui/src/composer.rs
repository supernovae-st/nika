// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The composer: where the human types (ADR-139 · law 5).
//!
//! `ratatui-textarea` sits behind this wrapper and owns nothing but the
//! text: the wrapper decides what `Enter` means (send), what `Alt+Enter`
//! means (a new line), that a paste is data (inserted verbatim, never
//! interpreted as keys, so a pasted `yes` or `/quit` acts on nothing until
//! the human presses `Enter`), and that `Up`/`Down` recall history only when
//! the cursor stands at the buffer's first or last line.
//!
//! Exit criterion (written down, ADR-139 §5): the day this wrapper needs to
//! re-implement cursor movement or wrapping, the crate is replaced by an
//! owned editor.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Widget;
use ratatui_textarea::{CursorMove, Input, Key, TextArea, WrapMode};

/// What a key did to the composer.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ComposerAction {
    /// Nothing the loop needs to know.
    Edited,
    /// The human sent the buffer; it is cleared and kept in history.
    Submit(String),
    /// `Tab`: the human asks the loop to complete the buffer.
    Complete,
    /// The key was not the composer's (the loop decides).
    Ignored,
}

/// What a completion came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Completion {
    /// The buffer became this one candidate.
    Done(String),
    /// Several candidates share the prefix (the buffer took the common
    /// part); the loop shows them.
    Several(Vec<String>),
    /// Nothing to complete here.
    None,
}

/// The wrapper.
#[derive(Debug)]
pub struct Composer {
    area: TextArea<'static>,
    history: Vec<String>,
    recall: Option<usize>,
    draft: Option<Vec<String>>,
}

impl Default for Composer {
    fn default() -> Self {
        Self::new()
    }
}

impl Composer {
    /// An empty composer with word wrap and no decoration.
    #[must_use]
    pub fn new() -> Self {
        let mut area = TextArea::default();
        area.set_wrap_mode(WrapMode::Word);
        area.set_cursor_line_style(Style::default());
        area.set_cursor_style(Style::default().add_modifier(Modifier::REVERSED));
        Self {
            area,
            history: Vec::new(),
            recall: None,
            draft: None,
        }
    }

    /// The buffer's lines.
    #[must_use]
    pub fn lines(&self) -> &[String] {
        self.area.lines()
    }

    /// The buffer as one text.
    #[must_use]
    pub fn text(&self) -> String {
        self.area.lines().join("\n")
    }

    /// Whether the buffer holds only whitespace.
    #[must_use]
    pub fn is_blank(&self) -> bool {
        self.area.lines().iter().all(|l| l.trim().is_empty())
    }

    /// The rows the buffer needs at `width` (wrapped), at least one.
    #[must_use]
    pub fn rows(&self, width: u16) -> u16 {
        let width = usize::from(width.max(1));
        let rows: usize = self
            .area
            .lines()
            .iter()
            .map(|line| {
                let cells = unicode_width::UnicodeWidthStr::width(line.as_str());
                cells.div_ceil(width).max(1)
            })
            .sum();
        u16::try_from(rows.max(1)).unwrap_or(u16::MAX)
    }

    /// Insert pasted text as data.
    pub fn paste(&mut self, text: &str) {
        self.recall = None;
        self.area
            .insert_str(text.replace("\r\n", "\n").replace('\r', "\n"));
    }

    /// The composer's own placeholder line, shown when empty.
    pub fn set_placeholder(&mut self, text: &str) {
        self.area.set_placeholder_text(text.to_owned());
    }

    /// Handle one key press.
    pub fn handle(&mut self, key: KeyEvent) -> ComposerAction {
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        match key.code {
            KeyCode::Enter if alt || shift || ctrl => {
                self.area.insert_newline();
                ComposerAction::Edited
            }
            KeyCode::Char('j') if ctrl => {
                self.area.insert_newline();
                ComposerAction::Edited
            }
            KeyCode::Enter => self.submit(),
            KeyCode::Up if self.at_first_line() => self.recall_older(),
            KeyCode::Down if self.at_last_line() => self.recall_newer(),
            KeyCode::Char(c) if ctrl && matches!(c, 'c' | 't' | 'o' | 'l' | 'd' | 'z') => {
                ComposerAction::Ignored
            }
            KeyCode::Tab => ComposerAction::Complete,
            KeyCode::Esc | KeyCode::PageUp | KeyCode::PageDown | KeyCode::BackTab => {
                ComposerAction::Ignored
            }
            KeyCode::Char(c) => {
                self.recall = None;
                self.area.input(Input {
                    key: Key::Char(c),
                    ctrl,
                    alt,
                    shift,
                });
                ComposerAction::Edited
            }
            code => {
                let key = match code {
                    KeyCode::Backspace => Key::Backspace,
                    KeyCode::Delete => Key::Delete,
                    KeyCode::Left => Key::Left,
                    KeyCode::Right => Key::Right,
                    KeyCode::Up => Key::Up,
                    KeyCode::Down => Key::Down,
                    KeyCode::Home => Key::Home,
                    KeyCode::End => Key::End,
                    _ => return ComposerAction::Ignored,
                };
                self.area.input(Input {
                    key,
                    ctrl,
                    alt,
                    shift,
                });
                ComposerAction::Edited
            }
        }
    }

    fn submit(&mut self) -> ComposerAction {
        let text = self.text();
        if !text.trim().is_empty() {
            self.history.push(text.clone());
        }
        self.clear();
        ComposerAction::Submit(text)
    }

    /// Empty the buffer and forget the recall position.
    pub fn clear(&mut self) {
        self.area = fresh_like(&self.area);
        self.recall = None;
        self.draft = None;
    }

    fn at_first_line(&self) -> bool {
        self.area.cursor().0 == 0
    }

    fn at_last_line(&self) -> bool {
        self.area.cursor().0 + 1 >= self.area.lines().len()
    }

    fn recall_older(&mut self) -> ComposerAction {
        if self.history.is_empty() {
            return ComposerAction::Ignored;
        }
        let index = match self.recall {
            None => {
                self.draft = Some(self.area.lines().to_vec());
                self.history.len() - 1
            }
            Some(0) => return ComposerAction::Edited,
            Some(i) => i - 1,
        };
        self.recall = Some(index);
        self.replace_with(self.history[index].clone());
        ComposerAction::Edited
    }

    fn recall_newer(&mut self) -> ComposerAction {
        let Some(index) = self.recall else {
            return ComposerAction::Ignored;
        };
        if index + 1 < self.history.len() {
            self.recall = Some(index + 1);
            self.replace_with(self.history[index + 1].clone());
        } else {
            self.recall = None;
            let draft = self.draft.take().unwrap_or_default().join("\n");
            self.replace_with(draft);
        }
        ComposerAction::Edited
    }

    /// Complete a one-line `/command` buffer from `candidates` (kept in
    /// the caller's order): one match fills the buffer, several take their
    /// common prefix and are returned for the hint row, none changes nothing.
    pub fn complete(&mut self, candidates: &[String]) -> Completion {
        let text = self.text();
        let typed = text.trim_end();
        if !typed.starts_with('/') || typed.contains('\n') || typed.contains(' ') {
            return Completion::None;
        }
        let matches: Vec<&String> = candidates.iter().filter(|c| c.starts_with(typed)).collect();
        match matches.as_slice() {
            [] => Completion::None,
            [one] => {
                self.replace_with((*one).clone());
                Completion::Done((*one).clone())
            }
            several => {
                let prefix = common_prefix(several);
                if prefix.len() > typed.len() {
                    self.replace_with(prefix);
                }
                Completion::Several(several.iter().map(|s| (*s).clone()).collect())
            }
        }
    }

    fn replace_with(&mut self, text: String) {
        let mut area = fresh_like(&self.area);
        area.insert_str(text);
        area.move_cursor(CursorMove::End);
        self.area = area;
    }

    /// Draw the composer into `area`.
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        (&self.area).render(area, buf);
    }
}

/// The longest prefix every candidate shares (on char boundaries).
fn common_prefix(candidates: &[&String]) -> String {
    let Some(first) = candidates.first() else {
        return String::new();
    };
    let mut prefix: String = (*first).clone();
    for candidate in &candidates[1..] {
        while !candidate.starts_with(prefix.as_str()) {
            prefix.pop();
        }
    }
    prefix
}

fn fresh_like(previous: &TextArea<'static>) -> TextArea<'static> {
    let mut area = TextArea::default();
    area.set_wrap_mode(WrapMode::Word);
    area.set_cursor_line_style(Style::default());
    area.set_cursor_style(previous.cursor_style());
    let placeholder = previous.placeholder_text();
    if !placeholder.is_empty() {
        area.set_placeholder_text(placeholder.to_owned());
    }
    area
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    fn type_text(composer: &mut Composer, text: &str) {
        for c in text.chars() {
            composer.handle(key(KeyCode::Char(c), KeyModifiers::NONE));
        }
    }

    #[test]
    fn enter_sends_and_alt_enter_breaks_a_line() {
        let mut composer = Composer::new();
        type_text(&mut composer, "read ./notes");
        assert_eq!(
            composer.handle(key(KeyCode::Enter, KeyModifiers::ALT)),
            ComposerAction::Edited
        );
        type_text(&mut composer, "and digest them");
        assert_eq!(composer.lines().len(), 2);
        assert_eq!(
            composer.handle(key(KeyCode::Enter, KeyModifiers::NONE)),
            ComposerAction::Submit("read ./notes\nand digest them".to_owned())
        );
        assert!(composer.is_blank());
    }

    #[test]
    fn a_pasted_yes_or_quit_is_data_until_enter() {
        let mut composer = Composer::new();
        composer.paste("yes\n/quit\nrun it\n");
        assert_eq!(composer.lines().len(), 4, "{:?}", composer.lines());
        assert_eq!(composer.text(), "yes\n/quit\nrun it\n");
        assert!(!composer.is_blank());
    }

    #[test]
    fn history_recalls_only_at_the_edges_and_keeps_the_draft() {
        let mut composer = Composer::new();
        type_text(&mut composer, "first");
        composer.handle(key(KeyCode::Enter, KeyModifiers::NONE));
        type_text(&mut composer, "second");
        composer.handle(key(KeyCode::Enter, KeyModifiers::NONE));
        type_text(&mut composer, "dra");
        composer.handle(key(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(composer.text(), "second");
        composer.handle(key(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(composer.text(), "first");
        composer.handle(key(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(composer.text(), "first", "the oldest stays");
        composer.handle(key(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(composer.text(), "second");
        composer.handle(key(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(composer.text(), "dra", "the draft comes back");
    }

    #[test]
    fn control_keys_the_loop_owns_are_ignored_here() {
        let mut composer = Composer::new();
        assert_eq!(
            composer.handle(key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            ComposerAction::Ignored
        );
        assert_eq!(
            composer.handle(key(KeyCode::Char('t'), KeyModifiers::CONTROL)),
            ComposerAction::Ignored
        );
        assert_eq!(
            composer.handle(key(KeyCode::Esc, KeyModifiers::NONE)),
            ComposerAction::Ignored
        );
        assert!(composer.is_blank());
    }

    #[test]
    fn rows_follow_the_width() {
        let mut composer = Composer::new();
        assert_eq!(composer.rows(80), 1);
        composer.paste("a".repeat(100).as_str());
        assert_eq!(composer.rows(40), 3);
        assert_eq!(composer.rows(200), 1);
    }
}

#[cfg(test)]
mod completion_tests {
    use super::*;

    fn commands() -> Vec<String> {
        [
            "/help",
            "/status",
            "/why",
            "/meaning",
            "/proof",
            "/show",
            "/intelligence",
            "/quit",
        ]
        .iter()
        .map(|c| (*c).to_owned())
        .collect()
    }

    /// One match fills the buffer; a shared prefix is taken and the
    /// candidates come back in the caller's order; a line that is not a
    /// slash command is left alone.
    #[test]
    fn tab_completes_a_slash_command() {
        let mut c = Composer::new();
        c.paste("/pro");
        assert_eq!(
            c.complete(&commands()),
            Completion::Done("/proof".to_owned())
        );
        assert_eq!(c.text(), "/proof");
        let mut c = Composer::new();
        c.paste("/s");
        assert_eq!(
            c.complete(&commands()),
            Completion::Several(vec!["/status".to_owned(), "/show".to_owned()])
        );
        assert_eq!(c.text(), "/s", "no longer common prefix to take");
        let mut c = Composer::new();
        c.paste("read ./notes");
        assert_eq!(c.complete(&commands()), Completion::None);
        assert_eq!(c.text(), "read ./notes");
        let mut c = Composer::new();
        c.paste("/zzz");
        assert_eq!(c.complete(&commands()), Completion::None);
    }
}
