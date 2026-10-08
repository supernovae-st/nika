// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A profile r2 release candidate named at build time, admitted through the public disk and
//! memory doors against the identity its producer's release record states (never the payload's
//! own declarations), and accounted for kind by kind: every row its manifest counts is held, in
//! its role, with its provenance, its complete body and its contract.
//!
//! Build with `NIKA_KNOWLEDGE_R2_PAYLOAD` (the payload root) and `NIKA_KNOWLEDGE_R2_RECORD` (the
//! producer's release record) and run the ignored tests.

use std::collections::BTreeMap;
use std::path::Path;

use nika_compile_seats::foundry::entry::role;
use nika_compile_seats::foundry::reach::descriptor;
use nika_compile_seats::foundry::release::canonical::jcs_json;
use nika_compile_seats::foundry::release::r2;
use nika_compile_seats::foundry::{ComponentCatalog, ComponentRef};
use serde_json::Value;

use super::{Snapshot, TrustedIdentity};

/// The payload root and the release record named at build time.
fn named() -> (&'static str, &'static str) {
    match (
        option_env!("NIKA_KNOWLEDGE_R2_PAYLOAD"),
        option_env!("NIKA_KNOWLEDGE_R2_RECORD"),
    ) {
        (Some(payload), Some(record)) => (payload, record),
        _ => panic!("NIKA_KNOWLEDGE_R2_PAYLOAD and NIKA_KNOWLEDGE_R2_RECORD name the candidate"),
    }
}

/// The identity the producer's record states: its profile, snapshot and policy.
fn identity(record: &Value) -> TrustedIdentity {
    assert_eq!(record["profile"], r2::PROFILE);
    TrustedIdentity::r2(
        record["snapshot_sha256"].as_str().unwrap(),
        record["policy"]["id"].as_str().unwrap(),
        record["policy"]["sha256"].as_str().unwrap(),
    )
    .unwrap()
}

/// Every file under `root`, by relative path.
fn files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned();
                files.insert(relative, std::fs::read(&path).unwrap());
            }
        }
    }
    files
}

/// The candidate admitted on disk and in memory: the same snapshot, its record's.
fn admitted() -> (Snapshot, Value) {
    let (payload, record) = named();
    let record: Value = serde_json::from_slice(&std::fs::read(record).unwrap()).unwrap();
    let identity = identity(&record);
    let root = Path::new(payload);
    let on_disk = Snapshot::open(root, Some(&identity)).unwrap();
    let in_memory = Snapshot::from_files("candidate", files(root), Some(&identity)).unwrap();
    assert_eq!(
        on_disk.manifest_sha256(),
        record["snapshot_sha256"].as_str().unwrap()
    );
    assert_eq!(in_memory.manifest_sha256(), on_disk.manifest_sha256());
    assert_eq!(in_memory.rows_sha256(), on_disk.rows_sha256());
    assert_eq!(on_disk.profile(), r2::PROFILE);
    (on_disk, record)
}

#[test]
#[ignore = "a release candidate: needs NIKA_KNOWLEDGE_R2_PAYLOAD and NIKA_KNOWLEDGE_R2_RECORD at build time"]
fn the_candidate_is_admitted_and_holds_every_row_its_manifest_counts() {
    let (snapshot, record) = admitted();
    let kinds = r2::profile().unwrap().kinds();
    let mut held = 0;
    for kind in kinds {
        let counted = snapshot.manifest["rows"][kind.name()]["count"]
            .as_u64()
            .unwrap();
        let rows = snapshot.rows(kind.stem());
        assert_eq!(rows.len() as u64, counted, "{}", kind.name());
        held += rows.len();
    }
    assert_eq!(
        snapshot.relations.len() as u64,
        snapshot.manifest["relations"]["count"].as_u64().unwrap()
    );
    assert_eq!(
        snapshot.identity()["verification"]["admission"],
        r2::PROFILE
    );
    assert_eq!(
        record["knowledge_version"],
        snapshot.manifest["knowledge_version"]
    );
    assert!(held > 0);
}

#[test]
#[ignore = "a release candidate: needs NIKA_KNOWLEDGE_R2_PAYLOAD and NIKA_KNOWLEDGE_R2_RECORD at build time"]
fn every_candidate_row_reaches_the_consumer_whole_in_its_role() {
    let (snapshot, _) = admitted();
    let kinds = r2::profile().unwrap().kinds();
    let held: usize = kinds
        .iter()
        .map(|kind| snapshot.rows(kind.stem()).len())
        .sum();
    let entries = snapshot.entries();
    assert_eq!(
        entries.len(),
        held - snapshot.rows("source_artifacts").len()
    );
    let mut roles: BTreeMap<&str, usize> = BTreeMap::new();
    let mut unresolved = 0;
    for row in &entries {
        let id = row["id"].as_str().unwrap();
        let role = role(row).unwrap();
        *roles.entry(role).or_default() += 1;
        let reference = snapshot.reference(id).unwrap();
        assert_eq!(reference.kind, row["kind"].as_str().unwrap(), "{id}");
        let text = &reference.text;
        assert!(text.contains(&format!("role: {role}")), "{id}");
        assert!(text.contains(&jcs_json(row)), "the whole contract of {id}");
        if let Some(file) = row["file"].as_str() {
            let body = std::str::from_utf8(&snapshot.files[file]).unwrap();
            assert!(text.contains(body.trim_end()), "the whole body of {id}");
        }
        for source in row["provenance"]["sources"].as_array().unwrap() {
            let source = source.as_str().unwrap();
            match snapshot.row(source) {
                Some(cited) => assert!(text.contains(cited["title"].as_str().unwrap()), "{id}"),
                None => panic!("{id} cites {source}, which admission let through"),
            }
        }
        // A pointer to a row the release does not hold (a skill's family, a pattern's
        // specialization) is a coverage fact, presented as written: never a refusal.
        for pointer in ["/family", "/specializes"] {
            if let Some(target) = row.pointer(pointer).and_then(Value::as_str) {
                unresolved += usize::from(snapshot.row(target).is_none());
            }
        }
        assert!(descriptor(row).contains(&format!("role: {role}")), "{id}");
        let resolved = snapshot.resolve(&ComponentRef::new(id));
        if role == "component" {
            let component = resolved.unwrap();
            assert_eq!(&component.contract, row, "{id}");
            assert_eq!(
                component.holes.len(),
                row["holes"].as_array().unwrap().len(),
                "{id}"
            );
        } else {
            assert!(resolved.is_err(), "{id} is no component");
        }
    }
    for role in [
        "component",
        "case",
        "boundary",
        "method",
        "contract",
        "structure",
        "reference",
    ] {
        assert!(
            roles.get(role).is_some_and(|n| *n > 0),
            "{role} in {roles:?}"
        );
    }
    assert!(unresolved <= entries.len());
}
