// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Filesystem adapter for the pure cadence ledger replay.
//!
//! Archive discovery and UTF-8 reads stay at the CLI effect edge. Chain
//! verification, lifecycle grouping, projection rebuilding, and the deadline
//! fold live in `nika_cadence::ledger`.

use std::io;
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use nika_cadence::firing::FiringState;
use nika_cadence::ledger::{LastRecord, first_line_is_versioned};

use super::HISTORY;

/// Projection plus the borrowed-at-fold journal material.
pub(crate) struct Replay {
    pub last: Option<LastRecord>,
    pub watermark: Option<Timestamp>,
    journals: Vec<(String, bool)>,
}

/// The report's folded lifecycle.
pub(crate) struct Folded {
    pub state: FiringState,
    pub beyond_last: bool,
    pub slot: Option<String>,
}

/// Read one beat's journals and replay them oldest-first.
pub(crate) fn replay(dir: &Path) -> io::Result<Replay> {
    let paths = journals(dir)?;
    let mut texts = Vec::with_capacity(paths.len());
    let mut versioned = Vec::with_capacity(paths.len());
    for path in paths {
        let text = std::fs::read_to_string(&path)?;
        let live = path.file_name().and_then(|name| name.to_str()) == Some(HISTORY);
        versioned.push(live && first_line_is_versioned(&text));
        texts.push(text);
    }
    let journals: Vec<(String, bool)> = texts.into_iter().zip(versioned).collect();
    let (last, watermark) = nika_cadence::ledger::replay_projection(
        journals
            .iter()
            .map(|(text, versioned)| (text.as_str(), *versioned)),
    );
    Ok(Replay {
        last,
        watermark,
        journals,
    })
}

/// Fold the replayed journals through the pure cadence machine.
pub(crate) fn fold_replay(replayed: &Replay, now: &Timestamp) -> Option<Folded> {
    let (state, beyond_last, slot) = nika_cadence::ledger::replay_state(
        replayed
            .journals
            .iter()
            .map(|(text, versioned)| (text.as_str(), *versioned)),
        now,
    )?;
    Some(Folded {
        state,
        beyond_last,
        slot,
    })
}

/// Discover W2 archives then the live journal, oldest first.
fn journals(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("history-w2") && name.ends_with(".ndjson"))
        })
        .collect();
    paths.sort_unstable();
    let live = dir.join(HISTORY);
    if live.exists() {
        paths.push(live);
    }
    Ok(paths)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use jiff::Timestamp;
    use nika_cadence::firing::{FiringState, SlotId};

    use super::super::{ArmState, Claim, FireKind, HistoryEntry};
    use super::{fold_replay, journals, replay};

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
}
