// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The evidence-pack tests — split from `evidence.rs` 2026-07-29 at the
//! 1,500-LOC ceiling (the `prologue/tests.rs` precedent).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use nika_event::{Event, EventKind};
use nika_types::id::EventId;
use nika_types::resource::{KeyValue, Value as FieldValue};
use nika_types::timestamp::Timestamp;

use super::*;
use crate::chain::CHAIN_GENESIS;

/// A workflow with a declared boundary, one exec task and one
/// check-clean and projectable (the `assert:` key it carried died
/// 2026-08-13 · spec 15 « the subtraction is the fix »).
const WF_YAML: &str = "nika: pay\npermits:\n  fs: { read: [\"./in/**\"], write: [\"./out/**\"] }\n  exec: [\"echo\"]\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n";

fn keypair() -> (String, minisign::SecretKey) {
    let pair = minisign::KeyPair::generate_unencrypted_keypair().expect("keypair");
    (pair.pk.to_box().expect("pk box").to_string(), pair.sk)
}

#[test]
fn enrollment_decodes_whole_boxes_and_rejects_non_public_material() {
    let (first, secret) = keypair();
    let (second, _) = keypair();
    let misplaced = secret
        .to_box(None)
        .expect("synthetic secret box")
        .to_string();
    let ledger = format!("{first}\n{second}\n{misplaced}\n{first}");
    let mut candidates = Vec::new();
    push_unique_pubkey(&mut candidates, &ledger);
    assert_eq!(
        candidates.len(),
        2,
        "two real public keys, neither duplicates nor private material"
    );
    assert_eq!(
        candidates[0],
        (fingerprint(first.trim()), first.trim().to_owned())
    );
    assert_eq!(
        candidates[1],
        (fingerprint(second.trim()), second.trim().to_owned())
    );
}

