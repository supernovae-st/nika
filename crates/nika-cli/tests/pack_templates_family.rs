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
const SLOT_ANSWER: &str = "answered by the committed corpus golden";

/// Templates whose committed golden still requires an explicit human answer.
///
/// `etl-state` holds a `nika:prompt` with NO `default:` **on purpose**: it
/// is the human gate that keeps NEP-0002's Rule of Two closed. Give that
/// prompt a default and `check` refuses the file with
/// `NIKA-SEC-009 lethal trifecta complete` — the gate IS the control, so an
/// unattended run has nothing legitimate to answer with. Its golden records
/// the safe explicit decline; the test supplies that answer without changing
/// the workflow's default-free security posture.
///
/// A name may only join this list with that shape of reason written down.
/// "It was red" is not a reason.
const REQUIRES_EXPLICIT_ANSWER: &[&str] = &["etl-state"];

fn scratch_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("nika-cli-pack-family-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// Lay one template down on disk, alone, the way `nika new` hands it
/// over — with its `<SLOT: …>` markers still in place.
fn plant_as_shipped(dir: &std::path::Path, name: &str) -> String {
    let body = nika_pack::template(name).unwrap_or_else(|| panic!("pack carries `{name}`"));
    write_at(dir, name, body)
}

/// The same skeleton with its slots answered — what an author holds a
/// minute later, and the state every claim below about auditing and
/// running is about.
fn plant(dir: &std::path::Path, name: &str) -> String {
    let body = nika_pack::template(name).unwrap_or_else(|| panic!("pack carries `{name}`"));
    write_at(dir, name, &fill_slots(body))
}

fn copy_committed_golden(name: &str, workflow: &str) {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../nika-pack/pack/templates")
        .join(format!("{name}.nika.yaml.golden.json"));
    let target = format!("{workflow}.golden.json");
    std::fs::copy(&source, &target).unwrap_or_else(|e| {
        panic!(
            "copy committed golden for `{name}` from {}: {e}",
            source.display()
        )
    });
}

fn committed_golden_names() -> Vec<String> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../nika-pack/pack/templates");
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read pack templates")
        .filter_map(Result::ok)
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()
                .and_then(|name| name.strip_suffix(".nika.yaml.golden.json"))
                .map(str::to_owned)
        })
        .collect();
    names.sort();
    names
}

fn write_at(dir: &std::path::Path, name: &str, body: &str) -> String {
    let path = dir.join(format!("{name}.nika.yaml"));
    std::fs::write(&path, body).expect("write template");
    path.to_str().expect("utf8 path").to_owned()
}

/// Answer every slot with one plain sentence.
///
/// Works on the SOURCE, so it has to close on the first `>` after the
/// opener rather than on a trimmed line ending — a quoted marker
/// (`query: "<SLOT: …>"`) closes before the quote.
fn fill_slots(body: &str) -> String {
    let mut out = body.to_owned();
    while let Some(open) = out.find(nika_check::MARKER_OPEN) {
        let Some(rel) = out[open..].find('>') else {
            break;
        };
        out.replace_range(open..=open + rel, SLOT_ANSWER);
    }
    out
}

/// Every `<SLOT:` a template carries in VALUE position — a marker
/// inside a comment is prose about the convention, not a hole.
fn markers_in(body: &str) -> usize {
    body.lines()
        .filter(
            |line| match (line.find(nika_check::MARKER_OPEN), line.find('#')) {
                (Some(slot), Some(hash)) => slot < hash,
                (Some(_), None) => true,
                (None, _) => false,
            },
        )
        .count()
}

