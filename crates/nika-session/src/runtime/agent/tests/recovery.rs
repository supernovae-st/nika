// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A failed API turn and reopen use the real Session/registry path on loopback.
use super::*;
use crate::reasoner::{ProviderReasoner, test_transport};
use crate::runtime::inference_tests::wire::{Peer, response};
use crate::{DataLocus, IntelligenceKind, ResolvedSessionIntelligence};

const ORIGINAL: &str = "Réponds simplement : prêt pour vérifier mon bilan.";
const RETRY: &str = "La clé est corrigée. Reprends ma demande.";

fn conversation(root: &Path) -> SessionRuntime {
    let selected = ResolvedSessionIntelligence::new(
        IntelligenceKind::Api {
            provider: "deepseek".into(),
        },
        Some("deepseek/deepseek-v4-pro".into()),
        DataLocus::Metered {
            provider: "deepseek".into(),
        },
        true,
        None,
    );
    let mut session = SessionRuntime::open(
        root,
        selected,
        Box::new(ProviderReasoner {
            model: "deepseek/deepseek-v4-pro".into(),
            label: "DeepSeek".into(),
        }),
    );
    session.agent = Some(Driver::with_store(TreeFile::memory()));
    session.enable_continuous_preparation();
    session
}

fn auth_peer() -> Peer {
    Peer::start(vec![
        (
            401,
            json!({"error": {"message": "Invalid API key", "type": "authentication_error"}}),
        ),
        (200, response("prêt pour vérifier mon bilan")),
    ])
}

#[test]
fn failed_conversation_reopen_preserves_exact_original_and_retry_on_the_wire() {
    let peer = auth_peer();
    let _http = test_transport::install(&peer.url);
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut session = conversation(root.path());
    session.enable_history(home.path()).unwrap();
    assert!(matches!(session.turn(ORIGINAL), TurnOutcome::Refusal(_)));
    assert_eq!(peer.bodies().len(), 1);
    drop(session);
    let mut resumed = conversation(root.path());
    resumed.enable_history(home.path()).unwrap();
    assert_eq!(
        peer.bodies().len(),
        1,
        "reopen does not replay the failed call"
    );
    let outcome = resumed.turn(RETRY);
    assert!(
        matches!(outcome, TurnOutcome::Reply(ref text) if text == "prêt pour vérifier mon bilan"),
        "{outcome:?}"
    );
    let bodies = peer.bodies();
    assert_eq!(bodies.len(), 2);
    let users: Vec<_> = bodies[1]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|message| message["role"] == "user")
        .map(|message| message["content"].as_str().unwrap())
        .collect();
    assert_eq!(
        users,
        [
            format!("{ORIGINAL}\n(cited as u1)"),
            format!("{RETRY}\n(cited as u2)")
        ]
    );
    assert!(resumed.pending.is_none() && resumed.requested_run.is_none());
}

#[test]
fn failed_inference_recovery_keeps_billing_unknown_without_inventing_workflow_effects() {
    let peer = auth_peer();
    let _http = test_transport::install(&peer.url);
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut session = conversation(root.path());
    session.enable_history(home.path()).unwrap();
    assert!(matches!(session.turn(ORIGINAL), TurnOutcome::Refusal(_)));
    assert_eq!(session.uncertain_charges(), 1);
    drop(session);
    let mut resumed = conversation(root.path());
    let notice = resumed.enable_history(home.path()).unwrap().unwrap();
    let notes: Vec<_> = resumed
        .kept_turns()
        .iter()
        .filter(|(user, _)| user == "(recovery)")
        .map(|(_, text)| text.as_str())
        .collect();
    assert_eq!(
        notes,
        [
            "[An earlier model request has unknown API usage or billing. This notice concerns inference, not an unobserved workflow effect. Your messages are kept; nothing was replayed.]"
        ]
    );
    assert!(
        notice.contains("historical API charge remains unknown"),
        "{notice}"
    );
    assert!(!notice.contains("inspect its effects"), "{notice}");
    assert_eq!(peer.bodies().len(), 1);
}

