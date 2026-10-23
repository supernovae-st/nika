// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A persisted engine pause keeps its actual question in the reopened native picker.
use super::*;
use crate::composer::Composer;
use crate::composer::answer::{Offered, Sent};
use crate::model::{Presentation, Shape, UiState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

const PAUSED: &str = include_str!("fixtures/paused-gate.ndjson");
const WORKFLOW: &str = "nika: gate-keyed\npermits: { tools: [\"nika:prompt\"] }\ntasks:\n  ask:\n    invoke: { tool: \"nika:prompt\", args: { mode: confirm, message: Ship it? } }\noutputs:\n  result: ${{ tasks.ask.output }}\n";

fn persisted_gate(room: &Room, calls: &Arc<AtomicUsize>) -> (PathBuf, PathBuf, String) {
    let home = room.0.join("home");
    std::fs::create_dir_all(&home).expect("home");
    let traces = room.0.join(".nika/traces");
    std::fs::create_dir_all(&traces).expect("traces");
    let trace = traces.join("2026-09-28T12-59-54Z-53b6.ndjson");
    std::fs::write(&trace, PAUSED).expect("real paused engine journal");
    std::fs::write(room.0.join("gate.nika"), WORKFLOW).expect("workflow");
    let mut live = session_live(room, calls);
    let runtime = live.runtime.as_mut().expect("runtime");
    runtime.enable_history(&home).expect("history");
    assert!(matches!(runtime.turn("/status"), TurnOutcome::Facts(_)));
    assert!(matches!(
        runtime.turn("run gate.nika"),
        TurnOutcome::RunRequested { .. }
    ));
    let TurnOutcome::GateAsk { question, .. } = runtime.observe_run(4, Some(&trace)) else {
        panic!("the recorded engine pause asks its question")
    };
    assert!(
        question.contains("« Ship it? »"),
        "initial run view: {question}"
    );
    let saved = nika_session::state::SessionState::load(&room.0)
        .expect("readable state")
        .expect("persisted gate");
    assert!(matches!(
        saved.pending,
        Some(nika_session::state::Pending::Gate { .. })
    ));
    // A fresh observation includes the whole RunFacts view. Reopening owns the
    // PendingGate question plus its lineage notice, not that earlier run report.
    let gate = nika_session::PendingGate::from_trace(Path::new("gate.nika"), &trace)
        .expect("the same paused gate");
    assert_eq!(
        (
            gate.task.as_str(),
            gate.message.as_str(),
            gate.mode.as_str()
        ),
        ("ask", "Ship it?", "confirm")
    );
    let restored = format!(
        "session record restored (.nika/{} · written {})\n{}\n  (no continuation of this run in .nika/traces · the engine's approval still judges your answer)",
        nika_session::state::STATE_FILE,
        saved.updated_at,
        gate.question()
    );
    (home, trace, restored)
}

fn reopened(room: &Room, home: PathBuf, calls: &Arc<AtomicUsize>) -> (Live, Vec<Beat>) {
    let mut census = IntelligenceCensus::empty();
    census.api_keys.push("deepseek".into());
    let preference = UserIntelligencePreference::new(
        IntelligenceKind::Api {
            provider: "deepseek".into(),
        },
        Some(MODEL.into()),
    );
    let counter = Arc::clone(calls);
    let mut live = Live::new(
        room.0.clone(),
        census,
        Some(preference),
        Some(home),
        Box::new(move |_| Box::new(Route(Arc::clone(&counter)))),
        runners(),
    );
    let beats = live.open();
    (live, beats)
}

#[test]
fn a_reopened_gate_keeps_its_exact_question_in_the_native_menu_and_f2() {
    let room = Room::new("reopened-gate");
    let calls = Arc::new(AtomicUsize::new(0));
    let (home, trace, restored) = persisted_gate(&room, &calls);
    let state_path = room.0.join(".nika").join(nika_session::state::STATE_FILE);
    let saved = std::fs::read(&state_path).expect("state bytes");
    let (live, beats) = reopened(&room, home, &calls);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(live.pending.is_none(), "opening requests no run or resume");
    assert_eq!(std::fs::read(&state_path).expect("unchanged state"), saved);
    assert_eq!(
        std::fs::read_to_string(&trace).expect("unchanged trace"),
        PAUSED
    );
    assert_eq!(
        std::fs::read_to_string(room.0.join("gate.nika")).expect("unchanged workflow"),
        WORKFLOW
    );
    let Waiting::Action { key, asked } = waits(&beats).expect("native gate") else {
        panic!("the restored gate has a finite menu")
    };
    assert_eq!(key, "gate");
    assert_eq!(asked.label, "Ship it?");
    assert!(!asked.free_text);
    let Shape::Choice(offers) = &asked.shape else {
        panic!("finite choice")
    };
    assert_eq!(
        offers
            .iter()
            .map(|offer| offer.key.as_str())
            .collect::<Vec<_>>(),
        ["yes", "no", "why?"]
    );
    let owner = said(&beats)
        .into_iter()
        .find_map(|(kind, text)| (kind == Kind::Gate).then_some(text))
        .expect("typed restored gate context");
    assert_eq!(
        owner, restored,
        "the exact restored gate contract is retained"
    );
    for ascii in [false, true] {
        let mut state = UiState::new(Presentation::Workspace, false, (80, 24));
        state.ascii = ascii;
        for beat in beats.clone() {
            state.apply(beat);
        }
        let index = crate::render::question::asked_block(&state).expect("F2 context");
        assert_eq!(state.transcript[index].text, owner);
        let mut composer = Composer::new();
        composer.follow(&state.waiting);
        assert_eq!(composer.offer_selected(&state.waiting), None);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        terminal
            .draw(|frame| crate::render::draw_focus(frame, &state, &composer))
            .expect("draw reopened picker");
        let buffer = terminal.backend().buffer();
        let text = (0..24)
            .map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        for exact in ["Ship it?", "Approve this step", "Refuse this step"] {
            assert!(text.contains(exact), "{exact}: {text}");
        }
    }
}

/// A typed yes or no answers the gate exactly as picking its row does (NIK-194: « oui » and
/// « yes » typed under « answer yes or no » sent nothing): each whole-line word, French or
/// English, leaves as the act of the row it names (an offer, as that row's own `Enter` sends
/// it), and the Session resumes the paused run with the very answer that row gives.
#[test]
fn a_typed_yes_or_no_answers_the_gate_as_its_row_does() {
    let key = |code: KeyCode| KeyEvent::new(code, KeyModifiers::NONE);
    for (words, row, resumed) in [
        ("oui", 0, "ask=true"),
        ("yes", 0, "ask=true"),
        ("Oui !", 0, "ask=true"),
        ("non", 1, "ask=false"),
        ("no", 1, "ask=false"),
        ("No.", 1, "ask=false"),
    ] {
        let typed = gate_answered(words, |composer, gate| {
            for c in words.chars() {
                assert!(composer.own_key(key(KeyCode::Char(c))), "{words:?}: {c:?}");
            }
            composer.offer_key(gate, key(KeyCode::Enter))
        });
        let picked = gate_answered(words, |composer, gate| {
            for _ in 0..=row {
                composer.offer_key(gate, key(KeyCode::Down));
            }
            composer.offer_key(gate, key(KeyCode::Enter))
        });
        assert_eq!(typed, picked, "{words:?} answers as row {row} does");
        assert_eq!(typed.1, resumed, "{words:?}");
    }
}

/// The reopened gate answered by `act` on its native card: the key `Enter` sent, and the answer
/// the paused run resumes with.
fn gate_answered(
    words: &str,
    act: impl Fn(&mut Composer, &Waiting) -> Offered,
) -> (String, String) {
    let room = Room::new("typed-gate");
    let calls = Arc::new(AtomicUsize::new(0));
    let (home, _, _) = persisted_gate(&room, &calls);
    let (mut live, beats) = reopened(&room, home, &calls);
    let gate = waits(&beats).expect("the native gate");
    let Waiting::Action { asked, .. } = &gate else {
        panic!("{words:?}: no native gate: {gate:?}")
    };
    let mut composer = Composer::new();
    composer.set_focused(true);
    composer.follow(&gate);
    let Offered::Answer(sent) = act(&mut composer, &gate) else {
        panic!("{words:?}: Enter sent nothing")
    };
    assert_eq!(
        composer.in_flight(),
        Some(Sent::Offer),
        "{words:?}: an offer"
    );
    let (busy, _) = std::sync::mpsc::channel();
    let (queue, _) = std::sync::mpsc::sync_channel(8);
    let seen = Seen::new(queue, Arc::new(feed::Gap::default()));
    let turn = live.pick_bound(&sent, &asked.witness, &busy, &seen);
    assert!(turn.handoff.is_some(), "{words:?}: {:?}", turn.beats);
    let Some((_, Work::Resume { answer, .. })) = &live.pending else {
        panic!("{words:?}: the paused run is not resumed: {:?}", turn.beats)
    };
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "{words:?}: no model read it"
    );
    (sent, answer.clone())
}

