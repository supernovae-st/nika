// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! L4 filesystem adapter for `.nika/arm/<label>` sidecars (D3).
//! Cadence owns pure ledger semantics; this module owns paths, locks, fsync,
//! projections, and W2 rotation. Lock order is beat → ledger.

use std::io::{self, Read as _, Seek as _, Write as _};
use std::path::{Path, PathBuf};

use jiff::{Timestamp, Zoned};
use nika_cadence::firing::SlotId;
use nika_cadence::ledger::{
    JournalFormat, chain_anchor_matches, claim_payload, classify_journal, decision_payload,
    last_projection, ledger_line, legacy_receipt_payload, parse_migration_intent,
    render_chain_anchor, render_last, render_migration_intent, rotation_payload, scan_chain,
};
use nika_fs::OwnedDir;
use nix::fcntl::{Flock, FlockArg};

pub use nika_cadence::ledger::{
    Claim, DecisionKind as FireKind, HistoryEntry, LastRecord, Receipt, RecordOutcome, Unsettled,
};

pub(crate) mod replay;
const ARM_DIR: &str = ".nika/arm";
const BEAT_LOCK: &str = "lock";
const LEDGER_LOCK: &str = "ledger.lock";
const HISTORY: &str = "history.ndjson";
const CHAIN_HEAD: &str = "head.json";
const MIGRATION_INTENT: &str = "migration-w2.json";
const LEDGER_LOCK_PASSES: u32 = 100;
const LEDGER_LOCK_NAP: std::time::Duration = std::time::Duration::from_millis(5);

