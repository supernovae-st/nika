// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The contract both doors speak, [`CONTRACT`]: the commands a client sends, the frames the host
//! answers and logs, and the mechanical projection of a [`TurnOutcome`]. Nothing here reads what
//! a line means: the Session routes every line itself, against the `Waiting` it published.

use serde::Serialize;

use nika_session::RunRequest;
use nika_session::TurnOutcome;
use nika_session::activity::{Activity, CallState, Phase, ToolState};
use nika_session::outcome::ReviewId;
use nika_session::steer::Queued;

/// The version every command and frame carries.
pub const CONTRACT: &str = "nika/session-host@1";

/// The capability word an engine that serves [`CONTRACT`] advertises.
pub const CAPABILITY: &str = "sessionHost";

/// The capability word of a door whose opener may name the conversation's intelligence (the
/// census's own words, held for that conversation only, never saved).
pub const SELECTION_CAPABILITY: &str = "sessionIntelligence";

/// The capability word of a door that takes [`Command::Steer`] and [`Command::FollowUp`]: a
/// client that does not find it refuses those lines itself, before it sends one.
pub const STEERING_CAPABILITY: &str = "sessionSteering";

/// The longest command identity a client may choose.
const MAX_COMMAND: usize = 128;

/// One command, the same bytes on both doors.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Command {
    /// One line, typed against one published snapshot.
    Submit {
        /// The caller's identity for this command.
        command: String,
        /// The handle of the snapshot the line was typed against.
        snapshot: String,
        /// The line, exactly as typed.
        line: String,
    },
    /// Stop the turn under way: its preparation, or the run it executes when its door can stop
    /// it (the run's first signal: in-flight work completes, no new wave starts). Never an abort.
    Stop {
        /// The caller's identity for this command.
        command: String,
    },
    /// A line for the conversation's run under way: it enters after the calls under way.
    Steer {
        /// The caller's identity for this command.
        command: String,
        /// The line, exactly as typed.
        line: String,
    },
    /// A line for the conversation's run under way: it enters when the run would end.
    FollowUp {
        /// The caller's identity for this command.
        command: String,
        /// The line, exactly as typed.
        line: String,
    },
    /// End the Session.
    Close,
    /// The current snapshot (the native door's read; HTTP reads it by GET).
    Snapshot,
    /// The details card of the current snapshot (the native door's read).
    Details,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    contract: String,
    op: String,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    snapshot: Option<String>,
    #[serde(default)]
    line: Option<String>,
}

impl Command {
    /// The command one JSON object states.
    ///
    /// # Errors
    /// Why it is not a command of [`CONTRACT`]: not one object, an unknown field or op, another
    /// contract, a missing or forbidden field, or a command identity outside
    /// `[A-Za-z0-9._:-]{1,128}`.
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let raw: Raw = serde_json::from_slice(bytes)
            .map_err(|error| format!("not a command of {CONTRACT}: {error}"))?;
        if raw.contract != CONTRACT {
            return Err(format!("the contract `{}` is not {CONTRACT}", raw.contract));
        }
        match (raw.op.as_str(), raw.command, raw.snapshot, raw.line) {
            ("submit", Some(command), Some(snapshot), Some(line)) => Ok(Self::Submit {
                command: identity(command)?,
                snapshot,
                line,
            }),
            ("stop", Some(command), None, None) => Ok(Self::Stop {
                command: identity(command)?,
            }),
            ("steer", Some(command), None, Some(line)) => Ok(Self::Steer {
                command: identity(command)?,
                line,
            }),
            ("follow_up", Some(command), None, Some(line)) => Ok(Self::FollowUp {
                command: identity(command)?,
                line,
            }),
            ("steer" | "follow_up", ..) => Err(format!("`{}` takes command and line", raw.op)),
            ("close", None, None, None) => Ok(Self::Close),
            ("snapshot", None, None, None) => Ok(Self::Snapshot),
            ("details", None, None, None) => Ok(Self::Details),
            ("submit", ..) => Err("`submit` takes command, snapshot and line".to_owned()),
            ("stop", ..) => Err("`stop` takes command only".to_owned()),
            ("close" | "snapshot" | "details", ..) => {
                Err(format!("`{}` takes no command, snapshot or line", raw.op))
            }
            (op, ..) => Err(format!("unknown op `{op}`")),
        }
    }

    /// The command identity a line names although it is not a command [`Command::parse`]
    /// accepts: the refusal of that line then names it, so the client that sent it tells it from
    /// another line's. `None` when the line is not one JSON object or names no valid identity.
    #[must_use]
    pub fn identity_of(bytes: &[u8]) -> Option<String> {
        let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
        let command = value.get("command")?.as_str()?;
        identity(command.to_owned()).ok()
    }

    /// The digest of the exact bytes a command identity is bound to: its op, its snapshot and
    /// its line, each length-prefixed.
    pub(crate) fn digest(&self) -> [u8; 32] {
        let (op, snapshot, line) = match self {
            Self::Submit { snapshot, line, .. } => ("submit", snapshot.as_str(), line.as_str()),
            Self::Stop { .. } => ("stop", "", ""),
            Self::Steer { line, .. } => ("steer", "", line.as_str()),
            Self::FollowUp { line, .. } => ("follow_up", "", line.as_str()),
            Self::Close => ("close", "", ""),
            Self::Snapshot => ("snapshot", "", ""),
            Self::Details => ("details", "", ""),
        };
        let mut hasher = blake3::Hasher::new();
        for part in [op, snapshot, line] {
            let length = u64::try_from(part.len()).unwrap_or(u64::MAX);
            hasher.update(&length.to_le_bytes());
            hasher.update(part.as_bytes());
        }
        *hasher.finalize().as_bytes()
    }
}

