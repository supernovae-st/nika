// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The replay (W7 · D1) — the chain is the truth, `last.json` a
//! rebuildable projection.
//!
//! READ-ONLY, always: the versioned chain verifies with the append's
//! own semantics (seq continuity · `prev_hash` linkage · the hash
//! recomputed) and the invalid tail is REFUSED — never cut (the cut
//! stays the append's gesture); the W2-era journals fold best-effort
//! (theirs is no chain to verify). The walk reads the rotated archives
//! first (the older truth), the live chain last. One pass rebuilds:
//!
//! - the projection (`Replay.last`) — the last SLOT-BEARING
//!   decision (`claimed`/`rotated` never bear one, a pre-slot skip
//!   neither), rendered byte-identical by `render_last`;
//! - the watermark's truth — the last DECIDED instant (every decision
//!   kind, `disarmed` included);
//! - the current lifecycle's events (`Replay.lifecycle`) — the
//!   group the last journal line joined (claims and receipts group by
//!   `slot_id`; a slot-less decision completes its own lifecycle), for
//!   the report's folded state (D5).

use std::io;
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use nika_cadence::firing::{
    self, ArmGeneration, FencingToken, FiringEvent, FiringState, SkipReason,
};

use super::{FireKind, HISTORY, LastRecord, first_line_is_versioned, verify_line};

/// What the replay rebuilt — see the module doc.
pub(crate) struct Replay {
    /// The rebuilt projection (the last slot-bearing decision).
    pub last: Option<LastRecord>,
    /// The last DECIDED instant (the watermark's truth).
    pub watermark: Option<Timestamp>,
    /// The current lifecycle's events (the last journal line's group).
    lifecycle: Vec<FiringEvent>,
    /// The current lifecycle is NOT the last decision's own (an
    /// in-flight or orphaned newer slot) — the report names it then.
    lifecycle_beyond_last: bool,
    /// The current lifecycle's slot identity (the wire string).
    lifecycle_slot: Option<String>,
}

/// The report's folded state (D5).
pub(crate) struct Folded {
    /// The lifecycle's folded state.
    pub state: FiringState,
    /// The lifecycle is a NEWER slot than the last decision's.
    pub beyond_last: bool,
    /// Its slot identity (the wire string) when the chain names one.
    pub slot: Option<String>,
}

/// Replay one beat's journals into the projections' truth.
pub(crate) fn replay(dir: &Path) -> io::Result<Replay> {
    let mut walker = Walker::new();
    for journal in journals(dir)? {
        let text = std::fs::read_to_string(&journal)?;
        let live = journal.file_name().and_then(|n| n.to_str()) == Some(HISTORY);
        if live && first_line_is_versioned(&text) {
            walker.fold_chain(&text);
        } else {
            walker.fold_legacy(&text);
        }
    }
    Ok(walker.finish())
}

/// Fold the replay's lifecycle through the machine, the crash
/// detector riding: an outstanding claim whose deadline passed folds
/// to `Ambiguous` (the run MAY have happened — at-least-once honesty).
pub(crate) fn fold_replay(replayed: &Replay, now: &Timestamp) -> Option<Folded> {
    if replayed.lifecycle.is_empty() {
        return None;
    }
    let mut state = firing::fold(&replayed.lifecycle);
    if matches!(state, FiringState::Claimed | FiringState::Running)
        && let Some((fencing, deadline)) = last_claim(&replayed.lifecycle)
        && *now > deadline
    {
        state = firing::transition(state, &FiringEvent::DeadlinePassed { fencing });
    }
    Some(Folded {
        state,
        beyond_last: replayed.lifecycle_beyond_last,
        slot: replayed.lifecycle_slot.clone(),
    })
}

/// The last claim's token + deadline in the lifecycle.
fn last_claim(events: &[FiringEvent]) -> Option<(FencingToken, Timestamp)> {
    events.iter().rev().find_map(|event| match event {
        FiringEvent::Claimed {
            fencing, deadline, ..
        } => Some((*fencing, *deadline)),
        _ => None,
    })
}

