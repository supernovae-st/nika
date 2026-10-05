// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The loop: one owner, one broker, one state, three presentations, one
//! conversation.
//!
//! Inline: every block the session finishes goes ABOVE the viewport through
//! `Terminal::insert_before`, into the terminal's own scrollback; the
//! viewport holds only the live area. Focus: the alternate screen shows the
//! transcript and the same live area; leaving it returns to the inline
//! viewport with the draft intact and the blocks finished meanwhile pushed
//! to the scrollback at that moment (they were never printed there).
//! Workspace: the alternate screen as the workspace ([`crate::workspace`]):
//! the project the conversation lends, the object in view, and the same
//! transcript and live area in the conversation panel. A door may open on it;
//! `Ctrl+T` opens it from inline when the terminal holds it (the focus view
//! otherwise); a resize below its minimum draws the focus view until the size
//! allows it again, the draft and the keyboard focus intact.
//!
//! Key precedence, first match wins: `Ctrl+C` (the interruption, in every
//! presentation and region), `Ctrl+T` (inline to full screen and back),
//! `Ctrl+L` (everything drawn again), then the full-screen presentation's own
//! keys (`decide`; the workspace's table is the crate-private
//! `workspace::desk`'s), then the composer. `Tab` stays the composer's
//! completion key everywhere.
//!
//! Opening a workflow in the workspace asks the conversation for one look
//! ([`Conversation::inspect`]) on this thread, never while drawing; a look
//! asked while a turn holds the conversation is taken when the turn ends, and
//! the opened workflow is looked at again after every turn, so the object
//! never stays on bytes a turn may have replaced. A look grants nothing.
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

use ratatui::Frame;

use crate::composer::{Composer, ComposerAction};
use crate::events::{Broker, Signal, UiEvent};
use crate::model::{
    Beat, Committed, Conversation, Handoff, Kind, Presentation, Turn, UiState, Waiting,
};
use crate::render;
use crate::terminal::{self, Owner, Screen};
use crate::visual::logomark;
use crate::workspace::desk::{self, Desk, Route};
use crate::workspace::object::Paint;
use crate::workspace::{conversation, project};

mod acquire;

/// How the shell runs.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Options {
    /// The presentation at start.
    pub presentation: Presentation,
    /// Colour allowed.
    pub color: bool,
    /// The ASCII glyph column (the caller's theme decision).
    pub ascii: bool,
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
            ascii: false,
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
    /// Words were typed into the composer while the current turn ran.
    typed_live: bool,
    /// The workspace's own state: the project the conversation last lent,
    /// the keyboard focus, the object in view and its look. Kept across
    /// presentations.
    desk: Desk,
    /// The composer's placeholder in effect: the workspace names the
    /// recipient there; the other presentations show none.
    placeholder: String,
}

