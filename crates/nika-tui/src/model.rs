// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The presentation state and the vocabulary the session speaks to it.
//!
//! A [`Beat`] is one typed thing the session did in a turn: it said a block
//! (a reply, a proposal preview, a run line, a result), it now waits for a
//! particular kind of line (the prompt names which: `nika ›` · `reply ›` ·
//! `Save? ›` · `answer ›`), or it is busy under a seat. The shapes mirror
//! `nika_session::runtime::TurnOutcome` one to one so the adapter of the next
//! wave is a match, never a parse. The UX-1 fixture ([`Script`]) emits the
//! same beats from a canned conversation so both presentations are judged
//! on identical input.

use nika_display::activity_card::{ActivityCard, Ending, Update};

/// Where the session is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Presentation {
    /// The default: an inline viewport at the bottom of the terminal, finished
    /// blocks committed into the terminal's own scrollback.
    Inline,
    /// On request: the alternate screen with the transcript scrollable.
    Focus,
    /// The alternate screen as the workspace ([`crate::workspace`]): the
    /// project the conversation lends in its header and aside, the object in
    /// view, the conversation beside or below it. Below
    /// [`crate::workspace::geometry::MIN_SIZE`] it draws the focus view
    /// instead, and comes back whole when the size allows.
    Workspace,
}

impl Presentation {
    /// The other presentation when the terminal's size is unknown: inline
    /// leads to the focus view, a full screen back to inline.
    #[must_use]
    pub fn toggled(self) -> Self {
        match self {
            Self::Inline => Self::Focus,
            Self::Focus | Self::Workspace => Self::Inline,
        }
    }

    /// What `Ctrl+T` opens on a terminal of `size` (columns, rows): from
    /// inline the workspace when it fits, the focus view otherwise; from
    /// either full screen, inline.
    #[must_use]
    pub fn toggled_at(self, size: (u16, u16)) -> Self {
        match self {
            Self::Inline if crate::workspace::geometry::fits(size) => Self::Workspace,
            other => other.toggled(),
        }
    }
}

/// What kind of block a committed line belongs to: the glyph and the role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Kind {
    /// The opening banner (seat, project, how to leave).
    Banner,
    /// A line the human typed, echoed into the transcript.
    Human,
    /// A reply, a fact, a help text.
    Reply,
    /// An authoring question the compiler asked.
    Question,
    /// A candidate's review (run order, boundary, identity).
    Proposal,
    /// A check report or a run's announcement.
    Report,
    /// One task line of a run.
    Run,
    /// A human gate inside a run.
    Gate,
    /// The result: what was produced, where, then the proof scope.
    Result,
    /// A refusal, named.
    Refusal,
    /// A notice of the shell itself (interrupted, mode switched).
    Notice,
    /// The workspace's one card of what the Session reported while a turn
    /// worked: kept up to date while the turn runs, settled when it ends.
    Activity,
}

/// One finished block: it may span several lines.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Committed {
    /// The block's kind.
    pub kind: Kind,
    /// The text, `\n`-separated lines.
    pub text: String,
}

impl Committed {
    /// A block of one kind.
    #[must_use]
    pub fn new(kind: Kind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
        }
    }
}

/// What the session waits for: the prompt names it, and a line typed under
/// one prompt never crosses to another (a `yes` under `reply ›` is an answer,
/// never a consent).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Waiting {
    /// A new intent or a command.
    #[default]
    Free,
    /// A choice on a numbered screen.
    Choosing,
    /// An authoring question by key.
    Question {
        /// The question's key (`model` · `const.rule_expression` …).
        key: String,
    },
    /// Consent on the exact bytes of a candidate.
    Proposal,
    /// A human gate inside a run.
    Gate,
}

impl Waiting {
    /// The prompt naming the same waiting state as the plain loop.
    #[must_use]
    pub fn prompt(&self) -> &'static str {
        match self {
            Self::Free => "nika › ",
            Self::Choosing => "› ",
            Self::Question { .. } => "reply › ",
            Self::Proposal => "Save? › ",
            Self::Gate => "answer › ",
        }
    }

    /// The one-line hint under the composer for this state. A spending
    /// question (the session's `unknown_cost` and `run_cost` keys) names its
    /// three choices in words, never by colour alone.
    #[must_use]
    pub fn hint(&self) -> &'static str {
        match self {
            Self::Free => "describe work · /help · Run: run <file>.nika",
            Self::Choosing => "1 account · 2 API · 3 local · 4 no AI · cancel",
            Self::Question { key } if key == "unknown_cost" || key == "run_cost" => {
                "yes approves once · no or Ctrl+C cancels · details shows the full evidence"
            }
            Self::Question { .. } => "answer the question above · cancel to stop",
            Self::Proposal => "yes + Enter: Save · no: cancel · /show: inspect",
            Self::Gate => "approve or refuse · nothing else answers a gate",
        }
    }
}

