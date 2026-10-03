// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a run left, acquired by the Live host adapter when the human opens
//! it (never while drawing), only below the project's root and only for a
//! path the host itself observed the run name:
//!
//! - a file the run reported writing, read NOW through an owned directory
//!   (no symlink at any component, regular files only), at most
//!   [`FILE_CAP`] bytes, witnessed. The run left no digest of what it wrote,
//!   so these are today's bytes at that path, never called the run's own,
//!   never « unchanged » nor « changed » since the run.
//! - the Proof of the trace its settlement named: the journal is captured
//!   ONCE (relative, under `.nika/traces/`, a regular file, at most
//!   [`JOURNAL_CAP`] bytes, witnessed), then the one verifier judges those
//!   very bytes (`trace_verify::verify_captured`, its `--json` document) and
//!   the run's fold reads the same bytes (`RunFacts::of`). The verdict is
//!   bound to the run only when the journal names exactly that execution,
//!   one start naming the run's own source hash, and the head and length
//!   its receipt named. The keys, the anchor sidecar and the writer lease
//!   are the verifier's own context, read by their owners when it judges;
//!   the witness covers the journal only. A verified journal records what
//!   happened; it never proves the work was right.
//!
//! A path that leaves the root, a symlink, a special file, a missing file or
//! one over its cap is refused with its reason, never read.

use std::path::{Component, Path, PathBuf};

use nika_display::run_story::ExecutionId;
use nika_session::change::Witness;
use nika_trace::run_view::RunFacts;
use nika_trace::trace_verify::{VerifyOptions, verify_captured};
use serde_json::Value;

/// The most bytes one produced file is read.
pub const FILE_CAP: u64 = 1 << 20;
/// The most bytes of a journal the Proof face captures: an interactive
/// bound, below the verifier's own (`JOURNAL_BOUND`); a larger journal is
/// refused here, `nika trace verify` judges it whole.
pub const JOURNAL_CAP: u64 = 8 << 20;

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
    pub(crate) fn refused(path: &str, why: impl Into<String>) -> Self {
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

/// What the host observed of the run whose journal it captures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Expect {
    pub execution: ExecutionId,
    pub workflow_sha256: Option<String>,
    pub chain_head: Option<String>,
    pub chain_len: Option<u64>,
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
}

