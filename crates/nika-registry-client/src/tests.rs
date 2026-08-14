// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The registry-client test suite — the trust chain exercised over the
//! mock HTTP seam (`src/<mod>/tests.rs` is the prod-LOC-exempt
//! convention · the nika-http `src/tests.rs` precedent). `super`
//! resolves to the registry module — semantics unchanged.

use super::*;
use nika_kernel_mock::MockHttp;

const REV: &str = "0123456789abcdef0123456789abcdef01234567";
const BODY: &[u8] = b"nika: greet\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n";

fn kind_code(err: &RegistryError) -> Option<&'static str> {
    err.code()
}

// -- ref parsing ---------------------------------------------------

#[test]
fn detects_registry_refs() {
    assert!(is_registry_ref("registry:acme/greet"));
    assert!(is_registry_ref("registry:acme/greet@0.1.0"));
    assert!(!is_registry_ref("flows/greet.nika.yaml"));
    assert!(!is_registry_ref("Registry:acme/greet")); // exact scheme, like every CLI literal
    assert!(!is_registry_ref("-"));
}

#[test]
fn parses_bare_and_versioned_refs() {
    let bare = parse_ref("registry:acme/greet").expect("bare ref parses");
    assert_eq!(bare.owner, "acme");
    assert_eq!(bare.name, "greet");
    assert_eq!(bare.version, None);

    let pinned = parse_ref("registry:acme-corp/my-flow@1.2.0").expect("versioned ref parses");
    assert_eq!(pinned.owner, "acme-corp");
    assert_eq!(pinned.name, "my-flow");
    assert_eq!(pinned.version.as_deref(), Some("1.2.0"));

    let pre = parse_ref("registry:a1/b2@1.0.0-rc.1").expect("pre-release version parses");
    assert_eq!(pre.version.as_deref(), Some("1.0.0-rc.1"));
}

#[test]
fn refuses_malformed_refs() {
    // Each row: the ref + the fragment its teaching text must carry.
    let rows: &[(&str, &str)] = &[
        ("registry:", "owner/name"),
        ("registry:greet", "owner/name"), // owner REQUIRED in v1
        ("registry:acme/", "owner/name"),
        ("registry:/greet", "owner/name"),
        ("registry:a/b/c", "owner/name"),
        ("registry:acme/greet@", "version"),
        ("registry:acme/greet@1.2", "version"), // SemVer is a triple
        ("registry:acme/greet@v1.2.0", "version"), // no v prefix
        ("registry:acme/greet@1.2.0+meta", "version"), // pin without build metadata
        ("registry:acme/Greet", "name"),        // contract: ^[a-z0-9][a-z0-9-]{0,63}$
        ("registry:acme/gre_et", "name"),
        ("registry:-acme/greet", "owner"),
        ("registry:ac me/greet", "owner"),
    ];
    for (arg, teach) in rows {
        let err = parse_ref(arg).expect_err(arg);
        assert!(
            kind_code(&err).is_none(),
            "parse refusals carry no REG code: {arg}"
        );
        let text = err.to_string();
        assert!(
            text.contains(teach) && text.contains("registry:owner/name"),
            "{arg} must teach `{teach}` + the form · got: {text}"
        );
    }
}

// -- version ordering ------------------------------------------------

#[test]
fn version_key_orders_by_semver_precedence() {
    let key = |v: &str| version_key(v).expect(v);
    assert!(key("0.10.0") > key("0.9.9"), "numeric, not lexical");
    assert!(
        key("0.2.0") > key("0.2.0-rc1"),
        "stable outranks its pre-release"
    );
    assert!(key("0.2.0-rc2") > key("0.2.0-rc1"));
    assert!(
        key("0.2.0-rc.10") > key("0.2.0-rc.9"),
        "numeric pre ids compare numerically"
    );
    assert!(
        key("1.0.0-alpha") > key("1.0.0-1"),
        "numeric ids rank below alphanumeric"
    );
    assert!(version_key("banana").is_none());
    assert!(version_key("1.2").is_none());
}

// -- resolve: fixtures ------------------------------------------------

fn artifact_json(
    owner: &str,
    name: &str,
    version: &str,
    kind: &str,
    digest: &str,
    advisories: &[&str],
) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "publisher": owner,
        "version": version,
        "type": kind,
        "sha256": digest,
        "source": {
            "repo": format!("{owner}/flows"),
            "rev": REV,
            "path": format!("flows/{name}.nika.yaml"),
        },
        "advisories": advisories,
        "description": "One line.",
        "license": "Apache-2.0",
        "spec": "nika/v1",
        "entry": format!("registry/workflows/{owner}/{name}/{version}.toml"),
    })
}

fn index_json(artifacts: &[serde_json::Value]) -> String {
    serde_json::json!({ "index_schema": 1, "artifacts": artifacts }).to_string()
}

