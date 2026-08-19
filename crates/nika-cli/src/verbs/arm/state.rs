// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The firing sidecar (D3) · the state the FILE never carries:
//!
//! ```text
//! .nika/arm/<label>/lock               the beat lock { "pid": u32, "started_at": RFC3339 }
//! .nika/arm/<label>/ledger.lock        the inner ledger lock · serializes verify + append
//!                                      + fsync between processes when the beat lock is
//!                                      NOT held (the overlap-skip path) · never held
//!                                      across a run
//! .nika/arm/<label>/last.json          the PROJECTION { "slot", "fired_at", "trace",
//!                                      "exit", "kind": "fired|skipped|paused|failed",
//!                                      "gen" } — a CACHE since W7: rebuilt from the
//!                                      chain whenever absent or unreadable
//! .nika/arm/<label>/watermark          the last DECIDED instant (RFC 3339 + newline) ·
//!                                      the restart reconciliation floor
//! .nika/arm/<label>/history.ndjson     the versioned ledger (nika/arm-event@1) · one
//!                                      line per event, hash-chained: v · seq · ts ·
//!                                      kind · slot_id · payload · prev_hash · hash
//!                                      (kind carries « claimed » · « rotated » ·
//!                                      « disarmed » too)
//! .nika/arm/<label>/history-w2.ndjson  a pre-ledger journal, rotated aside on the
//!                                      first versioned append, kept FOREVER (N4)
//! ```
//!
//! It lives NEXT TO the traces, at the root of the project that arms
//! the beats (the directory holding `nika.yaml`) · never in the YAML
//! (what changes by itself is never written in what a human re-reads).
//!
//! The lock's owner must be ALIVE to hold: a lock whose pid answers
//! signal 0 is a running tick (law ⑥ governs it); a dead pid's lock is
//! a crash remnant, taken over. The takeover assumes ONE firer per beat
//! (D2 · launchd today, `serve` at ②): two racers re-reading the same
//! stale lock resolve through the atomic `create_new`, the loser
//! re-judges the winner's live pid.
//!
//! The read half (W7's reversal, pinned): never-fired-on-missing dated
//! from BEFORE the ledger. The chain is the truth since W5-bis, so a
//! missing OR corrupt `last.json` now REPLAYS the chain — the verified
//! prefix folds, the invalid tail is refused (a read cuts NOTHING; the
//! cut stays the append's gesture) — and the projection is rebuilt from
//! the last slot-bearing decision (`fired|skipped|paused|failed`;
//! `claimed`/`rotated` never bear slots). An absent CHAIN still reads
//! as never-fired (N2 — the direction that is truly safe: the planner
//! then owes the on-time window alone and invents no backlog, at most
//! one on-time re-fire, never a catch-up storm). `last.json` keeps
//! being written (fast reads); it is only a cache.
//!
//! The ledger law (W5-bis): every append verifies the chain first (seq
//! continuity from 1 · `prev_hash` linkage · every hash recomputed over
//! the exact bytes) and CUTS an invalid tail · a valid line is never
//! rewritten. The append lands fsync'd, then `last.json`, then the
//! watermark · a crash between the append and the watermark is a redo,
//! the safe direction. Lock order, pinned: beat lock → ledger lock,
//! NEVER the inverse; the ledger lock is never held across a run.

use std::io;
use std::path::{Path, PathBuf};

use jiff::{Timestamp, Zoned};
use nika_cadence::firing::{ArmGeneration, FencingToken, SlotId};

/// The replay half (W7 · D1) — read-only chain verification + the
/// projection rebuild. `last()` falls back to it; `migrate` rebuilds
/// with it.
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

/// The ledger line's schema tag (versioned from day one · FCI-003).
const LEDGER_SCHEMA: &str = "nika/arm-event@1";

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

/// The decision kinds — the `kind:` vocabulary of `last.json` and
/// `history.ndjson`, and the firer's one-line prefixes (D8).
/// `Disarmed` is the W3 disarm gesture's: history-only (it bears no
/// slot, so `record` never writes it to `last.json`, and [`ArmState::last`]
/// reads the word as unreadable — the safe direction).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FireKind {
    /// The run went and exited clean.
    Fired,
    /// A policy said no (missed · overlap · already · inactive · …).
    Skipped,
    /// A human gate paused the run — PARKED with its trace (law N2:
    /// never resumed, never answered by the firer).
    Paused,
    /// The run went and failed (or refused its own file).
    Failed,
    /// The emitted OS unit was torn down (`arm disarm --write` — W3).
    Disarmed,
}

