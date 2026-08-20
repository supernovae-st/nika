// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use nika_cadence::FencingToken;

fn state(tag: &str) -> (tempfile::TempDir, ArmState) {
    let dir = tempfile::Builder::new()
        .prefix(&format!("nika-arm-state-{tag}-"))
        .tempdir()
        .expect("tmp dir");
    let state = ArmState::at_project(dir.path());
    (dir, state)
}

fn at(text: &str) -> Zoned {
    text.parse::<Timestamp>()
        .expect("ts")
        .to_zoned(jiff::tz::TimeZone::UTC)
}

fn entry(kind: FireKind) -> HistoryEntry {
    HistoryEntry {
        slot: Some(ts("2026-08-19T03:00:00Z")),
        decided_at: ts("2026-08-19T03:02:00Z"),
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

fn ts(text: &str) -> Timestamp {
    text.parse::<Timestamp>().expect("ts")
}

/// (a) No state reads as never-fired (N2); the recorded slot reads
/// back as the planner's `last_fired`.
#[test]
fn last_fired_is_none_then_the_recorded_slot() {
    let (_dir, state) = state("last");
    assert!(state.last_fired("doctor").is_none(), "N2: no state");
    state
        .record("doctor", &entry(FireKind::Fired))
        .expect("record");
    let fired = state.last_fired("doctor").expect("the recorded slot");
    let expected: Timestamp = "2026-08-19T03:00:00Z".parse().expect("ts");
    assert_eq!(fired.timestamp(), expected);
}

/// (b) A lock whose pid is THIS process is a LIVE holder.
#[test]
fn a_lock_held_by_a_living_owner_refuses_the_takeover() {
    let (_dir, state) = state("live");
    let now = at("2026-08-19T03:02:00Z");
    let me = std::process::id();
    let first = state.try_lock("doctor", me, &now).expect("lock");
    assert_eq!(first, LockOutcome::Acquired);
    let second = state
        .try_lock("doctor", me.wrapping_add(1), &now)
        .expect("re-lock");
    assert_eq!(second, LockOutcome::HeldAlive { pid: me });
    state.release("doctor").expect("release");
}

/// (c) A lock whose pid cannot exist (999999 dies past macOS's and
/// Linux's `pid_max`) is a crash remnant — taken over.
#[test]
fn a_dead_owner_s_lock_is_taken_over() {
    let (_dir, state) = state("stale");
    let now = at("2026-08-19T03:02:00Z");
    let dir = state.root().join("doctor");
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(
        dir.join("lock"),
        "{\"pid\":999999,\"started_at\":\"2026-08-19T03:00:00Z\"}\n",
    )
    .expect("remnant");
    let outcome = state
        .try_lock("doctor", std::process::id(), &now)
        .expect("lock");
    assert_eq!(
        outcome,
        LockOutcome::StaleTaken {
            old_pid: Some(999_999)
        }
    );
    // The takeover rewrote the file with OUR live pid.
    let body = std::fs::read_to_string(dir.join("lock")).expect("lock body");
    assert!(body.contains(&std::process::id().to_string()), "{body}");
    state.release("doctor").expect("release");
}

/// An unparseable lock proves no live owner — stale, taken over.
#[test]
fn a_corrupt_lock_is_taken_over() {
    let (_dir, state) = state("corrupt");
    let now = at("2026-08-19T03:02:00Z");
    let dir = state.root().join("doctor");
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join("lock"), "not json\n").expect("remnant");
    let outcome = state
        .try_lock("doctor", std::process::id(), &now)
        .expect("lock");
    assert!(
        matches!(outcome, LockOutcome::StaleTaken { .. }),
        "{outcome:?}"
    );
    state.release("doctor").expect("release");
}

/// (d) Two decisions = two history lines, append-only.
#[test]
fn two_records_append_two_history_lines() {
    let (_dir, state) = state("hist");
    let mut skipped = entry(FireKind::Skipped);
    skipped.reason = Some("missed:1".to_owned());
    state
        .record("doctor", &entry(FireKind::Fired))
        .expect("one");
    state.record("doctor", &skipped).expect("two");
    let text =
        std::fs::read_to_string(state.root().join("doctor/history.ndjson")).expect("history");
    assert_eq!(text.lines().count(), 2, "{text}");
    assert!(text.contains("\"kind\":\"fired\""), "{text}");
    assert!(text.contains("\"reason\":\"missed:1\""), "{text}");
    // The tallies the report prints ride the same journal.
    assert_eq!(state.tallies("doctor"), Some((1, 1)));
    // … and last.json carries the LAST decision.
    let last = state.last("doctor").expect("last.json");
    assert_eq!(last.kind, FireKind::Skipped);
}

/// The orphan walk names sidecar dirs no registry knows — N4:
/// reported, never erased.
#[test]
fn a_sidecar_dir_without_a_registry_entry_is_an_orphan() {
    let (_dir, state) = state("orphan");
    state
        .record("doctor", &entry(FireKind::Fired))
        .expect("rec");
    state.record("ghost", &entry(FireKind::Fired)).expect("rec");
    let orphans = state.orphans(&["doctor".to_owned()]);
    assert_eq!(orphans, vec!["ghost".to_owned()]);
    // … and the orphan's record DEMEURE (the walk never writes).
    assert!(state.root().join("ghost/last.json").exists());
}

/// Release is idempotent — an absent lock is released.
#[test]
fn release_without_a_lock_is_ok() {
    let (_dir, state) = state("rel");
    state.release("doctor").expect("idempotent");
}

/// Idempotence covers absence only: a lock path of the wrong kind
/// is corruption and must remain loud.
#[test]
fn release_refuses_a_lock_path_that_is_a_directory() {
    let (_dir, state) = state("release-dir");
    std::fs::create_dir_all(state.root().join("doctor/lock")).expect("lock directory");
    let error = state
        .release("doctor")
        .expect_err("a directory is not an absent lock");
    assert_ne!(error.kind(), io::ErrorKind::NotFound);
}

/// The migration walk distinguishes an absent sidecar from a path
/// that exists but cannot be walked.
#[test]
fn beat_dirs_refuses_a_sidecar_root_that_is_a_file() {
    let (dir, state) = state("beat-dirs-file");
    std::fs::create_dir_all(dir.path().join(".nika")).expect(".nika");
    std::fs::write(state.root(), "not a directory").expect("arm file");
    let error = state.beat_dirs().expect_err("the walk must refuse");
    assert_ne!(error.kind(), io::ErrorKind::NotFound);
}

/// Restore a directory's mode on drop — the tempdir cleanup needs
/// the write bit back, panic or not.
#[cfg(unix)]
struct ModeGuard(PathBuf);

#[cfg(unix)]
impl Drop for ModeGuard {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
    }
}