fn entry_toml(owner: &str, name: &str, version: &str, digest: &str) -> String {
    format!(
        r#"schema = 1
type = "workflow"
name = "{name}"
publisher = "{owner}"
version = "{version}"
description = "One line."
license = "Apache-2.0"
spec = "nika/v1"

[source]
repo = "{owner}/flows"
rev = "{REV}"
path = "flows/{name}.nika.yaml"

[integrity]
sha256 = "{digest}"
"#
    )
}

fn client(mock: MockHttp) -> (RegistryClient<MockHttp>, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("cache tempdir");
    let root = dir.path().join("registry");
    (RegistryClient::new(mock, root), dir)
}

/// The three-response happy-path mock: index → entry → artifact.
fn happy_mock(owner: &str, name: &str, version: &str, body: &[u8]) -> MockHttp {
    let digest = sha256_hex(body);
    MockHttp::new()
        .enqueue_ok(
            200,
            index_json(&[artifact_json(
                owner,
                name,
                version,
                "workflow",
                &digest,
                &[],
            )]),
        )
        .enqueue_ok(200, entry_toml(owner, name, version, &digest))
        .enqueue_ok(200, body.to_vec())
}

fn resolve(client: &RegistryClient<MockHttp>, arg: &str) -> Result<Resolved, RegistryError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(client.resolve(arg))
}

// -- resolve: the trust chain ---------------------------------------

#[test]
fn fetches_verifies_and_caches_a_versioned_ref() {
    let mock = happy_mock("acme", "greet", "0.1.0", BODY);
    let (client, _dir) = client(mock.clone());

    let got = resolve(&client, "registry:acme/greet@0.1.0").expect("resolves");
    assert!(got.fetched);
    assert!(!got.pinned);
    assert_eq!(got.coordinate, "acme/greet@0.1.0");
    assert_eq!(got.sha256, sha256_hex(BODY));
    assert_eq!(
        std::fs::read(&got.path).expect("cached artifact readable"), // seam-bypass-ok: test harness disk fixtures
        BODY,
        "the cached file is the verified bytes"
    );
    assert!(
        got.path.ends_with("acme/greet/0.1.0.nika.yaml"),
        "one canonical cache layout · got {}",
        got.path.display()
    );

    // The URLs are CONSTRUCTED, never trusted from fetched data.
    let urls: Vec<String> = mock.sent_requests().into_iter().map(|r| r.url).collect();
    assert_eq!(
        urls,
        vec![
            format!("{INDEX_BASE}/index.json"),
            format!("{INDEX_BASE}/registry/workflows/acme/greet/0.1.0.toml"),
            format!("{RAW_BASE}/acme/flows/{REV}/flows/greet.nika.yaml"),
        ]
    );

    // No version in the ref → no pin. A versioned ref writes none.
    assert!(
        !client.cache_root.join("acme/greet/pin").exists(),
        "a versioned ref pins nothing"
    );
    assert!(
        !got.signed,
        "an unsigned entry stays on the v0.1 digest floor"
    );
}

// -- v0.2 · the signature half (minisign + TOFU) ----------------------

/// One fresh publisher keypair per test (the minisign crate generates;
/// both sides live in-test, so determinism is not needed).
fn test_keypair() -> (String, minisign::SecretKey) {
    let pair = minisign::KeyPair::generate_unencrypted_keypair().expect("keypair generates");
    let pk = pair.pk.to_box().expect("pk boxes").to_string();
    (pk, pair.sk)
}

fn sign_bytes(sk: &minisign::SecretKey, bytes: &[u8]) -> String {
    minisign::sign(None, sk, std::io::Cursor::new(bytes), None, None)
        .expect("signs")
        .into_string()
}

fn signed_entry_toml(
    owner: &str,
    name: &str,
    version: &str,
    digest: &str,
    sig: &str,
    pk: &str,
) -> String {
    let mut entry = entry_toml(owner, name, version, digest);
    entry.push_str(&format!(
        "\n[signature]\nsignature = \"\"\"\n{sig}\"\"\"\npubkey = \"\"\"\n{pk}\"\"\"\n"
    ));
    entry
}

#[test]
fn a_signed_entry_verifies_and_anchors_the_tofu_key() {
    let (pk, sk) = test_keypair();
    let sig = sign_bytes(&sk, BODY);
    let digest = sha256_hex(BODY);
    let mock = MockHttp::new()
        .enqueue_ok(
            200,
            index_json(&[artifact_json(
                "acme",
                "greet",
                "0.1.0",
                "workflow",
                &digest,
                &[],
            )]),
        )
        .enqueue_ok(
            200,
            signed_entry_toml("acme", "greet", "0.1.0", &digest, &sig, &pk),
        )
        .enqueue_ok(200, BODY.to_vec());
    let (client, _dir) = client(mock);

    let got = resolve(&client, "registry:acme/greet@0.1.0").expect("signed resolves");
    assert!(got.signed, "the resolution reports the verified signature");
    let record = client.cache_root.join("keys/acme.pub");
    assert!(record.exists(), "the key anchored TOFU after verifying");
    assert_eq!(
        std::fs::read_to_string(record) // seam-bypass-ok: test harness disk fixtures
            .expect("record reads")
            .trim(),
        pk.trim(),
        "the anchored key is the publisher's"
    );
}