/// One typed thing a turn produced.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Beat {
    /// A finished block.
    Say(Committed),
    /// What the session waits for now.
    Wait(Waiting),
    /// Work is active under a seat; the label is the session's own line,
    /// never a percentage.
    Busy(String),
    /// Where the automation stands, in the session's own words (« Ready for
    /// review · … », « Saved · checked · not active · nothing has run »):
    /// the status row, replaced at every turn, never a block.
    Status(String),
    /// Where the automation stands as separate facts (« Draft ✓ · Saved ✓
    /// · Checked ✓ · Active ○ · Run ○ »): the lifecycle rail on the row
    /// above the status, replaced at every turn — declared is never active.
    Rail(String),
    /// The Session stopped the turn's preparation at the human's request, in
    /// its own words: the activity card says so, and no proposal came of it.
    Cancelled(String),
    /// The session closed the door.
    Quit,
}

/// The whole presentation state, derived from beats; the renderer reads it
/// and never writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
#[allow(clippy::struct_excessive_bools)] // two theme decisions and two loop flags, each independent
pub struct UiState {
    /// Where the session is drawn.
    pub presentation: Presentation,
    /// Every committed block, in order (the focus view scrolls it; the inline
    /// view has already handed them to the terminal).
    pub transcript: Vec<Committed>,
    /// How many transcript blocks the inline view has committed to scrollback.
    pub committed_inline: usize,
    /// What the session waits for.
    pub waiting: Waiting,
    /// The busy label, when work is active.
    pub busy: Option<String>,
    /// The loader's frame beside the busy label (`None`: still — reduced
    /// motion, or no turn under way).
    pub spinner: Option<u8>,
    /// Where the automation stands (the session's status line); empty when
    /// nothing is under way.
    pub status: String,
    /// The lifecycle rail (empty until the session reports one).
    pub rail: String,
    /// A first `Ctrl+C` was pressed: the next one leaves.
    pub interrupt_armed: bool,
    /// Candidates a `Tab` left for the hint row, until the next key.
    pub completion: Option<String>,
    /// Colour allowed (the theme's decision, never the renderer's).
    pub color: bool,
    /// The ASCII glyph column (the theme's decision: `--ascii`, CI logs, a
    /// legacy console): the renderer's own glyphs take their twins.
    pub ascii: bool,
    /// Scroll offset of the full-screen transcript, in rendered rows from the end.
    pub focus_scroll: usize,
    /// Terminal size as last reported.
    pub size: (u16, u16),
    /// The session asked to close.
    pub quit: bool,
    activity: Option<Activity>,
}

impl UiState {
    /// A fresh state for one presentation.
    #[must_use]
    pub fn new(presentation: Presentation, color: bool, size: (u16, u16)) -> Self {
        Self {
            presentation,
            transcript: Vec::new(),
            committed_inline: 0,
            waiting: Waiting::Free,
            busy: None,
            spinner: None,
            status: String::new(),
            rail: String::new(),
            interrupt_armed: false,
            completion: None,
            color,
            ascii: false,
            focus_scroll: 0,
            size,
            quit: false,
            activity: None,
        }
    }

    /// Apply one beat.
    pub fn apply(&mut self, beat: Beat) {
        match beat {
            Beat::Say(block) => {
                self.busy = None;
                // A run's own task lines repeat the steps its live card observed: the card
                // gives way (unless already in scrollback), so each step reads once, after
                // the run's check and announcement. Any other block only settles the card.
                if block.kind == Kind::Run {
                    if let Some(mut card) = self.activity.take() {
                        if card.index >= self.committed_inline {
                            self.transcript.remove(card.index);
                        } else {
                            card.card.settle(Ending::Completed, None);
                            card.paint(&mut self.transcript, self.ascii);
                        }
                    }
                } else if block.kind == Kind::Refusal {
                    self.settle_card(Ending::Failed, None);
                } else {
                    self.settle_card(Ending::Completed, None);
                }
                self.transcript.push(block);
            }
            Beat::Wait(waiting) => {
                self.settle_card(Ending::Completed, None);
                self.activity = None;
                self.busy = None;
                self.waiting = waiting;
            }
            Beat::Cancelled(note) => {
                self.busy = None;
                self.settle_card(Ending::Stopped, None);
                self.transcript.push(Committed::new(Kind::Notice, note));
            }
            Beat::Busy(label) => self.busy = Some(label),
            Beat::Status(line) => self.status = line,
            Beat::Rail(line) => self.rail = line,
            Beat::Quit => self.quit = true,
        }
    }

