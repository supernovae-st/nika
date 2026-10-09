// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! An answered path completes the empty placeholder a native candidate declared, and nothing
//! else. A seat that asks where a file goes can only declare `fs.write: [""]` while the path
//! is unknown (the one narrow boundary its judge admits); the answer round replays its record
//! with zero calls, and the compiler completes that placeholder from the answer as it grants
//! an answered endpoint's host: the exact path the capability inference derives, in the
//! direction the tool uses, inside the workspace. DIALOG-01 (2026-09-24, f140be40) refused
//! even a correct destination: nothing completed the placeholder. The finish then waits for its
//! round's judge (R4 A11, step 2): these keyless rounds permit none, so an admitted finish is
//! held on the whole request alone, and a refused one never is.
//!
//! The placeholder is the one empty entry of its side, beside the entries the seat stated: an
//! open name (`un payload out/notification.json`, whose first word the reader leaves open) asked
//! as a closed choice of its readings completes that entry in place, the stated grants kept.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::surface::literal_projection;
use nika_compile::{
    CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, QuestionType, compile,
    intent_sha256,
};
use serde_json::{Value, json};

mod common;
use common::{approved_round, held_for_its_judge};

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
    compile(&answer_round((INTENT, INTENT), source, questions, answers)).unwrap()
}

/// The answer round's request: the `intent` asked again with the native record written for
/// `recorded`, its `source` and `questions`, and these answers.
fn answer_round(
    (intent, recorded): (&str, &str),
    source: &str,
    questions: &Value,
    answers: &[(&str, &str)],
) -> CompileRequest {
    let record = json!({
        "strategy": "native",
        "intent_sha256": intent_sha256(recorded),
        "source": source,
        "questions": questions,
        "gaps": [],
        "trigger": null,
    });
    let mut request = CompileRequest::create(intent).with_plan(record);
    for (key, literal) in answers {
        request = request.answer(*key, *literal);
    }
    request
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
        assert!(held_for_its_judge(&out, INTENT), "{path}: {out:#?}");
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
        assert!(!held_for_its_judge(&out, INTENT), "{path}: {out:#?}");
        assert_eq!(
            fs(&out, "write"),
            json!([""]),
            "{path}: the placeholder stays"
        );
        assert!(!granted(&out, "write"), "{path}");
    }
}