/// A legacy engine system stays on disk; an explicit new turn gets the current contract.
#[test]
fn reopened_legacy_engine_instructions_refresh_only_when_the_person_continues() {
    let peer = auth_peer();
    let _http = test_transport::install(&peer.url);
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut session = conversation(root.path());
    session.enable_history(home.path()).unwrap();
    assert!(matches!(session.turn(ORIGINAL), TurnOutcome::Refusal(_)));
    let old = legacy_instructions();
    let driver = session.agent.as_mut().unwrap();
    let tree = driver.tree.as_mut().unwrap();
    tree.append(EntryKind::System { text: old.clone() }, unix_ms(), |line| {
        Store::append(&mut driver.store, line)
    })
    .unwrap();
    let old_tree = driver.store.text().unwrap().unwrap();
    drop(session);
    let mut resumed = conversation(root.path());
    resumed.enable_history(home.path()).unwrap();
    assert_eq!(
        resumed
            .agent
            .as_ref()
            .unwrap()
            .store
            .text()
            .unwrap()
            .unwrap(),
        old_tree
    );
    assert_eq!(peer.bodies().len(), 1);
    assert!(matches!(resumed.turn(RETRY), TurnOutcome::Reply(_)));
    let bodies = peer.bodies();
    let systems: Vec<_> = bodies[1]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|message| message["role"] == "system")
        .collect();
    assert_eq!(systems.len(), 1);
    assert_eq!(
        systems[0]["content"],
        instructions("deepseek/deepseek-v4-pro")
    );
    let driver = resumed.agent.as_ref().unwrap();
    assert!(driver.store.text().unwrap().unwrap().starts_with(&old_tree));
    assert_eq!(
        driver.tree.as_ref().unwrap().cited("u1").unwrap().1,
        ORIGINAL
    );
    assert_eq!(driver.tree.as_ref().unwrap().cited("u2").unwrap().1, RETRY);
}

fn legacy_instructions() -> String {
    // This exact historical body is recognized by the production legacy ownership guard.
    let body = include_str!("recovery/fixtures/legacy-author-system.md");
    assert_eq!(
        nika_event::source_id::sha256_hex(body.as_bytes()),
        "c529c155b2cc0e93c929d76113e47bb1118b85488a5a0362d7f91af8ebbbfd92"
    );
    let current = instructions("deepseek/deepseek-v4-pro");
    let suffix = current.strip_prefix(SYSTEM).unwrap();
    format!("{body}{suffix}")
}

/// Similar-looking or appended custom systems have no engine ownership witness.
#[test]
fn engine_refresh_preserves_custom_instructions_and_their_historical_bytes() {
    let dir = tempfile::tempdir().unwrap();
    for text in [
        "Custom host instructions".to_owned(),
        format!("{}\nCustom instruction", legacy_instructions()),
    ] {
        let mut store = TreeFile::memory();
        let mut tree = start_tree(&mut store, dir.path(), "deepseek/deepseek-v4-pro").unwrap();
        tree.append(
            EntryKind::System { text: text.clone() },
            unix_ms(),
            |line| Store::append(&mut store, line),
        )
        .unwrap();
        let before = store.text().unwrap();
        system::refresh(&mut tree, &mut store, "deepseek/deepseek-flash").unwrap();
        assert_eq!(store.text().unwrap(), before);
        assert_eq!(tree.context().system, Some(text));
    }
}

#[test]
fn owned_instruction_refresh_is_recorded_once_without_changing_human_citations() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = TreeFile::memory();
    let mut tree = start_tree(&mut store, dir.path(), "deepseek/deepseek-v4-pro").unwrap();
    tree.append_user(ORIGINAL, None, None, unix_ms(), |line| {
        Store::append(&mut store, line)
    })
    .unwrap();
    let before = store.text().unwrap().unwrap();
    system::refresh(&mut tree, &mut store, "deepseek/deepseek-flash").unwrap();
    assert_eq!(
        tree.context().system,
        Some(instructions("deepseek/deepseek-flash"))
    );
    assert_eq!(tree.cited("u1").unwrap().1, ORIGINAL);
    assert_eq!(tree.next_cite(), "u2");
    let updated = store.text().unwrap().unwrap();
    assert!(updated.starts_with(&before));
    assert_eq!(updated.lines().count(), before.lines().count() + 2);
    system::refresh(&mut tree, &mut store, "deepseek/deepseek-flash").unwrap();
    assert_eq!(store.text().unwrap().unwrap(), updated);
}

