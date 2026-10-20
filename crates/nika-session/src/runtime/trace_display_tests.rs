// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A trace is shown as the project holds it, relative to its root, or as `<journal>` when the
//! project does not hold it: never a host's absolute layout. Display only: a run is still read on
//! its real path, and its kept handle is unchanged.

use std::path::{Path, PathBuf};

use nika_onboard::compile::{CompileRequest, compile};

use super::*;
use crate::intelligence::{DataLocus, IntelligenceKind};
use crate::reasoner::NoReasoner;

fn session(root: &Path) -> SessionRuntime {
    let none = ResolvedSessionIntelligence::new(
        IntelligenceKind::None,
        None,
        DataLocus::None,
        false,
        None,
    );
    SessionRuntime::open(root, none, Box::new(NoReasoner))
}

#[test]
fn a_trace_is_shown_relative_to_the_project_or_as_journal() {
    let root = tempfile::tempdir().expect("root");
    let held = root.path().join(".nika/traces/t.ndjson");
    std::fs::create_dir_all(root.path().join(".nika/traces")).expect("traces");
    std::fs::create_dir_all(root.path().join("sub")).expect("sub");
    std::fs::write(&held, "").expect("trace");
    let relative = ".nika/traces/t.ndjson";
    assert_eq!(shown_trace(root.path(), Path::new(relative)), relative);
    assert_eq!(shown_trace(root.path(), &held), relative);
    // Another spelling of the same root (`/var` for `/private/var`, a `..`): its canonical form.
    let spelled = root.path().join("sub").join("..");
    assert_eq!(shown_trace(&spelled, &held), relative);
    let canonical = held.canonicalize().expect("canonical");
    assert_eq!(shown_trace(root.path(), &canonical), relative);
    for elsewhere in [
        PathBuf::from("/elsewhere/.nika/traces/t.ndjson"),
        PathBuf::from("../t.ndjson"),
        PathBuf::from(".nika/../../t.ndjson"),
    ] {
        let shown = shown_trace(root.path(), &elsewhere);
        assert_eq!(shown, "<journal>", "{}", elsewhere.display());
    }
}

/// The run's observation, `/proof` on a trace it cannot read, `details` and a gate's aside name
/// a trace under the root by its project path; a trace elsewhere is `<journal>` in each, and the
/// run keeps its real path as its handle.
#[test]
fn every_host_text_names_the_trace_as_the_project_holds_it() {
    let root = tempfile::tempdir().expect("root");
    let absolute = root.path().join(".nika/traces/gone.ndjson");
    let root_text = root.path().display().to_string();
    let mut s = session(root.path());
    let TurnOutcome::Facts(observed) = s.observe_run(0, Some(&absolute)) else {
        panic!("an observation");
    };
    assert!(
        observed.contains("run observed · exit 0 · succeeded · trace `.nika/traces/gone.ndjson`"),
        "{observed}"
    );
    let TurnOutcome::Facts(proof) = s.turn("/proof") else {
        panic!("a proof line");
    };
    assert!(
        proof.contains(
            "the trace `.nika/traces/gone.ndjson` cannot be read now · `nika trace verify .nika/traces/gone.ndjson`"
        ),
        "the unreadable trace"
    );
    // `details` tells the last run beside the last reading of a workflow.
    let read = compile(&CompileRequest::create(
        "Read ./notes/brief.md and write it to ./out/copy.md",
    ));
    s.last_outcome = Some(read.expect("compiles"));
    let details = s.details();
    assert!(
        details.contains("last run: trace `.nika/traces/gone.ndjson`"),
        "{details}"
    );
    assert_eq!(
        s.last_trace.as_deref(),
        Some(absolute.as_path()),
        "read on its real path"
    );
    for text in [&observed, &proof, &details] {
        assert!(!text.contains(&root_text), "the root text");
    }

    let elsewhere = Path::new("/elsewhere/.nika/traces/t.ndjson");
    let TurnOutcome::Facts(observed) = s.observe_run(1, Some(elsewhere)) else {
        panic!("an observation");
    };
    assert!(observed.contains("· trace `<journal>`"), "{observed}");
    assert!(s.details().contains("last run: trace `<journal>`"));
    assert!(!observed.contains("/elsewhere"), "{observed}");

    let gate = |trace: PathBuf| crate::change::PendingGate {
        workflow: PathBuf::from("flow.nika"),
        trace,
        task: "approve".to_owned(),
        message: "Ship it?".to_owned(),
        mode: "confirm".to_owned(),
        values: None,
        withheld: None,
    };
    let aside = aside::explain_gate(&gate(absolute.clone()), root.path(), Lang::En);
    assert!(
        aside.contains("the trace `.nika/traces/gone.ndjson` holds what ran before it"),
        "{aside}"
    );
    let aside = aside::explain_gate(&gate(elsewhere.to_path_buf()), root.path(), Lang::En);
    assert!(aside.contains("the trace `<journal>` holds"), "{aside}");
}

