// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The loop: one owner, one broker, one state, two presentations, one
//! conversation.
//!
//! Inline: every block the session finishes goes ABOVE the viewport through
//! `Terminal::insert_before`, into the terminal's own scrollback; the
//! viewport holds only the live area. Focus: the alternate screen shows the
//! transcript and the same live area; leaving it returns to the inline
//! viewport with the draft intact and the blocks finished meanwhile pushed
//! to the scrollback at that moment (they were never printed there).
//!
//! A handoff (a run through the plain path) hands the terminal back: the
//! reader parks, the viewport is cleared, the cursor moves to its first
//! row, every mode is restored, the conversation performs the work in the
//! terminal's normal flow, and the shell takes the terminal again with a
//! fresh viewport anchored below what the work printed.
//!
//! `Ctrl+C` is state-aware: with work active it interrupts and says so; idle,
//! the first press arms and the second leaves. `SIGTERM` leaves at once.
//! Every exit path restores the terminal through the one owner.

use std::io::{self, Write as _};

use crossterm::cursor::MoveTo;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{Clear, ClearType};

use crate::composer::{Composer, ComposerAction};
use crate::events::{Broker, Signal, UiEvent};
use crate::model::{Beat, Committed, Conversation, Handoff, Kind, Presentation, UiState};
use crate::render;
use crate::terminal::{self, Owner, Screen};

/// How the shell runs.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Options {
    /// The presentation at start.
    pub presentation: Presentation,
    /// Colour allowed.
    pub color: bool,
    /// Proof hook: panic after this many submitted lines.
    pub panic_after: Option<usize>,
    /// Proof hook: leave cleanly after this many submitted lines.
    pub exit_after: Option<usize>,
    /// The caller's reading of `TERM` (`dumb` refuses).
    pub term: Option<String>,
}

impl Options {
    /// Inline, no colour, no proof hook.
    #[must_use]
    pub fn new(presentation: Presentation) -> Self {
        Self {
            presentation,
            color: false,
            panic_after: None,
            exit_after: None,
            term: None,
        }
    }
}

/// Why the loop ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Exit {
    /// The session or the human closed the door.
    Quit,
    /// Two `Ctrl+C` while idle.
    Interrupted,
    /// `SIGTERM`.
    Terminated,
    /// The input reader closed.
    Closed,
}

impl Exit {
    /// The process exit code the shell maps to.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Quit | Self::Closed => 0,
            Self::Interrupted => 130,
            Self::Terminated => 143,
        }
    }
}

struct Shell<C: Conversation> {
    owner: Owner,
    screen: Screen,
    state: UiState,
    composer: Composer,
    conversation: C,
    options: Options,
    submitted: usize,
}

/// Run the shell over a conversation until it ends. The terminal is
/// restored before this returns, on every path.
///
/// # Errors
///
/// The terminal could not be taken (not a TTY) or a draw failed.
pub fn run<C: Conversation>(conversation: C, options: Options) -> io::Result<Exit> {
    terminal::install_panic_hook();
    let (owner, screen) = terminal::enter(options.presentation, options.term.as_deref())?;
    let size = crossterm::terminal::size().unwrap_or((80, 24));
    let state = UiState::new(options.presentation, options.color, size);
    let mut shell = Shell {
        owner,
        screen,
        state,
        composer: Composer::new(),
        conversation,
        options,
        submitted: 0,
    };
    let broker = Broker::start();
    let outcome = shell.drive(broker);
    shell.owner.restore()?;
    outcome
}

/// What a key or a signal asks of the loop beyond the state.
enum Step {
    Stay,
    Leave(Exit),
    Switch(Presentation),
    Handoff(Handoff),
}

