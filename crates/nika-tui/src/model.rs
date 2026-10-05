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
                self.activity = None;
                self.busy = None;
                self.transcript.push(block);
            }
            Beat::Wait(waiting) => {
                self.activity = None;
                self.busy = None;
                self.waiting = waiting;
            }
            Beat::Busy(label) => self.busy = Some(label),
            Beat::Status(line) => self.status = line,
            Beat::Rail(line) => self.rail = line,
            Beat::Quit => self.quit = true,
        }
    }

    /// Retain the progress the Session actually reported, in one compact card.
    /// The workspace retains steps; inline and focus keep their transcript behavior.
    pub(crate) fn observe_activity(&mut self, label: &str) {
        if self.presentation != Presentation::Workspace || label.trim().is_empty() {
            return;
        }
        let activity = self.activity.get_or_insert_with(|| {
            let index = self.transcript.len();
            self.transcript.push(Committed::new(Kind::Report, ""));
            Activity {
                index,
                lines: Vec::new(),
                omitted: 0,
            }
        });
        if activity.lines.last().is_some_and(|last| last == label) {
            return;
        }
        activity.lines.push(label.to_owned());
        if activity.lines.len() > 12 {
            activity.lines.remove(0);
            activity.omitted += 1;
        }
        let separator = if self.ascii { "-" } else { "·" };
        let prefix = if activity.omitted == 0 {
            format!("Activity {separator} observed steps")
        } else {
            format!(
                "Activity {separator} {} earlier updates omitted",
                activity.omitted
            )
        };
        if let Some(block) = self.transcript.get_mut(activity.index) {
            block.text = format!("{prefix}\n{}", activity.lines.join("\n"));
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct Activity {
    index: usize,
    lines: Vec<String>,
    omitted: usize,
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
        let finished = state.transcript[0].clone();
        state.apply(Beat::Say(Committed::new(Kind::Question, "Which source?")));
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
