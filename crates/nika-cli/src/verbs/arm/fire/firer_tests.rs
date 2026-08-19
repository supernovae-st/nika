// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The W5-bis firer law, pinned end to end: the lock outlives the shot
//! (the order law), the claim is appended + fsync'd BEFORE the run and
//! the receipt settles it by fencing, the queue re-decides after ANY
//! wait, an interrupted wait skips `serve-stop`, and a healed ledger
//! says so on the ONE line. Every test drives [`fire_beat`] with the
//! seams injected — the run seam is ALWAYS a stub here (the real
//! in-process run chdirs, and parallel tests race on the process CWD;
//! the binary tests under `tests/` own that ground).
// The workspace bans std::process::Command (production spawns ride the
// kernel ShellExecutor seam). These tests' OTHER firer is a real live
// `sleep` child — the lock-liveness law needs a pid that answers
// signal 0, and no kernel seam lends « a borrowed live pid ».
#![allow(clippy::disallowed_types)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

use jiff::Timestamp;

use super::*;

/// A one-beat registry (validated green) — the same shape the decide
/// suite rides, with the overlap policy as the variable.
fn registry_with(body: &str) -> ArmRegistry {
    let text = format!(
        "nika: v1\narm:\n  - workflow: workflows/doctor.nika.yaml\n    cadence: \"TZ=UTC 0 3 * * *\"\n    plafond: 0.25\n{body}"
    );
    let registry = nika_cadence::parse_registry(&text).expect("parse");
    assert!(
        nika_cadence::validate(&registry).next().is_none(),
        "the fixture must be lawful"
    );
    registry
}

/// `manqué: sauter`, the safe default overlap.
const SAUTER: &str = "    manqué: sauter\n";
/// The bounded in-memory queue.
const FILE: &str = "    manqué: sauter\n    chevauchement: file\n";

fn at(text: &str) -> Zoned {
    text.parse::<Timestamp>()
        .expect("ts")
        .to_zoned(jiff::tz::TimeZone::UTC)
}

fn ts(text: &str) -> Timestamp {
    text.parse::<Timestamp>().expect("ts")
}

/// A tempdir project root — the impure firer's ground.
fn project(tag: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(&format!("nika-arm-firer-{tag}-"))
        .tempdir()
        .expect("tmp dir")
}

/// A live `sleep` child — the OTHER firer (its pid answers signal 0,
/// so its lock reads `HeldAlive`). Killed + reaped on drop; [`die`](Self::die)
/// for the mid-test completion (a zombie still answers the probe).
struct LiveChild(std::process::Child);

impl LiveChild {
    fn spawn() -> Self {
        Self(
            std::process::Command::new("sleep")
                .arg("30")
                .spawn()
                .expect("a sleep child"),
        )
    }

    fn pid(&self) -> u32 {
        self.0.id()
    }

