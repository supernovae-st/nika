// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The shared vectors of profile r2 (`tests/knowledge-r2`): byte-identical to the producer's
//! `vectors-r2`, and the INDEX both sides pin.
//!
//! Each vector is admitted with exactly its `SNAPSHOT_SHA256`, or refused with exactly its one
//! code at the step its `requirement` names (its last word). The memory form runs for every
//! vector, and the disk form for every vector that has one.

use nika_compile_seats::foundry::release::r2;
use serde_json::json;

use super::vectors::{expected, identity_for, outcome, payload, vectors_of, write};
use super::{RefusalCode, admit, admit_memory};

/// The INDEX both sides pin: the producer's copy holds the same bytes.
const INDEX_SHA256: &str = "a03f212e87cf46294124063b4b095fd1e91920fe87b199ab69c652ff66d3cc44";

#[test]
fn profile_r2_codes_are_the_contracts_thirty_eight() {
    let (index, _) = vectors_of("knowledge-r2", INDEX_SHA256);
    let words: Vec<&str> = RefusalCode::ALL_R2
        .iter()
        .map(|code| code.as_str())
        .collect();
    assert_eq!(
        words,
        r2::CODES,
        "the r2 rules speak this reader's codes, in order"
    );
    let mut sorted = words.clone();
    sorted.sort_unstable();
    assert_eq!(index["codes"], json!(sorted));
    assert_eq!(index["profile"], r2::PROFILE);
    assert_eq!(index["release_format"], r2::RELEASE_FORMAT);
    for word in r2::CODES {
        assert_eq!(
            RefusalCode::of_word(word).map(RefusalCode::as_str),
            Some(word)
        );
    }
}

#[test]
fn every_shared_r2_vector_gives_its_exact_verdict_in_memory_and_on_disk() {
    let (_, vectors) = vectors_of("knowledge-r2", INDEX_SHA256);
    assert_eq!(vectors.len(), 61, "the pinned vector count");
    let (mut failures, mut memory_runs, mut disk_runs) = (Vec::new(), 0, 0);
    for (name, vector) in &vectors {
        let files = payload(vector, &vectors);
        let identity = identity_for(&vector["expected"], r2::PROFILE);
        let want = expected(vector);
        let memory = outcome(admit_memory(files.clone(), identity.as_ref()));
        memory_runs += 1;
        if memory != want {
            failures.push(format!("{name} (memory): {memory:?}, want {want:?}"));
        }
        // The disk form is defined for Unix descriptors only.
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
            disk_runs += 1;
            if disk != want {
                failures.push(format!("{name} (disk): {disk:?}, want {want:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(memory_runs, 61);
    assert_eq!(disk_runs, if cfg!(unix) { 59 } else { 0 });
}
