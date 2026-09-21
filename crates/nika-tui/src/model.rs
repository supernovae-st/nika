// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The presentation state and the vocabulary the session speaks to it.
//!
//! A [`Beat`] is one typed thing the session did in a turn: it said a block
//! (a reply, a proposal preview, a run line, a result), it now waits for a
//! particular kind of line (the prompt names which: `nika ›` · `reply ›` ·
//! `apply? ›` · `answer ›`), or it is busy under a seat. The shapes mirror
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
}

impl Presentation {
    /// The other presentation.
    #[must_use]
    pub fn toggled(self) -> Self {
        match self {
            Self::Inline => Self::Focus,
            Self::Focus => Self::Inline,
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
    /// The prompt the plain loop prints for the same state.
    #[must_use]
    pub fn prompt(&self) -> &'static str {
        match self {
            Self::Free => "nika › ",
            Self::Choosing => "› ",
            Self::Question { .. } => "reply › ",
            Self::Proposal => "apply? › ",
            Self::Gate => "answer › ",
        }
    }

    /// The one-line hint under the composer for this state.
    #[must_use]
    pub fn hint(&self) -> &'static str {
        match self {
            Self::Free => "describe work · /help · Ctrl+T focus view · Ctrl+C twice to leave",
            Self::Choosing => "type a number · Esc keeps the current choice",
            Self::Question { .. } => "answer the question · an empty line takes the default",
            Self::Proposal => "yes applies these exact bytes · no keeps the file untouched · /show",
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
    /// The session closed the door.
    Quit,
}

/// The whole presentation state, derived from beats; the renderer reads it
/// and never writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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
    /// A first `Ctrl+C` was pressed: the next one leaves.
    pub interrupt_armed: bool,
    /// Colour allowed (the theme's decision, never the renderer's).
    pub color: bool,
    /// Scroll offset of the focus transcript, in blocks from the end.
    pub focus_scroll: usize,
    /// Terminal size as last reported.
    pub size: (u16, u16),
    /// The session asked to close.
    pub quit: bool,
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
            interrupt_armed: false,
            color,
            focus_scroll: 0,
            size,
            quit: false,
        }
    }

    /// Apply one beat.
    pub fn apply(&mut self, beat: Beat) {
        match beat {
            Beat::Say(block) => {
                self.busy = None;
                self.transcript.push(block);
            }
            Beat::Wait(waiting) => {
                self.busy = None;
                self.waiting = waiting;
            }
            Beat::Busy(label) => self.busy = Some(label),
            Beat::Quit => self.quit = true,
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
                "nika · session\nauthoring · deterministic · no model is contacted until you seat one\nproject ./ · history kept under ~/.nika · /help lists the doors",
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
pub trait Conversation {
    /// The beats of the opening (banner, restored state, first prompt).
    fn open(&mut self) -> Vec<Beat>;
    /// The beats of one submitted line, and the handoff it asks for.
    fn submit(&mut self, line: &str) -> Turn;
    /// Perform the handed-off work with the terminal handed back; the beats
    /// that follow it (the observation, the next prompt).
    fn perform(&mut self, handoff: &Handoff) -> Vec<Beat>;
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
        assert_eq!(Waiting::Proposal.prompt(), "apply? › ");
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
