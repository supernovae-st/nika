// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ONE owner of the terminal (ADR-139 · law 3).
//!
//! Every mode this crate switches on is switched on here, in a fixed order,
//! and switched off in the reverse order from exactly one place: the normal
//! exit, `Ctrl+C`, a panic (the hook installed by [`install_panic_hook`]) and
//! `SIGTERM` all land in [`restore_everything`]. What was never enabled is
//! never disabled: the flags below remember the state so the panic hook can
//! restore blindly without sending a keyboard-protocol pop to a terminal
//! that never received the push.
//!
//! Order on entry (Codex `set_modes`, ratatui `try_init_with_options`):
//! raw mode → bracketed paste → focus change (unix) → keyboard enhancement
//! (probed) → the alternate screen (focus presentation only). On restore:
//! raw mode first ("it has more side effects", ratatui), then the alternate
//! screen, the keyboard flags, focus change, bracketed paste, and the cursor
//! shown with its default shape.

use std::io::{self, IsTerminal as _, Write as _};
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::cursor::{SetCursorStyle, Show};
use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, EnableBracketedPaste, EnableFocusChange,
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    is_raw_mode_enabled, supports_keyboard_enhancement,
};
use ratatui::backend::CrosstermBackend;
use ratatui::{Terminal, TerminalOptions, Viewport};

use crate::model::Presentation;

/// The terminal ratatui draws on: crossterm over the process stdout.
pub type Screen = Terminal<CrosstermBackend<io::Stdout>>;

static RAW: AtomicBool = AtomicBool::new(false);
static PASTE: AtomicBool = AtomicBool::new(false);
static FOCUS: AtomicBool = AtomicBool::new(false);
static KITTY: AtomicBool = AtomicBool::new(false);
static ALT: AtomicBool = AtomicBool::new(false);
/// The terminal's title was pushed (xterm title stack · `CSI 22;0 t`)
/// before ours was set; the restore pops it (`CSI 23;0 t`) so the shell's
/// own title returns, on the panic path too.
static TITLE: AtomicBool = AtomicBool::new(false);

/// Name the terminal window for this session, keeping the previous title
/// on the terminal's stack so [`restore_everything`] gives it back.
///
/// # Errors
///
/// The crossterm command failed to reach the terminal.
pub fn set_title(title: &str) -> io::Result<()> {
    let mut out = io::stdout();
    // The previous title is pushed once; every later call only sets ours.
    if !TITLE.swap(true, Ordering::SeqCst) {
        write!(out, "\x1b[22;0t")?;
    }
    crossterm::execute!(out, crossterm::terminal::SetTitle(title))?;
    out.flush()
}

/// The inline viewport height the shell asks for at entry: the live area
/// (pending block · composer · status) grows and shrinks inside it.
pub const INLINE_HEIGHT: u16 = 12;

/// The owner. Dropping it restores the terminal; [`Owner::restore`] does the
/// same explicitly and reports the first error instead of swallowing it.
#[derive(Debug)]
#[non_exhaustive]
pub struct Owner {
    restored: bool,
}

/// The terminal is not interactive: bare `nika` keeps its plain line loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NotATerminal {
    /// stdin is not a TTY.
    Stdin,
    /// stdout is not a TTY.
    Stdout,
    /// `TERM=dumb`: no cursor addressing, no escape sequences.
    Dumb,
}

impl std::fmt::Display for NotATerminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stdin => f.write_str("stdin is not a terminal"),
            Self::Stdout => f.write_str("stdout is not a terminal"),
            Self::Dumb => f.write_str("TERM=dumb has no screen to draw on"),
        }
    }
}

impl std::error::Error for NotATerminal {}

/// Refuse to own anything that is not an interactive terminal: the plain
/// loop of the CLI is the right face of a pipe, a redirect or `TERM=dumb`.
///
/// # Errors
///
/// Which stream is not a terminal, or `TERM=dumb`. `term` is the caller's
/// reading of `TERM` (the renderer never reads the environment itself).
pub fn probe(term: Option<&str>) -> Result<(), NotATerminal> {
    if !io::stdin().is_terminal() {
        return Err(NotATerminal::Stdin);
    }
    if !io::stdout().is_terminal() {
        return Err(NotATerminal::Stdout);
    }
    if term == Some("dumb") {
        return Err(NotATerminal::Dumb);
    }
    Ok(())
}

/// Take the terminal for one presentation. The caller holds the [`Owner`]
/// for as long as it draws; the [`Screen`] is ratatui's handle.
///
/// # Errors
///
/// A refused probe (not a terminal) or a crossterm/ratatui failure while
/// enabling a mode; every mode enabled before the failure is restored.
pub fn enter(presentation: Presentation, term: Option<&str>) -> io::Result<(Owner, Screen)> {
    probe(term).map_err(io::Error::other)?;
    let mut owner = Owner { restored: false };
    if let Err(error) = enter_modes(presentation) {
        owner.restore()?;
        return Err(error);
    }
    let backend = CrosstermBackend::new(io::stdout());
    let viewport = match presentation {
        Presentation::Inline => Viewport::Inline(INLINE_HEIGHT),
        Presentation::Focus => Viewport::Fullscreen,
    };
    match Terminal::with_options(backend, TerminalOptions { viewport }) {
        Ok(screen) => Ok((owner, screen)),
        Err(error) => {
            owner.restore()?;
            Err(error)
        }
    }
}

