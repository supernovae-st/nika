// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The captured world's tests (the size-cap split of `snapshot`).

use super::*;

fn valid_snapshot() -> ExecutionSnapshot {
    let root = "root.nika.yaml".to_owned();
    let unit = CapturedUnit::new(
        root.clone(),
        SnapshotUnitKind::Root,
        b"nika: root\npermits:\n  tools: [\"nika:jq\"]\ntasks:\n  value:\n    invoke:\n      tool: nika:jq\n      args: { input: 1, expression: \".\" }\n"
            .to_vec(),
    );
    let units = BTreeMap::from([(root.clone(), unit)]);
    let digest = snapshot_digest(&root, &units);
    ExecutionSnapshot {
        format_version: SNAPSHOT_FORMAT_VERSION,
        root,
        units,
        digest,
    }
}

/// A captured world that names an `mcp:` tool carries the project's MCP
/// registry and pins as units — the admission check inside it sees the
/// configured servers; a world without `mcp:` carries none (#1374).
#[test]
fn the_mcp_registry_rides_the_captured_world_when_a_workflow_names_a_server() {
    let dir = tempfile::tempdir().expect("project");
    std::fs::create_dir_all(dir.path().join(".nika")).expect(".nika");
    std::fs::write(
        dir.path().join(".nika/mcp_servers.json"),
        b"{\"mcp_servers_format\":1,\"servers\":{\"sandbox\":{\"command\":[\"true\"]}}}",
    )
    .expect("registry");
    std::fs::write(
        dir.path().join(".nika/mcp_pins.json"),
        b"{\"sandbox\":{\"echo\":\"0000\"}}",
    )
    .expect("pins");
    std::fs::write(
        dir.path().join("mcp.nika.yaml"),
        b"nika: mcp\npermits:\n  tools: [\"mcp:sandbox/echo\"]\ntasks:\n  call:\n    invoke:\n      tool: \"mcp:sandbox/echo\"\n      args: { text: hi }\n",
    )
    .expect("workflow");
    std::fs::write(
        dir.path().join("plain.nika.yaml"),
        b"nika: plain\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 4 }\n",
    )
    .expect("workflow");
    let project = nika_fs::OwnedDir::open(dir.path()).expect("open");
    let limits = SnapshotLimits::default();
    let with_mcp =
        ExecutionSnapshot::capture(&project, Path::new("mcp.nika.yaml"), limits).expect("captured");
    assert!(
        with_mcp
            .units()
            .any(|u| u.logical_path() == ".nika/mcp_servers.json"),
        "the registry rides the world: {:?}",
        with_mcp
            .units()
            .map(|u| u.logical_path().to_owned())
            .collect::<Vec<_>>()
    );
    assert!(
        with_mcp
            .units()
            .any(|u| u.logical_path() == ".nika/mcp_pins.json")
    );
    with_mcp
        .revalidate(limits)
        .expect("readmits whole with the registry");
    let plain = ExecutionSnapshot::capture(&project, Path::new("plain.nika.yaml"), limits)
        .expect("captured");
    assert!(
        !plain
            .units()
            .any(|u| u.logical_path().starts_with(".nika/")),
        "no mcp: no registry"
    );
}

#[test]
fn public_readmission_revalidates_owned_bytes_without_a_reader() {
    let snapshot = valid_snapshot();
    let digest = snapshot.digest().to_owned();
    let admitted = crate::ExecutionService::default()
        .readmit_snapshot(snapshot)
        .expect("owned snapshot readmits");

    assert_eq!(admitted.snapshot().digest(), digest);
    assert_eq!(admitted.snapshot().root(), "root.nika.yaml");
}

#[test]
fn readmission_refuses_stale_unit_and_aggregate_identities() {
    let mut stale_unit = valid_snapshot();
    stale_unit
        .units
        .get_mut("root.nika.yaml")
        .expect("root")
        .digest = "0".repeat(64);
    assert!(matches!(
        crate::ExecutionService::default().readmit_snapshot(stale_unit),
        Err(ExecutionError::UnitDigestMismatch { .. })
    ));

    let mut stale_world = valid_snapshot();
    stale_world.digest = "f".repeat(64);
    assert!(matches!(
        crate::ExecutionService::default().readmit_snapshot(stale_world),
        Err(ExecutionError::SnapshotDigestMismatch)
    ));
}

