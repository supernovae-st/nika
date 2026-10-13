// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use crate::composer::{Composer, answer::Offered};
use crate::model::{Asked, Shape};
use crate::session::feed::Gap;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};

fn source(ty: &Value) -> String {
    r#"nika: native-input
inputs:
  value: { type: TYPE, required: true }
permits: { fs: { write: ["./result.txt"] }, tools: ["nika:write"] }
tasks:
  write:
    invoke: { tool: "nika:write", args: { path: "./result.txt", content: "value=${{ inputs.value }}" } }
"#
    .replace("TYPE", &ty.to_string())
}

fn start(room: &Room, ty: &Value, calls: &Arc<AtomicUsize>) -> (Live, Waiting) {
    std::fs::write(room.0.join("input.nika"), source(ty)).expect("workflow");
    let mut live = session_live(room, calls);
    let turn = live.submit("run input.nika");
    assert!(turn.handoff.is_none());
    let waiting = waits(&turn.beats).expect("waiting");
    (live, waiting)
}

fn asked(waiting: &Waiting) -> &Asked {
    let Waiting::Action { key, asked } = waiting else {
        panic!("native input: {waiting:?}")
    };
    assert_eq!(key, "run_input");
    asked
}

fn offers(waiting: &Waiting) -> &[crate::model::Offer] {
    let Shape::Choice(offers) = &asked(waiting).shape else {
        panic!("single selection")
    };
    offers
}

fn answer(live: &mut Live, text: &str, witness: &str) -> Turn {
    let (busy, _) = std::sync::mpsc::channel();
    let (seen, _) = std::sync::mpsc::sync_channel(8);
    live.answer_bound(
        text,
        witness,
        &busy,
        &Seen::new(seen, Arc::new(Gap::default())),
    )
}

fn enter() -> KeyEvent {
    KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
}

#[test]
fn required_bool_is_a_native_unselected_choice_and_only_the_selected_value_binds() {
    let room = Room::new("finite-bool");
    let calls = Arc::new(AtomicUsize::new(0));
    let (mut live, waiting) = start(&room, &json!("bool"), &calls);
    assert_eq!(asked(&waiting).label, "Choose a value for value (bool)");
    assert!(!asked(&waiting).free_text);
    assert_eq!(
        offers(&waiting)
            .iter()
            .map(|offer| offer.key.as_str())
            .collect::<Vec<_>>(),
        ["true", "false", "cancel"]
    );
    let mut composer = Composer::default();
    composer.set_focused(true);
    composer.follow(&waiting);
    assert_eq!(composer.offer_selected(&waiting), None);
    assert_eq!(composer.offer_key(&waiting, enter()), Offered::Pass);
    assert!(live.pending.is_none());
    assert_eq!(
        live.runtime.as_ref().expect("runtime").pending_input(),
        Some("value")
    );
    assert!(composer.select_offer(&waiting, 1));
    assert!(live.pending.is_none(), "selection is local");
    let Offered::Answer(key) = composer.offer_key(&waiting, enter()) else {
        panic!("selected answer")
    };
    assert_eq!(key, "false");
    let turn = answer(&mut live, &key, &asked(&waiting).witness);
    assert!(turn.handoff.is_some());
    let Some((_, Work::Run(run))) = &live.pending else {
        panic!("one pending Run")
    };
    assert_eq!(run.vars, ["value=false"]);
    assert!(run.bytes.is_some());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(!room.0.join("result.txt").exists());
}

#[test]
fn native_enum_strings_are_exact_data_and_cancellation_has_a_different_key() {
    for value in ["/quit", "cancel", "true", "1", "  spaced  ", "", "  "] {
        let room = Room::new("finite-enum");
        let calls = Arc::new(AtomicUsize::new(0));
        let (mut live, waiting) = start(&room, &json!({"enum": [value, "other"]}), &calls);
        assert_eq!(asked(&waiting).label, "Choose a value for value (enum)");
        let key = json!(value).to_string();
        let index = offers(&waiting)
            .iter()
            .position(|offer| offer.key == key)
            .expect("literal option");
        let label = if value.trim().is_empty() {
            key.as_str()
        } else {
            value
        };
        assert_eq!(offers(&waiting)[index].label, label);
        assert_eq!(offers(&waiting).last().expect("cancel").key, "cancel");
        let mut composer = Composer::default();
        composer.set_focused(true);
        composer.follow(&waiting);
        assert_eq!(composer.offer_selected(&waiting), None);
        assert!(composer.select_offer(&waiting, index));
        let Offered::Answer(sent) = composer.offer_key(&waiting, enter()) else {
            panic!("selected answer")
        };
        assert_eq!(sent, key);
        let turn = answer(&mut live, &sent, &asked(&waiting).witness);
        assert!(turn.handoff.is_some());
        assert!(!turn.beats.iter().any(|beat| matches!(beat, Beat::Quit)));
        let Some((_, Work::Run(run))) = &live.pending else {
            panic!("Run")
        };
        assert_eq!(run.vars, [format!("value={key}")]);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(!room.0.join("result.txt").exists());
    }
}