/// A restored record whose paused trace no longer carries its pause names that trace by its
/// project path.
#[test]
fn a_restored_pause_names_its_trace_as_the_project_holds_it() {
    let root = tempfile::tempdir().expect("root");
    let mut state = crate::state::SessionState::new("2026-10-08T00:00:00Z".to_owned());
    state.pending = Some(crate::state::Pending::Gate {
        workflow: PathBuf::from("flow.nika"),
        trace: root.path().join(".nika/traces/paused.ndjson"),
        task: "approve".to_owned(),
        mode: "confirm".to_owned(),
    });
    state.save(root.path()).expect("a record with a gate");
    let mut s = session(root.path());
    let notice = s.restore_state().expect("restores");
    assert!(
        notice.contains("its trace `.nika/traces/paused.ndjson` carries no pause"),
        "{notice}"
    );
    assert!(
        !notice.contains(&root.path().display().to_string()),
        "{notice}"
    );
}

/// A native run's work names the verifier's seal facts for its local journal, read when the
/// Session observes the run and held beside the kept value, which stays the one an earlier engine
/// reads: a journal that is not a verified chain is neither seal-verified nor signed. A run told
/// no journal names no seal fact, and a served run keeps the facts its own door attested.
#[test]
fn a_native_runs_journal_is_judged_by_the_verifier_when_observed() {
    let root = tempfile::tempdir().expect("root");
    let journal = root.path().join(".nika/traces/t.ndjson");
    std::fs::create_dir_all(root.path().join(".nika/traces")).expect("traces");
    std::fs::write(&journal, "{\"kind\":\"not a journal\"}\n").expect("journal");
    let mut s = session(root.path());
    let _ = s.observe_run(0, Some(&journal));
    let run = s.work().run.expect("a run");
    assert_eq!(
        (run.seal_verified, run.signed),
        (Some(false), Some(false)),
        "the local journal was read and judged"
    );
    let kept = s.kept_run().expect("kept").expect("readable");
    assert_eq!(
        kept.seal_verified, None,
        "the kept value of a native run is unchanged"
    );
    let _ = s.observe_run(0, None);
    let run = s.work().run.expect("a run");
    assert_eq!(
        (run.seal_verified, run.signed),
        (None, None),
        "no journal, no seal fact"
    );
    let served = crate::run_view::KeptRun::new().served("job-1", None, None);
    let _ = s.observe_run_leg(0, Some(&journal), served);
    let run = s.work().run.expect("a run");
    assert_eq!(run.job.as_deref(), Some("job-1"));
    assert_eq!(run.seal_verified, None, "its own door attests a served run");
}

/// An interrupted run whose receipt named its chain head stopped at a wave boundary: `sealed`
/// reads that fact, whatever a verifier says of its journal. An unsigned journal is judged
/// neither seal-verified nor signed, and its run still reads sealed: no verifier's judgment
/// turns a Stop into a cut, and no `sealed` fact reads as a signature.
#[test]
fn an_unsigned_complete_chain_reads_sealed_and_never_signed() {
    let root = tempfile::tempdir().expect("root");
    let journal = root.path().join(".nika/traces/t.ndjson");
    std::fs::create_dir_all(root.path().join(".nika/traces")).expect("traces");
    std::fs::write(&journal, "{\"kind\":\"not a journal\"}\n").expect("journal");
    let mut s = session(root.path());
    let mut leg = crate::run_view::KeptRun::new();
    leg.chain_head = Some("cd".to_owned());
    let _ = s.observe_run_leg(130, Some(&journal), leg);
    let run = s.work().run.expect("a run");
    assert!(run.sealed, "its receipt named the chain head");
    assert_eq!(
        (run.seal_verified, run.signed),
        (Some(false), Some(false)),
        "the verifier judged the journal unsigned"
    );
}

/// An opaque trace identity (a resident's, kept from served history that predates `job`) is never
/// read as a local journal, whatever this root holds under that name: its seal facts stay
/// unknown, never a judged `false`, and `sealed` keeps its own meaning (the receipt named the
/// chain head).
#[test]
fn an_opaque_trace_is_never_read_as_a_local_journal() {
    let root = tempfile::tempdir().expect("root");
    let opaque = "01a11bc6d9d775e3b6f53a184c031914";
    std::fs::write(root.path().join(opaque), "{\"kind\":\"not a journal\"}\n")
        .expect("a file under that name");
    let mut s = session(root.path());
    let mut leg = crate::run_view::KeptRun::new();
    leg.trace = Some(opaque.to_owned());
    leg.trace_opaque = true;
    leg.chain_head = Some("cd".to_owned());
    let _ = s.observe_run_leg(0, None, leg);
    let run = s.work().run.expect("a run");
    assert_eq!(run.trace.as_deref(), Some(opaque));
    assert_eq!(
        (run.seal_verified, run.signed),
        (None, None),
        "an opaque identity is never read here"
    );
    assert!(
        run.sealed,
        "its receipt named the chain head: `sealed` keeps that meaning"
    );
}