impl FireKind {
    /// The wire word (JSON + the one-line prefix).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fired => "fired",
            Self::Skipped => "skipped",
            Self::Paused => "paused",
            Self::Failed => "failed",
            Self::Disarmed => "disarmed",
        }
    }
}

/// One decision, journaled. `slot` is `None` only for the pre-slot
/// skips (inactive · cloud · expired · webhook) — those journal the
/// decision but leave `last.json` untouched (its `slot` is not
/// nullable). `slots` carries the silence's count when
/// `rattraper-une-fois` fires ONE run for n slots. On the ledger the
/// fields ride the `payload` object and `decided_at` is promoted to
/// the envelope's `ts`. The W7 types ride the cadence newtypes — the
/// wire keeps their strings.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    /// The slot decided (the absolute instant — zone-free on the wire).
    pub slot: Option<Timestamp>,
    /// The decision instant (the injected clock — D5).
    pub decided_at: Timestamp,
    /// What was decided.
    pub kind: FireKind,
    /// The skip's reason (`missed:3` · `overlap` · `cloud` · …).
    pub reason: Option<String>,
    /// The run's trace path, repo-relative (`fired` · `paused` · `failed`).
    pub trace: Option<String>,
    /// The run's exit code (`fired` 0 · `failed` 1|2|3 · `paused` 4).
    pub exit: Option<u8>,
    /// `rattraper-une-fois`: how many slots the one fire answers for.
    pub slots: Option<u32>,
    /// The slot's canonical identity ([`SlotId::derive`]) — `None` for
    /// the pre-slot skips.
    pub slot_id: Option<SlotId>,
    /// A receipt pins the claim it settles (the claim's seq) — `None`
    /// on every line that follows no claim.
    pub fencing: Option<FencingToken>,
    /// The pinned generation (D3 · F17) — the claim's, inherited by the
    /// receipt; `None` on the skips and when the workflow bytes were
    /// unreadable at claim time.
    pub generation: Option<ArmGeneration>,
}

/// The durable claim (W5-bis): appended + fsync'd BEFORE the run — a
/// crash between the claim and its receipt leaves a VISIBLE orphan
/// ([`ArmState::unsettled`]), never a silent double-fire. The
/// guarantee is at-least-once, never exactly-once.
#[derive(Debug, Clone)]
pub struct Claim {
    /// The slot's canonical identity ([`SlotId::derive`]).
    pub slot_id: SlotId,
    /// The generation this firing pins (D3 · F17) — `None` only when
    /// the workflow bytes were unreadable (the run then fails its own
    /// receipt; the claim pins nothing rather than lying).
    pub generation: Option<ArmGeneration>,
    /// The crash-detector deadline — the beat's next theoretical slot
    /// (the sweep that reads it is W8's).
    pub deadline: Timestamp,
    /// The decision instant (the envelope's `ts`).
    pub decided_at: Timestamp,
}

/// What an append landed: the line's seq (a claim's fencing token) and
/// the invalid-tail count the append cut first (0 = a clean chain).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordOutcome {
    /// The appended line's seq (1-based, chain-continuous).
    pub seq: u64,
    /// How many invalid tail lines the append cut before landing.
    pub repaired: u64,
}

/// What migrate's per-beat gesture landed (D2 — the report names each
/// act, never silent).
pub(crate) struct HealOutcome {
    /// The rotation performed on the way in, when one was.
    pub rotated: Option<Rotation>,
    /// The invalid tail lines the verify cut.
    pub repaired: u64,
    /// The valid chain's length after the gesture.
    pub lines: u64,
    /// `last.json` was rebuilt by the replay.
    pub rebuilt_last: bool,
    /// The watermark was rebuilt by the replay.
    pub rebuilt_watermark: bool,
}

