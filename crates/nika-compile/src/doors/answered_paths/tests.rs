// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Pure materialization and path projection; no host, files, process, or provider.
use super::*;
use crate::CompileStatus;
use serde_json::json;

const INTENT: &str = "Write the text I choose to the file I choose.";
const WRITE: &str = r#"nika: answered-note
const:
  place: ""
  note: ""
permits:
  tools: ["nika:write"]
  fs:
    write: [""]
tasks:
  save:
    invoke:
      tool: "nika:write"
      args:
        path: "${{ const.place }}"
        content: "${{ const.note }}"
"#;

fn question(key: &str) -> Value {
    json!({"key": key, "label": "Choose a value", "answer_type": "text", "why": "The human chooses it."})
}

fn record(source: &str, keys: &[&str]) -> Value {
    json!({
        "strategy": "native", "intent_sha256": crate::intent_sha256(INTENT),
        "source": source, "questions": keys.iter().map(|key| question(key)).collect::<Vec<_>>(),
        "gaps": [], "trigger": null,
    })
}

fn request() -> CompileRequest {
    CompileRequest::create(INTENT)
        .answer("const.place", "\"chosen.txt\"")
        .answer("const.note", "\"./secret.txt\"")
}

fn applied(record: &Value, request: &CompileRequest) -> String {
    let mut out = crate::initial();
    super::super::native_apply(record, request, &mut out);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    out.candidate
        .expect("the pure native application emits a candidate")
}

#[test]
fn an_answered_write_is_not_a_read_and_content_is_not_authority() {
    let record = record(WRITE, &["const.place", "const.note"]);
    let request = request().answer("const.unasked", "\"./another-secret.txt\"");
    let candidate = applied(&record, &request);
    let paths = native_answered_paths(&record, &request, &candidate).expect("admitted answers");
    assert!(paths.reads().is_empty());
    assert_eq!(paths.writes(), ["chosen.txt"]);
    assert!(
        candidate.contains("./secret.txt"),
        "the content remains content"
    );
}

#[test]
fn an_answered_read_keeps_its_direction() {
    let source = r#"nika: chosen-source
const:
  material: ""
permits:
  tools: ["nika:read"]
  fs:
    read: [""]
tasks:
  read:
    invoke:
      tool: "nika:read"
      args: { path: "${{ const.material }}" }
"#;
    let record = record(source, &["const.material"]);
    let request = CompileRequest::create(INTENT).answer("const.material", "\"in/source.txt\"");
    let candidate = applied(&record, &request);
    let paths = native_answered_paths(&record, &request, &candidate).expect("admitted read");
    assert_eq!(paths.reads(), ["in/source.txt"]);
    assert!(paths.writes().is_empty());
}

#[test]
fn another_answer_candidate_or_request_does_not_reuse_paths() {
    let record = record(WRITE, &["const.place", "const.note"]);
    let request = request();
    let first = applied(&record, &request);
    let changed = request.clone().answer("const.place", "\"new.txt\"");
    assert!(native_answered_paths(&record, &changed, &first).is_none());
    let second = applied(&record, &changed);
    assert_eq!(
        native_answered_paths(&record, &changed, &second)
            .unwrap()
            .writes(),
        ["new.txt"]
    );
    assert!(native_answered_paths(&record, &request, &(first + "# drift\n")).is_none());
    let replaced = request.with_replaced_input("Read another request.");
    assert!(native_answered_paths(&record, &replaced, &second).is_none());
}

#[test]
fn forged_metadata_is_ignored_and_an_unbound_record_is_rejected() {
    let mut record = record(WRITE, &["const.place", "const.note"]);
    let request = request();
    let candidate = applied(&record, &request);
    record["answered_paths"] = json!({"reads": ["./secret.txt"], "writes": ["elsewhere.txt"]});
    record["decision"] = json!({"rehearsal": {"passed": true, "candidate": candidate}});
    let paths = native_answered_paths(&record, &request, &candidate).unwrap();
    assert!(paths.reads().is_empty());
    assert_eq!(paths.writes(), ["chosen.txt"]);
    record["intent_sha256"] = json!(crate::intent_sha256("another request"));
    assert!(native_answered_paths(&record, &request, &candidate).is_none());
}

