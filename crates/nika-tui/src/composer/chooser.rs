// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The command chooser the composer holds: the slash list and the palette.
//!
//! Two doors open one list. A one-line draft that starts with `/` lists the
//! Session commands that begin with what is typed; the palette (`Ctrl+O`, or
//! `Tab` in an empty box) lists every command and view key, searched by its
//! own query while the draft waits untouched and out of view. The arrows and
//! `Tab` choose. Choosing a command INSERTS its words into the draft and
//! never sends them: `Enter` stays the human's own act, and on a whole
//! command it sends as it always did. A view key chosen in the palette goes
//! back to the shell, which presses it once the palette has closed. `Esc`
//! closes the innermost thing: the palette (the draft as it was), the slash
//! list (until the draft changes), then a draft the palette set aside.
//!
//! A command takes a whole line, so a palette choice over words already in
//! the box sets those words aside: never lost and never sent, they return to
//! the box once the line that replaced them is taken (sent, or queued as a
//! correction), or at once with `Esc`.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::Composer;

/// How many entries one `PgUp` or `PgDn` moves the selection.
const PAGE: usize = 5;

/// What choosing an entry does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Act {
    /// Put these words in the box (a Session command); the human sends them.
    Insert(String),
    /// Press this view key once the palette has closed.
    Press(KeyEvent),
}

/// One thing the chooser lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    /// What the human reads first: `/status`, `F6`.
    pub(crate) name: String,
    /// What it does, in a few words.
    pub(crate) effect: String,
    /// Where or when it applies.
    pub(crate) scope: String,
    /// One sentence of detail, shown for the selected entry.
    pub(crate) help: String,
    /// More words a palette search finds it by (`model` finds `/intelligence`).
    pub(crate) words: String,
    /// What choosing it does.
    pub(crate) act: Act,
}

impl Entry {
    /// A Session command: choosing it inserts `name` into the draft.
    pub(crate) fn command(name: &str, effect: &str, scope: &str, help: &str, words: &str) -> Self {
        Self {
            name: name.to_owned(),
            effect: effect.to_owned(),
            scope: scope.to_owned(),
            help: help.to_owned(),
            words: words.to_owned(),
            act: Act::Insert(name.to_owned()),
        }
    }

    /// A view key of the shell: choosing it presses `key`.
    pub(crate) fn key(
        key: KeyEvent,
        name: &str,
        effect: &str,
        scope: &str,
        help: &str,
        words: &str,
    ) -> Self {
        Self {
            act: Act::Press(key),
            ..Self::command(name, effect, scope, help, words)
        }
    }

    /// Whether every search term is in the entry's words, case aside.
    fn found_by(&self, terms: &[String]) -> bool {
        let words = format!(
            "{} {} {} {} {}",
            self.name, self.effect, self.scope, self.help, self.words
        )
        .to_lowercase();
        terms.iter().all(|term| words.contains(term.as_str()))
    }
}

/// Which door the list came through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Door {
    /// A one-line draft that starts with `/`: the Session commands it begins.
    Slash,
    /// The palette: every command and view key, searched by its own query.
    Palette,
}

/// What the chooser did with a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Chosen {
    /// Not the chooser's key: it takes its ordinary path (an open palette
    /// closed first, the draft as it was).
    Pass,
    /// The chooser read it: the selection, the query, the list or the
    /// draft's words changed, and nothing was sent.
    Read,
    /// The palette closed on a command it put in the draft: the keys stay on
    /// the composer, so the `Enter` that sends it is the human's own.
    Inserted,
    /// A view key chosen in the palette, which closed: the shell presses it.
    Press(KeyEvent),
}

/// The chooser's state, kept by the composer between frames.
#[derive(Clone, Debug, Default)]
pub(crate) struct Chooser {
    /// Everything that can be chosen now: the conversation's commands, then
    /// the shell's view keys.
    entries: Vec<Entry>,
    /// The selected entry, by name: a narrower list keeps it while listed.
    picked: Option<String>,
    /// The draft the slash list was closed for: it opens again once the
    /// draft changes.
    closed_for: Option<String>,
    /// The palette's query, while the palette is open.
    query: Option<String>,
    /// The composer holds the keys; the slash list shows and reads only then.
    focused: bool,
}