    /// A line said while the turn works (a run's story), kept as a step.
    pub(crate) fn observe_activity(&mut self, label: &str) {
        if !label.trim().is_empty() {
            self.observe(Update::Step(label));
        }
    }

    /// Retain what the Session reported, typed, in one compact card
    /// ([`ActivityCard`]). The workspace keeps the card; inline and focus keep
    /// their transcript behavior.
    pub(crate) fn observe(&mut self, update: Update<'_>) {
        if self.presentation != Presentation::Workspace {
            return;
        }
        // A settled card stays as it was; a later update opens a new one below.
        if self
            .activity
            .as_ref()
            .is_some_and(|card| !card.card.is_live())
        {
            self.activity = None;
        }
        let activity = self.activity.get_or_insert_with(|| {
            let index = self.transcript.len();
            self.transcript.push(Committed::new(Kind::Activity, ""));
            Activity {
                index,
                card: ActivityCard::new(),
            }
        });
        if activity.card.observe(update) {
            activity.paint(&mut self.transcript, self.ascii);
        }
    }

    /// The turn that fed the live card ended after `took` (the shell's own
    /// clock, from the line sent to the answer back): the card settles and
    /// says so. A stop or a refusal the turn's beats tell later still names
    /// how it ended ([`ActivityCard::settle`]).
    pub(crate) fn settle_activity(&mut self, took: std::time::Duration) {
        self.settle_card(Ending::Completed, Some(took));
    }

    fn settle_card(&mut self, ending: Ending, took: Option<std::time::Duration>) {
        if let Some(card) = self.activity.as_mut()
            && card.card.settle(ending, took)
        {
            card.paint(&mut self.transcript, self.ascii);
        }
    }

    /// The blocks the inline view has not yet handed to the terminal.
    #[must_use]
    pub fn uncommitted(&self) -> &[Committed] {
        self.transcript
            .get(self.committed_inline..)
            .unwrap_or_default()
    }
}

/// The live card in the transcript: where its block is, and what it folded.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Activity {
    index: usize,
    card: ActivityCard,
}

impl Activity {
    /// Write the card's words into its block.
    fn paint(&self, transcript: &mut [Committed], ascii: bool) {
        if let Some(block) = transcript.get_mut(self.index) {
            block.text = self.card.lines(ascii).join("\n");
        }
    }
}

/// A canned conversation: every submitted line advances one turn and yields
/// that turn's beats. The same script drives both presentations in UX-1.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Script {
    opening: Vec<Beat>,
    turns: Vec<Vec<Beat>>,
    next: usize,
}

impl Script {
    /// A script from its opening beats and its turns.
    #[must_use]
    pub fn new(opening: Vec<Beat>, turns: Vec<Vec<Beat>>) -> Self {
        Self {
            opening,
            turns,
            next: 0,
        }
    }

    /// The beats of the opening (banner, restored state, first prompt).
    #[must_use]
    pub fn open(&self) -> Vec<Beat> {
        self.opening.clone()
    }

    /// The beats of the next turn; past the last turn, the same closing
    /// reply and the free prompt.
    pub fn submit(&mut self, line: &str) -> Vec<Beat> {
        let turn = self.turns.get(self.next).cloned();
        self.next += 1;
        match turn {
            Some(beats) => beats,
            None => vec![
                Beat::Say(Committed::new(
                    Kind::Reply,
                    format!("the fixture has no turn left for « {} »", line.trim()),
                )),
                Beat::Wait(Waiting::Free),
            ],
        }
    }

