// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The artifact door's pure parts: how a request names an artifact, which journal events
//! record one, which verifier document holds, and the verified stream's two checks.

use std::io::Write as _;

use http_body_util::BodyExt as _;
use nika_event::{Event, EventKind};
use nika_types::id::EventId;
use nika_types::resource::{KeyValue, Value as FieldValue};
use nika_types::timestamp::Timestamp;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use super::journal::sealed_tier;
use super::manifest::{Artifact, Kind, contained, derive};
use super::stream::{VerifiedBody, verify_whole};
use super::{MAX_LISTED, ManifestBody, Verified, decode_name, digest_bytes, route};

const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const BLAKE: &str = "647b64117d6ccccd96618d832c51249886c69f0f403106a1dfc9ca35d4bb3dd3";

fn event(kind: EventKind, serial: u128, fields: &[(&str, String)]) -> Event {
    let fields = fields
        .iter()
        .map(|(key, value)| KeyValue::new(*key, FieldValue::string(value.clone())))
        .collect();
    Event::new(
        EventId::new(uuid::Uuid::from_u128(serial)),
        Timestamp::from_unix_ms(0),
        kind,
    )
    .with_fields(fields)
}

/// A builtin's own successful `task_completed` frame carrying `value`.
fn completed(serial: u128, task: &str, tool: &str, value: &Value) -> Event {
    let outcome =
        json!({"class": "success", "cause": "normal", "payload": {"attempts": 1, "value": value}});
    event(
        EventKind::TaskCompleted,
        serial,
        &[
            ("task", task.to_owned()),
            ("note", format!("invoke · {tool}")),
            ("outcome", outcome.to_string()),
        ],
    )
}

fn observed(serial: u128, observation: &Value) -> Event {
    event(
        EventKind::AgentImageObserved,
        serial,
        &[
            ("task", "draw".to_owned()),
            ("harness_image", observation.to_string()),
        ],
    )
}

fn stored_image(sha256: &str, size: u64) -> Value {
    json!({
        "schema": "nika/harness-image-observation@1", "source": "harness_reported",
        "tool_call_id": "call-1", "mime_type": "image/png", "received_bytes": size,
        "blob": {"hash": format!("blake3:{BLAKE}"), "mime_type": "image/png", "size": size},
        "storage": "stored", "storage_failure": null, "received_sha256": sha256,
        "reported_saved_path": "/elsewhere/codex/image.png", "file_verified": false,
        "permission_evidence": "separate permit_checked frames"
    })
}

fn names(artifacts: &[Artifact]) -> Vec<&str> {
    artifacts
        .iter()
        .map(|artifact| artifact.name.as_str())
        .collect()
}

/// The rest of the path is decoded once: a literal `/` and `%2F` name the same entry (an
/// `OpenAPI` client sends the second for one path parameter). Only a bad escape, bytes that are
/// not UTF-8, NUL or nothing refuse; any other decoded value is a key that must match exactly.
#[test]
fn a_request_names_an_artifact_by_decoding_its_path_once() {
    for (raw, name) in [
        ("file/out/a.png", "file/out/a.png"),
        ("file%2Fout%2Fa.png", "file/out/a.png"),
        ("file/out%2fa.png", "file/out/a.png"),
        ("file/out/Rapport%20final.pdf", "file/out/Rapport final.pdf"),
        ("file/caf%C3%A9.txt", "file/café.txt"),
        ("file/%4a.txt", "file/J.txt"),
        // Decoded, then matched against names that never hold these forms.
        ("file/%2e%2e%2fsecret", "file/../secret"),
        ("file//a.png", "file//a.png"),
        ("file/%252F", "file/%2F"),
    ] {
        assert_eq!(decode_name(raw).as_deref(), Some(name), "{raw}");
    }
    for refused in [
        "file/%zz", "file/%4", "file/%+f", "file/%00", "", "file/%C3", "%",
    ] {
        assert_eq!(decode_name(refused), None, "{refused}");
    }
}

