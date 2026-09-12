// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Frozen v3 bytes pin the durable reader and writer together (#1467).

use super::*;

const V3: &str = include_str!("fixtures/v3.json");

#[test]
fn version_three_golden_reads_and_roundtrips_without_rewriting_evidence() {
    let decoded = decode_state(V3).expect("frozen v3 fixture validates");
    assert!(!decoded.migrated, "v3 is read without migration");
    assert_eq!(decoded.state.version, 3);
    assert_eq!(decoded.state.incarnation.current.get(), 4);
    assert_eq!(
        decoded
            .state
            .incarnation
            .settled
            .expect("previous owner")
            .get(),
        3
    );
    let writer = decoded.state.writer.as_ref().expect("writer stamp");
    assert_eq!(writer.engine_version, "0.119.0");
    assert_eq!(writer.machine_protocol_version, 1);
    assert_eq!(decoded.state.jobs.len(), 2);
    assert_eq!(decoded.state.jobs[0].record.status(), JobStatus::Succeeded);
    assert_eq!(decoded.state.jobs[1].record.status(), JobStatus::Queued);
    assert_eq!(decoded.state.jobs[0].terminal_sequence, Some(1));
    assert_eq!(decoded.state.jobs[1].terminal_sequence, None);
    assert_eq!(
        prepare_snapshot(&decoded.state).expect("encode").as_str(),
        V3
    );

    let root = tempfile::tempdir().expect("root");
    drop(JobStore::open(root.path()).expect("initialize store"));
    let path = root.path().join("jobs/state.json");
    std::fs::write(&path, V3).expect("install frozen bytes");
    let store = JobStore::open(root.path()).expect("open frozen store");
    for frozen in &decoded.state.jobs {
        let loaded = store
            .get(frozen.record.id())
            .expect("read job")
            .expect("job exists");
        assert_eq!(loaded.status(), frozen.record.status());
        assert_eq!(loaded.receipt(), frozen.record.receipt());
        assert_eq!(loaded.outputs(), frozen.record.outputs());
    }
    assert_eq!(std::fs::read_to_string(path).expect("unchanged store"), V3);
}
