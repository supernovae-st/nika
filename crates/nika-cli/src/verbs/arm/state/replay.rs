// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Filesystem adapter for the pure cadence ledger replay.

use std::io;
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use nika_cadence::firing::FiringState;
use nika_cadence::ledger::{
    JournalFormat, LastRecord, archive_commitment_matches, classify_journal,
    journal_snapshot_matches,
};

use super::HISTORY;

pub(crate) struct Replay {
    pub last: Option<LastRecord>,
    pub watermark: Option<Timestamp>,
    journals: Vec<(String, bool)>,
}

pub(crate) struct Folded {
    pub state: FiringState,
    pub beyond_last: bool,
    pub slot: Option<String>,
}

type JournalSnapshot = (String, String, bool);

pub(crate) fn replay(dir: &Path) -> io::Result<Replay> {
    let journals = journal_texts(dir)?;
    let (last, watermark) = nika_cadence::ledger::replay_projection(
        journals
            .iter()
            .map(|(text, versioned)| (text.as_str(), *versioned)),
    )
    .ok_or_else(invalid_journal)?;
    Ok(Replay {
        last,
        watermark,
        journals,
    })
}

pub(super) fn journal_texts(dir: &Path) -> io::Result<Vec<(String, bool)>> {
    validate_snapshot(dir, read_snapshot(dir)?)
}

fn read_snapshot(dir: &Path) -> io::Result<Vec<JournalSnapshot>> {
    journals(dir)?
        .into_iter()
        .map(|path| {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(invalid_journal)?
                .to_owned();
            let live = name == HISTORY;
            Ok((name, std::fs::read_to_string(path)?, live))
        })
        .collect()
}

fn validate_snapshot(
    dir: &Path,
    snapshot: Vec<JournalSnapshot>,
) -> io::Result<Vec<(String, bool)>> {
    let borrowed: Vec<(&str, &str, bool)> = snapshot
        .iter()
        .map(|(name, text, live)| (name.as_str(), text.as_str(), *live))
        .collect();
    let anchor = super::read_chain_anchor(dir)?;
    if !journal_snapshot_matches(anchor.as_deref(), &borrowed) {
        return Err(invalid_journal());
    }
    Ok(snapshot
        .into_iter()
        .map(|(_, text, _)| {
            let versioned = matches!(classify_journal(&text), Some(JournalFormat::Versioned));
            (text, versioned)
        })
        .collect())
}

pub(super) fn validate_archive_commitment(dir: &Path, live: &str) -> io::Result<()> {
    if !matches!(classify_journal(live), Some(JournalFormat::Versioned)) {
        return Ok(());
    }
    let archives = archive_texts(dir)?;
    let borrowed: Vec<(&str, &str)> = archives
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    archive_commitment_matches(live, &borrowed)
        .then_some(())
        .ok_or_else(invalid_journal)
}

pub(super) fn archive_texts(dir: &Path) -> io::Result<Vec<(String, String)>> {
    read_snapshot(dir)?
        .into_iter()
        .filter(|(_, _, live)| !live)
        .map(|(name, text, _)| {
            (classify_journal(&text) == Some(JournalFormat::Legacy))
                .then_some((name, text))
                .ok_or_else(invalid_journal)
        })
        .collect()
}

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

fn journals(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_file() && path_archive_ordinal(&entry.path()).is_some() {
            paths.push(entry.path());
        }
    }
    paths.sort_by_key(|path| path_archive_ordinal(path));
    let live = dir.join(HISTORY);
    if live.exists() {
        paths.push(live);
    }
    Ok(paths)
}

pub(super) fn latest_archive(dir: &Path) -> io::Result<Option<PathBuf>> {
    Ok(journals(dir)?
        .into_iter()
        .filter(|path| path.file_name().and_then(|name| name.to_str()) != Some(HISTORY))
        .next_back())
}

fn path_archive_ordinal(path: &Path) -> Option<u32> {
    let name = path.file_name()?.to_str()?;
    nika_cadence::ledger::archive_ordinal(name)
}