#[test]
fn only_the_two_artifact_routes_are_recognized() {
    assert_eq!(route("/v1/jobs/j1/artifacts"), Some(("j1", None)));
    assert_eq!(
        route("/v1/jobs/j1/artifacts/file/out/a.png"),
        Some(("j1", Some("file/out/a.png")))
    );
    assert_eq!(route("/v1/jobs/j1/artifacts/"), Some(("j1", Some(""))));
    for other in [
        "/v1/jobs/j1/artifactsX",
        "/v1/jobs//artifacts",
        "/v1/jobs/j1",
        "/v1/jobs/j1/events",
        "/v1/jobs/j1/trace/verify",
        "/v1/workflows/artifacts",
    ] {
        assert_eq!(route(other), None, "{other}");
    }
}

#[test]
fn a_digest_is_exactly_sixty_four_hex_digits() {
    assert_eq!(digest_bytes(SHA_A), Some([0xaa; 32]));
    assert_eq!(digest_bytes(&SHA_A[..62]), None);
    assert_eq!(digest_bytes(&format!("{}zz", &SHA_A[..62])), None);
}

#[test]
fn each_file_writing_builtin_records_its_saved_files() {
    let events = [
        completed(
            1,
            "plot",
            "nika:chart",
            &json!({"path": "./out/chart.svg", "sha256": SHA_A, "bytes": 120, "wrote": true,
                "vega_lite": {"path": "./out/chart.vl.json", "sha256": SHA_B}}),
        ),
        completed(
            2,
            "render",
            "nika:image_generate",
            &json!({"count": 2, "images": [
                {"path": "out/img/a.webp", "sha256": SHA_A, "size_bytes": 10, "mime_type": "image/webp"},
                {"path": "out/img/b.png", "sha256": SHA_B, "size_bytes": 11, "mime_type": "image/png"}
            ]}),
        ),
        completed(
            3,
            "speak",
            "nika:tts_generate",
            &json!({"audio": {"path": "out/voice.mp3", "sha256": SHA_A, "size_bytes": 12, "mime_type": "audio/mpeg"}}),
        ),
        completed(
            4,
            "crop",
            "nika:image_fx",
            &json!({"input": "in/raw.png", "input_sha256": SHA_B, "path": "out/crop.jpeg",
                "sha256": SHA_A, "size_bytes": 13, "format": "jpeg"}),
        ),
    ];
    let artifacts = derive(&events);
    assert_eq!(
        names(&artifacts),
        [
            "file/out/chart.svg",
            "file/out/img/a.webp",
            "file/out/img/b.png",
            "file/out/voice.mp3",
            "file/out/crop.jpeg"
        ],
        "the chart's companion has no journaled size, the fx input is read, never written"
    );
    let chart = &artifacts[0];
    assert_eq!(chart.kind, Kind::File);
    assert_eq!(chart.path(), Some("out/chart.svg"));
    assert_eq!((chart.size, chart.sha256.as_str()), (120, SHA_A));
    assert_eq!(chart.mime, "image/svg+xml");
    assert_eq!(chart.task.as_deref(), Some("plot"));
    assert_eq!(chart.event_id, uuid::Uuid::from_u128(1).to_string());
    assert_eq!(artifacts[1].mime, "image/webp");
    assert_eq!(artifacts[3].mime, "audio/mpeg");
    assert_eq!(
        (artifacts[4].mime.as_str(), artifacts[4].size),
        ("image/jpeg", 13)
    );
}

/// The frames the runtime journals for a fan-out (`task/fan_out.rs` · `fan_note`): its
/// `task_started` and `task_completed` noted `note`, which names no tool, and between them
/// one tool-plane `permit_checked` frame per item, as each item's dispatch decided its gate.
fn fan_out(task: &str, note: &str, gates: &[&str], value: &Value) -> Vec<Event> {
    let mut frames = vec![event(
        EventKind::TaskStarted,
        100,
        &[("task", task.to_owned()), ("note", note.to_owned())],
    )];
    for (serial, gate) in (101..).zip(gates) {
        frames.push(event(
            EventKind::PermitChecked,
            serial,
            &[
                ("task", task.to_owned()),
                ("plane", "tool".to_owned()),
                ("gate", (*gate).to_owned()),
                ("decision", "allow".to_owned()),
                ("why", "permits.tools covers the id".to_owned()),
            ],
        ));
    }
    let outcome =
        json!({"class": "success", "cause": "normal", "payload": {"attempts": 1, "value": value}});
    frames.push(event(
        EventKind::TaskCompleted,
        200,
        &[
            ("task", task.to_owned()),
            ("note", note.to_owned()),
            ("outcome", outcome.to_string()),
        ],
    ));
    frames
}

