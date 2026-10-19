// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `/new` and `Ctrl+N` reach the Session's own door: the opening again with the Session's words
//! of what carries over, or its refusal while a decision waits.
use super::*;

/// A conversation over `room` that reads `./notes/brief.md`, its deterministic author lent.
fn opened(room: &Room) -> Live {
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
    live
}

/// `/new` is in the palette's commands, before `/quit`, and begins a new conversation: the
/// banner again, the Session's words of what carries over (the project's workflows, schedules
/// and runs), the rail and the free prompt; the saved workflow stays saved.
#[test]
fn new_begins_a_new_conversation_with_the_sessions_words() {
    let room = Room::new("new-conversation");
    let mut live = opened(&room);
    let commands = live.commands();
    let new = commands.iter().position(|command| command == "/new");
    let quit = commands.iter().position(|command| command == "/quit");
    assert!(
        new.zip(quit).is_some_and(|(new, quit)| new < quit),
        "{commands:?}"
    );
    let proposed = live.submit("Lis ./notes/brief.md et écris-le dans ./out/copie.md");
    let (_, consent) = native_action(&proposed.beats);
    let saved = answer_native(&mut live, "yes", &consent.witness);
    assert!(joined(&saved.beats).contains("applied · wrote"));
    let turn = live.submit("/new");
    let said = said(&turn.beats);
    let kinds: Vec<Kind> = said.iter().map(|(kind, _)| *kind).collect();
    assert_eq!(kinds, [Kind::Banner, Kind::Notice], "{said:?}");
    let words = &said[1].1;
    assert!(
        words.starts_with("Nouvelle conversation")
            && words
                .contains("les workflows, planifications et runs de ce projet restent tels quels"),
        "{words}"
    );
    assert!(turn.beats.iter().any(|beat| matches!(beat, Beat::Rail(_))));
    assert!(matches!(waits(&turn.beats), Some(Waiting::Free)));
    assert!(
        room.0.join("compiled-workflow.nika").is_file(),
        "the workflow stays"
    );
}

/// While a decision waits, `/new` changes nothing: the Session refuses in its own words, through
/// the decision's door as through the general one, and the decision is shown again.
#[test]
fn new_is_refused_while_a_decision_waits() {
    let room = Room::new("new-conversation-refused");
    let mut live = opened(&room);
    let proposed = live.submit("Lis ./notes/brief.md et écris-le dans ./out/copie.md");
    let (key, consent) = native_action(&proposed.beats);
    assert_eq!(key, "save");
    for turn in [
        answer_native(&mut live, "/new", &consent.witness),
        live.submit("/new"),
    ] {
        let said = joined(&turn.beats);
        assert!(
            said.contains("une décision vous attend encore"),
            "the Session's refusal: {said}"
        );
        let Some(Waiting::Action { key, .. }) = waits(&turn.beats) else {
            panic!("the decision is shown again: {:?}", turn.beats)
        };
        assert_eq!(key, "save");
    }
    assert!(
        !room.0.join("compiled-workflow.nika").exists(),
        "nothing saved"
    );
}

/// The waiting native action the beats leave.
fn native_action(beats: &[Beat]) -> (String, crate::model::Asked) {
    match waits(beats).expect("waiting") {
        Waiting::Action { key, asked } => (key, asked),
        other => panic!("expected native action, got {other:?}"),
    }
}

/// `text` answered through the decision painted as `witness`.
fn answer_native(live: &mut Live, text: &str, witness: &str) -> Turn {
    let (busy, _) = std::sync::mpsc::channel();
    let (seen, _) = std::sync::mpsc::sync_channel(8);
    live.answer_bound(
        text,
        witness,
        &busy,
        &crate::session::feed::Seen::new(seen, Arc::new(crate::session::feed::Gap::default())),
    )
}
