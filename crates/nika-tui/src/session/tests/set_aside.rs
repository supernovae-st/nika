// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Every card can be left (NIK-227, live gauntlet wave 8 r02): the recovery card of a saved
//! workflow whose files changed since its rehearsal offered one act, and neither `Esc`, words of
//! one's own nor `/new` left it; only quitting did. `Esc` now sets a decision's card aside,
//! saying what stays undecided, words typed at that recovery are the person's next line, and
//! `/new` begins a new conversation the card does not come back over.
use super::*;
use crate::model::{Asked, Shape};

/// A conversation over `room` whose deterministic author copies `./notes/brief.md`.
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

/// `text` answered through the card painted as `witness`.
fn answered(live: &mut Live, text: &str, witness: &str) -> Turn {
    let (busy, _) = std::sync::mpsc::channel();
    let (seen, _) = std::sync::mpsc::sync_channel(8);
    let seen = Seen::new(seen, Arc::new(feed::Gap::default()));
    live.answer_bound(text, witness, &busy, &seen)
}

/// The card `beats` leave waiting: the whole wait, its key and its question.
fn card(beats: &[Beat]) -> (Waiting, String, Asked) {
    let waiting = waits(beats).unwrap_or_else(|| panic!("nothing waits: {beats:?}"));
    let Waiting::Action { key, asked } = &waiting else {
        panic!("no card waits: {waiting:?} in {beats:?}")
    };
    (waiting.clone(), key.clone(), asked.clone())
}

/// Whether `beats` give words back to the box as not taken.
fn not_taken(beats: &[Beat]) -> bool {
    beats.iter().any(|beat| matches!(beat, Beat::NotTaken(_)))
}

/// The copy proposed and saved, its source edited after its rehearsal, then `run it`: the run is
/// refused, and the recovery card of the drifted workflow waits.
fn drifted(room: &Room) -> (Live, Waiting, Asked) {
    let mut live = opened(room);
    let proposed = live.submit("Copy ./notes/brief.md as is to ./out/copie.md");
    let (_, key, consent) = card(&proposed.beats);
    assert_eq!(key, "save", "{:?}", proposed.beats);
    let saved = answered(&mut live, "yes", &consent.witness);
    assert!(
        joined(&saved.beats).contains("applied · wrote"),
        "{:?}",
        saved.beats
    );
    std::fs::write(room.0.join("notes/brief.md"), "brief, edited").expect("edited");
    let refused = live.submit("run it");
    assert!(
        joined(&refused.beats).contains("was withdrawn for this attempt"),
        "{:?}",
        refused.beats
    );
    let (waiting, key, asked) = card(&refused.beats);
    assert_eq!(key, "stale_run", "{:?}", refused.beats);
    assert!(live.pending.is_none(), "nothing was run");
    (live, waiting, asked)
}

/// `Esc` sets the recovery card aside: the conversation says the workflow stays not run and why,
/// the free prompt comes back, and the card stays aside while the same drift stands, a run asked
/// again included; nothing was answered and nothing ran.
#[test]
fn esc_sets_the_recovery_card_aside_and_says_what_stays_undecided() {
    let room = Room::new("stale-run-esc");
    let (mut live, waiting, _) = drifted(&room);
    let beats = live.set_aside(&waiting);
    let said = joined(&beats);
    assert!(
        said.contains("`compiled-workflow.nika`") && said.contains("stays not run"),
        "{said}"
    );
    assert_eq!(waits(&beats), Some(Waiting::Free), "{beats:?}");
    let again = live.submit("run it");
    assert!(
        joined(&again.beats).contains("was withdrawn for this attempt"),
        "the run law stands: {:?}",
        again.beats
    );
    assert_eq!(
        waits(&again.beats),
        Some(Waiting::Free),
        "{:?}",
        again.beats
    );
    assert!(live.pending.is_none(), "nothing ran");
    assert!(!room.0.join("out/copie.md").exists(), "nothing was written");
}

