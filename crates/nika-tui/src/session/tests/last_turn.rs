// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The real Session and deterministic compiler; only classification and Run observation
//! are injected. No provider socket or model, and no claim of executing the workflow.
use super::*;
use nika_session::turn::{RoutingMethod, TurnAct, TurnClassifier, TurnContext, TurnDecision};

struct Modify;
impl TurnClassifier for Modify {
    fn classify(&mut self, _: &TurnContext, _: &str) -> TurnDecision {
        TurnDecision::new(TurnAct::Modify, RoutingMethod::Model)
    }
}

fn live_for_revision(room: &Room) -> Live {
    std::fs::create_dir(room.0.join("notes")).expect("notes");
    std::fs::write(room.0.join("notes/brief.md"), "unchanged input\n").expect("input");
    let none = UserIntelligencePreference::new(IntelligenceKind::None, None);
    let mut live = Live::new(room.0.clone(), IntelligenceCensus::empty(), Some(none), None,
        Box::new(|_| Box::new(nika_session::reasoner::NoReasoner)), runners())
        .with_run_tapped_observed(Box::new(|_, _, sink| {
            let mut story = nika_display::run_story::RunStory::default();
            story.tell(r#"{"correlation":null,"execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"},"fields":[{"key":"workflow","value":"compiled-workflow"}],"id":{"uuid":"01a0ef11-03a1-73d9-a2bc-2548bdab1943"},"kind":"workflow_started","run":null,"timestamp":1}"#, sink);
            (0, None, story.lines)
        }));
    let _ = live.open();
    live.runtime
        .as_mut()
        .expect("runtime")
        .set_authoring_context(nika_session::authoring::AuthoringContext::default());
    live
}

fn shown_status(beats: &[Beat]) -> &str {
    beats
        .iter()
        .rev()
        .find_map(|b| match b {
            Beat::Status(s) => Some(s.as_str()),
            _ => None,
        })
        .expect("status is shown")
}

#[test]
fn a_failed_saved_revision_is_not_presented_as_the_old_runs_success() {
    let room = Room::new("last-turn");
    let mut live = live_for_revision(&room);
    let proposed = live.submit("Read ./notes/brief.md and write it to ./out/copy.md");
    assert!(
        waits(&proposed.beats) == Some(Waiting::Proposal),
        "deterministic proposal"
    );
    let saved = live.submit("yes");
    assert!(
        joined(&saved.beats).contains("applied · wrote"),
        "Save must finish"
    );
    let before = live
        .inspect("compiled-workflow.nika")
        .expect("saved preview");
    let (busy, _) = std::sync::mpsc::channel();
    let ran = live.submit_with("run compiled-workflow.nika with a ceiling of 0", &busy);
    assert!(
        joined(&ran.beats).contains("run observed · exit 0"),
        "the fixture Run was observed"
    );
    let observed = shown_status(&ran.beats);
    assert!(observed.starts_with("Last Run · "), "{observed}");
    let kept = live
        .runtime
        .as_ref()
        .expect("runtime")
        .kept_run()
        .expect("kept Run");
    let leg = live
        .legs
        .lock()
        .expect("legs")
        .newest()
        .cloned()
        .expect("observed leg");
    live.runtime
        .as_mut()
        .expect("runtime")
        .with_classifier(Box::new(Modify));
    let failed = live.submit("actually write it to ./out/copie-2.md instead");
    assert!(
        joined(&failed.beats).contains("I could not revise"),
        "the real compiler refuses this revision"
    );
    assert!(joined(&failed.beats).contains("the saved workflow is unchanged"));
    assert!(waits(&failed.beats) == Some(Waiting::Free));
    let status = shown_status(&failed.beats);
    assert!(status.starts_with("Latest reply above · Last Run · "));
    assert!(
        status.contains("the run succeeded"),
        "retain the older Run's exact outcome"
    );
    assert!(
        !status.starts_with("Done"),
        "the reply must not inherit the older success"
    );
    let rail = failed
        .beats
        .iter()
        .find_map(|beat| match beat {
            Beat::Rail(rail) => Some(rail.clone()),
            _ => None,
        })
        .expect("rail");
    for width in [40, 80] {
        let (rows, _) = footer_rows(
            [Beat::Rail(rail.clone()), Beat::Status(status.into())],
            width,
        );
        assert!(
            rows[1].starts_with("Latest reply above"),
            "a narrow footer keeps the latest reply first"
        );
    }
    assert!(
        live.candidate().is_none(),
        "a failed revision grants no Save"
    );
    let after = live
        .inspect("compiled-workflow.nika")
        .expect("old preview remains available");
    assert!(
        before == after,
        "the existing workflow preview must not change"
    );
    assert!(live.runtime.as_ref().expect("runtime").kept_run() == Some(kept));
    assert!(
        live.legs.lock().expect("legs").newest() == Some(&leg),
        "Run evidence remains available"
    );
    let facts = live.submit("/status");
    assert!(shown_status(&facts.beats).starts_with("Latest reply above · Last Run · "));
    assert!(!room.0.join("out/copie-2.md").exists());
    assert!(
        std::fs::read_to_string(room.0.join("notes/brief.md")).expect("source")
            == "unchanged input\n"
    );
}

#[test]
fn reply_status_never_replaces_a_pending_save_or_invents_a_failure() {
    let room = Room::new("pending-status");
    let mut live = live_for_revision(&room);
    let _ = live.submit("Read ./notes/brief.md and write it to ./out/copy.md");
    let mut old = KeptRun::new();
    old.exit = Some(0);
    live.kept = Some(Ok(old));
    let shown = live.submit("/show");
    assert!(waits(&shown.beats) == Some(Waiting::Proposal));
    assert!(shown_status(&shown.beats).starts_with("Ready for review"));
    assert!(
        footer::reply_label(&TurnOutcome::Facts("an arbitrary fact".into()))
            == Some("Latest reply above")
    );
    let note = TurnOutcome::Refusal(nika_session::Refusal::new(
        nika_session::RefusalClass::AuthoringRefused,
        "refused",
    ));
    assert!(footer::reply_label(&note) == Some("Latest turn refused"));
    assert!(
        footer::reply_label(&TurnOutcome::Cancelled("stopped".into()))
            == Some("Preparation stopped")
    );
}
