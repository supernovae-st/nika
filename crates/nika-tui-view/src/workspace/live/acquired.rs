// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a run left, as the renderer's host acquired it when the person opened it: a file the
//! run reported writing, read now; the Proof of the journal its settlement named; and the
//! journal of a child run a task called, judged and compared. These are the values the live
//! run's faces read; the host that reads the files and the journals, and the verifier that
//! judges them, stay in the renderer (`nika_tui::session::acquire`). Nothing here reads.

use std::sync::Arc;

use nika_display::run_story::Event;
use nika_display::state::TaskRow;
use serde_json::Value;

/// A file the run reported writing, as read now.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Fetched {
    path: String,
    bytes: Option<Vec<u8>>,
    witness: Option<String>,
    missing: bool,
    why: Option<String>,
}

impl Fetched {
    /// The bytes read now at `path`, with their witness (blake3, hex).
    #[must_use]
    pub fn read(path: &str, bytes: Vec<u8>, witness: String) -> Self {
        Self {
            path: path.to_owned(),
            bytes: Some(bytes),
            witness: Some(witness),
            missing: false,
            why: None,
        }
    }

    /// Nothing is at `path` now.
    #[must_use]
    pub fn absent(path: &str) -> Self {
        Self {
            missing: true,
            ..Self::refused(path, "nothing is at this path now")
        }
    }

    /// `path` was not read, and why.
    #[must_use]
    pub fn refused(path: &str, why: impl Into<String>) -> Self {
        Self {
            path: path.to_owned(),
            bytes: None,
            witness: None,
            missing: false,
            why: Some(why.into()),
        }
    }

    /// The path the run named.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The bytes read now, when they could be read.
    #[must_use]
    pub fn bytes(&self) -> Option<&[u8]> {
        self.bytes.as_deref()
    }

    /// Their witness (blake3, hex).
    #[must_use]
    pub fn witness(&self) -> Option<&str> {
        self.witness.as_deref()
    }

    /// Nothing is at that path now.
    #[must_use]
    pub fn missing(&self) -> bool {
        self.missing
    }

    /// Why the bytes were not read, when they were not.
    #[must_use]
    pub fn why(&self) -> Option<&str> {
        self.why.as_deref()
    }
}

/// The Proof of one captured journal: the verifier's verdict over its bytes
/// and whether that journal is the run's own.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Proven {
    trace: String,
    witness: Option<String>,
    doc: Option<Value>,
    terminal: Option<String>,
    unbound: Vec<String>,
    why: Option<String>,
    /// The run's events from these very bytes, when they are its verified
    /// journal; else why nothing historical is shown.
    events: Option<Arc<[Event]>>,
    projection_why: Option<String>,
}

impl Proven {
    /// Nothing of the journal at `trace` was judged, and why.
    #[must_use]
    pub fn refused(trace: &str, why: impl Into<String>) -> Self {
        Self {
            trace: trace.to_owned(),
            witness: None,
            doc: None,
            terminal: None,
            unbound: Vec::new(),
            why: Some(why.into()),
            events: None,
            projection_why: None,
        }
    }

    /// The journal at `trace`, captured once and judged: the witness of its bytes, the
    /// verifier's `--json` verdict over them and the terminal word its fold read, with every
    /// reason it is not the run's own (`unbound`, empty when it is). It lends no event until
    /// [`Self::lent`] says what it lends.
    #[must_use]
    pub fn verified(
        trace: impl Into<String>,
        witness: String,
        (doc, terminal): (Option<Value>, Option<String>),
        unbound: Vec<String>,
    ) -> Self {
        Self {
            trace: trace.into(),
            witness: Some(witness),
            doc,
            terminal,
            unbound,
            why: None,
            events: None,
            projection_why: None,
        }
    }

    /// The run's events this Proof lends from the very bytes it judged, or why it lends none.
    #[must_use]
    pub fn lent(mut self, events: Option<Arc<[Event]>>, why: Option<String>) -> Self {
        self.events = events;
        self.projection_why = why;
        self
    }

    /// The run's events decoded from the captured bytes, only when they are
    /// its verified journal (the fold and the ledger read these).
    #[must_use]
    pub fn events(&self) -> Option<&[Event]> {
        self.events.as_deref()
    }