impl Proven {
    pub(crate) fn refused(trace: &str, why: impl Into<String>) -> Self {
        Self {
            trace: trace.to_owned(),
            witness: None,
            doc: None,
            terminal: None,
            unbound: Vec::new(),
            why: Some(why.into()),
        }
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
}

#[cfg(test)]
impl Proven {
    /// A Proof as the verifier judged it, for the faces' tests.
    pub(crate) fn judged(trace: &str, doc: Value, unbound: Vec<String>) -> Self {
        Self {
            trace: trace.to_owned(),
            witness: Some("ab".repeat(32)),
            doc: Some(doc),
            terminal: Some("succeeded".to_owned()),
            unbound,
            why: None,
        }
    }
}

/// `path` as a relative path inside the root: `./` dropped, nothing absolute,
/// no `..`, not empty.
fn inside(path: &str) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for part in Path::new(path).components() {
        match part {
            Component::Normal(name) => out.push(name),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

/// At most `cap` bytes of `rel` below `root`: `Ok(None)` when absent, an
/// error when refused or over the cap.
fn capture(root: &Path, rel: &Path, cap: u64) -> Result<Option<Vec<u8>>, String> {
    // The selected project root may be reached through a link (the
    // operator's own): resolved once, then held; below it nothing is followed.
    let root = std::fs::canonicalize(root).map_err(|e| format!("the project root: {e}"))?;
    let opened = nika_fs::OwnedDir::open(&root).and_then(|dir| dir.open_relative(rel));
    let read = opened.and_then(|mut file| nika_fs::read_capped(&mut file, cap));
    match read {
        Ok(capped) if capped.over => Err(format!("larger than {cap} bytes")),
        Ok(capped) => Ok(Some(capped.bytes.to_vec())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// The file at `path` below `root`, read now within [`FILE_CAP`].
pub(crate) fn fetch(root: &Path, path: &str) -> Fetched {
    let Some(rel) = inside(path) else {
        return Fetched::refused(path, "not a path inside the project");
    };
    match capture(root, &rel, FILE_CAP) {
        Ok(Some(bytes)) => Fetched {
            path: path.to_owned(),
            witness: Some(Witness::of(&bytes).0),
            bytes: Some(bytes),
            missing: false,
            why: None,
        },
        Ok(None) => Fetched {
            missing: true,
            ..Fetched::refused(path, "nothing is at this path now")
        },
        Err(why) => Fetched::refused(path, why),
    }
}

/// The Proof of the journal at `trace` below `root`: captured once, judged
/// and folded over the same bytes, bound to `expect` or said why not.
pub(crate) fn prove(root: &Path, trace: &str, expect: &Expect) -> Proven {
    let Some(rel) = inside(trace).filter(|r| r.starts_with(".nika/traces")) else {
        return Proven::refused(trace, "not a trace under .nika/traces/ of this project");
    };
    let raw = match capture(root, &rel, JOURNAL_CAP) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return Proven::refused(trace, "no journal is at this path now"),
        Err(why) => return Proven::refused(trace, why),
    };
    let witness = Witness::of(&raw).0;
    let Ok(raw) = String::from_utf8(raw) else {
        return Proven::refused(trace, "the journal is not UTF-8");
    };
    let original = root.join(&rel);
    let opts = VerifyOptions {
        json: true,
        ..VerifyOptions::default()
    };
    let judged = verify_captured(&original.to_string_lossy(), &raw, &opts);
    let doc = judged
        .text
        .lines()
        .last()
        .and_then(|l| serde_json::from_str(l).ok());
    let facts = RunFacts::of(&original, &raw);
    let unbound = binding(facts.as_ref(), doc.as_ref(), expect);
    Proven {
        trace: trace.to_owned(),
        witness: Some(witness),
        terminal: facts.as_ref().and_then(|f| f.terminal().map(str::to_owned)),
        doc,
        unbound,
        why: None,
    }
}

/// Every reason the captured journal is not exactly the observed run's.
fn binding(facts: Option<&RunFacts>, doc: Option<&Value>, expect: &Expect) -> Vec<String> {
    let mut why = Vec::new();
    let Some(facts) = facts else {
        return vec!["the journal holds no frame the fold reads".to_owned()];
    };
    match (facts.executions(), facts.unidentified()) {
        ([one], 0) if *one == expect.execution => {}
        ([one], 0) => why.push(format!("the journal records another execution ({one})")),
        (all, 0) => why.push(format!(
            "the journal records {} executions, not one",
            all.len()
        )),
        (_, n) => why.push(format!("{n} frame(s) of the journal name no execution")),
    }
    match (facts.starts(), expect.workflow_sha256.as_deref()) {
        ([Some(named)], Some(seen)) if named == seen => {}
        ([_], None) => {
            why.push("the run's own start was not observed: its source is not compared".to_owned());
        }
        ([None], Some(_)) => {
            why.push(
                "the journal's start names no source hash: its source is not compared".to_owned(),
            );
        }
        ([_], Some(_)) => {
            why.push("the journal's start names other bytes than the run's start".to_owned());
        }
        (starts, _) => why.push(format!(
            "the journal holds {} starts, not one",
            starts.len()
        )),
    }
    let chain = doc.and_then(|d| d.get("chain"));
    let head = chain.and_then(|c| c.get("head")).and_then(Value::as_str);
    let events = chain.and_then(|c| c.get("events")).and_then(Value::as_u64);
    match (expect.chain_head.as_deref(), expect.chain_len) {
        (Some(h), Some(n)) if head == Some(h) && events == Some(n) => {}
        (Some(_), Some(_)) if head.is_none() => why.push(
            "the verifier judged no chain: the end its receipt named is not compared".to_owned(),
        ),
        (Some(_), Some(_)) => {
            why.push("the journal ends elsewhere than its receipt named".to_owned());
        }
        _ => why
            .push("the settlement named no receipt: the journal's end is not compared".to_owned()),
    }
    why
}

#[cfg(test)]
#[cfg(unix)]
#[allow(clippy::expect_used, clippy::panic)]
mod acquire_tests;