/// The watermark IS the decided instant, durable — and a sidecar
/// that refuses the write fails the record LOUDLY (a fire without
/// its record is a fire that re-fires: the error propagates, the
/// firer's line says it, exit ENV).
#[cfg(unix)]
#[test]
fn the_watermark_tracks_the_decision_and_a_readonly_sidecar_fails_loudly() {
    use std::os::unix::fs::PermissionsExt as _;
    let (dir, state) = state("watermark");
    let outcome = state
        .record("doctor", &entry(FireKind::Fired))
        .expect("record");
    assert_eq!(outcome.seq, 1, "the genesis line");
    assert_eq!(outcome.repaired, 0, "a clean append");
    let sidecar = dir.path().join(".nika/arm/doctor");
    let text = std::fs::read_to_string(sidecar.join("watermark")).expect("watermark");
    assert_eq!(text, "2026-08-19T03:02:00Z\n");
    assert!(
        text.trim().parse::<Timestamp>().is_ok(),
        "the watermark parses as RFC 3339: {text}"
    );
    // A read-only sidecar: the record REFUSES — loudly.
    std::fs::set_permissions(&sidecar, std::fs::Permissions::from_mode(0o555)).expect("chmod 555");
    let _restore = ModeGuard(sidecar);
    assert!(
        state.record("doctor", &entry(FireKind::Fired)).is_err(),
        "a read-only sidecar refuses the record"
    );
}