/// The journals, oldest first: the rotated W2 archives, then the live
/// chain (its own classification decides the fold — an unrotated W2
/// journal folds legacy).
fn journals(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut journals: Vec<PathBuf> = entries
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("history-w2") && n.ends_with(".ndjson"))
        })
        .collect();
    journals.sort_unstable();
    let live = dir.join(HISTORY);
    if live.exists() {
        journals.push(live);
    }
    Ok(journals)
}

/// One lifecycle's group: the slot's identity (the wire string — `None`
/// for a slot-less decision's singleton) and its events in order.
struct Group {
    key: Option<String>,
    events: Vec<FiringEvent>,
}

/// The walk's fold state.
struct Walker {
    last: Option<LastRecord>,
    watermark: Option<Timestamp>,
    groups: Vec<Group>,
    /// The group the last journaled line joined (the current lifecycle).
    current: Option<usize>,
    /// The group holding the last slot-bearing DECISION.
    last_projection: Option<usize>,
}

impl Walker {
    fn new() -> Self {
        Self {
            last: None,
            watermark: None,
            groups: Vec::new(),
            current: None,
            last_projection: None,
        }
    }

    /// The versioned chain: the append's verification semantics, the
    /// invalid tail REFUSED (a read cuts nothing).
    fn fold_chain(&mut self, text: &str) {
        let mut seq = 0u64;
        let mut prev: Option<String> = None;
        for line in text.lines() {
            match verify_line(line, seq + 1, prev.as_deref()) {
                Some(hash) => {
                    seq += 1;
                    prev = Some(hash);
                    if let Ok(doc) = serde_json::from_str::<serde_json::Value>(line) {
                        self.fold_versioned(&doc);
                    }
                }
                // Detection IS the refusal: the tail never folds.
                None => break,
            }
        }
    }

    /// A W2-era journal: no chain to verify — best-effort, a line
    /// that does not parse simply folds nothing.
    fn fold_legacy(&mut self, text: &str) {
        for line in text.lines() {
            let Ok(doc) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            self.fold_legacy_line(&doc);
        }
    }