impl Chooser {
    /// The commands whose name begins with `word`, case aside.
    fn beginning(&self, word: &str) -> Vec<&Entry> {
        let word = word.to_lowercase();
        self.entries
            .iter()
            .filter(|entry| matches!(entry.act, Act::Insert(_)))
            .filter(|entry| entry.name.to_lowercase().starts_with(&word))
            .collect()
    }

    /// Every entry the palette's `query` finds, in the offered order.
    fn found(&self, query: &str) -> Vec<&Entry> {
        let terms: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        self.entries
            .iter()
            .filter(|entry| entry.found_by(&terms))
            .collect()
    }

    /// Where the picked entry stands in `listed`; the first one otherwise.
    fn position(&self, listed: &[&Entry]) -> usize {
        self.picked
            .as_deref()
            .and_then(|name| listed.iter().position(|entry| entry.name == name))
            .unwrap_or(0)
    }
}

/// What the live area paints of the chooser.
#[derive(Debug)]
pub(crate) struct Listing<'a> {
    /// The door the list came through.
    pub(crate) door: Door,
    /// The listed entries, in order (the palette's may be none: no match).
    pub(crate) entries: Vec<&'a Entry>,
    /// The selected entry's index in `entries`.
    pub(crate) selected: usize,
    /// The palette's query (`None` through the slash door).
    pub(crate) query: Option<&'a str>,
    /// Through the slash door, the draft already IS the selected command:
    /// `Enter` sends it, as it would without the list.
    pub(crate) whole: bool,
}

impl Listing<'_> {
    /// The selected entry, when one is listed.
    pub(crate) fn current(&self) -> Option<&Entry> {
        self.entries.get(self.selected).copied()
    }
}

/// A draft that names a command being typed: `/`, then a name holding no
/// whitespace and no path separator (`/tmp/a.csv` and `/notes.md` are paths,
/// as the Session reads them).
pub(crate) fn command_word(text: &str) -> bool {
    text.strip_prefix('/').is_some_and(|name| {
        !name.contains(|c: char| c.is_whitespace() || matches!(c, '/' | '.' | '\\'))
    })
}

impl Composer {
    /// Offer `entries` to choose from: the conversation's commands, then the
    /// shell's view keys. The selection stays on its entry while listed.
    pub(crate) fn offer(&mut self, entries: Vec<Entry>) {
        self.chooser.entries = entries;
    }

    /// Whether the composer holds the keys (the workspace may give them to
    /// another region): the slash list shows and reads keys only then.
    pub(crate) fn set_focused(&mut self, focused: bool) {
        self.chooser.focused = focused;
    }

    /// Open the palette over the draft, which waits untouched; open, close it.
    pub(crate) fn toggle_palette(&mut self) {
        if self.chooser.query.is_some() {
            self.close_palette();
        } else {
            self.chooser.query = Some(String::new());
            self.chooser.picked = None;
        }
    }

    /// Close the palette: the draft is exactly as it was before it opened.
    pub(crate) fn close_palette(&mut self) {
        self.chooser.query = None;
        self.chooser.picked = None;
    }

    /// Whether the palette is open.
    #[must_use]
    pub(crate) fn palette_open(&self) -> bool {
        self.chooser.query.is_some()
    }

    /// The words the palette set aside to insert a command, if any.
    #[must_use]
    pub(crate) fn aside(&self) -> Option<&str> {
        self.aside.as_deref()
    }

    /// Pasted text goes into the open palette's search as data, its line
    /// breaks read as spaces: `false`, and nothing done, when it is closed.
    pub(crate) fn paste_query(&mut self, text: &str) -> bool {
        let Some(query) = self.chooser.query.as_mut() else {
            return false;
        };
        query.extend(text.chars().map(|c| if c.is_control() { ' ' } else { c }));
        self.chooser.picked = None;
        true
    }