    /// Why the captured journal lends no event, when it was read but lends none.
    #[must_use]
    pub fn projection_why(&self) -> Option<&str> {
        self.projection_why.as_deref()
    }

    /// This Proof lending nothing: its verdict, witness and reasons kept,
    /// `why` said for its projection (a reading the host did not adopt).
    #[must_use]
    pub fn without_lending(mut self, why: impl Into<String>) -> Self {
        self.events = None;
        self.projection_why = Some(why.into());
        self
    }

    /// The trace the settlement named.
    #[must_use]
    pub fn trace(&self) -> &str {
        &self.trace
    }

    /// The witness of the captured journal bytes (blake3, hex).
    #[must_use]
    pub fn witness(&self) -> Option<&str> {
        self.witness.as_deref()
    }

    /// The verifier's versioned `--json` verdict over those bytes.
    #[must_use]
    pub fn verdict(&self) -> Option<&Value> {
        self.doc.as_ref()
    }

    /// The attained tier the verdict names (`ok` · `sealed` · …, or a refusal class).
    #[must_use]
    pub fn tier(&self) -> Option<&str> {
        self.doc.as_ref()?.get("tier")?.as_str()
    }

    /// The exit class the verdict names (0 = the reported tier holds).
    #[must_use]
    pub fn exit(&self) -> Option<u64> {
        self.doc.as_ref()?.get("exit")?.as_u64()
    }

    /// The journal's terminal word, when it holds a terminal frame.
    #[must_use]
    pub fn terminal(&self) -> Option<&str> {
        self.terminal.as_deref()
    }

    /// Why the verdict is not the run's own (empty: it is).
    #[must_use]
    pub fn unbound(&self) -> &[String] {
        &self.unbound
    }

    /// Why nothing was judged, when nothing was.
    #[must_use]
    pub fn why(&self) -> Option<&str> {
        self.why.as_deref()
    }

    /// This Proof lending `events`, as a verified journal of the run does: the faces' and the
    /// fold's test fixtures, in this crate and the renderer's.
    #[doc(hidden)]
    #[must_use]
    pub fn lending(mut self, events: Vec<Event>) -> Self {
        self.events = Some(events.into());
        self
    }

    /// A Proof as the verifier judged it, for the faces' tests here and in the renderer.
    #[doc(hidden)]
    #[must_use]
    pub fn judged(trace: &str, doc: Value, unbound: Vec<String>) -> Self {
        Self::verified(
            trace,
            "ab".repeat(32),
            (Some(doc), Some("succeeded".to_owned())),
            unbound,
        )
    }
}

/// A child journal captured from the relation its parent's settle named,
/// judged, folded and compared with each engagement of that relation.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ChildRead {
    proven: Proven,
    compared: Vec<(bool, String)>,
    rows: Vec<TaskRow>,
}

impl ChildRead {
    /// The child journal at `trace` was not read, and why.
    #[must_use]
    pub fn refused(trace: &str, why: impl Into<String>) -> Self {
        Self {
            proven: Proven::refused(trace, why),
            compared: Vec::new(),
            rows: Vec::new(),
        }
    }

    /// The child journal judged (`proven`), each engagement of its relation compared, and its
    /// tasks folded from the same captured bytes.
    #[must_use]
    pub fn new(proven: Proven, compared: Vec<(bool, String)>, rows: Vec<TaskRow>) -> Self {
        Self {
            proven,
            compared,
            rows,
        }
    }

    /// The verifier's verdict over the captured bytes, or why none was read;
    /// its `unbound` lists every engagement that does not hold.
    #[must_use]
    pub fn proven(&self) -> &Proven {
        &self.proven
    }

    /// Each engagement, compared: `true` only when it holds; otherwise the
    /// words say whether it differs or was not compared.
    #[must_use]
    pub fn compared(&self) -> &[(bool, String)] {
        &self.compared
    }

    /// The child's tasks, folded from the same captured bytes.
    #[must_use]
    pub fn rows(&self) -> &[TaskRow] {
        &self.rows
    }
}