#[test]
fn readmission_refuses_an_owned_but_unreachable_unit() {
    let mut snapshot = valid_snapshot();
    let orphan = CapturedUnit::new(
        "orphan.nika.yaml".to_owned(),
        SnapshotUnitKind::Child,
        b"nika: orphan\npermits: {}\ntasks: {}\n".to_vec(),
    );
    snapshot
        .units
        .insert(orphan.logical_path().to_owned(), orphan);
    snapshot.digest = snapshot_digest(&snapshot.root, &snapshot.units);

    assert!(matches!(
        crate::ExecutionService::default().readmit_snapshot(snapshot),
        Err(ExecutionError::SnapshotStructureMismatch)
    ));
}

#[test]
fn readmission_refuses_an_unknown_snapshot_format() {
    let mut snapshot = valid_snapshot();
    snapshot.format_version = SNAPSHOT_FORMAT_VERSION + 1;

    assert!(matches!(
        crate::ExecutionService::default().readmit_snapshot(snapshot),
        Err(ExecutionError::UnsupportedSnapshotFormat { .. })
    ));
}

#[test]
fn encode_round_trip_preserves_owned_bytes_and_refuses_tampering() {
    let snapshot = valid_snapshot();
    let encoded = snapshot.encode().expect("encode");
    let decoded = ExecutionSnapshot::decode(&encoded).expect("decode");
    assert!(same_snapshot(&snapshot, &decoded));
    crate::ExecutionService::default()
        .readmit_snapshot(decoded)
        .expect("readmit encoded world");

    let tampered = encoded.replace(&snapshot.digest, &"a".repeat(64));
    assert!(matches!(
        ExecutionSnapshot::decode(&tampered),
        Err(ExecutionError::SnapshotDigestMismatch)
    ));
    assert!(ExecutionSnapshot::decode("{").is_err());
}

#[test]
fn encoded_snapshot_round_trip_preserves_json_escaped_paths() {
    let root = "quoted\"root.nika.yaml".to_owned();
    let unit = CapturedUnit::new(root.clone(), SnapshotUnitKind::Root, Vec::new());
    let units = BTreeMap::from([(root.clone(), unit)]);
    let snapshot = ExecutionSnapshot {
        format_version: SNAPSHOT_FORMAT_VERSION,
        digest: snapshot_digest(&root, &units),
        root,
        units,
    };

    let encoded = snapshot.encode().expect("encode escaped path");
    let decoded = ExecutionSnapshot::decode(&encoded).expect("decode escaped path");
    assert!(same_snapshot(&snapshot, &decoded));
}

#[test]
fn encoded_envelope_is_bounded_before_json_deserialization() {
    let limits = SnapshotLimits::new(0, 0, 0, 0);
    let encoded_limit = limits.max_encoded_bytes();
    let oversized = " ".repeat(encoded_limit + 1);

    assert_eq!(
        SnapshotLimits::default().max_total_bytes(),
        16 * 1024 * 1024
    );
    assert!(SnapshotLimits::default().max_encoded_bytes() > 16 * 1024 * 1024);

    assert!(matches!(
        ExecutionSnapshot::decode_with_limits(&oversized, limits),
        Err(ExecutionError::EncodedSnapshotSizeLimit { limit })
            if limit == encoded_limit
    ));
}

#[test]
fn root_and_unit_path_metadata_are_independently_bounded() {
    let oversized_root = serde_json::json!({
        "format_version": SNAPSHOT_FORMAT_VERSION,
        "root": "r".repeat(MAX_LOGICAL_PATH_BYTES + 1),
        "digest": "0".repeat(SHA256_HEX_BYTES),
        "units": [],
    })
    .to_string();
    assert!(matches!(
        ExecutionSnapshot::decode(&oversized_root),
        Err(ExecutionError::SnapshotMetadataLimit { field: "root", limit })
            if limit == MAX_LOGICAL_PATH_BYTES
    ));

    let oversized_path = serde_json::json!({
        "format_version": SNAPSHOT_FORMAT_VERSION,
        "root": "root.nika.yaml",
        "digest": "0".repeat(SHA256_HEX_BYTES),
        "units": [{
            "path": "p".repeat(MAX_LOGICAL_PATH_BYTES + 1),
            "kind": 0,
            "digest": "0".repeat(SHA256_HEX_BYTES),
            "bytes_hex": "",
        }],
    })
    .to_string();
    assert!(matches!(
        ExecutionSnapshot::decode(&oversized_path),
        Err(ExecutionError::SnapshotMetadataLimit { field: "unit path", limit })
            if limit == MAX_LOGICAL_PATH_BYTES
    ));
}