/// A failed second append can leave exact current instructions without their ownership Fact.
#[test]
fn a_missing_instruction_fact_is_repaired_once_for_exact_engine_bytes() {
    let mut store = TreeFile::memory();
    let mut tree = Tree::new("session", "project", unix_ms(), |line| {
        Store::append(&mut store, line)
    })
    .unwrap();
    tree.append(
        EntryKind::System {
            text: instructions("deepseek/deepseek-flash"),
        },
        unix_ms(),
        |line| Store::append(&mut store, line),
    )
    .unwrap();
    let before = store.text().unwrap().unwrap();
    system::refresh(&mut tree, &mut store, "deepseek/deepseek-flash").unwrap();
    let marked = store.text().unwrap().unwrap();
    assert!(marked.starts_with(&before));
    assert_eq!(marked.lines().count(), before.lines().count() + 1);
    system::refresh(&mut tree, &mut store, "deepseek/deepseek-flash").unwrap();
    assert_eq!(store.text().unwrap().unwrap(), marked);
    system::refresh(&mut tree, &mut store, "deepseek/deepseek-v4-pro").unwrap();
    assert_eq!(
        tree.context().system,
        Some(instructions("deepseek/deepseek-v4-pro"))
    );
}

fn ask_reply() -> Value {
    let mut reply = response("");
    reply["choices"][0]["message"] = json!({
        "content": null,
        "tool_calls": [{"id": "old_call", "type": "function", "function": {
            "name": "ask", "arguments": json!({"questions": [{
                "key": "detail", "question": "Which detail?", "free_text": true,
                "options": [{"key": "brief", "label": "Brief"}, {"key": "full", "label": "Full"}]
            }]}).to_string()
        }}]
    });
    reply["choices"][0]["finish_reason"] = json!("tool_calls");
    reply
}