    /// One versioned line (verified by the caller).
    fn fold_versioned(&mut self, doc: &serde_json::Value) {
        let Some(kind) = doc.get("kind").and_then(serde_json::Value::as_str) else {
            return;
        };
        match kind {
            "claimed" => {
                if let Some(event) = claim_event(doc) {
                    let key = doc
                        .get("slot_id")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned);
                    self.push_event(key, event);
                }
            }
            "fired" | "skipped" | "paused" | "failed" => {
                if let Some(decided) = envelope_ts(doc) {
                    self.watermark = Some(decided);
                }
                let payload = doc.get("payload");
                let slot = payload
                    .and_then(|p| p.get("slot"))
                    .and_then(serde_json::Value::as_str)
                    .and_then(|s| s.parse::<Timestamp>().ok());
                let key = doc
                    .get("slot_id")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned);
                // A pre-slot skip is a decision (the watermark moves)
                // but never a lifecycle (it consumed no slot).
                if slot.is_none() && key.is_none() {
                    return;
                }
                let group = self.push_event(key, receipt_event(kind, payload));
                if let (Some(slot), Some(decided)) = (slot, envelope_ts(doc)) {
                    self.last = Some(LastRecord {
                        slot,
                        fired_at: decided,
                        trace: payload
                            .and_then(|p| p.get("trace"))
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_owned),
                        exit: payload
                            .and_then(|p| p.get("exit"))
                            .and_then(serde_json::Value::as_u64)
                            .and_then(|e| u8::try_from(e).ok()),
                        kind: fire_kind_of(kind),
                        generation: payload
                            .and_then(|p| p.get("gen"))
                            .and_then(serde_json::Value::as_str)
                            .and_then(ArmGeneration::from_wire),
                    });
                    self.last_projection = Some(group);
                }
            }
            // `disarmed` decides (the watermark moves) but bears no
            // slot; `rotated` is structural; the unknown folds nothing.
            "disarmed" => self.watermark = envelope_ts(doc).or(self.watermark),
            _ => {}
        }
    }

    /// One legacy line (the W2 shape: `decided_at` inside, no envelope).
    fn fold_legacy_line(&mut self, doc: &serde_json::Value) {
        let Some(kind) = doc.get("kind").and_then(serde_json::Value::as_str) else {
            return;
        };
        if !matches!(kind, "fired" | "skipped" | "paused" | "failed") {
            return;
        }
        let decided = doc
            .get("decided_at")
            .and_then(serde_json::Value::as_str)
            .and_then(|s| s.parse::<Timestamp>().ok());
        if decided.is_some() {
            self.watermark = decided;
        }
        let slot = doc
            .get("slot")
            .and_then(serde_json::Value::as_str)
            .and_then(|s| s.parse::<Timestamp>().ok());
        let exit = doc
            .get("exit")
            .and_then(serde_json::Value::as_u64)
            .and_then(|e| u8::try_from(e).ok());
        let Some(slot) = slot else {
            return; // a pre-slot decision: the watermark alone
        };
        let event = if kind == "skipped" {
            FiringEvent::Skipped {
                reason: doc
                    .get("reason")
                    .and_then(serde_json::Value::as_str)
                    .and_then(SkipReason::parse),
            }
        } else {
            FiringEvent::Finished {
                fencing: None,
                code: exit.unwrap_or(0),
            }
        };
        let group = self.push_event(None, event);
        if let Some(decided) = decided {
            self.last = Some(LastRecord {
                slot,
                fired_at: decided,
                trace: doc
                    .get("trace")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                exit,
                kind: fire_kind_of(kind),
                generation: None,
            });
            self.last_projection = Some(group);
        }
    }

    /// Join (or open) the event's lifecycle group; the last line's
    /// group is the current one.
    fn push_event(&mut self, key: Option<String>, event: FiringEvent) -> usize {
        let index = match key.as_deref() {
            Some(identity) => self
                .groups
                .iter()
                .position(|g| g.key.as_deref() == Some(identity))
                .unwrap_or_else(|| self.open_group(key)),
            None => self.open_group(None),
        };
        self.groups[index].events.push(event);
        self.current = Some(index);
        index
    }

    fn open_group(&mut self, key: Option<String>) -> usize {
        self.groups.push(Group {
            key,
            events: Vec::new(),
        });
        self.groups.len() - 1
    }

    fn finish(self) -> Replay {
        let (lifecycle, lifecycle_slot) = match self.current {
            Some(index) => (
                self.groups[index].events.clone(),
                self.groups[index].key.clone(),
            ),
            None => (Vec::new(), None),
        };
        Replay {
            last: self.last,
            watermark: self.watermark,
            lifecycle,
            lifecycle_beyond_last: self.current.is_some() && self.current != self.last_projection,
            lifecycle_slot,
        }
    }
}

/// The envelope's `ts`, parsed.
fn envelope_ts(doc: &serde_json::Value) -> Option<Timestamp> {
    doc.get("ts")
        .and_then(serde_json::Value::as_str)
        .and_then(|s| s.parse::<Timestamp>().ok())
}

/// The claim line's event (the durable claim: fencing · generation ·
/// deadline). A line that cannot name its token settles nothing.
fn claim_event(doc: &serde_json::Value) -> Option<FiringEvent> {
    let payload = doc.get("payload")?;
    let fencing = payload.get("fencing").and_then(serde_json::Value::as_u64)?;
    let deadline = payload
        .get("deadline")
        .and_then(serde_json::Value::as_str)
        .and_then(|s| s.parse::<Timestamp>().ok())?;
    let generation = payload
        .get("gen")
        .and_then(serde_json::Value::as_str)
        .and_then(ArmGeneration::from_wire);
    Some(FiringEvent::Claimed {
        fencing: FencingToken::new(fencing),
        generation,
        deadline,
    })
}

