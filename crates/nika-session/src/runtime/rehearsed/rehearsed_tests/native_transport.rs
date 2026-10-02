// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Opt-in positive transport: real Session compiler requests to the existing local Peer,
//! real observed rooms, no compiler-result or room-report doubles. The fixture is synthetic;
//! it proves neither model quality nor production endpoint behavior.

use super::*;
use crate::authoring::{AuthoringContext, Reading};
use crate::change::ProjectChangeSet;
use crate::reasoner::{ProviderReasoner, test_transport};
use crate::runtime::inference_tests::wire::{Peer, response};
use crate::runtime::rehearsed::ProofOrigin;
use nika_onboard::compile::{
    CompileRequest, CompileStatus, Strategy, stated_destinations, stated_sources,
};

mod support;
use support::*;

const TEST: &str = "native_transport_adds_a_real_output_after_a_fresh_answer";
const MODEL: &str = "deepseek/deepseek-v4-pro";
const DIRECT: &str = "Read ./in/source.txt and write its bytes unchanged to ./out/copied.txt.";
const ADD: &str = "Keep the existing copy and additionally write a note to ./out/note.txt. Ask me for the note text.";
const NOTE: &str = "This is the additional note.";
const ORIGINAL: &str = "original source\r\nUnicode Ω and template ${{ untouched }}\n";
const BASE: &str = include_str!("native_transport/fixtures/copy.nika");
const ADDITION: &str = include_str!("native_transport/fixtures/add-note.nika");

#[test]
#[ignore = "real local compiler transport and observed rooms: require an explicit bounded grant"]
fn native_transport_adds_a_real_output_after_a_fresh_answer() {
    let case = Case::new();
    let peer = peer();
    let transport = test_transport::install(&peer.url);
    let observed = Arc::new(Calls::default());
    let mut s = session(&case, observed.clone());
    let before = snapshot(&case.project, "before");
    let round = AuthoringRound::new(DIRECT);
    assert_paths(&round, false);
    let copy = s.rehearse_copy(&round);
    record_counts(&s, &peer, &observed, "not-copy");
    assert!(copy.expect("NotCopy").is_none());
    assert_eq!(observed.count(), 0);
    // This is the production Session seated-compile entry, explicitly NativeOnly. Calling it
    // avoids mistaking a deterministic first-pass result from turn() for authored work.
    let seat = s.seat.clone();
    let compiled = s.compile_round(&round, &seat);
    record_counts(&s, &peer, &observed, "initial-compile");
    let out = compiled.expect("native transport");
    assert_eq!(out.provenance.strategy, Some(Strategy::Native));
    assert_eq!(out.status, CompileStatus::Ready);
    let settled = s.settle(round, Reading::Ready(out));
    record_counts(&s, &peer, &observed, "initial-settle");
    let (old, preview) = proposal(settled);
    assert!(preview.contains("observed result of this candidate"));
    assert_native(&s, &old);
    let original = s.pending.clone().expect("original proposal");
    assert_counts(&s, &peer, &observed, 2, 1);
    observed.assert_outputs(0, false);
    unchanged(&case, &before, "original-proposal");

    ask(&mut s, &original, &peer, &observed, "first-question");
    assert_counts(&s, &peer, &observed, 3, 1);
    refuse_old(&mut s, &old, &peer, &observed, "first-stale-consent");
    unchanged(&case, &before, "first-question");
    let cancelled = s.turn("cancel");
    record_counts(&s, &peer, &observed, "cancel");
    assert!(matches!(cancelled, TurnOutcome::Held { ref id, .. } if id == &old));
    assert_eq!(s.pending.as_ref(), Some(&original));
    assert_native(&s, &old);
    assert_counts(&s, &peer, &observed, 3, 1);
    unchanged(&case, &before, "cancel-restored");

    ask(&mut s, &original, &peer, &observed, "second-question");
    assert_counts(&s, &peer, &observed, 4, 1);
    refuse_old(&mut s, &old, &peer, &observed, "second-stale-consent");
    let answered = s.turn(NOTE);
    record_counts(&s, &peer, &observed, "answer");
    let (new, preview) = proposal(answered);
    assert_ne!(new, old);
    assert_native(&s, &new);
    assert!(preview.contains("observed result of this candidate"));
    assert!(
        preview.contains(NOTE),
        "the new preview must show the real note readback"
    );
    assert_counts(&s, &peer, &observed, 6, 2);
    assert_wire(&peer);
    observed.assert_outputs(1, true);
    unchanged(&case, &before, "answered-proposal");
    refuse_old(&mut s, &old, &peer, &observed, "replaced-stale-consent");
    let revised = s.pending.clone().expect("revised proposal");
    let bytes = revised.changes[0].content().to_owned();
    let saved = s.consent_to(&new, "yes");
    record_counts(&s, &peer, &observed, "save-only");
    assert!(matches!(saved, TurnOutcome::Facts(_)), "{saved:?}");
    assert!(
        std::fs::read_to_string(case.project.join(revised.changes[0].path()))
            .expect("saved revised workflow")
            == bytes
    );
    unchanged(&case, &before, "save-only");
    assert_counts(&s, &peer, &observed, 6, 2);
    let quit = s.turn("/quit");
    record_counts(&s, &peer, &observed, "quit");
    assert!(matches!(quit, TurnOutcome::Quit));
    assert!(
        std::fs::read_dir(&case.rooms)
            .expect("owned rooms directory")
            .next()
            .is_none(),
        "rooms drained and removed"
    );
    drop(s);
    drop(transport);
    drop(peer); // The existing Peer joins its owned server thread here.
    room_call(
        &json!({"schema":"native-edit-end/1", "test":TEST, "peer_joined":true, "rooms_empty":true}),
    );
}