/// A command identity the ledger may key by.
fn identity(command: String) -> Result<String, String> {
    let allowed = |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-');
    if command.is_empty() || command.len() > MAX_COMMAND || !command.bytes().all(allowed) {
        return Err(format!(
            "a command identity is 1 to {MAX_COMMAND} characters of [A-Za-z0-9._:-]"
        ));
    }
    Ok(command)
}

/// Why a command was refused before it reached the Session, or why the Session cannot be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Refused {
    /// Not a command of this contract.
    Malformed,
    /// No live Session by that identity: unknown, closed, or from another incarnation.
    SessionNotFound,
    /// The snapshot named is not the current one: this Session published it earlier.
    StaleSnapshot,
    /// The snapshot named was never published by this Session.
    UnknownSnapshot,
    /// A turn is under way; the line was not taken.
    Busy,
    /// The command identity is bound to other bytes.
    CommandConflict,
    /// A Session is already live for this project.
    SessionLive,
    /// The Session could not open (its history is held elsewhere, or it refused).
    SessionUnavailable,
}

impl Refused {
    /// The machine word.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Malformed => "malformed",
            Self::SessionNotFound => "session_not_found",
            Self::StaleSnapshot => "stale_snapshot",
            Self::UnknownSnapshot => "unknown_snapshot",
            Self::Busy => "busy",
            Self::CommandConflict => "command_conflict",
            Self::SessionLive => "session_live",
            Self::SessionUnavailable => "session_unavailable",
        }
    }
}

/// What a turn is doing while it runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TurnPhase {
    /// The Session prepares: a Stop applies.
    Preparing,
    /// A run requested by the Session executes: a Stop asks it to stop when its door can.
    Running,
    /// The run took its first signal: in-flight work completes, no new wave starts.
    Stopping,
    /// The turn's result is being settled: a Stop arrives too late.
    Settling,
}

impl TurnPhase {
    const fn word(self) -> &'static str {
        match self {
            Self::Preparing => "preparing",
            Self::Running => "running",
            Self::Stopping => "stopping",
            Self::Settling => "settling",
        }
    }
}

/// The turn under way, as a snapshot shows it: with the lines the person queued for the
/// conversation's run while it reads them.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct Busy {
    command: String,
    phase: &'static str,
    stop_requested: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    queued: Vec<Queued>,
}

impl Busy {
    pub(crate) fn new(command: &str, phase: TurnPhase, stop_requested: bool) -> Self {
        Self {
            command: command.to_owned(),
            phase: phase.word(),
            stop_requested,
            queued: Vec::new(),
        }
    }