/// A claim no receipt settled — the crash detector's output (W5-bis
/// detects; the sweep is W8's). In the single-firer world an orphan is
/// unambiguous: its holder is this process's dead ancestor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsettled {
    /// The claim's seq (= its fencing token).
    pub seq: u64,
    /// The claimed slot's canonical identity.
    pub slot_id: SlotId,
    /// The deadline the claim declared.
    pub deadline: Timestamp,
    /// The claim's instant.
    pub claimed_at: Timestamp,
}

/// The parsed `last.json` — the report (PROUVE) and the planner read it.
#[derive(Debug, Clone)]
pub struct LastRecord {
    /// The last DECIDED slot (a skip consumes its slot too).
    pub slot: Timestamp,
    /// The decision instant.
    pub fired_at: Timestamp,
    /// The run's trace path when the decision ran something.
    pub trace: Option<String>,
    /// The run's exit code when it ran.
    pub exit: Option<u8>,
    /// The decision kind.
    pub kind: FireKind,
    /// The generation the firing pinned (D3 · F17) — `None` on the
    /// decisions that never claim (the skips) and on W2-era lines.
    pub generation: Option<ArmGeneration>,
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

    /// The last DECIDED slot as the planner's `last_fired` — strictly
    /// after it, a slot is due again. A skip/park consumes its slot:
    /// « fired » here means « decided ». Missing or corrupt state reads
    /// as never-fired (N2 — the on-time window alone then decides).
    #[must_use]
    pub fn last_fired(&self, label: &str) -> Option<Zoned> {
        self.last(label)
            .map(|r| r.slot.to_zoned(jiff::tz::TimeZone::UTC))
    }

    /// The parsed `last.json` of one beat. W7's reversal (the module
    /// doc pins it): absent or unreadable, the projection is REBUILT by
    /// replaying the chain — the verified prefix folds, the invalid
    /// tail is refused, the read writes nothing. `None` only when the
    /// chain itself is absent or decides nothing (N2's safe direction).
    #[must_use]
    pub fn last(&self, label: &str) -> Option<LastRecord> {
        let dir = self.root.join(label);
        if let Some(record) = read_last_file(&dir) {
            Some(record)
        } else {
            let record = replay::replay(&dir).ok()?.last?;
            // The replay is read-only; this caller owns the cache
            // repair. A read-only filesystem still gets the truth
            // in memory — the failed cache write never hides it.
            let _ = write_atomic(&dir.join("last.json"), &render_last(&record));
            Some(record)
        }
    }

