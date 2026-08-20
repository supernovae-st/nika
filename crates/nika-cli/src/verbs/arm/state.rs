// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! L4 filesystem adapter for the firing sidecar (D3):
//!
//! ```text
//! .nika/arm/<label>/lock              live beat owner
//! .nika/arm/<label>/ledger.lock       verify + append critical section
//! .nika/arm/<label>/last.json         rebuildable projection cache
//! .nika/arm/<label>/watermark         last decided instant
//! .nika/arm/<label>/history.ndjson    nika/arm-event@1 hash chain (truth)
//! .nika/arm/<label>/history-w2*.ndjson immutable legacy archives (N4)
//! ```
//!
//! Pure codec, verification, reconciliation, and replay live in
//! `nika_cadence::ledger`; this module owns paths, locks, fsync, atomic
//! projection writes, and legacy rotation only. Reads fold the verified prefix
//! without cutting evidence. Appends may cut an invalid tail, then land the
//! ledger before projections. Lock order is beat → ledger, never the inverse;
//! neither ledger lock nor state survives across a run.

use std::io;
use std::path::{Path, PathBuf};

use jiff::{Timestamp, Zoned};
use nika_cadence::firing::SlotId;
use nika_cadence::ledger::{
    decision_payload, first_line_is_versioned, json_str, ledger_line, parse_last, render_last,
    scan_chain,
};

pub use nika_cadence::ledger::{
    Claim, DecisionKind as FireKind, HistoryEntry, LastRecord, RecordOutcome, Unsettled,
};

/// Read-only journal discovery adapter; the pure fold lives in cadence.
pub(crate) mod replay;

/// The sidecar root below a project directory (D3 · next to the traces).
const ARM_DIR: &str = ".nika/arm";

/// The beat lock's file name (law ⑥).
const BEAT_LOCK: &str = "lock";

/// The inner ledger lock's file name · see the module doc for the
/// lock-ordering law.
const LEDGER_LOCK: &str = "ledger.lock";

/// The versioned ledger's file name.
const HISTORY: &str = "history.ndjson";

/// The ledger-lock wait: 100 × 5 ms — an eternity against a
/// microseconds-long critical section, finite against a wedged holder
/// (the record then refuses LOUDLY; an eternal block never happens).
const LEDGER_LOCK_PASSES: u32 = 100;
const LEDGER_LOCK_NAP: std::time::Duration = std::time::Duration::from_millis(5);

/// The firing state, rooted at `<project>/.nika/arm`.
pub struct ArmState {
    root: PathBuf,
}

/// What a lock attempt found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockOutcome {
    /// No live owner — the lock is ours.
    Acquired,
    /// A LIVE process holds it (signal 0 answered) — law ⑥ governs.
    HeldAlive {
        /// The holding process.
        pid: u32,
    },
    /// The holder's pid is dead (a crash remnant) — taken over.
    StaleTaken {
        /// The dead pid, when the remnant parsed.
        old_pid: Option<u32>,
    },
}

/// One audible migration result.
pub(crate) struct HealOutcome {
    pub rotated: Option<Rotation>,
    pub repaired: u64,
    pub lines: u64,
    pub rebuilt_last: bool,
    pub rebuilt_watermark: bool,
}

impl ArmState {
    /// The sidecar of the project rooted at `project_dir` (D3).
    #[must_use]
    pub fn at_project(project_dir: &Path) -> Self {
        Self {
            root: project_dir.join(ARM_DIR),
        }
    }

    /// The sidecar root (the report's orphan walk reads it).
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The last decided slot; a skip or park consumes it too.
    #[must_use]
    pub fn last_fired(&self, label: &str) -> Option<Zoned> {
        self.last(label)
            .map(|r| r.slot.to_zoned(jiff::tz::TimeZone::UTC))
    }

    /// Read the projection cache, rebuilding it from the verified chain on miss.
    #[must_use]
    pub fn last(&self, label: &str) -> Option<LastRecord> {
        let dir = self.root.join(label);
        if let Some(record) = read_last_file(&dir) {
            Some(record)
        } else {
            let record = replay::replay(&dir).ok()?.last?;
            // Cache repair failure never hides replayed truth in memory.
            let _ = write_atomic(&dir.join("last.json"), &render_last(&record));
            Some(record)
        }
    }