fn parsed_wf() -> nika_schema::raw::RawWorkflow {
    nika_schema::parse(
        WF_YAML,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses")
}

fn wf_semantic() -> String {
    semantic_ir_hash(&parsed_wf())
        .expect("projectable")
        .as_hex()
        .to_owned()
}

fn wf_permits_json() -> String {
    let permits = parsed_wf().permits.expect("fixture declares a boundary");
    serde_json::to_string(&permits.value).expect("permits serialize")
}

/// One journaled event with string fields.
fn event(kind: EventKind, fields: &[(&str, &str)]) -> Event {
    let mut ev = Event::new(
        EventId::generate(),
        Timestamp::from_unix_ms(1_700_000_000_000),
        kind,
    );
    for (key, value) in fields {
        ev = ev.with_field(KeyValue::new(*key, FieldValue::String((*value).to_owned())));
    }
    ev
}

/// Append one event to a raw journal, continuing the chain (mirrors
/// the sink: the `chain` field is the PREVIOUS line's sha256, the
/// head advances over the exact written bytes).
fn append_chained(raw: &mut String, chain: &mut String, ev: &Event) {
    let mut v = serde_json::to_value(ev).expect("event serializes");
    v.as_object_mut()
        .expect("an event is an object")
        .insert("chain".to_owned(), Value::String(chain.clone()));
    let line = serde_json::to_string(&v).expect("line serializes");
    *chain = sha256_hex(line.as_bytes());
    raw.push_str(&line);
    raw.push('\n');
}

fn chained(events: &[Event]) -> (String, String) {
    let mut raw = String::new();
    let mut chain = sha256_hex(CHAIN_GENESIS);
    for ev in events {
        append_chained(&mut raw, &mut chain, ev);
    }
    (raw, chain)
}

/// The A5+ `workflow_started` — records the boundary, the semantic
/// hash and the sandbox mode in the journal itself.
fn started_v2(sem: &str) -> Event {
    event(
        EventKind::WorkflowStarted,
        &[
            ("workflow", "pay"),
            ("permits", "declared boundary · default-deny"),
            ("workflow_sha256", &sha256_hex(WF_YAML.as_bytes())),
            ("engine_version", "0.105.0"),
            ("platform", "macos/aarch64"),
            ("semantic_hash", sem),
            ("sandbox", "seatbelt"),
            ("permits_json", &wf_permits_json()),
        ],
    )
}

/// A pre-A5 `workflow_started` — none of the evidence fields.
fn started_v1() -> Event {
    event(
        EventKind::WorkflowStarted,
        &[
            ("workflow", "pay"),
            ("permits", "declared boundary · default-deny"),
            ("workflow_sha256", &sha256_hex(WF_YAML.as_bytes())),
            ("engine_version", "0.105.0"),
        ],
    )
}

fn completed() -> Event {
    event(EventKind::WorkflowCompleted, &[("workflow", "pay")])
}

/// A sealed journal over the A5 started event, minted with an
/// explicit key. Returns (raw, `final_head`, fingerprint,
/// `pubkey_box`) — the pubkey rides back so tests enroll it.
fn sealed_journal_with(pk: &str, sk: &minisign::SecretKey) -> (String, String, String, String) {
    let sem = wf_semantic();
    let events = vec![started_v2(&sem), completed()];
    let (mut raw, mut chain) = chained(&events);
    let seal = seal_with(sk, pk, &chain, events.len(), &sem, "0.105.0");
    append_chained(&mut raw, &mut chain, &seal);
    (raw, chain.clone(), fingerprint(pk), pk.to_owned())
}

/// The seal-event WRITER with an explicit key (the pack's grade
/// enrolls the matching pubkey).
fn seal_with(
    sk: &minisign::SecretKey,
    pk: &str,
    head: &str,
    events: usize,
    workflow_hash: &str,
    engine: &str,
) -> Event {
    let covers = serde_json::json!({
        "head": head,
        "events": events,
        "workflow": workflow_hash,
        "engine": engine,
    });
    let preimage = preimage(HashDomain::Trace, 1, &covers);
    let sig_box = minisign::sign(None, sk, Cursor::new(preimage.as_bytes()), None, None)
        .expect("the seal signs");
    Event::new(
        EventId::generate(),
        Timestamp::from_unix_ms(1_700_000_000_100),
        EventKind::RunSealed,
    )
    .with_fields(vec![
        KeyValue::new("seal_format", FieldValue::Int(1)),
        KeyValue::new("covers", FieldValue::String(covers.to_string())),
        KeyValue::new("key_id", FieldValue::String(fingerprint(pk))),
        KeyValue::new("alg", FieldValue::String("ed25519".to_owned())),
        KeyValue::new("sig", FieldValue::String(sig_box.into_string())),
    ])
}

/// Unique-per-test staging under the cargo tmp root (plain `cargo
/// test` shares one process across tests — namespacing per test).
fn stage(test: &str, name: &str, body: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nika-evidence-{test}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let path = dir.join(name);
    std::fs::write(&path, body).expect("staged");
    path
}

fn manifest_of(dir: &Path) -> Value {
    let text = std::fs::read_to_string(dir.join("pack.json")).expect("pack.json readable");
    serde_json::from_str(&text).expect("pack.json parses")
}

/// Build + write + return the manifest (the test pipeline most
/// cases share). The default class — REDACTED (T9).
fn pack_over(
    test: &str,
    raw: &str,
    workflow: Option<&Path>,
    keys: &[(String, String)],
) -> (PathBuf, Value) {
    let trace = stage(test, "run.ndjson", raw);
    let pack = build(&trace, workflow, keys).expect("the pack builds");
    let out = trace.with_extension("out");
    write(&out, &pack).expect("the pack writes");
    let manifest = manifest_of(&out);
    (out, manifest)
}

/// The FULL-CONTENT pipeline (`build_full`) — the exact-bytes class.
fn pack_over_full(
    test: &str,
    raw: &str,
    workflow: Option<&Path>,
    keys: &[(String, String)],
) -> (PathBuf, Value) {
    let trace = stage(test, "run.ndjson", raw);
    let pack = build_full(&trace, workflow, keys).expect("the full pack builds");
    let out = trace.with_extension("out");
    write(&out, &pack).expect("the pack writes");
    let manifest = manifest_of(&out);
    (out, manifest)
}

/// One journaled `task_completed` carrying a payload CANARY in both
/// content fields — the redaction pins read it back.
fn payload_journal(canary: &str) -> (String, String, String) {
    let outcome = serde_json::json!({
        "cause": "normal",
        "class": "success",
        "payload": { "attempts": 1, "value": canary },
    })
    .to_string();
    let mut finished = event(
        EventKind::TaskCompleted,
        &[("task", "ask"), ("output", canary), ("outcome", &outcome)],
    );
    finished = finished.with_field(KeyValue::new("duration_ms", FieldValue::Int(12009)));
    finished = finished.with_field(KeyValue::new(
        "def_hash",
        FieldValue::String("311b6465662a".to_owned()),
    ));
    let sem = wf_semantic();
    let events = vec![
        started_v2(&sem),
        event(EventKind::TaskStarted, &[("task", "ask")]),
        finished,
        completed(),
    ];
    let (raw, head) = chained(&events);
    (raw, head, outcome)
}

/// The field entry of one projection line, by key.
fn field<'v>(line: &'v Value, key: &str) -> &'v Value {
    line["fields"]
        .as_array()
        .expect("fields ride")
        .iter()
        .find(|f| f["key"] == json!(key))
        .unwrap_or_else(|| panic!("the {key} field rides"))
}