    /// What the chooser lists now, if it shows: the palette while open, else
    /// the slash list of a command being typed (never of a line recalled
    /// from history, never once `Esc` closed it for this very draft).
    #[must_use]
    pub(crate) fn listing(&self) -> Option<Listing<'_>> {
        let (door, entries) = if let Some(query) = &self.chooser.query {
            (Door::Palette, self.chooser.found(query))
        } else {
            let word = self.typed_command()?;
            (Door::Slash, self.chooser.beginning(&word))
        };
        if door == Door::Slash && entries.is_empty() {
            return None;
        }
        let selected = self.chooser.position(&entries);
        let whole = door == Door::Slash
            && entries
                .get(selected)
                .is_some_and(|entry| entry.name == self.text());
        Some(Listing {
            door,
            entries,
            selected,
            query: self.chooser.query.as_deref(),
            whole,
        })
    }

    /// The draft, when it is a command being typed with the keys here.
    fn typed_command(&self) -> Option<String> {
        let text = self.text();
        let open = self.chooser.focused
            && command_word(&text)
            && !self.recalled(&text)
            && self.chooser.closed_for.as_deref() != Some(text.as_str());
        open.then_some(text)
    }

    /// Whether `text` is the history entry `Up` or `Down` put in the box.
    fn recalled(&self, text: &str) -> bool {
        self.recall
            .and_then(|index| self.history.get(index))
            .is_some_and(|line| line == text)
    }

    /// Read one key ([`Chosen`]). The palette reads its own keys; an open
    /// slash list reads the arrows, `Tab`, `Enter` and `Esc`; `Tab` opens the
    /// slash list of a command being typed again, or the palette in an empty
    /// box; `Esc` brings back a draft the palette set aside. Every other key
    /// is passed on, and none of them ever sends anything.
    pub(crate) fn choose(&mut self, key: KeyEvent) -> Chosen {
        if self.chooser.query.is_some() {
            return self.palette_key(key);
        }
        if self.listing().is_some() {
            return self.slash_key(key);
        }
        let plain = key.modifiers.is_empty();
        match key.code {
            KeyCode::Tab if plain && self.chooser.focused && command_word(&self.text()) => {
                self.chooser.closed_for = None;
                self.recall = None;
                Chosen::Read
            }
            KeyCode::Tab if plain && self.is_blank() => {
                self.toggle_palette();
                Chosen::Read
            }
            KeyCode::Esc if plain && self.aside.is_some() => {
                self.restore_aside();
                Chosen::Read
            }
            _ => Chosen::Pass,
        }
    }

    /// A key while the slash list shows.
    fn slash_key(&mut self, key: KeyEvent) -> Chosen {
        if !key.modifiers.is_empty() && key.code != KeyCode::BackTab {
            return Chosen::Pass;
        }
        match key.code {
            KeyCode::Esc => {
                self.chooser.closed_for = Some(self.text());
                Chosen::Read
            }
            KeyCode::Up | KeyCode::BackTab => self.step(false),
            KeyCode::Down => self.step(true),
            KeyCode::PageUp => self.page(false),
            KeyCode::PageDown => self.page(true),
            // `Enter` on the whole command is the composer's own: it sends.
            KeyCode::Enter if self.listing().is_some_and(|listing| listing.whole) => Chosen::Pass,
            KeyCode::Tab | KeyCode::Enter => self.insert_selected(),
            _ => Chosen::Pass,
        }
    }

    /// A key while the palette is open: its search, its selection, its
    /// choice; any other key closes it and goes on its ordinary way.
    fn palette_key(&mut self, key: KeyEvent) -> Chosen {
        let typed = !key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        match key.code {
            KeyCode::Esc => {
                self.close_palette();
                Chosen::Read
            }
            KeyCode::Up | KeyCode::BackTab => self.step(false),
            KeyCode::Down => self.step(true),
            KeyCode::PageUp => self.page(false),
            KeyCode::PageDown => self.page(true),
            KeyCode::Home => self.step_to(0),
            KeyCode::End => self.step_to(usize::MAX),
            KeyCode::Tab | KeyCode::Enter => self.pick(),
            KeyCode::Backspace => {
                if let Some(query) = self.chooser.query.as_mut() {
                    query.pop();
                }
                self.chooser.picked = None;
                Chosen::Read
            }
            KeyCode::Char(c) if typed => {
                if let Some(query) = self.chooser.query.as_mut() {
                    query.push(c);
                }
                self.chooser.picked = None;
                Chosen::Read
            }
            _ => {
                self.close_palette();
                Chosen::Pass
            }
        }
    }

    /// Move the selection one entry, wrapping around the list.
    fn step(&mut self, down: bool) -> Chosen {
        let Some(listing) = self.listing() else {
            return Chosen::Read;
        };
        let count = listing.entries.len();
        if count == 0 {
            return Chosen::Read;
        }
        let next = if down {
            (listing.selected + 1) % count
        } else {
            (listing.selected + count - 1) % count
        };
        self.step_to(next)
    }

    /// Move the selection a page, stopping at either end.
    fn page(&mut self, down: bool) -> Chosen {
        let Some(at) = self.listing().map(|listing| listing.selected) else {
            return Chosen::Read;
        };
        let next = if down {
            at.saturating_add(PAGE)
        } else {
            at.saturating_sub(PAGE)
        };
        self.step_to(next)
    }

    /// Select the entry at `index` of the list, the last one past its end.
    fn step_to(&mut self, index: usize) -> Chosen {
        let name = self.listing().and_then(|listing| {
            let last = listing.entries.len().checked_sub(1)?;
            listing
                .entries
                .get(index.min(last))
                .map(|entry| entry.name.clone())
        });
        if name.is_some() {
            self.chooser.picked = name;
        }
        Chosen::Read
    }

    /// The slash list's choice: the draft becomes the selected command.
    fn insert_selected(&mut self) -> Chosen {
        let name = self
            .listing()
            .and_then(|listing| listing.current().map(|entry| entry.name.clone()));
        if let Some(name) = name {
            self.replace_with(name.clone());
            self.chooser.picked = Some(name);
            self.chooser.closed_for = None;
        }
        Chosen::Read
    }

    /// The palette's choice: a command goes into the box, a view key back to
    /// the shell. With nothing listed the palette stays open.
    fn pick(&mut self) -> Chosen {
        let Some(act) = self
            .listing()
            .and_then(|listing| listing.current().map(|entry| entry.act.clone()))
        else {
            return Chosen::Read;
        };
        self.close_palette();
        match act {
            Act::Insert(words) => {
                self.put(&words);
                Chosen::Inserted
            }
            Act::Press(key) => Chosen::Press(key),
        }
    }

    /// Put a command chosen in the palette in the box. Words already there
    /// (the unsent draft, while a history entry is shown) are set aside, after
    /// any set aside earlier: nothing typed is lost, nothing is sent.
    fn put(&mut self, words: &str) {
        let unsent = match self.recall.take() {
            Some(_) => self.draft.take().unwrap_or_default().join("\n"),
            None => self.text(),
        };
        if !unsent.trim().is_empty() && !command_word(&unsent) {
            self.aside = Some(match self.aside.take() {
                Some(earlier) => format!("{earlier}\n{unsent}"),
                None => unsent,
            });
        }
        self.replace_with(words.to_owned());
        self.chooser.picked = Some(words.to_owned());
        self.chooser.closed_for = None;
    }

    /// The words set aside come back to the box, replacing what is there.
    pub(crate) fn restore_aside(&mut self) {
        if let Some(aside) = self.aside.take() {
            self.replace_with(aside);
            self.recall = None;
        }
    }
}