/// A question identity expires on reopen, so the old tool call cannot reinterpret u3 as an
/// answer to the assistant's earlier question. The actual provider wire keeps all human words.
#[test]
fn an_expired_parked_question_closes_before_the_reopened_human_turn() {
    let peer = Peer::start(vec![
        (401, json!({"error": {"message": "Invalid API key"}})),
        (200, ask_reply()),
        (200, response("prêt pour vérifier mon bilan")),
    ]);
    let _http = test_transport::install(&peer.url);
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut session = conversation(root.path());
    session.enable_history(home.path()).unwrap();
    assert!(matches!(session.turn(ORIGINAL), TurnOutcome::Refusal(_)));
    let asked = session.turn(RETRY);
    assert!(matches!(asked, TurnOutcome::Question { .. }), "{asked:?}");
    let before = session
        .agent
        .as_ref()
        .unwrap()
        .store
        .text()
        .unwrap()
        .unwrap();
    assert_eq!(
        session
            .agent
            .as_ref()
            .unwrap()
            .tree
            .as_ref()
            .unwrap()
            .parked(),
        Some(("old_call", "ask"))
    );
    drop(session);

    let mut resumed = conversation(root.path());
    resumed.enable_history(home.path()).unwrap();
    assert_eq!(peer.bodies().len(), 2, "reopen makes no model call");
    assert_eq!(
        resumed
            .agent
            .as_ref()
            .unwrap()
            .store
            .text()
            .unwrap()
            .unwrap(),
        before
    );
    assert!(
        resumed
            .agent
            .as_ref()
            .unwrap()
            .toolbox
            .with(|c| c.questions().is_empty())
    );
    let outcome = resumed.turn(RETRY);
    assert!(
        matches!(outcome, TurnOutcome::Reply(ref text) if text == "prêt pour vérifier mon bilan"),
        "{outcome:?}"
    );
    let bodies = peer.bodies();
    assert_eq!(bodies.len(), 3);
    let messages = bodies[2]["messages"].as_array().unwrap();
    let users: Vec<_> = messages
        .iter()
        .filter(|m| m["role"] == "user")
        .map(|m| m["content"].as_str().unwrap())
        .collect();
    assert_eq!(users.len(), 3);
    assert_eq!(users[0], format!("{ORIGINAL}\n(cited as u1)"));
    assert_eq!(users[1], format!("{RETRY}\n(cited as u2)"));
    let (human, note) = users[2].split_once("\nNika (not the person):\n").unwrap();
    assert_eq!(human, format!("{RETRY}\n(cited as u3)"));
    let (guidance, context) = note.rsplit_once('\n').unwrap();
    assert!(guidance.contains("together with all later human messages"));
    assert!(guidance.contains("grant no new authority to save, run or perform work"));
    assert_eq!(
        serde_json::from_str::<Value>(context).unwrap(),
        json!({
            "current_request": {"since": 0, "first_human_message": {"citation": "u1", "text": ORIGINAL}},
            "candidate_revision": null, "expired_question_call": "old_call"
        })
    );
    let tools: Vec<_> = messages.iter().filter(|m| m["role"] == "tool").collect();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["tool_call_id"], "old_call");
    assert_eq!(tools[0]["content"], reopened::EXPIRED);
    assert_eq!(messages.iter().filter(|m| m["role"] == "system").count(), 1);
    let driver = resumed.agent.as_ref().unwrap();
    let tree = driver.tree.as_ref().unwrap();
    assert!(tree.parked().is_none());
    assert!(tree.branch().iter().any(|entry| matches!(&entry.kind, EntryKind::User { cite, text, answers: None, .. } if cite == "u3" && text == RETRY)));
    assert!(driver.store.text().unwrap().unwrap().starts_with(&before));
    assert!(driver.toolbox.with(|c| c.bindings().is_empty()));
    assert!(resumed.pending.is_none() && resumed.requested_run.is_none());
}

#[test]
fn a_current_question_keeps_its_bound_answer_door() {
    let peer = Peer::start(vec![(200, ask_reply()), (200, response("Understood."))]);
    let _http = test_transport::install(&peer.url);
    let root = tempfile::tempdir().unwrap();
    let mut session = conversation(root.path());
    let outcome = session.turn("Let me choose the detail.");
    assert!(
        matches!(outcome, TurnOutcome::Question { .. }),
        "{outcome:?}"
    );
    let id = session
        .agent
        .as_ref()
        .unwrap()
        .toolbox
        .with(|c| c.questions()[0].id.clone());
    let outcome = session.answer_question_for(&id, "brief");
    assert!(
        matches!(outcome, TurnOutcome::Reply(ref text) if text == "Understood."),
        "{outcome:?}"
    );
    let bodies = peer.bodies();
    assert_eq!(bodies.len(), 2);
    let messages = bodies[1]["messages"].as_array().unwrap();
    let answered = messages
        .iter()
        .find(|m| m["role"] == "tool" && m["tool_call_id"] == "old_call")
        .unwrap();
    assert!(
        answered["content"]
            .as_str()
            .unwrap()
            .starts_with("The person answered, cited as u2:\nbrief"),
        "{answered}"
    );
    assert!(
        !answered["content"]
            .as_str()
            .unwrap()
            .contains(reopened::EXPIRED)
    );
    assert!(messages.iter().all(|message| {
        !message["content"]
            .as_str()
            .is_some_and(|text| text.contains("current request anchor"))
    }));
    let tree = session.agent.as_ref().unwrap().tree.as_ref().unwrap();
    assert!(tree.branch().iter().any(|entry| matches!(&entry.kind, EntryKind::User { cite, answers: Some(call), .. } if cite == "u2" && call == "old_call")));
    assert!(session.pending.is_none() && session.requested_run.is_none());
}