/// The sealed pack: journal bytes verbatim · the seal verifies
/// against the enrolled key · journal-provenance boundary, sandbox,
/// engine — and the receipt stays absent (no `--workflow`), said.
/// The byte-identical assertion needs the FULL class (`build_full`):
/// the default class is the redacted projection (T9).
#[test]
fn sealed_pack_exports_with_journal_provenance() {
    let (pk, sk) = keypair();
    let (raw, head, fp, pk) = sealed_journal_with(&pk, &sk);
    let keys = vec![(fp.clone(), pk)];
    let (out, pack) = pack_over_full("sealed", &raw, None, &keys);

    // The journal copy is byte-identical — never re-serialized.
    let copied = std::fs::read_to_string(out.join("journal.ndjson")).expect("journal copied");
    assert_eq!(copied, raw, "the bundle's journal is the exact bytes");
    assert_eq!(pack["redaction"]["class"], json!("full"), "{pack}");
    assert!(
        pack["trace"].get("projection_sha256").is_none(),
        "no projection on the full class: {pack}"
    );

    assert_eq!(pack["evidence_format"], json!(1));
    assert_eq!(pack["trace"]["chain"], json!("intact"));
    assert_eq!(pack["trace"]["head"], json!(head));
    assert_eq!(pack["trace"]["events"], json!(3));
    assert_eq!(
        pack["trace"]["journal_sha256"],
        json!(sha256_hex(raw.as_bytes()))
    );
    assert_eq!(pack["seal"]["present"], json!(true));
    assert_eq!(pack["seal"]["verifies"], json!(true), "{}", pack["seal"]);
    assert_eq!(pack["seal"]["key_id"], json!(fp));
    assert_eq!(pack["seal"]["alg"], json!("ed25519"));
    assert_eq!(pack["seal"]["covers_chain"], json!(true));
    assert_eq!(pack["workflow"]["semantic_hash"], json!(wf_semantic()));
    assert_eq!(pack["workflow"]["semantic_hash_source"], json!("seal"));
    assert_eq!(pack["boundary"]["declared"], json!(true));
    assert_eq!(pack["boundary"]["source"], json!("journal"));
    assert_eq!(
        pack["boundary"]["permits"]["fs"]["write"],
        json!(["./out/**"])
    );
    assert_eq!(pack["sandbox"]["mode"], json!("seatbelt"));
    assert_eq!(pack["engine"]["version"], json!("0.105.0"));
    assert_eq!(pack["receipt"]["present"], json!(false));
    assert!(
        pack["trifecta"].is_null() && !pack["unavailable"]["trifecta"].is_null(),
        "check-time verdict is an honest null without --workflow: {pack}"
    );
    assert!(!out.join("receipt.json").exists());

    let verify_md = std::fs::read_to_string(out.join("VERIFY.md")).expect("VERIFY.md");
    assert!(verify_md.contains("nika trace verify journal.ndjson"));
    assert!(verify_md.contains(&fp), "the enrolled key is named");
    let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
}