/// A fan-out over one file-saving tool records each item's file: its note is
/// `for_each · 2 items`, so the tool is the one every tool-plane decision of the task allowed.
/// A fan-out an agent frame came from, one whose gates name two tools, one with a recovered
/// item and one whose value is not one answer per item record nothing.
#[test]
fn a_fan_out_records_one_file_per_item() {
    let value = json!([
        {"path": "out/1.svg", "sha256": SHA_A, "bytes": 1},
        {"path": "out/2.svg", "sha256": SHA_B, "bytes": 2}
    ]);
    let chart = ["nika:chart", "nika:chart"];
    let clean = fan_out("draw", "for_each · 2 items", &chart, &value);
    let artifacts = derive(&clean);
    assert_eq!(names(&artifacts), ["file/out/1.svg", "file/out/2.svg"]);
    assert_eq!(
        (artifacts[1].size, artifacts[1].sha256.as_str()),
        (2, SHA_B)
    );
    assert_eq!(artifacts[1].task.as_deref(), Some("draw"));
    assert_eq!(artifacts[1].mime, "image/svg+xml");

    let mut agent = clean.clone();
    agent.insert(
        1,
        event(
            EventKind::ToolInvoked,
            150,
            &[
                ("task", "draw".to_owned()),
                ("tool", "nika:chart".to_owned()),
            ],
        ),
    );
    let refused = [
        ("an agent frame came from it", agent),
        (
            "its gates name two tools",
            fan_out(
                "draw",
                "for_each · 2 items",
                &["nika:chart", "nika:image_fx"],
                &value,
            ),
        ),
        (
            "an item was recovered",
            fan_out("draw", "for_each · 1/2 ok · 1 recovered: b", &chart, &value),
        ),
        (
            "the value is not one answer per item",
            fan_out("draw", "for_each · 3 items", &chart, &value),
        ),
        (
            "no tool-plane decision allowed a tool",
            fan_out("draw", "for_each · 2 items", &[], &value),
        ),
    ];
    for (why, frames) in refused {
        assert!(derive(&frames).is_empty(), "{why}");
    }
    // Another task's decisions are not this task's: the facts are per task.
    let mut elsewhere = fan_out("other", "for_each · 2 items", &chart, &json!([]));
    elsewhere.extend(fan_out("draw", "for_each · 2 items", &[], &value));
    assert!(derive(&elsewhere).is_empty());
}

#[test]
fn a_value_the_tool_did_not_answer_itself_records_nothing() {
    let value = json!({"path": "out/chart.svg", "sha256": SHA_A, "bytes": 1});
    let shaped = |class: &str, cause: &str, extra: Option<(&str, Value)>| {
        let mut payload = json!({"attempts": 1, "value": value});
        if let Some((key, field)) = extra {
            payload[key] = field;
        }
        let outcome = json!({"class": class, "cause": cause, "payload": payload});
        event(
            EventKind::TaskCompleted,
            1,
            &[
                ("task", "plot".to_owned()),
                ("note", "invoke · nika:chart".to_owned()),
                ("outcome", outcome.to_string()),
            ],
        )
    };
    let recovered = json!({"code": "NIKA-X", "message": "failed"});
    for refused in [
        shaped("success", "recovered", Some(("recovered_from", recovered))),
        shaped("success", "normal", Some(("recovered_from", json!({})))),
        shaped("failure", "normal", None),
    ] {
        assert!(derive(&[refused]).is_empty());
    }
    // The same shape from a tool that only reads, hashes or writes without a digest.
    for tool in [
        "nika:write",
        "nika:hash",
        "nika:read",
        "mcp:files/save",
        "workflow:child.nika",
    ] {
        assert!(
            derive(&[completed(1, "t", tool, &value)]).is_empty(),
            "{tool}"
        );
    }
    // Not an invoke at all.
    let agent = event(
        EventKind::TaskCompleted,
        2,
        &[
            ("note", "agent · 1 turns".to_owned()),
            ("outcome", "{}".to_owned()),
        ],
    );
    assert!(derive(&[agent]).is_empty());
    // The fields a cache hit or a start carries are not a write of this run.
    let mut hit = completed(3, "plot", "nika:chart", &value);
    hit.kind = EventKind::TaskCacheHit;
    assert!(derive(&[hit]).is_empty());
}

