// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Local conversation evidence, never a store of executable permissions.
//! The lifetime lock owns this project's writer; no transaction is held
//! across inference. Hash chaining detects accidental damage, not forgery
//! by someone who can rewrite the private history directory.

use std::fs::File;
use std::io::{self, BufRead as _, BufReader};
use std::path::Path;
use std::time::Duration;

use nika_display::front_door::recovery;
use nika_fs::OwnedDir;
use serde::{Deserialize, Serialize};

const LOG: &str = "events.ndjson";
const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";
/// How long a lease may look held before it is refused as foreign. A sibling
/// thread spawning a child (a run, an exec) duplicates this process's
/// descriptors until the child's exec closes them, and a BSD `flock` rides the
/// duplicate for that window: a bounded wait tells that window from an owner.
const LEASE_GRACE: Duration = Duration::from_millis(250);

pub(super) enum HistoryMode {
    Ephemeral,
    Active(Box<History>),
    Blocked(String),
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Saved {
    pub goal: Option<String>,
    pub decisions: Vec<String>,
    pub unresolved: Vec<String>,
    pub recent: Vec<(String, String)>,
    /// The proposal pending when the record was written, as a versioned draft value
    /// (`draft.rs`): read only by the draft schema, kept unchanged otherwise. Absent from
    /// records that had none, whose bytes stay exactly as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<serde_json::Value>,
    /// The authoring round whose question waited when the record was written, as a versioned
    /// round value (`nika_onboard::compile::round`): the one durable copy of a round (the
    /// project's structured record keeps none), read only by the round schema, kept unchanged
    /// otherwise. Absent from records that had none, whose bytes stay exactly as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round: Option<serde_json::Value>,
    /// Bounded, byte-bound program evidence; kept opaque across unknown versions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub programs: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inference_checkpoint: Option<serde_json::Value>,
    /// The last observed run (a `run_view::KeptRun` value), kept unchanged when unreadable;
    /// absent from records that had none. A present `null` is refused, never an absence.
    #[serde(default, deserialize_with = "present")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run: Option<serde_json::Value>,
    /// This conversation's own explicit intelligence choice (a `UserIntelligencePreference`
    /// value): it resumes with the conversation; absent from records that had none. A present
    /// `null` is refused, never an absence.
    #[serde(default, deserialize_with = "present")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection: Option<serde_json::Value>,
}

fn present<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<serde_json::Value>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    let null = || serde::de::Error::custom("a kept value is present but null");
    (!value.is_null()).then_some(Some(value)).ok_or_else(null)
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Operation {
    Turn,
    /// A closed Run request; its ceiling does not amend Session inference.
    Run,
    Choice,
    Consent,
    Gate,
    Observation,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RunState {
    #[default]
    Idle,
    AwaitingObservation,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AuthorityState {
    #[default]
    None,
    Proposal,
    Gate,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum EffectState {
    NoUncertaintyReported,
    Unknown,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Event {
    Opened,
    Started {
        operation: Operation,
        input: String,
    },
    Completed {
        state: Box<Saved>,
        run: RunState,
        authority: AuthorityState,
        effect: EffectState,
        // Diagnostic representation only; never deserialized as a command.
        outcome: String,
    },
    Recovered,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u8,
    project: String,
    sequence: u64,
    previous: String,
    event: Event,
    digest: String,
}

impl Record {
    fn digest(&self) -> io::Result<String> {
        let bytes = serde_json::to_vec(&(
            self.version,
            &self.project,
            self.sequence,
            &self.previous,
            &self.event,
        ))?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }
}

/// Presentation acknowledgements rebuilt from journal events, never accounting or authority.
#[derive(Default)]
struct Notices {
    unreported_effect: bool,
    run_notified: bool,
}

pub(super) struct History {
    dir: OwnedDir,
    _lease: File,
    project: String,
    sequence: u64,
    previous: String,
    bytes: usize,
    started: Option<(Operation, String)>,
    pub state: Saved,
    pub run: RunState,
    pub authority: AuthorityState,
    pub uncertain: bool,
    notices: Notices,
    pub restored: bool,
    pub monetary_seen: bool,
}

impl History {
    pub(super) fn open(home: &Path, project: &Path) -> io::Result<Self> {
        // Storage identity only: aliases of the same project share its lease.
        // Execution continues to use the runtime's separately observed root.
        let canonical = project.canonicalize()?;
        let project = canonical
            .to_str()
            .ok_or_else(|| invalid("project path is not UTF-8"))?
            .to_owned();
        let name = blake3::hash(project.as_bytes()).to_hex().to_string();
        // Confirm each directory name in its held parent before descending.
        // A synced leaf journal alone does not publish newly created ancestors.
        let mut dir = OwnedDir::open(home)?;
        for component in [".nika", "sessions", &name] {
            let child = dir.create_below(&[component])?;
            dir.as_file().sync_all()?;
            dir = child;
        }
        let lease = dir
            .hold_lock("session.lock", LEASE_GRACE)
            .map_err(|error| {
                io::Error::other(format!(
                    "cannot exclusively open conversation history: {error}"
                ))
            })?;
        let mut history = Self {
            dir,
            _lease: lease,
            project,
            sequence: 0,
            previous: GENESIS.to_owned(),
            bytes: 0,
            started: None,
            state: Saved::default(),
            run: RunState::Idle,
            authority: AuthorityState::None,
            uncertain: false,
            notices: Notices::default(),
            restored: false,
            monetary_seen: false,
        };
        match history.dir.open_relative(Path::new(LOG)) {
            Ok(file) => history.replay(file)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let line = history.encode(Event::Opened)?;
                // First publication syncs both file and its directory.
                history.dir.write_once(LOG, &format!("{line}\n"))?;
                history.accept_line(&line)?;
            }
            Err(error) => return Err(error),
        }
        if history.restored {
            // Replay marks reported uncertainty; pending authority still expires.
            // Recovery never reconciles effects or calls a reasoner or a workflow.
            history.append(Event::Recovered)?;
        }
        Ok(history)
    }

    fn replay(&mut self, file: File) -> io::Result<()> {
        // Retain one record at a time, not the entire journal. Its size is not a
        // conversation allowance; every record still passes the same chain checks.
        let mut reader = BufReader::new(file);
        let mut line = String::new();
        while reader.read_line(&mut line)? != 0 {
            if !line.ends_with('\n') {
                return Err(invalid(
                    "conversation history is empty or truncated; nothing was reset",
                ));
            }
            line.pop();
            self.accept_line(&line)?;
            line.clear();
        }
        if self.sequence == 0 {
            return Err(invalid(
                "conversation history is empty or truncated; nothing was reset",
            ));
        }
        self.restored = true;
        Ok(())
    }

    fn encode(&self, event: Event) -> io::Result<String> {
        let mut record = Record {
            version: 1,
            project: self.project.clone(),
            sequence: self.sequence,
            previous: self.previous.clone(),
            event,
            digest: String::new(),
        };
        record.digest = record.digest()?;
        let text = serde_json::to_string(&record)?;
        Ok(text)
    }

    fn append(&mut self, event: Event) -> io::Result<()> {
        #[cfg(test)]
        if REFUSE_APPEND.with(std::cell::Cell::get) {
            return Err(io::Error::other("an append this test refuses"));
        }
        let line = self.encode(event)?;
        // Do not recreate a removed journal or append to a truncated one.
        let file = self.dir.open_relative(Path::new(LOG))?;
        if file.metadata()?.len() != self.bytes as u64 {
            return Err(invalid("conversation history changed while it was open"));
        }
        self.dir.append_line(LOG, &line)?;
        self.accept_line(&line)
    }

    fn accept_line(&mut self, line: &str) -> io::Result<()> {
        let record: Record = serde_json::from_str(line)?;
        if record.version != 1
            || record.project != self.project
            || record.sequence != self.sequence
            || record.previous != self.previous
            || record.digest != record.digest()?
        {
            return Err(invalid(
                "conversation history version, binding or integrity mismatch",
            ));
        }
        match record.event {
            Event::Opened if self.sequence == 0 => {}
            Event::Started { operation, input } if self.sequence > 0 && self.started.is_none() => {
                if matches!(
                    operation,
                    Operation::Turn | Operation::Consent | Operation::Gate
                ) {
                    self.monetary_seen |=
                        super::money_parse::parse(&input).map_or(true, |p| p.amount.is_some());
                }
                self.started = Some((operation, input));
            }
            Event::Completed {
                state,
                run,
                authority,
                effect,
                outcome,
            } if self.started.is_some() => {
                if state.recent.len() > super::RECENT_TURNS {
                    return Err(invalid(
                        "conversation projection exceeds its context window",
                    ));
                }
                self.state = *state;
                self.notices.unreported_effect |= effect == EffectState::Unknown;
                self.notices.run_notified &= run == self.run
                    && !matches!(outcome.as_str(), "run_requested" | "resume_requested");
                self.run = run;
                self.authority = authority;
                self.uncertain |= effect == EffectState::Unknown;
                self.started = None;
            }
            Event::Recovered if self.sequence > 0 => self.recover(),
            _ => return Err(invalid("conversation history transition is invalid")),
        }
        self.previous = record.digest;
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| invalid("history exhausted"))?;
        self.bytes += line.len() + 1;
        Ok(())
    }

    fn recover(&mut self) {
        let notify = self.notices.unreported_effect
            || self.started.is_some()
            || (self.run == RunState::AwaitingObservation && !self.notices.run_notified);
        self.notices.unreported_effect = false;
        self.notices.run_notified = self.run == RunState::AwaitingObservation;
        self.uncertain |= self.started.is_some() || self.run == RunState::AwaitingObservation;
        if let Some((Operation::Turn | Operation::Run, input)) = self.started.take() {
            self.recovery_line(input, recovery::INTERRUPTED_TURN);
        }
        let note = recovery::conversation_note(notify, self.authority != AuthorityState::None);
        if let Some(note) = note {
            self.recovery_line("(recovery)".to_owned(), note);
        }
        self.authority = AuthorityState::None;
    }

    fn recovery_line(&mut self, user: String, note: &str) {
        if self
            .state
            .recent
            .last()
            .is_none_or(|(a, b)| a != &user || b != note)
        {
            self.state.recent.push((user, note.to_owned()));
        }
        if self.state.recent.len() > super::RECENT_TURNS {
            self.state.recent.remove(0);
        }
    }

    pub(super) fn begin(&mut self, operation: Operation, input: &str) -> io::Result<()> {
        let input = crate::broker::redact(input).0;
        self.append(Event::Started { operation, input })
    }

    pub(super) fn complete(
        &mut self,
        state: Saved,
        run: RunState,
        authority: AuthorityState,
        outcome: String,
        effect: EffectState,
    ) -> io::Result<()> {
        self.append(Event::Completed {
            state: Box::new(state),
            run,
            authority,
            outcome,
            effect,
        })
    }
}

#[cfg(test)]
thread_local! {
    /// A test's injected append failure, on its own thread only.
    pub(super) static REFUSE_APPEND: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