/// `Esc` sets the gate's card aside (NIK-227): the conversation says the run stays paused on its
/// question and nothing was answered, and the gate's own prompt waits; a line beside the gate
/// keeps the card aside, and « oui » at the prompt resumes the run as the card's act does.
#[test]
fn a_gate_set_aside_stays_paused_and_takes_a_yes_at_its_prompt() {
    let room = Room::new("gate-set-aside");
    let calls = Arc::new(AtomicUsize::new(0));
    let (home, _, _) = persisted_gate(&room, &calls);
    let (mut live, beats) = reopened(&room, home, &calls);
    let gate = waits(&beats).expect("the native gate");
    let aside = live.set_aside(&gate);
    let said = joined(&aside);
    assert!(
        said.contains("nothing was answered") && said.contains("\"Ship it?\""),
        "{said}"
    );
    assert_eq!(waits(&aside), Some(Waiting::Gate), "{aside:?}");
    assert!(live.pending.is_none(), "nothing resumed");
    let why = live.submit("pourquoi ?");
    assert_eq!(waits(&why.beats), Some(Waiting::Gate), "{:?}", why.beats);
    assert!(live.pending.is_none(), "a why answers nothing");
    let turn = live.submit("oui");
    assert!(turn.handoff.is_some(), "{:?}", turn.beats);
    let Some((_, Work::Resume { answer, .. })) = &live.pending else {
        panic!("the paused run is not resumed: {:?}", turn.beats)
    };
    assert_eq!(answer, "ask=true");
    assert_eq!(calls.load(Ordering::SeqCst), 0, "no model read it");
}