/// The firing state, rooted at `<project>/.nika/arm`.
pub struct ArmState {
    project: PathBuf,
    root: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LockOutcome {
    Acquired,
    HeldAlive { pid: u32 },
}
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
            project: project_dir.to_path_buf(),
            root: project_dir.join(ARM_DIR),
        }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// # Errors
    /// The sidecar is inaccessible, redirected, or fails verified replay.
    pub fn last_fired(&self, label: &str) -> io::Result<Option<Zoned>> {
        Ok(self
            .last(label)?
            .map(|r| r.slot.to_zoned(jiff::tz::TimeZone::UTC)))
    }

    /// Read the projection cache, rebuilding it from the verified chain on miss.
    /// # Errors
    /// The sidecar is inaccessible, redirected, or fails verified replay.
    pub fn last(&self, label: &str) -> io::Result<Option<LastRecord>> {
        let dir = self.safe_dir(label)?;
        let Some(record) = replay::replay_safe(&dir)?.last else {
            return Ok(None);
        };
        let rendered = render_last(&record);
        if dir.read_optional("last.json")?.as_deref() != Some(rendered.as_str()) {
            dir.write_atomic("last.json", &rendered)?;
        }
        Ok(Some(record))
    }

    #[must_use]
    pub fn tallies(&self, label: &str) -> Option<(usize, usize)> {
        let dir = self.safe_dir(label).ok()?;
        let journals = replay::journal_texts(&dir).ok()?;
        if journals.is_empty() {
            return None;
        }
        nika_cadence::ledger::tallies(
            journals
                .iter()
                .map(|(text, versioned)| (text.as_str(), *versioned)),
        )
    }

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

    pub(crate) fn acquire_beat_lock(
        &self,
        label: &str,
        pid: u32,
        now: &Zoned,
    ) -> io::Result<LockAttempt> {
        let dir = self.safe_dir(label)?;
        acquire_named_lock(dir, BEAT_LOCK, pid, now)
    }

    fn ledger_guard(dir: &OwnedDir, now: &Zoned) -> io::Result<LedgerGuard> {
        let pid = std::process::id();
        for _ in 0..LEDGER_LOCK_PASSES {
            let attempt = acquire_named_lock(dir.try_clone()?, LEDGER_LOCK, pid, now)?;
            match attempt.outcome {
                LockOutcome::Acquired => {
                    return Ok(LedgerGuard {
                        _lease: attempt.lease.ok_or_else(|| {
                            io::Error::other("arm ledger lock: acquired without a lease")
                        })?,
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
        let dir = self.safe_dir(label)?;
        let now = entry.decided_at.to_zoned(jiff::tz::TimeZone::UTC);
        let _ledger = Self::ledger_guard(&dir, &now)?;
        let terminal = matches!(
            entry.kind,
            FireKind::Fired | FireKind::Paused | FireKind::Failed
        );
        let outcome = append_event(
            &dir,
            entry.decided_at,
            entry.kind.as_str(),
            entry.slot_id.as_ref().map(SlotId::as_str),
            |_| {
                if terminal && entry.slot_id.is_none() && entry.fencing.is_none() {
                    legacy_receipt_payload(entry)
                } else {
                    Some(decision_payload(entry))
                }
            },
        )?;
        if let Some(last) = last_projection(entry) {
            dir.write_atomic("last.json", &render_last(&last))?;
        }
        dir.write_atomic("watermark", &format!("{}\n", entry.decided_at))?;
        Ok(outcome)
    }

    /// Append and fsync the claim before the run; projections do not move.
    ///
    /// # Errors
    /// As [`record`](Self::record).
    pub fn record_claim(&self, label: &str, claim: &Claim) -> io::Result<RecordOutcome> {
        let dir = self.safe_dir(label)?;
        Self::record_claim_in(&dir, claim)
    }

    pub(crate) fn record_claim_with_lease(
        lease: &LockLease,
        claim: &Claim,
    ) -> io::Result<RecordOutcome> {
        Self::record_claim_in(&lease.dir, claim)
    }

    fn record_claim_in(dir: &OwnedDir, claim: &Claim) -> io::Result<RecordOutcome> {
        let now = claim.decided_at.to_zoned(jiff::tz::TimeZone::UTC);
        let _ledger = Self::ledger_guard(dir, &now)?;
        append_event(
            dir,
            claim.decided_at,
            "claimed",
            Some(claim.slot_id.as_str()),
            |seq| claim_payload(claim, seq),
        )
    }

    pub(crate) fn record_receipt_with_lease(
        lease: &LockLease,
        receipt: &Receipt,
    ) -> io::Result<RecordOutcome> {
        let entry = receipt.history_entry();
        let dir = &lease.dir;
        let now = entry.decided_at.to_zoned(jiff::tz::TimeZone::UTC);
        let _ledger = Self::ledger_guard(dir, &now)?;
        let outcome = append_event(
            dir,
            entry.decided_at,
            entry.kind.as_str(),
            entry.slot_id.as_ref().map(SlotId::as_str),
            |_| Some(decision_payload(&entry)),
        )?;
        let last = last_projection(&entry).ok_or_else(invalid_ledger_line)?;
        dir.write_atomic("last.json", &render_last(&last))?;
        dir.write_atomic("watermark", &format!("{}\n", entry.decided_at))?;
        Ok(outcome)
    }

    /// Find claims without a matching later fenced receipt.
    #[must_use = "the unsettled result must be consumed"]
    pub fn unsettled(&self, label: &str) -> Option<impl Iterator<Item = Unsettled> + use<>> {
        let dir = self.safe_dir(label).ok()?;
        let journals = replay::journal_texts(&dir).ok()?;
        let text = journals.last().map_or("", |(text, _)| text.as_str());
        nika_cadence::ledger::unsettled(text)
    }

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

    pub(crate) fn has_journal_evidence(&self, label: &str) -> io::Result<bool> {
        let dir = self.safe_dir(label)?;
        Ok(dir.exists(HISTORY)?
            || dir.exists(MIGRATION_INTENT)?
            || replay::latest_archive(&dir)?.is_some())
    }

    pub(crate) fn heal(&self, label: &str, now: &Timestamp) -> io::Result<HealOutcome> {
        let dir = self.safe_dir(label)?;
        let now_zoned = now.to_zoned(jiff::tz::TimeZone::UTC);
        let _ledger = Self::ledger_guard(&dir, &now_zoned)?;
        let head = chain_head(&dir, now)?;
        let replayed = replay::replay_safe(&dir)?;
        if let Some(last) = &replayed.last {
            dir.write_atomic("last.json", &render_last(last))?;
        }
        if let Some(watermark) = replayed.watermark {
            dir.write_atomic("watermark", &format!("{watermark}\n"))?;
        }
        Ok(HealOutcome {
            rotated: head.rotated,
            repaired: head.repaired,
            lines: head.seq,
            rebuilt_last: replayed.last.is_some(),
            rebuilt_watermark: replayed.watermark.is_some(),
        })
    }

    pub(crate) fn folded(
        &self,
        label: &str,
        now: &Timestamp,
    ) -> io::Result<Option<replay::Folded>> {
        let dir = self.safe_dir(label)?;
        let replayed = replay::replay_safe(&dir)?;
        Ok(replay::fold_replay(&replayed, now))
    }

    fn safe_dir(&self, label: &str) -> io::Result<OwnedDir> {
        OwnedDir::create(&self.project, &[".nika", "arm", label])
    }
}

fn append_event(
    dir: &OwnedDir,
    at: Timestamp,
    kind: &str,
    slot_id: Option<&str>,
    payload: impl FnOnce(u64) -> Option<String>,
) -> io::Result<RecordOutcome> {
    let head = chain_head(dir, &at)?;
    let seq = head.seq + 1;
    let payload = payload(seq).ok_or_else(invalid_ledger_line)?;
    let (line, hash) = ledger_line(seq, at, kind, slot_id, &payload, head.prev_hash.as_deref())
        .ok_or_else(invalid_ledger_line)?;
    let mut candidate = dir.read_optional(HISTORY)?.unwrap_or_default();
    candidate.push_str(&line);
    candidate.push('\n');
    if scan_chain(&candidate).2 != candidate.lines().count() {
        return Err(invalid_ledger_line());
    }
    dir.append_line(HISTORY, &line)?;
    write_chain_anchor(dir, seq, Some(&hash))?;
    Ok(RecordOutcome::new(seq, head.repaired))
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
    pub resumed: bool,
}

fn chain_head(dir: &OwnedDir, now: &Timestamp) -> io::Result<ChainHead> {
    let resumed = finish_intended_rotation(dir, true)?;
    let text = if let Some(text) = dir.read_optional(HISTORY)? {
        text
    } else {
        if replay::latest_archive(dir)?.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "arm ledger: archive exists without a migration intent or live journal",
            ));
        }
        String::new()
    };
    let lines: Vec<&str> = text.lines().collect();
    match classify_journal(&text) {
        Some(JournalFormat::Empty) => {
            if replay::latest_archive(dir)?.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "arm ledger: empty live journal beside an archive has no migration intent",
                ));
            }
        }
        Some(JournalFormat::Legacy) => return rotate_legacy(dir, lines.len(), now),
        Some(JournalFormat::Versioned) => replay::validate_archive_commitment(dir, &text)?,
        None | Some(_) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "arm ledger: journal dialect or chain is invalid",
            ));
        }
    }
    let (seq, prev_hash, valid_lines) = scan_chain(&text);
    let repaired = u64::try_from(lines.len() - valid_lines).unwrap_or(u64::MAX);
    validate_chain_anchor(dir, &text)?;
    write_chain_anchor(dir, seq, prev_hash.as_deref())?;
    if repaired > 0 {
        let mut prefix = lines[..valid_lines].join("\n");
        if valid_lines > 0 {
            prefix.push('\n');
        }
        dir.write_atomic(HISTORY, &prefix)?;
    }
    Ok(ChainHead {
        seq,
        prev_hash,
        repaired,
        rotated: resumed,
    })
}

