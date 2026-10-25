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

#[test]
fn a_request_names_an_artifact_by_decoded_segments_and_nothing_else() {
    assert_eq!(
        decode_name("file/out/a.png").as_deref(),
        Some("file/out/a.png")
    );
    assert_eq!(
        decode_name("file/out/Rapport%20final.pdf").as_deref(),
        Some("file/out/Rapport final.pdf")
    );
    assert_eq!(
        decode_name("file/caf%C3%A9.txt").as_deref(),
        Some("file/café.txt")
    );
    assert_eq!(decode_name("file/%4a.txt").as_deref(), Some("file/J.txt"));
    for refused in [
        "file/out%2Fa.png",
        "file/%2e%2e%2fsecret",
        "file/%zz",
        "file/%4",
        "file/%+f",
        "file/%00",
        "file//a.png",
        "file/",
        "",
        "file/%C3",
    ] {
        assert_eq!(decode_name(refused), None, "{refused}");
    }
    // `..` decodes, and matches no manifest name: names never hold one.
    assert_eq!(decode_name("file/%2e%2e/x").as_deref(), Some("file/../x"));
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

#[test]
fn a_fan_out_records_one_file_per_item() {
    let value = json!([
        {"path": "out/1.svg", "sha256": SHA_A, "bytes": 1},
        {"path": "out/2.svg", "sha256": SHA_B, "bytes": 2}
    ]);
    let artifacts = derive(&[completed(1, "plot", "nika:chart", &value)]);
    assert_eq!(names(&artifacts), ["file/out/1.svg", "file/out/2.svg"]);
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
        "",
        ".",
        "out\\a.png",
        "out/\0.png",
    ] {
        assert_eq!(contained(refused), None, "{refused:?}");
    }
    assert_eq!(contained(&format!("out/{}", "a".repeat(5000))), None);
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
    let bytes = vec![7_u8; 300_000];
    let (file, digest) = file_with(&bytes);
    let mut handle = file.reopen().expect("reopen");
    assert!(verify_whole(&mut handle, 300_000, &digest).expect("read"));
    assert!(!verify_whole(&mut handle, 299_999, &digest).expect("longer than its record"));
    assert!(!verify_whole(&mut handle, 300_001, &digest).expect("shorter than its record"));
    assert!(!verify_whole(&mut handle, 300_000, &[0; 32]).expect("other bytes"));
    let (empty, nothing) = file_with(b"");
    assert!(verify_whole(&mut empty.reopen().expect("reopen"), 0, &nothing).expect("empty"));
}

/// Every byte arrives when the file still holds the record; a file changed after the
/// whole check never yields a complete body: its last chunk is withheld.
#[tokio::test(flavor = "multi_thread")]
async fn the_stream_withholds_its_last_chunk_from_a_changed_file() {
    let bytes: Vec<u8> = (0..700_000_u32).map(|n| (n % 251) as u8).collect();
    let (file, digest) = file_with(&bytes);
    let streamed = VerifiedBody::new(file.reopen().expect("reopen"), 700_000, digest)
        .collect()
        .await
        .expect("infallible")
        .to_bytes();
    assert_eq!(streamed.as_ref(), bytes.as_slice());

    let body = VerifiedBody::new(file.reopen().expect("reopen"), 700_000, digest);
    let mut changed = std::fs::OpenOptions::new()
        .write(true)
        .open(file.path())
        .expect("open for change");
    std::io::Seek::seek(&mut changed, std::io::SeekFrom::Start(650_000)).expect("seek");
    changed.write_all(b"tampered").expect("change in place");
    let short = body.collect().await.expect("infallible").to_bytes();
    assert!(
        short.len() < 700_000,
        "only {} bytes may leave",
        short.len()
    );
    assert_eq!(
        &short[..],
        &bytes[..short.len()],
        "what left is the record's prefix"
    );

    let truncated = VerifiedBody::new(file.reopen().expect("reopen"), 700_000, digest);
    file.as_file().set_len(100_000).expect("truncate");
    let cut = truncated.collect().await.expect("infallible").to_bytes();
    assert!(cut.len() <= 100_000);

    let (empty, nothing) = file_with(b"");
    let none = VerifiedBody::new(empty.reopen().expect("reopen"), 0, nothing)
        .collect()
        .await
        .expect("infallible")
        .to_bytes();
    assert!(none.is_empty());
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
