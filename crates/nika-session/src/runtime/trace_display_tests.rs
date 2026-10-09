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
    };
    let aside = aside::explain_gate(&gate(absolute.clone()), root.path());
    assert!(
        aside.contains("the trace `.nika/traces/gone.ndjson` holds what ran before it"),
        "{aside}"
    );
    let aside = aside::explain_gate(&gate(elsewhere.to_path_buf()), root.path());
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
