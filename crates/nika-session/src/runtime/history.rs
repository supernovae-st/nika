// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Local conversation evidence, never a store of executable permissions.
//! The lifetime lock owns this project's writer; no transaction is held
//! across inference. Hash chaining detects accidental damage, not forgery
//! by someone who can rewrite the private history directory.

use std::fs::File;
use std::io::{self, Read as _};
use std::path::Path;

use nika_fs::OwnedDir;
use serde::{Deserialize, Serialize};

const LOG: &str = "events.ndjson";
const MAX_LOG_BYTES: usize = 16 * 1024 * 1024;
const MAX_RECORD_BYTES: usize = 1024 * 1024;
const MAX_INPUT_BYTES: usize = 64 * 1024;
const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";

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
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Operation {
    Turn,
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
        state: Saved,
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
    pub restored: bool,
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
        let lease = dir.open_lock("session.lock")?;
        lease.try_lock().map_err(|error| {
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
            restored: false,
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
            // Expire pending authority and record uncertainty before any new
            // operation. Recovery never calls a reasoner or a workflow.
            history.append(Event::Recovered)?;
        }
        Ok(history)
    }

    fn replay(&mut self, file: File) -> io::Result<()> {
        let mut bytes = Vec::new();
        file.take(MAX_LOG_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_LOG_BYTES {
            return Err(invalid(
                "conversation history exceeds 16 MiB; preserve it for migration",
            ));
        }
        let text = String::from_utf8(bytes).map_err(|_| invalid("history is not UTF-8"))?;
        if text.is_empty() || !text.ends_with('\n') {
            return Err(invalid(
                "conversation history is empty or truncated; nothing was reset",
            ));
        }
        for line in text.lines() {
            self.accept_line(line)?;
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
        if text.len() > MAX_RECORD_BYTES || self.bytes + text.len() + 1 > MAX_LOG_BYTES {
            return Err(invalid(
                "conversation history capacity reached; preserve it for migration",
            ));
        }
        Ok(text)
    }

    fn append(&mut self, event: Event) -> io::Result<()> {
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
        if line.len() > MAX_RECORD_BYTES {
            return Err(invalid("conversation history record exceeds 1 MiB"));
        }
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
                if input.len() > MAX_INPUT_BYTES {
                    return Err(invalid("conversation input exceeds 64 KiB"));
                }
                self.started = Some((operation, input));
            }
            Event::Completed {
                state,
                run,
                authority,
                effect,
                ..
            } if self.started.is_some() => {
                if state.recent.len() > super::RECENT_TURNS {
                    return Err(invalid(
                        "conversation projection exceeds its context window",
                    ));
                }
                self.state = state;
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
        self.uncertain |= self.started.is_some() || self.run == RunState::AwaitingObservation;
        if let Some((Operation::Turn, input)) = self.started.take() {
            self.recovery_line(
                input,
                "[Interrupted turn: no completed reply was recorded.]",
            );
        }
        let note = if self.uncertain {
            Some(
                "[An earlier operation has an uncertain result. Nothing was replayed; inspect effects and receipts before proposing a retry.]",
            )
        } else if self.authority != AuthorityState::None {
            Some(
                "[The earlier proposal or gate expired. Fresh validation and consent are required.]",
            )
        } else {
            None
        };
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
        if input.len() > MAX_INPUT_BYTES {
            return Err(invalid(
                "conversation input exceeds 64 KiB; operation not started",
            ));
        }
        // Reserve one maximum record for the completion before starting an
        // effect. An oversized completion still refuses, never drops history.
        if self.bytes + 2 * MAX_RECORD_BYTES + 2 > MAX_LOG_BYTES {
            return Err(invalid(
                "conversation history is full; operation not started",
            ));
        }
        let input = crate::broker::redact(input).0;
        if input.len() > MAX_INPUT_BYTES {
            return Err(invalid("redacted conversation input exceeds 64 KiB"));
        }
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
            state,
            run,
            authority,
            outcome,
            effect,
        })
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
