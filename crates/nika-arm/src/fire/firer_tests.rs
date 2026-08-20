// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! End-to-end firer law: lock through receipt, re-decision after waits,
//! deterministic run seams, and live-process overlap leases. Binary tests own
//! the real in-process run because parallel library tests cannot share CWD.
#![allow(clippy::disallowed_types)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

use jiff::Timestamp;

use super::super::state::LockLease;
use super::*;

type RegistryFixture = (String, ArmRegistry);

/// A one-beat registry (validated green) — the same shape the decide
/// suite rides, with the overlap policy as the variable.
fn registry_with(body: &str) -> RegistryFixture {
    let source = format!(
        "nika: proj\narm:\n  - workflow: workflows/doctor.nika.yaml\n    cadence: \"TZ=UTC 0 3 * * *\"\n    plafond: 0.25\n{body}"
    );
    let registry = nika_cadence::parse_registry(&source).expect("parse");
    assert!(
        nika_cadence::validate(&registry).next().is_none(),
        "the fixture must be lawful"
    );
    (source, registry)
}

fn minutely_registry(body: &str) -> RegistryFixture {
    let source = format!(
        "nika: proj\narm:\n  - workflow: workflows/doctor.nika.yaml\n    cadence: \"TZ=UTC * * * * *\"\n    plafond: 0.25\n{body}"
    );
    let registry = nika_cadence::parse_registry(&source).expect("parse");
    assert!(nika_cadence::validate(&registry).next().is_none());
    (source, registry)
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
    let dir = tempfile::Builder::new()
        .prefix(&format!("nika-arm-firer-{tag}-"))
        .tempdir()
        .expect("tmp dir");
    std::fs::create_dir_all(dir.path().join("workflows")).expect("workflows dir");
    std::fs::write(
        dir.path().join("workflows/doctor.nika.yaml"),
        "schema: nika/workflow@0.12\ntasks: {}\n",
    )
    .expect("workflow source");
    dir
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
fn ctx(root: &Path, fixture: RegistryFixture, now: &str, wait: WaitSeam, run: RunSeam) -> FireCtx {
    std::fs::write(root.join("nika.yaml"), &fixture.0).expect("project registry");
    FireCtx::new(
        root.to_path_buf(),
        fixture.1,
        0,
        at(now),
        std::process::id(),
        run,
    )
    .expect("valid beat index")
    .with_wait(wait)
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
    let attempt = sidecar
        .acquire_beat_lock("doctor", holder.pid(), &now)
        .expect("lock");
    assert_eq!(attempt.outcome, LockOutcome::Acquired);
    let _lease: LockLease = attempt.lease.expect("lease");
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
        assert_eq!(shot.root(), root.as_path());
        assert_eq!(shot.workflow(), "workflows/doctor.nika.yaml");
        assert_eq!(shot.ceiling().to_bits(), 0.25f64.to_bits());
        RunUpshot::new(exit::OK, None)
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
    // … the kernel leases ended before the verdict. Their stable diagnostic
    // paths remain and a new holder can acquire immediately.
    assert!(dir.path().join(".nika/arm/doctor/lock").exists());
    assert!(dir.path().join(".nika/arm/doctor/ledger.lock").exists());
    let probe = ArmState::at_project(dir.path())
        .acquire_beat_lock("doctor", std::process::id(), &at("2026-08-19T03:03:00Z"))
        .expect("released kernel lease");
    assert_eq!(probe.outcome, LockOutcome::Acquired);
    // … and the projections moved: last.json fired, the watermark = the
    // decided instant.
    let last =
        std::fs::read_to_string(dir.path().join(".nika/arm/doctor/last.json")).expect("last.json");
    assert!(last.contains("\"kind\":\"fired\""), "{last}");
    let watermark =
        std::fs::read_to_string(dir.path().join(".nika/arm/doctor/watermark")).expect("watermark");
    assert_eq!(watermark, "2026-08-19T03:02:00Z\n");
}

#[test]
fn source_edit_after_claim_cannot_change_the_pinned_run_bytes() {
    let dir = project("pin-edit");
    let source = dir.path().join("workflows/doctor.nika.yaml");
    let original = std::fs::read(&source).expect("source A");
    let registry = registry_with(SAUTER);
    let expected = ArmGeneration::compute(registry.1.beats().next().expect("beat"), &original);
    let logical_path = Rc::new(RefCell::new(None::<String>));
    let seen_path = Rc::clone(&logical_path);
    let seam: RunSeam = Rc::new(move |shot| {
        std::fs::write(
            &source,
            "schema: nika/workflow@0.12\ntasks: {b: {exec: echo B}}\n",
        )
        .expect("replace declared source with B");
        assert_eq!(shot.source().as_bytes(), original.as_slice());
        assert_eq!(shot.generation(), &expected);
        *seen_path.borrow_mut() = Some(shot.workflow().to_owned());
        RunUpshot::new(exit::OK, None)
    });
    let verdict = fire_beat(&ctx(
        dir.path(),
        registry,
        "2026-08-19T03:02:00Z",
        instant_wait(),
        seam,
    ));
    assert_eq!(verdict.code, exit::OK, "{}", verdict.line);
    assert_eq!(
        logical_path.borrow().as_deref(),
        Some("workflows/doctor.nika.yaml"),
        "the captured bytes retain their declared resolution base"
    );
}

#[cfg(unix)]
#[test]
fn source_symlink_swap_after_claim_cannot_change_the_pinned_run_bytes() {
    use std::os::unix::fs::symlink;

    let dir = project("pin-symlink-swap");
    let source = dir.path().join("workflows/doctor.nika.yaml");
    let replacement = dir.path().join("workflows/replacement.nika.yaml");
    let original = std::fs::read(&source).expect("source A");
    std::fs::write(
        &replacement,
        "schema: nika/workflow@0.12\ntasks: {b: {exec: echo B}}\n",
    )
    .expect("source B");
    let registry = registry_with(SAUTER);
    let expected = ArmGeneration::compute(registry.1.beats().next().expect("beat"), &original);
    let seam: RunSeam = Rc::new(move |shot| {
        std::fs::remove_file(&source).expect("remove A");
        symlink(&replacement, &source).expect("swap to symlink B");
        assert_eq!(shot.source().as_bytes(), original.as_slice());
        assert_eq!(shot.generation(), &expected);
        RunUpshot::new(exit::OK, None)
    });
    let verdict = fire_beat(&ctx(
        dir.path(),
        registry,
        "2026-08-19T03:02:00Z",
        instant_wait(),
        seam,
    ));
    assert_eq!(verdict.code, exit::OK, "{}", verdict.line);
}

#[cfg(unix)]
#[test]
fn a_symlink_workflow_is_refused_before_claim_or_run() {
    use std::os::unix::fs::symlink;

    let dir = project("pin-initial-symlink");
    let source = dir.path().join("workflows/doctor.nika.yaml");
    let replacement = dir.path().join("workflows/replacement.nika.yaml");
    std::fs::write(&replacement, "schema: nika/workflow@0.12\ntasks: {}\n").expect("target");
    std::fs::remove_file(&source).expect("remove source");
    symlink(&replacement, &source).expect("source symlink");
    let (runs, run) = run_counter();
    let verdict = fire_beat(&ctx(
        dir.path(),
        registry_with(SAUTER),
        "2026-08-19T03:02:00Z",
        instant_wait(),
        run,
    ));
    assert_eq!(verdict.code, exit::ENV, "{}", verdict.line);
    assert_eq!(runs.get(), 0);
    assert!(
        history(dir.path(), "doctor").is_empty(),
        "no claim was recorded"
    );
}

#[cfg(unix)]
#[test]
fn a_symlinked_parent_is_refused_before_claim_or_run() {
    use std::os::unix::fs::symlink;

    let dir = project("pin-symlinked-parent");
    let outside = tempfile::tempdir().expect("outside");
    std::fs::write(
        outside.path().join("doctor.nika.yaml"),
        "schema: nika/workflow@0.12\ntasks: {}\n",
    )
    .expect("outside workflow");
    std::fs::remove_dir_all(dir.path().join("workflows")).expect("remove workflows");
    symlink(outside.path(), dir.path().join("workflows")).expect("symlinked parent");
    let (runs, run) = run_counter();
    let verdict = fire_beat(&ctx(
        dir.path(),
        registry_with(SAUTER),
        "2026-08-19T03:02:00Z",
        instant_wait(),
        run,
    ));
    assert_eq!(verdict.code, exit::ENV, "{}", verdict.line);
    assert_eq!(runs.get(), 0);
    assert!(history(dir.path(), "doctor").is_empty());
}

#[cfg(unix)]
#[test]
fn a_symlinked_project_root_is_refused_before_claim_or_run() {
    use std::os::unix::fs::symlink;

    let project = project("pin-symlinked-root");
    let links = tempfile::tempdir().expect("links");
    let linked = links.path().join("project");
    symlink(project.path(), &linked).expect("project symlink");
    let (runs, run) = run_counter();
    let fixture = registry_with(SAUTER);
    std::fs::write(project.path().join("nika.yaml"), &fixture.0).expect("registry");
    let error = FireCtx::new(
        linked,
        fixture.1,
        0,
        at("2026-08-19T03:02:00Z"),
        std::process::id(),
        run,
    )
    .err()
    .expect("symlink root refused");
    assert!(error.to_string().contains("project custody refused"));
    assert_eq!(runs.get(), 0);
    assert!(history(project.path(), "doctor").is_empty());
}

#[cfg(unix)]
#[test]
fn a_dot_suffixed_symlinked_project_root_is_refused() {
    use std::os::unix::fs::symlink;

    let project = project("pin-dot-symlinked-root");
    let links = tempfile::tempdir().expect("links");
    let linked = links.path().join("project");
    symlink(project.path(), &linked).expect("project symlink");
    let fixture = registry_with(SAUTER);
    std::fs::write(project.path().join("nika.yaml"), &fixture.0).expect("registry");
    let error = FireCtx::new(
        linked.join("."),
        fixture.1,
        0,
        at("2026-08-19T03:02:00Z"),
        std::process::id(),
        Rc::new(|_| RunUpshot::new(exit::OK, None)),
    )
    .err()
    .expect("dot-suffixed symlink root refused");
    assert!(error.to_string().contains("project custody refused"));
}

#[test]
fn a_registry_from_another_project_is_refused_at_construction() {
    let project = project("registry-provenance");
    let held = registry_with(SAUTER);
    std::fs::write(project.path().join("nika.yaml"), held.0).expect("held registry");
    let foreign = registry_with(FILE);
    let error = FireCtx::new(
        project.path().to_path_buf(),
        foreign.1,
        0,
        at("2026-08-19T03:02:00Z"),
        std::process::id(),
        Rc::new(|_| RunUpshot::new(exit::OK, None)),
    )
    .err()
    .expect("foreign registry refused");
    assert!(
        error
            .to_string()
            .contains("supplied registry does not belong to the held project")
    );
}

#[cfg(unix)]
#[test]
fn project_path_replacement_cannot_split_workflow_and_state_custody() {
    let dir = project("pin-root-replacement");
    let original = std::fs::read_to_string(dir.path().join("workflows/doctor.nika.yaml"))
        .expect("original workflow");
    let seen = Rc::new(Cell::new(false));
    let saw_original = Rc::clone(&seen);
    let run: RunSeam = Rc::new(move |shot| {
        assert_eq!(shot.source(), original);
        saw_original.set(true);
        RunUpshot::new(exit::OK, None)
    });
    let firing = ctx(
        dir.path(),
        registry_with(SAUTER),
        "2026-08-19T03:02:00Z",
        instant_wait(),
        run,
    );
    let moved = dir.path().with_extension("held-project");
    std::fs::rename(dir.path(), &moved).expect("move visible root");
    std::fs::create_dir_all(dir.path().join("workflows")).expect("replacement root");
    std::fs::write(
        dir.path().join("workflows/doctor.nika.yaml"),
        "schema: nika/workflow@0.12\ntasks: {evil: {exec: echo evil}}\n",
    )
    .expect("replacement workflow");

    let verdict = fire_beat(&firing);
    assert_eq!(verdict.code, exit::OK, "{}", verdict.line);
    assert!(seen.get());
    assert!(!history(dir.path(), "doctor").contains("\"fired\""));
    assert!(history(&moved, "doctor").contains("\"fired\""));

    std::fs::remove_dir_all(dir.path()).expect("remove replacement");
    std::fs::rename(&moved, dir.path()).expect("restore temp project");
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
    let probe = ArmState::at_project(dir.path())
        .acquire_beat_lock("doctor", std::process::id(), &at("2026-08-19T03:03:00Z"))
        .expect("the failure path released its kernel lease");
    assert_eq!(probe.outcome, LockOutcome::Acquired);
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
    let now = at("2026-08-19T03:00:59.500Z");
    let attempt = sidecar
        .acquire_beat_lock("doctor", holder.pid(), &now)
        .expect("lock");
    assert_eq!(attempt.outcome, LockOutcome::Acquired);
    let lease = attempt.lease.expect("lease");
    // The wait's first beat: the holder's fire COMPLETES (its receipt +
    // last.json land, the chain grows) and its process dies.
    let holder = RefCell::new(Some((holder, lease)));
    let waits = Rc::new(RefCell::new(Vec::new()));
    let seen_waits = Rc::clone(&waits);
    let wait: WaitSeam = Box::new(move |span| {
        seen_waits.borrow_mut().push(span);
        if let Some((child, lease)) = holder.borrow_mut().take() {
            let claim = Claim::new(
                SlotId::derive(
                    "workflows/doctor.nika.yaml",
                    "TZ=UTC * * * * *",
                    &at("2026-08-19T03:00:00Z"),
                ),
                ts("2026-08-19T03:01:00Z"),
                ts("2026-08-19T03:00:59.500Z"),
            );
            let claimed =
                ArmState::record_claim_with_lease(&lease, &claim).expect("the holder's claim");
            let receipt = Receipt::for_claim(
                &claim,
                FencingToken::new(claimed.seq),
                ts("2026-08-19T03:00:00Z"),
                ts("2026-08-19T03:00:59.600Z"),
                None,
                0,
                None,
            );
            ArmState::record_receipt_with_lease(&lease, &receipt).expect("the holder's receipt");
            drop(lease);
            child.die();
        }
        Wait::Elapsed
    });
    let (runs, run) = run_counter();
    let ctx = ctx(
        dir.path(),
        minutely_registry(FILE),
        "2026-08-19T03:00:59.500Z",
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
    assert_eq!(
        waits.borrow().as_slice(),
        &[SignedDuration::from_millis(500)],
        "the wait is exactly bounded by the next slot"
    );
    // The chain carries the holder's claim + receipt — an `already`
    // re-decision journals nothing.
    let text = history(dir.path(), "doctor");
    assert_eq!(text.lines().count(), 2, "{text}");
    assert!(text.contains("\"kind\":\"fired\""), "{text}");
    // The stable diagnostic path remains; only the kernel lease is released.
    assert!(dir.path().join(".nika/arm/doctor/lock").exists());
}

#[test]
fn the_queue_times_out_after_its_exact_remaining_budget() {
    let dir = project("queue-budget");
    let sidecar = ArmState::at_project(dir.path());
    let holder = LiveChild::spawn();
    let now = at("2026-08-19T03:00:59.500Z");
    let attempt = sidecar
        .acquire_beat_lock("doctor", holder.pid(), &now)
        .expect("lock");
    let _lease = attempt.lease.expect("holder lease");
    let waits = Rc::new(RefCell::new(Vec::new()));
    let seen_waits = Rc::clone(&waits);
    let (runs, run) = run_counter();
    let verdict = fire_beat(&ctx(
        dir.path(),
        minutely_registry(FILE),
        "2026-08-19T03:00:59.500Z",
        Box::new(move |span| {
            seen_waits.borrow_mut().push(span);
            Wait::Elapsed
        }),
        run,
    ));
    assert_eq!(verdict.code, exit::OK, "{}", verdict.line);
    assert!(verdict.line.contains("overlap-timeout"), "{}", verdict.line);
    assert_eq!(runs.get(), 0);
    assert_eq!(
        waits.borrow().as_slice(),
        &[SignedDuration::from_millis(500)]
    );
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
    let attempt = sidecar
        .acquire_beat_lock("doctor", holder.pid(), &now)
        .expect("lock");
    assert_eq!(attempt.outcome, LockOutcome::Acquired);
    let _lease = attempt.lease.expect("kernel lease");
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

/// A truncated ledger refuses before decision: corrupt evidence is never
/// reported as never-fired or silently healed by a fire.
#[test]
fn a_truncated_ledger_refuses_before_the_decision() {
    let dir = project("repair");
    let sidecar = ArmState::at_project(dir.path());
    // One anchored decision, then a partial unanchored append left by a crash.
    let mut seed = HistoryEntry::new(
        Some(ts("2026-08-18T03:00:00Z")),
        ts("2026-08-18T03:01:00Z"),
        FireKind::Skipped,
    );
    seed.reason = Some("overlap".to_owned());
    seed.exit = Some(0);
    sidecar.record("doctor", &seed).expect("record");
    let ledger = dir.path().join(".nika/arm/doctor/history.ndjson");
    let mut text = std::fs::read_to_string(&ledger).expect("ledger");
    text.push_str("{\"schema\":\"nika/arm-event@1\",\"seq\":2");
    std::fs::write(&ledger, text).expect("partial append");
    let (runs, run) = run_counter();
    let ctx = ctx(
        dir.path(),
        registry_with(SAUTER),
        "2026-08-19T10:00:00Z",
        instant_wait(),
        run,
    );
    let verdict = fire_beat(&ctx);
    assert_eq!(verdict.code, exit::ENV, "{}", verdict.line);
    assert!(
        verdict.line.contains("the record refused"),
        "{}",
        verdict.line
    );
    assert_eq!(runs.get(), 0);
    assert!(
        std::fs::read_to_string(&ledger)
            .expect("evidence")
            .ends_with("\"seq\":2"),
        "the corrupt bytes remain untouched"
    );
}