    /// Kill + reap NOW — a zombie still answers signal 0.
    fn die(mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Drop for LiveChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The firer's context with every seam injected: the pid is the test
/// process's own, the wait is scripted by the test, the run is a stub.
fn ctx(root: &Path, registry: ArmRegistry, now: &str, wait: WaitSeam, run: RunSeam) -> FireCtx {
    FireCtx {
        project_root: root.to_path_buf(),
        registry,
        index: 0,
        label: "doctor".to_owned(),
        now: at(now),
        state: ArmState::at_project(root),
        pid: std::process::id(),
        wait,
        run,
    }
}

/// A run stub that counts its calls and exits clean.
fn run_counter() -> (Rc<Cell<u32>>, RunSeam) {
    let count = Rc::new(Cell::new(0u32));
    let seen = Rc::clone(&count);
    let seam: RunSeam = Rc::new(move |_: &RunShot| {
        seen.set(seen.get() + 1);
        RunUpshot {
            code: exit::OK,
            trace: None,
        }
    });
    (count, seam)
}

/// The wait that elapses at once (no signal, no scripted clock).
fn instant_wait() -> WaitSeam {
    Box::new(|_| Wait::Elapsed)
}

/// One beat's ledger text (`""` when absent).
fn history(root: &Path, label: &str) -> String {
    std::fs::read_to_string(root.join(".nika/arm").join(label).join("history.ndjson"))
        .unwrap_or_default()
}

/// R1 · deux processus, un run — a LIVE other firer holds the lock:
/// the due tick skips `overlap` WITH the slot (D8's consistency), the
/// run seam is never called, and the ledger gains exactly ONE chained
/// line (kind skipped · reason overlap · the slot's identity).
#[test]
fn two_firers_one_run() {
    let dir = project("two-firers");
    let sidecar = ArmState::at_project(dir.path());
    let holder = LiveChild::spawn();
    let now = at("2026-08-19T03:02:00Z");
    // The other firer's lock — its pid lives, the lock is its run's.
    assert_eq!(
        sidecar
            .try_lock("doctor", holder.pid(), &now)
            .expect("lock"),
        LockOutcome::Acquired
    );
    let (runs, run) = run_counter();
    let ctx = ctx(
        dir.path(),
        registry_with(SAUTER),
        "2026-08-19T03:02:00Z",
        instant_wait(),
        run,
    );
    let verdict = fire_beat(&ctx);
    assert_eq!(verdict.code, exit::OK, "{}", verdict.line);
    assert!(
        verdict.line.starts_with(&format!(
            "skipped doctor · overlap · pid {} tient le créneau",
            holder.pid()
        )),
        "{}",
        verdict.line
    );
    assert!(
        verdict.line.ends_with("· slot 2026-08-19T03:00:00Z"),
        "the slot rides the line (D8): {}",
        verdict.line
    );
    assert_eq!(runs.get(), 0, "the run seam NEVER fired");
    let text = history(dir.path(), "doctor");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1, "exactly one chained line: {text}");
    let doc: serde_json::Value = serde_json::from_str(lines[0]).expect("json");
    assert_eq!(doc["schema"], "nika/arm-event@1", "{text}");
    assert_eq!(doc["kind"], "skipped", "{text}");
    assert_eq!(doc["payload"]["reason"], "overlap", "{text}");
    assert_eq!(doc["seq"], 1, "the genesis line: {text}");
    assert!(doc["prev_hash"].is_null(), "the genesis line: {text}");
    assert!(
        doc["slot_id"].is_string(),
        "a slot-bearing skip carries the slot identity: {text}"
    );
    // The holder's lock is untouched — law ⑥ sauter.
    assert!(dir.path().join(".nika/arm/doctor/lock").exists());
}

/// R2 · the order law: under the lock, the claim lands BEFORE the run
/// (the stub sees it as the chain's last line, the lock file ours),
/// the receipt lands AFTER — seq + 1, fencing the claim's seq, the
/// same slot identity — and the release comes last of all.
#[test]
fn the_claim_precedes_the_run_and_the_receipt_settles_it() {
    let dir = project("ordering");
    let root = dir.path().to_path_buf();
    let during = move |shot: &RunShot| {
        // DURING the run: the chain's last line is the claim …
        let text = std::fs::read_to_string(root.join(".nika/arm/doctor/history.ndjson"))
            .expect("the claim lands BEFORE the run");
        let last_line = text.lines().last().expect("one line");
        let doc: serde_json::Value = serde_json::from_str(last_line).expect("json");
        assert_eq!(doc["kind"], "claimed", "{text}");
        assert_eq!(
            doc["payload"]["fencing"], doc["seq"],
            "the claim fences its own seq: {text}"
        );
        assert_eq!(doc["payload"]["attempt"], 1, "one attempt — v0: {text}");
        assert_eq!(
            doc["payload"]["deadline"], "2026-08-20T03:00:00Z",
            "the deadline is the beat's next theoretical slot: {text}"
        );
        // … and the beat lock is OURS, carrying the context's pid.
        let lock = std::fs::read_to_string(root.join(".nika/arm/doctor/lock"))
            .expect("the lock outlives the shot");
        assert!(
            lock.contains(&format!("\"pid\":{}", std::process::id())),
            "the lock carries OUR pid: {lock}"
        );
        assert_eq!(shot.workflow, "workflows/doctor.nika.yaml");
        RunUpshot {
            code: exit::OK,
            trace: None,
        }
    };
    let ctx = ctx(
        dir.path(),
        registry_with(SAUTER),
        "2026-08-19T03:02:00Z",
        instant_wait(),
        Rc::new(during),
    );
    let verdict = fire_beat(&ctx);
    assert_eq!(verdict.code, exit::OK, "{}", verdict.line);
    assert!(
        verdict
            .line
            .starts_with("fired doctor · slot 2026-08-19T03:00:00Z · exit 0"),
        "{}",
        verdict.line
    );
    // AFTER: the receipt settles the claim …
    let text = history(dir.path(), "doctor");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "the claim, then the receipt: {text}");
    let claim: serde_json::Value = serde_json::from_str(lines[0]).expect("claim json");
    let receipt: serde_json::Value = serde_json::from_str(lines[1]).expect("receipt json");
    assert_eq!(claim["kind"], "claimed");
    assert_eq!(receipt["kind"], "fired");
    assert_eq!(
        receipt["seq"].as_u64(),
        claim["seq"].as_u64().map(|seq| seq + 1),
        "the receipt follows the claim"
    );
    assert_eq!(
        receipt["payload"]["fencing"], claim["seq"],
        "the receipt fences the claim's seq"
    );
    assert_eq!(
        receipt["slot_id"], claim["slot_id"],
        "the same slot identity"
    );
    // … the release came last: no lock outlives the verdict …
    assert!(!dir.path().join(".nika/arm/doctor/lock").exists());
    assert!(!dir.path().join(".nika/arm/doctor/ledger.lock").exists());
    // … and the projections moved: last.json fired, the watermark = the
    // decided instant.
    let last =
        std::fs::read_to_string(dir.path().join(".nika/arm/doctor/last.json")).expect("last.json");
    assert!(last.contains("\"kind\":\"fired\""), "{last}");
    let watermark =
        std::fs::read_to_string(dir.path().join(".nika/arm/doctor/watermark")).expect("watermark");
    assert_eq!(watermark, "2026-08-19T03:02:00Z\n");
}