fn invalid_journal() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "arm ledger: journal dialect or chain is invalid",
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use jiff::Timestamp;
    use nika_cadence::firing::{FiringState, SlotId};

    use super::super::{ArmState, Claim, FireKind, HISTORY, HistoryEntry, write_chain_anchor};
    use super::{fold_replay, journals, read_snapshot, replay, validate_snapshot};

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
        assert!(
            state.last("doctor").is_none(),
            "the durable head refuses laundering an anchored tamper"
        );
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
        assert!(
            state.last("doctor").is_none(),
            "the durable head refuses laundering an anchored reorder"
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

        assert!(
            state.last("doctor").is_none(),
            "the durable head refuses laundering an anchored truncation"
        );
        assert_eq!(
            std::fs::read_to_string(&ledger).expect("evidence"),
            truncated,
            "replay never cuts or repairs evidence"
        );
    }

    /// Removing a complete suffix leaves a self-consistent hash prefix. The
    /// durable high-water anchor must still make that rollback loud, without
    /// laundering the older prefix into either projection cache.
    #[test]
    fn a_clean_tail_deletion_is_refused_by_the_durable_head() {
        let (dir, state) = state("clean-tail-deletion");
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
        let sidecar = dir.path().join(".nika/arm/doctor");
        let ledger = sidecar.join("history.ndjson");
        let last_before = std::fs::read_to_string(sidecar.join("last.json")).expect("last cache");
        let watermark_before =
            std::fs::read_to_string(sidecar.join("watermark")).expect("watermark");
        let text = std::fs::read_to_string(&ledger).expect("ledger");
        let mut lines: Vec<&str> = text.lines().collect();
        lines.pop().expect("tail");
        std::fs::write(&ledger, format!("{}\n", lines.join("\n"))).expect("clean truncation");

        assert!(
            replay(&sidecar).is_err(),
            "rollback must be named as invalid"
        );
        assert!(state.last("doctor").is_none(), "no older PROUVÉ cache");
        assert_eq!(
            std::fs::read_to_string(sidecar.join("last.json")).expect("last survives"),
            last_before
        );
        assert_eq!(
            std::fs::read_to_string(sidecar.join("watermark")).expect("watermark survives"),
            watermark_before
        );
        let error = state
            .record(
                "doctor",
                &entry(
                    FireKind::Fired,
                    "2026-08-20T03:00:00Z",
                    "2026-08-20T03:02:00Z",
                ),
            )
            .expect_err("append cannot launder a clean rollback");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        std::fs::remove_file(sidecar.join("head.json")).expect("delete durable head");
        assert!(
            replay(&sidecar).is_err(),
            "a non-empty versioned chain without its head fails closed"
        );
    }

    /// Discovery, validation, and fold consume one immutable snapshot. Live
    /// deletion and archive mutation after its read cannot select new bytes.
    #[test]
    fn snapshot_validation_and_fold_share_exact_buffers() {
        for delete_archive in [false, true] {
            let (dir, state) = state(if delete_archive {
                "snapshot-delete"
            } else {
                "snapshot-mutate"
            });
            let sidecar = dir.path().join(".nika/arm/doctor");
            std::fs::create_dir_all(&sidecar).expect("sidecar");
            let legacy = "{\"slot\":\"2026-08-17T03:00:00Z\",\"decided_at\":\"2026-08-17T03:02:00Z\",\"kind\":\"fired\"}\n";
            std::fs::write(sidecar.join(HISTORY), legacy).expect("legacy");
            state
                .record(
                    "doctor",
                    &entry(
                        FireKind::Fired,
                        "2026-08-19T03:00:00Z",
                        "2026-08-19T03:02:00Z",
                    ),
                )
                .expect("migrate and record");
            let live = std::fs::read_to_string(sidecar.join(HISTORY)).expect("live");
            let snapshot = read_snapshot(&sidecar).expect("one filesystem snapshot");

            std::fs::remove_file(sidecar.join(HISTORY)).expect("delete live after read");
            let archive = sidecar.join("history-w2.ndjson");
            if delete_archive {
                std::fs::remove_file(archive).expect("delete archive after read");
            } else {
                std::fs::write(archive, "tampered\n").expect("mutate archive after read");
            }

            let journals = validate_snapshot(&sidecar, snapshot)
                .expect("validation consumes the captured bytes only");
            assert_eq!(journals, vec![(legacy.to_owned(), false), (live, true)]);
        }
    }

    #[test]
    fn an_anchored_chain_refuses_empty_or_absent_live_history() {
        let (dir, state) = state("empty-or-absent-live");
        state
            .record(
                "doctor",
                &entry(
                    FireKind::Fired,
                    "2026-08-19T03:00:00Z",
                    "2026-08-19T03:02:00Z",
                ),
            )
            .expect("anchored event");
        let sidecar = dir.path().join(".nika/arm/doctor");
        let ledger = sidecar.join("history.ndjson");
        let original = std::fs::read_to_string(&ledger).expect("history");

        std::fs::write(&ledger, "").expect("truncate to zero");
        assert!(replay(&sidecar).is_err(), "empty live cannot bypass head");
        assert!(state.last("doctor").is_none(), "no false PROUVÉ state");
        assert!(state.tallies("doctor").is_none(), "no false tally");

        std::fs::write(&ledger, original).expect("restore");
        std::fs::remove_file(&ledger).expect("delete live");
        assert!(replay(&sidecar).is_err(), "absent live cannot bypass head");
        assert!(state.last("doctor").is_none(), "no false PROUVÉ state");
        assert!(state.tallies("doctor").is_none(), "no false tally");
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
        assert!(
            state.last("doctor").is_none(),
            "the durable head refuses laundering an anchored payload change"
        );
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
            "history-w2-02.ndjson",
            "history-w2-4294967295.ndjson",
            "history-w2.txt",
            "not-history-w2.ndjson",
        ] {
            std::fs::write(sidecar.join(name), "\n").expect("journal candidate");
        }
        #[cfg(unix)]
        {
            std::fs::write(dir.path().join("outside.ndjson"), "hijack\n").expect("target");
            std::os::unix::fs::symlink(
                dir.path().join("outside.ndjson"),
                sidecar.join("history-w2-3.ndjson"),
            )
            .expect("archive symlink");
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
        assert_eq!(names, vec!["history-w2.ndjson", "history-w2-2.ndjson"]);
    }

    /// Repeated W2 recoveries replay by rotation ordinal, not lexical path.
    /// The suffixed archive is newer and therefore owns the projection.
    #[test]
    fn multiple_rotations_replay_the_newest_archive_last() {
        let (dir, _state) = state("multiple-rotations");
        let sidecar = dir.path().join(".nika/arm/doctor");
        std::fs::create_dir_all(&sidecar).expect("sidecar");
        let old = "{\"slot\":\"2026-08-18T03:00:00Z\",\"decided_at\":\"2026-08-18T03:02:00Z\",\"kind\":\"fired\",\"exit\":0}\n";
        let new = "{\"slot\":\"2026-08-19T03:00:00Z\",\"decided_at\":\"2026-08-19T03:02:00Z\",\"kind\":\"skipped\",\"reason\":\"missed:1\",\"exit\":0}\n";
        std::fs::write(sidecar.join("history-w2.ndjson"), old).expect("old archive");
        std::fs::write(sidecar.join("history-w2-2.ndjson"), new).expect("new archive");
        let payload = nika_cadence::ledger::rotation_payload(&[
            ("history-w2.ndjson", old),
            ("history-w2-2.ndjson", new),
        ])
        .expect("commit archives");
        let (genesis, hash) = nika_cadence::ledger::ledger_line(
            1,
            ts("2026-08-19T03:02:01Z"),
            "rotated",
            None,
            &payload,
            None,
        )
        .expect("genesis");
        std::fs::write(sidecar.join(HISTORY), format!("{genesis}\n")).expect("live");
        write_chain_anchor(&sidecar, 1, Some(&hash)).expect("head");
        let replayed = replay(&sidecar).expect("replay");
        let last = replayed.last.expect("projection");
        assert_eq!(last.kind, FireKind::Skipped);
        assert_eq!(last.slot, ts("2026-08-19T03:00:00Z"));
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