fn enter_modes(presentation: Presentation) -> io::Result<()> {
    let mut out = io::stdout();
    enable_raw_mode()?;
    RAW.store(true, Ordering::SeqCst);
    crossterm::execute!(out, EnableBracketedPaste)?;
    PASTE.store(true, Ordering::SeqCst);
    if cfg!(unix) {
        crossterm::execute!(out, EnableFocusChange)?;
        FOCUS.store(true, Ordering::SeqCst);
    }
    if supports_keyboard_enhancement().unwrap_or(false) {
        crossterm::execute!(
            out,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )?;
        KITTY.store(true, Ordering::SeqCst);
    }
    if presentation == Presentation::Focus {
        crossterm::execute!(out, EnterAlternateScreen)?;
        ALT.store(true, Ordering::SeqCst);
    }
    out.flush()
}

/// Switch the alternate screen on or off while the owner lives: the focus
/// presentation is entered on request and left with the inline state kept.
///
/// # Errors
///
/// The crossterm command failed to reach the terminal.
pub fn set_alternate_screen(on: bool) -> io::Result<()> {
    let mut out = io::stdout();
    match (on, ALT.load(Ordering::SeqCst)) {
        (true, false) => {
            crossterm::execute!(out, EnterAlternateScreen)?;
            ALT.store(true, Ordering::SeqCst);
        }
        (false, true) => {
            crossterm::execute!(out, LeaveAlternateScreen)?;
            ALT.store(false, Ordering::SeqCst);
        }
        _ => {}
    }
    out.flush()
}

/// Whether the alternate screen is on right now.
#[must_use]
pub fn on_alternate_screen() -> bool {
    ALT.load(Ordering::SeqCst)
}

impl Owner {
    /// Restore the terminal now and report the first failure.
    ///
    /// # Errors
    ///
    /// The first crossterm command that failed; the remaining commands are
    /// still attempted, so a partial failure leaves as little enabled as
    /// possible.
    pub fn restore(&mut self) -> io::Result<()> {
        if self.restored {
            return Ok(());
        }
        self.restored = true;
        restore_everything()
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        // The error has no reader in a destructor; the panic hook and the
        // explicit restore already reported what could be reported.
        let _ = self.restore();
    }
}

/// Undo every mode that is on, in reverse order, attempting each step even
/// when an earlier one failed; the first error is returned.
///
/// # Errors
///
/// The first crossterm command that failed.
pub fn restore_everything() -> io::Result<()> {
    let mut first: Option<io::Error> = None;
    let mut note = |result: io::Result<()>| {
        if let Err(error) = result
            && first.is_none()
        {
            first = Some(error);
        }
    };
    let mut out = io::stdout();
    if RAW.swap(false, Ordering::SeqCst) || is_raw_mode_enabled().unwrap_or(false) {
        note(disable_raw_mode());
    }
    if ALT.swap(false, Ordering::SeqCst) {
        note(crossterm::execute!(out, LeaveAlternateScreen));
    }
    if KITTY.swap(false, Ordering::SeqCst) {
        note(crossterm::execute!(out, PopKeyboardEnhancementFlags));
    }
    if FOCUS.swap(false, Ordering::SeqCst) {
        note(crossterm::execute!(out, DisableFocusChange));
    }
    if PASTE.swap(false, Ordering::SeqCst) {
        note(crossterm::execute!(out, DisableBracketedPaste));
    }
    if TITLE.swap(false, Ordering::SeqCst) {
        note(write!(out, "\x1b[23;0t"));
    }
    note(crossterm::execute!(
        out,
        Show,
        SetCursorStyle::DefaultUserShape
    ));
    note(out.flush());
    match first {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// Install the panic hook that restores the terminal BEFORE the default
/// hook prints the panic: the message lands on a sane screen. Call it after
/// every other hook the program installs (ratatui's own rule), and before
/// [`enter`]; installing it twice chains harmlessly.
pub fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // A restore that fails inside a panic has nowhere to report.
        let _ = restore_everything();
        original(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_with_nothing_enabled_touches_nothing_but_the_cursor() {
        // Under `cargo test` stdout is a pipe: every flag is false, raw mode
        // is off, and the only commands sent are the cursor show/shape,
        // which a pipe accepts. The point: no flag was left true.
        let _ = restore_everything();
        assert!(!RAW.load(Ordering::SeqCst));
        assert!(!PASTE.load(Ordering::SeqCst));
        assert!(!FOCUS.load(Ordering::SeqCst));
        assert!(!KITTY.load(Ordering::SeqCst));
        assert!(!ALT.load(Ordering::SeqCst));
    }

    #[test]
    fn a_pipe_is_refused_before_any_mode_is_touched() {
        // `cargo test` never runs on a TTY for stdin; the probe refuses and
        // names which stream, so bare `nika` keeps its plain loop there.
        let refused = probe(Some("xterm")).err();
        assert!(matches!(
            refused,
            Some(NotATerminal::Stdin | NotATerminal::Stdout)
        ));
        assert!(probe(Some("dumb")).is_err());
        assert!(!NotATerminal::Dumb.to_string().is_empty());
    }

    #[test]
    fn entering_on_a_pipe_fails_and_leaves_no_owner_behind() {
        let error = enter(Presentation::Inline, None).err();
        assert_eq!(error.map(|e| e.kind()), Some(io::ErrorKind::Other));
        assert!(!RAW.load(Ordering::SeqCst));
    }
}
