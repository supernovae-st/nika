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

mod replay;
pub use replay::Folded;
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
    project: ProjectRoot,
    root: PathBuf,
}

enum ProjectRoot {
    Open(OwnedDir),
    Refused { kind: io::ErrorKind, detail: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LockOutcome {
    Acquired,
    HeldAlive { pid: u32 },
}
#[derive(Debug)]
#[non_exhaustive]
pub struct HealOutcome {
    rotated: Option<Rotation>,
    repaired: u64,
    lines: u64,
    rebuilt_last: bool,
    rebuilt_watermark: bool,
}

impl HealOutcome {
    #[must_use]
    pub fn rotation(&self) -> Option<&Rotation> {
        self.rotated.as_ref()
    }

    #[must_use]
    pub fn repaired_lines(&self) -> u64 {
        self.repaired
    }

    #[must_use]
    pub fn line_count(&self) -> u64 {
        self.lines
    }

    #[must_use]
    pub fn rebuilt_last(&self) -> bool {
        self.rebuilt_last
    }

    #[must_use]
    pub fn rebuilt_watermark(&self) -> bool {
        self.rebuilt_watermark
    }
}

impl ArmState {
    /// The sidecar of the project rooted at `project_dir` (D3).
    #[must_use]
    pub fn at_project(project_dir: &Path) -> Self {
        match Self::open(project_dir) {
            Ok(state) => state,
            Err(error) => Self {
                project: ProjectRoot::Refused {
                    kind: error.kind(),
                    detail: error.to_string(),
                },
                root: project_dir.join(ARM_DIR),
            },
        }
    }

    /// Open one project custody capability without following path symlinks.
    ///
    /// # Errors
    /// The project path is inaccessible, escaping, or redirected.
    pub fn open(project_dir: &Path) -> io::Result<Self> {
        Ok(Self {
            project: ProjectRoot::Open(OwnedDir::open(project_dir)?),
            root: project_dir.join(ARM_DIR),
        })
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
        let now = Zoned::now();
        let _ledger = Self::ledger_guard(&dir, &now)?;
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
        let Ok(arm) = self
            .project_dir()
            .and_then(|project| project.open_below(&[".nika", "arm"]))
        else {
            return Vec::new();
        };
        let Ok(directories) = arm.directory_names() else {
            return Vec::new();
        };
        let mut out: Vec<String> = directories
            .into_iter()
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
    pub(crate) fn record(&self, label: &str, entry: &HistoryEntry) -> io::Result<RecordOutcome> {
        let dir = self.safe_dir(label)?;
        Self::record_in(&dir, entry)
    }

    fn record_in(dir: &OwnedDir, entry: &HistoryEntry) -> io::Result<RecordOutcome> {
        let now = entry.decided_at.to_zoned(jiff::tz::TimeZone::UTC);
        let _ledger = Self::ledger_guard(dir, &now)?;
        let terminal = matches!(
            entry.kind,
            FireKind::Fired | FireKind::Paused | FireKind::Failed
        );
        let outcome = append_event(
            dir,
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

    /// Journal one explicit disarm while owning the beat custody lock.
    ///
    /// # Errors
    /// The beat is firing, or its descriptor-rooted journal refuses the record.
    pub fn record_disarm(
        &self,
        label: &str,
        decided_at: Timestamp,
        pid: u32,
        reason: &str,
    ) -> io::Result<RecordOutcome> {
        let now = decided_at.to_zoned(jiff::tz::TimeZone::UTC);
        let attempt = self.acquire_beat_lock(label, pid, &now)?;
        let lease = exclusive_lease(attempt, "arm disarm")?;
        let mut entry = HistoryEntry::new(None, decided_at, FireKind::Disarmed);
        entry.reason = Some(reason.to_owned());
        Self::record_in(&lease.dir, &entry)
    }

    /// Append and fsync the claim before the run; projections do not move.
    ///
    /// # Errors
    /// As [`record`](Self::record).
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn record_claim(&self, label: &str, claim: &Claim) -> io::Result<RecordOutcome> {
        let dir = self.safe_dir(label)?;
        Self::record_claim_in(&dir, claim)
    }

    /// Append a fixture entry for cross-crate tests.
    ///
    /// This deliberately bypasses beat custody and exists only behind the
    /// non-default `test-support` feature.
    ///
    /// # Errors
    /// The fixture sidecar cannot be opened or appended safely.
    #[cfg(feature = "test-support")]
    pub fn record_fixture(&self, label: &str, entry: &HistoryEntry) -> io::Result<RecordOutcome> {
        self.record(label, entry)
    }

    /// Append a fixture claim for cross-crate tests.
    ///
    /// # Errors
    /// The fixture sidecar cannot be opened or appended safely.
    #[cfg(feature = "test-support")]
    pub fn record_claim_fixture(&self, label: &str, claim: &Claim) -> io::Result<RecordOutcome> {
        self.record_claim(label, claim)
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

    /// Return the sorted labels that already own an ARM sidecar directory.
    ///
    /// # Errors
    /// The sidecar root cannot be inspected or contains a non-UTF-8 label.
    pub fn beat_dirs(&self) -> io::Result<Vec<String>> {
        let arm = match self.project_dir()?.open_below(&[".nika", "arm"]) {
            Ok(arm) => arm,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let mut out = arm.directory_names()?;
        out.sort_unstable();
        Ok(out)
    }

    /// Whether a label contains a live journal, migration intent, or archive.
    ///
    /// # Errors
    /// The descriptor-rooted sidecar cannot be inspected safely.
    pub fn has_journal_evidence(&self, label: &str) -> io::Result<bool> {
        let dir = self.safe_dir(label)?;
        Ok(dir.exists(HISTORY)?
            || dir.exists(MIGRATION_INTENT)?
            || replay::latest_archive(&dir)?.is_some())
    }

    /// Verify and heal one sidecar, including legacy rotation and projections.
    ///
    /// # Errors
    /// The journal is invalid or a descriptor-rooted filesystem operation fails.
    pub fn heal(&self, label: &str, now: &Timestamp) -> io::Result<HealOutcome> {
        let now_zoned = now.to_zoned(jiff::tz::TimeZone::UTC);
        let attempt = self.acquire_beat_lock(label, std::process::id(), &now_zoned)?;
        let beat = exclusive_lease(attempt, "arm migrate")?;
        let _ledger = Self::ledger_guard(&beat.dir, &now_zoned)?;
        let head = chain_head(&beat.dir, now)?;
        let replayed = replay::replay_safe(&beat.dir)?;
        if let Some(last) = &replayed.last {
            beat.dir.write_atomic("last.json", &render_last(last))?;
        }
        if let Some(watermark) = replayed.watermark {
            beat.dir
                .write_atomic("watermark", &format!("{watermark}\n"))?;
        }
        Ok(HealOutcome {
            rotated: head.rotated,
            repaired: head.repaired,
            lines: head.seq,
            rebuilt_last: replayed.last.is_some(),
            rebuilt_watermark: replayed.watermark.is_some(),
        })
    }

    /// Fold the verified journal into its lifecycle projection at `now`.
    ///
    /// # Errors
    /// The journal snapshot or its descriptor-rooted reads are invalid.
    pub fn folded(&self, label: &str, now: &Timestamp) -> io::Result<Option<Folded>> {
        let dir = self.safe_dir(label)?;
        let replayed = replay::replay_safe(&dir)?;
        Ok(replay::fold_replay(&replayed, now))
    }

    fn safe_dir(&self, label: &str) -> io::Result<OwnedDir> {
        self.project_dir()?.create_below(&[".nika", "arm", label])
    }

    pub(crate) fn open_project_file(
        &self,
        relative: &Path,
    ) -> io::Result<(OwnedDir, std::fs::File)> {
        let project = self.project_dir()?;
        Ok((project.try_clone()?, project.open_relative(relative)?))
    }

    fn project_dir(&self) -> io::Result<&OwnedDir> {
        match &self.project {
            ProjectRoot::Open(project) => Ok(project),
            ProjectRoot::Refused { kind, detail } => Err(io::Error::new(*kind, detail.clone())),
        }
    }
}

fn exclusive_lease(attempt: LockAttempt, operation: &str) -> io::Result<LockLease> {
    match (attempt.outcome, attempt.lease) {
        (LockOutcome::Acquired, Some(lease)) => Ok(lease),
        (LockOutcome::Acquired, None) => Err(io::Error::other(format!(
            "{operation}: acquired without a lease"
        ))),
        (LockOutcome::HeldAlive { pid }, _) => Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            format!("{operation}: beat held by live pid {pid}"),
        )),
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
#[non_exhaustive]
pub struct Rotation {
    name: String,
    lines: usize,
    resumed: bool,
}

impl Rotation {
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn line_count(&self) -> usize {
        self.lines
    }

    #[must_use]
    pub fn resumed(&self) -> bool {
        self.resumed
    }
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
    if repaired.checked_sub(1).is_some() {
        // A versioned journal has a verified genesis, so a repairable
        // suffix always leaves at least that first line.
        let prefix = format!("{}\n", lines[..valid_lines].join("\n"));
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
    if !migrated_genesis_is_valid(seq, valid_lines, text.lines().count()) {
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

fn migrated_genesis_is_valid(seq: u64, valid_lines: usize, total_lines: usize) -> bool {
    matches!((seq, valid_lines, total_lines), (1, 1, 1))
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
    let archives = replay::archive_texts(dir)?;
    let Some((_, archive_text)) = archives.iter().find(|(name, _)| name == &archive_name) else {
        return Err(invalid_migration_state());
    };
    if archive_text.lines().count() != legacy_lines {
        return Err(invalid_migration_state());
    }
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
        Ok(Some(current)) if current == *archive_text => {
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