fn rotate_legacy(dir: &OwnedDir, legacy_lines: usize, now: &Timestamp) -> io::Result<ChainHead> {
    let mut name = "history-w2.ndjson".to_owned();
    let mut n = 2u32;
    while dir.exists(&name)? {
        name = format!("history-w2-{n}.ndjson");
        n = n.checked_add(1).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "arm ledger: every W2 archive suffix is occupied",
            )
        })?;
    }
    let intent =
        render_migration_intent(&name, legacy_lines, now).ok_or_else(invalid_migration_state)?;
    dir.write_atomic(MIGRATION_INTENT, &intent)?;
    let rotation = finish_intended_rotation(dir, false)?
        .ok_or_else(|| io::Error::other("arm ledger: durable migration intent was not consumed"))?;
    let text = dir.read(HISTORY)?;
    let (seq, prev_hash, valid_lines) = scan_chain(&text);
    if valid_lines != text.lines().count() || seq != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "arm ledger: migrated genesis is invalid",
        ));
    }
    write_chain_anchor(dir, seq, prev_hash.as_deref())?;
    Ok(ChainHead {
        seq,
        prev_hash,
        repaired: 0,
        rotated: Some(rotation),
    })
}

fn finish_intended_rotation(dir: &OwnedDir, resumed: bool) -> io::Result<Option<Rotation>> {
    let Some(text) = dir.read_optional(MIGRATION_INTENT)? else {
        return Ok(None);
    };
    let (archive_name, legacy_lines, rotated_at) =
        parse_migration_intent(&text).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "arm ledger: migration intent is invalid",
            )
        })?;
    if !dir.exists(&archive_name)? {
        let live = dir.read(HISTORY)?;
        if classify_journal(&live) != Some(JournalFormat::Legacy)
            || live.lines().count() != legacy_lines
        {
            return Err(invalid_migration_state());
        }
        dir.hard_link(HISTORY, &archive_name)?;
        dir.remove(HISTORY)?;
    }
    let archive_text = dir.read(&archive_name)?;
    if classify_journal(&archive_text) != Some(JournalFormat::Legacy)
        || archive_text.lines().count() != legacy_lines
    {
        return Err(invalid_migration_state());
    }
    let archives = replay::archive_texts(dir)?;
    let borrowed: Vec<(&str, &str)> = archives
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    let payload = rotation_payload(&borrowed).ok_or_else(invalid_migration_state)?;
    let (genesis, genesis_hash) = ledger_line(1, rotated_at, "rotated", None, &payload, None)
        .ok_or_else(invalid_ledger_line)?;
    let expected = format!("{genesis}\n");
    match dir.read_optional(HISTORY) {
        Ok(Some(current)) if current == expected => {}
        Ok(Some(current)) if current == archive_text => {
            dir.remove(HISTORY)?;
            dir.write_atomic(HISTORY, &expected)?;
        }
        Ok(Some(current)) if current.is_empty() => dir.write_atomic(HISTORY, &expected)?,
        Ok(None) => dir.write_atomic(HISTORY, &expected)?,
        Ok(Some(_)) => return Err(invalid_migration_state()),
        Err(error) => return Err(error),
    }
    write_chain_anchor(dir, 1, Some(&genesis_hash))?;
    dir.remove(MIGRATION_INTENT)?;
    Ok(Some(Rotation {
        name: archive_name,
        lines: legacy_lines,
        resumed,
    }))
}

