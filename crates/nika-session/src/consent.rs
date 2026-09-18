// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Durable evidence of a consent (#1465 · ADR-126 « a consent is a trusted
//! session event »): an append-only `.nika/consents.ndjson` under the
//! PROJECT — what was previewed (the proposal's identity · every path with
//! the witness of the bytes it was previewed over and the witness of the
//! bytes it lands) · what landed · when. The runtime writes it at apply;
//! never the transcript (that is the history under the home). One JSON
//! object per line, readable by any `nika trace`-class reader.

use std::io::{self, Read as _};
use std::path::{Path, PathBuf};

use nika_fs::OwnedDir;
use serde::{Deserialize, Serialize};

use crate::change::{ProjectChangeSet, Witness};
use crate::outcome::ProposalId;

/// The journal's name under the project's `.nika/`.
pub const CONSENTS_FILE: &str = "consents.ndjson";
const NIKA_DIR: &str = ".nika";
/// The most bytes a reader takes from the journal (a bound, not a quota).
const MAX_JOURNAL_BYTES: u64 = 16 * 1024 * 1024;

/// What the consent decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConsentDecision {
    /// Every change landed.
    Applied,
    /// A later write was refused after earlier ones landed: `written`
    /// names exactly what did (the write loop's own record, never a tree
    /// scan); the proposal stays undecided.
    Partial,
}

/// One change as the human saw it and as it landed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct ConsentWitness {
    /// Relative to the project root.
    pub path: PathBuf,
    /// The witness of the bytes the preview was built over; none for a
    /// create (the preview promised absence).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// The witness of the exact bytes the change lands.
    pub after: String,
}

/// One line of the journal: one consent, decided.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct ConsentRecord {
    /// The line's format ([`ConsentRecord::VERSION`]).
    pub version: u8,
    /// When the consent was applied (RFC 3339 · UTC).
    pub at: String,
    /// The proposal the consent named — the witness of the exact preview
    /// the human saw (hex).
    pub proposal: String,
    /// What the consent decided.
    pub decision: ConsentDecision,
    /// Every change of the set, in set order.
    pub witnesses: Vec<ConsentWitness>,
    /// The paths this consent landed, in write order.
    pub written: Vec<PathBuf>,
    /// The workflow the human asked to run with the change, when they did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<PathBuf>,
}

impl ConsentRecord {
    /// The current line format.
    pub const VERSION: u8 = 1;

    /// The record of a consent over `set`, decided at `at`.
    #[must_use]
    pub fn of(
        set: &ProjectChangeSet,
        id: &ProposalId,
        decision: ConsentDecision,
        written: &[PathBuf],
        at: String,
    ) -> Self {
        Self {
            version: Self::VERSION,
            at,
            proposal: id.as_str().to_owned(),
            decision,
            witnesses: set
                .changes
                .iter()
                .map(|change| ConsentWitness {
                    path: change.path(),
                    before: change.witness().map(|w| w.0.clone()),
                    after: Witness::of(change.content().as_bytes()).0,
                })
                .collect(),
            written: written.to_vec(),
            run: set.run.as_ref().map(|run| run.workflow.clone()),
        }
    }

    /// Append this line to `<root>/.nika/consents.ndjson` (the directory
    /// and the journal are created on first use, below the root's own
    /// descriptor).
    ///
    /// # Errors
    /// The file system's refusal; nothing is retried.
    pub fn append(&self, root: &Path) -> io::Result<()> {
        let dir = OwnedDir::open(root)?.create_below(&[NIKA_DIR])?;
        let line = serde_json::to_string(self)?;
        dir.append_line(CONSENTS_FILE, &line)
    }