/// The `--workflow` arm: the file hash-matches the journal, so the
/// receipt (certificate + asserts + trace verdict), the trifecta
/// verdict and the file-provenance fields land.
#[test]
fn hash_checked_workflow_adds_receipt_and_trifecta() {
    let (pk, sk) = keypair();
    let (raw, _, fp, pk) = sealed_journal_with(&pk, &sk);
    let wf_path = stage("filearm", "wf.nika.yaml", WF_YAML);
    let keys = vec![(fp, pk)];
    let (out, pack) = pack_over("filearm", &raw, Some(&wf_path), &keys);

    // The default class: REDACTED — and the seal still grades (the
    // facts fold from the ORIGINAL bytes before the projection).
    assert_eq!(pack["redaction"]["class"], json!("redacted"), "{pack}");
    assert_eq!(pack["seal"]["verifies"], json!(true), "{pack}");
    assert_eq!(pack["receipt"]["present"], json!(true));
    assert_eq!(pack["receipt"]["proves"], json!(wf_semantic()));
    assert_eq!(pack["trifecta"]["verdict"], json!("clean"));
    assert_eq!(pack["trifecta"]["source"], json!("file"));
    assert_eq!(pack["boundary"]["source"], json!("journal"));

    // The receipt verifies against the workflow's semantic hash and
    // folds the real certificate + the judged assert.
    let receipt_text =
        std::fs::read_to_string(out.join("receipt.json")).expect("receipt.json written");
    let receipt: Value = serde_json::from_str(&receipt_text).expect("receipt parses");
    assert!(
        nika_runtime::proof::receipt::verify(&receipt, &wf_semantic()),
        "the receipt verifies: {receipt}"
    );
    assert_eq!(receipt["receipt_format"], json!(1));
    assert_eq!(receipt["lock_digest"], json!(LOCK_UNRECORDED));
    assert_eq!(receipt["assertions"], json!([]));
    assert_eq!(receipt["trace_verdict"]["outcome"], json!("completed"));
    assert_eq!(receipt["trace_verdict"]["sealed"], json!(true));
    assert!(
        receipt["certificate"].is_object(),
        "the real RunCertificate"
    );
    assert!(
        !nika_runtime::proof::receipt::verify(&receipt, "blake3:someother"),
        "a swapped proof is refused"
    );
    let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
}

/// F-P13 (NEP-0014 law 2) — the input origins project with journal
/// provenance; absent on an older journal, said (never invented).
#[test]
fn the_pack_projects_the_input_origins() {
    let sem = wf_semantic();
    let inputs = serde_json::json!({ "count": "ci-context", "region": "file" }).to_string();
    let events = vec![
        event(
            EventKind::WorkflowStarted,
            &[
                ("workflow", "pay"),
                ("engine_version", "0.106.1"),
                ("semantic_hash", sem.as_str()),
                ("inputs", inputs.as_str()),
            ],
        ),
        completed(),
    ];
    let (raw, _) = chained(&events);
    let (out, pack) = pack_over("inputs", &raw, None, &[]);
    assert_eq!(pack["inputs"]["origins"]["count"], json!("ci-context"));
    assert_eq!(pack["inputs"]["origins"]["region"], json!("file"));
    assert_eq!(pack["inputs"]["source"], json!("journal"));
    let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
}

/// The honest null: a journal without the `inputs` field (pre-F-P13
/// · or a no-input workflow) projects `null` and names why.
#[test]
fn absent_input_origins_are_said_not_invented() {
    let (raw, _) = chained(&[started_v1(), completed()]);
    let (out, pack) = pack_over("inputs-absent", &raw, None, &[]);
    assert!(pack["inputs"].is_null(), "{pack}");
    assert!(
        pack["unavailable"]["inputs"]
            .as_str()
            .is_some_and(|r| r.contains("predates input-origin journaling")),
        "the reason is named: {pack}"
    );
    let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
}

/// An UNSEALED journal packs too: `seal.present: false`, VERIFY.md
/// says what the unsigned tier means — never a faked seal.
#[test]
fn unsealed_pack_is_honest_about_the_unsigned_tier() {
    let sem = wf_semantic();
    let (raw, _) = chained(&[started_v2(&sem), completed()]);
    let (out, pack) = pack_over("unsealed", &raw, None, &[]);
    assert_eq!(pack["seal"], json!({ "present": false }));
    // Journal-provenance fields still land (A5+ journal, unsealed).
    assert_eq!(pack["workflow"]["semantic_hash_source"], json!("journal"));
    assert_eq!(pack["boundary"]["declared"], json!(true));

    let verify_md = std::fs::read_to_string(out.join("VERIFY.md")).expect("VERIFY.md");
    assert!(
        verify_md.contains("NOT sealed") && verify_md.contains("tamper-EVIDENT only"),
        "the unsigned tier explained: {verify_md}"
    );
    let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
}