#[test]
fn only_a_relative_path_inside_the_project_and_outside_dot_nika_qualifies() {
    assert_eq!(contained("./out/a.png").as_deref(), Some("out/a.png"));
    assert_eq!(contained("out//b/./c.png").as_deref(), Some("out/b/c.png"));
    for refused in [
        "../outside.png",
        "out/../../escape.png",
        "/etc/passwd",
        ".nika/serve.token",
        "./.nika/blobs/64/x",
        ".NIKA/keys/run.svg",
        ".Nika/serve.token",
        ".NiKa/case.svg",
        "",
        ".",
        "out\\a.png",
        "out/\0.png",
    ] {
        assert_eq!(contained(refused), None, "{refused:?}");
    }
    assert_eq!(contained(&format!("out/{}", "a".repeat(5000))), None);
    // A spelling only a case-insensitive filesystem folds onto `.nika` (the KELVIN SIGN) passes
    // the name check; the route refuses it by the directory's device and inode.
    assert_eq!(
        contained("./.ni\u{212A}a/state.svg").as_deref(),
        Some(".ni\u{212A}a/state.svg")
    );
    let events = [
        completed(
            1,
            "a",
            "nika:chart",
            &json!({"path": "/tmp/abs.svg", "sha256": SHA_A, "bytes": 1}),
        ),
        completed(
            2,
            "b",
            "nika:chart",
            &json!({"path": "../up.svg", "sha256": SHA_A, "bytes": 1}),
        ),
        completed(
            3,
            "c",
            "nika:chart",
            &json!({"path": ".nika/keys/run.svg", "sha256": SHA_A, "bytes": 1}),
        ),
        completed(
            4,
            "d",
            "nika:chart",
            &json!({"path": "out/ok.svg", "sha256": "ABC", "bytes": 1}),
        ),
        completed(
            5,
            "e",
            "nika:chart",
            &json!({"path": "out/ok.svg", "sha256": SHA_A, "bytes": -1}),
        ),
    ];
    assert!(derive(&events).is_empty());
}

#[test]
fn a_rewritten_file_is_listed_once_at_its_last_write() {
    let first = completed(
        1,
        "a",
        "nika:chart",
        &json!({"path": "out/x.svg", "sha256": SHA_A, "bytes": 1}),
    );
    let other = completed(
        2,
        "b",
        "nika:chart",
        &json!({"path": "out/y.svg", "sha256": SHA_A, "bytes": 1}),
    );
    let again = completed(
        3,
        "c",
        "nika:chart",
        &json!({"path": "./out/x.svg", "sha256": SHA_B, "bytes": 2}),
    );
    let artifacts = derive(&[first, other, again]);
    assert_eq!(names(&artifacts), ["file/out/y.svg", "file/out/x.svg"]);
    assert_eq!(
        (artifacts[1].sha256.as_str(), artifacts[1].size),
        (SHA_B, 2)
    );
    assert_eq!(artifacts[1].task.as_deref(), Some("c"));
}