/// R4 · an orphan claim is VISIBLE; the matching receipt (same slot
/// identity, fencing = the claim's seq) settles it.
#[test]
fn an_orphan_claim_is_visible_until_its_receipt_lands() {
    let (_dir, state) = state("unsettled");
    let identity = SlotId::derive(
        "workflows/doctor.nika.yaml",
        "TZ=UTC 0 3 * * *",
        &at("2026-08-19T03:00:00Z"),
    );
    let claim = Claim {
        slot_id: identity.clone(),
        generation: None,
        deadline: ts("2026-08-20T03:00:00Z"),
        decided_at: ts("2026-08-19T03:02:00Z"),
    };
    let claimed = state.record_claim("doctor", &claim).expect("claim");
    assert_eq!(claimed.seq, 1);
    let orphans = state.unsettled("doctor");
    assert_eq!(orphans.len(), 1, "the orphan is visible: {orphans:?}");
    assert_eq!(orphans[0].seq, 1);
    assert_eq!(orphans[0].slot_id, identity);
    assert_eq!(orphans[0].deadline, ts("2026-08-20T03:00:00Z"));
    // The receipt settles it — the same slot identity, fencing the
    // claim's seq.
    let mut receipt = entry(FireKind::Fired);
    receipt.slot_id = Some(identity);
    receipt.fencing = Some(FencingToken::new(claimed.seq));
    state.record("doctor", &receipt).expect("receipt");
    assert!(
        state.unsettled("doctor").is_empty(),
        "settled by the receipt"
    );
}

/// A receipt settles exactly one claim: it must follow the claim
/// and match both its slot identity and its fencing token.
#[test]
fn a_receipt_with_the_wrong_slot_or_fence_settles_nothing() {
    let (_dir, state) = state("unsettled-exact");
    let first = SlotId::derive(
        "a.nika.yaml",
        "TZ=UTC 0 3 * * *",
        &at("2026-08-19T03:00:00Z"),
    );
    let other = SlotId::derive(
        "b.nika.yaml",
        "TZ=UTC 0 3 * * *",
        &at("2026-08-19T03:00:00Z"),
    );
    let claim = Claim {
        slot_id: first.clone(),
        generation: None,
        deadline: ts("2026-08-20T03:00:00Z"),
        decided_at: ts("2026-08-19T03:02:00Z"),
    };
    let claimed = state.record_claim("doctor", &claim).expect("claim");

    let mut wrong_slot = entry(FireKind::Failed);
    wrong_slot.slot_id = Some(other);
    wrong_slot.fencing = Some(FencingToken::new(claimed.seq));
    state
        .record("doctor", &wrong_slot)
        .expect("wrong slot receipt");

    let mut wrong_fence = entry(FireKind::Failed);
    wrong_fence.slot_id = Some(first.clone());
    wrong_fence.fencing = Some(FencingToken::new(claimed.seq + 99));
    state
        .record("doctor", &wrong_fence)
        .expect("wrong fence receipt");

    let unsettled = state.unsettled("doctor");
    assert_eq!(
        unsettled.len(),
        1,
        "neither near-match settles: {unsettled:?}"
    );
    assert_eq!(unsettled[0].slot_id, first);
}

/// A receipt that predates a future claim cannot settle it, even
/// when its predicted fencing token and slot happen to match.
#[test]
fn an_earlier_receipt_does_not_settle_a_later_claim() {
    let (_dir, state) = state("unsettled-order");
    let identity = SlotId::derive(
        "a.nika.yaml",
        "TZ=UTC 0 3 * * *",
        &at("2026-08-19T03:00:00Z"),
    );
    let mut early = entry(FireKind::Failed);
    early.slot_id = Some(identity.clone());
    early.fencing = Some(FencingToken::new(2));
    state.record("doctor", &early).expect("early receipt");
    let claim = Claim {
        slot_id: identity,
        generation: None,
        deadline: ts("2026-08-20T03:00:00Z"),
        decided_at: ts("2026-08-19T03:03:00Z"),
    };
    let claimed = state.record_claim("doctor", &claim).expect("claim");
    assert_eq!(claimed.seq, 2, "the receipt predicted this token");
    assert_eq!(
        state.unsettled("doctor").len(),
        1,
        "order is part of settlement"
    );
}

/// R5 · the slot identity's known vector RELOCATED to nika-cadence
/// in W7 (the machine owns the newtype) — this pin guards the
/// delegation: the wire the ledger writes derives there.
#[test]
fn the_slot_id_derives_in_the_cadence_machine() {
    let identity = SlotId::derive(
        "workflows/doctor.nika.yaml",
        "TZ=UTC 0 3 * * *",
        &at("2026-08-19T03:00:00Z"),
    );
    assert_eq!(identity.as_str().len(), 64);
    assert_eq!(
        identity,
        SlotId::from_wire(identity.as_str()).expect("wire")
    );
}