    /// Turns still to play.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.turns.len().saturating_sub(self.next)
    }

    /// The UX-1 fixture: one real-shaped conversation from a sentence to a
    /// result with a gate, the shapes of the plain loop's outcomes.
    #[must_use]
    pub fn demo() -> Self {
        let opening = vec![
            Beat::Say(Committed::new(
                Kind::Banner,
                "Nika · demo\n\nWhat do you want to automate?\n  describe the outcome · Nika asks only for what's missing · /help · /status",
            )),
            Beat::Wait(Waiting::Free),
        ];
        let turns = vec![
            vec![
                Beat::Busy("reading the sentence".to_owned()),
                Beat::Say(Committed::new(
                    Kind::Question,
                    "Which file holds the notes to digest? (const.source_path)",
                )),
                Beat::Wait(Waiting::Question {
                    key: "const.source_path".to_owned(),
                }),
            ],
            vec![
                Beat::Say(Committed::new(
                    Kind::Proposal,
                    "digest-notes · 3 steps in run order\n  1 read      ./notes/lundi.md\n  2 draft     one paragraph, the seat drafts it\n  3 write     ./digest.md\nboundary · reads ./notes/** · writes ./digest.md · no network · no exec\nidentity · 9f3c1a · these exact bytes, nothing else",
                )),
                Beat::Wait(Waiting::Proposal),
            ],
            vec![
                Beat::Say(Committed::new(
                    Kind::Report,
                    "saved ./digest-notes.nika · check · VALID · ACCESS READY · CAPACITY FIT · RUN READY\nsay « run it » to run it once · ceiling $0.25",
                )),
                Beat::Wait(Waiting::Free),
            ],
            vec![
                Beat::Say(Committed::new(
                    Kind::Report,
                    "running ./digest-notes.nika once · ceiling $0.25",
                )),
                Beat::Say(Committed::new(Kind::Run, "read     ✓  12 ms")),
                Beat::Say(Committed::new(Kind::Run, "draft    ✓  1.8 s  $0.0021")),
                Beat::Say(Committed::new(
                    Kind::Gate,
                    "write ./digest.md · 412 bytes · overwrite the existing file?",
                )),
                Beat::Wait(Waiting::Gate),
            ],
            vec![
                Beat::Say(Committed::new(Kind::Run, "write    ✓  3 ms")),
                Beat::Say(Committed::new(
                    Kind::Result,
                    "produced ./digest.md (412 bytes)\n2.1 s · $0.0021 · trace .nika/traces/2026-09-21T21-40-02.ndjson · sealed",
                )),
                Beat::Wait(Waiting::Free),
            ],
            vec![Beat::Say(Committed::new(Kind::Reply, "bye")), Beat::Quit],
        ];
        Self::new(opening, turns)
    }
}

/// Terminal work a turn needs done outside the renderer: the shell hands
/// the terminal back to the plain path, the conversation performs the work
/// through [`Conversation::perform`], and the shell takes the terminal
/// again. The id ties the request to the work the conversation keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Handoff {
    /// The conversation's own id of the pending work.
    pub id: u64,
    /// What the human is told is happening (the report line).
    pub label: String,
}

/// What a submitted line produced.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Turn {
    /// The beats, in order.
    pub beats: Vec<Beat>,
    /// The terminal work the turn asks for, performed after the beats.
    pub handoff: Option<Handoff>,
}

/// What one stop request found ([`Conversation::stopper`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Stopping {
    /// The turn's preparation was asked to stop: the Session admits no
    /// further call for it and answers the turn itself (stopped, or what it
    /// had already settled).
    Requested,
    /// A Run is under way: it keeps its own doors and is never stopped here.
    RunUnderway,
}

/// The shell's hold on one turn's preparation: armed before the turn moves
/// to its worker, called from the shell's thread while the turn runs.
pub type Stopper = Box<dyn Fn() -> Stopping + Send>;