#[test]
fn a_signature_mismatch_refuses_and_writes_nothing() {
    let (pk, sk) = test_keypair();
    // The entry pins BODY's digest but the signature covers OTHER bytes.
    let sig = sign_bytes(&sk, b"other bytes");
    let digest = sha256_hex(BODY);
    let mock = MockHttp::new()
        .enqueue_ok(
            200,
            index_json(&[artifact_json(
                "acme",
                "greet",
                "0.1.0",
                "workflow",
                &digest,
                &[],
            )]),
        )
        .enqueue_ok(
            200,
            signed_entry_toml("acme", "greet", "0.1.0", &digest, &sig, &pk),
        )
        .enqueue_ok(200, BODY.to_vec());
    let (client, _dir) = client(mock);

    let err = resolve(&client, "registry:acme/greet@0.1.0").expect_err("refused");
    assert_eq!(err.code(), Some("NIKA-REG-006"));
    assert!(
        !client
            .cache_root
            .join("acme/greet/0.1.0.nika.yaml")
            .exists(),
        "nothing was written"
    );
}

#[test]
fn a_rekeyed_publisher_is_refused_by_the_tofu_record() {
    let (pk1, sk1) = test_keypair();
    let (pk2, sk2) = test_keypair();
    let digest = sha256_hex(BODY);
    // First fetch anchors pk1 (signed by sk1).
    let sig1 = sign_bytes(&sk1, BODY);
    let mock = MockHttp::new()
        .enqueue_ok(
            200,
            index_json(&[artifact_json(
                "acme",
                "greet",
                "0.1.0",
                "workflow",
                &digest,
                &[],
            )]),
        )
        .enqueue_ok(
            200,
            signed_entry_toml("acme", "greet", "0.1.0", &digest, &sig1, &pk1),
        )
        .enqueue_ok(200, BODY.to_vec());
    let (client1, _dir) = client(mock);
    resolve(&client1, "registry:acme/greet@0.1.0").expect("the first key anchors");

    // A later entry presents a DIFFERENT key for the same publisher —
    // signed correctly BY that key (a real re-key attack), so only the
    // TOFU record can catch it.
    let sig2 = sign_bytes(&sk2, BODY);
    let mock2 = MockHttp::new()
        .enqueue_ok(
            200,
            index_json(&[artifact_json(
                "acme",
                "greet",
                "0.2.0",
                "workflow",
                &digest,
                &[],
            )]),
        )
        .enqueue_ok(
            200,
            signed_entry_toml("acme", "greet", "0.2.0", &digest, &sig2, &pk2),
        )
        .enqueue_ok(200, BODY.to_vec());
    // Same machine: the second client shares the first one's cache root.
    let client2 = RegistryClient::new(mock2, client1.cache_root.clone());
    let err = resolve(&client2, "registry:acme/greet@0.2.0").expect_err("re-key refused");
    assert_eq!(err.code(), Some("NIKA-REG-007"));
    assert!(
        !client1
            .cache_root
            .join("acme/greet/0.2.0.nika.yaml")
            .exists(),
        "nothing was written"
    );
}

#[test]
fn bare_ref_picks_newest_semver_and_writes_the_pin() {
    let body_new = b"nika: greet-new\n";
    let d_old = sha256_hex(BODY);
    let d_new = sha256_hex(body_new);
    let mock = MockHttp::new()
        .enqueue_ok(
            200,
            index_json(&[
                artifact_json("acme", "greet", "0.1.0", "workflow", &d_old, &[]),
                artifact_json("acme", "greet", "0.2.0-rc1", "workflow", &d_new, &[]),
                artifact_json("acme", "greet", "0.2.0", "workflow", &d_new, &[]),
            ]),
        )
        .enqueue_ok(200, entry_toml("acme", "greet", "0.2.0", &d_new))
        .enqueue_ok(200, body_new.to_vec());
    let (client, _dir) = client(mock);

    let got = resolve(&client, "registry:acme/greet").expect("bare ref resolves");
    assert_eq!(got.coordinate, "acme/greet@0.2.0", "stable outranks rc");
    let pin = client.cache_root.join("acme/greet/pin");
    assert_eq!(
        std::fs::read_to_string(&pin).expect("pin written").trim(), // seam-bypass-ok: test harness disk fixtures
        "0.2.0",
        "bare refs pin at resolve time — they never float"
    );
}

#[test]
fn tampered_bytes_refuse_and_write_nothing() {
    let digest = sha256_hex(BODY);
    let mock = MockHttp::new()
        .enqueue_ok(
            200,
            index_json(&[artifact_json(
                "acme",
                "greet",
                "0.1.0",
                "workflow",
                &digest,
                &[],
            )]),
        )
        .enqueue_ok(200, entry_toml("acme", "greet", "0.1.0", &digest))
        .enqueue_ok(200, b"nika: v1\n# tampered payload\n".to_vec());
    let (client, _dir) = client(mock);

    let err = resolve(&client, "registry:acme/greet@0.1.0").expect_err("must refuse");
    assert_eq!(kind_code(&err), Some("NIKA-REG-003"));
    let text = err.to_string();
    assert!(
        text.contains("nothing was written"),
        "teaches the refuse: {text}"
    );
    assert!(
        !client.cache_root.join("acme/greet").exists(),
        "a refused artifact leaves NO cache residue"
    );
}