impl<C: Conversation> Shell<C> {
    fn drive(&mut self, mut broker: Broker) -> io::Result<Exit> {
        let opening = self.conversation.open();
        self.apply_all(opening)?;
        self.draw()?;
        loop {
            let Some(event) = broker.recv() else {
                broker.stop();
                return Ok(Exit::Closed);
            };
            let step = match event {
                UiEvent::Key(key) => self.on_key(key)?,
                UiEvent::Paste(text) => {
                    self.composer.paste(&text);
                    Step::Stay
                }
                UiEvent::Resize(cols, rows) => {
                    // The inline viewport recomputes its origin from a
                    // cursor-position report: the reader parks while the
                    // terminal answers.
                    self.state.size = (cols, rows);
                    broker.pause();
                    let resized = self.screen.autoresize();
                    broker.resume();
                    resized?;
                    Step::Stay
                }
                UiEvent::FocusGained | UiEvent::FocusLost => Step::Stay,
                UiEvent::Signal(Signal::Terminate) => Step::Leave(Exit::Terminated),
                UiEvent::Signal(Signal::Interrupt) => self.interrupt()?,
                UiEvent::Closed => Step::Leave(Exit::Closed),
            };
            match step {
                Step::Leave(exit) => {
                    broker.stop();
                    return Ok(exit);
                }
                Step::Switch(to) => {
                    broker.pause();
                    let switched = self.switch(to);
                    broker.resume();
                    switched?;
                }
                Step::Handoff(handoff) => {
                    broker.pause();
                    let handed = self.hand_over(&handoff);
                    broker.resume();
                    let beats = handed?;
                    self.apply_all(beats)?;
                }
                Step::Stay => {}
            }
            // The last blocks are drawn before the door closes: a result the
            // human never saw is not a result.
            self.draw()?;
            if self.state.quit {
                broker.stop();
                return Ok(Exit::Quit);
            }
        }
    }

