// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A real cancelled Session call followed by a correction sent through the compiler's wire.
use super::*;
use std::sync::Mutex;

const ORIGINAL: &str = "À partir de inventory.json, crée reorder.nika qui prépare un réapprovisionnement : pour chaque article dont stock est strictement inférieur à 8, écris dans reorder.json son sku et reorder_qty = 12 moins stock. Trie le résultat par sku. Le fichier inventory.json doit rester identique.";
const CORRECTION: &str = "Reprends la préparation de reorder.nika avec une correction : le seuil demeure stock strictement inférieur à 8, mais la quantité à commander doit être 10 moins stock. Trie par sku et conserve inventory.json intact.";

struct Route {
    act: TurnAct,
    seen: Arc<Mutex<Vec<(TurnContext, String)>>>,
}
impl TurnClassifier for Route {
    fn classify(&mut self, context: &TurnContext, input: &str) -> TurnDecision {
        self.seen
            .lock()
            .unwrap()
            .push((context.clone(), input.into()));
        TurnDecision::new(self.act, crate::turn::RoutingMethod::Model)
    }
}
fn route(session: &mut SessionRuntime, act: TurnAct) -> Arc<Mutex<Vec<(TurnContext, String)>>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    session.with_classifier(Box::new(Route {
        act,
        seen: seen.clone(),
    }));
    seen
}
fn interrupted(root: &Path, home: &Path) -> SessionRuntime {
    std::fs::write(root.join("inventory.json"), r#"[{"sku":"A1","stock":5}]"#).unwrap();
    let peer = HeldPeer::start();
    let mut session = open_unpriced(root);
    session.enable_continuous_preparation();
    session.enable_history(home).unwrap();
    route(&mut session, TurnAct::NewWork);
    let stop = session.begin_preparation_turn();
    let url = peer.url.clone();
    let (returned, receive) = mpsc::channel();
    let worker = std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            let _wire = test_transport::install(&url);
            let outcome = session.turn(ORIGINAL);
            returned.send((session, outcome)).unwrap();
        })
        .unwrap();
    peer.wait_received();
    stop.cancel();
    let (session, outcome) = receive.recv_timeout(Duration::from_secs(3)).unwrap();
    worker.join().unwrap();
    peer.hang_up();
    assert!(
        matches!(outcome, TurnOutcome::Cancelled(_)),
        "the held preparation must return cancelled"
    );
    assert_eq!(peer.requests(), 1);
    assert_eq!(session.intent.goal.as_deref(), Some(ORIGINAL));
    assert!(session.authoring.is_none());
    assert!(session.pending_proposal().is_none());
    assert!(!root.join("reorder.nika").exists());
    session
}
fn unavailable() -> Peer {
    Peer::start(vec![(
        400,
        json!({"error":{"message":"synthetic author unavailable"}}),
    )])
}
fn user_prompt(peer: &Peer) -> String {
    let bodies = peer.bodies();
    assert!(
        !bodies.is_empty(),
        "the actual compiler reached the provider"
    );
    bodies[0]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|message| message["role"] == "user")
        .map(|message| message["content"].as_str().unwrap())
        .collect::<Vec<_>>()
        .join("\n")
}
fn no_effects(session: &SessionRuntime, root: &Path) {
    assert!(session.pending_proposal().is_none());
    assert!(!root.join("reorder.nika").exists());
    assert!(!root.join("reorder.json").exists());
    assert_eq!(
        std::fs::read_to_string(root.join("inventory.json")).unwrap(),
        r#"[{"sku":"A1","stock":5}]"#
    );
}

#[test]
fn stopped_creation_correction_reaches_the_author_with_its_original_requirements() {
    for reopen in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let mut session = interrupted(root.path(), home.path());
        let before = session.cost_observations();
        if reopen {
            drop(session);
            session = open_unpriced(root.path());
            session.enable_continuous_preparation();
            session.enable_history(home.path()).unwrap();
            session.restore_state();
            assert_eq!(session.intent.goal.as_deref(), Some(ORIGINAL));
            assert!(
                session.cost_observations() == before,
                "reopen must preserve every cost observation exactly"
            );
            assert!(session.pending_proposal().is_none());
        }
        let seen = route(&mut session, TurnAct::Modify);
        let peer = unavailable();
        let _wire = test_transport::install(&peer.url);
        session.begin_preparation_turn();
        let outcome = session.turn(CORRECTION);
        assert!(!matches!(
            outcome,
            TurnOutcome::Proposal { .. } | TurnOutcome::RunRequested { .. }
        ));
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0.automation.as_deref(), Some(ORIGINAL));
        assert_eq!(seen[0].1, CORRECTION);
        let prompt = user_prompt(&peer);
        assert!(
            prompt.contains(ORIGINAL),
            "the author prompt must keep the original files, fields and obligations"
        );
        assert!(
            prompt.contains(CORRECTION),
            "the author prompt must include the exact later change"
        );
        assert!(
            prompt.contains("takes precedence over the original"),
            "the author prompt must state correction precedence"
        );
        assert!(prompt.find(ORIGINAL) < prompt.find(CORRECTION));
        let goal = session.intent.goal.as_deref().unwrap();
        assert!(goal.contains(ORIGINAL) && goal.contains(CORRECTION));
        no_effects(&session, root.path());
    }
}

#[test]
fn new_work_after_stop_does_not_inherit_the_interrupted_request() {
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut session = interrupted(root.path(), home.path());
    let seen = route(&mut session, TurnAct::NewWork);
    let peer = unavailable();
    let _wire = test_transport::install(&peer.url);
    session.begin_preparation_turn();
    session.turn(WORK);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0.automation.as_deref(), Some(ORIGINAL));
    let prompt = user_prompt(&peer);
    assert!(
        prompt.contains(WORK),
        "the author prompt must contain the independent request"
    );
    for stale in ["reorder.json", "reorder_qty", "12 moins stock"] {
        assert!(
            !prompt.contains(stale),
            "independent work must not inherit interrupted requirements"
        );
    }
    assert_eq!(session.intent.goal.as_deref(), Some(WORK));
    no_effects(&session, root.path());
}

#[test]
fn unknown_line_after_stop_neither_restarts_the_request_nor_replaces_its_goal() {
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut session = interrupted(root.path(), home.path());
    let seen = route(&mut session, TurnAct::Unknown);
    let peer = Peer::start(vec![]);
    let _wire = test_transport::install(&peer.url);
    session.begin_preparation_turn();
    assert!(matches!(session.turn(CORRECTION), TurnOutcome::Facts(_)));
    assert_eq!(seen.lock().unwrap().len(), 1);
    assert!(peer.bodies().is_empty(), "no phantom author call");
    assert_eq!(session.intent.goal.as_deref(), Some(ORIGINAL));
    no_effects(&session, root.path());
}