    /// The same turn with the lines its run reads now.
    pub(crate) fn with_queued(mut self, queued: Vec<Queued>) -> Self {
        self.queued = queued;
        self
    }
}

/// One published snapshot, as a frame carries it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct Snapshot {
    #[serde(rename = "snapshot")]
    pub(crate) handle: String,
    pub(crate) seq: u64,
    pub(crate) busy: Option<Busy>,
    pub(crate) work: serde_json::Value,
}

/// One activity of the turn under way.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct ActivityWire {
    phase: &'static str,
    note: String,
    done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    call: Option<CallWire>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool: Option<ToolWire>,
}

/// One tool the conversation's intelligence called, as its run observed it: never its
/// arguments or reply.
#[derive(Clone, Debug, PartialEq, Serialize)]
struct ToolWire {
    call: String,
    name: String,
    state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    elapsed_ms: Option<u64>,
}

/// A physical compiler call an activity reported; `model` is the requested one.
#[derive(Clone, Debug, PartialEq, Serialize)]
struct CallWire {
    ordinal: u32,
    role: String,
    model: String,
    state: &'static str,
}

impl ActivityWire {
    /// The Session's typed activity, word for word.
    pub(crate) fn of(activity: &Activity) -> Self {
        let phase = match activity.phase {
            Phase::Understanding => "understanding",
            Phase::Knowledge => "knowledge",
            Phase::Authoring => "authoring",
            Phase::Checking => "checking",
            Phase::Repairing => "repairing",
            _ => "other",
        };
        let call = activity.call.as_ref().map(|call| CallWire {
            ordinal: call.ordinal,
            role: call.role.clone(),
            model: call.model.clone(),
            state: match call.state {
                CallState::Started => "started",
                CallState::Finished => "finished",
                CallState::Cancelled => "cancelled",
                _ => "other",
            },
        });
        let tool = activity.tool.as_ref().map(|tool| ToolWire {
            call: tool.call.clone(),
            name: tool.name.clone(),
            state: match tool.state {
                ToolState::Started => "started",
                ToolState::Finished => "finished",
                ToolState::Failed => "failed",
                _ => "other",
            },
            elapsed_ms: tool.elapsed_ms,
        });
        Self {
            phase,
            note: activity.note.clone(),
            done: activity.done,
            call,
            tool,
        }
    }

    /// One line of a requested run's story.
    pub(crate) fn run(line: String) -> Self {
        Self {
            phase: "run",
            note: line,
            done: false,
            call: None,
            tool: None,
        }
    }
}

/// One outcome of a turn, projected mechanically from the Session's [`TurnOutcome`].
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Outcome {
    Reply {
        text: String,
    },
    Facts {
        text: String,
    },
    Help {
        text: String,
    },
    Aside {
        text: String,
    },
    Ask {
        text: String,
    },
    Cancelled {
        text: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        withdrawn: Vec<Outcome>,
    },
    Stopped {
        reach: &'static str,
        text: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        unsent: Vec<Queued>,
        #[serde(skip_serializing_if = "Option::is_none")]
        candidate: Option<u64>,
    },
    Quit,
    Refusal {
        class: &'static str,
        text: String,
    },
    Held {
        proposal: String,
        text: String,
    },
    Proposal {
        proposal: String,
        text: String,
    },
    Question {
        key: String,
        text: String,
    },
    Gate {
        trace: String,
        task: String,
        text: String,
    },
    RunRequested {
        text: String,
        workflow: String,
        inputs: Vec<String>,
        max_cost_usd: f64,
    },
    ResumeRequested {
        workflow: String,
        trace: String,
        answer: String,
    },
    RunReview {
        review: String,
        text: String,
    },
    RunReviewed {
        review: String,
        approve: bool,
    },
    RunNotStarted {
        text: String,
    },
    RunUnobserved {
        text: String,
    },
    /// The Stop reached the run and it sealed its trace as cancelled.
    RunStopped {
        text: String,
    },
    /// The Stop reached the run but it ended without sealing its trace (an abort or a crash).
    RunAborted {
        text: String,
    },
    Resumed {
        text: String,
    },
    Other,
}

