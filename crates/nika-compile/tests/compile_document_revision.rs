// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A complete document revised at its exact revision, then judged by the
//! compile core's own door (strict parse, pure Check, status) and by Check
//! alone: an edit keeps every untouched verdict, a negative program stays
//! negative, and the constant door and the document edit write the same
//! bytes. No file is written, no provider is called.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]

use nika_compile::{CompileRequest, CompileStatus, compile, finish, initial};
use nika_compile_fidelity::document::{
    Document, DocumentRevision, Edit, Origin, Path, Refusal, Revision, RevisionRefusal,
};
use serde_json::json;

fn fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../nika-schema/tests/document_fixtures")
        .join(name);
    std::fs::read_to_string(path).expect("a document fixture")
}

fn path(dotted: &str) -> Path {
    Path::dotted(dotted).expect("a dotted path")
}

/// The finding codes of a pure Check, sorted.
fn codes(document: &Document) -> Vec<String> {
    let report = nika_check::check(document.workflow());
    let mut out: Vec<String> = report
        .findings
        .iter()
        .map(|f| f.code.clone().unwrap_or_else(|| f.kind.to_owned()))
        .collect();
    out.sort();
    out
}

#[test]
fn a_rich_revision_is_judged_by_the_door_every_candidate_takes() {
    let base = fixture("rich-revision.nika");
    let first = DocumentRevision::import(base.clone()).expect("import");
    let mut judged = initial();
    finish(base.clone(), &mut judged);
    assert_eq!(
        judged.status,
        CompileStatus::Ready,
        "{:?}",
        judged.diagnostics
    );
    let second = first
        .apply(
            first.revision(),
            &[Edit::set(path("const.window_hours"), json!(72))],
        )
        .expect("48 to 72");
    let mut revised = initial();
    finish(second.source().to_owned(), &mut revised);
    assert_eq!(
        revised.status,
        CompileStatus::Ready,
        "{:?}",
        revised.diagnostics
    );
    assert_eq!(revised.candidate.as_deref(), Some(second.source()));
    assert_eq!(codes(second.document()), codes(first.document()));
    assert_eq!(
        revised
            .requested_boundary
            .as_ref()
            .map(|b| format!("{b:?}")),
        judged.requested_boundary.as_ref().map(|b| format!("{b:?}")),
        "an unrelated value keeps the requested boundary"
    );
    assert_eq!(second.parent(), Some(first.revision()));
    assert_eq!(second.revision(), &Revision::of(second.source()));
    // A correction typed against the first revision cannot overwrite the second.
    let late = second
        .apply(
            first.revision(),
            &[Edit::set(path("const.window_hours"), json!(96))],
        )
        .expect_err("stale");
    assert!(matches!(late, RevisionRefusal::Stale { .. }), "{late}");
}

/// The constant door writes compact JSON; the document edit keeps the
/// author's presentation. Both mean the same, and where the presentation is
/// plain or flow they write the very same bytes.
#[test]
fn the_constant_door_and_the_document_edit_agree() {
    let base = fixture("rich-revision.nika");
    for (name, literal, value, same_bytes) in [
        ("window_hours", "72", json!(72), true),
        ("limits", "5", json!(5), true),
        ("label", "\"it's early\"", json!("it's early"), false),
    ] {
        let door = compile(&CompileRequest::set_constant(&base, name, literal)).expect("compile");
        assert_eq!(
            door.status,
            CompileStatus::Ready,
            "{name}: {:?}",
            door.diagnostics
        );
        let candidate = door.candidate.expect("a candidate");
        let document = Document::parse(base.clone()).expect("import");
        let at = document.constant_path(name).expect("declared");
        let applied = document.apply(&[Edit::set(at, value)]).expect("edit");
        let edited = applied.document().source();
        assert_eq!(
            Document::parse(candidate.clone())
                .expect("the door's bytes")
                .literal(),
            applied.document().literal(),
            "{name}: the same meaning"
        );
        assert_eq!(
            candidate == edited,
            same_bytes,
            "{name}:\n{candidate}\n{edited}"
        );
    }
    let document = Document::parse(base.clone()).expect("import");
    let kept = document
        .apply(&[Edit::set(path("const.label"), json!("it's early"))])
        .expect("label");
    assert!(kept.document().source().contains("label: 'it''s early'"));
}