    /// Count skips/fires across the live ledger and W2 archives.
    #[must_use]
    pub fn tallies(&self, label: &str) -> Option<(usize, usize)> {
        let dir = self.root.join(label);
        let mut journals: Vec<PathBuf> = std::fs::read_dir(&dir)
            .ok()?
            .filter_map(std::result::Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n == HISTORY || n.starts_with("history-w2"))
            })
            .collect();
        if journals.is_empty() {
            return None;
        }
        journals.sort_unstable();
        let mut texts = Vec::with_capacity(journals.len());
        for journal in journals {
            let Ok(text) = std::fs::read_to_string(&journal) else {
                continue;
            };
            texts.push(text);
        }
        Some(nika_cadence::ledger::tallies(
            texts.iter().map(String::as_str),
        ))
    }

    /// Unknown sidecars are reported, never erased (N4).
    #[must_use]
    pub fn orphans(&self, known: &[String]) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut out: Vec<String> = entries
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|name| !known.contains(name))
            .collect();
        out.sort_unstable();
        out
    }

    /// Try the beat's lock. A live holder answers signal 0 — `HeldAlive`
    /// (law ⑥ then governs: sauter · file). A dead holder's file is a
    /// crash remnant — taken over (`StaleTaken`). No file: `Acquired`.
    ///
    /// # Errors
    /// I/O on the sidecar directory (unwritable project, …).
    pub fn try_lock(&self, label: &str, pid: u32, now: &Zoned) -> io::Result<LockOutcome> {
        let dir = self.dir(label)?;
        try_named_lock(&dir, BEAT_LOCK, pid, now)
    }

    /// Release the beat's lock. Idempotent: an absent lock is released.
    ///
    /// # Errors
    /// I/O other than `NotFound`.
    pub fn release(&self, label: &str) -> io::Result<()> {
        self.release_named(label, BEAT_LOCK)
    }

    /// Release one lock file by name. Idempotent.
    fn release_named(&self, label: &str, name: &str) -> io::Result<()> {
        match std::fs::remove_file(self.root.join(label).join(name)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Bound the ledger critical section; never hold it across a run.
    fn ledger_guard(&self, dir: &Path, label: &str, now: &Zoned) -> io::Result<LedgerGuard<'_>> {
        let pid = std::process::id();
        for _ in 0..LEDGER_LOCK_PASSES {
            match try_named_lock(dir, LEDGER_LOCK, pid, now)? {
                LockOutcome::Acquired | LockOutcome::StaleTaken { .. } => {
                    return Ok(LedgerGuard {
                        state: self,
                        label: label.to_owned(),
                    });
                }
                LockOutcome::HeldAlive { .. } => std::thread::sleep(LEDGER_LOCK_NAP),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "arm ledger lock: a live holder outlived the wait bound",
        ))
    }

    /// Append and fsync a decision before updating its projections.
    ///
    /// # Errors
    /// I/O on the sidecar.
    pub fn record(&self, label: &str, entry: &HistoryEntry) -> io::Result<RecordOutcome> {
        let dir = self.dir(label)?;
        let now = entry.decided_at.to_zoned(jiff::tz::TimeZone::UTC);
        let _ledger = self.ledger_guard(&dir, label, &now)?;
        let head = chain_head(&dir, &entry.decided_at)?;
        let seq = head.seq + 1;
        let (line, _) = ledger_line(
            seq,
            entry.decided_at,
            entry.kind.as_str(),
            entry.slot_id.as_ref().map(SlotId::as_str),
            &decision_payload(entry),
            head.prev_hash.as_deref(),
        );
        append_line(&dir.join(HISTORY), &line)?;
        if let Some(slot) = entry.slot {
            write_atomic(
                &dir.join("last.json"),
                &render_last(&LastRecord {
                    slot,
                    fired_at: entry.decided_at,
                    trace: entry.trace.clone(),
                    exit: entry.exit,
                    kind: entry.kind,
                    generation: entry.generation.clone(),
                }),
            )?;
        }
        write_atomic(&dir.join("watermark"), &format!("{}\n", entry.decided_at))?;
        Ok(RecordOutcome {
            seq,
            repaired: head.repaired,
        })
    }

    /// Append and fsync the claim before the run; projections do not move.
    ///
    /// # Errors
    /// As [`record`](Self::record).
    pub fn record_claim(&self, label: &str, claim: &Claim) -> io::Result<RecordOutcome> {
        let dir = self.dir(label)?;
        let now = claim.decided_at.to_zoned(jiff::tz::TimeZone::UTC);
        let _ledger = self.ledger_guard(&dir, label, &now)?;
        let head = chain_head(&dir, &claim.decided_at)?;
        let seq = head.seq + 1;
        // The claim's own sequence is its fencing token.
        let generation = claim
            .generation
            .as_ref()
            .map_or("null".to_owned(), |g| json_str(g.as_str()));
        let payload = format!(
            "{{\"attempt\":1,\"deadline\":\"{}\",\"fencing\":{seq},\"gen\":{generation}}}",
            claim.deadline
        );
        let (line, _) = ledger_line(
            seq,
            claim.decided_at,
            "claimed",
            Some(claim.slot_id.as_str()),
            &payload,
            head.prev_hash.as_deref(),
        );
        append_line(&dir.join(HISTORY), &line)?;
        Ok(RecordOutcome {
            seq,
            repaired: head.repaired,
        })
    }

    /// Find claims without a matching later fenced receipt.
    #[must_use]
    pub fn unsettled(&self, label: &str) -> Vec<Unsettled> {
        let Ok(text) = std::fs::read_to_string(self.root.join(label).join(HISTORY)) else {
            return Vec::new();
        };
        nika_cadence::ledger::unsettled(&text)
    }

    /// List sidecar beat directories in stable order.
    pub(crate) fn beat_dirs(&self) -> io::Result<Vec<String>> {
        let entries = match std::fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        let mut out = Vec::new();
        for entry in entries {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let label = entry.file_name().into_string().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "arm sidecar label is not UTF-8")
            })?;
            out.push(label);
        }
        out.sort_unstable();
        Ok(out)
    }

    /// Verify/rotate under lock, then rebuild projections from replay.
    pub(crate) fn heal(&self, label: &str, now: &Timestamp) -> io::Result<HealOutcome> {
        let dir = self.root.join(label);
        let now_zoned = now.to_zoned(jiff::tz::TimeZone::UTC);
        let _ledger = self.ledger_guard(&dir, label, &now_zoned)?;
        let head = chain_head(&dir, now)?;
        let replayed = replay::replay(&dir)?;
        if let Some(last) = &replayed.last {
            write_atomic(&dir.join("last.json"), &render_last(last))?;
        }
        if let Some(watermark) = replayed.watermark {
            write_atomic(&dir.join("watermark"), &format!("{watermark}\n"))?;
        }
        Ok(HealOutcome {
            rotated: head.rotated,
            repaired: head.repaired,
            lines: head.seq,
            rebuilt_last: replayed.last.is_some(),
            rebuilt_watermark: replayed.watermark.is_some(),
        })
    }

    /// Fold the current lifecycle for the report.
    pub(crate) fn folded(&self, label: &str, now: &Timestamp) -> Option<replay::Folded> {
        let replayed = replay::replay(&self.root.join(label)).ok()?;
        replay::fold_replay(&replayed, now)
    }

    /// The beat's directory, created on demand.
    fn dir(&self, label: &str) -> io::Result<PathBuf> {
        let dir = self.root.join(label);
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }
}

