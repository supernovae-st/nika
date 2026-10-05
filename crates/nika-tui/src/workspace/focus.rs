// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Which region of the workspace listens to the keyboard, and what a key does
//! there. The conversation's composer has the keys by default, so typing never
//! needs a first click. `F6` moves to the next region and `Shift+F6` back (the
//! aside is skipped when the width folds it); `Esc` returns to the composer.
//! In the aside, `Up`/`Down`/`Home`/`End` move the selection, `Left`/`Right`
//! choose the projection (Nika · Files) and `Enter` opens the entry: the
//! object in view changes, the conversation does not, and nothing is attached
//! to the next message. In the object, `Up`/`Down` and the page keys scroll
//! it, `Left`/`Right` change its face and `r` asks its owner to read it again;
//! on the task list of a run in view the desk reads `Up`/`Down`, `Enter` and
//! `Backspace` first (it picks a task and opens its detail).
//! `Tab` stays the composer's completion key.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::aside::Tab;

/// A region that can hold the keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Region {
    /// The conversation's composer (the default).
    Conversation,
    /// The project aside.
    Aside,
    /// The object in view.
    Object,
}

/// What the caller does after a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Action {
    /// The key belongs to the composer: hand it over unchanged.
    Compose,
    /// The region ignores the key; nothing changes.
    Ignored,
    /// The focus, the selection or the scroll moved: repaint.
    Moved,
    /// Open the aside entry at this index as the object in view. It opens
    /// only; attaching it to the next message is a separate act.
    Open(usize),
    /// Show the object's next face (`true`) or its previous one.
    Face(bool),
    /// Look at the object again: its owner reads it anew.
    Again,
}

/// What the regions hold at the moment of the key, as the screen shows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Extent {
    /// Whether the aside is on screen (from 120 columns).
    pub aside_shown: bool,
    /// The entries the aside lists.
    pub aside_entries: usize,
    /// The lines of the object in view.
    pub object_lines: usize,
    /// The rows the object region shows.
    pub object_rows: u16,
}

/// The keyboard focus and the positions it moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Focus {
    /// The region that has the keys.
    pub region: Region,
    /// The selected aside entry.
    pub selected: usize,
    /// The first object line shown.
    pub scroll: usize,
    /// The aside's projection.
    pub tab: Tab,
}

impl Focus {
    /// The composer has the keys; the Nika projection, nothing selected or
    /// scrolled. (No `Default`: the start is named, like every other state of
    /// the screen.)
    #[must_use]
    pub const fn composing() -> Self {
        Self {
            region: Region::Conversation,
            selected: 0,
            scroll: 0,
            tab: Tab::Nika,
        }
    }

    /// Apply one key under `extent`.
    pub fn handle(&mut self, key: KeyEvent, extent: Extent) -> Action {
        if key.code == KeyCode::F(6) {
            let back = key.modifiers.contains(KeyModifiers::SHIFT);
            self.region = self.cycle(back, extent.aside_shown);
            return Action::Moved;
        }
        match self.region {
            Region::Conversation => Action::Compose,
            _ if key.code == KeyCode::Esc => {
                self.region = Region::Conversation;
                Action::Moved
            }
            Region::Aside => self.aside_key(key.code, extent.aside_entries),
            Region::Object => self.object_key(key.code, extent),
        }
    }

    /// Scroll the preview in rows without taking its keyboard focus or
    /// interpreting a run's task-navigation keys.
    pub(crate) fn scroll_rows(&mut self, older: bool, step: usize, extent: Extent) {
        let last = extent
            .object_lines
            .saturating_sub(usize::from(extent.object_rows));
        let before = self.scroll.min(last);
        self.scroll = if older {
            before.saturating_sub(step)
        } else {
            before.saturating_add(step).min(last)
        };
    }

    /// Move the listing's anchor while preserving the region holding the draft.
    pub(crate) fn scroll_aside(&mut self, older: bool, step: usize, entries: usize) {
        let last = entries.saturating_sub(1);
        let before = self.selected.min(last);
        self.selected = if older {
            before.saturating_sub(step)
        } else {
            before.saturating_add(step).min(last)
        };
    }

    /// The next region (or the previous one when `back`), skipping a folded aside.
    fn cycle(&self, back: bool, aside_shown: bool) -> Region {
        let order: &[Region] = if aside_shown {
            &[Region::Aside, Region::Conversation, Region::Object]
        } else {
            &[Region::Conversation, Region::Object]
        };
        let at = order.iter().position(|r| *r == self.region).unwrap_or(0);
        let step = if back { order.len() - 1 } else { 1 };
        order[(at + step) % order.len()]
    }

    fn aside_key(&mut self, code: KeyCode, entries: usize) -> Action {
        let tab = match code {
            KeyCode::Left => Some(Tab::Nika),
            KeyCode::Right => Some(Tab::Files),
            _ => None,
        };
        if let Some(tab) = tab {
            if tab == self.tab {
                return Action::Ignored;
            }
            // Another projection lists other entries: the selection starts over.
            self.tab = tab;
            self.selected = 0;
            return Action::Moved;
        }
        let last = entries.saturating_sub(1);
        let before = self.selected.min(last);
        let after = match code {
            KeyCode::Up => before.saturating_sub(1),
            KeyCode::Down => (before + 1).min(last),
            KeyCode::Home => 0,
            KeyCode::End => last,
            KeyCode::Enter if entries > 0 => return Action::Open(before),
            _ => return Action::Ignored,
        };
        self.selected = after;
        if after == before {
            Action::Ignored
        } else {
            Action::Moved
        }
    }