#[test]
fn excessive_unit_count_and_hex_fail_with_typed_limits() {
    let unit = serde_json::json!({
        "path": "root.nika.yaml",
        "kind": 0,
        "digest": sha256_hex(&[]),
        "bytes_hex": "",
    });
    let excessive_count = serde_json::json!({
        "format_version": SNAPSHOT_FORMAT_VERSION,
        "root": "root.nika.yaml",
        "digest": "0".repeat(SHA256_HEX_BYTES),
        "units": [unit.clone(), unit],
    })
    .to_string();
    let count_limits = SnapshotLimits::new(0, 1, usize::MAX, usize::MAX);
    assert!(matches!(
        ExecutionSnapshot::decode_with_limits(&excessive_count, count_limits),
        Err(ExecutionError::UnitCountLimit { limit: 1 })
    ));

    let excessive_hex = serde_json::json!({
        "format_version": SNAPSHOT_FORMAT_VERSION,
        "root": "root.nika.yaml",
        "digest": "0".repeat(SHA256_HEX_BYTES),
        "units": [{
            "path": "root.nika.yaml",
            "kind": 0,
            "digest": sha256_hex(&[0, 0]),
            "bytes_hex": "0000",
        }],
    })
    .to_string();
    let body_limits = SnapshotLimits::new(0, 1, 1, usize::MAX);
    assert!(matches!(
        ExecutionSnapshot::decode_with_limits(&excessive_hex, body_limits),
        Err(ExecutionError::UnitSizeLimit { limit: 1, .. })
    ));
}

#[test]
fn malformed_hex_and_digest_metadata_fail_without_truncation() {
    let malformed_hex = serde_json::json!({
        "format_version": SNAPSHOT_FORMAT_VERSION,
        "root": "root.nika.yaml",
        "digest": "0".repeat(SHA256_HEX_BYTES),
        "units": [{
            "path": "root.nika.yaml",
            "kind": 0,
            "digest": sha256_hex(&[0]),
            "bytes_hex": "0G",
        }],
    })
    .to_string();
    assert!(matches!(
        ExecutionSnapshot::decode(&malformed_hex),
        Err(ExecutionError::SnapshotStructureMismatch)
    ));

    let tampered_unit_digest = serde_json::json!({
        "format_version": SNAPSHOT_FORMAT_VERSION,
        "root": "root.nika.yaml",
        "digest": "0".repeat(SHA256_HEX_BYTES),
        "units": [{
            "path": "root.nika.yaml",
            "kind": 0,
            "digest": "f".repeat(SHA256_HEX_BYTES),
            "bytes_hex": "",
        }],
    })
    .to_string();
    assert!(matches!(
        ExecutionSnapshot::decode(&tampered_unit_digest),
        Err(ExecutionError::UnitDigestMismatch { .. })
    ));

    let oversized_digest = serde_json::json!({
        "format_version": SNAPSHOT_FORMAT_VERSION,
        "root": "root.nika.yaml",
        "digest": "f".repeat(SHA256_HEX_BYTES + 1),
        "units": [],
    })
    .to_string();
    assert!(matches!(
        ExecutionSnapshot::decode(&oversized_digest),
        Err(ExecutionError::SnapshotMetadataLimit {
            field: "snapshot digest",
            limit: SHA256_HEX_BYTES,
        })
    ));
}

#[test]
fn encoded_limit_arithmetic_saturates_instead_of_wrapping() {
    let limits = SnapshotLimits::new(usize::MAX, usize::MAX, usize::MAX, usize::MAX);
    assert_eq!(limits.max_encoded_bytes(), usize::MAX);

    let snapshot = valid_snapshot();
    let encoded = snapshot.encode().expect("encode");
    let decoded = ExecutionSnapshot::decode_with_limits(&encoded, limits)
        .expect("saturated limits remain permissive");
    assert!(same_snapshot(&snapshot, &decoded));
}