/// Whatever answers the composer: the live session runtime, or a fixture.
/// `Send`: the shell computes a turn on a worker thread so the terminal
/// stays live (the busy state changes while a seat is called).
pub trait Conversation: Send {
    /// Discard typeahead when a fresh human decision first becomes visible.
    fn fresh_input_required(&self) -> bool {
        false
    }
    /// Invalidate an unsubmitted decision on interruption, without running it.
    fn cancel_pending(&mut self) -> Vec<Beat> {
        Vec::new()
    }
    /// Arm a stop for the turn about to start, BEFORE the conversation moves
    /// to the worker thread. The default arms none: `Ctrl+C` then warns and
    /// leaves, as it always did. Stopping a preparation never stops a Run.
    fn stopper(&mut self) -> Option<Stopper> {
        None
    }
    /// The human stopped this turn's preparation and the turn ended anyway:
    /// what the stopped preparation left pending is withdrawn, so no obsolete
    /// result stays actionable. It answers nothing: no Save, Run, gate or cost
    /// decision is answered or declined. The default withdraws nothing.
    fn withdraw_stopped(&mut self) -> Vec<Beat> {
        Vec::new()
    }
    /// The beats of the opening (banner, restored state, first prompt).
    fn open(&mut self) -> Vec<Beat>;
    /// The beats of one submitted line, and the handoff it asks for.
    fn submit(&mut self, line: &str) -> Turn;
    /// [`Conversation::submit`], with a sink for the truthful busy labels
    /// the turn produces WHILE it runs (« Working through this workflow… »);
    /// the shell draws each one as it arrives. The default sends none.
    fn submit_with(&mut self, line: &str, busy: &std::sync::mpsc::Sender<String>) -> Turn {
        let _ = busy;
        self.submit(line)
    }
    /// [`Conversation::submit_with`], with the shell's queue for what it
    /// observes of a run the turn drives ([`crate::session::feed::Seen`]): the
    /// request, then each typed frame of the run's stream. The default
    /// observes none.
    fn submit_observed(
        &mut self,
        line: &str,
        busy: &std::sync::mpsc::Sender<String>,
        seen: &crate::session::feed::Seen,
    ) -> Turn {
        let _ = seen;
        self.submit_with(line, busy)
    }
    /// Perform the handed-off work with the terminal handed back; the beats
    /// that follow it (the observation, the next prompt).
    fn perform(&mut self, handoff: &Handoff) -> Vec<Beat>;
    /// The work a submitted line starts, named before it runs, so the shell
    /// shows the busy state from the first instant; the labels the turn
    /// itself emits ([`Conversation::submit_with`]) replace it as they
    /// arrive. `None` draws nothing.
    fn busy_label(&self, _line: &str) -> Option<String> {
        None
    }
    /// The slash commands this conversation answers, in its own order:
    /// `Tab` completes them. The default knows none.
    fn commands(&self) -> Vec<String> {
        Vec::new()
    }
    /// A host's kept display arrangement, without Session or Run authority.
    /// The shell reads it at open, never while drawing. The default keeps
    /// the automatic Session layout.
    fn arrangement(&self) -> Option<crate::workspace::geometry::Arrangement> {
        None
    }
    /// Keep a settled human display change through the host. A notice may
    /// report an unconfirmed preference save; no input decision is answered.
    /// The default retains no preferences outside this shell.
    fn keep_arrangement(
        &mut self,
        _arrangement: crate::workspace::geometry::Arrangement,
    ) -> Vec<Beat> {
        Vec::new()
    }
    /// The project this conversation stands in, read-only, as its Session
    /// observed it: the workspace's header, aside and welcome read it. The
    /// shell asks after the opening and after every turn, never while it
    /// draws; the answer is a projection of what the conversation already
    /// holds, never a fresh walk of the disk. The default lends none, and
    /// the workspace then says that no project is known.
    fn project(&self) -> Option<crate::workspace::project::ProjectView> {
        None
    }
    /// One look at the listed workflow `path`, taken by the conversation's
    /// Session when the human opens it (never while drawing): the exact bytes,
    /// their witness, and the check and graph of those same bytes. A look
    /// grants nothing and is never sent to a model. The default takes none,
    /// and the object then shows what the listing judged.
    fn inspect(&mut self, _path: &str) -> Option<crate::workspace::inspect::Inspected> {
        None
    }
    /// The candidate under review, as the conversation's Session lends it:
    /// the identity a consent names, the changes a yes lands, what its
    /// workflow reaches, its rehearsal and the look of its exact pending
    /// bytes, folded when the turn that proposed it ended. The shell asks
    /// after every batch of beats, never while it draws; showing it grants
    /// nothing. The default lends none.
    fn candidate(&self) -> Option<crate::workspace::candidate::Proposed> {
        None
    }
    /// A file the run `execution` reported writing, read now by the
    /// conversation's host below its project (never while drawing): today's
    /// bytes at that path, never called the run's own. The default reads none.
    fn fetch(
        &mut self,
        _execution: &nika_display::run_story::ExecutionId,
        _path: &str,
    ) -> Option<crate::session::acquire::Fetched> {
        None
    }
    /// The Proof of the journal the run `execution` settled with, captured
    /// and verified by the host (never while drawing). The default proves none.
    fn prove(
        &mut self,
        _execution: &nika_display::run_story::ExecutionId,
    ) -> Option<crate::session::acquire::Proven> {
        None
    }
    /// Adopt what `proven` (the verified journal of the kept run
    /// `execution`, the reading the shell has just accepted as current)
    /// records: its written names and child relations, for reading only.
    /// Called once per accepted reading; `false` when nothing is adopted.
    /// The default adopts nothing.
    fn adopt(
        &mut self,
        _execution: &nika_display::run_story::ExecutionId,
        _proven: &crate::session::acquire::Proven,
    ) -> bool {
        false
    }
    /// The journal of the child run task `task` of the run `execution`
    /// called, as the relation `relation` its settle frame named: read by
    /// the host only when it kept that very relation itself (never a path
    /// the renderer names), captured once and verified, never while
    /// drawing. The default reads none.
    fn child(
        &mut self,
        _execution: &nika_display::run_story::ExecutionId,
        _task: &str,
        _relation: &nika_display::run_story::ChildRun,
    ) -> Option<crate::session::acquire::ChildRead> {
        None
    }
    /// The last run an earlier session kept, as HOME history recorded it:
    /// evidence, never authority. The default keeps none.
    fn kept_run(&self) -> Option<Result<nika_session::KeptRun, String>> {
        None
    }
}