/// A side with no empty entry, or with two, is never touched: the answered path stays outside what
/// the seat declared. One empty entry beside a stated grant is completed in place, the stated
/// grant kept as it is.
#[test]
fn an_explicit_boundary_or_two_placeholders_is_never_widened() {
    let answer = [("const.destination_path", r#""sortie.txt""#)];
    let mixed = answered(
        &copy_to(r#"["./out/**", ""]"#),
        &asked(&["const.destination_path"]),
        &answer,
    );
    assert!(held_for_its_judge(&mixed, INTENT), "{mixed:#?}");
    assert_eq!(fs(&mixed, "write"), json!(["./out/**", "sortie.txt"]));
    assert!(granted(&mixed, "write") && !granted(&mixed, "read"));
    for write in [r#"["./out/**"]"#, r#"["", ""]"#] {
        let out = answered(
            &copy_to(write),
            &asked(&["const.destination_path"]),
            &answer,
        );
        let declared: Value = serde_json::from_str(write).unwrap();
        assert_eq!(fs(&out, "write"), declared, "{write}");
        assert!(!granted(&out, "write"), "{write}");
        assert_ne!(
            out.status,
            CompileStatus::Ready,
            "{write}: sortie.txt stays outside what the seat declared"
        );
        assert!(!held_for_its_judge(&out, INTENT), "{write}: {out:#?}");
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
    assert!(held_for_its_judge(&out, INTENT), "{out:#?}");
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
    assert!(held_for_its_judge(&out, INTENT), "{out:#?}");
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
/// leaves every other byte alone; beside a stated entry, that entry is kept in its place. A
/// block with no empty entry, or two, is kept byte for byte and never widened.
#[test]
fn a_block_placeholder_completes_the_same_and_any_other_block_is_kept() {
    let flow = copy_to(r#"[""]"#);
    let placeholder = "    write:\n      - \"\"\n";
    let block = flow.replace("    write: [\"\"]\n", placeholder);
    assert!(block.contains(placeholder), "{block}");
    let answer = [("const.destination_path", r#""sortie.txt""#)];
    let questions = asked(&["const.destination_path"]);
    let out = answered(&block, &questions, &answer);
    assert!(held_for_its_judge(&out, INTENT), "{out:#?}");
    assert_eq!(fs(&out, "write"), json!(["sortie.txt"]));
    assert!(granted(&out, "write") && !granted(&out, "read"));
    let expected = block
        .replace(placeholder, "    write:\n      [\"sortie.txt\"]\n")
        .replace(
            "  destination_path: \"\"\n",
            "  destination_path: \"sortie.txt\"\n",
        );
    assert_eq!(out.candidate.as_deref(), Some(expected.as_str()));
    let beside = "    write:\n      - \"./out/**\"\n      - \"\"\n";
    let out = answered(
        &flow.replace("    write: [\"\"]\n", beside),
        &questions,
        &answer,
    );
    assert!(held_for_its_judge(&out, INTENT), "{out:#?}");
    let completed = "    write:\n      [\"./out/**\",\"sortie.txt\"]\n";
    let expected = expected.replace("    write:\n      [\"sortie.txt\"]\n", completed);
    assert_eq!(out.candidate.as_deref(), Some(expected.as_str()));

    for entries in ["      - \"\"\n      - \"\"\n", "      - \"./out/**\"\n"] {
        let kept = flow.replace("    write: [\"\"]\n", &format!("    write:\n{entries}"));
        let out = answered(&kept, &questions, &answer);
        assert!(!granted(&out, "write"), "{entries}");
        assert_ne!(out.status, CompileStatus::Ready, "{entries}");
        assert!(!held_for_its_judge(&out, INTENT), "{entries}");
        let source = out.candidate.as_deref().unwrap_or_default();
        assert!(
            source.contains(entries),
            "the seat's block stays as written: {source}"
        );
    }
}

/// The block may also be indentless (`- ""` at its key's own column, the shape `copy-fr.json`
/// uses): it completes the same, its flow form written two columns deeper so it stays the
/// key's value, a stated entry beside it kept in its place; an indentless block with two empty
/// entries is kept byte for byte.
#[test]
fn an_indentless_block_placeholder_completes_the_same() {
    let flow = copy_to(r#"[""]"#);
    let placeholder = "    write:\n    - \"\"\n";
    let block = flow.replace("    write: [\"\"]\n", placeholder);
    assert!(block.contains(placeholder), "{block}");
    let answer = [("const.destination_path", r#""sortie.txt""#)];
    let questions = asked(&["const.destination_path"]);
    let out = answered(&block, &questions, &answer);
    assert!(held_for_its_judge(&out, INTENT), "{out:#?}");
    assert_eq!(fs(&out, "write"), json!(["sortie.txt"]), "{out:#?}");
    assert!(granted(&out, "write") && !granted(&out, "read"));
    let expected = block
        .replace(placeholder, "    write:\n      [\"sortie.txt\"]\n")
        .replace(
            "  destination_path: \"\"\n",
            "  destination_path: \"sortie.txt\"\n",
        );
    assert_eq!(out.candidate.as_deref(), Some(expected.as_str()));
    let beside = "    write:\n    - \"./out/**\"\n    - \"\"\n";
    let out = answered(
        &flow.replace("    write: [\"\"]\n", beside),
        &questions,
        &answer,
    );
    assert!(held_for_its_judge(&out, INTENT), "{out:#?}");
    let completed = "    write:\n      [\"./out/**\",\"sortie.txt\"]\n";
    let expected = expected.replace("    write:\n      [\"sortie.txt\"]\n", completed);
    assert_eq!(out.candidate.as_deref(), Some(expected.as_str()));
    let entries = "    write:\n    - \"\"\n    - \"\"\n";
    let kept = flow.replace("    write: [\"\"]\n", entries);
    let out = answered(&kept, &questions, &answer);
    assert!(!granted(&out, "write"));
    assert_ne!(out.status, CompileStatus::Ready);
    let source = out.candidate.as_deref().unwrap_or_default();
    assert!(source.contains(entries), "kept as written: {source}");
}

/// The stock request's words around its payload, as its sentence states them: the
/// report it names exactly, then `un payload out/notification.json`, whose first word the reader
/// leaves open, and the route it posts to.
const STOCK: &str = "Écris out/report.json avec les alertes. Une fois la source complète, si des alertes certaines existent, prépare exactement un payload out/notification.json contenant channel=\"stock\" et item_ids dans leur ordre, puis effectue exactement un POST /notifications/stock vers le sink local fourni par le futur pilote.";

/// The open name, owed exactly unless the human settles it.
const PAYLOAD: &str = "payload out/notification.json";

/// The seat's stock candidate: the report written where the request names it, the payload where
/// the asked `const.payload_path` says, one empty write entry beside the report's grant.
fn payload_to(write: &str) -> String {
    format!(
        r#"nika: stock-notification
const:
  payload_path: ""
permits:
  fs:
    write: {write}
  tools: ["nika:write"]
tasks:
  write_report:
    invoke:
      tool: "nika:write"
      args: {{ path: "out/report.json", content: "{{}}" }}
  write_payload:
    invoke:
      tool: "nika:write"
      args: {{ path: "${{{{ const.payload_path }}}}", content: "{{}}" }}
"#
    )
}

/// The questions `asked` (`key`, label) as the native judge records them over `source` for
/// `intent`: a question naming an open name a task's whole path asks is the closed choice of that
/// name's readings (`fidelity::asked_readings`); any other stays text.
fn recorded(intent: &str, source: &str, asked: &[(&str, &str)]) -> Value {
    let doc = literal_projection(source).unwrap();
    let said: Vec<Value> = (asked.iter())
        .map(|(key, label)| json!({"key": key, "label": label, "why": "The request leaves it open."}))
        .collect();
    let readings =
        |key: &str| nika_compile_fidelity::fidelity::asked_readings(intent, Some(&doc), &said, key);
    (said.iter())
        .map(|question| {
            let options = readings(question["key"].as_str().unwrap());
            let shape = if options.is_empty() { "text" } else { "choice" };
            let mut question = question.clone();
            question["answer_type"] = json!(shape);
            question["options"] = json!(options);
            question
        })
        .collect()
}

/// The keys a recorded question offers, in their order.
fn offered(question: &Value) -> Vec<&str> {
    (question["options"].as_array().into_iter().flatten())
        .filter_map(|option| option["key"].as_str())
        .collect()
}

/// An open name asked as a closed choice of its readings: unanswered, it stays asked and nothing is
/// emitted or granted; answered with either reading, the same recorded candidate is replayed with
/// its constant baked and its one empty write entry completed in place, the report's grant kept,
/// every other byte as the seat wrote it, held for its round's judge; an approving judge then
/// settles it READY. A genuine spaced name stays whole.
#[tokio::test]
async fn an_open_name_answered_among_its_readings_completes_its_entry_in_place() {
    let source = payload_to(r#"["out/report.json", ""]"#);
    let label = format!("Quel fichier est « {PAYLOAD} » ?");
    let questions = recorded(STOCK, &source, &[("const.payload_path", &label)]);
    assert_eq!(questions[0]["answer_type"], "choice");
    assert_eq!(offered(&questions[0]), [PAYLOAD, "out/notification.json"]);
    let open = compile(&answer_round((STOCK, STOCK), &source, &questions, &[])).unwrap();
    assert_eq!(open.status, CompileStatus::Incomplete, "{open:#?}");
    assert!(open.candidate.is_none(), "no candidate while it is asked");
    let asked = (open.questions.iter()).find(|q| q.key == "const.payload_path");
    let asked = asked.expect("the open name is asked");
    assert!(asked.mandatory && asked.answer_type == QuestionType::Choice);
    let keys: Vec<&str> = asked.options.iter().map(|o| o.key.as_str()).collect();
    assert_eq!(keys, [PAYLOAD, "out/notification.json"]);
    for reading in ["out/notification.json", PAYLOAD] {
        let literal = json!(reading).to_string();
        let answers = [("const.payload_path", literal.as_str())];
        let request = answer_round((STOCK, STOCK), &source, &questions, &answers);
        let out = compile(&request).unwrap();
        assert!(held_for_its_judge(&out, STOCK), "{reading}: {out:#?}");
        let expected = source
            .replace(
                "  payload_path: \"\"\n",
                &format!("  payload_path: {literal}\n"),
            )
            .replace(
                "    write: [\"out/report.json\", \"\"]\n",
                &format!("    write: {}\n", json!(["out/report.json", reading])),
            );
        assert_eq!(
            out.candidate.as_deref(),
            Some(expected.as_str()),
            "{reading}"
        );
        assert!(
            granted(&out, "write") && !granted(&out, "read"),
            "{reading}"
        );
        let judged = approved_round(&request).await;
        assert_eq!(
            judged.status,
            CompileStatus::Ready,
            "{reading}: {judged:#?}"
        );
        assert_eq!(
            judged.candidate, out.candidate,
            "{reading}: the same bytes, judged"
        );
    }
}

/// An answer off the readings never mutates the candidate: the question stays, nothing is baked or
/// granted. The offers bind by key, in any order. An answer for another revision of the request
/// (a record written for other words, or the readings of another open name) binds nothing.
#[test]
fn an_answer_off_its_readings_or_for_another_revision_never_touches_the_candidate() {
    let source = payload_to(r#"["out/report.json", ""]"#);
    let label = format!("Quel fichier est « {PAYLOAD} » ?");
    let mut questions = recorded(STOCK, &source, &[("const.payload_path", &label)]);
    for answer in [
        "notification.json",
        "out/other.json",
        "./out/notification.json",
    ] {
        let literal = json!(answer).to_string();
        let answers = [("const.payload_path", literal.as_str())];
        let out = compile(&answer_round((STOCK, STOCK), &source, &questions, &answers)).unwrap();
        assert_eq!(out.status, CompileStatus::Incomplete, "{answer}: {out:#?}");
        assert!(
            out.candidate.is_none() && !granted(&out, "write"),
            "{answer}"
        );
        assert!(
            (out.questions.iter()).any(|q| q.key == "const.payload_path" && q.mandatory),
            "{answer}: asked again"
        );
        let refused = |d: &nika_compile::CompileDiagnostic| {
            d.kind == DiagnosticKind::Missed
                && d.target == "const.payload_path"
                && d.message.starts_with("Answer one of the offered keys")
        };
        assert!(out.diagnostics.iter().any(refused), "{answer}: {out:#?}");
    }
    let answers = [("const.payload_path", r#""out/notification.json""#)];
    let first = compile(&answer_round((STOCK, STOCK), &source, &questions, &answers)).unwrap();
    questions[0]["options"].as_array_mut().unwrap().reverse();
    let reordered = compile(&answer_round((STOCK, STOCK), &source, &questions, &answers)).unwrap();
    assert!(held_for_its_judge(&reordered, STOCK), "{reordered:#?}");
    assert_eq!(reordered.candidate, first.candidate);
    // The record was written for the stock words; the request asked now says another thing.
    let other = STOCK.replace(
        "un payload out/notification.json",
        "un message out/alerte.json",
    );
    let stale = compile(&answer_round(
        (&other, STOCK),
        &source,
        &questions,
        &answers,
    ))
    .unwrap();
    assert!(
        stale.candidate.is_none() && !granted(&stale, "write"),
        "{stale:#?}"
    );
    assert!(
        stale
            .diagnostics
            .iter()
            .any(|d| d.target == "recorded_plan")
    );
    // The revision of those words asks the readings of its own open name: the old answer is none.
    let revised = recorded(
        &other,
        &source,
        &[("const.payload_path", "« message out/alerte.json »")],
    );
    assert_eq!(
        offered(&revised[0]),
        ["message out/alerte.json", "out/alerte.json"]
    );
    let out = compile(&answer_round((&other, &other), &source, &revised, &answers)).unwrap();
    assert!(
        out.candidate.is_none() && !granted(&out, "write"),
        "{out:#?}"
    );
}

/// The stock words with a second open name, a source: `un journal in/a.json`.
const TWO: &str = "Lis un journal in/a.json. Écris out/report.json, puis prépare exactement un payload out/c.json.";

/// A candidate reading the asked `const.source_path` and writing the report and the asked
/// `const.payload_path`, each side with its one empty entry (the write side beside the report).
const TWO_SIDES: &str = r#"nika: journal-payload
const:
  source_path: ""
  payload_path: ""
permits:
  fs:
    read: [""]
    write: ["out/report.json", ""]
  tools: ["nika:read", "nika:write"]
tasks:
  load:
    invoke:
      tool: "nika:read"
      args: { path: "${{ const.source_path }}" }
  write_report:
    with: { text: "${{ tasks.load.output }}" }
    invoke:
      tool: "nika:write"
      args: { path: "out/report.json", content: "${{ with.text }}" }
  write_payload:
    with: { text: "${{ tasks.load.output }}" }
    invoke:
      tool: "nika:write"
      args: { path: "${{ const.payload_path }}", content: "${{ with.text }}" }
"#;

/// Two open names, each asked by the question naming it: each offers its own readings whatever
/// the questions' order; one answer leaves the other asked, nothing emitted or granted; both, in
/// either order, complete each side exactly, the report's grant kept.
#[test]
fn two_open_names_answered_out_of_order_complete_each_side_exactly() {
    let asked = [
        (
            "const.payload_path",
            "Quel fichier est « payload out/c.json » ?",
        ),
        (
            "const.source_path",
            "Quel fichier est « journal in/a.json » ?",
        ),
    ];
    let questions = recorded(TWO, TWO_SIDES, &asked);
    assert_eq!(offered(&questions[0]), ["payload out/c.json", "out/c.json"]);
    assert_eq!(offered(&questions[1]), ["journal in/a.json", "in/a.json"]);
    let swapped = recorded(TWO, TWO_SIDES, &[asked[1], asked[0]]);
    assert_eq!(swapped[0], questions[1]);
    assert_eq!(swapped[1], questions[0]);
    let payload = ("const.payload_path", r#""out/c.json""#);
    let journal = ("const.source_path", r#""in/a.json""#);
    let partial = compile(&answer_round((TWO, TWO), TWO_SIDES, &questions, &[payload])).unwrap();
    assert!(partial.candidate.is_none(), "{partial:#?}");
    let still: Vec<&str> = (partial.questions.iter())
        .filter(|q| q.mandatory)
        .map(|q| q.key.as_str())
        .collect();
    assert_eq!(still, ["const.source_path"]);
    assert!(!granted(&partial, "read") && !granted(&partial, "write"));
    let one = compile(&answer_round(
        (TWO, TWO),
        TWO_SIDES,
        &questions,
        &[journal, payload],
    ));
    let two = compile(&answer_round(
        (TWO, TWO),
        TWO_SIDES,
        &swapped,
        &[payload, journal],
    ));
    let (one, two) = (one.unwrap(), two.unwrap());
    assert!(held_for_its_judge(&one, TWO), "{one:#?}");
    assert_eq!(one.candidate, two.candidate);
    assert_eq!(fs(&one, "read"), json!(["in/a.json"]));
    assert_eq!(fs(&one, "write"), json!(["out/report.json", "out/c.json"]));
}

/// An answer that is no asked path widens nothing: content stays content, and an answer no
/// question owns is never applied; the grant is exactly the asked path beside the stated one.
#[test]
fn an_unrelated_answer_never_widens_the_boundary() {
    let source = payload_to(r#"["out/report.json", ""]"#)
        .replace(
            "  payload_path: \"\"\n",
            "  payload_path: \"\"\n  note: \"\"\n",
        )
        .replace(
            "path: \"out/report.json\", content: \"{}\"",
            "path: \"out/report.json\", content: \"${{ const.note }}\"",
        );
    assert!(
        source.contains("content: \"${{ const.note }}\""),
        "{source}"
    );
    let label = format!("Quel fichier est « {PAYLOAD} » ?");
    let asked = [
        ("const.payload_path", label.as_str()),
        ("const.note", "Quel texte ?"),
    ];
    let questions = recorded(STOCK, &source, &asked);
    assert_eq!(
        questions[1]["answer_type"], "text",
        "content is no open name's"
    );
    let answers = [
        ("const.payload_path", r#""out/notification.json""#),
        ("const.note", r#""./secret.txt""#),
        ("const.unasked", r#""./other.txt""#),
    ];
    let out = compile(&answer_round((STOCK, STOCK), &source, &questions, &answers)).unwrap();
    let candidate = out.candidate.as_deref().expect("a candidate");
    assert_eq!(
        fs(&out, "write"),
        json!(["out/report.json", "out/notification.json"])
    );
    assert_eq!(fs(&out, "read"), Value::Null);
    assert!(
        candidate.contains("./secret.txt"),
        "the content stays content"
    );
    assert!(
        !candidate.contains("other.txt"),
        "an unowned answer is never applied"
    );
}
