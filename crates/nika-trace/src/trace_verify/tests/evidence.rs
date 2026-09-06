use super::*;

/// A failed file lane can leave this prefix after the primary run succeeds.
/// The journal cannot distinguish that outcome from a crash.
#[test]
fn a_missing_terminal_attests_no_runtime_outcome() -> Result<(), serde_json::Error> {
    let trace = stage(
        "missing-terminal-outcome.ndjson",
        &chained(&["workflow_started", "task_started", "permit_checked"]),
    );
    for json in [false, true] {
        let out = verify_with(
            &trace.to_string_lossy(),
            &VerifyOptions {
                json,
                ..VerifyOptions::default()
            },
        );
        assert_eq!(out.code, super::super::super::exit::INCOMPLETE);
        assert!(!out.text.contains("killed or crashed"), "{}", out.text);
        assert!(!out.text.contains("the dying run"), "{}", out.text);
        assert!(!out.text.contains("the run never settled"), "{}", out.text);
        if json {
            let doc: serde_json::Value = serde_json::from_str(&out.text)?;
            assert_eq!(doc["chain"]["headline"], "incomplete");
            assert_eq!(doc["exit"], super::super::super::exit::INCOMPLETE);
        } else {
            assert!(
                out.text.contains("runtime outcome is unattested"),
                "{}",
                out.text
            );
        }
    }
    let _ = std::fs::remove_file(trace);
    Ok(())
}

/// The process is still alive after releasing its journal lease. The dead
/// liveness tag cannot establish a crash or the primary runtime's outcome.
#[cfg(unix)]
#[test]
fn a_released_lease_does_not_imply_a_failed_or_unsettled_run() -> std::io::Result<()> {
    let trace = stage(
        "released-lease-outcome.ndjson",
        &chained(&["workflow_started", "task_started", "permit_checked"]),
    );
    drop(nika_dap::liveness::hold(&trace)?);
    assert_eq!(
        nika_dap::liveness::probe(&trace),
        nika_dap::liveness::Liveness::Dead {
            pid: std::process::id()
        }
    );
    for json in [false, true] {
        let out = verify_with(
            &trace.to_string_lossy(),
            &VerifyOptions {
                json,
                ..VerifyOptions::default()
            },
        );
        assert_eq!(out.code, super::super::super::exit::INCOMPLETE);
        assert!(
            out.text.contains("writer lease is no longer held"),
            "{}",
            out.text
        );
        assert!(
            out.text.contains("runtime outcome is unattested"),
            "{}",
            out.text
        );
        assert!(!out.text.contains("killed or crashed"), "{}", out.text);
        assert!(!out.text.contains("the run never settled"), "{}", out.text);
        if json {
            let doc: serde_json::Value =
                serde_json::from_str(&out.text).map_err(std::io::Error::other)?;
            assert_eq!(doc["chain"]["headline"], "incomplete");
            assert_eq!(doc["chain"]["liveness"], "dead");
            assert_eq!(doc["exit"], super::super::super::exit::INCOMPLETE);
        }
    }
    nika_dap::liveness::remove_lease(&trace);
    let _ = std::fs::remove_file(trace);
    Ok(())
}

/// Narrower evidence language must keep a complete journal's existing verdict.
#[test]
fn a_complete_journal_keeps_its_verdict() -> Result<(), serde_json::Error> {
    let trace = stage(
        "complete-outcome-control.ndjson",
        &chained(&["workflow_started", "workflow_completed"]),
    );
    for json in [false, true] {
        let out = verify_with(
            &trace.to_string_lossy(),
            &VerifyOptions {
                json,
                ..VerifyOptions::default()
            },
        );
        assert_eq!(out.code, super::super::super::exit::OK);
        if json {
            let doc: serde_json::Value = serde_json::from_str(&out.text)?;
            assert_eq!(doc["chain"]["headline"], "intact");
            assert_eq!(doc["exit"], super::super::super::exit::OK);
        } else {
            assert!(out.text.contains("OK — 2 events"), "{}", out.text);
            assert!(!out.text.contains("INCOMPLETE"), "{}", out.text);
        }
    }
    let _ = std::fs::remove_file(trace);
    Ok(())
}

/// An intentionally appended invalid byte produces the same tail shape as
/// an interrupted write, so neither cause can be inferred from the journal.
#[test]
fn an_invalid_tail_does_not_prove_a_crash_or_exclude_tampering() -> Result<(), serde_json::Error> {
    let raw = format!("{}{{", chained(&["workflow_started", "workflow_completed"]));
    let trace = stage("invalid-tail-cause.ndjson", &raw);
    for json in [false, true] {
        let out = verify_with(
            &trace.to_string_lossy(),
            &VerifyOptions {
                json,
                ..VerifyOptions::default()
            },
        );
        assert_eq!(out.code, super::super::super::exit::OK);
        assert!(!out.text.contains("not tampering"), "{}", out.text);
        assert!(!out.text.contains("a crash mid-write"), "{}", out.text);
        if json {
            let doc: serde_json::Value = serde_json::from_str(&out.text)?;
            assert_eq!(doc["chain"]["headline"], "torn");
            assert_eq!(doc["exit"], super::super::super::exit::OK);
        } else {
            assert!(out.text.contains("cause is unattested"), "{}", out.text);
        }
    }
    let _ = std::fs::remove_file(trace);
    Ok(())
}
