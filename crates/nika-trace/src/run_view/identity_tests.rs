// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Captured bytes fold as the file does, and name the identity they carry:
//! every execution, every start's hash, the terminal word only when a
//! terminal frame is there.

use super::*;

fn id(uuid: &str) -> ExecutionId {
    serde_json::from_value(serde_json::json!({ "uuid": uuid })).expect("an execution")
}

const A: &str = "01a0ef11-0212-70de-a8b3-99de9427fccc";
const B: &str = "01a0ef11-0212-70de-a8b3-99de94270000";

fn frame(exec: &str, kind: &str, fields: &str) -> String {
    format!(
        r#"{{"execution":{{"uuid":"{exec}"}},"fields":[{fields}],"id":{{"uuid":"01a0ef11-03a1-73d9-a2bc-2548bdab1943"}},"kind":"{kind}","run":null,"timestamp":1}}"#
    )
}

#[test]
fn captured_bytes_name_their_executions_starts_and_terminal() {
    let start = |exec, hash: &str| {
        frame(
            exec,
            "workflow_started",
            &format!(
                r#"{{"key":"workflow","value":"w"}},{{"key":"workflow_sha256","value":"{hash}"}}"#
            ),
        )
    };
    let one = [
        start(A, "aa"),
        frame(
            A,
            "workflow_completed",
            r#"{"key":"status","value":"succeeded"}"#,
        ),
    ]
    .join("\n");
    let facts = RunFacts::of(Path::new("t.ndjson"), &one).expect("frames");
    assert_eq!(
        (facts.executions(), facts.unidentified()),
        (&[id(A)][..], 0)
    );
    assert_eq!(facts.starts(), [Some("aa".to_owned())]);
    assert_eq!(
        (facts.workflow_sha256(), facts.terminal()),
        (Some("aa"), Some("succeeded"))
    );

    let mixed = [start(A, "aa"), start(B, "bb"), frame(A, "task_started", "")].join("\n");
    let facts = RunFacts::of(Path::new("t.ndjson"), &mixed).expect("frames");
    assert_eq!(facts.executions(), [id(A), id(B)]);
    assert_eq!(
        facts.starts().len(),
        2,
        "every start is kept, never the last alone"
    );
    assert_eq!(
        facts.terminal(),
        None,
        "no terminal frame is never « done »"
    );
    assert!(RunFacts::of(Path::new("t.ndjson"), "not a frame\n").is_none());
    let anonymous = [
        frame("not-a-uuid", "task_started", ""),
        r#"{"kind":"run_sealed"}"#.to_owned(),
    ];
    let facts = RunFacts::of(Path::new("t.ndjson"), &anonymous.join("\n")).expect("frames");
    assert_eq!(
        (facts.executions().len(), facts.unidentified()),
        (0, 2),
        "never guessed"
    );
}

/// A journal that ends at a gate names its end: paused, never « done ».
#[test]
fn a_journal_ending_at_a_gate_names_its_end_paused() {
    let paused = [
        frame(A, "workflow_started", ""),
        frame(A, "workflow_paused", r#"{"key":"task","value":"ask"}"#),
    ]
    .join("\n");
    let facts = RunFacts::of(Path::new("t.ndjson"), &paused).expect("frames");
    assert_eq!(facts.terminal(), Some("paused"));
}