#[test]
fn advisory_refuses_before_any_bytes_move() {
    let digest = sha256_hex(BODY);
    let mock = MockHttp::new().enqueue_ok(
        200,
        index_json(&[artifact_json(
            "acme",
            "greet",
            "0.1.0",
            "workflow",
            &digest,
            &["NIKA-ADV-2026-0001"],
        )]),
    );
    let (client, _dir) = client(mock.clone());

    let err = resolve(&client, "registry:acme/greet@0.1.0").expect_err("must refuse");
    assert_eq!(kind_code(&err), Some("NIKA-REG-002"));
    assert!(err.to_string().contains("NIKA-ADV-2026-0001"));
    assert_eq!(
        mock.sent_requests().len(),
        1,
        "advisory refuses after the index fetch, BEFORE entry/artifact bytes"
    );
}

#[test]
fn cache_hit_answers_offline_and_reverifies() {
    // Seed the cache through a real resolve…
    let (seeded, dir) = client(happy_mock("acme", "greet", "0.1.0", BODY));
    resolve(&seeded, "registry:acme/greet@0.1.0").expect("seed resolve");

    // …then resolve again with a client whose queue is EMPTY: any
    // request would fail, so success proves zero network.
    let offline_mock = MockHttp::new();
    let offline = RegistryClient::new(offline_mock.clone(), dir.path().join("registry"));
    let got = resolve(&offline, "registry:acme/greet@0.1.0").expect("offline cache hit");
    assert!(!got.fetched);
    assert_eq!(got.sha256, sha256_hex(BODY));
    assert!(
        offline_mock.sent_requests().is_empty(),
        "zero requests on a cache hit"
    );
}

#[test]
fn bare_ref_answers_offline_via_the_pin() {
    let (seeded, dir) = client(happy_mock("acme", "greet", "0.1.0", BODY));
    resolve(&seeded, "registry:acme/greet").expect("seed bare resolve");

    let offline_mock = MockHttp::new();
    let offline = RegistryClient::new(offline_mock.clone(), dir.path().join("registry"));
    let got = resolve(&offline, "registry:acme/greet").expect("pin answers offline");
    assert!(!got.fetched);
    assert!(
        got.pinned,
        "the pin record chose the version, not the network"
    );
    assert_eq!(got.coordinate, "acme/greet@0.1.0");
    assert!(offline_mock.sent_requests().is_empty());
}

#[test]
fn tampered_cache_refuses_with_the_record_law() {
    let (seeded, dir) = client(happy_mock("acme", "greet", "0.1.0", BODY));
    let got = resolve(&seeded, "registry:acme/greet@0.1.0").expect("seed resolve");
    std::fs::write(&got.path, "nika: v1\n# locally tampered\n").expect("tamper"); // seam-bypass-ok: test harness disk fixtures

    let offline = RegistryClient::new(MockHttp::new(), dir.path().join("registry"));
    let err = resolve(&offline, "registry:acme/greet@0.1.0").expect_err("must refuse");
    assert_eq!(kind_code(&err), Some("NIKA-REG-004"));
    assert!(
        err.to_string().contains("delete"),
        "teaches the heal: {err}"
    );
}