/// A decision line's event: the receipt's code classifies the
/// terminal (4 parks, 0 succeeds, the rest fails), the skip's reason
/// rides typed when known.
fn receipt_event(kind: &str, payload: Option<&serde_json::Value>) -> FiringEvent {
    if kind == "skipped" {
        return FiringEvent::Skipped {
            reason: payload
                .and_then(|p| p.get("reason"))
                .and_then(serde_json::Value::as_str)
                .and_then(SkipReason::parse),
        };
    }
    let code = payload
        .and_then(|p| p.get("exit"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|e| u8::try_from(e).ok())
        .unwrap_or(0);
    let fencing = payload
        .and_then(|p| p.get("fencing"))
        .and_then(serde_json::Value::as_u64)
        .map(FencingToken::new);
    FiringEvent::Finished { fencing, code }
}

/// The decision word — never `None` here (the callers match the four
/// first); a word this machine predates reads as a failure (the
/// cautious direction), never as a success.
fn fire_kind_of(word: &str) -> FireKind {
    match word {
        "fired" => FireKind::Fired,
        "skipped" => FireKind::Skipped,
        "paused" => FireKind::Paused,
        _ => FireKind::Failed,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use jiff::Timestamp;
    use nika_cadence::firing::{FiringEvent, FiringState, SlotId};

    use super::super::{ArmState, Claim, FireKind, HistoryEntry};
    use super::{Walker, fire_kind_of, fold_replay, journals, replay};

    fn state(tag: &str) -> (tempfile::TempDir, ArmState) {
        let dir = tempfile::Builder::new()
            .prefix(&format!("nika-arm-replay-{tag}-"))
            .tempdir()
            .expect("tmp dir");
        let state = ArmState::at_project(dir.path());
        (dir, state)
    }

    fn ts(text: &str) -> Timestamp {
        text.parse::<Timestamp>().expect("ts")
    }

    fn entry(kind: FireKind, slot: &str, decided: &str) -> HistoryEntry {
        HistoryEntry {
            slot: Some(ts(slot)),
            decided_at: ts(decided),
            kind,
            reason: None,
            trace: None,
            exit: Some(0),
            slots: None,
            slot_id: None,
            fencing: None,
            generation: None,
        }
    }

    /// (a) · the reversal, pinned: `last.json` deleted → the CHAIN
    /// replays it, byte-identical. Never-fired-on-missing dated from
    /// before the ledger.
    #[test]
    fn a_deleted_last_json_replays_byte_identical() {
        let (dir, state) = state("rebuild");
        let mut fired = entry(
            FireKind::Fired,
            "2026-08-19T03:00:00Z",
            "2026-08-19T03:02:00Z",
        );
        fired.trace = Some(".nika/traces/2026-08-19T03-02-00Z_cafe.ndjson".to_owned());
        state.record("doctor", &fired).expect("record");
        let path = dir.path().join(".nika/arm/doctor/last.json");
        let original = std::fs::read_to_string(&path).expect("last.json");
        std::fs::remove_file(&path).expect("delete the projection");
        let last = state.last("doctor").expect("W7: the chain replays it");
        let rendered = format!(
            "{{\"slot\":\"{}\",\"fired_at\":\"{}\",\"trace\":{},\"exit\":{},\"kind\":\"{}\",\"gen\":{}}}\n",
            last.slot,
            last.fired_at,
            last.trace
                .as_deref()
                .map_or("null".to_owned(), |t| format!("\"{t}\"")),
            last.exit.unwrap_or(0),
            last.kind.as_str(),
            last.generation
                .as_ref()
                .map_or("null".to_owned(), |g| format!("\"{}\"", g.as_str())),
        );
        assert_eq!(rendered, original, "the replay rebuilds byte-identical");
        assert_eq!(
            std::fs::read_to_string(&path).expect("the cache is physically rebuilt"),
            original,
            "the deleted projection is recreated byte-identical"
        );
    }

    /// (c) · a tampered line is REFUSED at replay: the fold stops at
    /// the first invalid line — and the read cuts NOTHING (the cut is
    /// the append's job).
    #[test]
    fn a_tampered_tail_is_refused_at_replay_without_cutting() {
        let (dir, state) = state("tamper");
        state
            .record(
                "doctor",
                &entry(
                    FireKind::Fired,
                    "2026-08-17T03:00:00Z",
                    "2026-08-17T03:02:00Z",
                ),
            )
            .expect("one");
        let mut skipped = entry(
            FireKind::Skipped,
            "2026-08-18T03:00:00Z",
            "2026-08-18T03:02:00Z",
        );
        skipped.reason = Some("missed:1".to_owned());
        state.record("doctor", &skipped).expect("two");
        state
            .record(
                "doctor",
                &entry(
                    FireKind::Fired,
                    "2026-08-19T03:00:00Z",
                    "2026-08-19T03:02:00Z",
                ),
            )
            .expect("three");
        let ledger = dir.path().join(".nika/arm/doctor/history.ndjson");
        let text = std::fs::read_to_string(&ledger).expect("ledger");
        std::fs::write(&ledger, text.replacen("\"seq\":2", "\"seq\":9", 1)).expect("tamper");
        std::fs::remove_file(dir.path().join(".nika/arm/doctor/last.json")).expect("delete");
        let last = state.last("doctor").expect("the valid prefix replays");
        assert_eq!(
            last.slot,
            ts("2026-08-17T03:00:00Z"),
            "line 1's decision — the tampered tail never folds"
        );
        assert_eq!(last.kind, FireKind::Fired);
        // The read is READ-ONLY: the tampered bytes survive for the
        // next append's repair.
        let after = std::fs::read_to_string(&ledger).expect("ledger");
        assert_eq!(after.lines().count(), 3, "the replay cut nothing");
    }

    /// (c) · two swapped lines break the seq continuity: the replay
    /// refuses from the first swapped position.
    #[test]
    fn a_reordered_chain_is_refused_at_replay() {
        let (dir, state) = state("reorder");
        for day in ["17", "18", "19"] {
            state
                .record(
                    "doctor",
                    &entry(
                        FireKind::Skipped,
                        &format!("2026-08-{day}T03:00:00Z"),
                        &format!("2026-08-{day}T03:02:00Z"),
                    ),
                )
                .expect("record");
        }
        let ledger = dir.path().join(".nika/arm/doctor/history.ndjson");
        let text = std::fs::read_to_string(&ledger).expect("ledger");
        let lines: Vec<&str> = text.lines().collect();
        let swapped = format!("{}\n{}\n{}\n", lines[0], lines[2], lines[1]);
        std::fs::write(&ledger, swapped).expect("swap");
        std::fs::remove_file(dir.path().join(".nika/arm/doctor/last.json")).expect("delete");
        let last = state.last("doctor").expect("the valid prefix replays");
        assert_eq!(
            last.slot,
            ts("2026-08-17T03:00:00Z"),
            "the swap refuses at position 2 — line 1's decision stands"
        );
    }

    /// (c) · a physically truncated final line is an invalid tail, not
    /// a partial decision. Replay stops before it and never rewrites the
    /// evidence; the last complete decision remains the projection.
    #[test]
    fn a_truncated_final_line_is_refused_at_replay() {
        let (dir, state) = state("truncated");
        for day in ["17", "18", "19"] {
            state
                .record(
                    "doctor",
                    &entry(
                        FireKind::Fired,
                        &format!("2026-08-{day}T03:00:00Z"),
                        &format!("2026-08-{day}T03:02:00Z"),
                    ),
                )
                .expect("record");
        }
        let ledger = dir.path().join(".nika/arm/doctor/history.ndjson");
        let text = std::fs::read_to_string(&ledger).expect("ledger");
        let cut = text.rfind('\n').expect("final newline") - 16;
        let truncated = &text[..cut];
        std::fs::write(&ledger, truncated).expect("truncate final line");
        std::fs::remove_file(dir.path().join(".nika/arm/doctor/last.json")).expect("delete");

        let last = state.last("doctor").expect("the complete prefix replays");
        assert_eq!(last.slot, ts("2026-08-18T03:00:00Z"));
        assert_eq!(
            std::fs::read_to_string(&ledger).expect("evidence"),
            truncated,
            "replay never cuts or repairs evidence"
        );
    }

    /// (c) · one altered payload byte breaks the hash: the replay folds
    /// the lines BEFORE the alteration and no further.
    #[test]
    fn an_altered_payload_is_refused_at_replay() {
        let (dir, state) = state("altered");
        state
            .record(
                "doctor",
                &entry(
                    FireKind::Fired,
                    "2026-08-17T03:00:00Z",
                    "2026-08-17T03:02:00Z",
                ),
            )
            .expect("one");
        let mut skipped = entry(
            FireKind::Skipped,
            "2026-08-18T03:00:00Z",
            "2026-08-18T03:02:00Z",
        );
        skipped.reason = Some("missed:1".to_owned());
        state.record("doctor", &skipped).expect("two");
        let mut fired = entry(
            FireKind::Fired,
            "2026-08-19T03:00:00Z",
            "2026-08-19T03:02:00Z",
        );
        fired.reason = Some("annotated".to_owned());
        state.record("doctor", &fired).expect("three");
        let ledger = dir.path().join(".nika/arm/doctor/history.ndjson");
        let text = std::fs::read_to_string(&ledger).expect("ledger");
        std::fs::write(&ledger, text.replacen("annotated", "Annotated", 1)).expect("alter");
        std::fs::remove_file(dir.path().join(".nika/arm/doctor/last.json")).expect("delete");
        let last = state.last("doctor").expect("the valid prefix replays");
        assert_eq!(
            last.slot,
            ts("2026-08-18T03:00:00Z"),
            "the alteration refuses line 3 — line 2's decision stands"
        );
        assert_eq!(last.kind, FireKind::Skipped);
    }

    /// N2 stands: no chain at all still reads as never-fired (the
    /// direction that is truly safe).
    #[test]
    fn an_absent_chain_still_reads_never_fired() {
        let (_dir, state) = state("absent");
        assert!(
            state.last("ghost").is_none(),
            "no last.json AND no chain → never fired"
        );
    }

    /// A W2-era journal (no `schema` on its first line) folds too — the
    /// projection rebuilds from the legacy decisions, and the read
    /// rotates NOTHING (rotation is the writer's gesture, N4).
    #[test]
    fn a_legacy_journal_rebuilds_the_projection_without_rotating() {
        let (dir, state) = state("legacy");
        let sidecar = dir.path().join(".nika/arm/doctor");
        std::fs::create_dir_all(&sidecar).expect("sidecar");
        let legacy = concat!(
            "{\"slot\":\"2026-08-18T03:00:00Z\",\"decided_at\":\"2026-08-18T03:02:00Z\",\"kind\":\"fired\",\"reason\":null,\"trace\":null,\"exit\":0,\"slots\":null}\n",
            "{\"slot\":\"2026-08-19T03:00:00Z\",\"decided_at\":\"2026-08-19T03:02:00Z\",\"kind\":\"skipped\",\"reason\":\"missed:1\",\"trace\":null,\"exit\":0,\"slots\":null}\n",
        );
        std::fs::write(sidecar.join("history.ndjson"), legacy).expect("legacy");
        let last = state.last("doctor").expect("the legacy journal replays");
        assert_eq!(last.slot, ts("2026-08-19T03:00:00Z"));
        assert_eq!(last.kind, FireKind::Skipped);
        assert_eq!(
            last.fired_at,
            ts("2026-08-19T03:02:00Z"),
            "decided_at becomes fired_at"
        );
        assert!(
            !sidecar.join("history-w2.ndjson").exists(),
            "the read rotated nothing"
        );
        let after = std::fs::read_to_string(sidecar.join("history.ndjson")).expect("ledger");
        assert_eq!(after, legacy, "verbatim — the read never writes");
    }

    /// The deadline is an open boundary: equality is still claimed;
    /// only a clock strictly beyond it makes the uncertainty visible.
    #[test]
    fn a_claim_becomes_ambiguous_only_after_its_deadline() {
        let (dir, state) = state("deadline-boundary");
        let claim = Claim {
            slot_id: SlotId::derive(
                "doctor.nika.yaml",
                "TZ=UTC 0 3 * * *",
                &ts("2026-08-19T03:00:00Z").to_zoned(jiff::tz::TimeZone::UTC),
            ),
            generation: None,
            deadline: ts("2026-08-20T03:00:00Z"),
            decided_at: ts("2026-08-19T03:02:00Z"),
        };
        state.record_claim("doctor", &claim).expect("claim");
        let replayed = replay(&dir.path().join(".nika/arm/doctor")).expect("replay");
        assert_eq!(
            fold_replay(&replayed, &claim.deadline).expect("fold").state,
            FiringState::Claimed
        );
        assert_eq!(
            fold_replay(&replayed, &ts("2026-08-20T03:00:00.000000001Z"))
                .expect("fold")
                .state,
            FiringState::Ambiguous
        );
    }

    /// Missing is empty; an existing non-directory is an I/O error.
    /// The archive selector requires both the prefix and suffix.
    #[test]
    fn journal_discovery_is_strict_about_absence_errors_and_names() {
        let (dir, _state) = state("journal-discovery");
        let missing = dir.path().join("missing");
        assert!(journals(&missing).expect("missing is empty").is_empty());

        let not_a_dir = dir.path().join("file");
        std::fs::write(&not_a_dir, "x").expect("file");
        assert!(
            journals(&not_a_dir).is_err(),
            "a file is not an absent directory"
        );

        let sidecar = dir.path().join("sidecar");
        std::fs::create_dir_all(&sidecar).expect("sidecar");
        for name in [
            "history-w2.ndjson",
            "history-w2-2.ndjson",
            "history-w2.txt",
            "not-history-w2.ndjson",
        ] {
            std::fs::write(sidecar.join(name), "\n").expect("journal candidate");
        }
        let names: Vec<String> = journals(&sidecar)
            .expect("walk")
            .into_iter()
            .map(|path| {
                path.file_name()
                    .expect("name")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(names, vec!["history-w2-2.ndjson", "history-w2.ndjson"]);
    }

    /// A disarm is a decision for the watermark even though it carries
    /// no slot and therefore never becomes `last.json`.
    #[test]
    fn disarmed_advances_replayed_watermark_without_a_projection() {
        let (dir, state) = state("disarmed");
        let mut disarmed = entry(
            FireKind::Disarmed,
            "2026-08-19T03:00:00Z",
            "2026-08-19T03:02:00Z",
        );
        disarmed.slot = None;
        disarmed.exit = None;
        state.record("doctor", &disarmed).expect("disarm");
        let replayed = replay(&dir.path().join(".nika/arm/doctor")).expect("replay");
        assert!(replayed.last.is_none());
        assert_eq!(replayed.watermark, Some(ts("2026-08-19T03:02:00Z")));
    }

    /// Legacy skipped decisions enter the typed machine as skipped,
    /// not as a successful finish.
    #[test]
    fn a_legacy_skip_folds_to_skipped() {
        let (dir, state) = state("legacy-skip-state");
        let sidecar = dir.path().join(".nika/arm/doctor");
        std::fs::create_dir_all(&sidecar).expect("sidecar");
        std::fs::write(
            sidecar.join("history.ndjson"),
            "{\"slot\":\"2026-08-19T03:00:00Z\",\"decided_at\":\"2026-08-19T03:02:00Z\",\"kind\":\"skipped\",\"reason\":\"overlap\",\"exit\":0}\n",
        )
        .expect("legacy");
        let folded = state
            .folded("doctor", &ts("2026-08-19T03:03:00Z"))
            .expect("folded");
        assert_eq!(folded.state, FiringState::Skipped);
    }

    /// Lifecycle grouping rejoins exactly the matching slot, and a
    /// group's own projection is not reported as a newer lifecycle.
    #[test]
    fn walker_groups_by_exact_slot_and_marks_only_newer_lifecycles() {
        let mut walker = Walker::new();
        let event = || FiringEvent::Skipped { reason: None };
        let first = walker.push_event(Some("slot-a".to_owned()), event());
        let second = walker.push_event(Some("slot-b".to_owned()), event());
        let again = walker.push_event(Some("slot-a".to_owned()), event());
        assert_eq!((first, second, again), (0, 1, 0));
        assert_eq!(walker.groups.len(), 2);
        walker.last_projection = Some(first);
        let replayed = walker.finish();
        assert!(!replayed.lifecycle_beyond_last);
        assert_eq!(replayed.lifecycle_slot.as_deref(), Some("slot-a"));
    }

    /// Paused stays paused in the projection vocabulary.
    #[test]
    fn paused_projection_kind_is_not_folded_into_failure() {
        assert_eq!(fire_kind_of("paused"), FireKind::Paused);
    }
}
