// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a run completed writing: a `nika:write` that a start frame opened and a successful
//! completion settled, its own write permit and its returned path naming the same file. A
//! permit alone, a completion without a start, an unsuccessful outcome, a recovered, replayed or
//! skipped task, another tool or another path names nothing. The fixtures carry the frames as a
//! real run journals them: the returned value as JSON, the outcome as a JSON object.

use super::*;
use serde_json::json;

/// The three frames a real run journaled for its write (`task_started`, the write permit,
/// `task_completed`), byte for byte.
const REAL: &str = r#"{"chain":"0e1f3a5823d875c9706810d27eba7858e25d3b3f58ed581a490e75d349ae12ed","correlation":null,"execution":{"uuid":"01a10897-52a2-7758-aa31-024a0098d5b1"},"fields":[{"key":"task","value":"write_reorder"},{"key":"note","value":"invoke · nika:write"}],"id":{"uuid":"01a10897-5438-7304-9f95-a4d920f70978"},"kind":"task_started","run":null,"timestamp":1791145497656000000}
{"chain":"02912d89f858a3efa616f25e2e65a38c283a339c5d0b5e5b4d32e90f4c666c40","correlation":null,"execution":{"uuid":"01a10897-52a2-7758-aa31-024a0098d5b1"},"fields":[{"key":"task","value":"write_reorder"},{"key":"plane","value":"fs"},{"key":"gate","value":"permits.fs.write ./reorder.json"},{"key":"decision","value":"allow"},{"key":"why","value":"the effective identity stays inside the declared set"}],"id":{"uuid":"01a10897-5439-76f8-ad7e-340a356abafd"},"kind":"permit_checked","run":null,"timestamp":1791145497657000000}
{"chain":"0dfa67a2a936d492c412a10d4eea5d566a054b990c68925e99ddd8d90e712512","correlation":null,"execution":{"uuid":"01a10897-52a2-7758-aa31-024a0098d5b1"},"fields":[{"key":"task","value":"write_reorder"},{"key":"note","value":"invoke · nika:write"},{"key":"duration_ms","value":0},{"key":"def_hash","value":"b5430e038474e32d654048016379abb1568c691e4520fb2732a9a8ac01f07c3c"},{"key":"input_hash","value":"d7682e9309f09ae1427030e6453d2bba46ff3b7c1474fec53d14b190a440b96a"},{"key":"output","value":"\"./reorder.json\""},{"key":"preview_digest","value":"a48b1e0a0c69f93c4d7010ca54bc605788d7172c52acea59b56d916357c699fc"},{"key":"commit_digest","value":"a48b1e0a0c69f93c4d7010ca54bc605788d7172c52acea59b56d916357c699fc"},{"key":"outcome","value":"{\"cause\":\"normal\",\"class\":\"success\",\"payload\":{\"attempts\":1,\"value\":\"./reorder.json\"}}"}],"id":{"uuid":"01a10897-5439-76f8-ad7e-340bfb1a224a"},"kind":"task_completed","run":null,"timestamp":1791145497657000000}
"#;

const WRITE: &str = "invoke · nika:write";
const GRANT: &str = "permits.fs.write ./reorder.json";
const PATH: &str = "./reorder.json";

fn frame(kind: &str, fields: &[(&str, Value)]) -> String {
    let fields: Vec<Value> = (fields.iter())
        .map(|(key, value)| json!({ "key": key, "value": value }))
        .collect();
    let execution = json!({ "uuid": "01a10897-52a2-7758-aa31-024a0098d5b1" });
    json!({ "execution": execution, "fields": fields, "kind": kind }).to_string()
}

/// A completion as a run journals it: the returned value as JSON, the outcome as an object.
fn completed(task: &str, class: &str, returned: &str) -> String {
    let outcome = json!({ "cause": "normal", "class": class, "payload": { "value": returned } });
    frame(
        "task_completed",
        &[
            ("task", json!(task)),
            ("note", json!(WRITE)),
            ("output", json!(json!(returned).to_string())),
            ("outcome", json!(outcome.to_string())),
        ],
    )
}

