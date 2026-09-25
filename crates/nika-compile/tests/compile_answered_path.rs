// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! An answered path completes the empty placeholder a native candidate declared, and nothing
//! else. A seat that asks where a file goes can only declare `fs.write: [""]` while the path
//! is unknown (the one narrow boundary its judge admits); the answer round replays its record
//! with zero calls, and the compiler completes that placeholder from the answer as it grants
//! an answered endpoint's host: the exact path the capability inference derives, in the
//! direction the tool uses, inside the workspace. DIALOG-01 (2026-09-24, f140be40) refused
//! even a correct destination: nothing completed the placeholder.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::surface::literal_projection;
use nika_compile::{
    CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, compile, intent_sha256,
};
use serde_json::{Value, json};

const INTENT: &str = "Copie entree.txt vers le fichier que je vais choisir.";

/// The seat's copy candidate in DIALOG-01's shape, its destination asked.
fn copy_to(write: &str) -> String {
    format!(
        r#"nika: copy-to-chosen-file
const:
  destination_path: ""
permits:
  fs:
    read: ["./entree.txt"]
    write: {write}
  tools: ["nika:read", "nika:write"]
tasks:
  read_source:
    invoke:
      tool: "nika:read"
      args: {{ path: "./entree.txt" }}
  write_destination:
    with: {{ text: "${{{{ tasks.read_source.output }}}}" }}
    invoke:
      tool: "nika:write"
      args: {{ path: "${{{{ const.destination_path }}}}", content: "${{{{ with.text }}}}" }}
"#
    )
}

fn asked(keys: &[&str]) -> Value {
    json!(
        keys.iter()
            .map(|key| json!({"key": key, "label": "Destination file path", "answer_type": "text", "why": "The request leaves it open."}))
            .collect::<Vec<_>>()
    )
}

/// The answer round: the recorded native candidate replayed with these answers.
fn answered(source: &str, questions: &Value, answers: &[(&str, &str)]) -> CompileOutcome {
    let record = json!({
        "strategy": "native",
        "intent_sha256": intent_sha256(INTENT),
        "source": source,
        "questions": questions,
        "gaps": [],
        "trigger": null,
    });
    let mut request = CompileRequest::create(INTENT).with_plan(record);
    for (key, literal) in answers {
        request = request.answer(*key, *literal);
    }
    compile(&request).unwrap()
}

/// The candidate's `permits.fs.<direction>` as emitted (null without a candidate).
fn fs(out: &CompileOutcome, direction: &str) -> Value {
    literal_projection(out.candidate.as_deref().unwrap_or_default()).unwrap_or_default()["permits"]
        ["fs"][direction]
        .clone()
}

fn granted(out: &CompileOutcome, direction: &str) -> bool {
    out.diagnostics
        .iter()
        .any(|d| d.kind == DiagnosticKind::Applied && d.target == format!("permits.fs.{direction}"))
}