    fn on_key(&mut self, key: KeyEvent) -> io::Result<Step> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('c') if ctrl => return self.interrupt(),
            KeyCode::Char('t') if ctrl => {
                self.state.interrupt_armed = false;
                return Ok(Step::Switch(self.state.presentation.toggled()));
            }
            KeyCode::Esc if self.state.presentation == Presentation::Focus => {
                return Ok(Step::Switch(Presentation::Inline));
            }
            KeyCode::PageUp if self.state.presentation == Presentation::Focus => {
                let max = self.state.transcript.len().saturating_sub(1);
                self.state.focus_scroll = (self.state.focus_scroll + 1).min(max);
                return Ok(Step::Stay);
            }
            KeyCode::PageDown if self.state.presentation == Presentation::Focus => {
                self.state.focus_scroll = self.state.focus_scroll.saturating_sub(1);
                return Ok(Step::Stay);
            }
            _ => {}
        }
        self.state.interrupt_armed = false;
        match self.composer.handle(key) {
            ComposerAction::Submit(line) => {
                if let Some(handoff) = self.submit(&line)? {
                    return Ok(Step::Handoff(handoff));
                }
            }
            ComposerAction::Edited | ComposerAction::Ignored => {}
        }
        Ok(Step::Stay)
    }

    fn interrupt(&mut self) -> io::Result<Step> {
        if self.state.busy.is_some() {
            self.state.busy = None;
            self.state.transcript.push(Committed::new(
                Kind::Notice,
                "interrupted · the run's trace keeps what happened",
            ));
            self.commit_inline()?;
            return Ok(Step::Stay);
        }
        if self.state.interrupt_armed {
            return Ok(Step::Leave(Exit::Interrupted));
        }
        self.state.interrupt_armed = true;
        Ok(Step::Stay)
    }

    /// The human sent a line: echo it, let the conversation answer, and
    /// report the handoff it asks for, if any.
    fn submit(&mut self, line: &str) -> io::Result<Option<Handoff>> {
        self.submitted += 1;
        let echo = format!("{}{}", self.state.waiting.prompt(), line.trim_end());
        self.state
            .transcript
            .push(Committed::new(Kind::Human, echo));
        self.commit_inline()?;
        // The proof hook of the PTY suite: a panic inside the loop must
        // leave the terminal restored (the hook restores before the message
        // prints). Never reachable without the flag.
        assert!(
            self.options.panic_after != Some(self.submitted),
            "nika-tui-proto: panic requested after {} line(s)",
            self.submitted
        );
        let turn = self.conversation.submit(line);
        self.apply_all(turn.beats)?;
        if self.options.exit_after == Some(self.submitted) {
            self.state.quit = true;
        }
        Ok(turn.handoff)
    }

    /// Hand the terminal back for one piece of work and take it again. The
    /// reader is parked by the caller (the viewport's clear and the fresh
    /// viewport both ask the terminal where the cursor is).
    fn hand_over(&mut self, handoff: &Handoff) -> io::Result<Vec<Beat>> {
        self.commit_inline()?;
        if self.state.presentation == Presentation::Inline {
            // The viewport's rows are wiped by hand (ratatui's `clear` would
            // ask the terminal where the cursor is, one round-trip more than
            // the fresh viewport below already needs).
            let top = self.screen.get_frame().area().y;
            let mut out = io::stdout();
            crossterm::execute!(out, MoveTo(0, top), Clear(ClearType::FromCursorDown))?;
            out.flush()?;
        }
        self.owner.restore()?;
        let beats = self.conversation.perform(handoff);
        let (owner, screen) =
            terminal::enter(self.state.presentation, self.options.term.as_deref())?;
        self.owner = owner;
        self.screen = screen;
        if self.state.presentation == Presentation::Inline {
            // Everything the work printed is the terminal's now; the
            // transcript blocks before it were committed already.
            self.state.committed_inline = self.state.transcript.len();
        }
        Ok(beats)
    }

    fn apply_all(&mut self, beats: Vec<Beat>) -> io::Result<()> {
        for beat in beats {
            let busy = matches!(beat, Beat::Busy(_));
            self.state.apply(beat);
            if busy {
                // A busy label is a state, not a block: draw it now so the
                // human sees work is active before the next beat lands.
                self.draw()?;
            }
        }
        self.commit_inline()
    }

    /// Hand every block not yet in the scrollback to the terminal (inline
    /// only; the focus view reads the transcript itself).
    fn commit_inline(&mut self) -> io::Result<()> {
        if self.state.presentation != Presentation::Inline {
            return Ok(());
        }
        let width = self.state.size.0.max(1);
        let color = self.state.color;
        let pending: Vec<Committed> = self.state.uncommitted().to_vec();
        for block in &pending {
            let rows = render::wrapped_rows(&render::block_lines(block, color), width);
            self.screen
                .insert_before(rows, |buf| render::render_block(block, color, buf))?;
        }
        self.state.committed_inline = self.state.transcript.len();
        Ok(())
    }

    fn switch(&mut self, to: Presentation) -> io::Result<()> {
        if to == self.state.presentation {
            return Ok(());
        }
        match to {
            Presentation::Focus => {
                terminal::set_alternate_screen(true)?;
                self.screen = fresh_screen(Presentation::Focus)?;
                self.state.presentation = Presentation::Focus;
                self.state.focus_scroll = 0;
            }
            Presentation::Inline => {
                terminal::set_alternate_screen(false)?;
                self.screen = fresh_screen(Presentation::Inline)?;
                self.state.presentation = Presentation::Inline;
                self.commit_inline()?;
            }
        }
        Ok(())
    }

    fn draw(&mut self) -> io::Result<()> {
        let state = &self.state;
        let composer = &self.composer;
        match state.presentation {
            Presentation::Inline => {
                self.screen
                    .draw(|frame| render::draw_inline(frame, state, composer))?;
            }
            Presentation::Focus => {
                self.screen
                    .draw(|frame| render::draw_focus(frame, state, composer))?;
            }
        }
        Ok(())
    }
}

fn fresh_screen(presentation: Presentation) -> io::Result<Screen> {
    use ratatui::backend::CrosstermBackend;
    use ratatui::{Terminal, TerminalOptions, Viewport};
    let viewport = match presentation {
        Presentation::Inline => Viewport::Inline(terminal::INLINE_HEIGHT),
        Presentation::Focus => Viewport::Fullscreen,
    };
    Terminal::with_options(
        CrosstermBackend::new(io::stdout()),
        TerminalOptions { viewport },
    )
}