/// R5 · the versioned envelope: every field present, the fixed byte
/// order, `decided_at` promoted to the envelope's `ts`, the genesis
/// linkage — and the hash recomputed INDEPENDENTLY over the line's
/// own bytes. A second record links to the first's hash and the
/// chain verifies clean (no repair reported).
#[test]
fn the_envelope_is_versioned_and_the_chain_verifies() {
    use sha2::Digest as _;
    let (dir, state) = state("envelope");
    state
        .record("doctor", &entry(FireKind::Fired))
        .expect("record");
    let text = std::fs::read_to_string(dir.path().join(".nika/arm/doctor/history.ndjson"))
        .expect("ledger");
    let line = text.lines().next().expect("one line");
    let doc: serde_json::Value = serde_json::from_str(line).expect("json");
    for field in [
        "schema",
        "v",
        "seq",
        "ts",
        "kind",
        "slot_id",
        "payload",
        "prev_hash",
        "hash",
    ] {
        assert!(doc.get(field).is_some(), "missing {field}: {line}");
    }
    assert_eq!(doc["schema"], "nika/arm-event@1");
    assert_eq!(doc["v"], 1);
    assert_eq!(doc["seq"], 1);
    assert_eq!(doc["ts"], "2026-08-19T03:02:00Z");
    assert_eq!(doc["kind"], "fired");
    assert!(doc["prev_hash"].is_null(), "the genesis line");
    // The payload keeps the W2 fields; `decided_at` moved up.
    assert_eq!(doc["payload"]["slot"], "2026-08-19T03:00:00Z");
    assert!(doc["payload"].get("decided_at").is_none(), "{line}");
    // The hash, recomputed by the test itself: prev-as-JSON + \n +
    // the line's exact bytes up to the hash field.
    let prefix = &line[..line.rfind(",\"hash\":\"").expect("the hash field")];
    let expected = format!(
        "{:x}",
        sha2::Sha256::digest(format!("null\n{prefix}").as_bytes())
    );
    assert_eq!(doc["hash"], expected, "{line}");
    // A second line links to the first's hash — and its own append
    // reports a chain that verified clean.
    let mut second = entry(FireKind::Skipped);
    second.reason = Some("missed:1".to_owned());
    let outcome = state.record("doctor", &second).expect("record");
    assert_eq!(outcome.seq, 2);
    assert_eq!(outcome.repaired, 0, "the chain verified clean");
    let text = std::fs::read_to_string(dir.path().join(".nika/arm/doctor/history.ndjson"))
        .expect("ledger");
    let follow: serde_json::Value =
        serde_json::from_str(text.lines().nth(1).expect("line 2")).expect("json");
    assert_eq!(follow["prev_hash"], doc["hash"], "linked to its parent");
}

/// Every projection decision word round-trips, and free text stays
/// valid JSON even when it contains quotes, slashes, and controls.
#[test]
fn projection_kinds_and_free_text_round_trip() {
    for kind in [
        FireKind::Fired,
        FireKind::Skipped,
        FireKind::Paused,
        FireKind::Failed,
    ] {
        assert_eq!(FireKind::parse_projection(kind.as_str()), Some(kind));
    }
    assert_eq!(
        FireKind::parse_projection("disarmed"),
        None,
        "history-only kind"
    );

    let (dir, state) = state("escaped-json");
    let mut decision = entry(FireKind::Failed);
    decision.reason = Some("line\t\"quoted\"\\tail".to_owned());
    decision.trace = Some("trace\nnext".to_owned());
    state.record("doctor", &decision).expect("escaped record");
    let sidecar = dir.path().join(".nika/arm/doctor");
    let ledger = std::fs::read_to_string(sidecar.join(HISTORY)).expect("ledger");
    let line: serde_json::Value = serde_json::from_str(ledger.trim()).expect("valid ledger JSON");
    assert_eq!(line["payload"]["reason"], "line\t\"quoted\"\\tail");
    assert_eq!(line["payload"]["trace"], "trace\nnext");
    assert!(
        ledger.contains("\\u0009"),
        "controls use canonical unicode escapes: {ledger}"
    );
    let cache = std::fs::read_to_string(sidecar.join("last.json")).expect("cache");
    let last: serde_json::Value = serde_json::from_str(&cache).expect("valid cache JSON");
    assert_eq!(last["trace"], "trace\nnext");
}

