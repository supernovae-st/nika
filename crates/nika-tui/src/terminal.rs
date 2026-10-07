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
//! (probed) → the alternate screen and mouse capture (full screen only). On restore:
//! raw mode first ("it has more side effects", ratatui), then the alternate
//! screen after mouse capture ends, the keyboard flags, focus change, bracketed paste, and the cursor
//! shown with its default shape.

use std::io::{self, IsTerminal as _, Write as _};
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::cursor::{SetCursorStyle, Show};
use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
    EnableFocusChange, EnableMouseCapture, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    is_raw_mode_enabled, supports_keyboard_enhancement,
};
use ratatui::backend::{Backend, ClearType, CrosstermBackend};
use ratatui::layout::Position;
use ratatui::{Terminal, TerminalOptions, Viewport};

use crate::model::Presentation;

/// The terminal ratatui draws on: crossterm over the process stdout.
pub type Screen = Terminal<CrosstermBackend<io::Stdout>>;

static RAW: AtomicBool = AtomicBool::new(false);
static PASTE: AtomicBool = AtomicBool::new(false);
static FOCUS: AtomicBool = AtomicBool::new(false);
static KITTY: AtomicBool = AtomicBool::new(false);
static ALT: AtomicBool = AtomicBool::new(false);
static MOUSE: AtomicBool = AtomicBool::new(false);
/// The terminal's title was pushed (xterm title stack · `CSI 22;0 t`)
/// before ours was set; the restore pops it (`CSI 23;0 t`) so the shell's
/// own title returns, on the panic path too.
static TITLE: AtomicBool = AtomicBool::new(false);

/// Name the terminal window for this session, keeping the previous title
/// on the terminal's stack so [`restore_everything`] gives it back. No
/// control character reaches the terminal: the title comes from a directory
/// name, and an ESC or BEL in it would close the title sequence and start
/// one of its own (an OSC 52 clipboard write under tmux, for one).
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
    crossterm::execute!(out, crossterm::terminal::SetTitle(printable(title)))?;
    out.flush()
}

/// `text` without its control characters: C0, DEL and C1.
fn printable(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_control()).collect()
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
    match screen_over(CrosstermBackend::new(io::stdout()), presentation) {
        Ok(screen) => Ok((owner, screen)),
        Err(error) => {
            owner.restore()?;
            Err(error)
        }
    }
}

/// The screen for one presentation over `backend`. An inline viewport is
/// cleared as it is created (without a second cursor query): a partial
/// line the shell or a handed-back run left under the cursor never shows
/// inside the live area.
fn screen_over<B: Backend>(
    backend: B,
    presentation: Presentation,
) -> Result<Terminal<B>, B::Error> {
    let viewport = match presentation {
        Presentation::Inline => Viewport::Inline(INLINE_HEIGHT),
        Presentation::Focus | Presentation::Workspace => Viewport::Fullscreen,
    };
    let mut screen = Terminal::with_options(backend, TerminalOptions { viewport })?;
    if presentation == Presentation::Inline {
        clear_inline(&mut screen)?;
    }
    Ok(screen)
}

/// Make the renderer's colour decision the only one: crossterm reads
/// `NO_COLOR` on its own and would drop every hue even when the caller's
/// chain (`--color`, `CLICOLOR_FORCE`) asked for colour, after the renderer
/// had already traded its weights for hues. The caller's decision
/// (`app::Options::color`) already honours `NO_COLOR`; crossterm's second
/// reading is switched off, so what the renderer paints is what shows.
fn own_the_colour() {
    crossterm::style::Colored::set_ansi_color_disabled(false);
}

fn enter_modes(presentation: Presentation) -> io::Result<()> {
    let mut out = io::stdout();
    own_the_colour();
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
    if presentation != Presentation::Inline {
        set_alternate_screen(true)?;
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
            if MOUSE.swap(false, Ordering::SeqCst) {
                crossterm::execute!(out, DisableMouseCapture)?;
            }
            crossterm::execute!(out, LeaveAlternateScreen)?;
            ALT.store(false, Ordering::SeqCst);
        }
        _ => {}
    }
    if on && !MOUSE.swap(true, Ordering::SeqCst) {
        // Mark before writing so the owner also cleans up a partial enable.
        crossterm::execute!(out, EnableMouseCapture)?;
    }
    out.flush()
}

/// Whether the alternate screen is on right now.
#[must_use]
pub fn on_alternate_screen() -> bool {
    ALT.load(Ordering::SeqCst)
}