/// The project the demo conversation ([`Script::demo`]) stands in: a fixture
/// of the shape a Session lends ([`Conversation::project`]), with one
/// workflow of each verdict. It names nothing on this machine.
#[must_use]
pub fn demo_project() -> crate::workspace::project::ProjectView {
    use crate::workspace::header::Manifest;
    use crate::workspace::project::{ProjectView, WorkflowView};
    ProjectView::new("local", "demo", "~/Projects/demo")
        .with_git(true)
        .governed(Manifest::Here)
        .listing(
            vec![
                WorkflowView::new("release.nika", Some("release"), true, 0, 4),
                WorkflowView::new("enrich.nika", Some("enrich"), false, 2, 3),
                WorkflowView::new(
                    "flows/weekly-digest.nika",
                    Some("weekly-digest"),
                    true,
                    0,
                    5,
                ),
            ],
            true,
        )
        .seated("the demo script, no model is called")
}

impl Conversation for Script {
    fn open(&mut self) -> Vec<Beat> {
        Script::open(self)
    }

    fn submit(&mut self, line: &str) -> Turn {
        Turn {
            beats: Script::submit(self, line),
            handoff: None,
        }
    }

    fn perform(&mut self, _handoff: &Handoff) -> Vec<Beat> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prompt_names_what_waits_and_never_crosses() {
        assert_eq!(Waiting::Free.prompt(), "nika › ");
        assert_eq!(Waiting::Proposal.prompt(), "Save? › ");
        assert_eq!(Waiting::Gate.prompt(), "answer › ");
        assert_eq!(
            Waiting::Question {
                key: "model".to_owned()
            }
            .prompt(),
            "reply › "
        );
        assert_eq!(Waiting::Choosing.prompt(), "› ");
    }

    /// A spending question names its choices in plain words (no colour is
    /// needed to read them); any other question keeps its own hint.
    #[test]
    fn a_spending_question_names_its_three_choices_in_words() {
        for key in ["unknown_cost", "run_cost"] {
            let hint = Waiting::Question {
                key: key.to_owned(),
            }
            .hint();
            for choice in ["yes approves once", "no or Ctrl+C cancels", "details"] {
                assert!(hint.contains(choice), "{key}: {hint}");
            }
        }
        // The key alone does not prove that an actual default was offered.
        for key in ["model", "revision.new_path", "", "required_input"] {
            assert_eq!(
                Waiting::Question {
                    key: key.to_owned()
                }
                .hint(),
                "answer the question above · cancel to stop"
            );
        }
    }

    #[test]
    fn observed_activity_is_bounded_deduplicated_and_stops_at_a_reply() {
        let mut state = UiState::new(Presentation::Workspace, false, (120, 40));
        state.observe_activity("reading the request");
        state.observe_activity("reading the request");
        assert_eq!(state.transcript.len(), 1);
        assert_eq!(
            state.transcript[0]
                .text
                .matches("reading the request")
                .count(),
            1
        );
        for n in 0..15 {
            state.observe_activity(&format!("repair {n}"));
        }
        assert_eq!(state.transcript.len(), 1);
        assert!(
            state.transcript[0]
                .text
                .contains("4 earlier updates omitted")
        );
        assert!(state.transcript[0].text.ends_with("repair 14"));
        assert_eq!(state.transcript[0].text.lines().count(), 13);
        state.apply(Beat::Say(Committed::new(Kind::Question, "Which source?")));
        let finished = state.transcript[0].clone();
        assert!(
            finished
                .text
                .starts_with("Settled · 4 earlier updates omitted")
        );
        assert!(finished.text.ends_with("repair 14"));
        state.observe_activity("reading the answer");
        assert_eq!(state.transcript.len(), 3);
        assert_eq!(state.transcript[0], finished);
        for presentation in [Presentation::Inline, Presentation::Focus] {
            let mut state = UiState::new(presentation, false, (80, 24));
            state.observe_activity("reading");
            assert!(state.transcript.is_empty());
        }
    }

    #[test]
    fn activity_chrome_is_ascii_without_rewriting_observed_content() {
        let mut state = UiState::new(Presentation::Workspace, false, (80, 24));
        state.ascii = true;
        state.observe_activity("reading the request");
        assert!(state.transcript[0].text.is_ascii());
        for n in 0..13 {
            state.observe_activity(&format!("step {n}"));
        }
        assert!(state.transcript[0].text.is_ascii());
        state.observe_activity("using private/été·beta");
        assert!(state.transcript[0].text.ends_with("using private/été·beta"));
    }

