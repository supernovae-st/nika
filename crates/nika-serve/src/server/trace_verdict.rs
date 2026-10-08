// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `GET /v1/jobs/{id}/trace/verify` — the door's verdict on the journal the
//! resident wrote for the job, through the ONE verifier `nika trace verify`
//! runs and its projection for a door (`nika_trace::trace_verify::door_verdict`).
//! The door locates the file by the job's identity; it never judges a chain
//! itself and never carries a filesystem path.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::JobRecord;

/// The journal a job's record names: the backend's journal directory and
/// the job's execution + trace identity.
pub(super) struct JournalKey {
    dir: PathBuf,
    execution: String,
    trace: String,
}

impl JournalKey {
    /// `None` while the record names no execution (a queued job).
    pub(super) fn of(dir: &Path, record: &JobRecord) -> Option<Self> {
        Some(Self {
            dir: dir.to_path_buf(),
            execution: record.execution_id()?.to_owned(),
            trace: record.trace_id()?.to_owned(),
        })
    }

    /// Locate the journal and verify it (blocking · the fs). `None` when no
    /// journal exists for this job — the route's `unavailable`.
    pub(super) fn verify(&self) -> Option<Value> {
        let path = nika_dap::store::locate_trace(&self.dir, &self.execution, &self.trace)?;
        Some(nika_trace::trace_verify::door_verdict(&path, &self.trace))
    }
}
