// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Settings are an interruption of a proposal, never an answer to its Save decision.

use super::*;
use crate::composer::Composer;
use crate::composer::answer::{Offered, choice};
use crate::model::Asked;
use crate::session::feed::Gap;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn action(waiting: &Waiting) -> &Asked {
    match waiting {
        Waiting::Action { key, asked } if key == "save" => asked,
        other => panic!("expected native Save decision, got {other:?}"),
    }
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

/// Select the offer `wanted` of the settings list `shown` with a press, apply it with
/// `Enter`, send what it sent, and follow the list that comes back.
fn activate(live: &mut Live, composer: &mut Composer, shown: &Waiting, wanted: &str) -> Waiting {
    let at = choice(shown)
        .expect("settings offers")
        .2
        .iter()
        .position(|offer| offer.key == wanted)
        .unwrap_or_else(|| panic!("{wanted}: {shown:?}"));
    assert!(composer.select_offer(shown, at));
    let Offered::Answer(sent) = composer.offer_key(shown, key(KeyCode::Enter)) else {
        panic!("{wanted} applies")
    };
    assert_eq!(sent, wanted);
    let next = waits(&live.submit(&sent).beats).expect("the next list");
    let _ = composer.answer_returned(false);
    composer.follow(&next);
    next
}

/// The Session keeps the conversation's goal, yet neither a decision nor the intelligence list
/// names it: it is never necessarily the request they answer.
#[test]
fn a_decision_and_the_intelligence_list_name_no_request() {
    let room = Room::new("no-stale-request");
    let mut live = proposal_live(&room);
    let goal = live.runtime.as_ref().expect("runtime").work().request.goal;
    assert_eq!(goal.as_deref(), Some("aggregate-by-key"));
    let waiting = waits(&[live.wait()]).expect("proposal decision");
    assert_eq!(
        action(&waiting).retained,
        None,
        "the decision names no request"
    );
    let home = waits(&live.submit("/intelligence").beats).expect("the picker's home");
    let Waiting::Selection { asked } = &home else {
        panic!("the picker's home is a list")
    };
    assert_eq!(asked.retained, None, "the list names no request");
}

#[test]
fn cancelling_intelligence_keeps_the_proposal_and_draft_but_refreshes_the_action_witness() {
    let room = Room::new("proposal-settings-return");
    let mut live = proposal_live(&room);
    let waiting = waits(&[live.wait()]).expect("proposal decision");
    let old_witness = action(&waiting).witness.clone();
    let runtime = live.runtime.as_ref().expect("runtime");
    let proposal = runtime.pending_proposal().expect("pending proposal");
    let original_kind = runtime.intelligence.kind.clone();
    let original_model = runtime.intelligence.model.clone();

    let mut composer = Composer::new();
    composer.set_focused(true);
    composer.paste("ordinary draft");
    for _ in 0..5 {
        let _ = composer.handle(key(KeyCode::Left));
    }
    composer.follow(&waiting);
    assert_eq!(composer.offer_selected(&waiting), None);
    assert_eq!(
        composer.offer_key(&waiting, key(KeyCode::Enter)),
        Offered::Pass
    );
    let save = choice(&waiting)
        .expect("save offers")
        .2
        .iter()
        .position(|offer| offer.key == "yes")
        .expect("Save offer");
    assert!(composer.select_offer(&waiting, save));

    // Palette commands use Said::Line, so they reach the ordinary Session submit door.
    let opened = live.submit("/intelligence");
    let home = waits(&opened.beats).expect("the picker's home");
    assert!(matches!(home, Waiting::Selection { .. }), "{opened:?}");
    composer.follow(&home);
    let groups = activate(&mut live, &mut composer, &home, "/intelligence connections");
    // Each settings list opens on its first row: browsing, nothing applied.
    assert_eq!(composer.offer_selected(&groups), Some(0));
    let models = activate(
        &mut live,
        &mut composer,
        &groups,
        "/intelligence models 3 ollama",
    );
    assert!(matches!(models, Waiting::Selection { .. }));
    assert_eq!(composer.offer_selected(&models), Some(0));
    // Esc inside an opened connection goes back to the connection list, nothing applied.
    assert_eq!(
        composer.offer_key(&models, key(KeyCode::Esc)),
        Offered::Answer("/intelligence connections".into())
    );
    let connections =
        waits(&live.submit("/intelligence connections").beats).expect("connections again");
    let _ = composer.answer_returned(false);
    composer.follow(&connections);
    assert!(
        choice(&connections)
            .expect("connection offers")
            .2
            .iter()
            .any(|offer| offer.key == "/intelligence models 3 ollama"),
        "{connections:?}"
    );
    // Every connection goes back home; at home, Esc keeps the current choice.
    assert_eq!(
        composer.offer_key(&connections, key(KeyCode::Esc)),
        Offered::Answer("/intelligence".into())
    );
    let home = waits(&live.submit("/intelligence").beats).expect("home again");
    let _ = composer.answer_returned(false);
    composer.follow(&home);
    // The recommendation selected at home is browsing, no choice: one Esc keeps the current one.
    assert_eq!(
        composer.offer_key(&home, key(KeyCode::Esc)),
        Offered::Answer("cancel".into())
    );
    let returned = live.submit("cancel");
    let _ = composer.answer_returned(false);
    let waiting = waits(&returned.beats).expect("proposal decision returns");
    assert_ne!(action(&waiting).witness, old_witness);
    composer.follow(&waiting);
    assert_eq!(composer.offer_selected(&waiting), None);
    assert_eq!(
        composer.offer_key(&waiting, key(KeyCode::Enter)),
        Offered::Pass
    );

    let refused = answer(&mut live, "yes", &old_witness);
    assert!(joined(&refused.beats).contains("Nothing was sent"));
    assert!(refused.handoff.is_none());
    let runtime = live.runtime.as_ref().expect("runtime");
    assert_eq!(runtime.pending_proposal(), Some(proposal.clone()));
    assert_eq!(runtime.intelligence.kind, original_kind);
    assert_eq!(runtime.intelligence.model, original_model);
    assert_eq!(runtime.work().run, None);
    assert_eq!(
        live.candidate.as_ref().expect("visible candidate").id(),
        &proposal
    );
    assert!(!room.0.join("aggregate-by-key.nika").exists());
    assert_eq!(composer.text(), "ordinary draft");
    // Probe the held ordinary editor through its existing API: its caret survived too.
    let _ = composer.handle(key(KeyCode::Char('|')));
    assert_eq!(composer.text(), "ordinary |draft");
}