impl Outcome {
    /// Whether a Stop's withdrawal removed what this outcome announced: a new proposal or a new
    /// question of the stopped preparation.
    pub(crate) const fn withdrawable(&self) -> bool {
        matches!(self, Self::Proposal { .. } | Self::Question { .. })
    }
}

/// What a turn asks its host to do after it: the run door's work, in order.
#[derive(Debug)]
pub(crate) enum Effect {
    /// Run the saved workflow once, as the Session requested it.
    Run(RunRequest),
    /// Resume a paused run with the human's answer.
    Resume {
        workflow: std::path::PathBuf,
        trace: std::path::PathBuf,
        answer: String,
    },
    /// The human answered the run cost review this host holds.
    Reviewed { review: ReviewId, approve: bool },
}

/// The text of a path, as the Session named it.
fn text(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Project one outcome into `wire`, in order, and collect what it asks the host to do.
pub(crate) fn project(outcome: TurnOutcome, wire: &mut Vec<Outcome>, effects: &mut Vec<Effect>) {
    let projected = match outcome {
        TurnOutcome::Reply(text) => Outcome::Reply { text },
        TurnOutcome::Facts(text) => Outcome::Facts { text },
        TurnOutcome::Help(text) => Outcome::Help { text },
        TurnOutcome::Aside(text) => Outcome::Aside { text },
        TurnOutcome::Ask(text) => Outcome::Ask { text },
        TurnOutcome::Cancelled(text) => Outcome::Cancelled {
            text,
            withdrawn: Vec::new(),
        },
        TurnOutcome::Stopped(stopped) => Outcome::Stopped {
            reach: stopped.reach.as_str(),
            text: stopped.text(),
            unsent: stopped.unsent,
            candidate: stopped.candidate,
        },
        TurnOutcome::Quit => Outcome::Quit,
        TurnOutcome::Refusal(refusal) => Outcome::Refusal {
            class: refusal.class.as_str(),
            text: refusal.text,
        },
        TurnOutcome::Held { id, preview } => Outcome::Held {
            proposal: id.as_str().to_owned(),
            text: preview,
        },
        TurnOutcome::Proposal { id, preview } => Outcome::Proposal {
            proposal: id.as_str().to_owned(),
            text: preview,
        },
        TurnOutcome::Question { key, question } => Outcome::Question {
            key,
            text: question,
        },
        TurnOutcome::GateAsk { id, question } => Outcome::Gate {
            trace: text(&id.trace),
            task: id.task,
            text: question,
        },
        TurnOutcome::RunRequested { report, run } => {
            let projected = Outcome::RunRequested {
                text: report,
                workflow: text(&run.workflow),
                inputs: (run.vars.iter())
                    .map(|var| var.split_once('=').map_or(var.as_str(), |(name, _)| name))
                    .map(str::to_owned)
                    .collect(),
                max_cost_usd: run.max_cost_usd,
            };
            effects.push(Effect::Run(run));
            projected
        }
        TurnOutcome::ResumeRequested {
            workflow,
            trace,
            answer,
        } => {
            let projected = Outcome::ResumeRequested {
                workflow: text(&workflow),
                trace: text(&trace),
                answer: answer.clone(),
            };
            effects.push(Effect::Resume {
                workflow,
                trace,
                answer,
            });
            projected
        }
        TurnOutcome::RunReviewed { review, approve } => {
            let projected = Outcome::RunReviewed {
                review: review.as_str().to_owned(),
                approve,
            };
            effects.push(Effect::Reviewed { review, approve });
            projected
        }
        TurnOutcome::Resumed { notice, outcome } => {
            wire.push(Outcome::Resumed { text: notice });
            return project(*outcome, wire, effects);
        }
        _ => Outcome::Other,
    };
    wire.push(projected);
}

/// One frame of the contract, as both doors write it: a direct reply, or an event of the
/// Session's log (then it carries `event`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Frame {
    contract: &'static str,
    #[serde(rename = "frame")]
    kind: &'static str,
    session: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    event: Option<u64>,
    #[serde(flatten)]
    body: Body,
}

