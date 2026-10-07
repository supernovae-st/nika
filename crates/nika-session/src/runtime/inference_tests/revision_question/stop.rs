// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
use super::*;
#[test]
fn stopping_a_revision_preserves_the_original_and_sends_no_revision_request() {
    let peer = seat(&revised());
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let mut session = session(dir.path(), None);
    let TurnOutcome::Proposal { id, .. } = session.turn(WORK) else {
        panic!("original proposal");
    };
    let original = session.pending.clone().unwrap();
    let before = files(dir.path());
    let calls = peer.bodies().len();
    session.enable_continuous_preparation();
    let stop = session.begin_preparation_turn();
    session.on_activity(std::sync::Arc::new(move |activity| {
        if activity.phase == crate::activity::Phase::Authoring {
            stop.cancel();
        }
    }));
    let outcome = session.consent(CHANGE);
    assert!(
        matches!(outcome, TurnOutcome::Cancelled(_)),
        "the revision must return cancelled"
    );
    assert_eq!(session.pending_proposal(), Some(id.clone()));
    assert!(
        session.pending.as_ref() == Some(&original),
        "the original pending proposal must remain exact"
    );
    assert!(session.withdraw_cancelled_preparation().is_none());
    assert_eq!(session.pending_proposal(), Some(id));
    assert_eq!(
        peer.bodies().len(),
        calls,
        "cancelled before the next transport"
    );
    assert_eq!(files(dir.path()), before);
    assert_eq!(session.intent.goal.as_deref(), Some(WORK));
}