#[test]
fn a_changed_source_or_replaced_run_never_accepts_the_old_input_choice() {
    let room = Room::new("finite-stale");
    let calls = Arc::new(AtomicUsize::new(0));
    let (mut live, waiting) = start(&room, &json!("bool"), &calls);
    std::fs::write(
        room.0.join("input.nika"),
        format!("{}\n", source(&json!("bool"))),
    )
    .expect("drift");
    assert_eq!(
        live.runtime
            .as_ref()
            .expect("runtime")
            .waiting_input_choices(),
        None
    );
    let refused = answer(&mut live, "true", &asked(&waiting).witness);
    assert!(refused.handoff.is_none());
    assert!(joined(&refused.beats).contains("Nothing was sent"));
    assert_eq!(
        live.runtime.as_ref().expect("runtime").pending_input(),
        Some("value")
    );
    assert!(live.pending.is_none());
    std::fs::write(room.0.join("input.nika"), source(&json!("bool"))).expect("restore");
    let fresh = waits(&[live.wait()]).expect("repaint");
    let runtime = live.runtime.as_mut().expect("runtime");
    let _ = runtime.turn("cancel");
    let _ = runtime.turn("run input.nika --max-cost-usd 0.7");
    let refused = answer(&mut live, "true", &asked(&fresh).witness);
    assert!(refused.handoff.is_none());
    assert!(joined(&refused.beats).contains("Nothing was sent"));
    assert_eq!(
        live.runtime.as_ref().expect("runtime").pending_input(),
        Some("value")
    );
    assert!(live.pending.is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(!room.0.join("result.txt").exists());
}

#[test]
fn cancel_then_the_identical_run_never_accepts_the_previous_input_presentation() {
    let room = Room::new("finite-identical-run");
    let calls = Arc::new(AtomicUsize::new(0));
    let (mut live, waiting) = start(&room, &json!("bool"), &calls);
    let runtime = live.runtime.as_mut().expect("runtime");
    let first = runtime.waiting_input_choices().expect("first Run input");
    assert!(matches!(runtime.turn("cancel"), TurnOutcome::Facts(_)));
    assert_eq!(runtime.pending_input(), None);
    let again = runtime.turn("run input.nika");
    assert!(matches!(again, TurnOutcome::Question { ref key, .. } if key == "input.value"));
    let current = runtime
        .waiting_input_choices()
        .expect("identical Run input");
    assert_eq!(current.0, first.0);
    assert_eq!(current.1, first.1);
    let before = runtime.kept_turns().to_vec();

    // The old host presentation deliberately remains: no repaint lends a new token.
    let refused = answer(&mut live, "true", &asked(&waiting).witness);
    assert!(refused.handoff.is_none(), "{refused:?}");
    assert!(joined(&refused.beats).contains("Nothing was sent"));
    let runtime = live.runtime.as_ref().expect("runtime");
    assert_eq!(runtime.pending_input(), Some("value"));
    assert_eq!(runtime.kept_turns(), before);
    assert_eq!(runtime.waiting_input_choices(), Some(current.clone()));
    assert_ne!(current.2, first.2, "each admission has its own identity");
    assert!(live.pending.is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(!room.0.join("result.txt").exists());
}

#[test]
fn finite_input_cancel_is_native_and_open_inputs_stay_open() {
    let room = Room::new("finite-cancel");
    let calls = Arc::new(AtomicUsize::new(0));
    let (mut live, waiting) = start(&room, &json!({"enum": ["cancel", "keep"]}), &calls);
    let turn = answer(&mut live, "cancel", &asked(&waiting).witness);
    assert!(turn.handoff.is_none());
    assert_eq!(
        live.runtime.as_ref().expect("runtime").pending_input(),
        None
    );
    assert!(live.pending.is_none());
    for ty in [
        json!("string"),
        json!("integer"),
        json!("number"),
        json!("path"),
    ] {
        let room = Room::new("open-input");
        let (live, waiting) = start(&room, &ty, &calls);
        assert_eq!(waiting, Waiting::Question { key: String::new() });
        assert_eq!(
            live.runtime.as_ref().expect("runtime").pending_input(),
            Some("value")
        );
        assert!(live.action.is_none());
        assert!(live.pending.is_none());
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
