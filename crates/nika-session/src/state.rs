// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The session's durable structured state (#1464 · the freeze decision):
//! the goal · the decisions · the unresolved questions · what is pending —
//! kept under the PROJECT's `.nika/`, never the transcript (the history
//! under the home is the conversation; this is the machine's record a TUI
//! or a remote door reads at open). The runtime writes it at every consent,
//! gate answer and run observation ([`crate::SessionRuntime::restore_state`]
//! reads it at open). What was pending at close never regains authority
//! (ADR-133): the record says what it was, the door says it expired.

use std::io::{self, Read as _};
use std::path::{Path, PathBuf};

use nika_fs::OwnedDir;
use serde::{Deserialize, Serialize};

/// The record's name under the project's `.nika/`.
pub const STATE_FILE: &str = "session-state.json";
const NIKA_DIR: &str = ".nika";
/// The most bytes a reader takes from the record (a bound, not a quota).
const MAX_STATE_BYTES: u64 = 1024 * 1024;

/// What waits for the human, as it was when the record was written. A
/// proposal is never here: nothing is written before its consent, and it
/// expires with the session that showed the preview (ADR-133).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub enum Pending {
    /// A paused run awaits the human's answer — the engine's own paused
    /// trace, which a fresh session waits on again.
    Gate {
        /// The workflow, relative to the root.
        workflow: PathBuf,
        /// The paused trace (the resume handle).
        trace: PathBuf,
        /// The gate's task id.
        task: String,
        /// The prompt's mode (`confirm` · `text` · `choice` …).
        mode: String,
    },
}

/// The record: the durable half of the conversation, structured.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct SessionState {
    /// The record's format ([`SessionState::VERSION`]).
    pub version: u8,
    /// When the record was written (RFC 3339 · UTC).
    pub updated_at: String,
    /// The goal, as first stated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    /// Decisions the human made, in order: a consent, a gate answer.
    #[serde(default)]
    pub decisions: Vec<String>,
    /// Questions still open.
    #[serde(default)]
    pub unresolved: Vec<String>,
    /// What waited for the human when the record was written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<Pending>,
}

impl SessionState {
    /// The current record format.
    pub const VERSION: u8 = 1;

    /// A record of this format, written at `updated_at`.
    #[must_use]
    pub fn new(updated_at: String) -> Self {
        Self {
            version: Self::VERSION,
            updated_at,
            goal: None,
            decisions: Vec::new(),
            unresolved: Vec::new(),
            pending: None,
        }
    }

    /// The record under `<root>/.nika/session-state.json`, or `None` when
    /// no session ever wrote one there.
    ///
    /// # Errors
    /// A record beyond 1 MiB, of another format, or that is not this
    /// record; the file system's refusal. Nothing is rewritten.
    pub fn load(root: &Path) -> io::Result<Option<Self>> {
        let dir = match OwnedDir::open(root).and_then(|dir| dir.open_below(&[NIKA_DIR])) {
            Ok(dir) => dir,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let file = match dir.open_relative(Path::new(STATE_FILE)) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let mut text = String::new();
        file.take(MAX_STATE_BYTES + 1).read_to_string(&mut text)?;
        if text.len() as u64 > MAX_STATE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the session record exceeds 1 MiB",
            ));
        }
        let state: Self = serde_json::from_str(&text)?;
        if state.version != Self::VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "the session record is format {} · this engine reads format {}",
                    state.version,
                    Self::VERSION
                ),
            ));
        }
        Ok(Some(state))
    }

    /// Replace the record under `<root>/.nika/` atomically (temp + rename
    /// below the root's own descriptor; the directory is created on first
    /// use).
    ///
    /// # Errors
    /// The file system's refusal; nothing is retried.
    pub fn save(&self, root: &Path) -> io::Result<()> {
        let dir = OwnedDir::open(root)?.create_below(&[NIKA_DIR])?;
        let text = serde_json::to_string_pretty(self)?;
        dir.write_atomic(STATE_FILE, &format!("{text}\n"))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn the_record_roundtrips_under_the_project_and_absence_is_none() {
        let root = tempfile::tempdir().expect("root");
        assert_eq!(SessionState::load(root.path()).expect("no record"), None);
        let mut state = SessionState::new("2026-09-13T19:09:09Z".to_owned());
        state.goal = Some("a brief".to_owned());
        state
            .decisions
            .push("applied proposal 0123456789ab".to_owned());
        state.unresolved.push("the closing line".to_owned());
        state.pending = Some(Pending::Gate {
            workflow: PathBuf::from("x.nika.yaml"),
            trace: PathBuf::from(".nika/traces/x.ndjson"),
            task: "gate".to_owned(),
            mode: "confirm".to_owned(),
        });
        state.save(root.path()).expect("saved");
        let path = root.path().join(".nika").join(STATE_FILE);
        let text = std::fs::read_to_string(&path).expect("the record");
        assert!(text.ends_with("}\n"), "pretty JSON, one trailing newline");
        let value: serde_json::Value = serde_json::from_str(&text).expect("json");
        assert_eq!(value["version"], 1);
        assert_eq!(value["pending"]["kind"], "gate");
        assert_eq!(value["pending"]["task"], "gate");
        assert_eq!(
            SessionState::load(root.path()).expect("reads back"),
            Some(state.clone())
        );
        state.pending = None;
        state.goal = None;
        state.save(root.path()).expect("replaced");
        let again = SessionState::load(root.path())
            .expect("reads back")
            .expect("present");
        assert_eq!(again, state);
        let text = std::fs::read_to_string(&path).expect("text");
        assert!(
            !text.contains("\"pending\"") && !text.contains("\"goal\""),
            "an absent value is absent, not null: {text}"
        );
    }

    #[test]
    fn a_damaged_or_foreign_record_is_named_never_rewritten() {
        let root = tempfile::tempdir().expect("root");
        let dir = root.path().join(".nika");
        std::fs::create_dir(&dir).expect("dir");
        let path = dir.join(STATE_FILE);
        std::fs::write(&path, "{ not json").expect("damage");
        assert!(SessionState::load(root.path()).is_err());
        assert_eq!(std::fs::read_to_string(&path).expect("kept"), "{ not json");
        let mut newer = SessionState::new("2099-01-01T00:00:00Z".to_owned());
        newer.version = 2;
        std::fs::write(&path, serde_json::to_string(&newer).expect("json")).expect("v2");
        let error = SessionState::load(root.path()).expect_err("format 2 is not read");
        assert!(error.to_string().contains("format 2"), "{error}");
    }
}