    /// The history's skip/fire tallies (`x sauts / y tirs`) — over the
    /// versioned ledger AND every rotated W2-era journal (both carry a
    /// top-level `kind`). `None` when no journal exists at all (the
    /// report then says nothing).
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
        // The walk is unordered — sort for a deterministic read (the
        // count itself is order-free).
        journals.sort_unstable();
        let mut skips = 0usize;
        let mut fires = 0usize;
        for journal in journals {
            let Ok(text) = std::fs::read_to_string(&journal) else {
                continue;
            };
            for line in text.lines() {
                let Ok(doc) = serde_json::from_str::<serde_json::Value>(line) else {
                    continue;
                };
                match doc.get("kind").and_then(serde_json::Value::as_str) {
                    Some("skipped") => skips += 1,
                    Some("fired") => fires += 1,
                    _ => {}
                }
            }
        }
        Some((skips, fires))
    }

    /// The sidecar directories that name NO known label (law N4:
    /// reported, NEVER erased). `known` carries the registry's labels.
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

    /// Take the inner ledger lock — held ONLY for verify + append +
    /// fsync + the projections, NEVER across a run (lock order: beat
    /// lock → ledger lock, never the inverse). A live holder is waited
    /// out (the critical section is microseconds); past the bound the
    /// record refuses loudly rather than block forever. A dead holder's
    /// file is a crash remnant, taken over — the beat lock's mechanics.
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

    /// Journal one decision on the versioned ledger: take the ledger
    /// lock, verify the chain (cutting an invalid tail — a valid line
    /// is NEVER rewritten), append + fsync the line, then the
    /// projections (`last.json` when the decision bears a slot, then
    /// the watermark — AFTER the append, each fsync'd: a crash between
    /// the two loses the watermark, never the decision, and a redo is
    /// the safe direction).
    ///
    /// # Errors
    /// I/O on the sidecar (the firer fails the decision loudly then —
    /// a fire without its record is a fire that re-fires).
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

    /// Append the durable CLAIM (+ fsync) — the run follows, the
    /// receipt settles. Neither `last.json` nor the watermark moves:
    /// nothing is DECIDED about the slot until the receipt lands.
    ///
    /// # Errors
    /// As [`record`](Self::record) — a claim that cannot land refuses
    /// the run loudly (an unclaimed run would be an invisible orphan).
    pub fn record_claim(&self, label: &str, claim: &Claim) -> io::Result<RecordOutcome> {
        let dir = self.dir(label)?;
        let now = claim.decided_at.to_zoned(jiff::tz::TimeZone::UTC);
        let _ledger = self.ledger_guard(&dir, label, &now)?;
        let head = chain_head(&dir, &claim.decided_at)?;
        let seq = head.seq + 1;
        // The fencing token IS the line's own seq (Kleppmann): the
        // receipt settles the claim by naming it. `gen` pins the
        // firing's generation (D3 · F17).
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

    /// The claims no receipt settled — the crash detector (W5-bis
    /// detects; the sweep is W8's). A claim is settled by a LATER line
    /// carrying the same `slot_id` and `payload.fencing` == the claim's
    /// seq. Detection only: read-only and best-effort (a line that does
    /// not parse simply settles nothing and claims nothing).
    #[must_use]
    pub fn unsettled(&self, label: &str) -> Vec<Unsettled> {
        let Ok(text) = std::fs::read_to_string(self.root.join(label).join(HISTORY)) else {
            return Vec::new();
        };
        let mut claims: Vec<(usize, Unsettled)> = Vec::new();
        let mut receipts: Vec<(usize, SlotId, u64)> = Vec::new();
        for (position, line) in text.lines().enumerate() {
            let Ok(doc) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if doc.get("kind").and_then(serde_json::Value::as_str) == Some("claimed") {
                let (Some(seq), Some(identity), Some(deadline), Some(claimed_at)) = (
                    doc.get("seq").and_then(serde_json::Value::as_u64),
                    doc.get("slot_id")
                        .and_then(serde_json::Value::as_str)
                        .and_then(SlotId::from_wire),
                    doc.get("payload")
                        .and_then(|p| p.get("deadline"))
                        .and_then(serde_json::Value::as_str)
                        .and_then(|d| d.parse::<Timestamp>().ok()),
                    doc.get("ts")
                        .and_then(serde_json::Value::as_str)
                        .and_then(|t| t.parse::<Timestamp>().ok()),
                ) else {
                    continue;
                };
                claims.push((
                    position,
                    Unsettled {
                        seq,
                        slot_id: identity,
                        deadline,
                        claimed_at,
                    },
                ));
                continue;
            }
            let receipt = (
                doc.get("slot_id")
                    .and_then(serde_json::Value::as_str)
                    .and_then(SlotId::from_wire),
                doc.get("payload")
                    .and_then(|p| p.get("fencing"))
                    .and_then(serde_json::Value::as_u64),
            );
            if let (Some(identity), Some(fencing)) = receipt {
                receipts.push((position, identity, fencing));
            }
        }
        claims
            .into_iter()
            .filter(|(position, claim)| {
                !receipts.iter().any(|(later, identity, fencing)| {
                    later > position && (identity, *fencing) == (&claim.slot_id, claim.seq)
                })
            })
            .map(|(_, claim)| claim)
            .collect()
    }

    /// The sidecar's beat directories, sorted (migrate's walk). An
    /// absent sidecar is an empty walk, never an error.
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

    /// migrate's per-beat gesture (D2), the whole of it under the
    /// ledger lock: verify the chain (a never-appended W2 journal is
    /// rotated aside, an invalid tail cut), then rebuild the
    /// projections BY REPLAY — the chain is the truth, a stale
    /// `last.json` is rewritten, never trusted.
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

    /// The report's folded state (D5): the current lifecycle through
    /// the cadence machine, the deadline judged against the injected
    /// `now`. `None` when the journal says nothing.
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

/// The `last.json` fast path — the cache read. `None` when absent,
/// unparseable, or carrying a word the projection never writes (the
/// replay then speaks, [`ArmState::last`]).
fn read_last_file(dir: &Path) -> Option<LastRecord> {
    let text = std::fs::read_to_string(dir.join("last.json")).ok()?;
    let doc: serde_json::Value = serde_json::from_str(&text).ok()?;
    let slot: Timestamp = doc.get("slot")?.as_str()?.parse().ok()?;
    let fired_at: Timestamp = doc.get("fired_at")?.as_str()?.parse().ok()?;
    Some(LastRecord {
        slot,
        fired_at,
        trace: doc
            .get("trace")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        exit: doc
            .get("exit")
            .and_then(serde_json::Value::as_u64)
            .and_then(|e| u8::try_from(e).ok()),
        kind: fire_kind(doc.get("kind")?.as_str()?)?,
        generation: doc
            .get("gen")
            .and_then(serde_json::Value::as_str)
            .and_then(ArmGeneration::from_wire),
    })
}

/// The decision word — the projection's closed vocabulary (a
/// `disarmed` never lands here: it bears no slot).
fn fire_kind(word: &str) -> Option<FireKind> {
    match word {
        "fired" => Some(FireKind::Fired),
        "skipped" => Some(FireKind::Skipped),
        "paused" => Some(FireKind::Paused),
        "failed" => Some(FireKind::Failed),
        _ => None,
    }
}

/// The `last.json` document — the locked shape (D3), `gen` joining in
/// W7 (a pre-W7 cache reads fine: the field parses as absent). The ONE
/// renderer: `record` writes through it and the replay rebuilds through
/// it, so a rebuilt projection is byte-identical by construction.
fn render_last(record: &LastRecord) -> String {
    let trace = record.trace.as_deref().map_or("null".to_owned(), json_str);
    let exit = record.exit.unwrap_or(0);
    let generation = record
        .generation
        .as_ref()
        .map_or("null".to_owned(), |g| format!("\"{}\"", g.as_str()));
    format!(
        "{{\"slot\":\"{}\",\"fired_at\":\"{}\",\"trace\":{trace},\"exit\":{exit},\"kind\":\"{}\",\"gen\":{generation}}}\n",
        record.slot,
        record.fired_at,
        record.kind.as_str()
    )
}

/// The five decision kinds' payload — the W2 fields verbatim
/// (`decided_at` is promoted to the envelope's `ts`), plus `fencing`
/// when the line is a receipt settling a claim, and `gen` when the
/// firing pinned its generation (D3 — the claim's, inherited).
fn decision_payload(entry: &HistoryEntry) -> String {
    let slot = entry.slot.map_or("null".to_owned(), |s| format!("\"{s}\""));
    let reason = entry.reason.as_deref().map_or("null".to_owned(), json_str);
    let trace = entry.trace.as_deref().map_or("null".to_owned(), json_str);
    let exit = entry.exit.map_or("null".to_owned(), |e| e.to_string());
    let slots = entry.slots.map_or("null".to_owned(), |s| s.to_string());
    let fencing = entry
        .fencing
        .map_or("null".to_owned(), |f| f.get().to_string());
    let generation = entry
        .generation
        .as_ref()
        .map_or("null".to_owned(), |g| json_str(g.as_str()));
    format!(
        "{{\"slot\":{slot},\"reason\":{reason},\"trace\":{trace},\"exit\":{exit},\"slots\":{slots},\"fencing\":{fencing},\"gen\":{generation}}}"
    )
}

/// A JSON string literal. The machine tokens never need the escapes,
/// but a free-text field (a trace path) must never break the line's
/// parse: the hash covers the bytes, the READER needs them valid.
fn json_str(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 2);
    out.push('"');
    for ch in raw.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// One ledger line, and its hash. The byte shape is LAW (the hash
/// covers these exact bytes), so the construction is manual and the
/// field order fixed: `schema · v · seq · ts · kind · slot_id ·
/// payload · prev_hash · hash`. The hash is sha256 over the `prev_hash`
/// rendered as JSON (`null` at the genesis) + `\n` + the line's exact
/// bytes up to the hash field.
fn ledger_line(
    seq: u64,
    ts: Timestamp,
    kind: &str,
    slot_id: Option<&str>,
    payload: &str,
    prev_hash: Option<&str>,
) -> (String, String) {
    let kind_json = json_str(kind);
    let slot_json = slot_id.map_or("null".to_owned(), json_str);
    let prev_json = prev_hash.map_or("null".to_owned(), json_str);
    let prefix = format!(
        "{{\"schema\":\"{LEDGER_SCHEMA}\",\"v\":1,\"seq\":{seq},\"ts\":\"{ts}\",\"kind\":{kind_json},\"slot_id\":{slot_json},\"payload\":{payload},\"prev_hash\":{prev_json}"
    );
    let hash = sha256_hex(format!("{prev_json}\n{prefix}").as_bytes());
    (format!("{prefix},\"hash\":\"{hash}\"}}"), hash)
}

/// The chain's head for the next append.
#[derive(Debug)]
struct ChainHead {
    /// The last valid line's seq (0 = the chain is empty).
    seq: u64,
    /// The last valid line's hash (`None` at the genesis).
    prev_hash: Option<String>,
    /// How many invalid tail lines the walk cut (0 = a clean chain).
    repaired: u64,
    /// The rotation performed on the way in (a pre-ledger journal
    /// moved aside — the archive's name and its line count).
    rotated: Option<Rotation>,
}

/// A pre-ledger journal's rotation, reported (migrate names it).
#[derive(Debug)]
pub(crate) struct Rotation {
    /// The archive's name (`history-w2.ndjson`, `-2`…).
    pub name: String,
    /// How many lines the archive carries.
    pub lines: usize,
}

/// Is the journal's first line a versioned ledger line? (The W2-era
/// journals carry no `schema` — `chain_head` rotates them, the replay
/// folds them.)
fn first_line_is_versioned(text: &str) -> bool {
    text.lines()
        .next()
        .and_then(|first| serde_json::from_str::<serde_json::Value>(first).ok())
        .and_then(|doc| {
            doc.get("schema")
                .and_then(|s| s.as_str())
                .map(str::to_owned)
        })
        .is_some_and(|s| s == LEDGER_SCHEMA)
}

/// Verify the chain and answer the head: seq continuity from 1,
/// `prev_hash` linkage, every hash recomputed over the exact bytes. An
/// invalid TAIL is truncated to the last valid line (the valid
/// prefix's bytes survive verbatim — `write_atomic`), the count riding
/// back in `repaired`. A pre-ledger journal (its first line carries no
/// `schema`) is ROTATED aside first — kept forever (N4) — and the
/// fresh chain opens with a `rotated` line naming it.
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
    let mut seq = 0u64;
    let mut prev_hash: Option<String> = None;
    let mut valid = 0usize;
    for line in &lines {
        match verify_line(line, seq + 1, prev_hash.as_deref()) {
            Some(hash) => {
                seq += 1;
                prev_hash = Some(hash);
                valid += 1;
            }
            None => break,
        }
    }
    let repaired = u64::try_from(lines.len() - valid).unwrap_or(u64::MAX);
    if repaired > 0 {
        let mut prefix = lines[..valid].join("\n");
        if valid > 0 {
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

/// The chain check for one line: schema · version · seq continuity ·
/// prev linkage · the hash recomputed over the exact bytes. Returns
/// the line's OWN hash when valid (the next line's expected prev).
fn verify_line(line: &str, expected_seq: u64, expected_prev: Option<&str>) -> Option<String> {
    let doc: serde_json::Value = serde_json::from_str(line).ok()?;
    if doc.get("schema")?.as_str()? != LEDGER_SCHEMA {
        return None;
    }
    if doc.get("v")?.as_u64()? != 1 {
        return None;
    }
    if doc.get("seq")?.as_u64()? != expected_seq {
        return None;
    }
    if doc.get("ts")?.as_str()?.parse::<Timestamp>().is_err() {
        return None;
    }
    doc.get("kind")?.as_str()?;
    doc.get("payload")?.as_object()?;
    if !matches!(
        doc.get("slot_id")?,
        serde_json::Value::Null | serde_json::Value::String(_)
    ) {
        return None;
    }
    let (prev_json, linked) = match doc.get("prev_hash")? {
        serde_json::Value::Null => ("null".to_owned(), expected_prev.is_none()),
        serde_json::Value::String(s) => (json_str(s), expected_prev == Some(s.as_str())),
        _ => return None,
    };
    if !linked {
        return None;
    }
    let hash = doc.get("hash")?.as_str()?;
    let cut = line.rfind(",\"hash\":\"")?;
    let prefix = &line[..cut];
    (sha256_hex(format!("{prev_json}\n{prefix}").as_bytes()) == hash).then(|| hash.to_owned())
}

/// The pre-ledger journal's rotation (N4 — kept FOREVER): renamed
/// aside (`history-w2.ndjson`, then `-2`, `-3`… on a collision), the
/// rename fsync'd, and a fresh chain opened with the `rotated` line
/// naming the archive and its line count.
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

/// sha256 hex over the exact bytes — the chain's and the slot
/// identity's one primitive (the source-identity convention).
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    let digest = sha2::Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// The ledger lock, RAII: released when the critical section ends,
/// error paths included.
struct LedgerGuard<'a> {
    state: &'a ArmState,
    label: String,
}

impl Drop for LedgerGuard<'_> {
    fn drop(&mut self) {
        let _ = self.state.release_named(&self.label, LEDGER_LOCK);
    }
}

/// The lock attempt's core, shared by the beat lock and the inner
/// ledger lock: the atomic `create_new` decides between racers, a live
/// holder answers signal 0, a dead one's file is a crash remnant
/// (taken over).
fn try_named_lock(dir: &Path, name: &str, pid: u32, now: &Zoned) -> io::Result<LockOutcome> {
    const MAX_PASSES: u32 = 8;
    let lock = dir.join(name);
    let body = format!("{{\"pid\":{pid},\"started_at\":\"{}\"}}\n", now.timestamp());
    // The remnant WE removed on the way in, if any (Some(None) =
    // the remnant was unparseable) — it rides the StaleTaken
    // verdict once the atomic create lands.
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
        // The holder is dead (or the file unparseable — either way
        // no LIVE owner is proven): remove the remnant, then the
        // atomic create decides between us and a racer — a racer's
        // win shows up as a live pid on the next pass.
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

/// The pid out of a lock file — `None` when the file does not parse
/// (an unparseable lock proves no live owner: taken over as stale).
fn lock_pid(text: &str) -> Option<u32> {
    let doc: serde_json::Value = serde_json::from_str(text).ok()?;
    doc.get("pid")?.as_u64().and_then(|p| u32::try_from(p).ok())
}

/// Does a process with this pid exist? Signal 0 probes without
/// delivering. `EPERM` is an owner too — a process we may not signal is
/// still a LIVE holder (stealing its lock would double-fire).
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
    // Ok = the process answered · EPERM = a process exists we may not
    // signal — both are a LIVE holder; ESRCH (and the rest) = dead.
    matches!(kill(pid, None), Ok(()) | Err(nix::errno::Errno::EPERM))
}

/// The non-unix fallback: no signal surface, so every holder reads
/// alive — the conservative direction (never steal, never double-fire).
#[cfg(not(unix))]
fn owner_alive(_pid: u32) -> bool {
    true
}

/// Rewrite a file atomically AND durably: write a sibling tmp, fsync
/// it, rename it over, then fsync the parent directory — a rename's
/// durability is the DIRECTORY's, not the file's.
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

/// fsync the directory holding `path` — the rename's durability lives
/// there.
#[cfg(unix)]
fn sync_parent(path: &Path) -> io::Result<()> {
    match path.parent() {
        Some(dir) => std::fs::File::open(dir)?.sync_all(),
        None => Ok(()),
    }
}

/// No portable directory-fsync off unix: the rename stays atomic, the
/// crash-durability degrades to the filesystem's default (the ship
/// targets are unix — the conservative no-op is SAID, not hidden).
#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> io::Result<()> {
    Ok(())
}

/// One line onto the ledger (`O_APPEND` — several firers never tear a
/// line, each append landing whole), fsync'd BEFORE the caller's next
/// act: the ledger append precedes the stdout line, durability first.
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