/// A pre-A5 journal (no evidence fields recorded) exports with
/// honest nulls — every one named in `unavailable`, never guessed.
#[test]
fn old_journal_exports_with_honest_unavailables() {
    let (raw, _) = chained(&[started_v1(), completed()]);
    let (out, pack) = pack_over("old", &raw, None, &[]);
    assert!(pack["boundary"].is_null(), "{pack}");
    assert!(pack["sandbox"].is_null(), "{pack}");
    assert!(pack["trifecta"].is_null(), "{pack}");
    assert!(pack["workflow"]["semantic_hash"].is_null(), "{pack}");
    assert_eq!(pack["receipt"]["present"], json!(false));
    for field in ["boundary", "sandbox", "trifecta", "semantic_hash"] {
        assert!(
            pack["unavailable"][field].is_string(),
            "unavailable.{field} names the reason: {pack}"
        );
    }
    assert!(
        pack["unavailable"]["boundary"]
            .as_str()
            .expect("a reason")
            .contains("--workflow"),
        "the pointer is actionable: {pack}"
    );
    // The engine version WAS recorded — journal provenance.
    assert_eq!(pack["engine"]["version"], json!("0.105.0"));
    let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
}

/// A `--workflow` that is NOT the run's file (hash mismatch) never
/// leaks its boundary/verdict into the pack — the journal's own
/// claims still land, the mismatch is spoken.
#[test]
fn hash_mismatch_workflow_never_leaks_into_the_pack() {
    let (pk, sk) = keypair();
    let (raw, _, fp, pk) = sealed_journal_with(&pk, &sk);
    let other = stage(
        "mismatch",
        "other.nika.yaml",
        "nika: other\ntasks:\n  b:\n    exec: { command: [\"echo\", \"yo\"] }\n",
    );
    let keys = vec![(fp, pk)];
    let (out, pack) = pack_over("mismatch", &raw, Some(&other), &keys);
    // The JOURNAL-provenance boundary still lands (the journal is
    // the attested record) — the mismatch only blocks the FILE arm.
    assert_eq!(pack["boundary"]["source"], json!("journal"), "{pack}");
    assert!(pack["trifecta"].is_null(), "{pack}");
    assert_eq!(pack["receipt"]["present"], json!(false));
    assert!(
        pack["unavailable"]["workflow_file"]
            .as_str()
            .expect("a reason")
            .contains("does not match"),
        "the mismatch is spoken: {pack}"
    );
    // The seal is untouched — the journal's own evidence still packs.
    assert_eq!(pack["seal"]["verifies"], json!(true));
    assert!(!out.join("receipt.json").exists());
    let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
}

/// On a pre-A5 journal (boundary not journaled), the mismatched
/// `--workflow` leaves the boundary itself an honest null.
#[test]
fn hash_mismatch_on_an_old_journal_nulls_the_boundary() {
    let (raw, _) = chained(&[started_v1(), completed()]);
    let other = stage(
        "mismatch-old",
        "other.nika.yaml",
        "nika: other\ntasks:\n  b:\n    exec: { command: [\"echo\", \"yo\"] }\n",
    );
    let (out, pack) = pack_over("mismatch-old", &raw, Some(&other), &[]);
    assert!(pack["boundary"].is_null(), "{pack}");
    assert!(
        pack["unavailable"]["workflow_file"]
            .as_str()
            .expect("a reason")
            .contains("does not match"),
        "the mismatch is spoken: {pack}"
    );
    let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
}

/// Key unavailable → `verifies: null` WITH the enrolment reason —
/// never silent, never false.
#[test]
fn unenrolled_key_grades_null_with_reason() {
    let (pk, sk) = keypair();
    let (raw, _, _, _) = sealed_journal_with(&pk, &sk);
    // Enrolled set carries a DIFFERENT key — the seal's key_id misses.
    let (wrong_pk, _) = keypair();
    let keys = vec![(fingerprint(&wrong_pk), wrong_pk)];
    let (out, pack) = pack_over("nokey", &raw, None, &keys);
    assert_eq!(pack["seal"]["present"], json!(true));
    assert!(pack["seal"]["verifies"].is_null(), "{pack}");
    assert!(
        pack["seal"]["reason"]
            .as_str()
            .expect("a reason")
            .contains("not enrolled"),
        "key-unavailable is null with reason: {pack}"
    );
    let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
}