fn started(task: &str, note: &str) -> String {
    frame(
        "task_started",
        &[("task", json!(task)), ("note", json!(note))],
    )
}

fn permit(task: &str, gate: &str) -> String {
    let fields = [
        ("task", json!(task)),
        ("plane", json!("fs")),
        ("gate", json!(gate)),
    ];
    let mut fields = fields.to_vec();
    fields.push(("decision", json!("allow")));
    frame("permit_checked", &fields)
}

fn writes(frames: &[String]) -> Vec<String> {
    let raw = frames.join("\n") + "\n";
    RunFacts::of(Path::new("t.ndjson"), &raw)
        .expect("frames")
        .completed_writes()
}

#[test]
fn the_real_journal_of_a_write_names_its_path() {
    let facts = RunFacts::of(Path::new("t.ndjson"), REAL).expect("frames");
    assert_eq!(facts.completed_writes(), [PATH]);
}

#[test]
fn a_started_granted_and_successful_write_is_named_with_or_without_dot_slash() {
    let ran = [
        started("w", WRITE),
        permit("w", GRANT),
        completed("w", "success", PATH),
    ];
    assert_eq!(writes(&ran), [PATH]);
    let bare = [
        started("w", WRITE),
        permit("w", GRANT),
        completed("w", "success", "reorder.json"),
    ];
    assert_eq!(
        writes(&bare),
        ["reorder.json"],
        "a leading ./ is the same path"
    );
}

#[test]
fn anything_short_of_a_started_granted_successful_write_names_nothing() {
    let settled = |end: &str| frame(end, &[("task", json!("w"))]);
    let raw_output = frame(
        "task_completed",
        &[
            ("task", json!("w")),
            ("output", json!(PATH)),
            ("outcome", json!("{\"class\":\"success\"}")),
        ],
    );
    let cases: [(&str, Vec<String>); 10] = [
        (
            "a permit alone",
            vec![
                started("w", WRITE),
                permit("w", GRANT),
                settled("task_failed"),
            ],
        ),
        (
            "no start frame",
            vec![permit("w", GRANT), completed("w", "success", PATH)],
        ),
        (
            "an outcome not success",
            vec![
                started("w", WRITE),
                permit("w", GRANT),
                completed("w", "failure", PATH),
            ],
        ),
        (
            "an output that is not JSON",
            vec![started("w", WRITE), permit("w", GRANT), raw_output],
        ),
        (
            "recovered",
            vec![
                started("w", WRITE),
                permit("w", GRANT),
                completed("w", "success", PATH),
                settled("task_recovered"),
            ],
        ),
        (
            "replayed",
            vec![permit("w", GRANT), settled("task_cache_hit")],
        ),
        (
            "skipped",
            vec![
                started("w", WRITE),
                permit("w", GRANT),
                settled("task_skipped"),
            ],
        ),
        (
            "another tool",
            vec![
                started("w", "invoke · nika:read"),
                permit("w", GRANT),
                completed("w", "success", PATH),
            ],
        ),
        (
            "another path",
            vec![
                started("w", WRITE),
                permit("w", GRANT),
                completed("w", "success", "./other.json"),
            ],
        ),
        (
            "a read permit",
            vec![
                started("w", WRITE),
                permit("w", "permits.fs.read ./reorder.json"),
                completed("w", "success", PATH),
            ],
        ),
    ];
    for (case, frames) in cases {
        assert!(writes(&frames).is_empty(), "{case}");
    }
}

#[test]
fn a_permit_answers_only_for_its_own_task() {
    let frames = [
        started("other", WRITE),
        permit("other", GRANT),
        started("w", WRITE),
        completed("w", "success", PATH),
    ];
    assert!(writes(&frames).is_empty());
}
