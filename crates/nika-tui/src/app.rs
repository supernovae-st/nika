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

use std::collections::VecDeque;
use std::io::{self, Write as _};
use std::sync::mpsc;

use crossterm::cursor::MoveTo;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{Clear, ClearType};

use crate::composer::{Composer, ComposerAction};
use crate::events::{Broker, Signal, UiEvent};
use crate::model::{Beat, Committed, Conversation, Handoff, Kind, Presentation, Turn, UiState};
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
    /// Reduced motion (the caller's reading of `NIKA_REDUCED_MOTION`): the
    /// busy row changes only when the turn says something new — no
    /// seconds tick, no bell.
    pub reduced_motion: bool,
    /// The terminal's title while the door is open (`nika · <project>`);
    /// `None` leaves the title alone.
    pub title: Option<String>,
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
            reduced_motion: false,
            title: None,
        }
    }
}

/// A turn longer than this ends with one bell (the survey's threshold for
/// « a tool over five seconds »): the human who looked away is called back.
const BELL_AFTER: std::time::Duration = std::time::Duration::from_secs(5);

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
    /// `None` only while a turn runs on its worker thread, or after an
    /// abandoned turn (the door leaves then).
    conversation: Option<C>,
    options: Options,
    submitted: usize,
    /// Events read while a turn ran that were not an interruption: keys
    /// typed ahead, a paste, a resize — replayed once the turn ends.
    deferred: VecDeque<UiEvent>,
    /// The conversation's slash commands, for `Tab`.
    commands: Vec<String>,
}

/// A terminal the renderer holds, between [`enter`] and [`run_on`].
/// Dropping it restores the terminal.
#[derive(Debug)]
#[non_exhaustive]
pub struct Taken {
    owner: Owner,
    screen: Screen,
}

/// Take the terminal for the renderer: the probe (a TTY, not `TERM=dumb`),
/// the modes, and the inline viewport's anchor (a cursor-position report
/// the terminal must answer). The caller decides what a refusal becomes —
/// the plain session is the same session, never a dead door.
///
/// # Errors
///
/// Not a terminal, `TERM=dumb`, or a terminal that never answered the
/// cursor report; every mode enabled before the failure is restored.
pub fn enter(options: &Options) -> io::Result<Taken> {
    terminal::install_panic_hook();
    let (owner, screen) = terminal::enter(options.presentation, options.term.as_deref())?;
    Ok(Taken { owner, screen })
}

/// Run the shell over a conversation until it ends. The terminal is
/// restored before this returns, on every path.
///
/// # Errors
///
/// The terminal could not be taken (not a TTY) or a draw failed.
pub fn run<C: Conversation + 'static>(conversation: C, options: Options) -> io::Result<Exit> {
    let taken = enter(&options)?;
    run_on(taken, conversation, options)
}