/// A path error while opening the chain is not mistaken for an
/// absent chain.
#[test]
fn chain_head_refuses_a_history_path_that_is_a_directory() {
    let (dir, _state) = state("chain-dir");
    let sidecar = dir.path().join(".nika/arm/doctor");
    std::fs::create_dir_all(sidecar.join(HISTORY)).expect("history directory");
    let error = chain_head(&sidecar, &ts("2026-08-19T03:02:00Z"))
        .expect_err("a directory is not an absent chain");
    assert_ne!(error.kind(), io::ErrorKind::NotFound);
}

/// R5 · one tampered byte inside line 2 of 3: the next append CUTS
/// the tail from the first bad line (lines 2 AND 3), lands at seq 2
/// linked to line 1's hash, says how much it cut — and the valid
/// prefix's bytes survive VERBATIM (never rewritten).
#[test]
fn a_tampered_line_truncates_the_tail() {
    let (dir, state) = state("tamper");
    for kind in [FireKind::Fired, FireKind::Skipped, FireKind::Fired] {
        state.record("doctor", &entry(kind)).expect("record");
    }
    let ledger = dir.path().join(".nika/arm/doctor/history.ndjson");
    let original = std::fs::read_to_string(&ledger).expect("ledger");
    let first: serde_json::Value =
        serde_json::from_str(original.lines().next().expect("line 1")).expect("json");
    let first_hash = first["hash"].as_str().expect("hash").to_owned();
    std::fs::write(&ledger, original.replacen("\"seq\":2", "\"seq\":9", 1)).expect("tamper");
    let outcome = state
        .record("doctor", &entry(FireKind::Skipped))
        .expect("record");
    assert_eq!(outcome.repaired, 2, "the tail from the first bad line");
    assert_eq!(outcome.seq, 2, "the append continues the valid chain");
    let healed = std::fs::read_to_string(&ledger).expect("ledger");
    assert_eq!(healed.lines().count(), 2, "{healed}");
    assert_eq!(
        healed.lines().next(),
        original.lines().next(),
        "line 1's bytes are verbatim — valid lines are never rewritten"
    );
    let second: serde_json::Value =
        serde_json::from_str(healed.lines().nth(1).expect("line 2")).expect("json");
    assert_eq!(
        second["prev_hash"], first_hash,
        "linked to the last VALID line"
    );
    // … and the chain verifies clean from here.
    let outcome = state
        .record("doctor", &entry(FireKind::Skipped))
        .expect("record");
    assert_eq!(outcome.repaired, 0, "{healed}");
    assert_eq!(outcome.seq, 3);
}

/// R5 · two swapped lines break the seq continuity — verification
/// fails at the first swapped position and the append repairs from
/// there.
#[test]
fn swapped_lines_fail_the_continuity() {
    let (dir, state) = state("swapped");
    for _ in 0..3 {
        state
            .record("doctor", &entry(FireKind::Skipped))
            .expect("record");
    }
    let ledger = dir.path().join(".nika/arm/doctor/history.ndjson");
    let original = std::fs::read_to_string(&ledger).expect("ledger");
    let lines: Vec<&str> = original.lines().collect();
    assert_eq!(lines.len(), 3);
    let swapped = format!("{}\n{}\n{}\n", lines[0], lines[2], lines[1]);
    std::fs::write(&ledger, swapped).expect("swap");
    let outcome = state
        .record("doctor", &entry(FireKind::Skipped))
        .expect("record");
    assert_eq!(outcome.repaired, 2, "the swap fails at position 2");
    assert_eq!(outcome.seq, 2);
    let healed = std::fs::read_to_string(&ledger).expect("ledger");
    assert_eq!(healed.lines().count(), 2, "{healed}");
}