/// What one key press decides, before anything is done about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyDecision {
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
fn decide(state: &UiState, desk: &mut Desk, key: KeyEvent) -> KeyDecision {
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
    let mut state = UiState::new(options.presentation, options.color, size);
    state.ascii = options.ascii;
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
        typed_live: false,
        desk: Desk::new(),
        placeholder: String::new(),
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
    /// Clear the screen and draw it whole (`Ctrl+L`).
    Repaint,
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
/// The busy row with the last completed phase kept beside the current one
/// (« ✓ understood 6 requirements · ● authoring »): what just finished and
/// what Nika does now, never a percentage.
fn busy_text_with(last_done: Option<&str>, base: Option<&str>, secs: u64, armed: bool) -> String {
    let current = base.unwrap_or("working");
    let joined;
    let base = match last_done {
        Some(done) if !current.starts_with(done) => {
            joined = format!("{done} · {current}");
            joined.as_str()
        }
        _ => current,
    };
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

/// How long the tail of a typing burst under way is still read as typed
/// ahead once a turn ends (keys in flight, over SSH too), before the
/// decision is painted.
const DEFUSE_WINDOW: std::time::Duration = std::time::Duration::from_millis(150);
/// One look at the input during [`DEFUSE_WINDOW`].
const DEFUSE_SLICE: std::time::Duration = std::time::Duration::from_millis(10);

/// What one event typed while Nika worked becomes once a decision is on
/// screen: it may fill the draft, never answer.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Defused {
    /// An edit of the draft: a character, a correction, a line break.
    Edit(KeyEvent),
    /// Pasted text, data for the draft.
    Text(String),
    /// A key that would send the draft or recall history: dropped.
    Drop,
    /// Everything else, handled as usual once the decision is on screen.
    Keep(UiEvent),
}

/// Whether a turn's beats leave the session waiting on the human (a
/// question, a proposal, a gate, a choice): the last wait they name is not
/// the free prompt.
fn ends_on_decision(beats: &[Beat]) -> bool {
    beats
        .iter()
        .rev()
        .find_map(|beat| match beat {
            Beat::Wait(waiting) => Some(*waiting != Waiting::Free),
            _ => None,
        })
        .unwrap_or(false)
}

/// Sort one typed-ahead event by the typeahead law.
fn defuse(event: UiEvent) -> Defused {
    let key = match event {
        UiEvent::Key(key) => key,
        UiEvent::Paste(text) => return Defused::Text(text),
        other => return Defused::Keep(other),
    };
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    match key.code {
        // A line break is an edit; a bare `Enter` would send.
        KeyCode::Enter if alt || shift || ctrl => Defused::Edit(key),
        KeyCode::Char('j') if ctrl => Defused::Edit(key),
        KeyCode::Enter | KeyCode::Up | KeyCode::Down | KeyCode::Tab => Defused::Drop,
        KeyCode::Char(_) if !ctrl && !alt => Defused::Edit(key),
        KeyCode::Backspace
        | KeyCode::Delete
        | KeyCode::Left
        | KeyCode::Right
        | KeyCode::Home
        | KeyCode::End => Defused::Edit(key),
        _ => Defused::Keep(UiEvent::Key(key)),
    }
}

/// Set typed-ahead events in `composer` by the typeahead law: edits and
/// pastes land in the draft, sending and recall are dropped. Returns the
/// events that keep their ordinary handling, in order.
fn set_aside(composer: &mut Composer, typed: Vec<UiEvent>) -> Vec<UiEvent> {
    let mut kept = Vec::new();
    for event in typed {
        match defuse(event) {
            Defused::Edit(key) => {
                // An edit key never submits: a bare `Enter` was dropped.
                let _ = composer.handle(key);
            }
            Defused::Text(text) => composer.paste(&text),
            Defused::Drop => {}
            Defused::Keep(event) => kept.push(event),
        }
    }
    kept
}

/// What the busy loop does after an event heard while a turn runs.
enum Heard {
    /// Leave now, with this exit.
    Leave(Exit),
    /// The draft, the hint or the scroll changed: draw.
    Redraw,
    /// Nothing to draw now.
    Nothing,
}

/// What a key does while Nika works.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Busy {
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
const ENTER_WAITS: &str = "Nika is working · Enter sends when it is your turn";

/// The hint row's words when `Esc` would leave the workspace while Nika works.
const LEAVE_WAITS: &str = "Nika is working · Esc leaves when it is your turn";

/// Sort a key typed while a turn runs: in the workspace the region that holds
/// the keyboard reads it first (only the view changes; a look it asks for is
/// taken when the turn ends), leaving waits for the turn, and what reaches
/// the composer follows [`during_turn`].
fn busy_key(state: &UiState, desk: &mut Desk, key: KeyEvent) -> Busy {
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

/// The notice when a spending question clears the draft typed while Nika
/// worked: that question takes only an answer typed after it shows.
fn cleared_notice(draft: &str, ascii: bool) -> String {
    let typed = typed_notice(draft, ascii);
    let kept = if ascii {
        " - it is in the box, not sent"
    } else {
        " · it is in the box, not sent"
    };
    let sep = if ascii { " - " } else { " · " };
    format!(
        "{}{sep}cleared: this cost question takes only an answer typed after it",
        typed.strip_suffix(kept).unwrap_or(&typed)
    )
}

/// The notice that says where the typeahead went: what is in the box (the
/// start of a long draft, on one line), and that it was not sent. The
/// renderer's own marks take their ASCII twins under `ascii`.
fn typed_notice(draft: &str, ascii: bool) -> String {
    const SHOWN: usize = 40;
    let words = draft.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut shown: String = words.chars().take(SHOWN).collect();
    let (open, close, sep, cut) = if ascii {
        ("\"", "\"", " - ", "...")
    } else {
        ("« ", " »", " · ", "…")
    };
    if words.chars().count() > SHOWN {
        shown.push_str(cut);
    }
    format!("you typed {open}{shown}{close} while Nika worked{sep}it is in the box, not sent")
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
                // The next draw writes every cell again.
                Step::Repaint => self.screen.clear()?,
                Step::Stay => {}
            }
            // The last blocks are drawn before the door closes: a result the
            // human never saw is not a result.
            self.draw()?;
            if self.state.quit {
                broker.stop();
                return Ok(Exit::Quit);
            }
            if let Some(exit) = self.acquire_wanted(&mut broker)? {
                broker.stop();
                return Ok(exit);
            }
        }
    }

    fn on_key(&mut self, key: KeyEvent, broker: &mut Broker) -> io::Result<Step> {
        if crate::scroll::end(&mut self.state, &self.desk, key) {
            return Ok(Step::Stay);
        }
        let decision = decide(&self.state, &mut self.desk, key);
        if decision == KeyDecision::Interrupt {
            return self.interrupt();
        }
        // Any other key keeps the session (« any key stays »).
        self.state.interrupt_armed = false;
        if decision == KeyDecision::Repaint {
            return Ok(Step::Repaint);
        }
        self.state.completion = None;
        match decision {
            KeyDecision::Present(to) => return Ok(Step::Switch(to)),
            KeyDecision::Route(Route::Leave) => return Ok(Step::Switch(Presentation::Inline)),
            KeyDecision::Route(Route::Older) => {
                crate::scroll::page(&mut self.state, &self.desk, &self.composer, true);
                return Ok(Step::Stay);
            }
            KeyDecision::Route(Route::Newer) => {
                crate::scroll::page(&mut self.state, &self.desk, &self.composer, false);
                return Ok(Step::Stay);
            }
            KeyDecision::Route(Route::Inspect) => {
                self.look();
                return Ok(Step::Stay);
            }
            // A focus, a selection, a scroll, a face or the object moved: the
            // loop draws after every event.
            KeyDecision::Route(_) => return Ok(Step::Stay),
            KeyDecision::Interrupt | KeyDecision::Repaint | KeyDecision::Compose => {}
        }
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
        if let Some(conversation) = self.conversation.as_mut() {
            let beats = conversation.cancel_pending();
            if !beats.is_empty() {
                self.apply_all(beats)?;
                return Ok(Step::Stay);
            }
        }
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
        // The turn is over: a hint about it (« Enter sends when it is your
        // turn ») goes with it, and a transcript scrolled back while Nika
        // worked returns to its end, where the answer is.
        self.state.completion = None;
        self.state.focus_scroll = 0;
        if started.elapsed() >= BELL_AFTER && !self.options.reduced_motion {
            // One bell: the human who looked away during a long turn is
            // called back; never for a short one, never under reduced motion.
            let mut out = io::stdout();
            out.write_all(b"\x07")?;
            out.flush()?;
        }
        // The typeahead law: what was typed while Nika worked is taken now,
        // before anything of the turn's answer is painted, so a key sent in
        // answer to what is on screen is never mistaken for it.
        let fresh = self.conversation()?.fresh_input_required();
        let typed = if !fresh && ends_on_decision(&turn.beats) {
            Some(self.take_typeahead(broker)?)
        } else {
            None
        };
        self.apply_all(turn.beats)?;
        if fresh {
            if let Some(exit) = self.fresh_input(broker)? {
                return Ok(Submitted::Left(exit));
            }
        } else if let Some(typed) = typed {
            // A decision is on screen: what was typed while Nika worked
            // fills the box and never answers it.
            self.set_aside_typeahead(typed)?;
        }
        if self.options.exit_after == Some(self.submitted) {
            self.state.quit = true;
        }
        Ok(Submitted::Handoff(turn.handoff))
    }

    /// Paint before accepting a fresh answer, retaining cancellation signals.
    fn fresh_input(&mut self, broker: &mut Broker) -> io::Result<Option<Exit>> {
        // A spending question takes only an answer typed after it shows:
        // what was typed while Nika worked is cleared, and a notice says so.
        let draft = self.composer.text();
        if !draft.trim().is_empty() {
            self.composer.clear();
            let notice = cleared_notice(&draft, self.state.ascii);
            self.state
                .transcript
                .push(Committed::new(Kind::Notice, notice));
            self.commit_inline()?;
        }
        // Paint the question before accepting input; reveal its reply prompt
        // only after the broker has discarded pre-question typeahead.
        let waiting = std::mem::replace(&mut self.state.waiting, Waiting::Free);
        self.draw()?;
        let buffered = broker.discard_typeahead();
        self.state.waiting = waiting;
        let buffered = buffered?;
        let mut cancel = false;
        for event in buffered {
            match event {
                UiEvent::Signal(Signal::Terminate) => {
                    return Ok(Some(Exit::Terminated));
                }
                UiEvent::Closed => return Ok(Some(Exit::Closed)),
                UiEvent::Signal(Signal::Interrupt) => cancel = true,
                UiEvent::Key(ref key) if is_ctrl_c(key) => cancel = true,
                UiEvent::Resize(_, _) => self.deferred.push_back(event),
                _ => {}
            }
        }
        if cancel {
            let beats = self.conversation()?.cancel_pending();
            self.apply_all(beats)?;
        }
        Ok(None)
    }

    /// The typeahead law, first half: everything typed while Nika worked
    /// and not yet read (the events set aside during the turn, and what the
    /// terminal already delivered), taken before anything of the turn's
    /// answer is painted; when a typing burst was under way, its tail that
    /// arrives in the next [`DEFUSE_WINDOW`] too, still before the paint. No
    /// key taken here can be a reply to the decision, and every key read
    /// after the paint is one.
    fn take_typeahead(&mut self, broker: &mut Broker) -> io::Result<Vec<UiEvent>> {
        let mut typed: Vec<UiEvent> = self.deferred.drain(..).collect();
        typed.extend(broker.discard_typeahead()?);
        let burst = self.typed_live
            || typed
                .iter()
                .any(|event| matches!(event, UiEvent::Key(_) | UiEvent::Paste(_)));
        if burst {
            let window = std::time::Instant::now();
            while window.elapsed() < DEFUSE_WINDOW {
                match broker.try_recv() {
                    Some(event) => typed.push(event),
                    None => std::thread::sleep(DEFUSE_SLICE),
                }
            }
        }
        Ok(typed)
    }

    /// The typeahead law, second half, once the turn ends on a decision (a
    /// question, a proposal, a gate, a choice): what was typed while Nika
    /// worked goes into the box and is never sent. Words and pastes land in
    /// the draft, `Enter` and history recall are dropped, and every other
    /// event keeps its ordinary handling (`Ctrl+C`, `SIGTERM`, a closed
    /// reader, a resize). One dim notice says what is in the box. The two
    /// spending questions keep their stricter discard ([`Self::fresh_input`]).
    fn set_aside_typeahead(&mut self, typed: Vec<UiEvent>) -> io::Result<()> {
        let kept = set_aside(&mut self.composer, typed);
        self.deferred.extend(kept);
        let draft = self.composer.text();
        if !draft.trim().is_empty() {
            let notice = typed_notice(&draft, self.state.ascii);
            self.state
                .transcript
                .push(Committed::new(Kind::Notice, notice));
            self.commit_inline()?;
        }
        Ok(())
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

    /// Take the look the opened workflow needs from the conversation, on this
    /// thread and never while drawing; while a turn holds the conversation it
    /// waits for the turn's end ([`Self::apply_all`]). What a run's face needs
    /// is acquired apart, on a worker ([`Self::acquire_wanted`]).
    fn look(&mut self) {
        let Some(path) = self.desk.opened_workflow().map(str::to_owned) else {
            self.desk.wants_look = false;
            return;
        };
        match self.conversation.as_mut() {
            Some(conversation) => {
                let look = conversation.inspect(&path);
                self.desk.took(&path, look);
            }
            None => self.desk.wants_look = true,
        }
    }

    fn apply_all(&mut self, beats: Vec<Beat>) -> io::Result<()> {
        // The project the conversation lends, read once per batch of beats
        // (the opening, a turn, a performed work, a cancellation), never
        // while drawing: a projection of what it already holds. The opened
        // workflow is looked at again: a turn may have replaced its bytes. The
        // candidate it proposes is the one it folded when the turn ended.
        if let Some(conversation) = self.conversation.as_ref() {
            self.desk.view = conversation.project();
            self.desk.proposed(conversation.candidate());
            self.desk.kept(conversation.kept_run());
        }
        if self.desk.wants_look || self.desk.opened_workflow().is_some() {
            self.look();
        }
        for beat in beats {
            let busy = matches!(beat, Beat::Busy(_));
            self.state.apply(beat);
            if busy {
                // A busy label is a state, not a block: draw it now so the
                // human sees work is active before the next beat lands.
                self.draw()?;
            }
        }
        self.refresh_title();
        self.commit_inline()
    }

    /// The terminal's title follows what waits: « nika · `<project>` · action
    /// required » while an answer, a consent, a choice or a gate waits on
    /// the human — persistent until resolved, never one transient bell.
    fn refresh_title(&self) {
        let Some(title) = self.options.title.as_deref() else {
            return;
        };
        let waits = !matches!(self.state.waiting, Waiting::Free);
        let full;
        let shown = if waits {
            full = format!("{title} · action required");
            full.as_str()
        } else {
            title
        };
        let _ = crate::terminal::set_title(shown);
    }

    /// Hand every block not yet in the scrollback to the terminal (inline
    /// only; the focus view reads the transcript itself).
    fn commit_inline(&mut self) -> io::Result<()> {
        if self.state.presentation != Presentation::Inline {
            return Ok(());
        }
        let width = self.state.size.0.max(1);
        let (color, ascii) = (self.state.color, self.state.ascii);
        let pending: Vec<Committed> = self.state.uncommitted().to_vec();
        for block in &pending {
            let rows = render::wrapped_rows(&render::block_lines(block, color, ascii), width);
            self.screen
                .insert_before(rows, |buf| render::render_block(block, color, ascii, buf))?;
        }
        self.state.committed_inline = self.state.transcript.len();
        Ok(())
    }

    fn switch(&mut self, to: Presentation) -> io::Result<()> {
        if to == self.state.presentation {
            return Ok(());
        }
        match to {
            Presentation::Focus | Presentation::Workspace => {
                if self.state.presentation == Presentation::Inline {
                    // The live area leaves with inline: erased, the cursor
                    // parked on its first row, which the alternate screen
                    // saves and gives back, so the viewport that returns is
                    // drawn where this one was, never below its ghost.
                    terminal::clear_inline(&mut self.screen)?;
                }
                terminal::set_alternate_screen(true)?;
                self.screen = fresh_screen(to)?;
                self.state.presentation = to;
                self.state.focus_scroll = 0;
                if to == Presentation::Workspace {
                    // The composer has the keys at every entry.
                    self.desk.enter();
                }
            }
            Presentation::Inline => {
                terminal::set_alternate_screen(false)?;
                self.screen = fresh_screen(Presentation::Inline)?;
                self.state.presentation = Presentation::Inline;
                self.name_recipient(String::new());
                self.commit_inline()?;
            }
        }
        Ok(())
    }

    /// Show `placeholder` in the empty composer (the workspace names the
    /// recipient of the next message there; the other presentations none).
    fn name_recipient(&mut self, placeholder: String) {
        if placeholder != self.placeholder {
            self.composer.set_placeholder(&placeholder);
            self.placeholder = placeholder;
        }
    }

    /// One turn on a worker thread (the conversation travels with it and
    /// comes back with the result); this thread keeps the terminal live:
    /// every busy label the turn emits is drawn as it arrives, the seconds
    /// count in the row, and an interruption is HEARD while the turn runs.
    /// A call to a seat cannot be recalled: one `Ctrl+C` warns (« again
    /// leaves now »), a second one or `SIGTERM` leaves at once with the
    /// terminal restored, the call left to die with the process. The
    /// composer stays usable meanwhile ([`Self::hear`]). A panic in the turn
    /// resumes here (the panic hook has restored the terminal).
    fn run_turn(&mut self, line: &str, broker: &mut Broker) -> io::Result<TurnEnd> {
        let (tx, rx) = mpsc::channel::<String>();
        let (done_tx, done_rx) = mpsc::channel::<(C, Turn)>();
        // What the shell observes of a run the turn drives: a bounded queue
        // drained each tick, its losses counted beside it (`session::feed`).
        let (seen_tx, seen_rx) = mpsc::sync_channel(crate::session::feed::QUEUE);
        let gap = std::sync::Arc::new(crate::session::feed::Gap::default());
        let seen = crate::session::feed::Seen::new(seen_tx, std::sync::Arc::clone(&gap));
        let mut conversation = self.conversation.take().ok_or_else(conversation_left)?;
        let line = line.to_owned();
        // The shell keeps one sender: the busy channel never disconnects,
        // so each wait below is one poll slice, never a spin.
        let _pace = tx.clone();
        let worker = std::thread::Builder::new()
            .name("nika-tui-turn".to_owned())
            .spawn(move || {
                let turn = conversation.submit_observed(&line, &tx, &seen);
                let _ = done_tx.send((conversation, turn));
            })?;
        self.typed_live = false;
        let started = std::time::Instant::now();
        let mut base = self.state.busy.clone();
        let mut last_done: Option<String> = None;
        let mut armed = false;
        let mut shown = u64::MAX;
        loop {
            if let Ok(label) = rx.recv_timeout(BUSY_POLL) {
                // A finished phase (the session's ✓ line) stays beside the
                // next current one; a current one replaces the previous.
                if label.starts_with("✓ ") {
                    last_done = Some(label);
                } else {
                    base = Some(label);
                }
                shown = u64::MAX;
            }
            if self.desk.observe(seen_rx.try_iter()) {
                shown = u64::MAX;
            }
            match done_rx.try_recv() {
                Ok((mut conversation, mut turn)) => {
                    // The last frames before the result, then what was lost.
                    self.desk.close_turn(&seen_rx, &gap);
                    if conversation.fresh_input_required()
                        && let Some(exit) =
                            self.fresh_end(&mut conversation, &mut turn, broker, armed)
                    {
                        return Ok(TurnEnd::Left(exit));
                    }
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
                match self.hear(event, &mut armed) {
                    Heard::Leave(exit) => return Ok(TurnEnd::Left(exit)),
                    Heard::Redraw => shown = u64::MAX,
                    Heard::Nothing => {}
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
                self.state.busy = Some(busy_text_with(
                    last_done.as_deref(),
                    base.as_deref(),
                    secs,
                    armed,
                ));
                self.draw()?;
            }
        }
    }

    /// A turn that ends on a fresh spending question: what was typed while it
    /// ran is read now, never as its answer; a termination or a closed reader
    /// leaves, an interruption (`armed` already, or heard now) cancels it.
    fn fresh_end(
        &mut self,
        conversation: &mut C,
        turn: &mut Turn,
        broker: &mut Broker,
        mut armed: bool,
    ) -> Option<Exit> {
        let buffered: Vec<_> = self
            .deferred
            .drain(..)
            .chain(std::iter::from_fn(|| broker.try_recv()))
            .collect();
        for event in buffered {
            match event {
                UiEvent::Signal(Signal::Terminate) => return Some(Exit::Terminated),
                UiEvent::Closed => return Some(Exit::Closed),
                UiEvent::Signal(Signal::Interrupt) => armed = true,
                UiEvent::Key(ref key) if is_ctrl_c(key) => armed = true,
                UiEvent::Resize(_, _) => self.deferred.push_back(event),
                _ => {}
            }
        }
        if armed {
            turn.beats = conversation.cancel_pending();
        }
        None
    }

    /// One event heard while a turn runs. An interruption acts now (the
    /// exit to leave with, once armed). The composer stays usable: words,
    /// pastes and edits land in the draft as they are typed, a bare `Enter`
    /// sends nothing and the hint row says when it will, and in a full
    /// screen the page keys scroll the transcript. Every other event waits
    /// for the turn. Once the turn ends on a decision, the typeahead law
    /// keeps the draft unsent ([`Self::set_aside_typeahead`]).
    fn hear(&mut self, event: UiEvent, armed: &mut bool) -> Heard {
        let key = match event {
            UiEvent::Signal(Signal::Terminate) => return Heard::Leave(Exit::Terminated),
            UiEvent::Closed => return Heard::Leave(Exit::Closed),
            UiEvent::Signal(Signal::Interrupt) => {
                return Self::arm(armed).map_or(Heard::Nothing, Heard::Leave);
            }
            UiEvent::Key(key) if is_ctrl_c(&key) => {
                return Self::arm(armed).map_or(Heard::Nothing, Heard::Leave);
            }
            UiEvent::Paste(text) => {
                self.state.completion = None;
                self.composer.paste(&text);
                self.typed_live = true;
                return Heard::Redraw;
            }
            UiEvent::Key(key) => key,
            UiEvent::Resize(cols, rows) if self.state.presentation != Presentation::Inline => {
                // A full screen reads its size without asking the terminal:
                // the frames drawn while the turn runs, and the keys the
                // workspace routes meanwhile, follow the new size at once.
                // The resize is still replayed once the turn ends.
                self.state.size = (cols, rows);
                self.deferred.push_back(event);
                return Heard::Redraw;
            }
            other => {
                self.deferred.push_back(other);
                return Heard::Nothing;
            }
        };
        if crate::scroll::end(&mut self.state, &self.desk, key) {
            return Heard::Redraw;
        }
        match busy_key(&self.state, &mut self.desk, key) {
            Busy::Edit => {
                self.state.completion = None;
                self.typed_live = true;
                if self.composer.handle(key) == ComposerAction::Complete
                    && let crate::composer::Completion::Several(list) =
                        self.composer.complete(&self.commands)
                {
                    self.state.completion = Some(list.join("  "));
                }
                Heard::Redraw
            }
            Busy::Hold => {
                self.state.completion = Some(ENTER_WAITS.to_owned());
                Heard::Redraw
            }
            Busy::Older => {
                crate::scroll::page(&mut self.state, &self.desk, &self.composer, true);
                Heard::Redraw
            }
            Busy::Newer => {
                crate::scroll::page(&mut self.state, &self.desk, &self.composer, false);
                Heard::Redraw
            }
            Busy::Later => {
                self.deferred.push_back(UiEvent::Key(key));
                Heard::Nothing
            }
            Busy::Leave => {
                self.state.completion = Some(LEAVE_WAITS.to_owned());
                self.deferred.push_back(UiEvent::Key(key));
                Heard::Redraw
            }
            Busy::Region => Heard::Redraw,
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
        if self.state.presentation != Presentation::Inline {
            // The frame takes the terminal's size as the backend reports it
            // (no query to the terminal): the state, the regions the keys
            // move over and the face rendered below take the same one.
            let size = self.screen.size()?;
            self.state.size = (size.width, size.height);
        }
        if self.state.presentation == Presentation::Workspace {
            let thread = project::thread(self.desk.view.as_ref(), None);
            self.name_recipient(conversation::placeholder(&thread));
            // The face in view is rendered here, before the frame, only when
            // it changed; the frame paints its lines.
            let (ascii, color) = (self.state.ascii, self.state.color);
            self.desk.prepare(self.state.size, ascii, color);
        }
        let (state, composer, desk) = (&self.state, &self.composer, &self.desk);
        self.screen
            .draw(|frame| draw_frame(frame, state, composer, desk))?;
        Ok(())
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

/// Draw one frame of the presentation in effect. Below the workspace's
/// minimum the focus view stands in, whole, until the size allows the
/// workspace again. The welcome's butterfly is drawn final at once: no cue
/// plays in this presentation.
fn draw_frame(frame: &mut Frame<'_>, state: &UiState, composer: &Composer, desk: &Desk) {
    match state.presentation {
        Presentation::Inline => render::draw_inline(frame, state, composer),
        Presentation::Focus => render::draw_focus(frame, state, composer),
        Presentation::Workspace => {
            let paint = Paint {
                ascii: state.ascii,
                color: state.color,
                elapsed: logomark::REVEAL_ENDS,
                reduced_motion: true,
            };
            if !desk::draw(frame, desk, paint, state, composer) {
                render::draw_focus(frame, state, composer);
            }
        }
    }
}

fn fresh_screen(presentation: Presentation) -> io::Result<Screen> {
    use ratatui::backend::CrosstermBackend;
    use ratatui::{Terminal, TerminalOptions, Viewport};
    let viewport = match presentation {
        Presentation::Inline => Viewport::Inline(terminal::INLINE_HEIGHT),
        Presentation::Focus | Presentation::Workspace => Viewport::Fullscreen,
    };
    Terminal::with_options(
        CrosstermBackend::new(io::stdout()),
        TerminalOptions { viewport },
    )
}

#[cfg(test)]
mod typeahead_tests {
    //! The typeahead law: `yes⏎` typed while Nika worked lands in the box
    //! once a decision is on screen, and is never sent.

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::{Defused, defuse, set_aside, typed_notice};
    use crate::composer::Composer;
    use crate::events::{Signal, UiEvent};

    fn key(code: KeyCode, modifiers: KeyModifiers) -> UiEvent {
        UiEvent::Key(KeyEvent::new(code, modifiers))
    }

    fn plain(code: KeyCode) -> UiEvent {
        key(code, KeyModifiers::NONE)
    }

    #[test]
    fn words_are_edits_sending_and_recall_are_dropped_the_rest_is_kept() {
        for edit in [
            plain(KeyCode::Char('y')),
            key(KeyCode::Char('Y'), KeyModifiers::SHIFT),
            plain(KeyCode::Backspace),
            plain(KeyCode::Left),
            key(KeyCode::Enter, KeyModifiers::ALT),
            key(KeyCode::Char('j'), KeyModifiers::CONTROL),
        ] {
            assert!(matches!(defuse(edit.clone()), Defused::Edit(_)), "{edit:?}");
        }
        for code in [KeyCode::Enter, KeyCode::Up, KeyCode::Down, KeyCode::Tab] {
            assert_eq!(defuse(plain(code)), Defused::Drop, "{code:?}");
        }
        assert_eq!(
            defuse(UiEvent::Paste("yes".to_owned())),
            Defused::Text("yes".to_owned())
        );
        for kept in [
            key(KeyCode::Char('c'), KeyModifiers::CONTROL),
            key(KeyCode::Char('t'), KeyModifiers::CONTROL),
            plain(KeyCode::Esc),
            plain(KeyCode::PageUp),
            UiEvent::Resize(80, 24),
            UiEvent::Signal(Signal::Terminate),
            UiEvent::Signal(Signal::Interrupt),
            UiEvent::Closed,
        ] {
            assert_eq!(defuse(kept.clone()), Defused::Keep(kept));
        }
    }

    /// `yes⏎` typed ahead is in the box, the `Enter` is gone, and the keys
    /// that keep their handling come back in order.
    #[test]
    fn yes_enter_typed_ahead_lands_in_the_box_and_is_never_sent() {
        let mut composer = Composer::new();
        let interrupt = key(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let typed = vec![
            plain(KeyCode::Char('y')),
            plain(KeyCode::Char('e')),
            plain(KeyCode::Char('s')),
            plain(KeyCode::Enter),
            UiEvent::Resize(100, 30),
            interrupt.clone(),
        ];
        let kept = set_aside(&mut composer, typed);
        assert_eq!(composer.text(), "yes");
        assert_eq!(kept, [UiEvent::Resize(100, 30), interrupt]);
        let mut pasted = Composer::new();
        let kept = set_aside(
            &mut pasted,
            vec![UiEvent::Paste("run it".to_owned()), plain(KeyCode::Enter)],
        );
        assert!(kept.is_empty());
        assert_eq!(pasted.text(), "run it");
        let mut corrected = Composer::new();
        set_aside(
            &mut corrected,
            vec![
                plain(KeyCode::Char('n')),
                plain(KeyCode::Char('o')),
                plain(KeyCode::Char('o')),
                plain(KeyCode::Backspace),
            ],
        );
        assert_eq!(
            corrected.text(),
            "no",
            "a correction typed ahead is kept too"
        );
    }

    /// A turn ends on a decision when the last wait its beats name is not
    /// the free prompt; a turn that names no wait (a handoff) does not.
    #[test]
    fn a_turn_ends_on_a_decision_when_its_last_wait_is_not_free() {
        use super::ends_on_decision;
        use crate::model::{Beat, Committed, Kind, Waiting};
        let say = Beat::Say(Committed::new(Kind::Reply, "x"));
        assert!(ends_on_decision(&[
            say.clone(),
            Beat::Wait(Waiting::Proposal)
        ]));
        assert!(ends_on_decision(&[
            Beat::Wait(Waiting::Free),
            Beat::Wait(Waiting::Gate)
        ]));
        assert!(ends_on_decision(&[Beat::Wait(Waiting::Choosing)]));
        assert!(!ends_on_decision(&[
            Beat::Wait(Waiting::Gate),
            Beat::Wait(Waiting::Free)
        ]));
        assert!(!ends_on_decision(&[say]));
        assert!(!ends_on_decision(&[]));
    }

    /// While Nika works the composer takes words and edits, a bare `Enter`
    /// is held, the page keys scroll only a full screen, and the rest waits.
    #[test]
    fn keys_typed_while_nika_works_edit_hold_scroll_or_wait() {
        use super::{Busy, during_turn};
        use crate::model::Presentation;
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

    #[test]
    fn a_cleared_draft_is_named_and_why() {
        assert_eq!(
            super::cleared_notice("yes", false),
            "you typed « yes » while Nika worked · cleared: this cost question takes only an answer typed after it"
        );
        assert!(super::cleared_notice("yes", true).is_ascii());
    }

    #[test]
    fn the_notice_says_what_is_in_the_box_in_the_glyph_column() {
        assert_eq!(
            typed_notice("yes", false),
            "you typed « yes » while Nika worked · it is in the box, not sent"
        );
        assert_eq!(
            typed_notice("yes", true),
            "you typed \"yes\" while Nika worked - it is in the box, not sent"
        );
        assert!(typed_notice("two\nlines", false).contains("« two lines »"));
        let long = typed_notice(&"word ".repeat(20), true);
        assert!(long.is_ascii() && long.contains("...\" while"), "{long}");
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    /// The busy row keeps the last completed phase beside the current one,
    /// and the armed row puts the second press first — never a percentage.
    #[test]
    fn the_busy_row_keeps_the_last_done_phase_beside_the_current_one() {
        assert_eq!(
            super::busy_text_with(
                Some("✓ understood 6 requirements"),
                Some("● authoring · openai/gpt-5.2"),
                0,
                false
            ),
            "✓ understood 6 requirements · ● authoring · openai/gpt-5.2"
        );
        assert_eq!(
            super::busy_text_with(None, Some("● checking"), 3, false),
            "● checking · 3s · Ctrl+C twice leaves"
        );
        assert!(
            super::busy_text_with(
                Some("✓ understood 2 requirements"),
                Some("● authoring"),
                1,
                true
            )
            .starts_with("Ctrl+C again leaves now")
        );
    }
}