/// Parse the projection cache; replay handles any miss.
fn read_last_file(dir: &Path) -> Option<LastRecord> {
    let text = std::fs::read_to_string(dir.join("last.json")).ok()?;
    parse_last(&text)
}

#[derive(Debug)]
struct ChainHead {
    seq: u64,
    prev_hash: Option<String>,
    repaired: u64,
    rotated: Option<Rotation>,
}

#[derive(Debug)]
pub(crate) struct Rotation {
    pub name: String,
    pub lines: usize,
}

/// Verify the live chain, rotating legacy or cutting only its invalid tail.
fn chain_head(dir: &Path, now: &Timestamp) -> io::Result<ChainHead> {
    let path = dir.join(HISTORY);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let lines: Vec<&str> = text.lines().collect();
    if !lines.is_empty() && !first_line_is_versioned(&text) {
        return rotate_legacy(dir, &path, lines.len(), now);
    }
    let (seq, prev_hash, valid_lines) = scan_chain(&text);
    let repaired = u64::try_from(lines.len() - valid_lines).unwrap_or(u64::MAX);
    if repaired > 0 {
        let mut prefix = lines[..valid_lines].join("\n");
        if valid_lines > 0 {
            prefix.push('\n');
        }
        write_atomic(&path, &prefix)?;
    }
    Ok(ChainHead {
        seq,
        prev_hash,
        repaired,
        rotated: None,
    })
}

