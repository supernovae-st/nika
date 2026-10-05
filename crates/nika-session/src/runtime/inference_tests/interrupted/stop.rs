// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The actual Session worker returns while its loopback provider is still held.
use super::*;
#[test]
fn stop_returns_the_conversation_with_history_and_no_new_proposal() {
    for input in [
        "hello",
        "Je veux que sortie.txt contienne exactement les octets présents dans entree.txt. Budget: 2 USD.",
    ] {
        let peer = HeldPeer::start();
        let root = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("entree.txt"), "input stays unchanged").unwrap();
        let mut session = open_unpriced(root.path());
        session.enable_continuous_preparation();
        session.enable_history(home.path()).unwrap();
        let stop = session.begin_preparation_turn();
        let url = peer.url.clone();
        let (returned, receive) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _wire = test_transport::install(&url);
            let result = session.turn(input);
            returned.send((session, result)).unwrap();
        });
        peer.wait_received();
        stop.cancel();
        let (mut session, result) = receive
            .recv_timeout(Duration::from_secs(3))
            .expect("Stop returns without waiting for the provider");
        worker.join().unwrap();
        assert!(
            matches!(&result, TurnOutcome::Cancelled(text) if text.contains("preparation stopped")),
            "the held turn must return the preparation-stopped notice"
        );
        assert!(session.pending_proposal().is_none());
        assert!(!root.path().join("sortie.txt").exists());
        assert_eq!(peer.requests(), 1);
        assert_eq!(last_history_event(home.path())["effect"], "unknown");
        let observations = session.cost_observations();
        let observed = observations
            .iter()
            .find(|o| o["schema"] == "nika/preparation-cost-observation@1")
            .unwrap();
        assert_eq!(observed["state"], "Uncertain");
        assert_eq!(observed["unknown_calls"], 1);
        peer.hang_up();
        let again = Peer::start(vec![(
            200,
            super::super::unknown_cost::unpriced_response("Hello again"),
        )]);
        let _wire = test_transport::install(&again.url);
        let next = session.begin_preparation_turn();
        assert!(!next.is_cancelled());
        assert!(matches!(session.turn("hello"), TurnOutcome::Reply(_)));
        assert_eq!(again.bodies().len(), 1);
        assert_eq!(
            session.uncertain_charges(),
            2,
            "unknown prices remain observations, no gate"
        );
        drop(session);
        let mut restored = open_unpriced(root.path());
        restored.enable_continuous_preparation();
        restored.enable_history(home.path()).unwrap();
        restored.restore_state();
        assert!(restored.cost_observations().iter().any(|o| o["schema"]
            == "nika/preparation-cost-observation@1"
            && o["unknown_calls"] == 2));
        assert_eq!(
            again.bodies().len(),
            1,
            "opening does not replay a model call"
        );
    }
}