/// Run the shell on a terminal already taken by [`enter`]. The terminal
/// is restored before this returns, on every path.
///
/// # Errors
///
/// A draw failed.
pub fn run_on<C: Conversation + 'static>(
    taken: Taken,
    conversation: C,
    options: Options,
) -> io::Result<Exit> {
    let Taken { owner, screen } = taken;
    let size = crossterm::terminal::size().unwrap_or((80, 24));
    let state = UiState::new(options.presentation, options.color, size);
    let mut shell = Shell {
        owner,
        screen,
        state,
        composer: Composer::new(),
        conversation: Some(conversation),
        options,
        submitted: 0,
        deferred: VecDeque::new(),
        commands: Vec::new(),
    };
    if let Some(title) = shell.options.title.as_deref() {
        // The previous title rides the terminal's stack; the restore pops it.
        let _ = crate::terminal::set_title(title);
    }
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

/// How a turn ended: with its beats, or abandoned by an interruption
/// heard while it ran (the door leaves at once).
enum TurnEnd {
    Done(Turn),
    Left(Exit),
}

/// What a submitted line came to.
enum Submitted {
    Handoff(Option<Handoff>),
    Left(Exit),
}

fn conversation_left() -> io::Error {
    io::Error::other("the conversation left with an abandoned turn")
}

fn is_ctrl_c(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c')
}

/// The busy row while a turn runs: the turn's own label, the seconds
/// once they count, and what an interruption does — a call to a seat
/// cannot be recalled, so one `Ctrl+C` warns and a second one leaves.
fn busy_text(base: Option<&str>, secs: u64, armed: bool) -> String {
    let base = base.unwrap_or("working");
    if armed {
        // What a second press does comes FIRST: the row must say it inside
        // an 80-column terminal, whatever the turn's own label is.
        format!("Ctrl+C again leaves now · the call cannot be recalled · {base} · {secs}s")
    } else if secs >= 2 {
        format!("{base} · {secs}s · Ctrl+C twice leaves")
    } else {
        base.to_owned()
    }
}

impl<C: Conversation + 'static> Shell<C> {
    fn conversation(&mut self) -> io::Result<&mut C> {
        self.conversation.as_mut().ok_or_else(conversation_left)
    }

    fn drive(&mut self, mut broker: Broker) -> io::Result<Exit> {
        self.commands = self.conversation()?.commands();
        let opening = self.conversation()?.open();
        self.apply_all(opening)?;
        self.draw()?;
        loop {
            let Some(event) = self.deferred.pop_front().or_else(|| broker.recv()) else {
                broker.stop();
                return Ok(Exit::Closed);
            };
            let step = match event {
                UiEvent::Key(key) => self.on_key(key, &mut broker)?,
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

    fn on_key(&mut self, key: KeyEvent, broker: &mut Broker) -> io::Result<Step> {
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
        self.state.completion = None;
        match self.composer.handle(key) {
            ComposerAction::Submit(line) => match self.submit(&line, broker)? {
                Submitted::Left(exit) => return Ok(Step::Leave(exit)),
                Submitted::Handoff(Some(handoff)) => return Ok(Step::Handoff(handoff)),
                Submitted::Handoff(None) => {}
            },
            ComposerAction::Complete => {
                if let crate::composer::Completion::Several(list) =
                    self.composer.complete(&self.commands)
                {
                    self.state.completion = Some(list.join("  "));
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
    fn submit(&mut self, line: &str, broker: &mut Broker) -> io::Result<Submitted> {
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
        // The busy state is drawn BEFORE the turn runs, with the
        // conversation's own name for the work; the turn then runs on a
        // worker thread while this thread draws every truthful label the
        // turn emits (« Working through this workflow… »). The turn's first
        // word clears the busy state.
        if let Some(label) = self.conversation()?.busy_label(line) {
            self.state.busy = Some(label);
            self.draw()?;
        }
        let started = std::time::Instant::now();
        let turn = match self.run_turn(line, broker)? {
            TurnEnd::Done(turn) => turn,
            TurnEnd::Left(exit) => return Ok(Submitted::Left(exit)),
        };
        self.state.busy = None;
        if started.elapsed() >= BELL_AFTER && !self.options.reduced_motion {
            // One bell: the human who looked away during a long turn is
            // called back; never for a short one, never under reduced motion.
            let mut out = io::stdout();
            out.write_all(b"\x07")?;
            out.flush()?;
        }
        self.apply_all(turn.beats)?;
        if self.options.exit_after == Some(self.submitted) {
            self.state.quit = true;
        }
        Ok(Submitted::Handoff(turn.handoff))
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
        let beats = self.conversation()?.perform(handoff);
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

    /// One turn on a worker thread (the conversation travels with it and
    /// comes back with the result); this thread keeps the terminal live:
    /// every busy label the turn emits is drawn as it arrives, the seconds
    /// count in the row, and an interruption is HEARD while the turn runs.
    /// A call to a seat cannot be recalled: one `Ctrl+C` warns (« again
    /// leaves now »), a second one or `SIGTERM` leaves at once with the
    /// terminal restored, the call left to die with the process. Keys
    /// typed ahead wait for the turn. A panic in the turn resumes here
    /// (the panic hook has restored the terminal).
    fn run_turn(&mut self, line: &str, broker: &mut Broker) -> io::Result<TurnEnd> {
        let (tx, rx) = mpsc::channel::<String>();
        let (done_tx, done_rx) = mpsc::channel::<(C, Turn)>();
        let mut conversation = self.conversation.take().ok_or_else(conversation_left)?;
        let line = line.to_owned();
        // The shell keeps one sender: the busy channel never disconnects,
        // so each wait below is one poll slice, never a spin.
        let _pace = tx.clone();
        let worker = std::thread::Builder::new()
            .name("nika-tui-turn".to_owned())
            .spawn(move || {
                let turn = conversation.submit_with(&line, &tx);
                let _ = done_tx.send((conversation, turn));
            })?;
        let started = std::time::Instant::now();
        let mut base = self.state.busy.clone();
        let mut armed = false;
        let mut shown = u64::MAX;
        loop {
            if let Ok(label) = rx.recv_timeout(BUSY_POLL) {
                base = Some(label);
                shown = u64::MAX;
            }
            match done_rx.try_recv() {
                Ok((conversation, turn)) => {
                    self.conversation = Some(conversation);
                    return Ok(TurnEnd::Done(turn));
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    return match worker.join() {
                        Ok(()) => Err(io::Error::other("the turn ended without a result")),
                        Err(panic) => std::panic::resume_unwind(panic),
                    };
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
            let was_armed = armed;
            while let Some(event) = broker.try_recv() {
                if let Some(exit) = self.hear(event, &mut armed) {
                    return Ok(TurnEnd::Left(exit));
                }
            }
            if armed != was_armed {
                shown = u64::MAX;
            }
            let (secs, frame) = if self.options.reduced_motion {
                (0, None)
            } else {
                let elapsed = started.elapsed();
                (elapsed.as_secs(), Some(spinner_frame(elapsed)))
            };
            if secs != shown || frame != self.state.spinner {
                shown = secs;
                self.state.spinner = frame;
                self.state.busy = Some(busy_text(base.as_deref(), secs, armed));
                self.draw()?;
            }
        }
    }

    /// One event heard while a turn runs: an interruption acts now (the
    /// exit to leave with, once armed), anything else waits for the turn.
    fn hear(&mut self, event: UiEvent, armed: &mut bool) -> Option<Exit> {
        match event {
            UiEvent::Signal(Signal::Terminate) => Some(Exit::Terminated),
            UiEvent::Closed => Some(Exit::Closed),
            UiEvent::Signal(Signal::Interrupt) => Self::arm(armed),
            UiEvent::Key(key) if is_ctrl_c(&key) => Self::arm(armed),
            other => {
                self.deferred.push_back(other);
                None
            }
        }
    }

    fn arm(armed: &mut bool) -> Option<Exit> {
        if *armed {
            return Some(Exit::Interrupted);
        }
        // The caller's next tick draws the row from the turn's own label:
        // it says what a second press does.
        *armed = true;
        None
    }

    fn draw(&mut self) -> io::Result<()> {
        draw_parts(&mut self.screen, &self.state, &self.composer)
    }
}

/// How long the shell waits for a busy label before looking whether the
/// turn finished.
const BUSY_POLL: std::time::Duration = std::time::Duration::from_millis(50);

/// The loader's frame for an elapsed time: one of the ten braille frames,
/// the next every 100 ms (a turn's motion, never a percentage).
fn spinner_frame(elapsed: std::time::Duration) -> u8 {
    u8::try_from((elapsed.as_millis() / 100) % render::SPINNER.len() as u128).unwrap_or(0)
}

/// Draw the live area from the state (both presentations).
fn draw_parts(screen: &mut Screen, state: &UiState, composer: &Composer) -> io::Result<()> {
    match state.presentation {
        Presentation::Inline => {
            screen.draw(|frame| render::draw_inline(frame, state, composer))?;
        }
        Presentation::Focus => {
            screen.draw(|frame| render::draw_focus(frame, state, composer))?;
        }
    }
    Ok(())
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