/// A seal whose signed content was edited grades `false` — the loud
/// forgery case (the journal itself stays chain-intact: the seal is
/// the LAST line, the one position the chain cannot self-check).
#[test]
fn a_tampered_seal_grades_false() {
    let (pk, sk) = keypair();
    let (raw, _, fp, pk) = sealed_journal_with(&pk, &sk);
    let tampered = raw.replace("\\\"events\\\":2", "\\\"events\\\":99");
    assert_ne!(tampered, raw, "the covers string was edited");
    let keys = vec![(fp, pk)];
    let (out, pack) = pack_over("tampered", &tampered, None, &keys);
    assert_eq!(pack["trace"]["chain"], json!("intact"), "{pack}");
    assert_eq!(pack["seal"]["verifies"], json!(false), "{pack}");
    let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
}

/// A mid-journal edit breaks the chain — the pack still exports and
/// says BROKEN loudly (the evidence of tampering IS the point).
#[test]
fn a_broken_chain_packs_loudly() {
    let sem = wf_semantic();
    let (raw, _) = chained(&[started_v2(&sem), completed()]);
    let broken = raw.replace("\"pay\"", "\"paz\"");
    assert_ne!(broken, raw);
    let (out, pack) = pack_over("broken", &broken, None, &[]);
    assert_eq!(pack["trace"]["chain"], json!("broken"), "{pack}");
    assert!(pack["trace"]["head"].is_null(), "{pack}");
    assert!(
        pack["trace"]["note"]
            .as_str()
            .expect("a note")
            .contains("broken at line"),
        "{pack}"
    );
    let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
}

/// The default out dir is `<trace-stem>.evidence/` beside the
/// journal; an existing dir refuses (evidence is never clobbered).
#[test]
fn default_out_dir_and_no_clobber() {
    let sem = wf_semantic();
    let (raw, _) = chained(&[started_v2(&sem), completed()]);
    let trace = stage("layout", "2026-07-20T18-57-07-ab12.ndjson", &raw);
    let expected = trace.with_file_name("2026-07-20T18-57-07-ab12.evidence");
    assert_eq!(default_out(&trace), expected);
    let pack = build(&trace, None, &[]).expect("the pack builds");
    write(&expected, &pack).expect("the pack writes");
    assert!(expected.join("pack.json").exists());
    // Second export over the same dir → the honest refusal.
    let again = write(&expected, &pack);
    let refusal = again.expect_err("an existing dir refuses");
    assert!(matches!(refusal, PackError::Write(_)), "{refusal}");
    assert!(refusal.to_string().contains("already exists"), "{refusal}");
    let _ = std::fs::remove_dir_all(trace.parent().expect("parent"));
}

/// The writer/reader round-trip: a seal minted by the mirror
/// verifies through the pack's grading path — the proof layer's
/// ONE canonicalization makes the preimage byte-equal. A
/// placeholder box in the enrolment set grades null-with-reason.
#[test]
fn the_crafted_seal_verifies_through_the_pack() {
    let (pk_box, sk) = keypair();
    let (raw, _, fp, pk) = sealed_journal_with(&pk_box, &sk);
    let events = crate::recover::recover_events(&raw, "t")
        .expect("recovers")
        .events;
    let facts = seal_facts(&events, &raw, &[(fp, pk)]);
    assert_eq!(facts.verifies, Some(true));
    assert_eq!(facts.covers_chain, Some(true));

    // A placeholder (non-key) box in the enrolment set cannot
    // verify — null with the enrolment reason, never a panic.
    let sealed = events
        .iter()
        .find(|e| matches!(e.kind, EventKind::RunSealed))
        .expect("the seal event");
    let key_id = str_field(sealed, "key_id").expect("key_id").to_owned();
    let covers: Value =
        serde_json::from_str(str_field(sealed, "covers").expect("covers")).expect("json");
    let sig = str_field(sealed, "sig").expect("sig");
    let (verifies, reason) = grade_seal(
        Some(&covers),
        Some(sig),
        Some(&key_id),
        &[(key_id.clone(), "not-a-key-box".to_owned())],
    );
    assert_eq!(verifies, Some(false), "a malformed box is a non-verify");
    assert!(reason.is_none());
}

