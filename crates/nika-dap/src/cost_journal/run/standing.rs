// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a host that admits Runs under an operator's standing cost policy keeps
//! beside the cost journal. Both are written under the cost lease a
//! [`Cleared`] project holds, so one writer at a time changes them.
//!
//! The standing ledger (`.nika/standing-cost-runs.json`) records when each Run
//! of a standing rule's route was admitted, so the rule's Runs-per-window
//! bound holds across restarts of its host. It is a count the host enforces,
//! never billing evidence: the journal and the trace stay that. An admission
//! forgets the route's Runs that left the window it was given, so a rule
//! rewritten with a longer window counts only what the shorter one kept.
//!
//! The archive keeps unattended Runs from filling the journal, which every
//! admission reads whole and refuses past its read bound. A cleared journal
//! (every Run in it settled clear or was reconciled, so no row of it bears on
//! a later admission) that leaves too little room under that bound is copied,
//! byte for byte, to a file named by its digest, and only then emptied.
//! Evidence moves; it is never dropped or rewritten, and a journal that still
//! blocks is never archived (it is not [`Cleared`]).

use super::super::READ_BOUND;
use super::Cleared;
use nika_fs::OwnedDir;
use std::collections::BTreeMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The standing ledger's name under the project's `.nika/` directory.
pub const STANDING_LEDGER: &str = "standing-cost-runs.json";

const SCHEMA: &str = "nika/standing-cost-runs@1";

/// The ledger is read whole and refuses past this, and is never written past
/// it: a rule admits at most 100,000 Runs per window, each one a number of
/// seconds.
const LEDGER_BOUND: u64 = 4 * 1024 * 1024;

/// A standing rule's window that admits no further Run now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct WindowFull {
    /// The Runs the ledger records within the window.
    pub admitted: u32,
    /// When the oldest of them leaves the window, in Unix seconds: the next
    /// Run is admitted from then.
    pub reopens_at: u64,
}

/// When each Run of a route was admitted, in Unix seconds, by route.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Ledger {
    schema: String,
    runs: BTreeMap<String, Vec<u64>>,
}

fn invalid(reason: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, reason)
}

fn unix_seconds(now: SystemTime) -> std::io::Result<u64> {
    now.duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .map_err(|_| invalid("the host clock reads before 1970: no standing window can be counted"))
}

/// Whether a Run admitted at `at` still counts at `now`: it is younger than
/// the window. A time after `now` (the clock moved back) counts too.
fn counts(at: u64, now: u64, window: u64) -> bool {
    now.saturating_sub(at) < window
}

/// The Runs of `route` that the standing ledger of the project at `root`
/// records within `window` before `now`, read without the cost lease. The
/// ledger is replaced atomically, so a reader sees one whole state of it. This
/// is the count for a host that only asks whether a rule's window is open
/// (it admits nothing and writes nothing); an admission counts under the
/// lease ([`Cleared::admit_standing_run`]).
///
/// # Errors
/// A ledger that cannot be read whole or is not one this engine wrote: its
/// count is unknown, and a host must refuse rather than assume zero.
pub fn standing_runs(
    root: &OwnedDir,
    route: &str,
    window: Duration,
    now: SystemTime,
) -> std::io::Result<u32> {
    let (now, window) = (unix_seconds(now)?, window.as_secs());
    let nika = match root.open_below(&[".nika"]) {
        Ok(nika) => nika,
        Err(absent) if absent.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    let ledger = read_ledger(&nika)?;
    let runs = ledger.runs.get(route).map_or(0, |times| {
        times.iter().filter(|at| counts(**at, now, window)).count()
    });
    Ok(u32::try_from(runs).unwrap_or(u32::MAX))
}

/// The ledger under `nika`, or an empty one when none was written yet.
fn read_ledger(nika: &OwnedDir) -> std::io::Result<Ledger> {
    let over = "the standing ledger exceeds its read bound";
    let Some(text) = nika.read_capped_below(&[], STANDING_LEDGER, LEDGER_BOUND, over)? else {
        return Ok(Ledger {
            schema: SCHEMA.to_owned(),
            runs: BTreeMap::new(),
        });
    };
    serde_json::from_str::<Ledger>(&text)
        .ok()
        .filter(|ledger| ledger.schema == SCHEMA)
        .ok_or_else(|| invalid("the standing ledger is unreadable: its Runs cannot be counted"))
}

impl Cleared {
    /// Count one more Run of `route` at `now`, unless `max_runs` of them were
    /// already admitted within `window`: the Run is recorded before it starts,
    /// so a Run that then fails to start still counts (never the reverse). On
    /// success, the Runs now within the window, this one included.
    ///
    /// # Errors
    /// As [`standing_runs`], a ledger that cannot be replaced safely, or one
    /// this Run would carry past its read bound (it is never written
    /// unreadable): nothing was counted, and the host must not start the Run.
    pub fn admit_standing_run(
        &self,
        route: &str,
        max_runs: u32,
        window: Duration,
        now: SystemTime,
    ) -> std::io::Result<Result<u32, WindowFull>> {
        let (now, window) = (unix_seconds(now)?, window.as_secs());
        let mut ledger = read_ledger(&self.nika)?;
        let times = ledger.runs.entry(route.to_owned()).or_default();
        times.retain(|at| counts(*at, now, window));
        let admitted = u32::try_from(times.len()).unwrap_or(u32::MAX);
        if admitted >= max_runs {
            let oldest = times.iter().copied().min().unwrap_or(now);
            return Ok(Err(WindowFull {
                admitted,
                reopens_at: oldest.saturating_add(window),
            }));
        }
        times.push(now);
        let body = serde_json::to_string(&ledger).map_err(std::io::Error::other)?;
        if u64::try_from(body.len()).unwrap_or(u64::MAX) > LEDGER_BOUND {
            return Err(invalid(
                "the standing ledger is full: no further Run can be counted until earlier ones leave their windows",
            ));
        }
        self.nika.write_atomic(STANDING_LEDGER, &body)?;
        Ok(Ok(admitted.saturating_add(1)))
    }

    /// Make room for a Run whose rows may take up to `reserve` bytes: when
    /// this cleared journal would pass its read bound with them, it is
    /// archived whole first. Its exact bytes are written to
    /// `inference-cost-observations.<sha256, 16 hex>.ndjson` beside it and
    /// made durable, and only then is the journal emptied. The archive's name,
    /// or `None` when the journal already leaves that room. A repeat after an
    /// interrupted archive writes the same file and finishes it.
    ///
    /// # Errors
    /// The journal cannot be read, the archive cannot be made durable (the
    /// journal is then left untouched), or the journal cannot be emptied.
    pub fn make_room(&self, reserve: u64) -> std::io::Result<Option<String>> {
        self.archive_over(READ_BOUND.saturating_sub(reserve))
    }

    /// Archive this cleared journal when it holds more than `threshold` bytes.
    fn archive_over(&self, threshold: u64) -> std::io::Result<Option<String>> {
        let bytes = self.lease.read()?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) <= threshold {
            return Ok(None);
        }
        // A cleared journal is whole lines of UTF-8 JSON; anything else stays put.
        let Ok(text) = String::from_utf8(bytes) else {
            return Ok(None);
        };
        let digest = nika_event::source_id::sha256_hex(text.as_bytes());
        let name = format!(
            "inference-cost-observations.{}.ndjson",
            digest.get(..16).unwrap_or(&digest)
        );
        self.nika.write_atomic(&name, &text)?;
        self.lease.reset()?;
        Ok(Some(name))
    }
}

#[cfg(test)]
mod tests;
