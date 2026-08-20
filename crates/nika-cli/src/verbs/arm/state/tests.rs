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

/// (b) The kernel lease, not PID metadata, is the lock authority.
#[test]
fn a_kernel_lease_refuses_overlap_then_releases_on_drop() {
    let (_dir, state) = state("live");
    let now = at("2026-08-19T03:02:00Z");
    let me = std::process::id();
    let first = state.acquire_beat_lock("doctor", me, &now).expect("lock");
    assert_eq!(first.outcome, LockOutcome::Acquired);
    let lease = first.lease.expect("kernel lease");
    let second = state
        .acquire_beat_lock("doctor", me.wrapping_add(1), &now)
        .expect("re-lock");
    assert_eq!(second.outcome, LockOutcome::HeldAlive { pid: me });
    drop(lease);
    let third = state
        .acquire_beat_lock("doctor", me.wrapping_add(1), &now)
        .expect("kernel released on drop");
    assert_eq!(third.outcome, LockOutcome::Acquired);
}

/// PID reuse cannot wedge the beat: stale metadata has no kernel lease.
#[test]
fn stale_pid_metadata_never_outvotes_the_kernel_lease() {
    let (_dir, state) = state("stale");
    let now = at("2026-08-19T03:02:00Z");
    let dir = state.root().join("doctor");
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(
        dir.join("lock"),
        "{\"pid\":999999,\"started_at\":\"2026-08-19T03:00:00Z\"}\n",
    )
    .expect("remnant");
    let attempt = state
        .acquire_beat_lock("doctor", std::process::id(), &now)
        .expect("no kernel owner remains");
    assert_eq!(attempt.outcome, LockOutcome::Acquired);
    assert!(dir.join("lock").exists());
    let metadata = std::fs::read_to_string(dir.join("lock")).expect("diagnostic metadata");
    assert_eq!(lock_pid(&metadata), Some(std::process::id()));
}

/// Crash remnants may be corrupt or empty; absent a kernel lease, the next
/// holder repairs diagnostic metadata while taking the same stable inode.
#[test]
fn corrupt_and_empty_crash_remnants_are_recoverable() {
    let (_dir, state) = state("corrupt");
    let now = at("2026-08-19T03:02:00Z");
    let dir = state.root().join("doctor");
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join("lock"), "not json\n").expect("remnant");
    let corrupt = state
        .acquire_beat_lock("doctor", std::process::id(), &now)
        .expect("corrupt metadata is not a lease");
    assert_eq!(corrupt.outcome, LockOutcome::Acquired);
    drop(corrupt.lease);
    std::fs::write(dir.join("lock"), "").expect("empty remnant");
    let empty = state
        .acquire_beat_lock("doctor", std::process::id(), &now)
        .expect("empty metadata is not a lease");
    assert_eq!(empty.outcome, LockOutcome::Acquired);
}

#[cfg(unix)]
#[test]
fn lock_paths_refuse_symlinks_and_non_regular_nodes() {
    use std::os::unix::fs::symlink;

    let (_dir, state) = state("lock-node-kind");
    let now = at("2026-08-19T03:02:00Z");
    let sidecar = state.root().join("doctor");
    std::fs::create_dir_all(&sidecar).expect("sidecar");
    let target = sidecar.join("target");
    std::fs::write(&target, "untouched").expect("target");
    symlink(&target, sidecar.join("lock")).expect("symlink");
    assert!(
        state
            .acquire_beat_lock("doctor", std::process::id(), &now)
            .is_err(),
        "a lock symlink never selects the authority inode"
    );
    assert_eq!(
        std::fs::read_to_string(target).expect("target"),
        "untouched"
    );
    std::fs::remove_file(sidecar.join("lock")).expect("remove symlink");
    std::fs::create_dir(sidecar.join("lock")).expect("directory node");
    assert!(
        state
            .acquire_beat_lock("doctor", std::process::id(), &now)
            .is_err(),
        "a directory never becomes a lease"
    );
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

/// A crash can land the fsynced event before the atomic head rewrite. The
/// older anchor may advance only when its exact `(seq, hash)` is still the
/// verified prefix of the longer chain.
#[test]
fn a_lagging_head_advances_from_its_matching_verified_prefix() {
    let (dir, state) = state("head-lag");
    let first = entry(FireKind::Fired);
    state.record("doctor", &first).expect("anchor seq 1");
    let sidecar = dir.path().join(".nika/arm/doctor");
    let history = sidecar.join("history.ndjson");
    let text = std::fs::read_to_string(&history).expect("history");
    let (_, prev_hash, _) = scan_chain(&text);
    let mut second = entry(FireKind::Skipped);
    second.reason = Some("missed:1".to_owned());
    let (line, _) = ledger_line(
        2,
        second.decided_at,
        second.kind.as_str(),
        None,
        &decision_payload(&second),
        prev_hash.as_deref(),
    )
    .expect("valid crash-window append");
    append_line(&history, &line).expect("event fsync before anchor");

    state
        .record("doctor", &entry(FireKind::Fired))
        .expect("matching anchor advances, then append continues");
    assert_eq!(
        std::fs::read_to_string(history)
            .expect("history")
            .lines()
            .count(),
        3
    );
    let head: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(sidecar.join("head.json")).expect("head"))
            .expect("head json");
    assert_eq!(head["seq"], 3);
}