/// A reopened saved candidate is context, not a new draft the model must propose again.
#[test]
fn a_reopened_saved_workflow_can_explain_a_run_failure_in_one_reply() {
    use nika_session_agent::conversation::Citations;
    const SOURCE: &str = "nika: saved\npermits:\n  tools: [\"nika:read\"]\n  fs:\n    read: [\"sales.csv\"]\ntasks:\n  read:\n    invoke: { tool: \"nika:read\", args: { path: \"sales.csv\" } }\n";
    const DIAGNOSIS: &str = "sales.csv is missing. Restore it at the project root, then ask to retry. The saved workflow is unchanged.";
    const REQUEST: &str = "Create and save a workflow that reads sales.csv.";
    let peer = Peer::start(vec![(200, response(DIAGNOSIS))]);
    let _http = test_transport::install(&peer.url);
    let root = tempfile::tempdir().unwrap();
    let history = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("saved.nika"), SOURCE).unwrap();
    let mut store = TreeFile::home(OwnedDir::open(history.path()).unwrap());
    let mut tree = start_tree(&mut store, root.path(), "deepseek/deepseek-v4-pro").unwrap();
    tree.append_user(REQUEST, None, None, unix_ms(), |line| {
        Store::append(&mut store, line)
    })
    .unwrap();
    let before = store.text().unwrap().unwrap();
    let mut citations = Citations::default();
    citations.record(3, Some(("u1".to_owned(), REQUEST.to_owned())));
    let mut prior = Conversation::default();
    let written = prior.write(
        &citations,
        (SOURCE.to_owned(), "Read sales.csv".to_owned()),
        (&[], &[]),
        &mut |_| Ok(()),
    );
    assert!(!written.is_error, "{written:?}");
    prior.saved_at("saved.nika", SOURCE);
    drop(store);
    let mut resumed = conversation(root.path());
    resumed.resume_agent(OwnedDir::open(history.path()), Some(&prior.kept()));
    assert!(peer.bodies().is_empty(), "reopening performs no inference");
    assert!(resumed.pending.is_none() && resumed.requested_run.is_none());
    assert!(
        resumed
            .agent
            .as_ref()
            .unwrap()
            .toolbox
            .with(|c| c.shown_verdict().is_none())
    );
    let outcome = resumed.after_run(1, "Failed: sales.csv was not found.".to_owned());
    assert!(
        matches!(outcome, TurnOutcome::Reply(ref text) if text == &format!("Failed: sales.csv was not found.\n\n{DIAGNOSIS}")),
        "{outcome:?}"
    );
    assert_eq!(
        peer.bodies().len(),
        1,
        "no false proposal reminder may cause a second response"
    );
    let driver = resumed.agent.as_ref().unwrap();
    let tree = driver.tree.as_ref().unwrap();
    assert_eq!(
        tree.next_cite(),
        "u2",
        "a diagnostic creates no human authority"
    );
    assert!(tree.branch().iter().all(|entry| !matches!(&entry.kind, EntryKind::Note { text } if text.contains("candidate you wrote is not proposed"))));
    let assistants: Vec<_> = tree
        .branch()
        .into_iter()
        .filter_map(|entry| match &entry.kind {
            EntryKind::Assistant { content, .. } => Some(content),
            _ => None,
        })
        .collect();
    assert_eq!(assistants.len(), 1);
    assert!(
        assistants[0]
            .iter()
            .all(|block| !matches!(block, nika_kernel::provider::ContentBlock::ToolUse { .. }))
    );
    assert!(driver.store.text().unwrap().unwrap().starts_with(&before));
    assert!(
        driver
            .toolbox
            .with(|c| c.shown_verdict().is_none() && c.questions().is_empty())
    );
    assert!(resumed.pending.is_none() && resumed.requested_run.is_none());
    assert_eq!(
        std::fs::read_to_string(root.path().join("saved.nika")).unwrap(),
        SOURCE
    );
    assert!(!root.path().join(".nika/consents.ndjson").exists());
    assert!(!root.path().join(".nika/traces").exists());
}

#[path = "recovery/anchor.rs"]
mod anchor;
