// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The registry-client test suite — the trust chain exercised over the
//! mock HTTP seam (`src/<mod>/tests.rs` is the prod-LOC-exempt
//! convention · the nika-http `src/tests.rs` precedent). `super`
//! resolves to the registry module — semantics unchanged.

use super::*;
use nika_kernel_mock::MockHttp;

const REV: &str = "0123456789abcdef0123456789abcdef01234567";
const BODY: &[u8] =
    b"nika: v1\nworkflow: greet\ntasks:\n  - id: a\n    exec: { command: [\"echo\", \"hi\"] }\n";

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
}

#[test]
fn bare_ref_picks_newest_semver_and_writes_the_pin() {
    let body_new = b"nika: v1\nworkflow: greet-new\n";
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