fn ask(
    s: &mut SessionRuntime,
    original: &ProjectChangeSet,
    peer: &Peer,
    calls: &Calls,
    stage: &str,
) {
    let result = s.consent(ADD);
    record_counts(s, peer, calls, stage);
    assert!(
        matches!(result, TurnOutcome::Question { ref key, .. } if key == "const.note_text"),
        "{result:?}"
    );
    assert!(s.pending.is_none());
    assert!(s.rehearsals.pending.is_none());
    assert!(s.rehearsals.revising.is_some());
    let round = s.authoring.as_ref().expect("native EDIT question");
    assert_paths(round, true);
    let request = round.request();
    let expected = CompileRequest::edit(original.changes[0].content(), ADD);
    assert_eq!(
        format!("{:?}", request.input),
        format!("{:?}", expected.input)
    );
    assert_eq!(request.original_intent.as_ref(), Some(&original.goal));
    assert!(
        request.plan.is_some(),
        "answer must replay the recorded edit"
    );
    assert!(request.answers.is_empty(), "no inherited path answer");
}

fn assert_paths(round: &AuthoringRound, added: bool) {
    let intent = round.effective_intent();
    assert_eq!(stated_sources(&intent), vec!["./in/source.txt".to_owned()]);
    let mut targets = stated_destinations(&intent);
    targets.sort();
    let mut expected = vec!["./out/copied.txt".to_owned()];
    if added {
        expected.push("./out/note.txt".to_owned());
    }
    assert_eq!(
        targets, expected,
        "all targets were human-stated before any content answer"
    );
}

fn assert_native(s: &SessionRuntime, id: &ProposalId) {
    let (proven, proof) = s.rehearsals.pending.as_ref().expect("live native proof");
    assert_eq!(proven, id);
    assert!(
        proof.origin == ProofOrigin::Native,
        "Copy selection must not replace this witness"
    );
    assert!(crate::runtime::rehearsed::selected(
        s.pending.as_ref().expect("pending native proposal"),
        &proof.witness
    ));
}

fn refuse_old(s: &mut SessionRuntime, old: &ProposalId, peer: &Peer, calls: &Calls, stage: &str) {
    let result = s.consent_to(old, "yes");
    record_counts(s, peer, calls, stage);
    assert!(matches!(result, TurnOutcome::Refusal(_)));
}