// ───────────────────────── T9 · the redacted default class ─────────

/// The redaction contract, end to end: no payload byte crosses into
/// the bundle, every placeholder's sha256 verifies against the
/// ORIGINAL field, structural fields and chain links survive
/// verbatim, and the manifest keeps attesting the ORIGINAL journal.
#[test]
fn the_default_pack_redacts_payloads_to_hashes() {
    let canary = "CANARY · the model answered s3cr3t-v4lue-9f8d2c";
    let (raw, head, outcome) = payload_journal(canary);
    let (out, pack) = pack_over("redacted", &raw, None, &[]);

    // No payload byte crosses — neither the bare field nor its copy
    // inside the outcome JSON.
    let projected = std::fs::read_to_string(out.join("journal.ndjson")).expect("journal written");
    assert!(
        !projected.contains(canary),
        "no payload crosses: {projected}"
    );
    assert!(
        !projected.contains("s3cr3t"),
        "not even a fragment: {projected}"
    );

    // One projection line per journal line; the placeholder verifies
    // against the original field's own bytes (the disclosure contract).
    let orig: Vec<Value> = raw
        .lines()
        .map(|l| serde_json::from_str(l).expect("orig parses"))
        .collect();
    let proj: Vec<Value> = projected
        .lines()
        .map(|l| serde_json::from_str(l).expect("projection parses"))
        .collect();
    assert_eq!(proj.len(), orig.len(), "line count is preserved");
    let finished = &proj[2];
    assert_eq!(finished["kind"], json!("task_completed"));
    let output = &field(finished, "output")["value"];
    assert_eq!(output["sha256"], json!(sha256_hex(canary.as_bytes())));
    assert!(
        output["unavailable"]
            .as_str()
            .expect("the reason rides")
            .contains("integrity, not content"),
        "the unavailable pattern carries the reason: {output}"
    );
    let outcome_ph = &field(finished, "outcome")["value"];
    assert_eq!(outcome_ph["sha256"], json!(sha256_hex(outcome.as_bytes())));

    // Structural fields survive verbatim — ids · task · hashes · and a
    // NON-STRING (the int duration keeps its type).
    assert_eq!(finished["id"], orig[2]["id"], "the event id is verbatim");
    assert_eq!(finished["timestamp"], orig[2]["timestamp"]);
    assert_eq!(field(finished, "task")["value"], json!("ask"));
    assert_eq!(field(finished, "duration_ms")["value"], json!(12009));
    assert_eq!(field(finished, "def_hash")["value"], json!("311b6465662a"));

    // The chain links ride verbatim — they attest the ORIGINAL
    // journal's linkage (they do not re-walk over the projection;
    // VERIFY.md says so).
    for (o, p) in orig.iter().zip(&proj) {
        assert_eq!(p["chain"], o["chain"], "the original link rides verbatim");
    }
    assert_eq!(pack["trace"]["head"], json!(head));
    let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
}

/// The manifest's redaction section: the class is SAID, the manifest
/// attests the ORIGINAL journal (`journal_sha256`) AND the shipped
/// projection (`projection_sha256`) — integrity against the bytes the
/// operator keeps, mappable to the bytes the auditor holds.
#[test]
fn the_manifest_attests_the_original_and_names_the_class() {
    let canary = "CANARY-manifest";
    let (raw, _, _) = payload_journal(canary);
    let (out, pack) = pack_over("redacted-manifest", &raw, None, &[]);
    let projected = std::fs::read_to_string(out.join("journal.ndjson")).expect("journal written");

    assert_eq!(pack["evidence_format"], json!(1), "additive — no bump");
    assert_eq!(
        pack["trace"]["journal_sha256"],
        json!(sha256_hex(raw.as_bytes())),
        "the manifest attests the ORIGINAL journal"
    );
    assert_eq!(
        pack["trace"]["projection_sha256"],
        json!(sha256_hex(projected.as_bytes())),
        "and the shipped projection"
    );
    assert_eq!(pack["redaction"]["class"], json!("redacted"));
    assert_eq!(pack["redaction"]["placeholders"], json!(2));
    let fields = pack["redaction"]["fields"]
        .as_array()
        .expect("the key list");
    for key in ["output", "outcome", "detail", "delta", "message", "choices"] {
        assert!(fields.contains(&json!(key)), "the policy names {key}");
    }
    assert!(
        pack["redaction"]["posture"]
            .as_str()
            .expect("the posture rides")
            .contains("INTEGRITY"),
        "the corollary is written where the auditor reads it: {pack}"
    );
    let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
}