/// #1066 — an unfilled scaffold is not yet a workflow, so it refuses
/// BEFORE the spend, and it refuses for every marker it carries.
///
/// The second half is the one that keeps the judge honest. `nika-check`
/// reads a NAMED set of value surfaces (`model:`, the `const:`/`inputs:`
/// literals, the `infer:`/`agent:` prompts) — a narrow probe returns a
/// clean result, so nothing but a count taken from the skeletons
/// themselves can prove the judge stayed as wide as the pack.
#[test]
fn an_unfilled_template_refuses_and_the_judge_sees_every_marker() {
    let dir = scratch_dir("slots");
    let mut marked: Vec<String> = Vec::new();
    for name in &nika_pack::template_names() {
        let body = nika_pack::template(name).unwrap_or_else(|| panic!("pack carries `{name}`"));
        let laid = markers_in(body);
        let path = plant_as_shipped(&dir, name);
        let parsed = nika_schema::parse(
            body,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .unwrap_or_else(|e| panic!("`{name}` parses as shipped: {e}"));
        let report = nika_check::check(&parsed);
        assert_eq!(
            report.slot_findings.len(),
            laid,
            "`{name}` lays {laid} marker(s) and the judge sees {} — a marker the judge \
             cannot reach is a hole that ships silently",
            report.slot_findings.len()
        );
        if laid == 0 {
            continue;
        }
        marked.push(name.clone());
        let out = check::run(&path, false, false, None, PLAIN);
        assert_ne!(
            out.code,
            exit::OK,
            "`{name}` carries {laid} unfilled slot(s) — it must refuse before the spend:\n{}",
            out.text
        );
        assert!(
            out.text.contains("ready to be filled"),
            "the refusal is an invitation, not an accusation (`{name}`):\n{}",
            out.text
        );
    }
    assert!(
        !marked.is_empty(),
        "no skeleton carries a slot — a traversal that proves nothing about \
         the class it exists for"
    );
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
            "`{name}` is a skeleton a stranger is handed — once its slots are \
             answered it audits, or it does not ship:\n{}",
            out.text
        );
    }
}

#[test]
fn every_shipped_template_runs_green_under_mock() {
    // The leg `check` cannot reach. Ordinary templates compare against the
    // COMMITTED pin with update=false: a missing or corrupt pin must fail this
    // test instead of being silently regenerated in scratch. The security-
    // gated template takes one explicit refusal decision for this invocation;
    // its workflow remains blocking and default-free.
    let dir = scratch_dir("run");
    let names = nika_pack::template_names();
    let committed = committed_golden_names();
    assert_eq!(
        committed, names,
        "every shipped template owns a committed golden"
    );

    let mut ran: Vec<String> = Vec::new();
    for name in &names {
        let path = plant(&dir, name);
        copy_committed_golden(name, &path);
        let code = if REQUIRES_EXPLICIT_ANSWER.contains(&name.as_str()) {
            nika_cli::verbs::test::run_with_answers(
                &path,
                false,
                &["approve=false".to_owned()],
                PLAIN,
            )
        } else {
            nika_cli::verbs::test::run(&path, false, PLAIN)
        };
        assert_eq!(code, exit::OK, "`{name}` executes under the mock plane");
        ran.push(name.clone());
    }

    assert_eq!(ran, names, "every shipped template actually executed");
}

#[test]
fn the_gated_template_stays_refusable_while_its_decline_is_pinned() {
    // The explicit-answer path above is only honest while its REASON holds.
    // If `etl-state` ever runs unattended, its human gate went away — and
    // that is a security change, not a test-maintenance chore.
    let dir = scratch_dir("gate");
    let name = REQUIRES_EXPLICIT_ANSWER
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
        nika_cli::verbs::test::run(&path, false, PLAIN),
        exit::OK,
        "`{name}` still refuses unattended execution — if this passes, the human gate \
         it depends on is gone and NEP-0002 needs re-checking"
    );
    copy_committed_golden(name, &path);
    assert_eq!(
        nika_cli::verbs::test::run_with_answers(&path, false, &["approve=false".to_owned()], PLAIN,),
        exit::OK,
        "the committed explicit decline executes the safe branch without weakening the gate"
    );

    let body = std::fs::read_to_string(&path).expect("read planted template");
    let weakened = body.replacen(
        "        message: \"Fetch",
        "        default: true\n        message: \"Fetch",
        1,
    );
    assert_ne!(weakened, body, "the test inserted the forbidden default");
    std::fs::write(&path, weakened).expect("write weakened specimen");
    let unsafe_report = check::run(&path, false, false, None, PLAIN);
    assert_ne!(unsafe_report.code, exit::OK, "the lethal trifecta refuses");
    assert!(
        unsafe_report.text.contains("NIKA-SEC-009"),
        "the refusal names the security law:\n{}",
        unsafe_report.text
    );
}