/// R3 · a record that cannot land is said LOUDLY: the failure line
/// REPLACES the decision's (exit ENV), nothing ran, and the lock is
/// STILL released — the read-only ledger refuses the claim's append.
#[cfg(unix)]
#[test]
fn a_refused_record_fails_loudly_and_still_releases() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = project("readonly");
    let sidecar = dir.path().join(".nika/arm/doctor");
    std::fs::create_dir_all(&sidecar).expect("sidecar");
    let ledger = sidecar.join("history.ndjson");
    std::fs::write(&ledger, "").expect("the ledger shell");
    std::fs::set_permissions(&ledger, std::fs::Permissions::from_mode(0o444))
        .expect("read-only ledger");
    let (runs, run) = run_counter();
    let ctx = ctx(
        dir.path(),
        registry_with(SAUTER),
        "2026-08-19T03:02:00Z",
        instant_wait(),
        run,
    );
    let verdict = fire_beat(&ctx);
    assert_eq!(verdict.code, exit::ENV, "{}", verdict.line);
    assert!(
        verdict
            .line
            .starts_with("failed doctor · the record refused:"),
        "the failure line replaces the decision's: {}",
        verdict.line
    );
    assert_eq!(runs.get(), 0, "the claim never landed — nothing ran");
    assert!(
        !sidecar.join("lock").exists(),
        "the release happens on the failure path too"
    );
    assert!(!sidecar.join("watermark").exists());
}

/// R6 · `chevauchement: file` RE-DECIDES after the wait: the holder's
/// fire completed while we queued (its receipt + last.json landed), so
/// the freshly-taken lock must NOT fire the pre-wait slot — the
/// re-decision reads `already` and nothing runs.
#[test]
fn the_queue_redecides_after_the_wait() {
    let dir = project("redecide");
    let sidecar = ArmState::at_project(dir.path());
    let holder = LiveChild::spawn();
    let now = at("2026-08-19T03:02:00Z");
    assert_eq!(
        sidecar
            .try_lock("doctor", holder.pid(), &now)
            .expect("lock"),
        LockOutcome::Acquired
    );
    // The wait's first beat: the holder's fire COMPLETES (its receipt +
    // last.json land, the chain grows) and its process dies.
    let root = dir.path().to_path_buf();
    let holder = RefCell::new(Some(holder));
    let wait: WaitSeam = Box::new(move |_| {
        if let Some(child) = holder.borrow_mut().take() {
            ArmState::at_project(&root)
                .record(
                    "doctor",
                    &HistoryEntry {
                        slot: Some(ts("2026-08-19T03:00:00Z")),
                        decided_at: ts("2026-08-19T03:02:10Z"),
                        kind: FireKind::Fired,
                        reason: None,
                        trace: None,
                        exit: Some(0),
                        slots: None,
                        slot_id: Some(slot_id(
                            "workflows/doctor.nika.yaml",
                            "TZ=UTC 0 3 * * *",
                            &at("2026-08-19T03:00:00Z"),
                        )),
                        fencing: None,
                    },
                )
                .expect("the holder's receipt");
            child.die();
        }
        Wait::Elapsed
    });
    let (runs, run) = run_counter();
    let ctx = ctx(
        dir.path(),
        registry_with(FILE),
        "2026-08-19T03:02:00Z",
        wait,
        run,
    );
    let verdict = fire_beat(&ctx);
    assert_eq!(verdict.code, exit::OK, "{}", verdict.line);
    assert!(
        verdict
            .line
            .starts_with("skipped doctor · already · slot 2026-08-19T03:00:00Z"),
        "the re-decision sees the holder's completed slot: {}",
        verdict.line
    );
    assert_eq!(runs.get(), 0, "the pre-wait slot must NOT fire twice");
    // The chain carries ONLY the holder's receipt — an `already`
    // re-decision journals nothing.
    let text = history(dir.path(), "doctor");
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(text.contains("\"kind\":\"fired\""), "{text}");
    // The lock we briefly took is released.
    assert!(!dir.path().join(".nika/arm/doctor/lock").exists());
}