/// R5 · a W2-era journal (no `schema` on its first line) is kept
/// FOREVER under a rotated name (N4); the fresh chain opens with a
/// `rotated` line naming it; the tallies read BOTH files.
#[test]
fn a_legacy_journal_rotates_and_the_tallies_read_both() {
    let (dir, state) = state("rotate");
    let sidecar = dir.path().join(".nika/arm/doctor");
    std::fs::create_dir_all(&sidecar).expect("sidecar");
    let legacy = concat!(
        "{\"slot\":\"2026-08-18T03:00:00Z\",\"decided_at\":\"2026-08-18T03:02:00Z\",\"kind\":\"fired\",\"reason\":null,\"trace\":null,\"exit\":0,\"slots\":null}\n",
        "{\"slot\":\"2026-08-19T03:00:00Z\",\"decided_at\":\"2026-08-19T03:02:00Z\",\"kind\":\"skipped\",\"reason\":\"missed:1\",\"trace\":null,\"exit\":0,\"slots\":null}\n",
    );
    std::fs::write(sidecar.join("history.ndjson"), legacy).expect("legacy");
    let mut skipped = entry(FireKind::Skipped);
    skipped.reason = Some("overlap".to_owned());
    let outcome = state.record("doctor", &skipped).expect("record");
    assert_eq!(outcome.seq, 2, "the rotated line opened the chain at 1");
    assert_eq!(outcome.repaired, 0);
    // The legacy bytes moved VERBATIM, kept forever (N4).
    let rotated = std::fs::read_to_string(sidecar.join("history-w2.ndjson")).expect("rotated");
    assert_eq!(rotated, legacy, "the legacy journal is kept verbatim");
    let text = std::fs::read_to_string(sidecar.join("history.ndjson")).expect("ledger");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "{text}");
    let first: serde_json::Value = serde_json::from_str(lines[0]).expect("json");
    assert_eq!(first["kind"], "rotated", "{text}");
    assert_eq!(first["payload"]["from"], "history-w2.ndjson", "{text}");
    assert_eq!(first["payload"]["lines"], 2, "{text}");
    assert!(first["prev_hash"].is_null(), "the fresh chain's genesis");
    let second: serde_json::Value = serde_json::from_str(lines[1]).expect("json");
    assert_eq!(second["prev_hash"], first["hash"], "linked to the rotation");
    // The tallies scan BOTH files: legacy fired + skipped, new skipped.
    assert_eq!(state.tallies("doctor"), Some((2, 1)));
}

/// Rotation never overwrites an earlier archive: collisions walk
/// monotonically to the first free suffix.
#[test]
fn legacy_rotation_skips_every_existing_archive_name() {
    let (dir, state) = state("rotate-collisions");
    let sidecar = dir.path().join(".nika/arm/doctor");
    std::fs::create_dir_all(&sidecar).expect("sidecar");
    std::fs::write(sidecar.join("history-w2.ndjson"), "archive one\n").expect("archive 1");
    std::fs::write(sidecar.join("history-w2-2.ndjson"), "archive two\n").expect("archive 2");
    std::fs::write(
        sidecar.join(HISTORY),
        "{\"slot\":\"2026-08-18T03:00:00Z\",\"decided_at\":\"2026-08-18T03:02:00Z\",\"kind\":\"fired\"}\n",
    )
    .expect("legacy");
    state
        .record("doctor", &entry(FireKind::Fired))
        .expect("rotate");
    assert_eq!(
        std::fs::read_to_string(sidecar.join("history-w2-3.ndjson")).expect("third archive"),
        "{\"slot\":\"2026-08-18T03:00:00Z\",\"decided_at\":\"2026-08-18T03:02:00Z\",\"kind\":\"fired\"}\n"
    );
    assert_eq!(
        std::fs::read_to_string(sidecar.join("history-w2.ndjson")).expect("one"),
        "archive one\n"
    );
    assert_eq!(
        std::fs::read_to_string(sidecar.join("history-w2-2.ndjson")).expect("two"),
        "archive two\n"
    );
}

/// Lock creation propagates the filesystem's real error instead
/// of treating every failure as contention.
#[test]
fn lock_creation_in_a_missing_directory_preserves_not_found() {
    let (dir, _state) = state("lock-missing-dir");
    let missing = dir.path().join("does-not-exist");
    let error = try_named_lock(
        &missing,
        "lock",
        std::process::id(),
        &at("2026-08-19T03:02:00Z"),
    )
    .expect_err("missing parent");
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
}

/// An overflowing u32 never aliases a small live process id.
#[cfg(unix)]
#[test]
fn an_unrepresentable_pid_is_not_alive() {
    assert!(
        !owner_alive(0),
        "pid zero addresses a process group, not an owner"
    );
    assert!(!owner_alive(u32::MAX));
}
