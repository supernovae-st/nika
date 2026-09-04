// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The resident, read from outside (ADR-132 · #1352): the stamps its two
//! stores carry and whether a resident holds the server lease right now.
//! `nika doctor`'s `resident` line is this report, rendered; nothing here
//! takes a lease the resident could need.

use std::path::Path;

use serde::Deserialize;

use crate::writer::WriterStamp;

const JOBS_STATE: &str = "jobs/state.json";
const SCHEDULES_STATE: &str = "schedules/state.json";
const SERVER_LOCK: &str = "jobs/server.lock";
/// The bounded read: a stamp lives in the first bytes of a state file that
/// can weigh megabytes; the probe reads the whole file but never more than
/// the store's own ceiling.
const MAX_PROBE_BYTES: u64 = 64 * 1024 * 1024;

/// What the doctor learns about a resident's stores.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ResidentReport {
    /// The engine that last wrote the job store, when the store carries a
    /// stamp (a store written before ADR-132 carries none).
    pub jobs_writer: Option<WriterStamp>,
    /// The engine that last wrote the schedule store, when stamped.
    pub schedules_writer: Option<WriterStamp>,
    /// Whether a resident holds the server lease on this host right now.
    pub alive: bool,
}

impl ResidentReport {
    /// The writer both stores agree on, else the job store's, else the
    /// schedule store's.
    #[must_use]
    pub fn writer(&self) -> Option<&WriterStamp> {
        self.jobs_writer.as_ref().or(self.schedules_writer.as_ref())
    }
}

#[derive(Deserialize)]
struct StampProbe {
    #[serde(default)]
    writer: Option<WriterStamp>,
}

/// Read the resident's report under `state_root` (the `--state-root` ·
/// `<cwd>/.nika/serve` by default). `None` when no resident store exists.
#[must_use]
pub fn inspect(state_root: &Path) -> Option<ResidentReport> {
    let jobs = state_root.join(JOBS_STATE);
    let schedules = state_root.join(SCHEDULES_STATE);
    if !jobs.exists() && !schedules.exists() {
        return None;
    }
    Some(ResidentReport {
        jobs_writer: read_stamp(&jobs),
        schedules_writer: read_stamp(&schedules),
        alive: lease_held(&state_root.join(SERVER_LOCK)),
    })
}

fn read_stamp(path: &Path) -> Option<WriterStamp> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_PROBE_BYTES {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<StampProbe>(&text).ok()?.writer
}

/// Whether a resident holds the server lease: a live resident holds it
/// exclusively for its lifetime, so a non-blocking SHARED attempt is
/// refused with `EWOULDBLOCK`. A free lock is taken and released at once.
fn lease_held(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use nix::fcntl::{Flock, FlockArg};
        let Ok(file) = std::fs::OpenOptions::new().read(true).open(path) else {
            return false;
        };
        match Flock::lock(file, FlockArg::LockSharedNonblock) {
            Ok(_released_at_once) => false,
            Err((_, nix::errno::Errno::EWOULDBLOCK)) => true,
            Err(_) => false,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    /// No store → no report; a fresh store → this engine's stamp on both
    /// stores and no resident alive; a claimed incarnation → alive.
    #[test]
    fn the_report_reads_the_stamps_and_the_lease() {
        let root = tempfile::tempdir().expect("root");
        assert_eq!(inspect(root.path()), None, "no store, no report");
        let jobs = crate::JobStore::open(root.path()).expect("job store");
        let _schedules = crate::ScheduleStore::open(root.path()).expect("schedule store");
        let report = inspect(root.path()).expect("a report");
        assert_eq!(report.jobs_writer, Some(WriterStamp::this_engine()));
        assert_eq!(report.schedules_writer, Some(WriterStamp::this_engine()));
        assert!(!report.alive, "nobody claimed the server lease");
        let incarnation = jobs.claim_server_incarnation().expect("claim");
        assert!(
            inspect(root.path()).expect("report").alive,
            "the lease is held"
        );
        drop(incarnation);
        assert!(!inspect(root.path()).expect("report").alive, "released");
    }
}