/// Only canonical W2 archive names enter replay and report aggregates.
#[test]
fn tallies_ignore_prefix_lookalike_archives() {
    let (dir, state) = state("tally-names");
    state
        .record("doctor", &entry(FireKind::Fired))
        .expect("record");
    let sidecar = dir.path().join(".nika/arm/doctor");
    for name in ["history-w2.txt", "history-w2-evil.ndjson"] {
        std::fs::write(sidecar.join(name), "{\"kind\":\"skipped\"}\n").expect("lookalike");
    }
    assert_eq!(state.tallies("doctor"), Some((0, 1)));
}

/// A ledger-looking but invalid schema is corruption, never W2 legacy that
/// aggregates and migration may launder into a green chain.
#[test]
fn an_invalid_ledger_schema_is_not_treated_as_legacy() {
    let (dir, state) = state("schema-confusion");
    let sidecar = dir.path().join(".nika/arm/doctor");
    std::fs::create_dir_all(&sidecar).expect("sidecar");
    std::fs::write(
        sidecar.join(HISTORY),
        "{\"schema\":\"nika/arm-event@2\",\"slot\":\"2026-08-19T03:00:00Z\",\"decided_at\":\"2026-08-19T03:02:00Z\",\"kind\":\"fired\",\"exit\":0}\n",
    )
    .expect("crafted ledger");
    assert!(state.tallies("doctor").is_none());
    let error = chain_head(&sidecar, &ts("2026-08-19T03:03:00Z"))
        .expect_err("invalid schema cannot rotate as legacy");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
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
    let orphans = state.unsettled("doctor").expect("valid journal");
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
        state.unsettled("doctor").expect("valid journal").is_empty(),
        "settled by the receipt"
    );
}

/// The orphan projection is fenced by the same durable head as replay. A
/// cleanly deleted receipt cannot turn a settled lifecycle back into an orphan.
#[test]
fn a_deleted_anchored_receipt_cannot_forge_an_orphan() {
    let (dir, state) = state("unsettled-rollback");
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
    let mut receipt = entry(FireKind::Fired);
    receipt.slot_id = Some(identity);
    receipt.fencing = Some(FencingToken::new(claimed.seq));
    state.record("doctor", &receipt).expect("receipt");
    assert!(state.unsettled("doctor").expect("healthy").is_empty());

    let ledger = dir.path().join(".nika/arm/doctor/history.ndjson");
    let text = std::fs::read_to_string(&ledger).expect("ledger");
    let first = text.lines().next().expect("claim");
    std::fs::write(&ledger, format!("{first}\n")).expect("delete receipt");
    assert!(
        state.unsettled("doctor").is_none(),
        "rollback refuses instead of forging an orphan"
    );
}

