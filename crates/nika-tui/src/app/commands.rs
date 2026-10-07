// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The shell's input: which reader takes a key first, and the command
//! chooser.
//!
//! The shell's own readers come first ([`Shell::catch`]): the full
//! diagnostic while it is open ([`diagnostic`]), the palette key (`Ctrl+O`,
//! from every region), the diagnostic key (`F2`, only while a summarized
//! diagnostic exists), then the composer's chooser while the composer holds
//! the keys or the palette is open ([`crate::composer::chooser`]). What they
//! leave takes the ordinary precedence ([`decide`]): `Ctrl+C` (the
//! interruption, in every presentation and region), `Ctrl+T`, `Ctrl+L`, the
//! full-screen presentation's keys, then the composer. `Ctrl+C` is never
//! taken by a reader: whatever is open closes, the draft as it was, and the
//! interruption acts.
//!
//! The palette opens in the composer's row, with the keys, from whichever
//! region held them. Cancelling it (`Esc`, `Ctrl+O` again, or any key it
//! does not read) gives the keys back to that region, the draft exact; a view
//! key chosen in it is pressed from that region too, except the conversation
//! navigation entries, which give the conversation the keys. Only a command chosen
//! leaves the keys on the composer, where the `Enter` that sends it is the
//! human's own act. Choosing never sends: a view key chosen is pressed
//! through the ordinary path, exactly as if typed. The chooser offers what
//! exists ([`catalog`]): the commands the conversation answers now, the view
//! keys the presentation routes. While a turn works the same readers run
//! first ([`Shell::catch_busy`]), so arrows and `Tab` choose without reaching
//! the turn, and what they leave keeps the busy law ([`busy_key`]).

mod catalog;
pub(super) mod diagnostic;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{Heard, Shell, is_ctrl_c};
use crate::composer::chooser::Chosen;
use crate::events::UiEvent;
use crate::model::{Committed, Conversation, Presentation, UiState};
use crate::workspace::desk::{self, Desk, Route};
use crate::workspace::focus::Region;
use crate::workspace::geometry;

/// What one key press decides, before anything is done about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum KeyDecision {
    /// `Ctrl+C`: the state-aware interruption, in every presentation.
    Interrupt,
    /// `Ctrl+T`: go to this presentation.
    Present(Presentation),
    /// `Ctrl+L`: everything drawn again, in every presentation.
    Repaint,
    /// A full-screen presentation read the key.
    Route(Route),
    /// The composer's key.
    Compose,
}

/// Decide one key by the precedence the module names: `Ctrl+C`, `Ctrl+T`,
/// `Ctrl+L`, the full-screen presentation's keys, then the composer. The
/// workspace's focus moves here when the key moves it; nothing else changes.
pub(super) fn decide(state: &UiState, desk: &mut Desk, key: KeyEvent) -> KeyDecision {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('c') if ctrl => return KeyDecision::Interrupt,
        KeyCode::Char('t') if ctrl => {
            return KeyDecision::Present(state.presentation.toggled_at(state.size));
        }
        KeyCode::Char('l') if ctrl => return KeyDecision::Repaint,
        _ => {}
    }
    let route = match state.presentation {
        Presentation::Inline => return KeyDecision::Compose,
        Presentation::Focus => desk::composer_route(key),
        Presentation::Workspace => desk.route(key, state.size),
    };
    match route {
        Route::Compose => KeyDecision::Compose,
        other => KeyDecision::Route(other),
    }
}

/// What a key does while Nika works.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Busy {
    /// An edit of the draft, live: a word, a correction, a line break, a
    /// history recall, a completion.
    Edit,
    /// A bare `Enter`: nothing is sent while Nika works.
    Hold,
    /// Scroll the transcript one page back (a full screen).
    Older,
    /// Scroll the transcript one page forward (a full screen).
    Newer,
    /// Anything else: it waits for the turn.
    Later,
    /// `Esc` from the workspace's composer region: it leaves once the turn is
    /// over, and the hint row says so.
    Leave,
    /// A workspace region read the key (a selection, a scroll, a face, an
    /// opened entry, the keyboard focus): only the view changed.
    Region,
}

