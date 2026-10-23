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
//! Key precedence, first match wins: the shell's own readers (the full
//! diagnostic while open, the palette's `Ctrl+O`, the diagnostic's `F2`, the
//! composer's command chooser), then `Ctrl+C` (the interruption, in every
//! presentation and region, never taken by a reader), `Ctrl+T` (inline to
//! full screen and back), `Ctrl+L` (everything drawn again), the full-screen
//! presentation's own keys (the workspace's table is the crate-private
//! `workspace::desk`'s), then the composer (`commands`). Choosing a command
//! inserts it into the draft and never sends it; `Tab` chooses.
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
use crate::visual::role::Depth;
use crate::workspace::desk::{self, Desk, Route};
use crate::workspace::object::Paint;

mod pointer;
use crate::workspace::{conversation, project};

mod acquire;
mod action_read;
mod commands;
mod idle;
mod opening;
mod progress;
mod pulse;
mod run_card;
mod said;
mod stop;
mod surface;
mod welcome;
mod worker;

use commands::{Busy, Caught, ENTER_WAITS, KeyDecision, LEAVE_WAITS, busy_key, decide};
use said::{Picked, Said};

/// How the shell runs.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
#[allow(clippy::struct_excessive_bools)] // four independent caller readings: colour, its 24-bit proof, the glyph column, motion
pub struct Options {
    /// The presentation at start.
    pub presentation: Presentation,
    /// Colour allowed.
    pub color: bool,
    /// The caller's reading of `COLORTERM`: the terminal proved 24-bit
    /// colour (`truecolor`, `24bit`). Without it the brand's tones take the
    /// nearest slot of the palette [`Options::term`] names (256 colours, else
    /// the 16 ANSI slots).
    pub truecolor: bool,
    /// The ASCII glyph column (the caller's theme decision).
    pub ascii: bool,
    /// Proof hook: panic after this many submitted lines.
    pub panic_after: Option<usize>,
    /// Proof hook: leave cleanly after this many submitted lines.
    pub exit_after: Option<usize>,
    /// The caller's reading of `TERM` (`dumb` refuses).
    pub term: Option<String>,
    /// Reduced motion (the caller's reading of `NIKA_REDUCED_MOTION`): a
    /// still busy mark with measured seconds and current work facts retained,
    /// no bell; the welcome mark is final at once. Idle stays silent.
    pub reduced_motion: bool,
    /// The terminal's title while the door is open (`nika · <project>`);
    /// `None` leaves the title alone.
    pub title: Option<String>,
    /// How often an idle shell asks its conversation what changed without a
    /// turn ([`Conversation::idle`]: a schedule's trigger that fired while the
    /// door stood open): about a minute. `None` never asks.
    pub idle_every: Option<std::time::Duration>,
}

