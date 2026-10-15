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
    let groups = waits(&opened.beats).expect("connections");
    assert!(matches!(groups, Waiting::Selection { .. }), "{opened:?}");
    composer.follow(&groups);
    assert_eq!(composer.offer_selected(&groups), None);
    let connection = choice(&groups)
        .expect("connection offers")
        .2
        .iter()
        .position(|offer| offer.key == "/intelligence models 3 ollama")
        .expect("fixture connection");
    assert!(composer.select_offer(&groups, connection));
    let Offered::Answer(group) = composer.offer_key(&groups, key(KeyCode::Enter)) else {
        panic!("activate connection group")
    };
    let models = waits(&live.submit(&group).beats).expect("models");
    let _ = composer.answer_returned(false);
    composer.follow(&models);
    assert!(matches!(models, Waiting::Selection { .. }));
    assert_eq!(composer.offer_selected(&models), None);
    assert_eq!(
        composer.offer_key(&models, key(KeyCode::Esc)),
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