/// An invalid tail is evidence of tampering, not a receipt or a tally.
/// Every aggregate stops at the same verified prefix as replay.
#[test]
fn a_forged_tail_cannot_settle_or_inflate_the_verified_ledger() {
    let (dir, state) = state("aggregate-tamper");
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
    state.record_claim("doctor", &claim).expect("claim");
    let path = dir.path().join(".nika/arm/doctor/history.ndjson");
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .expect("ledger");
    writeln!(
        file,
        r#"{{"kind":"fired","slot_id":"{}","payload":{{"fencing":1}}}}"#,
        identity.as_str()
    )
    .expect("forged tail");
    assert_eq!(state.unsettled("doctor").expect("valid journal").len(), 1);
    assert_eq!(state.tallies("doctor"), Some((0, 0)));
}

/// `last.json` is a cache only. Parseable forged bytes never outrank the
/// verified ledger and are repaired from it on read.
#[test]
fn a_parseable_forged_last_cache_never_overrides_the_chain() {
    let (dir, state) = state("forged-cache");
    state
        .record("doctor", &entry(FireKind::Fired))
        .expect("record");
    let cache = dir.path().join(".nika/arm/doctor/last.json");
    std::fs::write(
        &cache,
        "{\"slot\":\"2026-08-20T03:00:00Z\",\"fired_at\":\"2026-08-20T03:02:00Z\",\"trace\":null,\"exit\":0,\"kind\":\"skipped\",\"gen\":null}\n",
    )
    .expect("forge cache");
    let last = state.last("doctor").expect("chain truth");
    assert_eq!(last.kind, FireKind::Fired);
    assert_eq!(last.slot, ts("2026-08-19T03:00:00Z"));
    assert!(
        std::fs::read_to_string(cache)
            .expect("repaired cache")
            .contains("\"kind\":\"fired\"")
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

    let unsettled = state.unsettled("doctor").expect("valid journal");
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
        state.unsettled("doctor").expect("valid journal").len(),
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

/// A tamper inside the anchored prefix is not a crash tail: append refuses
/// and preserves every byte for diagnosis.
#[test]
fn a_tampered_anchored_line_refuses_append() {
    let (dir, state) = state("tamper");
    for kind in [FireKind::Fired, FireKind::Skipped, FireKind::Fired] {
        state.record("doctor", &entry(kind)).expect("record");
    }
    let ledger = dir.path().join(".nika/arm/doctor/history.ndjson");
    let original = std::fs::read_to_string(&ledger).expect("ledger");
    let tampered = original.replacen("\"seq\":2", "\"seq\":9", 1);
    std::fs::write(&ledger, &tampered).expect("tamper");
    let error = state
        .record("doctor", &entry(FireKind::Skipped))
        .expect_err("an anchored tamper cannot be healed");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(std::fs::read_to_string(ledger).expect("evidence"), tampered);
}

/// Reordering anchored lines is a rollback/tamper refusal, never repair.
#[test]
fn swapped_anchored_lines_refuse_append() {
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
    std::fs::write(&ledger, &swapped).expect("swap");
    let error = state
        .record("doctor", &entry(FireKind::Skipped))
        .expect_err("an anchored reorder cannot be healed");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(std::fs::read_to_string(ledger).expect("evidence"), swapped);
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
    let archive_one = "{\"slot\":\"2026-08-16T03:00:00Z\",\"decided_at\":\"2026-08-16T03:02:00Z\",\"kind\":\"fired\"}\n";
    let archive_two = "{\"slot\":\"2026-08-17T03:00:00Z\",\"decided_at\":\"2026-08-17T03:02:00Z\",\"kind\":\"skipped\"}\n";
    std::fs::write(sidecar.join("history-w2.ndjson"), archive_one).expect("archive 1");
    std::fs::write(sidecar.join("history-w2-2.ndjson"), archive_two).expect("archive 2");
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
        archive_one
    );
    assert_eq!(
        std::fs::read_to_string(sidecar.join("history-w2-2.ndjson")).expect("two"),
        archive_two
    );
}

/// The W7 genesis commits the ordered W2 archive bundle. Any later byte,
/// order, membership, or deletion change fails every reader and writer closed.
#[test]
fn migrated_archives_are_bound_to_the_w7_genesis() {
    #[derive(Clone, Copy)]
    enum Mutation {
        Alter,
        Reorder,
        Delete,
        Add,
    }

    for (tag, mutation) in [
        ("alter", Mutation::Alter),
        ("reorder", Mutation::Reorder),
        ("delete", Mutation::Delete),
        ("add", Mutation::Add),
    ] {
        let (dir, state) = state(&format!("archive-commit-{tag}"));
        let sidecar = dir.path().join(".nika/arm/doctor");
        std::fs::create_dir_all(&sidecar).expect("sidecar");
        let legacy = concat!(
            "{\"slot\":\"2026-08-17T03:00:00Z\",\"decided_at\":\"2026-08-17T03:02:00Z\",\"kind\":\"fired\"}\n",
            "{\"slot\":\"2026-08-18T03:00:00Z\",\"decided_at\":\"2026-08-18T03:02:00Z\",\"kind\":\"skipped\"}\n",
        );
        std::fs::write(sidecar.join(HISTORY), legacy).expect("legacy");
        state
            .record("doctor", &entry(FireKind::Fired))
            .expect("migrate and append");
        let archive = sidecar.join("history-w2.ndjson");
        let live = std::fs::read_to_string(sidecar.join(HISTORY)).expect("live");
        let genesis: serde_json::Value =
            serde_json::from_str(live.lines().next().expect("genesis")).expect("json");
        assert_eq!(genesis["payload"]["archives"], 1);
        assert_eq!(
            genesis["payload"]["archives_sha256"].as_str().map(str::len),
            Some(64)
        );

        match mutation {
            Mutation::Alter => {
                std::fs::write(&archive, legacy.replacen("fired", "failed", 1))
                    .expect("alter archive");
            }
            Mutation::Reorder => {
                let lines: Vec<&str> = legacy.lines().collect();
                std::fs::write(&archive, format!("{}\n{}\n", lines[1], lines[0]))
                    .expect("reorder archive");
            }
            Mutation::Delete => std::fs::remove_file(&archive).expect("delete archive"),
            Mutation::Add => {
                std::fs::write(sidecar.join("history-w2-2.ndjson"), legacy)
                    .expect("inject archive");
            }
        }

        assert!(
            replay::replay(&sidecar).is_err(),
            "{tag}: replay refuses the changed bundle"
        );
        assert!(state.last("doctor").is_none(), "{tag}: no false PROUVÉ");
        assert!(state.tallies("doctor").is_none(), "{tag}: no false tally");
        let error = state
            .record("doctor", &entry(FireKind::Skipped))
            .expect_err("append cannot launder a changed archive bundle");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData, "{tag}");
    }
}