    /// Every line of the journal, oldest first — empty when no consent was
    /// ever applied under this root.
    ///
    /// # Errors
    /// A journal beyond 16 MiB, a line that is not a record, or the file
    /// system's refusal.
    pub fn read_all(root: &Path) -> io::Result<Vec<Self>> {
        let dir = match OwnedDir::open(root).and_then(|dir| dir.open_below(&[NIKA_DIR])) {
            Ok(dir) => dir,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let file = match dir.open_relative(Path::new(CONSENTS_FILE)) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let mut text = String::new();
        file.take(MAX_JOURNAL_BYTES + 1).read_to_string(&mut text)?;
        if text.len() as u64 > MAX_JOURNAL_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the consent journal exceeds 16 MiB; preserve it for migration",
            ));
        }
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).map_err(io::Error::from))
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::change::ProjectChange;

    fn set(root: &Path) -> ProjectChangeSet {
        ProjectChangeSet {
            root: root.to_path_buf(),
            goal: "a brief".to_owned(),
            changes: vec![
                ProjectChange::CreateWorkflow {
                    path: PathBuf::from("brief.nika"),
                    content: "nika: brief\n".to_owned(),
                },
                ProjectChange::UpdateProjectFile {
                    before: Witness::of(b"nika: old\n"),
                    content: "nika: new\n".to_owned(),
                },
            ],
            run: Some(crate::change::RunRequest {
                workflow: PathBuf::from("brief.nika"),
                vars: Vec::new(),
                max_cost_usd: 0.05,
            }),
            repairs: Vec::new(),
            audits: Vec::new(),
        }
    }

    #[test]
    fn a_record_carries_the_preview_and_what_landed() {
        let root = tempfile::tempdir().expect("root");
        let set = set(root.path());
        let id = ProposalId::of("the preview");
        let record = ConsentRecord::of(
            &set,
            &id,
            ConsentDecision::Applied,
            &[PathBuf::from("brief.nika"), PathBuf::from("nika.yaml")],
            "2026-09-13T19:09:09Z".to_owned(),
        );
        assert_eq!(record.version, 1);
        assert_eq!(record.proposal, id.as_str());
        assert_eq!(record.witnesses.len(), 2);
        assert_eq!(record.witnesses[0].path, PathBuf::from("brief.nika"));
        assert_eq!(
            record.witnesses[0].before, None,
            "a create witnesses absence"
        );
        assert_eq!(record.witnesses[0].after, Witness::of(b"nika: brief\n").0);
        assert_eq!(record.witnesses[1].path, PathBuf::from("nika.yaml"));
        assert_eq!(
            record.witnesses[1].before.as_deref(),
            Some(Witness::of(b"nika: old\n").0.as_str())
        );
        assert_eq!(record.run, Some(PathBuf::from("brief.nika")));
        let line = serde_json::to_string(&record).expect("one line");
        assert!(!line.contains('\n'));
        let value: serde_json::Value = serde_json::from_str(&line).expect("json");
        assert_eq!(value["decision"], "applied");
        assert_eq!(value["at"], "2026-09-13T19:09:09Z");
        assert_eq!(value["written"][1], "nika.yaml");
    }

    #[test]
    fn the_journal_is_append_only_under_the_project_and_reads_back_in_order() {
        let root = tempfile::tempdir().expect("root");
        assert!(
            ConsentRecord::read_all(root.path())
                .expect("no journal is empty")
                .is_empty()
        );
        let set = set(root.path());
        let first = ConsentRecord::of(
            &set,
            &ProposalId::of("one"),
            ConsentDecision::Applied,
            &[PathBuf::from("brief.nika")],
            "2026-09-13T19:09:09Z".to_owned(),
        );
        let second = ConsentRecord::of(
            &set,
            &ProposalId::of("two"),
            ConsentDecision::Partial,
            &[],
            "2026-09-13T19:10:00Z".to_owned(),
        );
        first.append(root.path()).expect("first line");
        second.append(root.path()).expect("second line");
        let journal = root.path().join(".nika").join(CONSENTS_FILE);
        let text = std::fs::read_to_string(&journal).expect("the journal");
        assert_eq!(text.lines().count(), 2, "one line per consent: {text}");
        assert!(text.ends_with('\n'));
        assert_eq!(
            ConsentRecord::read_all(root.path()).expect("reads back"),
            vec![first, second]
        );
        std::fs::write(&journal, format!("{text}not json\n")).expect("damage");
        assert!(
            ConsentRecord::read_all(root.path()).is_err(),
            "a damaged line is named, never skipped"
        );
    }
}