impl Options {
    /// Inline, no colour, no proof hook.
    #[must_use]
    pub fn new(presentation: Presentation) -> Self {
        Self {
            presentation,
            color: false,
            truecolor: false,
            ascii: false,
            panic_after: None,
            exit_after: None,
            term: None,
            reduced_motion: false,
            title: None,
            idle_every: Some(idle::EVERY),
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
    /// The composer's placeholder in effect: the workspace invites the work
    /// and the commands there; the other presentations show none.
    placeholder: String,
    /// One reveal per shell, never an idle animation.
    welcome: welcome::Reveal,
    /// The running turn's stop and the correction queued during it.
    hold: stop::Hold,
    /// The full diagnostic, while open over the frame.
    diagnostic: Option<commands::diagnostic::Diagnostic>,
    /// The region that held the keys when the palette opened: cancelling the
    /// palette, or a view key chosen in it, gives the keys back there.
    palette_from: Option<crate::workspace::focus::Region>,
    /// One surface activation, one act: its repeated `Enter` is taken.
    guard: surface::Guard,
    /// What the last turn's live line observed, for the line saying how it ended.
    pulse: Option<pulse::Pulse>,
    /// The run whose end card was said, so a story committed again adds none.
    carded: Option<nika_display::run_story::ExecutionId>,
    /// What the runs this session observed spent, metered, in USD.
    run_spent: f64,
    /// A decision a turn just put on screen has not been drawn yet: no key
    /// is read against it before its first frame ([`Self::set_aside_unseen`]).
    unpainted: bool,
    /// When the idle shell next asks the conversation what changed.
    idle: idle::Idle,
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
/// The conversation refused to open, or a draw failed.
pub fn run_on<C: Conversation + 'static>(
    taken: Taken,
    mut conversation: C,
    options: Options,
) -> io::Result<Exit> {
    let Taken { owner, screen } = taken;
    // The words the conversation builds for the screen take the glyph column in use, and its
    // instants the machine's clock, where the person reads them.
    conversation.glyphs(options.ascii);
    let zone = jiff::tz::TimeZone::system();
    conversation.clock(&zone);
    let size = crossterm::terminal::size().unwrap_or((80, 24));
    let mut state = UiState::new(options.presentation, options.color, size);
    state.ascii = options.ascii;
    state.depth = Depth::of(options.color, options.truecolor, options.term.as_deref());
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
        welcome: welcome::Reveal::default(),
        hold: stop::Hold::default(),
        diagnostic: None,
        palette_from: None,
        guard: surface::Guard::default(),
        pulse: None,
        carded: None,
        run_spent: 0.0,
        unpainted: false,
        idle: idle::Idle::default(),
    };
    // Runs are named on the machine's clock, where the person reads them.
    shell.desk.zone = zone;
    // The layout this conversation kept from an earlier session, if any.
    if let Some(conversation) = shell.conversation.as_ref()
        && let Some(kept) = conversation.arrangement()
    {
        shell.desk.arrange(kept);
    }
    if let Some(title) = shell.options.title.as_deref() {
        // The previous title rides the terminal's stack; the restore pops it.
        let _ = crate::terminal::set_title(title);
    }
    let broker = Broker::start();
    let outcome = shell.drive(broker);
    shell.owner.restore()?;
    outcome
}

/// The line that brings the welcome back ([`Shell::welcome_again`]): this screen's own command,
/// listed beside the conversation's.
pub(crate) const WELCOME: &str = "/welcome";

/// The commands the palette and the slash list offer: the conversation's, and this screen's
/// own [`WELCOME`] before `/quit`.
fn offered_commands(mut commands: Vec<String>) -> Vec<String> {
    if !commands.iter().any(|command| command == WELCOME) {
        let at = (commands.iter()).position(|command| command == "/quit");
        commands.insert(at.unwrap_or(commands.len()), WELCOME.to_owned());
    }
    commands
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

/// The Session's own words of what the turn does: the last completed phase
/// kept beside the current one (« ✓ understood 6 requirements · ● authoring
/// »), what just finished and what Nika does now, never a percentage. The
/// live line ([`pulse`]) reads them after its facts.
fn phase_words(last_done: Option<&str>, base: Option<&str>) -> String {
    match (last_done, base) {
        (Some(done), Some(current)) if !current.starts_with(done) => {
            format!("{done} · {current}")
        }
        (_, Some(current)) => current.to_owned(),
        (Some(done), None) => done.to_owned(),
        (None, None) => String::new(),
    }
}

/// The busy row once a first `Ctrl+C` was heard: a call to a seat cannot be
/// recalled, so what a second press does comes FIRST (the row must say it
/// inside an 80-column terminal), then the turn's own words and seconds.
fn armed_text(phases: &str, secs: u64, voice: crate::lang::Voice) -> String {
    let base = if phases.is_empty() {
        voice.say("working")
    } else {
        phases
    };
    (voice.say(ARMED_TURN))
        .replace("{secs}", &secs.to_string())
        .replace("{base}", base)
}

/// The live line once one `Ctrl+C` armed the exit while a call was under way:
/// `{base}` the Session's phase words (or `working`), `{secs}` its seconds.
const ARMED_TURN: &str = "Ctrl+C again leaves now · the call cannot be recalled · {base} · {secs}s";

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
/// the free prompt, or an answer was not taken (its question still waits).
fn ends_on_decision(beats: &[Beat]) -> bool {
    beats.iter().any(|beat| matches!(beat, Beat::NotTaken(_)))
        || beats
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
        // Pointer coordinates from the previous frame never act on a fresh review.
        UiEvent::Mouse(_) => return Defused::Drop,
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

/// The words and pastes among `typed` as one text, by the typeahead law (edits land, `Enter`
/// and history recall are dropped), and the events that keep their ordinary handling
/// (signals, `Ctrl+C`, a closed reader, a resize). A fresh spending question never takes
/// this text as its answer, and nothing typed is dropped unseen: it is kept whole in the
/// conversation instead.
fn typed_text(typed: Vec<UiEvent>) -> (String, Vec<UiEvent>) {
    let mut scratch = Composer::new();
    let kept = set_aside(&mut scratch, typed);
    (scratch.text(), kept)
}

/// What the busy loop does after an event heard while a turn runs.
enum Heard {
    /// Leave now, with this exit.
    Leave(Exit),
    /// The draft, the hint or the scroll changed: draw.
    Redraw,
    /// The terminal may have discarded cells, even if its size returned.
    Repaint,
    /// Nothing to draw now.
    Nothing,
}

/// The notice when a spending question clears an existing draft: that question
/// takes only an answer typed after it shows. Said in `voice`, the draft whole.
fn cleared_notice(draft: &str, voice: crate::lang::Voice) -> String {
    let sep = if voice.ascii { " - " } else { " · " };
    let quoted = nika_display::activity_card::quoted(draft, voice.ascii);
    let phrase = voice.say(
        "your draft {draft}{sep}cleared: this cost question takes only an answer typed after it{sep}kept in this conversation, whole:\n{text}",
    );
    crate::lang::fill(
        phrase,
        &[("{sep}", sep), ("{draft}", &quoted), ("{text}", draft)],
    )
}

/// The notice that names the retained draft, without inferring when it was typed:
/// what is in the box (the start of a long draft, on one line), and that it was
/// not sent, said in `voice`. The renderer's own marks take their ASCII twins
/// under ASCII.
fn typed_notice(draft: &str, voice: crate::lang::Voice) -> String {
    let sep = if voice.ascii { " - " } else { " · " };
    let quoted = nika_display::activity_card::quoted(draft, voice.ascii);
    let phrase = voice.say("your draft {draft}{sep}it is in the box, not sent");
    crate::lang::fill(phrase, &[("{sep}", sep), ("{draft}", &quoted)])
}

impl<C: Conversation + 'static> Shell<C> {
    fn conversation(&mut self) -> io::Result<&mut C> {
        self.conversation.as_mut().ok_or_else(conversation_left)
    }

    fn drive(&mut self, mut broker: Broker) -> io::Result<Exit> {
        self.commands = offered_commands(self.conversation()?.commands());
        let opening = self.conversation()?.open();
        if let Some(refusal) = opening::refusal(&opening) {
            broker.stop();
            return Err(refusal);
        }
        self.apply_all(opening)?;
        self.draw()?;
        let mut batched = 0;
        loop {
            let Some(event) = self.next_event(&mut broker)? else {
                broker.stop();
                return Ok(Exit::Closed);
            };
            let typed = matches!(event, UiEvent::Key(_) | UiEvent::Paste(_));
            let step = self.step_of(event, &mut broker)?;
            // Keys already waiting are read before the next frame, a bounded batch at a
            // time: typing never waits on one draw per key, and the frame shows the last
            // state. Anything else, a key that did more than edit, or a reading the run's
            // faces want (which holds the keys that follow) draws first.
            if typed
                && matches!(step, Step::Stay)
                && !self.state.quit
                && batched < BATCH
                && self.desk.wanted().is_none()
                && !self.unpainted
                && let Some(next) = broker.try_recv()
            {
                batched += 1;
                self.deferred.push_back(next);
                continue;
            }
            batched = 0;
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
                Step::Repaint => self.repaint(&broker)?,
                Step::Stay => {}
            }
            // A decision a turn just put on screen is drawn before any key is
            // read for it: what already waits was typed before it showed.
            if std::mem::take(&mut self.unpainted) {
                self.set_aside_unseen(&mut broker)?;
            }
            // An idle checkpoint: a layout the desk settled is kept.
            self.keep_arrangement()?;
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

    /// What one event asks of the loop: a key by its precedence, a pointer, a
    /// paste, a resize, a focus change, a signal or the reader's end.
    fn step_of(&mut self, event: UiEvent, broker: &mut Broker) -> io::Result<Step> {
        Ok(match event {
            UiEvent::Key(key) => self.on_key(key, broker)?,
            UiEvent::Mouse(mouse) => {
                if let Some(said) = self.submit_pointed(mouse) {
                    self.send_activated(said, broker)?
                } else {
                    self.on_mouse(mouse);
                    Step::Stay
                }
            }
            UiEvent::Paste(text) => {
                self.paste(&text);
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
                if self.state.presentation == Presentation::Inline {
                    Step::Stay
                } else {
                    Step::Repaint
                }
            }
            UiEvent::FocusGained | UiEvent::FocusLost => Step::Stay,
            UiEvent::Signal(Signal::Terminate) => Step::Leave(Exit::Terminated),
            UiEvent::Signal(Signal::Interrupt) => self.interrupt()?,
            UiEvent::Closed => Step::Leave(Exit::Closed),
        })
    }

    fn on_key(&mut self, key: KeyEvent, broker: &mut Broker) -> io::Result<Step> {
        if self.repeated_enter(&key) {
            return Ok(Step::Stay);
        }
        match self.catch(key) {
            Some(Caught::Read) => Ok(Step::Stay),
            // A view key the palette chose, pressed past the chooser.
            Some(Caught::Press(chosen)) => self.route_key(chosen, broker),
            // What the typed choice holding the line sent, bound to it: a bare `Enter`'s
            // activation takes its repeat, while `Esc` stepping back to the list above takes
            // none, so the `Enter` pressed next on that list is a new act.
            Some(Caught::Sent(said)) if key.code == KeyCode::Enter => {
                self.send_activated(said, broker)
            }
            Some(Caught::Sent(said)) => self.send_stepped(said, broker),
            Some(Caught::SetAside) => self.set_aside(),
            None => self.route_key(key, broker),
        }
    }

    /// One key by the ordinary precedence ([`decide`]).
    fn route_key(&mut self, key: KeyEvent, broker: &mut Broker) -> io::Result<Step> {
        // A surface hides the transcript: `End` never moves it under the band.
        let hidden = render::surface::open(&self.state, &self.composer);
        if !hidden && crate::scroll::end(&mut self.state, &self.desk, key) {
            return Ok(Step::Stay);
        }
        if !hidden && crate::scroll::home(&mut self.state, &self.desk, &self.composer, key) {
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
            // `/new` as a line of its own, whatever the draft holds: the draft stays.
            KeyDecision::New => return self.send(Said::Line("/new".to_owned()), broker),
            KeyDecision::Interrupt | KeyDecision::Repaint | KeyDecision::Compose => {}
        }
        // A key that brought the keys back to the conversation reaches its composer focused.
        self.composer.set_focused(self.composer_has_keys());
        // A typed choice's offers read first; `Tab` reaches the composer only
        // where the chooser has nothing to offer.
        let said = match self.pick(key) {
            Picked::Sent(said) => said,
            Picked::Moved => return Ok(Step::Stay),
            Picked::SetAside => return self.set_aside(),
            Picked::Pass => match self.composer.handle(key) {
                ComposerAction::Submit(line) => self.said(line),
                ComposerAction::Complete | ComposerAction::Edited | ComposerAction::Ignored => {
                    return Ok(Step::Stay);
                }
            },
        };
        self.send(said, broker)
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

    /// One line sent: echo it, let the conversation answer, and report the
    /// handoff it asks for, if any ([`Self::submit`] sends what it queued).
    /// A typed question's answer takes the same path to its own door
    /// ([`Said`]); the echo says what was sent, never that it applied.
    fn submit_one(&mut self, said: &Said, broker: &mut Broker) -> io::Result<Submitted> {
        // The welcome again is this screen's, never a line for the Session.
        if matches!(said, Said::Line(line) if line.trim() == WELCOME) {
            self.welcome_again()?;
            return Ok(Submitted::Handoff(None));
        }
        let candidate = self.conversation()?.candidate();
        let read = action_read::Request::capture(said, &self.state.waiting, candidate.as_ref());
        // A command beside a decision or a question: its answer reads above the band.
        let command = (read.is_none())
            .then(|| action_read::Command::capture(said, &self.state.waiting))
            .flatten();
        self.submitted += 1;
        let line = said.text();
        // A line sent from a scrolled transcript (a queued correction) keeps the reading position.
        if let Some(echo) = said.echo(&self.state.waiting, self.state.voice()) {
            self.keep_reading(|state, _| state.transcript.push(Committed::new(Kind::Human, echo)));
            self.commit_inline()?;
        }
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
            self.set_busy(label);
            self.draw()?;
        }
        let spent_before = self.conversation()?.spend();
        let started = std::time::Instant::now();
        let mut turn = match self.run_turn(said, broker)? {
            TurnEnd::Done(turn) => turn,
            TurnEnd::Left(exit) => return Ok(Submitted::Left(exit)),
        };
        crate::scroll::preserve_reading(&mut self.state, &self.desk, &self.composer, |state| {
            state.busy = None;
            state.settle_activity(started.elapsed());
            state.completion = None;
        });
        // A stop that raced the result: what the stopped preparation left is withdrawn first.
        let (stopped, queued) = (self.hold.stopping(), self.hold.release());
        if stopped {
            turn.beats.extend(self.conversation()?.withdraw_stopped());
        }
        // Completing a turn keeps the reader's position. The live decision
        // remains visible; End returns the transcript to the new answer.
        self.bell_after(started.elapsed())?;
        // The typeahead law: what was typed while Nika worked is taken now,
        // before anything of the turn's answer is painted, so a key sent in
        // answer to what is on screen is never mistaken for it.
        let fresh = self.conversation()?.fresh_input_required();
        let typed = if !fresh && ends_on_decision(&turn.beats) {
            Some(self.take_typeahead(broker)?)
        } else {
            None
        };
        let detail = if stopped {
            None
        } else {
            let candidate = self.conversation()?.candidate();
            read.and_then(|read| read.completed(&turn, candidate.as_ref()))
        };
        let answered = (command.filter(|_| !stopped)).and_then(|command| command.completed(&turn));
        let unsent = self.answered(&turn.beats);
        self.apply_all(turn.beats)?;
        self.say_prepared(started.elapsed(), spent_before, stopped)?;
        if fresh {
            if let Some(exit) = self.fresh_input(broker)? {
                return Ok(Submitted::Left(exit));
            }
        } else if let Some(typed) = typed {
            // A decision is on screen: what was typed while Nika worked
            // fills the box and never answers it, nor what arrives before
            // the decision's first frame.
            self.set_aside_typeahead(typed)?;
            self.unpainted = true;
        }
        // An answer not taken: its exact words first, what was typed after.
        self.give_back(unsent, fresh);
        self.show_read(detail, answered);
        if self.options.exit_after == Some(self.submitted) {
            self.state.quit = true;
        }
        self.next_correction(queued, fresh, turn.handoff.is_some())?;
        // The turn returned the conversation: a layout settled meanwhile is kept.
        self.keep_arrangement()?;
        Ok(Submitted::Handoff(turn.handoff))
    }

    /// An explicitly selected read-only act opens its own returned words above the menu, and so
    /// does a command's answer beside the same choice, or the hint says it had none.
    fn show_read(
        &mut self,
        detail: Option<commands::diagnostic::Diagnostic>,
        answered: Option<action_read::Shown>,
    ) {
        match (detail, answered) {
            (Some(detail), _) | (None, Some(action_read::Shown::Read(detail))) => {
                self.diagnostic = Some(detail);
            }
            (None, Some(action_read::Shown::Nothing(words))) => {
                if self.state.completion.is_none() {
                    self.state.completion = Some(render::own(&words, self.state.ascii));
                }
            }
            (None, None) => {}
        }
    }

    /// One bell after a turn that took `took`: the human who looked away during a long turn is
    /// called back; never for a short one, never under reduced motion.
    fn bell_after(&self, took: std::time::Duration) -> io::Result<()> {
        if took >= BELL_AFTER && !self.options.reduced_motion {
            let mut out = io::stdout();
            out.write_all(b"\x07")?;
            out.flush()?;
        }
        Ok(())
    }

    /// Keep the layout the desk settled (once per settled change, never
    /// mid-drag), only while this thread holds the conversation; what
    /// keeping it says is applied like any beat. No I/O while drawing.
    fn keep_arrangement(&mut self) -> io::Result<()> {
        let Some(conversation) = self.conversation.as_mut() else {
            return Ok(());
        };
        let Some(settled) = self.desk.take_settled() else {
            return Ok(());
        };
        let beats = conversation.keep_arrangement(settled);
        self.apply_all(beats)
    }

    /// Paint before accepting a fresh answer, retaining cancellation signals.
    fn fresh_input(&mut self, broker: &mut Broker) -> io::Result<Option<Exit>> {
        // A spending question takes only an answer typed after it shows:
        // what was typed while Nika worked is cleared, and a notice says so.
        let draft = self.keep_reading(|_, composer| composer.discard_before_question());
        if !draft.trim().is_empty() {
            let notice = cleared_notice(&draft, self.state.voice());
            self.keep_reading(|state, _| {
                state.transcript.push(Committed::new(Kind::Notice, notice));
            });
            self.commit_inline()?;
        }
        // Paint the question before accepting input; reveal its reply prompt
        // only after the broker has discarded pre-question typeahead.
        let waiting = std::mem::replace(&mut self.state.waiting, Waiting::Free);
        self.draw()?;
        let buffered = broker.discard_typeahead();
        self.state.waiting = waiting;
        // What was typed until the question showed is never its answer, and is never dropped
        // unseen either: it joins the conversation whole, like the draft above.
        let (late, buffered) = typed_text(buffered?);
        if !late.trim().is_empty() {
            let notice = cleared_notice(&late, self.state.voice());
            self.keep_reading(|state, _| {
                state.transcript.push(Committed::new(Kind::Notice, notice));
            });
            self.commit_inline()?;
        }
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

    /// The typeahead law up to a decision's first frame: what arrived after the
    /// turn's own drain ([`Self::take_typeahead`]) and waits unread when that
    /// frame is about to be drawn was typed before the decision showed. It
    /// joins the draft as what was typed while Nika worked does, and never
    /// answers the decision; every key read after the frame is one.
    fn set_aside_unseen(&mut self, broker: &mut Broker) -> io::Result<()> {
        let unseen: Vec<UiEvent> = std::iter::from_fn(|| broker.try_recv()).collect();
        if unseen.is_empty() {
            return Ok(());
        }
        self.set_aside_typeahead(unseen)
    }

    /// The typeahead law, second half, once the turn ends on a decision (a
    /// question, a proposal, a gate, a choice): what was typed while Nika
    /// worked goes into the box and is never sent. Words and pastes land in
    /// the draft, `Enter` and history recall are dropped, and every other
    /// event keeps its ordinary handling (`Ctrl+C`, `SIGTERM`, a closed
    /// reader, a resize). One dim notice says what is in the box. The two
    /// spending questions keep their stricter discard ([`Self::fresh_input`]).
    fn set_aside_typeahead(&mut self, typed: Vec<UiEvent>) -> io::Result<()> {
        // A scrolled transcript keeps its reading position: the draft's rows
        // and the notice land below it.
        let kept = self.keep_reading(|state, composer| {
            let kept = set_aside(composer, typed);
            let draft = composer.text();
            if !draft.trim().is_empty() {
                let notice = typed_notice(&draft, state.voice());
                state.transcript.push(Committed::new(Kind::Notice, notice));
            }
            kept
        });
        self.deferred.extend(kept);
        self.commit_inline()
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
        // A batch is the conversation's news of now: the idle ask waits again.
        self.idle
            .rearm(std::time::Instant::now(), self.options.idle_every);
        // The project the conversation lends, read once per batch of beats
        // (the opening, a turn, a performed work, a cancellation), never
        // while drawing: a projection of what it already holds. The opened
        // workflow is looked at again: a turn may have replaced its bytes. The
        // candidate it proposes is the one it folded when the turn ended.
        if let Some(conversation) = self.conversation.as_ref() {
            self.desk.view = conversation.project();
            self.desk.proposed(conversation.candidate());
            self.desk.kept(conversation.kept_run());
            // The commands it answers now (`/restore` comes and goes).
            self.commands = offered_commands(conversation.commands());
            // The language the person writes, which the TUI's own words follow.
            self.state.lang = conversation.language().unwrap_or_default();
            self.composer.speak(self.state.voice());
            self.desk.lang = self.state.lang;
        }
        if self.desk.wants_look || self.desk.opened_workflow().is_some() {
            self.look();
        }
        for mut beat in beats {
            let busy = matches!(beat, Beat::Busy(_));
            let story = matches!(&beat, Beat::Say(block) if block.kind == Kind::Run);
            // A run's story folds once, at its commit, under the leg observed whole.
            if let Beat::Say(block) = &mut beat {
                block.run = self.desk.folded().filter(|_| block.kind == Kind::Run);
            }
            if let Beat::Wait(waiting) = &beat {
                // A typed choice's selection follows the question it belongs to.
                self.composer.follow(waiting);
            }
            crate::scroll::preserve_reading(&mut self.state, &self.desk, &self.composer, |state| {
                state.apply(beat);
            });
            if story {
                // How the run that story tells ended, right below it.
                self.card_run();
            }
            if busy {
                // A busy label is a state, not a block: draw it now so the
                // human sees work is active before the next beat lands.
                self.draw()?;
            }
        }
        // What the session spent so far, once this batch's run card counted.
        self.spend_so_far();
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
            let lines = render::block_lines_within(block, color, ascii, width);
            render::pages(&lines, width, self.state.size.1.max(1), |page| {
                let rows = u16::try_from(page.len()).unwrap_or(self.state.size.1.max(1));
                self.screen
                    .insert_before(rows, |buf| render::paint_page(page, buf))
            })?;
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

    /// Show `placeholder` in the empty composer (the workspace invites the
    /// work and the commands there; the other presentations show none).
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
    /// terminal restored, the call left to die with the process; a turn
    /// that ends between the two presses keeps the warning
    /// ([`Self::end_turn`]). The composer stays usable meanwhile
    /// ([`Self::hear`]). A panic in the turn resumes here (the panic hook
    /// has restored the terminal).
    fn run_turn(&mut self, said: &Said, broker: &mut Broker) -> io::Result<TurnEnd> {
        let (tx, rx) = mpsc::channel::<String>();
        let (done_tx, done_rx) = mpsc::channel::<(C, Turn)>();
        // What the shell observes of a run the turn drives: a bounded queue
        // drained each tick, its losses counted beside it (`session::feed`).
        let (seen_tx, seen_rx) = mpsc::sync_channel(crate::session::feed::QUEUE);
        let gap = std::sync::Arc::new(crate::session::feed::Gap::default());
        let seen = crate::session::feed::Seen::new(seen_tx, std::sync::Arc::clone(&gap));
        self.follow_language(said);
        let mut conversation = self.conversation.take().ok_or_else(conversation_left)?;
        self.hold.arm(conversation.stopper());
        let said = said.clone();
        // The shell keeps one sender: the busy channel never disconnects,
        // so each wait below is one poll slice, never a spin.
        let _pace = tx.clone();
        let worker = worker::turn().spawn(move || {
            // A line to whatever waits, or a typed answer by its identity.
            let turn = said.send(&mut conversation, &tx, &seen);
            let _ = done_tx.send((conversation, turn));
        })?;
        self.typed_live = false;
        let started = std::time::Instant::now();
        let mut pulse = pulse::Pulse::new(started).speaking(self.state.voice());
        let mut base = self.state.busy.clone();
        let mut last_done: Option<String> = None;
        let mut armed = false;
        let mut shown = u64::MAX;
        loop {
            if let Ok(label) = rx.recv_timeout(BUSY_POLL) {
                crate::scroll::preserve_reading(
                    &mut self.state,
                    &self.desk,
                    &self.composer,
                    |state| state.observe_activity(&label),
                );
                // A finished phase (the session's ✓ line) stays beside the
                // next current one; a current one replaces the previous.
                if label.starts_with("✓ ") {
                    last_done = Some(label);
                } else {
                    base = Some(label);
                }
                shown = u64::MAX;
            }
            if self.drain(&seen_rx, started, (&mut base, &mut last_done), &mut pulse) {
                shown = u64::MAX;
            }
            match done_rx.try_recv() {
                Ok((conversation, turn)) => {
                    // The last frames before the result, then what was lost.
                    self.drain(&seen_rx, started, (&mut base, &mut last_done), &mut pulse);
                    self.desk.close_turn(&seen_rx, &gap);
                    self.pulse = Some(pulse);
                    return Ok(self.end_turn(conversation, turn, broker, armed));
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
                match self.hear_first(event, &mut armed) {
                    Heard::Leave(exit) => return Ok(TurnEnd::Left(exit)),
                    Heard::Redraw => shown = u64::MAX,
                    Heard::Repaint => {
                        self.repaint(broker)?;
                        shown = u64::MAX;
                    }
                    Heard::Nothing => {}
                }
            }
            if armed != was_armed {
                shown = u64::MAX;
            }
            let phases = (last_done.as_deref(), base.as_deref());
            self.busy_frame(started, phases, (armed, &mut pulse), &mut shown)?;
        }
    }

    /// A line at the free prompt opens a turn of the Session, which first reads the person's
    /// language in it ([`nika_session::Lang::of`], its own rule): the TUI's words follow from
    /// this turn's first frame, and its end reads the Session's ([`Self::apply_all`]).
    fn follow_language(&mut self, said: &Said) {
        if let Said::Line(line) = said
            && self.state.waiting == Waiting::Free
            && (self.conversation.as_ref()).is_some_and(|c| c.language().is_some())
            && let Some(lang) = nika_session::Lang::of(line)
        {
            self.state.lang = lang;
        }
    }

    /// The busy line of a turn running since `started`, drawn when its second or its spinner
    /// frame changed since the one `shown`: the phases it reported, or, once a stop is armed,
    /// what stopping does.
    fn busy_frame(
        &mut self,
        started: std::time::Instant,
        (last_done, base): (Option<&str>, Option<&str>),
        (armed, pulse): (bool, &mut pulse::Pulse),
        shown: &mut u64,
    ) -> io::Result<()> {
        let elapsed = started.elapsed();
        let secs = elapsed.as_secs();
        let frame = (!self.options.reduced_motion).then(|| spinner_frame(elapsed));
        if secs == *shown && frame == self.state.spinner {
            return Ok(());
        }
        *shown = secs;
        self.state.spinner = frame;
        let phases = phase_words(last_done, base);
        let line = if armed {
            armed_text(&phases, secs, self.state.voice())
        } else {
            let leg = self.desk.live.as_ref();
            pulse.line(
                std::time::Instant::now(),
                leg,
                self.hold.can_stop(),
                &phases,
            )
        };
        self.set_busy(line);
        self.draw()
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
        // Words and pastes typed while the turn ran join the draft, never the question's
        // answer: the question's fresh input sets that whole draft aside in the conversation.
        let buffered = self.keep_reading(|_, composer| set_aside(composer, buffered));
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

    /// The turn's result is back. A fresh spending question reads what was
    /// typed meanwhile ([`Self::fresh_end`]); a press that stopped the
    /// preparation never cancels it. Any other end keeps a press heard while
    /// the turn ran, one that stopped the preparation included: the busy row
    /// said « Ctrl+C again leaves », so the idle row says a second press
    /// leaves and it does (any other key disarms, as at an idle prompt). A
    /// correction queued meanwhile is the next line: it keeps no press.
    fn end_turn(
        &mut self,
        mut conversation: C,
        mut turn: Turn,
        broker: &mut Broker,
        armed: bool,
    ) -> TurnEnd {
        if conversation.fresh_input_required() {
            let armed = armed && !self.hold.stopping();
            if let Some(exit) = self.fresh_end(&mut conversation, &mut turn, broker, armed) {
                return TurnEnd::Left(exit);
            }
        } else if armed && !self.hold.queued() {
            self.state.interrupt_armed = true;
        }
        self.conversation = Some(conversation);
        TurnEnd::Done(turn)
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
            // Invite a new request only while this line has no pending decision.
            let invitation = if matches!(self.state.waiting, Waiting::Free) {
                let voice = self.state.voice();
                render::own(voice.say(conversation::invitation(false)), voice.ascii)
            } else {
                String::new()
            };
            self.name_recipient(invitation);
            // The face in view is rendered here, before the frame, only when
            // it changed; the frame paints its lines.
            let (ascii, color) = (self.state.ascii, self.state.color);
            self.desk.prepare_for(&self.state, ascii, color);
        }
        self.sync_choices();
        // Where the keys are when the composer does not hold them, said on its hint row.
        self.state.keys_on = (!self.composer_has_keys())
            .then_some(self.desk.focus.region)
            .and_then(|region| match region {
                crate::workspace::focus::Region::Aside => Some("the project list"),
                crate::workspace::focus::Region::Object => Some("the object in view"),
                crate::workspace::focus::Region::Conversation => None,
            });
        let paint = self.welcome_paint();
        let (state, composer, desk) = (&self.state, &self.composer, &self.desk);
        let diagnostic = self.diagnostic.as_mut();
        self.screen.draw(|frame| {
            draw_frame(frame, state, composer, desk, paint, diagnostic);
        })?;
        Ok(())
    }
}

/// How long the shell waits for a busy label before looking whether the
/// turn finished.
const BUSY_POLL: std::time::Duration = std::time::Duration::from_millis(50);

/// The most waiting keys read before a frame is drawn anyway.
const BATCH: usize = 64;

/// The loader's frame for an elapsed time: one of the ten braille frames,
/// the next every 100 ms (a turn's motion, never a percentage).
fn spinner_frame(elapsed: std::time::Duration) -> u8 {
    u8::try_from((elapsed.as_millis() / 100) % render::SPINNER.len() as u128).unwrap_or(0)
}

/// Draw one frame of the presentation in effect. Below the workspace's
/// minimum the focus view stands in, whole, until the size allows the
/// workspace again. The full diagnostic, while open, covers the frame. The
/// shell supplies the welcome clock; painting reads none.
fn draw_frame(
    frame: &mut Frame<'_>,
    state: &UiState,
    composer: &Composer,
    desk: &Desk,
    paint: Paint,
    diagnostic: Option<&mut commands::diagnostic::Diagnostic>,
) {
    match state.presentation {
        Presentation::Inline => render::draw_inline(frame, state, composer),
        Presentation::Focus => render::draw_focus(frame, state, composer),
        Presentation::Workspace => {
            if !desk::draw(frame, desk, paint, state, composer) {
                render::draw_focus(frame, state, composer);
            }
        }
    }
    if let Some(diagnostic) = diagnostic {
        let area = frame.area();
        diagnostic.fit((area.width, area.height));
        diagnostic.render(frame, area, state.voice(), state.color);
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

    use super::{Defused, defuse, set_aside, typed_notice, typed_text};
    use crate::composer::Composer;
    use crate::events::{Signal, UiEvent};

    fn key(code: KeyCode, modifiers: KeyModifiers) -> UiEvent {
        UiEvent::Key(KeyEvent::new(code, modifiers))
    }

    fn plain(code: KeyCode) -> UiEvent {
        key(code, KeyModifiers::NONE)
    }

    /// Before a cost question showed, the typed tail still in the reader was dropped unseen;
    /// it is now read as text (never sent, `Enter` dropped) and the signals keep their
    /// handling, so the question's notice can keep it whole.
    #[test]
    fn typeahead_before_a_cost_question_is_kept_as_text_never_sent() {
        let (text, kept) = typed_text(vec![
            plain(KeyCode::Char('y')),
            plain(KeyCode::Char('e')),
            plain(KeyCode::Char('s')),
            plain(KeyCode::Enter),
            UiEvent::Paste(" and run it".to_owned()),
            key(KeyCode::Char('c'), KeyModifiers::CONTROL),
            UiEvent::Signal(Signal::Interrupt),
            UiEvent::Resize(80, 24),
        ]);
        assert_eq!(text, "yes and run it");
        assert!(
            matches!(kept.as_slice(), [
                UiEvent::Key(k),
                UiEvent::Signal(Signal::Interrupt),
                UiEvent::Resize(80, 24)
            ] if k.code == KeyCode::Char('c')),
            "{kept:?}"
        );
        let (empty, none) = typed_text(vec![plain(KeyCode::Enter), plain(KeyCode::Up)]);
        assert!(empty.is_empty() && none.is_empty());
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
        assert!(!ends_on_decision(std::slice::from_ref(&say)));
        assert!(!ends_on_decision(&[]));
        // An answer not taken: its words return to the box, so a key typed
        // while it worked never sends them, whatever wait follows (or none).
        let refused = Beat::NotTaken("eur".to_owned());
        assert!(ends_on_decision(&[say, refused.clone()]));
        assert!(ends_on_decision(&[refused, Beat::Wait(Waiting::Free)]));
    }

    /// The English voice in the glyph column `ascii`.
    const fn english(ascii: bool) -> crate::lang::Voice {
        crate::lang::Voice {
            lang: crate::lang::Lang::En,
            ascii,
        }
    }

    /// The draft notices in French: the draft quoted and kept whole as typed
    /// (a brace it holds included), the notice's own words French, folded
    /// under ASCII.
    #[test]
    fn the_draft_notices_speak_french_and_keep_the_draft() {
        let french = crate::lang::Voice {
            lang: crate::lang::Lang::Fr,
            ascii: false,
        };
        assert_eq!(
            typed_notice("oui {text}", french),
            "votre brouillon « oui {text} » · il est dans la zone de saisie, non envoyé"
        );
        assert_eq!(
            super::cleared_notice("oui {draft}", french),
            "votre brouillon « oui {draft} » · effacé : cette question de coût ne prend qu'une réponse tapée après elle · gardé dans cette conversation, en entier :\noui {draft}"
        );
        let plain = typed_notice(
            "oui",
            crate::lang::Voice {
                ascii: true,
                ..french
            },
        );
        assert_eq!(
            plain,
            "votre brouillon \"oui\" - il est dans la zone de saisie, non envoye"
        );
    }

    #[test]
    fn a_cleared_draft_is_named_and_why() {
        assert_eq!(
            super::cleared_notice("yes", english(false)),
            "your draft « yes » · cleared: this cost question takes only an answer typed after it · kept in this conversation, whole:\nyes"
        );
        let ascii = super::cleared_notice("yes", english(true));
        assert_eq!(
            ascii,
            "your draft \"yes\" - cleared: this cost question takes only an answer typed after it - kept in this conversation, whole:\nyes"
        );
        assert!(ascii.is_ascii());
    }

    #[test]
    fn the_notice_says_what_is_in_the_box_in_the_glyph_column() {
        assert_eq!(
            typed_notice("yes", english(false)),
            "your draft « yes » · it is in the box, not sent"
        );
        assert_eq!(
            typed_notice("yes", english(true)),
            "your draft \"yes\" - it is in the box, not sent"
        );
        assert!(typed_notice("two\nlines", english(false)).contains("« two lines »"));
        let long = typed_notice(&"word ".repeat(20), english(true));
        assert!(
            long.is_ascii() && long.starts_with("your draft \""),
            "{long}"
        );
        assert!(
            long.contains("...\" - it is in the box, not sent"),
            "{long}"
        );
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    /// The phase words keep the last completed phase beside the current one,
    /// and the armed row puts the second press first — never a percentage.
    #[test]
    fn the_busy_row_keeps_the_last_done_phase_beside_the_current_one() {
        let phases = super::phase_words(
            Some("✓ understood 6 requirements"),
            Some("● authoring · openai/gpt-5.2"),
        );
        assert_eq!(
            phases,
            "✓ understood 6 requirements · ● authoring · openai/gpt-5.2"
        );
        assert_eq!(super::phase_words(None, Some("● checking")), "● checking");
        assert_eq!(super::phase_words(Some("✓ read"), None), "✓ read");
        assert_eq!(super::phase_words(None, None), "");
        let english = crate::lang::Voice::default();
        assert_eq!(
            super::armed_text(&phases, 1, english),
            "Ctrl+C again leaves now · the call cannot be recalled · ✓ understood 6 requirements · ● authoring · openai/gpt-5.2 · 1s"
        );
        assert!(super::armed_text("", 4, english).ends_with("· working · 4s"));
        // In French the shell's words change; the Session's phase words stay as said.
        let french = crate::lang::Voice {
            lang: crate::lang::Lang::Fr,
            ascii: false,
        };
        assert_eq!(
            super::armed_text(&phases, 1, french),
            "Ctrl+C encore quitte maintenant · l'appel ne peut pas être rappelé · ✓ understood 6 requirements · ● authoring · openai/gpt-5.2 · 1s"
        );
        assert!(super::armed_text("", 4, french).ends_with("· travail en cours · 4s"));
    }
}