/// An archive is evidence only through a committed `rotated` genesis. An
/// archive-only sidecar and an empty-live bootstrap both fail every surface.
#[test]
fn archives_without_a_committed_live_genesis_fail_closed() {
    for empty_live in [false, true] {
        let (dir, state) = state(if empty_live {
            "archive-empty-live"
        } else {
            "archive-only"
        });
        let sidecar = dir.path().join(".nika/arm/doctor");
        std::fs::create_dir_all(&sidecar).expect("sidecar");
        let legacy = "{\"slot\":\"2026-08-17T03:00:00Z\",\"decided_at\":\"2026-08-17T03:02:00Z\",\"kind\":\"fired\"}\n";
        std::fs::write(sidecar.join("history-w2.ndjson"), legacy).expect("archive");
        if empty_live {
            std::fs::write(sidecar.join(HISTORY), "").expect("empty live");
            write_chain_anchor(&sidecar, 0, None).expect("bootstrap head");
        }

        assert!(replay::replay(&sidecar).is_err(), "replay refuses");
        assert!(state.last("doctor").is_none(), "last refuses");
        assert!(state.tallies("doctor").is_none(), "tallies refuse");
        assert_eq!(
            state
                .record("doctor", &entry(FireKind::Fired))
                .expect_err("append refuses ambiguity")
                .kind(),
            io::ErrorKind::InvalidData
        );
    }
}

/// Lock creation propagates the filesystem's real error instead
/// of treating every failure as contention.
#[test]
fn lock_creation_in_a_missing_directory_preserves_not_found() {
    let (dir, _state) = state("lock-missing-dir");
    let missing = dir.path().join("does-not-exist");
    let error = acquire_named_lock(
        &missing,
        "lock",
        std::process::id(),
        &at("2026-08-19T03:02:00Z"),
    )
    .err()
    .expect("missing parent");
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
}