/// R7 (the unit half) · a wait broken by a signal: `serve-stop`,
/// journaled WITH the slot, no run, and the verdict comes back at
/// once — the budget is not burned waiting.
#[test]
fn an_interrupted_wait_skips_serve_stop() {
    let dir = project("interrupted");
    let sidecar = ArmState::at_project(dir.path());
    let holder = LiveChild::spawn();
    let now = at("2026-08-19T03:02:00Z");
    assert_eq!(
        sidecar
            .try_lock("doctor", holder.pid(), &now)
            .expect("lock"),
        LockOutcome::Acquired
    );
    let (runs, run) = run_counter();
    let ctx = ctx(
        dir.path(),
        registry_with(FILE),
        "2026-08-19T03:02:00Z",
        Box::new(|_| Wait::Interrupted),
        run,
    );
    let verdict = fire_beat(&ctx);
    assert_eq!(verdict.code, exit::OK, "{}", verdict.line);
    assert_eq!(
        verdict.line,
        "skipped doctor · serve-stop · slot 2026-08-19T03:00:00Z"
    );
    assert_eq!(runs.get(), 0, "a stopped firer never runs");
    let text = history(dir.path(), "doctor");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1, "{text}");
    let doc: serde_json::Value = serde_json::from_str(lines[0]).expect("json");
    assert_eq!(doc["kind"], "skipped");
    assert_eq!(doc["payload"]["reason"], "serve-stop");
    assert!(doc["slot_id"].is_string(), "{text}");
    // The abandoned tick consumed its slot: last.json moved.
    let last =
        std::fs::read_to_string(dir.path().join(".nika/arm/doctor/last.json")).expect("last.json");
    assert!(last.contains("\"kind\":\"skipped\""), "{last}");
    // The holder's lock is not ours to touch.
    assert!(dir.path().join(".nika/arm/doctor/lock").exists());
}

/// A healed ledger says so ON the decision line — ` · ledger
/// réparé (-n)` names the truncated tail (D8 stays ONE line).
#[test]
fn a_repaired_ledger_tail_rides_the_decision_line() {
    let dir = project("repair");
    let sidecar = ArmState::at_project(dir.path());
    // Three clean decisions against a past slot, then one byte of
    // tamper inside line 2 (its seq no longer continues the chain).
    let seed = HistoryEntry {
        slot: Some(ts("2026-08-18T03:00:00Z")),
        decided_at: ts("2026-08-18T03:01:00Z"),
        kind: FireKind::Skipped,
        reason: Some("overlap".to_owned()),
        trace: None,
        exit: Some(0),
        slots: None,
        slot_id: None,
        fencing: None,
    };
    for _ in 0..3 {
        sidecar.record("doctor", &seed).expect("record");
    }
    let ledger = dir.path().join(".nika/arm/doctor/history.ndjson");
    let text = std::fs::read_to_string(&ledger).expect("ledger");
    assert_eq!(text.lines().count(), 3, "{text}");
    std::fs::write(&ledger, text.replacen("\"seq\":2", "\"seq\":9", 1)).expect("tamper");
    let (runs, run) = run_counter();
    let ctx = ctx(
        dir.path(),
        registry_with(SAUTER),
        "2026-08-19T10:00:00Z",
        instant_wait(),
        run,
    );
    let verdict = fire_beat(&ctx);
    assert_eq!(verdict.code, exit::OK, "{}", verdict.line);
    assert!(
        verdict
            .line
            .starts_with("skipped doctor · missed:1 · slot 2026-08-19T03:00:00Z"),
        "{}",
        verdict.line
    );
    assert!(
        verdict.line.ends_with(" · ledger réparé (-2)"),
        "the repair rides the one line: {}",
        verdict.line
    );
    assert_eq!(runs.get(), 0);
    // The chain healed: the tampered tail is gone, the append continued
    // at seq 2 linked to line 1's hash.
    let healed = std::fs::read_to_string(&ledger).expect("ledger");
    assert_eq!(healed.lines().count(), 2, "{healed}");
    let second: serde_json::Value =
        serde_json::from_str(healed.lines().nth(1).expect("line 2")).expect("json");
    assert_eq!(second["seq"], 2, "{healed}");
    assert_eq!(second["kind"], "skipped", "{healed}");
    assert_eq!(second["payload"]["reason"], "missed:1", "{healed}");
}