#[test]
fn a_literal_candidate_read_does_not_become_an_answered_path() {
    let source = r#"nika: source-only
permits:
  tools: ["nika:read"]
  fs:
    read: ["./private.txt"]
tasks:
  read:
    invoke:
      tool: "nika:read"
      args: { path: "./private.txt" }
"#;
    let record = record(source, &[]);
    let request = request();
    let candidate = applied(&record, &request);
    let paths = native_answered_paths(&record, &request, &candidate).unwrap();
    assert!(paths.reads().is_empty() && paths.writes().is_empty());
}

#[test]
fn an_unanswered_or_unoffered_value_cannot_supply_paths() {
    let mut record = record(WRITE, &["const.place", "const.note"]);
    let request = request();
    let candidate = applied(&record, &request);
    let mut missing = request.clone();
    missing.answers.remove("const.place");
    assert!(native_answered_paths(&record, &missing, &candidate).is_none());
    record["questions"][0]["answer_type"] = json!("choice");
    record["questions"][0]["options"] = json!([{"key": "other.txt", "label": "Other"}]);
    assert!(native_answered_paths(&record, &request, &candidate).is_none());
}

#[test]
fn unsafe_or_composed_paths_do_not_acquire_a_projection() {
    let record = record(WRITE, &["const.place", "const.note"]);
    for answer in [
        "../secret.txt",
        "/tmp/secret.txt",
        "~/secret.txt",
        "out/*.txt",
    ] {
        let request = request().answer("const.place", json!(answer).to_string());
        let mut out = crate::initial();
        super::super::native_apply(&record, &request, &mut out);
        assert!(
            native_answered_paths(
                &record,
                &request,
                out.candidate.as_deref().unwrap_or_default()
            )
            .is_none()
        );
    }
    let source = WRITE.replace("${{ const.place }}", "./out/${{ const.place }}.txt");
    let composed = record_for_composed(&source);
    let mut out = crate::initial();
    super::super::native_apply(&composed, &request(), &mut out);
    assert!(
        native_answered_paths(
            &composed,
            &request(),
            out.candidate.as_deref().unwrap_or_default()
        )
        .is_none_or(|paths| paths.reads().is_empty() && paths.writes().is_empty())
    );
}

fn record_for_composed(source: &str) -> Value {
    record(source, &["const.place", "const.note"])
}

#[test]
fn an_edit_or_replacement_never_inherits_the_previous_answer_record() {
    let record = record(WRITE, &["const.place", "const.note"]);
    let request = request();
    let candidate = applied(&record, &request);
    let edit =
        CompileRequest::edit(candidate.clone(), "Change the text.").with_original_intent(INTENT);
    assert!(native_answered_paths(&record, &edit, &candidate).is_none());
    let replaced = request.answer(
        "intent.clarification",
        "\"Write a new request to fresh.txt.\"",
    );
    assert!(native_answered_paths(&record, &replaced, &candidate).is_none());
}

#[test]
fn dynamic_paths_without_answered_questions_keep_an_empty_projection() {
    let source = r#"nika: runtime-source
inputs:
  path:
    type: string
    default: "./in/source.txt"
permits:
  tools: ["nika:read"]
  fs:
    read: ["./in/**"]
tasks:
  read:
    invoke:
      tool: "nika:read"
      args: { path: "${{ inputs.path }}" }
"#;
    let inferred = nika_check::infer_permits(&crate::parse(source).unwrap());
    assert!(inferred.partial.fs, "this control must remain dynamic");
    let record = record(source, &[]);
    let request = CompileRequest::create(INTENT);
    let candidate = applied(&record, &request);
    let paths = native_answered_paths(&record, &request, &candidate).unwrap();
    assert!(paths.reads().is_empty() && paths.writes().is_empty());
}
