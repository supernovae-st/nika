// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The shared vectors of the r1 contract §11 (`tests/knowledge-r1`). They are byte-identical to
//! the producer's copy, and the INDEX both sides pin.
//!
//! Each vector is admitted with exactly its `SNAPSHOT_SHA256`, or refused with exactly its one
//! code at the step its `requirement` names (its last word). The memory form runs for every
//! vector, and the disk form for every vector that has one.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use nika_event::source_id::sha256_hex;
use serde_json::Value;

use super::profile::safe_relative;
use super::{Admitted, Refusal, RefusalCode, TrustedIdentity, admit, admit_memory};

/// The INDEX both sides pin: the producer's copy holds the same bytes.
const INDEX_SHA256: &str = "cff5cbe23e14fe4dd16d36612c11c0f6bf38bf3487abd7508b27d745c21d9de4";

/// A payload's files by relative path.
pub(super) type Files = BTreeMap<String, Vec<u8>>;

/// An admission as the contract states it: the snapshot admitted, or the one code refused at its
/// step.
pub(super) type Verdict = Result<String, (&'static str, &'static str)>;

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/knowledge-r1")
}

/// A vector file's bytes and value, checked against its INDEX pin.
fn read(name: &str, pin: &str) -> Value {
    let bytes = std::fs::read(dir().join(format!("{name}.json"))).expect("a pinned vector");
    assert_eq!(sha256_hex(&bytes), pin, "{name}: not its INDEX pin");
    serde_json::from_slice(&bytes).expect("a vector is JSON")
}

/// The INDEX, checked against the pin, and every vector it pins by name.
pub(super) fn vectors() -> (Value, BTreeMap<String, Value>) {
    let bytes = std::fs::read(dir().join("INDEX.json")).expect("the INDEX");
    assert_eq!(sha256_hex(&bytes), INDEX_SHA256, "the INDEX both sides pin");
    let index: Value = serde_json::from_slice(&bytes).expect("the INDEX is JSON");
    let vectors = index["vectors"]
        .as_object()
        .expect("vectors by name")
        .iter()
        .map(|(name, pin)| (name.clone(), read(name, pin.as_str().expect("a pin"))))
        .collect();
    (index, vectors)
}

fn bytes_of(entry: &Value) -> Vec<u8> {
    if let Some(text) = entry["text"].as_str() {
        return text.as_bytes().to_vec();
    }
    let hex = entry["hex"].as_str().expect("text or hex");
    (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).expect("hex"))
        .collect()
}

/// A vector's payload: its base's files, minus `removed`, plus its own `files`.
pub(super) fn payload(vector: &Value, vectors: &BTreeMap<String, Value>) -> Files {
    let mut files = Files::new();
    if let Some(base) = vector["base"].as_str() {
        for (path, entry) in vectors[base]["files"].as_object().expect("files") {
            files.insert(path.clone(), bytes_of(entry));
        }
    }
    for removed in vector["removed"].as_array().expect("removed") {
        files.remove(removed.as_str().expect("a path"));
    }
    for (path, entry) in vector["files"].as_object().expect("files") {
        files.insert(path.clone(), bytes_of(entry));
    }
    files
}

/// A positive vector's payload and the identity it is admitted against.
#[cfg_attr(not(unix), allow(dead_code))] // only the constructed cases (Unix) build on it
pub(super) fn positive(name: &str) -> (Files, TrustedIdentity, String) {
    let (_, vectors) = vectors();
    let vector = &vectors[name];
    let identity = TrustedIdentity::from_json(&vector["expected"]).expect("an identity");
    let snapshot = vector["verdict"]["snapshot_sha256"]
        .as_str()
        .expect("an admission")
        .to_owned();
    (payload(vector, &vectors), identity, snapshot)
}

/// Write `files` under `root`, every path safe.
pub(super) fn write(root: &Path, files: &Files) {
    for (path, bytes) in files {
        assert!(safe_relative(path), "{path} is no path to write");
        let at = root.join(path);
        std::fs::create_dir_all(at.parent().expect("a parent")).expect("a directory");
        std::fs::write(at, bytes).expect("a file");
    }
}

/// An admission as a vector states it: the snapshot admitted, or the one code refused, with the
/// step of the admission order that refused it.
pub(super) fn outcome(result: Result<Admitted, Refusal>) -> Verdict {
    result
        .map(|admitted| admitted.manifest_sha256)
        .map_err(|Refusal(step, code, _)| (code.as_str(), step))
}

/// A vector's verdict: its snapshot, or its code and the step its `requirement` names.
fn expected(vector: &Value) -> Result<String, (&str, &str)> {
    let verdict = &vector["verdict"];
    if verdict["admit"] == true {
        return Ok(verdict["snapshot_sha256"]
            .as_str()
            .expect("a snapshot")
            .to_owned());
    }
    let requirement = vector["requirement"].as_str().expect("a requirement");
    let step = requirement.rsplit(' ').next().expect("a step");
    Err((verdict["code"].as_str().expect("a code"), step))
}

#[test]
fn this_readers_codes_are_the_contracts_thirty_eight() {
    let (index, _) = vectors();
    let mut ours: Vec<&str> = RefusalCode::ALL.iter().map(|code| code.as_str()).collect();
    ours.sort_unstable();
    let theirs: Vec<&str> = index["codes"]
        .as_array()
        .expect("codes")
        .iter()
        .map(|code| code.as_str().expect("a code"))
        .collect();
    assert_eq!(ours, theirs);
    assert_eq!(index["profile"], super::ADMISSION_PROFILE);
    assert_eq!(index["release_format"], super::RELEASE_FORMAT);
}

#[test]
fn every_shared_vector_gives_its_exact_verdict_in_memory_and_on_disk() {
    let (_, vectors) = vectors();
    assert_eq!(vectors.len(), 176, "the pinned vector count");
    let mut failures = Vec::new();
    for (name, vector) in &vectors {
        let files = payload(vector, &vectors);
        let identity = TrustedIdentity::from_json(&vector["expected"]);
        let want = expected(vector);
        let memory = outcome(admit_memory(files.clone(), identity.as_ref()));
        if memory != want {
            failures.push(format!("{name} (memory): {memory:?}, want {want:?}"));
        }
        // The disk form is defined for Unix descriptors only (§3.2).
        let on_disk = cfg!(unix)
            && vector["forms"]
                .as_array()
                .expect("forms")
                .iter()
                .any(|form| form == "disk");
        if on_disk {
            let root = tempfile::tempdir().expect("a scratch root");
            write(root.path(), &files);
            let disk = outcome(admit(root.path(), identity.as_ref(), &mut |_, _| {}));
            if disk != want {
                failures.push(format!("{name} (disk): {disk:?}, want {want:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