#[test]
fn a_stored_agent_image_is_its_blob_and_nothing_else_is() {
    let size = 1_239_154;
    let artifacts = derive(&[
        observed(1, &stored_image(SHA_A, size)),
        observed(2, &stored_image(SHA_B, 7)),
    ]);
    assert_eq!(
        names(&artifacts),
        ["agent_image/1.png", "agent_image/2.png"]
    );
    let image = &artifacts[0];
    assert_eq!(image.kind, Kind::AgentImage);
    assert_eq!(image.at, format!(".nika/blobs/64/{}", &BLAKE[2..]));
    assert_eq!(image.path(), None, "a blob's place is never on the wire");
    assert_eq!(
        (image.size, image.sha256.as_str(), image.mime.as_str()),
        (size, SHA_A, "image/png")
    );
    let refused = |patch: &dyn Fn(&mut Value)| {
        let mut observation = stored_image(SHA_A, 9);
        patch(&mut observation);
        derive(&[observed(9, &observation)])
    };
    assert!(refused(&|o| o["storage"] = json!("failed")).is_empty());
    assert!(refused(&|o| o["storage"] = json!("unconfirmed")).is_empty());
    assert!(refused(&|o| o["received_bytes"] = json!(8)).is_empty());
    assert!(refused(&|o| o["received_sha256"] = Value::Null).is_empty());
    assert!(refused(&|o| o["blob"]["hash"] = json!(format!("sha256:{BLAKE}"))).is_empty());
    assert!(refused(&|o| o["blob"]["hash"] = json!("blake3:../../etc")).is_empty());
    assert!(refused(&|o| o["blob"] = Value::Null).is_empty());
    let jpeg = refused(&|o| o["mime_type"] = json!("image/jpeg"));
    assert_eq!(names(&jpeg), ["agent_image/1.jpg"]);
    let odd = refused(&|o| {
        o["mime_type"] = json!("image/png\r\nX-Evil: 1");
        o["blob"]["mime_type"] = Value::Null;
    });
    assert_eq!(odd[0].mime, "application/octet-stream");
    assert_eq!(odd[0].name, "agent_image/1.bin");
    let not_an_image = refused(&|o| {
        o["mime_type"] = json!("text/html");
        o["blob"]["mime_type"] = json!("text/html");
    });
    assert_eq!(not_an_image[0].mime, "application/octet-stream");
}

/// B-D18 · an image the engine wrote under `images: { save: }` is a project file artifact with
/// the RECEIVED digest — beside its blob, never the path the harness reported.
#[test]
fn a_saved_agent_image_is_also_its_project_file_with_the_received_digest() {
    let mut saved = stored_image(SHA_A, 42);
    saved["saved_path"] = json!("./out/release/banner.png");
    let artifacts = derive(&[observed(1, &saved)]);
    assert_eq!(
        names(&artifacts),
        ["agent_image/1.png", "file/out/release/banner.png"]
    );
    let file = &artifacts[1];
    assert_eq!(file.kind, Kind::File);
    assert_eq!(file.path(), Some("out/release/banner.png"));
    assert_eq!(
        (file.size, file.sha256.as_str(), file.mime.as_str()),
        (42, SHA_A, "image/png")
    );
    assert_eq!(file.task.as_deref(), Some("draw"));
    // Unsaved, path-only or outside the project: no file artifact, and the reported path
    // is never one.
    let unsaved = derive(&[observed(2, &stored_image(SHA_B, 7))]);
    assert_eq!(names(&unsaved), ["agent_image/1.png"]);
    for outside in [
        "/elsewhere/codex/image.png",
        "../out.png",
        ".nika/blobs/x.png",
    ] {
        let mut escaped = stored_image(SHA_A, 42);
        escaped["saved_path"] = json!(outside);
        assert_eq!(
            names(&derive(&[observed(3, &escaped)])),
            ["agent_image/1.png"],
            "{outside}"
        );
    }
    let mut undigested = stored_image(SHA_A, 42);
    undigested["saved_path"] = json!("out/a.png");
    undigested["received_sha256"] = Value::Null;
    assert!(derive(&[observed(4, &undigested)]).is_empty());
    // A later write of the same path by a builtin replaces it, like any rewritten file.
    let rewritten = derive(&[
        observed(5, &saved),
        completed(
            6,
            "fx",
            "nika:image_fx",
            &json!({"path": "out/release/banner.png", "sha256": SHA_B, "size_bytes": 9, "format": "png"}),
        ),
    ]);
    assert_eq!(
        names(&rewritten),
        ["agent_image/1.png", "file/out/release/banner.png"]
    );
    assert_eq!(rewritten[1].sha256, SHA_B, "the last journaled write wins");
}

fn document(tier: &str, exit: u8, headline: &str, seal: &str, head: &str) -> String {
    json!({
        "verify_version": 1, "trace": "/p/.nika/traces/x.ndjson", "tier": tier, "exit": exit,
        "chain": {"events": 9, "head": head, "headline": headline, "liveness": null},
        "seal": {"tier": seal}, "anchor": {"tier": "not-present"}, "replay": {"tier": "not-asked"},
        "lines": []
    })
    .to_string()
}