/// Erase the inline viewport on the way out: the live area (status,
/// composer, hint, the « interrupted » notice) is chrome that leaves with
/// the door, so the shell's prompt returns right under the conversation.
/// The cursor moves to the viewport's first row and everything from there
/// down is cleared; the scrollback above stays as the session left it. The
/// loop calls it before [`Owner::restore`] when it leaves the inline
/// presentation; the focus presentation's alternate screen needs nothing.
///
/// # Errors
///
/// The backend could not move the cursor, clear or flush.
pub fn clear_inline<B: Backend>(screen: &mut Terminal<B>) -> Result<(), B::Error> {
    let top = screen.get_frame().area().y;
    screen.set_cursor_position(Position::new(0, top))?;
    screen.backend_mut().clear_region(ClearType::AfterCursor)?;
    screen.backend_mut().flush()
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
    if MOUSE.swap(false, Ordering::SeqCst) {
        note(crossterm::execute!(out, DisableMouseCapture));
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
        assert!(!MOUSE.load(Ordering::SeqCst));
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

    /// A title keeps no control byte: ESC, BEL, DEL and the C1 range would
    /// let a directory name end the title sequence and write its own (an
    /// OSC 52 clipboard write, a CSI colour); every other character stays.
    #[test]
    fn a_title_keeps_no_control_byte() {
        let hostile = "proj\x1b]52;c;ZXZpbA==\x07\x1b\\\u{9b}31m\u{9d}0;x\u{7f}ect";
        let clean = printable(hostile);
        assert_eq!(clean, "proj]52;c;ZXZpbA==\\31m0;xect");
        assert!(!clean.chars().any(char::is_control), "{clean:?}");
        assert_eq!(printable("nika · démo 日本 🦋"), "nika · démo 日本 🦋");
    }

    /// The inline viewport starts clean: a partial line left under the cursor
    /// (the shell's, or a handed-back run's last words) is cleared as the
    /// viewport is created, and the rows above it stay as they were.
    #[test]
    #[allow(clippy::expect_used)]
    fn the_inline_viewport_is_cleared_as_it_is_created() {
        use ratatui::backend::TestBackend;
        let mut lines = vec!["the run said this", "partial line"];
        lines.resize(16, "");
        let mut backend = TestBackend::with_lines(lines);
        backend
            .set_cursor_position(Position::new(12, 1))
            .expect("a cursor");
        let screen = screen_over(backend, Presentation::Inline).expect("an inline screen");
        let buffer = screen.backend().buffer().clone();
        let row = |y: u16| -> String {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol().to_owned())
                .collect::<String>()
                .trim_end()
                .to_owned()
        };
        assert_eq!(row(0), "the run said this", "the scrollback stays");
        for y in 1..buffer.area.height {
            assert_eq!(row(y), "", "row {y} inside the viewport");
        }
    }

    /// The renderer owns colour: once the terminal is taken, crossterm's own
    /// `NO_COLOR` reading no longer drops the hues a `--color` asked for.
    #[test]
    fn the_renderer_owns_the_colour() {
        use crossterm::style::{Color, Colored};
        // As crossterm memoizes it when NO_COLOR=1 is in the environment.
        Colored::set_ansi_color_disabled(true);
        assert!(
            Colored::ForegroundColor(Color::Yellow)
                .to_string()
                .is_empty()
        );
        own_the_colour();
        assert!(!Colored::ansi_color_disabled_memoized());
        assert!(
            !Colored::ForegroundColor(Color::Yellow)
                .to_string()
                .is_empty()
        );
    }

    #[test]
    fn entering_on_a_pipe_fails_and_leaves_no_owner_behind() {
        let error = enter(Presentation::Inline, None).err();
        assert_eq!(error.map(|e| e.kind()), Some(io::ErrorKind::Other));
        assert!(!RAW.load(Ordering::SeqCst));
    }

    /// B4 · the live area leaves with the door: after two `Ctrl+C` the
    /// composer frame and the « interrupted » notice were still painted under
    /// the conversation. The clear erases every viewport row, keeps what the
    /// session committed above it, and parks the cursor on the viewport's
    /// first row, where the shell's prompt comes back.
    #[test]
    #[allow(clippy::expect_used)]
    fn leaving_inline_erases_the_live_area_and_keeps_the_conversation() {
        use ratatui::backend::TestBackend;
        use ratatui::style::Style;
        let mut backend = TestBackend::new(24, 8);
        backend
            .set_cursor_position(Position::new(0, 1))
            .expect("a cursor");
        let mut screen = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Inline(4),
            },
        )
        .expect("an inline test terminal");
        screen
            .insert_before(1, |buf| {
                buf.set_string(0, 0, "the conversation", Style::default());
            })
            .expect("a committed block");
        screen
            .draw(|frame| {
                let area = frame.area();
                let buf = frame.buffer_mut();
                buf.set_string(0, area.y, "interrupted", Style::default());
                buf.set_string(0, area.y + 1, "nika ›", Style::default());
                buf.set_string(0, area.y + 2, "describe work", Style::default());
            })
            .expect("the live area");
        let top = screen.get_frame().area().y;
        clear_inline(&mut screen).expect("the clear");
        let buffer = screen.backend().buffer().clone();
        let row = |y: u16| -> String {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol().to_owned())
                .collect::<String>()
                .trim_end()
                .to_owned()
        };
        assert_eq!(row(top - 1), "the conversation", "the scrollback stays");
        for y in top..buffer.area.height {
            assert_eq!(row(y), "", "viewport row {y} still painted");
        }
        let cursor = screen.backend_mut().get_cursor_position().ok();
        assert_eq!(
            cursor,
            Some(Position::new(0, top)),
            "the prompt returns here"
        );
    }
}