/// Rotate a W2 journal forever and open the versioned chain with its receipt.
fn rotate_legacy(
    dir: &Path,
    path: &Path,
    legacy_lines: usize,
    now: &Timestamp,
) -> io::Result<ChainHead> {
    let mut name = "history-w2.ndjson".to_owned();
    let mut n = 2u32;
    while dir.join(&name).exists() {
        name = format!("history-w2-{n}.ndjson");
        n = n.checked_add(1).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "arm ledger: every W2 archive suffix is occupied",
            )
        })?;
    }
    std::fs::rename(path, dir.join(&name))?;
    sync_parent(&dir.join(&name))?;
    let payload = format!("{{\"from\":{},\"lines\":{legacy_lines}}}", json_str(&name));
    let (line, hash) = ledger_line(1, *now, "rotated", None, &payload, None);
    append_line(path, &line)?;
    Ok(ChainHead {
        seq: 1,
        prev_hash: Some(hash),
        repaired: 0,
        rotated: Some(Rotation {
            name,
            lines: legacy_lines,
        }),
    })
}

/// Ledger lock released on every critical-section exit.
struct LedgerGuard<'a> {
    state: &'a ArmState,
    label: String,
}

impl Drop for LedgerGuard<'_> {
    fn drop(&mut self) {
        let _ = self.state.release_named(&self.label, LEDGER_LOCK);
    }
}

/// Atomic lock attempt shared by beat and ledger locks.
fn try_named_lock(dir: &Path, name: &str, pid: u32, now: &Zoned) -> io::Result<LockOutcome> {
    const MAX_PASSES: u32 = 8;
    let lock = dir.join(name);
    let body = format!("{{\"pid\":{pid},\"started_at\":\"{}\"}}\n", now.timestamp());
    // Some(None) records an unparseable remnant we removed.
    let mut removed: Option<Option<u32>> = None;
    for _ in 0..MAX_PASSES {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock)
        {
            Ok(mut f) => {
                use std::io::Write as _;
                f.write_all(body.as_bytes())?;
                return Ok(match removed {
                    Some(old_pid) => LockOutcome::StaleTaken { old_pid },
                    None => LockOutcome::Acquired,
                });
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
        let old_pid = std::fs::read_to_string(&lock)
            .ok()
            .and_then(|text| lock_pid(&text));
        if let Some(old) = old_pid
            && owner_alive(old)
        {
            return Ok(LockOutcome::HeldAlive { pid: old });
        }
        // No live owner is proven; atomic create resolves any racer.
        match std::fs::remove_file(&lock) {
            Ok(()) => removed = Some(old_pid),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::WouldBlock,
        "arm lock: contended past the pass bound",
    ))
}

/// Parse the pid from a lock file.
fn lock_pid(text: &str) -> Option<u32> {
    let doc: serde_json::Value = serde_json::from_str(text).ok()?;
    doc.get("pid")?.as_u64().and_then(|p| u32::try_from(p).ok())
}

/// Probe a pid without signalling; `EPERM` still proves a live owner.
#[cfg(unix)]
fn owner_alive(pid: u32) -> bool {
    use nix::sys::signal::kill;
    use nix::unistd::Pid;
    let Ok(raw) = i32::try_from(pid) else {
        return false;
    };
    if raw == 0 {
        return false;
    }
    let pid = Pid::from_raw(raw);
    matches!(kill(pid, None), Ok(()) | Err(nix::errno::Errno::EPERM))
}

/// Without a signal probe, never steal a lock.
#[cfg(not(unix))]
fn owner_alive(_pid: u32) -> bool {
    true
}

/// Atomic durable rewrite: fsync file, rename, then fsync parent.
fn write_atomic(path: &Path, body: &str) -> io::Result<()> {
    use std::io::Write as _;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("state");
    let tmp = path.with_file_name(format!("{name}.tmp-{}", std::process::id()));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(body.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    sync_parent(path)
}

/// Fsync the directory that makes the rename durable.
#[cfg(unix)]
fn sync_parent(path: &Path) -> io::Result<()> {
    match path.parent() {
        Some(dir) => std::fs::File::open(dir)?.sync_all(),
        None => Ok(()),
    }
}

/// Directory fsync has no portable non-Unix form.
#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> io::Result<()> {
    Ok(())
}

/// Append and fsync one ledger line before the caller's next act.
fn append_line(path: &Path, line: &str) -> io::Result<()> {
    use std::io::Write as _;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    f.write_all(line.as_bytes())?;
    f.write_all(b"\n")?;
    f.sync_all()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[path = "state/tests.rs"]
mod tests;