/// The hint row's words when `Enter` is pressed while Nika works.
pub(super) const ENTER_WAITS: &str = "Nika is working · Enter sends when it is your turn";

/// The hint row's words when `Esc` would leave the workspace while Nika works.
pub(super) const LEAVE_WAITS: &str = "Nika is working · Esc leaves when it is your turn";

/// Sort a key typed while a turn runs: in the workspace the region that holds
/// the keyboard reads it first (only the view changes; a look it asks for is
/// taken when the turn ends), leaving waits for the turn, and what reaches
/// the composer follows [`during_turn`].
pub(super) fn busy_key(state: &UiState, desk: &mut Desk, key: KeyEvent) -> Busy {
    let ctrl_t = key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('t');
    if state.presentation != Presentation::Workspace || ctrl_t {
        return during_turn(state.presentation, key);
    }
    match desk.route(key, state.size) {
        Route::Compose => during_turn(state.presentation, key),
        Route::Older => Busy::Older,
        Route::Newer => Busy::Newer,
        Route::Leave => Busy::Leave,
        Route::Inspect => {
            desk.wants_look = true;
            Busy::Region
        }
        Route::Repaint | Route::Nothing => Busy::Region,
    }
}

/// Sort a key typed while a turn runs in `presentation` (`Ctrl+C` is heard
/// before this).
fn during_turn(presentation: Presentation, key: KeyEvent) -> Busy {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let full_screen = presentation != Presentation::Inline;
    match key.code {
        KeyCode::Enter if alt || shift || ctrl => Busy::Edit,
        KeyCode::Enter => Busy::Hold,
        KeyCode::Char('j') if ctrl => Busy::Edit,
        KeyCode::Char(_) if !ctrl => Busy::Edit,
        KeyCode::PageUp if full_screen => Busy::Older,
        KeyCode::PageDown if full_screen => Busy::Newer,
        KeyCode::Backspace
        | KeyCode::Delete
        | KeyCode::Left
        | KeyCode::Right
        | KeyCode::Home
        | KeyCode::End
        | KeyCode::Up
        | KeyCode::Down
        | KeyCode::Tab => Busy::Edit,
        _ => Busy::Later,
    }
}

/// The key that opens and closes the palette, from every region: `Ctrl+O`
/// (the composer leaves it unbound; `Ctrl+K` keeps its line editing).
fn is_palette_key(key: &KeyEvent) -> bool {
    key.code == KeyCode::Char('o') && key.modifiers == KeyModifiers::CONTROL
}

/// What the shell's own readers did with a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Caught {
    /// A reader took it: only the view, the selection, the search or the
    /// draft's words changed; nothing was sent, answered or stopped.
    Read,
    /// The palette chose this view key: it takes the ordinary path, as if
    /// it had been pressed.
    Press(KeyEvent),
}

