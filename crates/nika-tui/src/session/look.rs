// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The look the Live host adapter takes when the human opens a workflow in the
//! workspace: an acquisition of view, never an authority. The Session lists
//! the workflows (its runtime's snapshot); this reads ONE of them, exactly as
//! listed, once: below the snapshot's root through an owned directory (no
//! symlink at any component, regular files only), at most [`LOOK_CAP`] bytes,
//! UTF-8 text. The bytes are witnessed with the Session's own witness, then
//! judged by the shared check facade (`oracle::audit_source`) with no reader
//! and no skills base: the file ALONE, so nothing it imports is read here and
//! every audited look says what it left out ([`super::super::workspace::inspect`]).
//! The graph is projected from that same audit.
//!
//! This runs on the shell's thread when the human opens or re-reads an entry,
//! never while drawing; the look joins no reasoner facts, no consent, no Save
//! and no Run. The complete closure (children, skills, registry) belongs to
//! the execution snapshot's capture, not to this view.

use std::path::Path;

use nika_cli_host::oracle::{AuditOptions, audit_source};
use nika_session::ProjectSnapshot;
use nika_session::change::Witness;

use crate::workspace::inspect::Inspected;

/// The most bytes one look reads of a workflow.
pub(crate) const LOOK_CAP: u64 = 1 << 20;

/// Look at the workflow `snapshot` lists at exactly `path`; `None` when it
/// lists none there (a needle, an escaping or an unlisted path).
pub(crate) fn take(snapshot: &ProjectSnapshot, path: &str) -> Option<Inspected> {
    let listed = snapshot.workflows.iter().find(|w| w.path == path)?;
    let path = listed.path.clone();
    let read = nika_fs::OwnedDir::open(&snapshot.root)
        .and_then(|dir| dir.open_relative(Path::new(&path)))
        .and_then(|mut file| nika_fs::read_capped(&mut file, LOOK_CAP))
        .map_err(|e| e.to_string());
    let bytes = match read {
        Ok(capped) if capped.over => {
            return Some(Inspected::unread(
                path,
                format!("larger than {LOOK_CAP} bytes"),
            ));
        }
        Ok(capped) => capped.bytes.to_vec(),
        Err(why) => return Some(Inspected::unread(path, why)),
    };
    let witness = Witness::of(&bytes).0;
    let Ok(source) = String::from_utf8(bytes) else {
        return Some(Inspected::unread(path, "not UTF-8 text"));
    };
    // The file alone: no reader, no skills base (see the module docs).
    let judged = audit_source(&source, &path, None, None, AuditOptions::default());
    let look = match &judged {
        Ok(audit) => {
            let graph = nika_display::dag_art::project(&audit.wf, &audit.report);
            Inspected::read(path, witness, source, Ok((audit, graph)))
        }
        Err(refusal) => {
            let said = refusal.diagnostic();
            let refused = Err((said.code.to_string(), said.message));
            Inspected::read(path, witness, source, refused)
        }
    };
    Some(look)
}

#[cfg(test)]
#[cfg(unix)]
#[allow(clippy::expect_used, clippy::panic)]
mod look_tests;