#[test]
fn only_an_intact_sealed_journal_with_the_receipts_head_holds() {
    assert_eq!(
        sealed_tier(0, &document("sealed", 0, "intact", "sealed", "h1"), "h1"),
        Some("sealed")
    );
    assert_eq!(
        sealed_tier(0, &document("anchored", 0, "intact", "sealed", "h1"), "h1"),
        Some("anchored")
    );
    for (code, doc, why) in [
        (
            0,
            document("sealed", 0, "intact", "sealed", "h2"),
            "another head",
        ),
        (
            2,
            document("ok", 2, "intact", "forged", "h1"),
            "a forged seal still reads ok",
        ),
        (
            2,
            document("sealed", 2, "intact", "sealed", "h1"),
            "an anchor gap still reads sealed",
        ),
        (3, document("ok", 3, "intact", "unsealed", "h1"), "unsealed"),
        (
            5,
            document("ok", 5, "incomplete", "unsealed", "h1"),
            "incomplete",
        ),
        (0, document("ok", 0, "torn", "sealed", "h1"), "a torn tail"),
        (
            0,
            document("sealed", 0, "intact", "unattributable", "h1"),
            "an unknown key",
        ),
        (0, "not json".to_owned(), "no document"),
    ] {
        assert_eq!(sealed_tier(code, &doc, "h1"), None, "{why}");
    }
}

fn file_with(bytes: &[u8]) -> (tempfile::NamedTempFile, [u8; 32]) {
    let mut file = tempfile::NamedTempFile::new().expect("temp file");
    file.write_all(bytes).expect("write");
    (file, Sha256::digest(bytes).into())
}

#[test]
fn the_whole_check_refuses_other_bytes_a_shorter_and_a_longer_file() {
    let bytes = vec![7_u8; 600_000];
    let (file, digest) = file_with(&bytes);
    let mut handle = file.reopen().expect("reopen");
    let chunks = verify_whole(&mut handle, 600_000, &digest)
        .expect("read")
        .expect("the recorded file");
    assert_eq!(chunks.len(), 3, "256 KiB chunks: 262144 · 262144 · 75712");
    let first: [u8; 32] = Sha256::digest(&bytes[..262_144]).into();
    let last: [u8; 32] = Sha256::digest(&bytes[524_288..]).into();
    assert_eq!((chunks[0], chunks[2]), (first, last));
    for (size, expected, why) in [
        (599_999, digest, "longer than its record"),
        (600_001, digest, "shorter than its record"),
        (600_000, [0; 32], "other bytes"),
    ] {
        let verdict = verify_whole(&mut handle, size, &expected).expect("read");
        assert_eq!(verdict, None, "{why}");
    }
    let (empty, nothing) = file_with(b"");
    let none = verify_whole(&mut empty.reopen().expect("reopen"), 0, &nothing).expect("read");
    assert_eq!(none, Some(Vec::new()), "an empty record holds no chunk");
}

/// The digests a whole check of `bytes` keeps, and the file holding them.
fn verified_file(bytes: &[u8]) -> (tempfile::NamedTempFile, Vec<[u8; 32]>) {
    let (file, digest) = file_with(bytes);
    let size = bytes.len() as u64;
    let chunks = verify_whole(&mut file.reopen().expect("reopen"), size, &digest)
        .expect("read")
        .expect("the recorded file");
    (file, chunks)
}

async fn stream(file: &tempfile::NamedTempFile, size: u64, chunks: &[[u8; 32]]) -> Vec<u8> {
    VerifiedBody::new(file.reopen().expect("reopen"), size, chunks.to_vec())
        .collect()
        .await
        .expect("infallible")
        .to_bytes()
        .to_vec()
}

fn change_at(file: &tempfile::NamedTempFile, offset: u64, bytes: &[u8]) {
    let mut handle = std::fs::OpenOptions::new()
        .write(true)
        .open(file.path())
        .expect("open for change");
    std::io::Seek::seek(&mut handle, std::io::SeekFrom::Start(offset)).expect("seek");
    handle.write_all(bytes).expect("change in place");
}

