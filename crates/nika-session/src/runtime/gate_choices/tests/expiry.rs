// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;

fn timed_pause(mode: &str) -> String {
    json!({"kind":"workflow_paused","fields":[
        {"key":"task","value":"pick"}, {"key":"mode","value":mode},
        {"key":"message","value":"Choose exactly one"},
        {"key":"choices","value":"[\"a\",\"b\"]"},
        {"key":"approval_shown_hash","value":"shown"},
        {"key":"approval_nonce","value":"run"},
        {"key":"approval_minted_at_ms","value":1_000_000},
        {"key":"approval_ttl_seconds","value":900}
    ]})
    .to_string()
}

#[test]
fn expiry_is_read_only_and_bound_to_the_pending_prompt_and_same_trace_bytes() {
    let dir = tempfile::tempdir().expect("root");
    let trace = dir.path().join("paused.ndjson");
    let mut session = ready_with(dir.path(), vec![]);
    for mode in ["confirm", "choice"] {
        let raw = timed_pause(mode);
        std::fs::write(&trace, &raw).expect("trace");
        session.pending_gate = PendingGate::from_trace(std::path::Path::new("gate.nika"), &trace);
        let pending = session.pending_gate.clone();
        let id = session.waiting_gate().expect("gate");
        let digest = nika_event::source_id::sha256_hex(raw.as_bytes());
        assert_eq!(
            session.waiting_gate_expiry_at(1_899_000),
            Some((id.clone(), false, digest.clone()))
        );
        assert_eq!(
            session.waiting_gate_expiry_at(1_899_001),
            Some((id, true, digest))
        );
        assert_eq!(session.pending_gate, pending);
        assert!(session.answered.is_none());
        assert_eq!(std::fs::read_to_string(&trace).expect("trace"), raw);
        std::fs::write(
            &trace,
            raw.replace("Choose exactly one", "Different question"),
        )
        .expect("mismatch");
        assert_eq!(session.waiting_gate_expiry_at(2_000_000), None);
        std::fs::write(&trace, raw.replace(mode, "input")).expect("mode mismatch");
        assert_eq!(session.waiting_gate_expiry_at(2_000_000), None);
    }
}

#[test]
fn missing_or_malformed_timing_stays_unknown_without_answering() {
    let dir = tempfile::tempdir().expect("root");
    let trace = dir.path().join("paused.ndjson");
    let mut session = ready_with(dir.path(), vec![]);
    for raw in [
        paused(&json!(["a", "b"])),
        timed_pause("confirm").replace(":900", ":\"900\""),
    ] {
        std::fs::write(&trace, raw).expect("trace");
        session.pending_gate = PendingGate::from_trace(std::path::Path::new("gate.nika"), &trace);
        assert_eq!(session.waiting_gate_expiry_at(2_000_000), None);
        assert!(session.answered.is_none());
    }
    std::fs::remove_file(trace).expect("remove trace");
    assert_eq!(session.waiting_gate_expiry_at(2_000_000), None);
}