/// A journal with NO payload field projects BYTE-IDENTICAL (untouched
/// lines ride verbatim) — the chain still re-walks over the
/// projection, and the placeholder count says nothing was cut.
#[test]
fn a_zero_payload_journal_projects_byte_identical() {
    let sem = wf_semantic();
    let (raw, _) = chained(&[started_v2(&sem), completed()]);
    let (out, pack) = pack_over("zero-payload", &raw, None, &[]);
    let projected = std::fs::read_to_string(out.join("journal.ndjson")).expect("journal written");
    assert_eq!(projected, raw, "no payload field → the exact bytes");
    assert_eq!(pack["redaction"]["placeholders"], json!(0));
    assert!(
        matches!(
            crate::chain::walk(&projected),
            crate::chain::Verdict::Intact { .. }
        ),
        "the untouched projection still re-walks"
    );
    let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
}

/// A torn tail (a crash mid-write) can carry HALF a payload: the
/// unparseable line never crosses — it becomes ONE marker object
/// carrying its sha256 (disclosable like any field).
#[test]
fn a_torn_tail_redacts_to_a_marker() {
    let sem = wf_semantic();
    let (mut raw, _) = chained(&[started_v2(&sem), completed()]);
    let tail =
        "{\"kind\":\"task_completed\",\"fields\":[{\"key\":\"output\",\"value\":\"CANARY-torn";
    raw.push_str(tail); // no newline — torn mid-write
    let (out, pack) = pack_over("torn", &raw, None, &[]);
    assert_eq!(pack["trace"]["chain"], json!("torn_tail"), "{pack}");
    let projected = std::fs::read_to_string(out.join("journal.ndjson")).expect("journal written");
    assert!(
        !projected.contains("CANARY-torn"),
        "the partial payload never crosses: {projected}"
    );
    let last = projected.lines().last().expect("a tail line");
    let marker: Value = serde_json::from_str(last).expect("the marker parses");
    assert_eq!(marker["unparseable_tail"], json!(true));
    assert_eq!(marker["sha256"], json!(sha256_hex(tail.as_bytes())));
    assert!(marker["unavailable"].is_string(), "the reason rides");
    let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
}

/// VERIFY.md teaches BOTH classes honestly: the redacted text leads
/// with the corollary (INTEGRITY ≠ CONTENT), warns the chain does not
/// re-walk here, and names the two disclosure checks; the full text
/// keeps the three commands and wears the sensitivity warning.
#[test]
fn verify_md_teaches_both_classes() {
    let canary = "CANARY-verify";
    let (raw, _, _) = payload_journal(canary);
    let (out, _) = pack_over("verify-redacted", &raw, None, &[]);
    let redacted = std::fs::read_to_string(out.join("VERIFY.md")).expect("VERIFY.md");
    assert!(
        redacted.contains("INTEGRITY, not its CONTENT"),
        "the corollary leads: {redacted}"
    );
    assert!(
        redacted.contains("trace.projection_sha256"),
        "the offline check is named"
    );
    assert!(
        redacted.contains("do not re-walk") || redacted.contains("does not re-walk"),
        "the chain-class change is said"
    );
    assert!(
        redacted.contains("trace.journal_sha256"),
        "the disclosure path names the original's attestation"
    );
    let _ = std::fs::remove_dir_all(out.parent().expect("parent"));

    let (out, pack) = pack_over_full("verify-full", &raw, None, &[]);
    assert_eq!(pack["redaction"]["class"], json!("full"));
    let full = std::fs::read_to_string(out.join("VERIFY.md")).expect("VERIFY.md");
    assert!(
        full.contains("nika trace verify journal.ndjson"),
        "the re-walk command stays on the full class"
    );
    assert!(
        full.contains("CONTENT") && full.contains("sensitively"),
        "the full class SAYS it carries content: {full}"
    );
    let _ = std::fs::remove_dir_all(out.parent().expect("parent"));
}