#[test]
fn a_reopened_gate_with_a_missing_trace_stays_a_notice_without_a_native_action() {
    let room = Room::new("reopened-gate-missing");
    let calls = Arc::new(AtomicUsize::new(0));
    let (home, trace, _) = persisted_gate(&room, &calls);
    std::fs::remove_file(trace).expect("missing paused journal");
    let (live, beats) = reopened(&room, home, &calls);
    assert!(
        live.runtime
            .as_ref()
            .expect("runtime")
            .waiting_gate()
            .is_none()
    );
    assert!(!matches!(waits(&beats), Some(Waiting::Action { .. })));
    assert!(said(&beats).iter().all(|(kind, _)| *kind != Kind::Gate));
    assert!(
        said(&beats)
            .iter()
            .any(|(kind, text)| *kind == Kind::Notice && text.contains("no longer waits"))
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(live.pending.is_none());
}

#[test]
fn an_expired_reopened_confirmation_shows_its_notice_without_resuming() {
    let room = Room::new("reopened-expired-gate");
    let calls = Arc::new(AtomicUsize::new(0));
    let (home, trace, _) = persisted_gate(&room, &calls);
    let mut frames: Vec<serde_json::Value> = PAUSED
        .lines()
        .map(|line| serde_json::from_str(line).expect("frame"))
        .collect();
    let pause = frames
        .iter_mut()
        .find(|f| f["kind"] == "workflow_paused")
        .expect("pause");
    pause["fields"].as_array_mut().expect("fields").extend([
        serde_json::json!({"key":"approval_shown_hash","value":"shown"}),
        serde_json::json!({"key":"approval_nonce","value":"run"}),
        serde_json::json!({"key":"approval_minted_at_ms","value":0}),
        serde_json::json!({"key":"approval_ttl_seconds","value":900}),
    ]);
    let raw = frames
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&trace, &raw).expect("display fixture, never trusted for a run");
    let state =
        std::fs::read(room.0.join(".nika").join(nika_session::state::STATE_FILE)).expect("state");
    let (live, beats) = reopened(&room, home, &calls);
    let Waiting::Action { key, asked } = waits(&beats).expect("native gate") else {
        panic!("confirmation remains native")
    };
    assert_eq!(key, "gate");
    assert_eq!(asked.label, "Ship it?");
    assert_eq!(
        asked.why,
        "Expired: a fresh question is needed before approval."
    );
    assert!(live.pending.is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        std::fs::read_to_string(&trace).expect("unchanged trace"),
        raw
    );
    assert_eq!(
        std::fs::read(room.0.join(".nika").join(nika_session::state::STATE_FILE)).expect("state"),
        state
    );
}
