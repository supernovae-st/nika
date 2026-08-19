// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika arm migrate` (W7 · D2) — the explicit one-shot upcast.
//!
//! Every beat is named. A W2 journal rotates verbatim, the resulting
//! chain is verified, and `last.json` + `watermark` are rebuilt by
//! replay. Running the verb twice performs no second rotation.

use std::fmt::Write as _;
use std::path::Path;

use jiff::Timestamp;

use super::state::{ArmState, HealOutcome};
use crate::verbs::{VerbOutput, exit};

/// Migrate the sidecar below the current project directory.
#[must_use]
pub fn run() -> VerbOutput {
    let cwd = match std::env::current_dir() {
        Ok(path) => path,
        Err(e) => {
            return VerbOutput::env(format!("arm migrate · cannot read current directory: {e}"));
        }
    };
    run_at(&cwd, &jiff::Zoned::now().timestamp())
}

/// The deterministic migration edge (the clock is injected by tests).
#[must_use]
pub fn run_at(project_root: &Path, now: &Timestamp) -> VerbOutput {
    let state = ArmState::at_project(project_root);
    let labels = match state.beat_dirs() {
        Ok(labels) => labels,
        Err(e) => return VerbOutput::env(format!("arm migrate · cannot inspect sidecars: {e}")),
    };
    if labels.is_empty() {
        return VerbOutput::ok("arm migrate · rien à migrer".to_owned());
    }

    let mut text = String::new();
    let mut migrated = 0usize;
    let mut refused = Vec::new();
    for label in labels {
        let journal = project_root
            .join(".nika/arm")
            .join(&label)
            .join("history.ndjson");
        if !journal.exists() {
            let _ = writeln!(text, "{label} · aucun journal — ignoré");
            continue;
        }
        match state.heal(&label, now) {
            Ok(outcome) => {
                migrated += 1;
                let _ = writeln!(text, "{}", render_outcome(&label, &outcome));
            }
            Err(e) => {
                refused.push(label.clone());
                let _ = writeln!(text, "{label} · REFUSÉ — {e}");
            }
        }
    }

    let _ = write!(
        text,
        "migrate · {} · {}",
        crate::text::count(migrated, "beat"),
        crate::text::count(refused.len(), "refusé")
    );
    if !refused.is_empty() {
        let _ = write!(text, ": {}", refused.join(", "));
    }
    if refused.is_empty() {
        VerbOutput::ok(text)
    } else {
        VerbOutput {
            text,
            code: exit::WORKFLOW,
        }
    }
}