    /// A run's steps read once and in order: its check, its announcement, then its own task
    /// lines. The card that showed them live gives way instead of repeating them above.
    #[test]
    fn a_run_block_replaces_its_observed_card_so_steps_read_once_in_order() {
        let mut state = UiState::new(Presentation::Workspace, false, (120, 40));
        state.apply(Beat::Say(Committed::new(Kind::Human, "run reorder.nika")));
        for label in [
            "▶ read_stock",
            "✔ read_stock · 2 ms · 1/2",
            "✔ write_order · 1 ms · 2/2",
        ] {
            state.observe_activity(label);
        }
        assert_eq!(
            state.transcript.len(),
            2,
            "progress shows while the run works"
        );
        let story = "running · reorder\n  ✔ read_stock · 2 ms\n  ✔ write_order · 1 ms\nsucceeded · 2/2 tasks";
        for beat in [
            Beat::Say(Committed::new(
                Kind::Report,
                "check · `reorder.nika` · clean ✔",
            )),
            Beat::Say(Committed::new(
                Kind::Report,
                "running `reorder.nika` once · ceiling $0.00",
            )),
            Beat::Say(Committed::new(Kind::Run, story)),
            Beat::Say(Committed::new(Kind::Result, "Done · ./order.json (24 B)")),
            Beat::Wait(Waiting::Free),
        ] {
            state.apply(beat);
        }
        let kinds: Vec<Kind> = state.transcript.iter().map(|block| block.kind).collect();
        assert_eq!(
            kinds,
            [
                Kind::Human,
                Kind::Report,
                Kind::Report,
                Kind::Run,
                Kind::Result
            ]
        );
        assert!(state.transcript[1].text.starts_with("check"));
        assert_eq!(state.transcript[3].text, story);
        let seen: usize = (state.transcript.iter())
            .map(|block| block.text.matches("write_order").count())
            .sum();
        assert_eq!(seen, 1, "each step reads once");
        state.observe_activity("reading the request");
        assert_eq!(state.transcript.len(), 6);
        assert_eq!(state.transcript[5].kind, Kind::Activity);
        assert!(state.transcript[5].text.starts_with("Working"));
    }

    /// A card already handed to the terminal's scrollback is never unprinted, and a card of a
    /// turn that said no run (or of an earlier turn) stays where it was, settled.
    #[test]
    fn a_printed_or_unrelated_activity_card_stays_where_it_was() {
        let mut state = UiState::new(Presentation::Workspace, false, (120, 40));
        state.observe_activity("✔ write_order · 1 ms · 2/2");
        state.committed_inline = state.transcript.len();
        state.apply(Beat::Say(Committed::new(
            Kind::Run,
            "succeeded · 2/2 tasks",
        )));
        assert_eq!(state.transcript.len(), 2);
        assert_eq!(
            state.transcript[0].text, "Settled\n✔ write_order · 1 ms · 2/2",
            "a printed card is kept, settled, never left saying work is under way"
        );
        let mut state = UiState::new(Presentation::Workspace, false, (120, 40));
        state.observe_activity("reading the request");
        state.apply(Beat::Say(Committed::new(
            Kind::Proposal,
            "proposed workflow",
        )));
        let first = state.transcript[0].clone();
        assert_eq!(first.text, "Settled\nreading the request");
        state.observe_activity("checking the proposal");
        state.apply(Beat::Wait(Waiting::Proposal));
        let cards = state.transcript.clone();
        state.apply(Beat::Say(Committed::new(
            Kind::Run,
            "succeeded · 2/2 tasks",
        )));
        assert_eq!(state.transcript[0], first);
        assert_eq!(state.transcript[..3], cards[..]);
        assert_eq!(state.transcript.len(), 4);
    }