impl<C: Conversation + 'static> Shell<C> {
    /// Whether the composer holds the keys: always inline and in the focus
    /// view, in the workspace when its focus is on the conversation (or
    /// while the focus view stands in for a workspace too small to fit).
    pub(super) fn composer_has_keys(&self) -> bool {
        match self.state.presentation {
            Presentation::Inline | Presentation::Focus => true,
            Presentation::Workspace => {
                !geometry::fits(self.state.size) || self.desk.focus.region == Region::Conversation
            }
        }
    }

    /// Offer the chooser what exists now: the conversation's commands and
    /// the view keys the presentation routes; tell it whether it has the keys.
    pub(super) fn sync_choices(&mut self) {
        let view = catalog::View {
            presentation: self.state.presentation,
            fits: geometry::fits(self.state.size),
            scrolled: self.state.focus_scroll > 0,
            // Only the palette lists keys: the transcript is read for it alone.
            diagnostic: self.composer.palette_open() && self.summarized().is_some(),
        };
        let entries = catalog::offered(&self.commands, view);
        self.composer.offer(entries);
        let focused = self.composer_has_keys();
        self.composer.set_focused(focused);
    }

    /// The latest block the conversation's cards show summarized, whose full
    /// words the diagnostic view shows.
    fn summarized(&self) -> Option<&Committed> {
        diagnostic::latest(&self.state.transcript, diagnostic::summarized)
    }

    /// The shell's readers, before the ordinary precedence (the module's
    /// order). `None`: the key takes the ordinary path.
    pub(super) fn catch(&mut self, key: KeyEvent) -> Option<Caught> {
        if !self.composer.palette_open() {
            // Closed by an edit of the draft (typeahead): the keys stay there.
            self.palette_from = None;
        }
        if is_ctrl_c(&key) {
            // The interruption keeps its place: what is open closes first.
            self.diagnostic = None;
            self.close_palette();
            return None;
        }
        if let Some(open) = self.diagnostic.as_mut() {
            match open.key(key) {
                diagnostic::Viewed::Read => return Some(self.caught(Caught::Read)),
                diagnostic::Viewed::Closed => {
                    self.diagnostic = None;
                    return Some(self.caught(Caught::Read));
                }
                diagnostic::Viewed::Passed => self.diagnostic = None,
            }
        }
        if is_palette_key(&key) {
            if self.composer.palette_open() {
                self.close_palette();
            } else {
                self.sync_choices();
                self.keep_reading(|_, composer| composer.toggle_palette());
                // The palette opens in the composer's row, with the keys.
                self.palette_from = Some(self.desk.focus.region);
                self.desk.focus.region = Region::Conversation;
                self.sync_choices();
            }
            return Some(self.caught(Caught::Read));
        }
        if self.open_diagnostic(key) {
            return Some(self.caught(Caught::Read));
        }
        if !self.composer.palette_open() && !self.composer_has_keys() {
            return None;
        }
        self.sync_choices();
        let chosen = self.keep_reading(|_, composer| composer.choose(key));
        if chosen == Chosen::Inserted {
            // A command chosen: the keys stay on the composer for its `Enter`.
            self.palette_from = None;
        } else if !self.composer.palette_open() {
            // Cancelled, or a key to press or to pass on: from where it opened.
            self.give_keys_back();
        }
        match chosen {
            Chosen::Pass => None,
            Chosen::Read | Chosen::Inserted => Some(self.caught(Caught::Read)),
            Chosen::Press(key) if self.open_diagnostic(key) => Some(self.caught(Caught::Read)),
            Chosen::Press(key) => {
                // These entries name the conversation, regardless of where
                // the palette opened. Other view keys retain their origin.
                if self.state.presentation == Presentation::Workspace
                    && geometry::fits(self.state.size)
                    && key.modifiers.is_empty()
                    && matches!(key.code, KeyCode::PageUp | KeyCode::PageDown | KeyCode::End)
                {
                    self.desk.focus.region = Region::Conversation;
                }
                Some(self.caught(Caught::Press(key)))
            }
        }
    }

    /// Both the physical key and the palette entry open the same reader.
    fn open_diagnostic(&mut self, key: KeyEvent) -> bool {
        if key.code != catalog::DIAGNOSTIC_KEY || !key.modifiers.is_empty() {
            return false;
        }
        let Some(block) = self.summarized() else {
            return false;
        };
        self.diagnostic = Some(diagnostic::Diagnostic::of(block));
        true
    }

    /// Close the palette, the draft exact, and give the keys back to the
    /// region that held them when it opened.
    fn close_palette(&mut self) {
        self.keep_reading(|_, composer| composer.close_palette());
        self.give_keys_back();
    }

    /// The keys back to the region that held them when the palette opened.
    fn give_keys_back(&mut self) {
        if let Some(region) = self.palette_from.take() {
            self.desk.focus.region = region;
        }
    }

    /// A reader took the key: as at the ordinary path, any key keeps the
    /// session (an armed interruption disarms) and the last notice ends.
    fn caught(&mut self, caught: Caught) -> Caught {
        self.state.interrupt_armed = false;
        self.state.completion = None;
        caught
    }

    /// One event heard while a turn runs: the shell's readers first, the busy
    /// law ([`Self::hear`]) for what they leave.
    pub(super) fn hear_first(&mut self, event: UiEvent, armed: &mut bool) -> Heard {
        match self.catch_busy(event, armed) {
            Ok(heard) => heard,
            Err(event) => self.hear(event, armed),
        }
    }

    /// [`Self::catch`] for an event heard while a turn runs: `Ok` with what
    /// to draw when a reader took it (a pasted text goes to an open palette's
    /// search), `Err` with the event for the busy law. A view key chosen in
    /// the palette goes through the busy law too.
    fn catch_busy(&mut self, event: UiEvent, armed: &mut bool) -> Result<Heard, UiEvent> {
        match event {
            UiEvent::Key(key) => match self.catch(key) {
                None => Err(UiEvent::Key(key)),
                Some(Caught::Read) => {
                    self.typed_live = true;
                    Ok(Heard::Redraw)
                }
                Some(Caught::Press(key)) => Ok(self.hear(UiEvent::Key(key), armed)),
            },
            UiEvent::Paste(text) if self.composer.palette_open() => {
                self.typed_live = true;
                self.keep_reading(|_, composer| composer.paste_query(&text));
                Ok(Heard::Redraw)
            }
            other => Err(other),
        }
    }

    /// A paste at rest: the open palette's search takes it, else the draft;
    /// data either way, never a key.
    pub(super) fn paste(&mut self, text: &str) {
        if !self.composer.paste_query(text) {
            self.composer.paste(text);
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::{Busy, during_turn, is_palette_key};
    use crate::model::Presentation;

    /// While Nika works the composer takes words and edits, a bare `Enter`
    /// is held, the page keys scroll only a full screen, and the rest waits.
    #[test]
    fn keys_typed_while_nika_works_edit_hold_scroll_or_wait() {
        let during = |presentation, code, modifiers| {
            during_turn(presentation, KeyEvent::new(code, modifiers))
        };
        let inline = Presentation::Inline;
        let none = KeyModifiers::NONE;
        for code in [
            KeyCode::Char('y'),
            KeyCode::Backspace,
            KeyCode::Left,
            KeyCode::Up,
            KeyCode::Tab,
        ] {
            assert_eq!(during(inline, code, none), Busy::Edit, "{code:?}");
        }
        assert_eq!(during(inline, KeyCode::Enter, none), Busy::Hold);
        assert_eq!(
            during(inline, KeyCode::Enter, KeyModifiers::ALT),
            Busy::Edit
        );
        assert_eq!(
            during(inline, KeyCode::Char('j'), KeyModifiers::CONTROL),
            Busy::Edit
        );
        assert_eq!(during(inline, KeyCode::PageUp, none), Busy::Later);
        let focus = Presentation::Focus;
        assert_eq!(during(focus, KeyCode::PageUp, none), Busy::Older);
        assert_eq!(during(focus, KeyCode::PageDown, none), Busy::Newer);
        for later in [
            (KeyCode::Char('t'), KeyModifiers::CONTROL),
            (KeyCode::Esc, none),
            (KeyCode::F(6), none),
        ] {
            assert_eq!(during(inline, later.0, later.1), Busy::Later, "{later:?}");
        }
    }

    /// Only `Ctrl+O` opens the palette: `Ctrl+K` stays the composer's
    /// kill-to-end-of-line, a plain `o` is a letter.
    #[test]
    fn the_palette_key_is_ctrl_o_alone() {
        let key = |code, modifiers| KeyEvent::new(code, modifiers);
        assert!(is_palette_key(&key(
            KeyCode::Char('o'),
            KeyModifiers::CONTROL
        )));
        for other in [
            key(KeyCode::Char('k'), KeyModifiers::CONTROL),
            key(KeyCode::Char('o'), KeyModifiers::NONE),
            key(
                KeyCode::Char('o'),
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            ),
        ] {
            assert!(!is_palette_key(&other), "{other:?}");
        }
    }
}