/// Every byte arrives while the file holds the record. Once a chunk changed after the whole
/// check, nothing from that chunk on leaves: the first chunk sends nothing at all, a later
/// one sends exactly the chunks before it, a truncation the whole chunks it still holds.
#[tokio::test(flavor = "multi_thread")]
async fn the_stream_sends_only_chunks_that_hash_to_the_record() {
    let bytes: Vec<u8> = (0..700_000_u32)
        .map(|n| u8::try_from(n % 251).expect("a byte"))
        .collect();
    let size = 700_000;
    let (file, chunks) = verified_file(&bytes);
    assert_eq!(
        stream(&file, size, &chunks).await,
        bytes,
        "the whole record"
    );

    change_at(&file, 10, b"tampered");
    assert!(
        stream(&file, size, &chunks).await.is_empty(),
        "a changed first chunk sends no byte"
    );
    change_at(&file, 10, &bytes[10..18]);
    change_at(&file, 650_000, b"tampered");
    let short = stream(&file, size, &chunks).await;
    assert_eq!(
        short.len(),
        524_288,
        "the two chunks before the changed one"
    );
    assert_eq!(&short[..], &bytes[..524_288], "exactly the record's bytes");

    file.as_file().set_len(300_000).expect("truncate");
    let cut = stream(&file, size, &chunks).await;
    assert_eq!(
        &cut[..],
        &bytes[..262_144],
        "only the whole chunk still held"
    );

    let (empty, none) = verified_file(b"");
    assert!(stream(&empty, 0, &none).await.is_empty());
}

/// A manifest lists at most its bound, in journal order, and says when more exist.
#[test]
fn a_long_manifest_lists_its_bound_and_says_it_is_truncated() {
    let events: Vec<Event> = (0..=MAX_LISTED)
        .map(|n| {
            let value = json!({"path": format!("out/{n}.svg"), "sha256": SHA_A, "bytes": n});
            completed(
                u128::try_from(n).expect("serial"),
                "plot",
                "nika:chart",
                &value,
            )
        })
        .collect();
    let receipt = crate::JobReceipt::new(
        crate::JobId::parse("3d6f0a7d-27d4-4e48-b17a-fc6fdb40255b").expect("job id"),
        "exe-1",
        "trace-1",
        SHA_A,
        Some("head".to_owned()),
    )
    .expect("receipt");
    let verified = Verified {
        receipt,
        verdict: "sealed",
        artifacts: derive(&events),
    };
    assert_eq!(verified.artifacts.len(), MAX_LISTED + 1);
    let body = serde_json::to_value(ManifestBody::new(&verified, 7)).expect("manifest");
    assert_eq!(body["truncated"], true);
    assert_eq!(body["max_artifact_bytes"], 7);
    let listed = body["artifacts"].as_array().expect("entries");
    assert_eq!(listed.len(), MAX_LISTED);
    assert_eq!(listed[0]["name"], "file/out/0.svg");
    assert_eq!(
        listed[MAX_LISTED - 1]["name"],
        format!("file/out/{}.svg", MAX_LISTED - 1)
    );
    let short = Verified {
        artifacts: verified.artifacts[..2].to_vec(),
        ..verified
    };
    let body = serde_json::to_value(ManifestBody::new(&short, 7)).expect("manifest");
    assert_eq!(body["truncated"], false);
    assert_eq!(body["artifacts"].as_array().map(Vec::len), Some(2));
}

/// The place the door reads an agent image from is the one the harness image store writes:
/// the image room's root and the blob store's `<2 hex>/<62 hex>` sharding, read from their own
/// sources (both keep them private), so a change there fails here instead of making every
/// stored image unavailable.
#[test]
fn the_blob_layout_is_the_image_rooms() {
    let room = include_str!("../../../../nika-runtime-laws/src/image_room.rs");
    let declared = format!("const BLOBS: &str = \"{}\";", super::manifest::BLOB_ROOT);
    assert!(room.contains(&declared), "the image room's blob root moved");
    let store = include_str!("../../../../nika-blob/src/filesystem.rs");
    for line in [
        "let shard = self.root.join(&raw[..2]);",
        "let blob = shard.join(&raw[2..]);",
    ] {
        assert!(
            store.contains(line),
            "the blob store's sharding moved: {line}"
        );
    }
    let image = derive(&[observed(1, &stored_image(SHA_A, 3))]);
    assert_eq!(
        image[0].at,
        format!(
            "{}/{}/{}",
            super::manifest::BLOB_ROOT,
            &BLAKE[..2],
            &BLAKE[2..]
        )
    );
}