#[test]
fn unknown_index_schema_refuses() {
    let mock = MockHttp::new().enqueue_ok(200, r#"{ "index_schema": 2, "artifacts": [] }"#);
    let (client, _dir) = client(mock);
    let err = resolve(&client, "registry:acme/greet@0.1.0").expect_err("must refuse");
    assert_eq!(kind_code(&err), Some("NIKA-REG-005"));
}

#[test]
fn unresolved_name_fails_loud() {
    let mock = MockHttp::new().enqueue_ok(200, index_json(&[]));
    let (client, _dir) = client(mock);
    let err = resolve(&client, "registry:acme/greet").expect_err("must refuse");
    assert_eq!(kind_code(&err), Some("NIKA-REG-001"), "the slopsquat guard");
}

#[test]
fn missing_version_lists_what_exists() {
    let digest = sha256_hex(BODY);
    let mock = MockHttp::new().enqueue_ok(
        200,
        index_json(&[artifact_json(
            "acme",
            "greet",
            "0.1.0",
            "workflow",
            &digest,
            &[],
        )]),
    );
    let (client, _dir) = client(mock);
    let err = resolve(&client, "registry:acme/greet@9.9.9").expect_err("must refuse");
    assert_eq!(kind_code(&err), Some("NIKA-REG-001"));
    assert!(
        err.to_string().contains("0.1.0"),
        "teaches the published versions: {err}"
    );
}

#[test]
fn non_workflow_artifacts_are_refused_with_their_kind() {
    let digest = sha256_hex(BODY);
    let mock = MockHttp::new().enqueue_ok(
        200,
        index_json(&[artifact_json(
            "acme",
            "greet",
            "0.1.0",
            "skill",
            &digest,
            &[],
        )]),
    );
    let (client, _dir) = client(mock);
    let err = resolve(&client, "registry:acme/greet").expect_err("must refuse");
    assert!(kind_code(&err).is_none());
    assert!(err.to_string().contains("skill"), "names the kind: {err}");
}

#[test]
fn cache_miss_offline_is_honest() {
    // Empty queue → the mock answers HttpError on the index fetch.
    let (client, _dir) = client(MockHttp::new());
    let err = resolve(&client, "registry:acme/greet@0.1.0").expect_err("must refuse");
    assert!(kind_code(&err).is_none());
    let text = err.to_string();
    assert!(
        text.contains("cache") && text.contains("offline"),
        "the offline story is explicit: {text}"
    );
}

#[test]
fn index_entry_digest_disagreement_refuses() {
    let digest = sha256_hex(BODY);
    let lie = sha256_hex(b"a different pin");
    let mock = MockHttp::new()
        .enqueue_ok(
            200,
            index_json(&[artifact_json(
                "acme",
                "greet",
                "0.1.0",
                "workflow",
                &digest,
                &[],
            )]),
        )
        .enqueue_ok(200, entry_toml("acme", "greet", "0.1.0", &lie));
    let (client, _dir) = client(mock);
    let err = resolve(&client, "registry:acme/greet@0.1.0").expect_err("must refuse");
    assert_eq!(kind_code(&err), Some("NIKA-REG-005"));
    assert!(err.to_string().contains("disagree"), "{err}");
}

#[test]
fn unknown_entry_field_is_a_smuggling_channel() {
    let digest = sha256_hex(BODY);
    // Both channels: a top-level key the contract does not name, and
    // a key smuggled into a known table ([integrity]).
    let top_level = entry_toml("acme", "greet", "0.1.0", &digest)
        .replace("[source]", "malicious = \"payload\"\n\n[source]");
    let mut in_table = entry_toml("acme", "greet", "0.1.0", &digest);
    in_table.push_str("smuggled = \"payload\"\n"); // appends inside [integrity]

    for entry in [top_level, in_table] {
        let mock = MockHttp::new()
            .enqueue_ok(
                200,
                index_json(&[artifact_json(
                    "acme",
                    "greet",
                    "0.1.0",
                    "workflow",
                    &digest,
                    &[],
                )]),
            )
            .enqueue_ok(200, entry);
        let (client, _dir) = client(mock);
        let err = resolve(&client, "registry:acme/greet@0.1.0").expect_err("must refuse");
        assert_eq!(kind_code(&err), Some("NIKA-REG-005"));
    }
}

#[test]
fn unknown_entry_schema_refuses() {
    // An entry `schema` the client does not understand is refused, same
    // closed-set law as the index's `index_schema` (ADR-106 REG-005).
    let digest = sha256_hex(BODY);
    let entry = entry_toml("acme", "greet", "0.1.0", &digest).replace("schema = 1", "schema = 2");
    let mock = MockHttp::new()
        .enqueue_ok(
            200,
            index_json(&[artifact_json(
                "acme",
                "greet",
                "0.1.0",
                "workflow",
                &digest,
                &[],
            )]),
        )
        .enqueue_ok(200, entry);
    let (client, _dir) = client(mock);
    let err = resolve(&client, "registry:acme/greet@0.1.0").expect_err("must refuse");
    assert_eq!(kind_code(&err), Some("NIKA-REG-005"));
    assert!(
        err.to_string().contains("schema 1"),
        "teaches what it speaks: {err}"
    );
}

#[test]
fn malformed_source_pins_refuse() {
    let digest = sha256_hex(BODY);
    let mut short_rev = artifact_json("acme", "greet", "0.1.0", "workflow", &digest, &[]);
    short_rev["source"]["rev"] = serde_json::json!("abc123"); // tags/branches are FORBIDDEN
    let mut traversal = artifact_json("acme", "greet", "0.1.0", "workflow", &digest, &[]);
    traversal["source"]["path"] = serde_json::json!("../../etc/passwd");

    for bad in [short_rev, traversal] {
        let mock = MockHttp::new().enqueue_ok(200, index_json(&[bad]));
        let (client, _dir) = client(mock);
        let err = resolve(&client, "registry:acme/greet@0.1.0").expect_err("must refuse");
        assert_eq!(kind_code(&err), Some("NIKA-REG-005"));
    }
}

#[test]
fn oversized_artifacts_refuse() {
    let big = vec![b'x'; MAX_ARTIFACT_BYTES + 1];
    let digest = sha256_hex(&big);
    let mock = MockHttp::new()
        .enqueue_ok(
            200,
            index_json(&[artifact_json(
                "acme",
                "greet",
                "0.1.0",
                "workflow",
                &digest,
                &[],
            )]),
        )
        .enqueue_ok(200, entry_toml("acme", "greet", "0.1.0", &digest))
        .enqueue_ok(200, big);
    let (client, _dir) = client(mock);
    let err = resolve(&client, "registry:acme/greet@0.1.0").expect_err("must refuse");
    assert!(err.to_string().contains("cap"), "{err}");
}

#[test]
fn entry_404_names_the_inconsistency() {
    let digest = sha256_hex(BODY);
    let mock = MockHttp::new()
        .enqueue_ok(
            200,
            index_json(&[artifact_json(
                "acme",
                "greet",
                "0.1.0",
                "workflow",
                &digest,
                &[],
            )]),
        )
        .enqueue_ok(404, "404: Not Found");
    let (client, _dir) = client(mock);
    let err = resolve(&client, "registry:acme/greet@0.1.0").expect_err("must refuse");
    assert!(err.to_string().contains("404"), "{err}");
}

#[test]
fn describe_speaks_fetch_and_cache() {
    let (seeded, dir) = client(happy_mock("acme", "greet", "0.1.0", BODY));
    let fetched = resolve(&seeded, "registry:acme/greet@0.1.0").expect("seed");
    assert!(fetched.describe().contains("fetched + digest verified"));
    assert!(fetched.describe().contains("cached:"));

    let offline = RegistryClient::new(MockHttp::new(), dir.path().join("registry"));
    let hit = resolve(&offline, "registry:acme/greet@0.1.0").expect("hit");
    assert!(hit.describe().contains("cache"));
    assert!(hit.describe().contains("offline"));
}

// -- NEP-0016 · provenance tiers + the operator admission floor --------

/// A signed happy-path mock (index → signed entry → artifact), plus the
/// publisher key the test machine will anchor TOFU.
fn signed_mock(owner: &str, name: &str, version: &str, body: &[u8]) -> (MockHttp, String) {
    let (pk, sk) = test_keypair();
    let sig = sign_bytes(&sk, body);
    let digest = sha256_hex(body);
    let mock = MockHttp::new()
        .enqueue_ok(
            200,
            index_json(&[artifact_json(
                owner,
                name,
                version,
                "workflow",
                &digest,
                &[],
            )]),
        )
        .enqueue_ok(
            200,
            signed_entry_toml(owner, name, version, &digest, &sig, &pk),
        )
        .enqueue_ok(200, body.to_vec());
    (mock, pk)
}

/// Write the operator policy beside the cache (`<root>/policy.toml`).
fn write_policy(client: &RegistryClient<MockHttp>, content: &str) {
    std::fs::create_dir_all(&client.cache_root).expect("cache root"); // seam-bypass-ok: test harness disk fixtures
    std::fs::write(client.cache_root.join("policy.toml"), content).expect("policy writes"); // seam-bypass-ok: test harness disk fixtures
}

/// The on-disk cache record, as raw JSON (the tier/signed assertions).
fn meta_json(client: &RegistryClient<MockHttp>, version: &str) -> serde_json::Value {
    let path = client
        .cache_root
        .join(format!("acme/greet/{version}.meta.json"));
    let raw = std::fs::read_to_string(path).expect("meta reads"); // seam-bypass-ok: test harness disk fixtures
    serde_json::from_str(&raw).expect("meta parses")
}

/// Overwrite the on-disk cache record (the tampered/legacy fixtures).
fn rewrite_meta(client: &RegistryClient<MockHttp>, version: &str, meta: &serde_json::Value) {
    let path = client
        .cache_root
        .join(format!("acme/greet/{version}.meta.json"));
    let body = serde_json::to_string_pretty(meta).expect("meta encodes");
    std::fs::write(path, body).expect("meta writes"); // seam-bypass-ok: test harness disk fixtures
}

#[test]
fn the_ladder_is_closed_and_totally_ordered() {
    assert!(ProvenanceTier::Unprovenanced < ProvenanceTier::Provenanced);
    assert!(ProvenanceTier::Provenanced < ProvenanceTier::StageClear);
    assert!(ProvenanceTier::StageClear < ProvenanceTier::Verified);
    for (raw, want) in [
        ("unprovenanced", ProvenanceTier::Unprovenanced),
        ("provenanced", ProvenanceTier::Provenanced),
        ("stage-clear", ProvenanceTier::StageClear),
        ("verified", ProvenanceTier::Verified),
    ] {
        assert_eq!(ProvenanceTier::parse(raw), Some(want));
        assert_eq!(want.as_str(), raw, "the spelling round-trips");
    }
    for bad in ["", "UNSIGNED", "signed", "stage_clear", "verify", "trusted"] {
        assert_eq!(
            ProvenanceTier::parse(bad),
            None,
            "`{bad}` is outside the closed set — it refuses, never parses"
        );
    }
}

#[test]
fn an_unsigned_fetch_below_a_provenanced_floor_refuses_and_writes_nothing() {
    let mock = happy_mock("acme", "greet", "0.1.0", BODY);
    let (client, _dir) = client(mock.clone());
    write_policy(&client, "version = 1\nfloor = \"provenanced\"\n");

    let err = resolve(&client, "registry:acme/greet@0.1.0").expect_err("must refuse");
    assert_eq!(err.code(), Some("NIKA-REG-008"));
    let text = err.to_string();
    assert!(
        text.contains("nothing was written") && text.contains("policy.toml"),
        "teaches the refusal + the operator surface: {text}"
    );
    assert_eq!(
        mock.sent_requests().len(),
        3,
        "the refusal lands AFTER verification (index + entry + artifact), before the store"
    );
    assert!(
        !client.cache_root.join("acme").exists(),
        "a below-floor fetch leaves NO residue — no artifact, no meta, no pin"
    );
}

#[test]
fn an_unknown_policy_version_or_floor_refuses_closed() {
    // Each row: the policy text + the fragment its teaching text must
    // carry. A typo'd or unversioned floor must never silently no-op.
    let rows: &[(&str, &str)] = &[
        ("version = 2\nfloor = \"unprovenanced\"\n", "version"),
        ("version = 1\nfloor = \"bogus\"\n", "unknown tier"),
        ("version = 1\nflor = \"provenanced\"\n", "unknown key"), // the typo'd floor
        ("version = 1\n", "floor"),                               // missing floor
        ("floor = \"provenanced\"\n", "version"),                 // missing version
    ];
    for (policy, teach) in rows {
        let (client, _dir) = client(happy_mock("acme", "greet", "0.1.0", BODY));
        write_policy(&client, policy);
        let err = resolve(&client, "registry:acme/greet@0.1.0").expect_err(policy);
        assert!(
            err.code().is_none(),
            "a broken operator file is the env class, not a registry refusal: {policy}"
        );
        let text = err.to_string();
        assert!(
            text.contains(teach) && text.contains("policy.toml"),
            "{policy:?} must teach `{teach}` + the file · got: {text}"
        );
    }
}

#[test]
fn a_record_claiming_a_tier_its_evidence_cannot_admit_is_tampered() {
    // Each row: the tier string written over the record + the teaching
    // fragment. The record keeps a digest that still matches — ONLY the
    // tier over-claims (row 3 downgrades against `signed: true`).
    let rows: &[(&str, &str)] = &[
        ("verified", "reserved"),
        ("bogus", "not a known tier"),
        ("unprovenanced", "disagree"),
    ];
    for (raw_tier, teach) in rows {
        let (mock, _pk) = signed_mock("acme", "greet", "0.1.0", BODY);
        let (seeded, dir) = client(mock);
        resolve(&seeded, "registry:acme/greet@0.1.0").expect("seed signed resolve");
        let mut meta = meta_json(&seeded, "0.1.0");
        meta["tier"] = serde_json::json!(raw_tier);
        rewrite_meta(&seeded, "0.1.0", &meta);

        let offline = RegistryClient::new(MockHttp::new(), dir.path().join("registry"));
        let err = resolve(&offline, "registry:acme/greet@0.1.0").expect_err("must refuse");
        assert_eq!(
            err.code(),
            Some("NIKA-REG-004"),
            "a record that over-claims is the tampered class"
        );
        assert!(err.to_string().contains(teach), "teaches: {err}");
    }
}

#[test]
fn a_cache_hit_under_a_tightened_floor_refuses_without_grandfathering() {
    // Fetched honestly under the default floor…
    let (seeded, dir) = client(happy_mock("acme", "greet", "0.1.0", BODY));
    resolve(&seeded, "registry:acme/greet@0.1.0").expect("seed resolve");

    // …then the operator tightens the floor: the SAME hit must refuse.
    let offline_mock = MockHttp::new();
    let offline = RegistryClient::new(offline_mock.clone(), dir.path().join("registry"));
    write_policy(&offline, "version = 1\nfloor = \"provenanced\"\n");
    let err = resolve(&offline, "registry:acme/greet@0.1.0").expect_err("must refuse");
    assert_eq!(err.code(), Some("NIKA-REG-008"));
    let text = err.to_string();
    assert!(
        text.contains("grandfather") && text.contains("delete"),
        "teaches the no-grandfather law + the heal: {text}"
    );
    assert!(
        offline_mock.sent_requests().is_empty(),
        "the refusal happens at the cache — zero network"
    );

    // Deleting the policy restores the honest default: the hit answers.
    std::fs::remove_file(dir.path().join("registry/policy.toml")).expect("policy deletes"); // seam-bypass-ok: test harness disk fixtures
    let got = resolve(&offline, "registry:acme/greet@0.1.0").expect("default floor admits");
    assert_eq!(got.tier, ProvenanceTier::Unprovenanced);
}

#[test]
fn legacy_records_read_their_boolean_as_the_tier() {
    // A pre-NEP-0016 record carries `signed` and no `tier` — simulate it
    // by stripping the field from a freshly written record.
    let (mock, _pk) = signed_mock("acme", "greet", "0.1.0", BODY);
    let (signed_seed, signed_dir) = client(mock);
    resolve(&signed_seed, "registry:acme/greet@0.1.0").expect("seed signed");
    let mut meta = meta_json(&signed_seed, "0.1.0");
    meta.as_object_mut()
        .expect("meta is an object")
        .remove("tier");
    rewrite_meta(&signed_seed, "0.1.0", &meta);

    let offline = RegistryClient::new(MockHttp::new(), signed_dir.path().join("registry"));
    let got = resolve(&offline, "registry:acme/greet@0.1.0").expect("legacy hit");
    assert!(got.signed, "the boolean survives untouched");
    assert_eq!(
        got.tier,
        ProvenanceTier::Provenanced,
        "`signed: true` denotes `provenanced` — no migration"
    );
    assert!(
        got.describe().contains("provenanced"),
        "the hit re-tells the recorded tier: {}",
        got.describe()
    );

    let (unsigned_seed, unsigned_dir) = client(happy_mock("acme", "greet", "0.1.0", BODY));
    resolve(&unsigned_seed, "registry:acme/greet@0.1.0").expect("seed unsigned");
    let mut meta = meta_json(&unsigned_seed, "0.1.0");
    meta.as_object_mut()
        .expect("meta is an object")
        .remove("tier");
    rewrite_meta(&unsigned_seed, "0.1.0", &meta);

    let offline = RegistryClient::new(MockHttp::new(), unsigned_dir.path().join("registry"));
    let got = resolve(&offline, "registry:acme/greet@0.1.0").expect("legacy hit");
    assert!(!got.signed);
    assert_eq!(
        got.tier,
        ProvenanceTier::Unprovenanced,
        "`signed: false` denotes `unprovenanced`"
    );
}

#[test]
fn a_signed_fetch_at_a_provenanced_floor_resolves_and_records_the_tier() {
    let (mock, _pk) = signed_mock("acme", "greet", "0.1.0", BODY);
    let (floored, _dir) = client(mock);
    write_policy(&floored, "version = 1\nfloor = \"provenanced\"\n");

    let got = resolve(&floored, "registry:acme/greet@0.1.0").expect("signed resolves at floor");
    assert_eq!(got.tier, ProvenanceTier::Provenanced);
    assert!(got.signed, "`signed` stays `tier >= provenanced`");
    let meta = meta_json(&floored, "0.1.0");
    assert_eq!(meta["tier"], "provenanced", "the record carries the tier");
    assert_eq!(meta["signed"], true, "…and the legacy boolean beside it");

    // The honest default: no policy file at all — the v0.1 floor admits
    // the unsigned entry, and the record says exactly that.
    let (plain, _dir2) = client(happy_mock("acme", "greet", "0.1.0", BODY));
    let got = resolve(&plain, "registry:acme/greet@0.1.0").expect("default resolves");
    assert_eq!(got.tier, ProvenanceTier::Unprovenanced);
    assert!(!got.signed);
    let meta = meta_json(&plain, "0.1.0");
    assert_eq!(meta["tier"], "unprovenanced");
    assert!(
        got.describe().contains("unprovenanced"),
        "describe() speaks the tier: {}",
        got.describe()
    );
}

#[test]
fn a_below_floor_fetch_anchors_no_tofu_key() {
    // The strict reading of "NOTHING is written": a signed fetch whose
    // tier is below a `stage-clear` floor refuses — and the first-sight
    // TOFU key must NOT anchor either.
    let (mock, _pk) = signed_mock("acme", "greet", "0.1.0", BODY);
    let (client, _dir) = client(mock);
    write_policy(&client, "version = 1\nfloor = \"stage-clear\"\n");

    let err = resolve(&client, "registry:acme/greet@0.1.0").expect_err("must refuse");
    assert_eq!(err.code(), Some("NIKA-REG-008"));
    assert!(
        !client.cache_root.join("keys/acme.pub").exists(),
        "a refused fetch anchors no key — nothing is written, of any kind"
    );
    assert!(!client.cache_root.join("acme").exists());
}

/// ⭐ The grammar (L0) and the ORDERING (here) must never disagree about
/// what a version IS. They are deliberately split — `nika-vocab` answers
/// « is this a version », this crate answers « which one wins » — and a
/// split like that is exactly where two spellings drift apart. This pins
/// them to one verdict, input by input.
#[test]
fn the_two_readers_agree_on_every_version() {
    for v in [
        "1.0.0",
        "1.2.3",
        "1.2.0-rc.1",
        "0.0.1-alpha.2",
        "1.0.0+build",
        "1.0",
        "1.0.0.0",
        "v1.0.0",
        "nightly",
        "1.2.0-",
        "",
        "1.a.0",
    ] {
        assert_eq!(
            super::version_key(v).is_some(),
            nika_vocab::registry_ref::is_plain_semver(v),
            "`{v}` — the grammar and the ordering disagree about validity"
        );
    }
}