#[test]
fn an_exact_component_joins_the_document_and_checks_clean() {
    let base = fixture("rich-revision.nika");
    let first = DocumentRevision::import(base).expect("import");
    let component = "# a reused notice, its bytes kept\nafter: { persist: success }\ninvoke:\n  tool: \"nika:log\"\n  args: { message: \"window ${{ const.window_hours }}h done\" }\n";
    let second = first
        .apply(
            first.revision(),
            &[Edit::insert_text(path("tasks"), "notice", component)],
        )
        .expect("insert");
    let Origin::Edited { changed, .. } = second.origin() else {
        panic!("an edit");
    };
    assert_eq!(changed, &[path("tasks.notice")]);
    let report = nika_check::check(second.document().workflow());
    assert!(report.is_clean(), "{:?}", report.findings);
    let third = second
        .apply(
            second.revision(),
            &[Edit::set(path("const.window_hours"), json!(72))],
        )
        .expect("72");
    assert!(
        third
            .source()
            .contains("message: \"window ${{ const.window_hours }}h done\"")
    );
    assert!(nika_check::check(third.document().workflow()).is_clean());
}

/// Every pack negative states its expected code on a `# Expected · NIKA-…` line.
fn negatives() -> Vec<(String, String, String)> {
    let shelf =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../nika-pack/pack/templates");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(shelf).expect("the template shelf") {
        let file = entry.expect("an entry").path();
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        if !name.ends_with(".negative.yaml") {
            continue;
        }
        let body = std::fs::read_to_string(&file).expect("a negative");
        let code = body
            .lines()
            .find_map(|l| l.strip_prefix("# Expected · "))
            .map(|c| c.trim_end_matches('.').to_owned())
            .expect("a declared code");
        out.push((name, body, code));
    }
    out.sort();
    out
}

#[test]
fn every_negative_program_stays_negative_after_an_unrelated_revision() {
    let mut revised = 0;
    for (name, body, code) in negatives() {
        let first = match DocumentRevision::import(body) {
            Ok(first) => first,
            // Refused by the strict parser, for its declared reason: no
            // revision can make it a candidate.
            Err(Refusal::Language { error, .. }) => {
                assert_eq!(error.spec_code().to_string(), code, "{name}: {error}");
                continue;
            }
            Err(other) => panic!("{name}: {other}"),
        };
        let renamed = format!(
            "{}-revised",
            first.document().workflow().workflow.as_ref().unwrap().value
        );
        let second = first
            .apply(first.revision(), &[Edit::set(path("nika"), json!(renamed))])
            .unwrap_or_else(|refusal| panic!("{name}: {refusal}"));
        let before = codes(first.document());
        let after = codes(second.document());
        assert!(
            before.contains(&code),
            "{name}: the base shows {code}: {before:?}"
        );
        assert_eq!(
            after, before,
            "{name}: the verdict of an unrelated revision"
        );
        let mut door = initial();
        finish(second.source().to_owned(), &mut door);
        assert_ne!(
            door.status,
            CompileStatus::Ready,
            "{name} stays refused at the door"
        );
        revised += 1;
    }
    assert!(revised >= 10, "negatives revised: {revised}");
}

#[test]
fn every_clean_pack_program_stays_clean_after_an_unrelated_revision() {
    let mut programs: Vec<(String, &'static str)> = Vec::new();
    for slug in nika_pack::example_slugs() {
        if let Some(source) = nika_pack::example(&slug) {
            programs.push((format!("example/{slug}"), source));
        }
    }
    for name in nika_pack::template_names() {
        if let Some(source) = nika_pack::template(&name) {
            programs.push((format!("template/{name}"), source));
        }
    }
    let mut clean = 0;
    for (name, source) in programs {
        let Ok(first) = DocumentRevision::import(source) else {
            continue;
        };
        if !nika_check::check(first.document().workflow()).is_clean() {
            continue;
        }
        let renamed = format!(
            "{}-revised",
            first.document().workflow().workflow.as_ref().unwrap().value
        );
        let second = first
            .apply(first.revision(), &[Edit::set(path("nika"), json!(renamed))])
            .unwrap_or_else(|refusal| panic!("{name}: {refusal}"));
        let report = nika_check::check(second.document().workflow());
        assert!(report.is_clean(), "{name}: {:?}", report.findings);
        // A renamed task keeps every reference: a missed one is a dangling read.
        let ids: Vec<String> = first
            .document()
            .workflow()
            .tasks
            .iter()
            .map(|t| t.value.id.value.clone())
            .collect();
        for id in ids {
            let renamed = first
                .apply(
                    first.revision(),
                    &[Edit::rename(
                        Path::new(["tasks", id.as_str()]),
                        format!("{id}_r"),
                    )],
                )
                .unwrap_or_else(|refusal| panic!("{name} {id}: {refusal}"));
            let report = nika_check::check(renamed.document().workflow());
            assert!(
                report.is_clean(),
                "{name} renamed {id}: {:?}",
                report.findings
            );
        }
        clean += 1;
    }
    assert!(clean >= 20, "clean programs revised: {clean}");
}