#[test]
fn an_answered_destination_completes_the_empty_write_placeholder() {
    for path in [
        "sortie.txt",
        "./out/sortie.txt",
        "exports/rapport final.txt",
    ] {
        let literal = json!(path).to_string();
        let out = answered(
            &copy_to(r#"[""]"#),
            &asked(&["const.destination_path"]),
            &[("const.destination_path", literal.as_str())],
        );
        assert_eq!(out.status, CompileStatus::Ready, "{path}: {out:#?}");
        assert_eq!(fs(&out, "write"), json!([path]), "{path}");
        assert_eq!(
            fs(&out, "read"),
            json!(["./entree.txt"]),
            "{path}: read untouched"
        );
        assert!(
            granted(&out, "write") && !granted(&out, "read"),
            "{path}: {out:#?}"
        );
    }
}

#[test]
fn an_escaping_absolute_home_or_glob_answer_is_never_granted() {
    for path in [
        "/etc/passwd",
        "../sortie.txt",
        "a/../../sortie.txt",
        "~/sortie.txt",
        "$HOME/sortie.txt",
        "out/*.txt",
        "out/?.txt",
        "out/[ab].txt",
    ] {
        let literal = json!(path).to_string();
        let out = answered(
            &copy_to(r#"[""]"#),
            &asked(&["const.destination_path"]),
            &[("const.destination_path", literal.as_str())],
        );
        assert_ne!(out.status, CompileStatus::Ready, "{path}: {out:#?}");
        assert_eq!(
            fs(&out, "write"),
            json!([""]),
            "{path}: the placeholder stays"
        );
        assert!(!granted(&out, "write"), "{path}");
    }
}

#[test]
fn an_explicit_or_mixed_boundary_is_never_widened() {
    for write in [r#"["./out/**"]"#, r#"["./out/**", ""]"#, r#"["", ""]"#] {
        let out = answered(
            &copy_to(write),
            &asked(&["const.destination_path"]),
            &[("const.destination_path", r#""sortie.txt""#)],
        );
        let declared: Value = serde_json::from_str(write).unwrap();
        assert_eq!(fs(&out, "write"), declared, "{write}");
        assert!(!granted(&out, "write"), "{write}");
        assert_ne!(
            out.status,
            CompileStatus::Ready,
            "{write}: sortie.txt stays outside what the seat declared"
        );
    }
}

#[test]
fn only_the_direction_the_answered_path_rides_is_completed() {
    let source = r#"nika: copy-from-chosen-file
const:
  source_path: ""
permits:
  fs:
    read: [""]
    write: ["./copie.txt"]
  tools: ["nika:read", "nika:write"]
tasks:
  read_source:
    invoke:
      tool: "nika:read"
      args: { path: "${{ const.source_path }}" }
  write_copy:
    with: { text: "${{ tasks.read_source.output }}" }
    invoke:
      tool: "nika:write"
      args: { path: "./copie.txt", content: "${{ with.text }}" }
"#;
    let out = answered(
        source,
        &asked(&["const.source_path"]),
        &[("const.source_path", r#""entree.txt""#)],
    );
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(fs(&out, "read"), json!(["entree.txt"]));
    assert_eq!(fs(&out, "write"), json!(["./copie.txt"]));
    assert!(granted(&out, "read") && !granted(&out, "write"));
}

/// A second answered constant that looks like a path but is never one (the content), and
/// a placeholder left unanswered: neither grants anything.
#[test]
fn an_unrelated_or_unanswered_constant_grants_nothing() {
    let source = r#"nika: write-a-note
const:
  destination_path: ""
  note_text: ""
permits:
  fs:
    write: [""]
  tools: ["nika:write"]
tasks:
  write_note:
    invoke:
      tool: "nika:write"
      args: { path: "${{ const.destination_path }}", content: "${{ const.note_text }}" }
"#;
    let questions = asked(&["const.destination_path", "const.note_text"]);
    let out = answered(
        source,
        &questions,
        &[
            ("const.destination_path", r#""notes.txt""#),
            ("const.note_text", r#""./secret.txt""#),
        ],
    );
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(fs(&out, "write"), json!(["notes.txt"]));
    assert_eq!(fs(&out, "read"), Value::Null);

    let open = answered(
        source,
        &questions,
        &[("const.note_text", r#""./secret.txt""#)],
    );
    assert_eq!(open.status, CompileStatus::Incomplete, "{open:#?}");
    assert!(
        open.candidate.is_none(),
        "no candidate while a question is open"
    );
    assert!(
        open.questions
            .iter()
            .any(|q| q.key == "const.destination_path" && q.mandatory)
    );
    assert!(!granted(&open, "write"));
}

/// A path composed around the constant is not the answer alone: the inference cannot pin
/// it, so nothing is granted from it (the run's own gate judges the resolved path).
#[test]
fn a_composed_path_is_never_granted_from_an_answer() {
    let source = copy_to(r#"[""]"#).replace(
        r#"path: "${{ const.destination_path }}""#,
        r#"path: "./out/${{ const.destination_path }}.txt""#,
    );
    assert!(source.contains("./out/${{ const.destination_path }}.txt"));
    let out = answered(
        &source,
        &asked(&["const.destination_path"]),
        &[("const.destination_path", r#""rapport""#)],
    );
    assert_eq!(fs(&out, "write"), json!([""]), "{out:#?}");
    assert!(!granted(&out, "write"));
}

/// The seat may write the same placeholder as a block sequence (`- ""`). The answered path
/// completes it exactly as the flow form: the bounded editor writes the one flow list in
/// the block's place (from its `-` to its item, both readers validating the document) and
/// leaves every other byte alone. A block boundary with any other entry is kept byte for
/// byte and never widened.
#[test]
fn a_block_placeholder_completes_the_same_and_any_other_block_is_kept() {
    let flow = copy_to(r#"[""]"#);
    let placeholder = "    write:\n      - \"\"\n";
    let block = flow.replace("    write: [\"\"]\n", placeholder);
    assert!(block.contains(placeholder), "{block}");
    let answer = [("const.destination_path", r#""sortie.txt""#)];
    let questions = asked(&["const.destination_path"]);
    let out = answered(&block, &questions, &answer);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(fs(&out, "write"), json!(["sortie.txt"]));
    assert!(granted(&out, "write") && !granted(&out, "read"));
    let expected = block
        .replace(placeholder, "    write:\n      [\"sortie.txt\"]\n")
        .replace(
            "  destination_path: \"\"\n",
            "  destination_path: \"sortie.txt\"\n",
        );
    assert_eq!(out.candidate.as_deref(), Some(expected.as_str()));

    for entries in [
        "      - \"./out/**\"\n      - \"\"\n",
        "      - \"\"\n      - \"\"\n",
        "      - \"./out/**\"\n",
    ] {
        let kept = flow.replace("    write: [\"\"]\n", &format!("    write:\n{entries}"));
        let out = answered(&kept, &questions, &answer);
        assert!(!granted(&out, "write"), "{entries}");
        assert_ne!(out.status, CompileStatus::Ready, "{entries}");
        let source = out.candidate.as_deref().unwrap_or_default();
        assert!(
            source.contains(entries),
            "the seat's block stays as written: {source}"
        );
    }
}