/// What a frame says, by kind.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub(crate) enum Body {
    Opened {
        snapshot: Snapshot,
        notices: Vec<String>,
    },
    Accepted {
        command: String,
        op: &'static str,
    },
    Activity {
        command: String,
        #[serde(flatten)]
        activity: ActivityWire,
    },
    Submitted {
        command: String,
        op: &'static str,
        replayed: bool,
        outcomes: Vec<Outcome>,
        snapshot: Snapshot,
    },
    Stopped {
        command: String,
        op: &'static str,
        replayed: bool,
        receipt: &'static str,
        target: Option<String>,
        snapshot: Snapshot,
    },
    Queued {
        command: String,
        op: &'static str,
        replayed: bool,
        receipt: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        queued: Option<Queued>,
        target: Option<String>,
        snapshot: Snapshot,
    },
    Closed {
        snapshot: Snapshot,
    },
    Refused {
        #[serde(skip_serializing_if = "Option::is_none")]
        command: Option<String>,
        error: &'static str,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        line: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        snapshot: Option<Snapshot>,
    },
    Current {
        snapshot: Snapshot,
    },
    Details {
        snapshot: String,
        text: String,
    },
    Resync {
        snapshot: Snapshot,
    },
}

impl Body {
    const fn kind(&self) -> &'static str {
        match self {
            Self::Opened { .. } => "opened",
            Self::Accepted { .. } => "accepted",
            Self::Activity { .. } => "activity",
            Self::Submitted { .. } | Self::Stopped { .. } | Self::Queued { .. } => "result",
            Self::Closed { .. } => "closed",
            Self::Refused { .. } => "refused",
            Self::Current { .. } => "snapshot",
            Self::Details { .. } => "details",
            Self::Resync { .. } => "resync",
        }
    }
}

impl Frame {
    /// A frame of `session`; `event` when the log records it.
    pub(crate) fn new(session: &str, event: Option<u64>, body: Body) -> Self {
        Self {
            contract: CONTRACT,
            kind: body.kind(),
            session: session.to_owned(),
            event,
            body,
        }
    }

    /// A refusal of `session`, carrying the refused line and the current snapshot when known.
    pub(crate) fn refused(
        session: &str,
        error: Refused,
        message: impl Into<String>,
        command: Option<&str>,
        line: Option<&str>,
        snapshot: Option<Snapshot>,
    ) -> Self {
        Self::new(
            session,
            None,
            Body::Refused {
                command: command.map(str::to_owned),
                error: error.word(),
                message: message.into(),
                line: line.map(str::to_owned),
                snapshot,
            },
        )
    }

    /// The same recorded result, answered again to a repeated command.
    #[must_use]
    pub(crate) fn replayed(&self) -> Self {
        let mut again = self.clone();
        match &mut again.body {
            Body::Submitted { replayed, .. }
            | Body::Stopped { replayed, .. }
            | Body::Queued { replayed, .. } => *replayed = true,
            _ => {}
        }
        again
    }

    /// The frame kind (`opened` · `result` · `refused` · …).
    #[must_use]
    pub fn kind(&self) -> &'static str {
        self.kind
    }

    /// The Session the frame belongs to.
    #[must_use]
    pub fn session(&self) -> &str {
        &self.session
    }

    /// The event number, when the Session's log records this frame.
    #[must_use]
    pub fn event(&self) -> Option<u64> {
        self.event
    }

    /// The refusal, when this frame is one.
    #[must_use]
    pub fn refusal(&self) -> Option<&'static str> {
        match &self.body {
            Body::Refused { error, .. } => Some(error),
            _ => None,
        }
    }

    /// The HTTP status this frame answers with.
    #[must_use]
    pub fn status(&self) -> u16 {
        match &self.body {
            Body::Refused { error, .. } => match *error {
                "malformed" => 400,
                "session_not_found" => 404,
                _ => 409,
            },
            Body::Opened { .. } => 201,
            _ => 200,
        }
    }

    /// The frame as one JSON line, without its newline.
    #[must_use]
    pub fn to_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|error| {
            format!(r#"{{"contract":"{CONTRACT}","frame":"refused","error":"malformed","message":"a frame could not be written: {error}"}}"#)
        })
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