    /// The typed reports fill one card the turn keeps; the turn's end settles it with the
    /// shell's time, a stop the Session confirmed says so, and a later turn opens its own.
    /// A string line (a run's story) is a step, never read for a phase.
    #[test]
    fn typed_reports_fill_one_card_that_settles_or_says_it_was_stopped() {
        use nika_display::activity::{CallState, Phase};
        use std::time::Duration;
        let mut state = UiState::new(Presentation::Workspace, false, (120, 40));
        state.observe(Update::phase(Phase::Authoring, "authoring", false));
        let at = Duration::from_secs(3);
        let started = CallState::Started;
        state.observe(Update::call(1, "fill", "m", Phase::Authoring, started, at));
        assert_eq!(state.transcript.len(), 1);
        assert_eq!(state.transcript[0].kind, Kind::Activity);
        assert_eq!(
            state.transcript[0].text,
            "Generating · 1 author call · conversation routing and decision-service calls not counted\n● Generating · authoring\n● call 1 · write a step · requested m · running"
        );
        state.observe_activity("✓ looks like a phase");
        assert!(
            state.transcript[0]
                .text
                .ends_with("✓ looks like a phase\n● call 1 · write a step · requested m · running"),
            "a string is a step: {}",
            state.transcript[0].text
        );
        state.settle_activity(Duration::from_secs(42));
        assert!(
            state.transcript[0]
                .text
                .starts_with("Settled · took 42 s · 1 author call · ")
        );
        state.apply(Beat::Cancelled("Stopped · nothing was proposed".to_owned()));
        assert!(
            state.transcript[0]
                .text
                .starts_with("Stopped by you · took 42 s · 1 author call · ")
        );
        assert_eq!(state.transcript[1].kind, Kind::Notice);
        state.apply(Beat::Wait(Waiting::Free));
        state.observe(Update::phase(
            Phase::Understanding,
            "reading your line",
            false,
        ));
        assert_eq!(state.transcript.len(), 3, "a later turn opens its own card");
        state.apply(Beat::Wait(Waiting::Proposal));
        assert_eq!(
            state.transcript[2].text,
            "Settled\n● Understanding · reading your line"
        );
        // A turn that ends on a refusal did not complete, and says so.
        state.observe(Update::phase(Phase::Checking, "checking", false));
        state.settle_activity(Duration::from_secs(5));
        state.apply(Beat::Say(Committed::new(Kind::Refusal, "not allowed")));
        assert!(
            state.transcript[3]
                .text
                .starts_with("Not completed · took 5 s\n")
        );
        let mut inline = UiState::new(Presentation::Inline, false, (80, 24));
        inline.observe(Update::phase(Phase::Checking, "checking", false));
        inline.apply(Beat::Cancelled("stopped".to_owned()));
        assert_eq!(inline.transcript.len(), 1, "inline keeps only the notice");
    }

    #[test]
    fn beats_derive_the_state_and_busy_clears_on_the_next_word() {
        let mut state = UiState::new(Presentation::Inline, false, (80, 24));
        state.apply(Beat::Busy("reading".to_owned()));
        assert_eq!(state.busy.as_deref(), Some("reading"));
        state.apply(Beat::Say(Committed::new(Kind::Reply, "hello")));
        assert!(state.busy.is_none());
        assert_eq!(state.transcript.len(), 1);
        assert_eq!(state.uncommitted().len(), 1);
        state.committed_inline = 1;
        assert!(state.uncommitted().is_empty());
        state.apply(Beat::Wait(Waiting::Gate));
        assert_eq!(state.waiting, Waiting::Gate);
        state.apply(Beat::Quit);
        assert!(state.quit);
    }

    /// The rail beat keeps the lifecycle beside the status: each replaced
    /// at every turn, neither a block.
    #[test]
    fn the_rail_beat_keeps_the_lifecycle_beside_the_status() {
        let mut state = UiState::new(Presentation::Inline, false, (80, 24));
        assert!(state.rail.is_empty());
        state.apply(Beat::Rail(
            "Draft ○ · Saved ○ · Checked ○ · Active ○ · Run ○".to_owned(),
        ));
        state.apply(Beat::Status("Ready for review · `x.nika`".to_owned()));
        assert_eq!(
            state.rail,
            "Draft ○ · Saved ○ · Checked ○ · Active ○ · Run ○"
        );
        assert_eq!(state.status, "Ready for review · `x.nika`");
        assert!(state.transcript.is_empty());
    }

    #[test]
    fn the_demo_script_walks_from_a_sentence_to_a_result_through_a_gate() {
        let mut script = Script::demo();
        assert!(matches!(
            script.open().last(),
            Some(Beat::Wait(Waiting::Free))
        ));
        let states: Vec<Waiting> = (0..5)
            .map(|_| {
                script
                    .submit("x")
                    .into_iter()
                    .filter_map(|b| match b {
                        Beat::Wait(w) => Some(w),
                        _ => None,
                    })
                    .next_back()
                    .unwrap_or_default()
            })
            .collect();
        assert_eq!(
            states,
            vec![
                Waiting::Question {
                    key: "const.source_path".to_owned()
                },
                Waiting::Proposal,
                Waiting::Free,
                Waiting::Gate,
                Waiting::Free,
            ]
        );
        assert_eq!(script.remaining(), 1);
        assert!(script.submit("bye").contains(&Beat::Quit));
        let past = script.submit("more");
        assert!(matches!(past.first(), Some(Beat::Say(c)) if c.kind == Kind::Reply));
    }
}