#[test]
fn encoded_snapshot_limits_fail_with_typed_errors_before_readmission() {
    let snapshot = valid_snapshot();
    let encoded = snapshot.encode().expect("encode");

    let count = SnapshotLimits::new(64, 0, usize::MAX, usize::MAX);
    assert!(matches!(
        ExecutionSnapshot::decode_with_limits(&encoded, count),
        Err(ExecutionError::UnitCountLimit { limit: 0 })
    ));

    let unit = SnapshotLimits::new(64, 1, 1, usize::MAX);
    assert!(matches!(
        ExecutionSnapshot::decode_with_limits(&encoded, unit),
        Err(ExecutionError::UnitSizeLimit { limit: 1, .. })
    ));

    let total = SnapshotLimits::new(64, 1, usize::MAX, 1);
    assert!(matches!(
        ExecutionSnapshot::decode_with_limits(&encoded, total),
        Err(ExecutionError::TotalSizeLimit { limit: 1 })
    ));
}

#[test]
fn aggregate_limit_preflights_every_unit_before_decoding_any_body() {
    let mut snapshot = valid_snapshot();
    let root_bytes = snapshot
        .units
        .get("root.nika.yaml")
        .map(|unit| unit.bytes.len())
        .unwrap_or_default();
    snapshot.units.insert(
        "z.import".to_owned(),
        CapturedUnit::new("z.import".to_owned(), SnapshotUnitKind::Import, vec![7]),
    );
    snapshot.digest = snapshot_digest(&snapshot.root, &snapshot.units);

    let encoded = snapshot.encode().expect("encode");
    let mut wire: serde_json::Value = serde_json::from_str(&encoded).expect("wire json");
    wire["units"][0]["digest"] = serde_json::Value::String("0".repeat(64));
    let tampered = serde_json::to_string(&wire).expect("tampered wire");
    let limits = SnapshotLimits::new(64, 2, usize::MAX, root_bytes);

    assert!(matches!(
        ExecutionSnapshot::decode_with_limits(&tampered, limits),
        Err(ExecutionError::TotalSizeLimit { limit }) if limit == root_bytes
    ));
}

proptest::proptest! {
    #[test]
    fn snapshot_digest_is_independent_of_map_insertion_order(
        left in proptest::collection::vec(proptest::num::u8::ANY, 0..256),
        right in proptest::collection::vec(proptest::num::u8::ANY, 0..256),
    ) {
        let a = CapturedUnit::new("imports/a".to_owned(), SnapshotUnitKind::Import, left);
        let b = CapturedUnit::new("imports/b".to_owned(), SnapshotUnitKind::Import, right);
        let first = BTreeMap::from([
            (a.logical_path().to_owned(), a.clone()),
            (b.logical_path().to_owned(), b.clone()),
        ]);
        let second = BTreeMap::from([
            (b.logical_path().to_owned(), b),
            (a.logical_path().to_owned(), a),
        ]);
        proptest::prop_assert_eq!(
            snapshot_digest("root.nika.yaml", &first),
            snapshot_digest("root.nika.yaml", &second),
        );
    }

    #[test]
    fn dot_segments_do_not_change_logical_identity(
        segments in proptest::collection::vec("[a-z][a-z0-9]{0,7}", 1..8),
    ) {
        let plain = segments.join("/");
        let dotted = format!("./{}", segments.join("/./"));
        proptest::prop_assert_eq!(
            normalize_logical(Path::new(&plain)).expect("plain path"),
            normalize_logical(Path::new(&dotted)).expect("dotted path"),
        );
    }
}

/// ADR-131 · digests are optional attestations on the wire: a body without
/// them decodes to the same snapshot (the engine computes the digests), a
/// body with a wrong one refuses.
#[test]
fn a_wire_body_without_digests_decodes_to_the_same_world() {
    let snapshot = valid_snapshot();
    let encoded = snapshot.encode().expect("encodes");
    let mut wire: serde_json::Value = serde_json::from_str(&encoded).expect("json");
    wire.as_object_mut().expect("object").remove("digest");
    for unit in wire["units"].as_array_mut().expect("units") {
        unit.as_object_mut().expect("unit").remove("digest");
    }
    let bare = ExecutionSnapshot::decode(&wire.to_string()).expect("a digest-less body decodes");
    assert_eq!(
        bare.digest(),
        snapshot.digest(),
        "the engine computed the same digest"
    );
    assert_eq!(
        bare.encode().expect("encodes"),
        encoded,
        "the same canonical world"
    );
    let mut wrong: serde_json::Value = serde_json::from_str(&encoded).expect("json");
    wrong["digest"] = serde_json::Value::String("0".repeat(64));
    assert!(
        matches!(
            ExecutionSnapshot::decode(&wrong.to_string()),
            Err(ExecutionError::SnapshotDigestMismatch)
        ),
        "an attested digest that mismatches refuses"
    );
}
