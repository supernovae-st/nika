// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Native acts still use the real Session review and one-use child response.
use super::*;
use crate::model::{Asked, Shape};
use crate::session::feed::Gap;

fn action(beats: &[Beat]) -> (String, Asked) {
    match waits(beats).expect("waiting") {
        Waiting::Action { key, asked } => (key, asked),
        other => panic!("expected native action, got {other:?}"),
    }
}

fn answer(live: &mut Live, key: &str, witness: &str) -> Turn {
    let (busy, _) = std::sync::mpsc::channel();
    let (seen, _) = std::sync::mpsc::sync_channel(8);
    live.answer_bound(
        key,
        witness,
        &busy,
        &Seen::new(seen, Arc::new(Gap::default())),
    )
}

fn keys(asked: &Asked) -> Vec<&str> {
    let Shape::Choice(offers) = &asked.shape else {
        panic!("finite choice")
    };
    offers.iter().map(|offer| offer.key.as_str()).collect()
}

#[test]
fn a_native_run_review_reads_details_without_answering_then_runs_exactly_once() {
    let room = Room::new("native-review");
    let mut live = run_live(&room);
    let (key, asked) = action(&live.submit("run one.nika").beats);
    assert_eq!(key, "run_cost");
    assert_eq!(keys(&asked), ["yes", "no", "details"]);
    assert!(!asked.free_text);
    assert!(live.fresh_input_required());
    assert!(reply_of(&room).is_empty());

    let details = answer(&mut live, "details", &asked.witness);
    assert!(question(&details.beats).contains("challenge nonce-s90"));
    let (_, current) = action(&details.beats);
    assert_ne!(current.witness, asked.witness);
    assert!(reply_of(&room).is_empty());
    let stale = answer(&mut live, "yes", &asked.witness);
    assert!(joined(&stale.beats).contains("Nothing was sent"));
    assert!(reply_of(&room).is_empty());
    // A refused stale gesture repaints the decision and lends a fresh identity.
    let (_, fresh) = action(&stale.beats);
    let result = answer(&mut live, "yes", &fresh.witness);
    assert!(result.handoff.is_none());
    let sent = String::from_utf8(reply_of(&room)).expect("reply");
    assert!(sent.ends_with(r#""yes":true}"#), "{sent}");
    assert_eq!(sent.matches("nika/run-cost-response@1").count(), 1);
    let _ = answer(&mut live, "yes", &fresh.witness);
    assert_eq!(
        String::from_utf8(reply_of(&room)).expect("same reply"),
        sent
    );
}

#[test]
fn native_review_witness_cannot_approve_a_replacement_challenge() {
    let room = Room::new("native-review-replaced");
    let mut live = run_live(&room);
    let (_, asked) = action(&live.submit("run one.nika").beats);
    let replacement = live
        .runtime
        .as_mut()
        .expect("runtime")
        .run_review_asked("another request", "different evidence");
    let refused = answer(&mut live, "yes", &asked.witness);
    assert!(joined(&refused.beats).contains("Nothing was sent"));
    assert!(reply_of(&room).is_empty());
    assert_eq!(
        live.runtime.as_ref().expect("runtime").waiting(),
        work::Waiting::RunReview {
            review: replacement
        }
    );
}

#[test]
fn native_review_cancel_sends_no_response_and_unknown_keys_cannot_approve() {
    let room = Room::new("native-review-cancel");
    let mut live = run_live(&room);
    let (_, asked) = action(&live.submit("run one.nika").beats);
    let refused = answer(&mut live, "yes, please", &asked.witness);
    assert!(joined(&refused.beats).contains("Nothing was sent"));
    assert!(reply_of(&room).is_empty());
    let (_, fresh) = action(&refused.beats);
    let _ = answer(&mut live, "no", &fresh.witness);
    assert!(reply_of(&room).is_empty());
    assert_eq!(
        live.runtime.as_ref().expect("runtime").waiting(),
        work::Waiting::Free
    );
}

fn proposal_live(room: &Room) -> Live {
    let (census, pref) = super::super::intelligence::fixture();
    let mut live = Live::new(
        room.0.clone(),
        census,
        Some(pref),
        None,
        Box::new(|_| Box::new(nika_session::reasoner::NoReasoner)),
        runners(),
    );
    let _ = live.open();
    let _ = live.submit("aggregate-by-key");
    let _ = live.submit("EUR");
    live
}

#[test]
fn a_native_save_names_the_visible_candidate_and_never_runs_it() {
    let room = Room::new("native-save");
    let mut live = proposal_live(&room);
    let (key, asked) = action(&[live.wait()]);
    assert_eq!(key, "save");
    assert_eq!(keys(&asked), ["yes", "save & run", "no", "/show"]);
    assert!(!room.0.join("aggregate-by-key.nika").exists());
    let inspected = answer(&mut live, "/show", &asked.witness);
    assert!(!room.0.join("aggregate-by-key.nika").exists());
    let (_, fresh) = action(&inspected.beats);
    let saved = answer(&mut live, "yes", &fresh.witness);
    assert!(saved.handoff.is_none(), "Save is not Run");
    assert!(room.0.join("aggregate-by-key.nika").exists());
    assert_eq!(live.runtime.as_ref().expect("runtime").work().run, None);
    let repeated = answer(&mut live, "yes", &fresh.witness);
    assert!(joined(&repeated.beats).contains("Nothing was sent"));
}

#[test]
fn a_native_save_without_the_visible_candidate_is_refused() {
    let room = Room::new("native-save-hidden");
    let mut live = proposal_live(&room);
    let (_, asked) = action(&[live.wait()]);
    live.candidate = None;
    let refused = answer(&mut live, "yes", &asked.witness);
    assert!(joined(&refused.beats).contains("Nothing was sent"));
    assert!(!room.0.join("aggregate-by-key.nika").exists());
    assert!(
        live.runtime
            .as_ref()
            .expect("runtime")
            .pending_proposal()
            .is_some()
    );
}

#[test]
fn only_the_finite_missed_schedule_policy_uses_native_actions() {
    let room = Room::new("native-schedule");
    std::fs::create_dir_all(room.0.join("notes")).expect("notes");
    std::fs::write(room.0.join("notes/brief.md"), "brief").expect("brief");
    let (census, pref) = super::super::intelligence::fixture();
    let mut live = Live::new(
        room.0.clone(),
        census,
        Some(pref),
        None,
        Box::new(|_| Box::new(nika_session::reasoner::NoReasoner)),
        runners(),
    );
    let _ = live.open();
    let _ = live.submit("Chaque matin à 8h, lis ./notes/brief.md et écris-le dans ./out/copie.md");
    let _ = live.submit("yes");
    let timezone = live.submit("activate");
    assert!(
        matches!(waits(&timezone.beats), Some(Waiting::Question { key }) if key == "project.timezone")
    );
    let policy = live.submit("Europe/Paris");
    let (key, asked) = action(&policy.beats);
    assert_eq!(key, "project.missed");
    assert_eq!(keys(&asked), ["1", "2", "3", "cancel"]);
    assert!(!asked.free_text);
    let ceiling = answer(&mut live, "2", &asked.witness);
    assert!(
        matches!(waits(&ceiling.beats), Some(Waiting::Question { key }) if key == "project.ceiling")
    );
    assert!(!room.0.join("nika.yaml").exists());
    let stale = answer(&mut live, "3", &asked.witness);
    assert!(joined(&stale.beats).contains("Nothing was sent"));
    assert_eq!(
        live.runtime.as_ref().expect("runtime").pending_activation(),
        Some("project.ceiling")
    );
}

fn gate_live(room: &Room, values: &[&str]) -> (Live, std::path::PathBuf) {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(room, &calls);
    let source = "nika: choose\npermits: { tools: [\"nika:prompt\"] }\ntasks:\n  pick:\n    invoke: { tool: \"nika:prompt\", args: { mode: choice, message: Choose one, choices: [a, b] } }\noutputs:\n  result: ${{ tasks.pick.output }}\n";
    let source = source.replace(
        "choices: [a, b]",
        &format!(
            "choices: {}",
            serde_json::to_string(values).expect("choices")
        ),
    );
    std::fs::write(room.0.join("gate.nika"), source).expect("workflow");
    let runtime = live.runtime.as_mut().expect("runtime");
    assert!(matches!(
        runtime.turn("run gate.nika"),
        TurnOutcome::RunRequested { .. }
    ));
    let trace = room.0.join("paused.ndjson");
    let frame = serde_json::json!({"kind":"workflow_paused","fields":[
        {"key":"task","value":"pick"}, {"key":"mode","value":"choice"},
        {"key":"message","value":"Choose one"},
        {"key":"choices","value":serde_json::to_string(values).expect("options")}
    ]});
    std::fs::write(&trace, frame.to_string()).expect("trace");
    assert!(matches!(
        runtime.observe_run(4, Some(&trace)),
        TurnOutcome::GateAsk { .. }
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    (live, trace)
}

#[test]
fn native_gate_choices_preserve_strings_and_never_interpret_them_as_commands() {
    for value in ["yes", "true", "1", "/quit", "why?", "  spaced  "] {
        let room = Room::new(&format!("native-choice-{}", value.len()));
        let (mut live, trace) = gate_live(&room, &[value, "another"]);
        let (key, asked) = action(&[live.wait()]);
        assert_eq!(key, "gate_choice");
        assert_eq!(asked.label, "Choose one");
        assert!(!asked.free_text);
        let Shape::Choice(offers) = &asked.shape else {
            panic!("choice")
        };
        assert_eq!(offers[0].label, value);
        let wire = serde_json::to_string(value).expect("wire");
        assert_eq!(offers[0].key, wire);
        let result = answer(&mut live, &wire, &asked.witness);
        assert!(
            result.handoff.is_some(),
            "the gesture only requests the resume"
        );
        let Some((
            _,
            Work::Resume {
                trace: sent_trace,
                answer: sent,
                ..
            },
        )) = &live.pending
        else {
            panic!("exact resume request: {:?}", result.beats)
        };
        assert_eq!(sent_trace, &trace);
        assert_eq!(sent, &format!("pick={wire}"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(sent.split_once('=').expect("pair").1)
                .expect("JSON"),
            serde_json::json!(value)
        );
        assert!(
            live.runtime
                .as_ref()
                .expect("runtime")
                .waiting_gate()
                .is_none()
        );
        let repeated = answer(&mut live, &wire, &asked.witness);
        assert!(joined(&repeated.beats).contains("Nothing was sent"));
    }
}

#[test]
fn a_changed_trace_or_an_unlisted_gate_value_never_requests_a_resume() {
    let room = Room::new("native-choice-stale");
    let (mut live, trace) = gate_live(&room, &["a", "b"]);
    let (_, asked) = action(&[live.wait()]);
    let refused = answer(&mut live, "a", &asked.witness);
    assert!(joined(&refused.beats).contains("Nothing was sent"));
    assert!(live.pending.is_none());
    let (_, fresh) = action(&refused.beats);
    let raw = std::fs::read_to_string(&trace).expect("trace");
    std::fs::write(&trace, format!("{raw}\n")).expect("same choices, changed bytes");
    let stale = answer(&mut live, "\"a\"", &fresh.witness);
    assert!(joined(&stale.beats).contains("Nothing was sent"));
    assert!(live.pending.is_none());
    assert!(
        live.runtime
            .as_ref()
            .expect("runtime")
            .waiting_gate()
            .is_some()
    );
}

#[test]
fn an_expired_native_gate_explains_the_extra_answer_without_changing_literal_choices() {
    let room = Room::new("expired-native-choice");
    let (mut live, trace) = gate_live(&room, &["true", "/quit"]);
    let raw = std::fs::read_to_string(&trace).expect("trace");
    let mut frame: serde_json::Value = serde_json::from_str(&raw).expect("frame");
    frame["fields"].as_array_mut().expect("fields").extend([
        serde_json::json!({"key":"approval_shown_hash","value":"shown"}),
        serde_json::json!({"key":"approval_nonce","value":"run"}),
        serde_json::json!({"key":"approval_minted_at_ms","value":0}),
        serde_json::json!({"key":"approval_ttl_seconds","value":900}),
    ]);
    let raw = frame.to_string();
    std::fs::write(&trace, &raw).expect("timed trace");
    let (key, asked) = action(&[live.wait()]);
    assert_eq!(key, "gate_choice");
    assert_eq!(asked.label, "Choose one");
    assert_eq!(
        asked.why,
        "Expired: a fresh question is needed before approval."
    );
    assert_eq!(keys(&asked), ["\"true\"", "\"/quit\""]);
    assert!(live.pending.is_none(), "projection dispatches no resume");
    assert_eq!(std::fs::read_to_string(&trace).expect("same trace"), raw);
}