    fn object_key(&mut self, code: KeyCode, extent: Extent) -> Action {
        let page = usize::from(extent.object_rows.saturating_sub(1)).max(1);
        let last = extent
            .object_lines
            .saturating_sub(usize::from(extent.object_rows));
        let before = self.scroll.min(last);
        let after = match code {
            KeyCode::Right => return Action::Face(true),
            KeyCode::Left => return Action::Face(false),
            KeyCode::Char('r') => return Action::Again,
            KeyCode::Up => before.saturating_sub(1),
            KeyCode::Down => (before + 1).min(last),
            KeyCode::PageUp => before.saturating_sub(page),
            KeyCode::PageDown => (before + page).min(last),
            KeyCode::Home => 0,
            KeyCode::End => last,
            _ => return Action::Ignored,
        };
        self.scroll = after;
        if after == before {
            Action::Ignored
        } else {
            Action::Moved
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn shift(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::SHIFT)
    }

    const WIDE: Extent = Extent {
        aside_shown: true,
        aside_entries: 3,
        object_lines: 40,
        object_rows: 10,
    };

    const NARROW: Extent = Extent {
        aside_shown: false,
        ..WIDE
    };

    #[test]
    fn the_composer_has_the_keys_until_f6_moves_them() {
        let mut focus = Focus::composing();
        for code in [
            KeyCode::Char('y'),
            KeyCode::Enter,
            KeyCode::Tab,
            KeyCode::Up,
            KeyCode::Esc,
        ] {
            assert_eq!(focus.handle(key(code), WIDE), Action::Compose, "{code:?}");
        }
        assert_eq!(focus.region, Region::Conversation);
        assert_eq!(focus.handle(key(KeyCode::F(6)), WIDE), Action::Moved);
        assert_eq!(focus.region, Region::Object);
        focus.handle(key(KeyCode::F(6)), WIDE);
        assert_eq!(focus.region, Region::Aside);
        focus.handle(key(KeyCode::F(6)), WIDE);
        assert_eq!(focus.region, Region::Conversation);
        focus.handle(shift(KeyCode::F(6)), WIDE);
        assert_eq!(
            focus.region,
            Region::Aside,
            "Shift+F6 goes left to the aside"
        );
    }

    #[test]
    fn a_folded_aside_is_skipped_and_esc_returns_to_the_composer() {
        let mut focus = Focus::composing();
        focus.handle(key(KeyCode::F(6)), NARROW);
        assert_eq!(focus.region, Region::Object);
        assert_eq!(focus.handle(key(KeyCode::Esc), NARROW), Action::Moved);
        assert_eq!(focus.region, Region::Conversation);
    }

    #[test]
    fn the_aside_selects_and_opens_without_touching_the_conversation() {
        let mut focus = Focus::composing();
        focus.handle(shift(KeyCode::F(6)), WIDE);
        assert_eq!(focus.handle(key(KeyCode::Up), WIDE), Action::Ignored);
        assert_eq!(focus.handle(key(KeyCode::Down), WIDE), Action::Moved);
        assert_eq!(focus.handle(key(KeyCode::End), WIDE), Action::Moved);
        assert_eq!(focus.selected, 2);
        assert_eq!(focus.handle(key(KeyCode::Down), WIDE), Action::Ignored);
        assert_eq!(focus.handle(key(KeyCode::Enter), WIDE), Action::Open(2));
        // Opening keeps the aside focused; the composer and its thread are untouched.
        assert_eq!(focus.region, Region::Aside);
        assert_eq!(focus.handle(key(KeyCode::Char('x')), WIDE), Action::Ignored);
        let empty = Extent {
            aside_entries: 0,
            ..WIDE
        };
        let mut none = Focus::composing();
        none.handle(shift(KeyCode::F(6)), empty);
        assert_eq!(none.handle(key(KeyCode::Enter), empty), Action::Ignored);
    }

    /// Left and Right choose the projection in the aside only; another
    /// projection starts its selection over.
    #[test]
    fn the_aside_arrows_choose_the_projection() {
        let mut focus = Focus::composing();
        assert_eq!(focus.handle(key(KeyCode::Right), WIDE), Action::Compose);
        assert_eq!(focus.tab, Tab::Nika);
        focus.handle(shift(KeyCode::F(6)), WIDE);
        focus.handle(key(KeyCode::Down), WIDE);
        assert_eq!(focus.handle(key(KeyCode::Left), WIDE), Action::Ignored);
        assert_eq!(focus.handle(key(KeyCode::Right), WIDE), Action::Moved);
        assert_eq!((focus.tab, focus.selected), (Tab::Files, 0));
        assert_eq!(focus.handle(key(KeyCode::Right), WIDE), Action::Ignored);
        assert_eq!(focus.handle(key(KeyCode::Left), WIDE), Action::Moved);
        assert_eq!(focus.tab, Tab::Nika);
    }

    #[test]
    fn the_object_scrolls_within_its_lines() {
        let mut focus = Focus::composing();
        focus.handle(key(KeyCode::F(6)), WIDE);
        assert_eq!(focus.region, Region::Object);
        assert_eq!(focus.handle(key(KeyCode::Up), WIDE), Action::Ignored);
        assert_eq!(focus.handle(key(KeyCode::PageDown), WIDE), Action::Moved);
        assert_eq!(focus.scroll, 9);
        focus.handle(key(KeyCode::End), WIDE);
        assert_eq!(focus.scroll, 30, "40 lines in 10 rows end at line 30");
        assert_eq!(focus.handle(key(KeyCode::Down), WIDE), Action::Ignored);
        focus.handle(key(KeyCode::Home), WIDE);
        assert_eq!(focus.scroll, 0);
        let short = Extent {
            object_lines: 4,
            ..WIDE
        };
        assert_eq!(focus.handle(key(KeyCode::PageDown), short), Action::Ignored);
    }
}