fn invalid_migration_state() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "arm ledger: migration intent does not match live and archived evidence",
    )
}

fn invalid_ledger_line() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "arm ledger: event violates the canonical schema",
    )
}

struct LedgerGuard {
    _lease: LockLease,
}

/// Kernel-owned lease. The stable path is diagnostic metadata, never the lock
/// authority and never unlinked by a compliant firer.
pub(crate) struct LockLease {
    _lock: Flock<std::fs::File>,
    dir: OwnedDir,
}

pub(crate) struct LockAttempt {
    pub(crate) outcome: LockOutcome,
    pub(crate) lease: Option<LockLease>,
}

fn acquire_named_lock(dir: OwnedDir, name: &str, pid: u32, now: &Zoned) -> io::Result<LockAttempt> {
    let file = dir.open_lock(name)?;
    match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
        Ok(mut held) => {
            let body = format!(
                "{{\"pid\":{pid},\"started_at\":\"{}\",\"epoch\":\"{}\"}}\n",
                now.timestamp(),
                now.timestamp()
            );
            held.set_len(0)?;
            held.seek(std::io::SeekFrom::Start(0))?;
            held.write_all(body.as_bytes())?;
            held.sync_all()?;
            Ok(LockAttempt {
                outcome: LockOutcome::Acquired,
                lease: Some(LockLease { _lock: held, dir }),
            })
        }
        Err((mut file, nix::errno::Errno::EAGAIN)) => {
            let owner = read_lock_owner(&mut file)
                .ok()
                .and_then(|text| lock_pid(&text))
                .unwrap_or(0);
            Ok(LockAttempt {
                outcome: LockOutcome::HeldAlive { pid: owner },
                lease: None,
            })
        }
        Err((_file, errno)) => Err(io::Error::from_raw_os_error(errno as i32)),
    }
}

fn read_lock_owner(file: &mut std::fs::File) -> io::Result<String> {
    file.seek(std::io::SeekFrom::Start(0))?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    Ok(text)
}

fn lock_pid(text: &str) -> Option<u32> {
    let doc: serde_json::Value = serde_json::from_str(text).ok()?;
    doc.get("pid")?.as_u64().and_then(|p| u32::try_from(p).ok())
}

/// Validate the live chain against its last durable high-water mark. An
/// invalid physical tail remains repairable; a shorter valid chain does not.
fn validate_chain_anchor(dir: &OwnedDir, text: &str) -> io::Result<()> {
    let anchor = read_chain_anchor(dir)?;
    chain_anchor_matches(text, anchor.as_deref())
        .then_some(())
        .ok_or_else(invalid_chain_anchor)
}

fn read_chain_anchor(dir: &OwnedDir) -> io::Result<Option<String>> {
    dir.read_optional(CHAIN_HEAD)
}

fn write_chain_anchor(dir: &OwnedDir, seq: u64, hash: Option<&str>) -> io::Result<()> {
    let body = render_chain_anchor(seq, hash).ok_or_else(invalid_chain_anchor)?;
    if dir.read_optional(CHAIN_HEAD)?.as_deref() == Some(body.as_str()) {
        return Ok(());
    }
    dir.write_atomic(CHAIN_HEAD, &body)
}

fn invalid_chain_anchor() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "arm ledger: durable head is invalid",
    )
}

#[cfg(test)]
fn append_line(path: &Path, line: &str) -> io::Result<()> {
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