/// One complete, audible per-beat verdict.
fn render_outcome(label: &str, outcome: &HealOutcome) -> String {
    let mut parts = Vec::new();
    if let Some(rotation) = &outcome.rotated {
        parts.push(format!(
            "rotated {} → {}",
            crate::text::count(rotation.lines, "ligne"),
            rotation.name
        ));
    }
    parts.push(format!(
        "chaîne ok ({})",
        crate::text::count(
            usize::try_from(outcome.lines).unwrap_or(usize::MAX),
            "ligne",
        )
    ));
    if outcome.repaired > 0 {
        parts.push(format!(
            "réparé (-{})",
            crate::text::count(
                usize::try_from(outcome.repaired).unwrap_or(usize::MAX),
                "ligne",
            )
        ));
    }
    match (outcome.rebuilt_last, outcome.rebuilt_watermark) {
        (true, true) => parts.push("reconstruit last.json + watermark".to_owned()),
        (true, false) => parts.push("reconstruit last.json".to_owned()),
        (false, true) => parts.push("reconstruit watermark".to_owned()),
        (false, false) => parts.push("aucune projection".to_owned()),
    }
    format!("{label} · {}", parts.join(" · "))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use jiff::Timestamp;

    use super::run_at;
    use crate::verbs::VerbOutput;
    use crate::verbs::arm::state::{ArmState, FireKind, HistoryEntry};
    use crate::verbs::exit;

    fn project(tag: &str) -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(&format!("nika-arm-migrate-{tag}-"))
            .tempdir()
            .expect("tmp dir")
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

    /// A W2-era journal: a fire then a skip, pre-ledger (no `schema`).
    const LEGACY: &str = concat!(
        "{\"slot\":\"2026-08-18T03:00:00Z\",\"decided_at\":\"2026-08-18T03:02:00Z\",\"kind\":\"fired\",\"reason\":null,\"trace\":null,\"exit\":0,\"slots\":null}\n",
        "{\"slot\":\"2026-08-19T03:00:00Z\",\"decided_at\":\"2026-08-19T03:02:00Z\",\"kind\":\"skipped\",\"reason\":\"missed:1\",\"trace\":null,\"exit\":0,\"slots\":null}\n",
    );

    fn seed_legacy(dir: &std::path::Path, label: &str) {
        let sidecar = dir.join(".nika/arm").join(label);
        std::fs::create_dir_all(&sidecar).expect("sidecar");
        std::fs::write(sidecar.join("history.ndjson"), LEGACY).expect("legacy");
    }

    /// Restore a directory's mode on drop — the tempdir cleanup needs
    /// the write bit back, panic or not.
    #[cfg(unix)]
    struct ModeGuard(std::path::PathBuf);

    #[cfg(unix)]
    impl Drop for ModeGuard {
        fn drop(&mut self) {
            use std::os::unix::fs::PermissionsExt as _;
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
        }
    }

    /// (e) · the one-shot upcast on a full W2-era sidecar: the rotation
    /// (verbatim, N4), the green chain, the projections rebuilt BY
    /// REPLAY — and every gesture NAMED (FCI-003, never silent).
    #[test]
    fn a_w2_sidecar_migrates_with_every_gesture_named() {
        let dir = project("w2");
        seed_legacy(dir.path(), "doctor");
        let doctor = dir.path().join(".nika/arm/doctor");
        // A STALE projection — it claims slot 1; the journal decided
        // slot 2. The chain is the truth: the rebuild must disagree.
        std::fs::write(
            doctor.join("last.json"),
            "{\"slot\":\"2026-08-18T03:00:00Z\",\"fired_at\":\"2026-08-18T03:02:00Z\",\"trace\":null,\"exit\":0,\"kind\":\"fired\"}\n",
        )
        .expect("stale last.json");
        let out: VerbOutput = run_at(dir.path(), &ts("2026-08-19T12:00:00Z"));
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(
            out.text.contains(
                "doctor · rotated 2 lignes → history-w2.ndjson · chaîne ok (1 ligne) · reconstruit last.json + watermark"
            ),
            "every gesture named: {}",
            out.text
        );
        assert!(
            out.text.contains("migrate · 1 beat · 0 refusés"),
            "the summary: {}",
            out.text
        );
        // N4: the archive is verbatim, kept forever.
        let archived = std::fs::read_to_string(doctor.join("history-w2.ndjson")).expect("archive");
        assert_eq!(archived, LEGACY, "the legacy journal is kept verbatim");
        // The fresh chain opens with the rotation receipt.
        let chain = std::fs::read_to_string(doctor.join("history.ndjson")).expect("chain");
        let first: serde_json::Value =
            serde_json::from_str(chain.lines().next().expect("one line")).expect("json");
        assert_eq!(first["kind"], "rotated", "{chain}");
        assert_eq!(first["payload"]["from"], "history-w2.ndjson", "{chain}");
        // The projection is rebuilt FROM THE JOURNAL (slot 2 ·
        // skipped) — the stale claim is gone.
        let last = std::fs::read_to_string(doctor.join("last.json")).expect("rebuilt");
        assert!(last.contains("\"slot\":\"2026-08-19T03:00:00Z\""), "{last}");
        assert!(last.contains("\"kind\":\"skipped\""), "{last}");
        let watermark = std::fs::read_to_string(doctor.join("watermark")).expect("watermark");
        assert_eq!(watermark, "2026-08-19T03:02:00Z\n");
        // … and the migrated chain verifies clean for the next append.
        let state = ArmState::at_project(dir.path());
        let outcome = state
            .record(
                "doctor",
                &entry(
                    FireKind::Fired,
                    "2026-08-20T03:00:00Z",
                    "2026-08-20T03:02:00Z",
                ),
            )
            .expect("record");
        assert_eq!(outcome.repaired, 0, "the migrated chain verifies");
        assert_eq!(outcome.seq, 2, "the rotation receipt is seq 1");
    }

    /// A versioned, healthy chain: no rotation gesture, the projections
    /// rebuilt BYTE-IDENTICAL from the replay.
    #[test]
    fn a_healthy_chain_is_reprojected_without_a_rotation() {
        let dir = project("healthy");
        let state = ArmState::at_project(dir.path());
        state
            .record(
                "nightly",
                &entry(
                    FireKind::Fired,
                    "2026-08-18T03:00:00Z",
                    "2026-08-18T03:02:00Z",
                ),
            )
            .expect("one");
        state
            .record(
                "nightly",
                &entry(
                    FireKind::Skipped,
                    "2026-08-19T03:00:00Z",
                    "2026-08-19T03:02:00Z",
                ),
            )
            .expect("two");
        // The emit log dir rides along — reported, never migrated.
        std::fs::create_dir_all(dir.path().join(".nika/arm/logs")).expect("logs");
        let last_path = dir.path().join(".nika/arm/nightly/last.json");
        let original = std::fs::read_to_string(&last_path).expect("last.json");
        std::fs::remove_file(&last_path).expect("delete last.json");
        std::fs::remove_file(dir.path().join(".nika/arm/nightly/watermark"))
            .expect("delete watermark");
        let out = run_at(dir.path(), &ts("2026-08-19T12:00:00Z"));
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(
            out.text
                .contains("nightly · chaîne ok (2 lignes) · reconstruit last.json + watermark"),
            "no rotation gesture on a versioned chain: {}",
            out.text
        );
        assert!(
            out.text.contains("logs · aucun journal — ignoré"),
            "the non-sidecar is named too: {}",
            out.text
        );
        let rebuilt = std::fs::read_to_string(&last_path).expect("rebuilt");
        assert_eq!(rebuilt, original, "byte-identical reprojection");
        let watermark = std::fs::read_to_string(dir.path().join(".nika/arm/nightly/watermark"))
            .expect("watermark");
        assert_eq!(watermark, "2026-08-19T03:02:00Z\n");
    }

    /// D2's one-shot is idempotent: after the first upcast, a second
    /// invocation rotates nothing, preserves both journals byte for
    /// byte, and still says what it verified and rebuilt.
    #[test]
    fn migrate_is_idempotent_after_the_w2_upcast() {
        let dir = project("idempotent");
        seed_legacy(dir.path(), "doctor");
        let now = ts("2026-08-19T12:00:00Z");
        let first = run_at(dir.path(), &now);
        assert_eq!(first.code, exit::OK, "{}", first.text);
        let sidecar = dir.path().join(".nika/arm/doctor");
        let archive = std::fs::read(sidecar.join("history-w2.ndjson")).expect("archive");
        let ledger = std::fs::read(sidecar.join("history.ndjson")).expect("ledger");

        let second = run_at(dir.path(), &now);
        assert_eq!(second.code, exit::OK, "{}", second.text);
        assert!(
            second
                .text
                .contains("doctor · chaîne ok (1 ligne) · reconstruit last.json + watermark"),
            "the second run reports verification without rotation: {}",
            second.text
        );
        assert!(!second.text.contains("rotated"), "{}", second.text);
        assert_eq!(
            std::fs::read(sidecar.join("history-w2.ndjson")).expect("archive again"),
            archive,
            "the legacy archive is immutable"
        );
        assert_eq!(
            std::fs::read(sidecar.join("history.ndjson")).expect("ledger again"),
            ledger,
            "the versioned chain is unchanged"
        );
    }

    /// A beat whose sidecar refuses the gesture is NAMED, the exit is
    /// 1 — and the other beats heal anyway.
    #[cfg(unix)]
    #[test]
    fn a_beat_that_refuses_is_named_and_the_exit_is_one() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = project("refused");
        seed_legacy(dir.path(), "broken");
        seed_legacy(dir.path(), "whole");
        let broken = dir.path().join(".nika/arm/broken");
        std::fs::set_permissions(&broken, std::fs::Permissions::from_mode(0o555))
            .expect("read-only");
        let _restore = ModeGuard(broken.clone());
        let out = run_at(dir.path(), &ts("2026-08-19T12:00:00Z"));
        assert_eq!(out.code, 1, "a refusal exits 1: {}", out.text);
        assert!(out.text.contains("broken · REFUSÉ"), "named: {}", out.text);
        assert!(
            out.text.contains("1 refusé: broken"),
            "the summary names it: {}",
            out.text
        );
        // The other beat healed anyway.
        let whole = dir.path().join(".nika/arm/whole");
        assert!(
            whole.join("history-w2.ndjson").exists(),
            "the healthy beat rotated"
        );
        assert!(out.text.contains("whole · rotated"), "{}", out.text);
    }

    /// No sidecar at all: a green nothing (migrate is not a failure
    /// where nothing was ever armed).
    #[test]
    fn nothing_to_migrate_without_a_sidecar() {
        let dir = project("empty");
        let out = run_at(dir.path(), &ts("2026-08-19T12:00:00Z"));
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(out.text.contains("rien à migrer"), "{}", out.text);
    }
}