/// Words typed at the recovery card are the person's next line, never an answer « not taken »
/// (r02 typed « lance-le » there): the Session answers it as any line, the card steps aside and
/// does not come back; its own act, picked, does the same.
#[test]
fn words_typed_at_the_recovery_card_are_the_persons_next_line() {
    let room = Room::new("stale-run-words");
    let (mut live, _, asked) = drifted(&room);
    let turn = answered(&mut live, "lance-le", &asked.witness);
    assert!(
        joined(&turn.beats).contains("was withdrawn for this attempt"),
        "the Session's own answer: {:?}",
        turn.beats
    );
    assert!(!not_taken(&turn.beats), "{:?}", turn.beats);
    assert_eq!(waits(&turn.beats), Some(Waiting::Free), "{:?}", turn.beats);
    assert!(live.pending.is_none(), "nothing ran");
    let room = Room::new("stale-run-act");
    let (mut live, _, asked) = drifted(&room);
    let Shape::Choice(offers) = &asked.shape else {
        panic!("a choice: {asked:?}")
    };
    let act = offers.first().expect("its one act").key.clone();
    let turn = answered(&mut live, &act, &asked.witness);
    assert!(!not_taken(&turn.beats), "{:?}", turn.beats);
    assert!(
        !matches!(waits(&turn.beats), Some(Waiting::Action { key, .. }) if key == "stale_run"),
        "the act never brings the same card back: {:?}",
        turn.beats
    );
}

/// `/new` typed at the recovery card begins a new conversation, and the card does not come back
/// over it; the saved workflow stays saved, and nothing ran.
#[test]
fn new_leaves_the_recovery_card() {
    let room = Room::new("stale-run-new");
    let (mut live, _, asked) = drifted(&room);
    let turn = answered(&mut live, "/new", &asked.witness);
    let said = said(&turn.beats);
    let kinds: Vec<Kind> = said.iter().map(|(kind, _)| *kind).collect();
    assert_eq!(kinds, [Kind::Banner, Kind::Notice], "{said:?}");
    assert!(said[1].1.starts_with("New conversation"), "{said:?}");
    assert_eq!(waits(&turn.beats), Some(Waiting::Free), "{:?}", turn.beats);
    assert!(room.0.join("compiled-workflow.nika").is_file(), "saved");
    assert!(live.pending.is_none(), "nothing ran");
}

/// The save decision set aside waits as its plain prompt: nothing was saved, the conversation
/// says in the person's French what still waits, and « oui » at that prompt saves it as the
/// card's own act does.
#[test]
fn a_set_aside_proposal_still_waits_and_saves_on_yes() {
    let room = Room::new("save-set-aside");
    let mut live = opened(&room);
    let proposed = live.submit("Lis ./notes/brief.md et écris-le dans ./out/copie.md");
    let (waiting, key, asked) = card(&proposed.beats);
    assert_eq!(key, "save");
    let beats = live.set_aside(&waiting);
    let said = joined(&beats);
    assert_eq!(
        said,
        format!(
            "mise de côté · rien n'a été répondu · « {} » vous attend toujours",
            asked.label
        )
    );
    assert_eq!(waits(&beats), Some(Waiting::Proposal), "{beats:?}");
    assert!(
        !room.0.join("compiled-workflow.nika").exists(),
        "nothing saved"
    );
    let saved = live.submit("oui");
    assert!(
        joined(&saved.beats).contains("appliqué · écrit"),
        "{:?}",
        saved.beats
    );
    assert!(room.0.join("compiled-workflow.nika").is_file());
}

/// A fresh spending question keeps its card: `Esc` sets nothing aside there (its own cancel or
/// Ctrl+C leaves it, nothing sent), the question still waits for an answer typed after it, and
/// the run's child waiting at its review is not answered.
#[test]
fn a_fresh_spending_question_keeps_its_card() {
    let room = Room::new("run-cost-kept");
    let mut live = run_live(&room);
    let asked = live.submit("run one.nika").beats;
    let (waiting, key, _) = card(&asked);
    assert_eq!(key, "run_cost", "{asked:?}");
    assert!(live.set_aside(&waiting).is_empty(), "nothing set aside");
    assert!(live.fresh_input_required(), "the question still waits");
    let reply = std::fs::read(room.0.join("reply.json")).unwrap_or_default();
    assert!(reply.is_empty(), "the child was not answered");
}
