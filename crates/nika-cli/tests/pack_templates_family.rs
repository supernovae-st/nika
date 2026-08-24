// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! The template FAMILY traversal — every shipped skeleton RUNS, not just
//! audits.
//!
//! G3 (`wiring-g3-unprobed`) counts 14 template declarations with no probe.
//! `nika check` on all 14 was already green while ten of them had never
//! executed once in CI: **a check rc=0 proves the audit, never the run**,
//! which is the whole reason `nika test` exists.
//!
//! This walks the family through the mock provider — offline, deterministic,
//! zero keys, zero spend — because a skeleton a stranger scaffolds with
//! `nika new` and cannot run is a broken first minute.

use nika_cli::Theme;
use nika_cli::verbs::{check, exit};

const PLAIN: Theme = Theme::new(false, false, false);

/// Templates that cannot be pinned by an unattended golden, and why.
///
/// `etl-state` holds a `nika:prompt` with NO `default:` **on purpose**: it
/// is the human gate that keeps NEP-0002's Rule of Two closed. Give that
/// prompt a default and `check` refuses the file with
/// `NIKA-SEC-009 lethal trifecta complete` — the gate IS the control, so an
/// unattended run has nothing legitimate to answer with.
///
/// A name may only join this list with that shape of reason written down.
/// "It was red" is not a reason.
const CANNOT_PIN_UNATTENDED: &[&str] = &["etl-state"];

fn scratch_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("nika-cli-pack-family-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// Lay one template down on disk, alone, the way `nika new` hands it over.
fn plant(dir: &std::path::Path, name: &str) -> String {
    let body = nika_pack::template(name).unwrap_or_else(|| panic!("pack carries `{name}`"));
    let path = dir.join(format!("{name}.nika.yaml"));
    std::fs::write(&path, body).expect("write template");
    path.to_str().expect("utf8 path").to_owned()
}

#[test]
fn every_shipped_template_audits() {
    let dir = scratch_dir("check");
    let names = nika_pack::template_names();
    assert!(!names.is_empty(), "the pack ships templates");

    for name in &names {
        let path = plant(&dir, name);
        let out = check::run(&path, false, false, None, PLAIN);
        assert_eq!(
            out.code,
            exit::OK,
            "`{name}` is a skeleton a stranger is handed — it audits or it does not ship:\n{}",
            out.text
        );
    }
}

#[test]
fn every_shipped_template_runs_green_under_mock() {
    // The leg `check` cannot reach. `--update` writes the golden into the
    // scratch copy and returns 0 only when the mock run itself was green,
    // so this asserts EXECUTION, not the presence of a committed file.
    let dir = scratch_dir("run");
    let names = nika_pack::template_names();

    let mut ran: Vec<String> = Vec::new();
    let mut refused: Vec<String> = Vec::new();
    for name in &names {
        let path = plant(&dir, name);
        let code = nika_cli::verbs::test::run(&path, true, PLAIN);
        if code == exit::OK {
            ran.push(name.clone());
        } else {
            refused.push(name.clone());
        }
    }

    assert_eq!(
        refused,
        CANNOT_PIN_UNATTENDED,
        "the set of templates that cannot run unattended moved.\n\
         ran ({}): {ran:?}\nrefused ({}): {refused:?}\n\
         Read the CANNOT_PIN_UNATTENDED doc comment — a template joining this list \
         needs a reason of the same shape, not a rubber stamp.",
        ran.len(),
        refused.len()
    );
    assert!(
        !ran.is_empty(),
        "at least one template actually executed — a traversal where nothing ran \
         proves the harness, not the pack"
    );
}

#[test]
fn the_gated_template_stays_refusable_for_the_reason_it_is_exempt() {
    // The exemption above is only honest while its REASON holds. If
    // `etl-state` ever becomes pinnable, it is because its human gate went
    // away — and that is a security change, not a test-maintenance chore.
    let dir = scratch_dir("gate");
    let name = CANNOT_PIN_UNATTENDED
        .first()
        .expect("the exempt set is non-empty");
    let path = plant(&dir, name);

    let audited = check::run(&path, false, false, None, PLAIN);
    assert_eq!(
        audited.code,
        exit::OK,
        "`{name}` audits clean as shipped — the exemption is about the RUN, not the audit:\n{}",
        audited.text
    );
    assert!(
        audited.text.contains("TRIFECTA"),
        "the rung that makes this template's gate load-bearing is present:\n{}",
        audited.text
    );
    assert_ne!(
        nika_cli::verbs::test::run(&path, true, PLAIN),
        exit::OK,
        "`{name}` still refuses an unattended golden — if this passes, the human gate \
         it depends on is gone and NEP-0002 needs re-checking"
    );
}
