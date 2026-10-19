// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Recovery projects the current request without replacing the human's later correction.
use super::*;

fn replacing_reply(text: &str) -> Value {
    let mut reply = response("");
    reply["choices"][0]["message"] = json!({"content": null, "tool_calls": [{
        "id": "replace_call", "type": "function", "function": {
            "name": "new_request",
            "arguments": json!({"message": "u2", "excerpt": text}).to_string()
        }
    }]});
    reply["choices"][0]["finish_reason"] = json!("tool_calls");
    reply
}

#[test]
fn recovery_anchors_the_replacement_and_keeps_the_later_correction_on_the_wire() {
    const OLD: &str = "Please describe how a summary works.";
    const REPLACEMENT: &str = "Replace that request: reply with the word ready.";
    const CORRECTION: &str = "Actually, reply with the word steady. Do not save or run anything.";
    let peer = Peer::start(vec![
        (200, response("A summary keeps the essential points.")),
        (200, replacing_reply(REPLACEMENT)),
        (200, ask_reply()),
        (200, response("steady")),
    ]);
    let _http = test_transport::install(&peer.url);
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut session = conversation(root.path());
    session.enable_history(home.path()).unwrap();
    assert!(matches!(session.turn(OLD), TurnOutcome::Reply(_)));
    let outcome = session.turn(REPLACEMENT);
    assert!(
        matches!(outcome, TurnOutcome::Question { .. }),
        "{outcome:?}"
    );
    let driver = session.agent.as_ref().unwrap();
    let since = driver.toolbox.with(|c| c.since());
    let citations = driver.store.citations();
    assert_eq!(since, citations.lock().unwrap().get("u2").unwrap().at);
    let before = driver.store.text().unwrap().unwrap();
    drop(session);

    let mut resumed = conversation(root.path());
    resumed.enable_history(home.path()).unwrap();
    assert_eq!(peer.bodies().len(), 3, "reopening invokes no provider");
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
    let outcome = resumed.turn(CORRECTION);
    assert!(matches!(outcome, TurnOutcome::Reply(ref text) if text == "steady"));
    let bodies = peer.bodies();
    assert_eq!(bodies.len(), 4);
    let messages = bodies[3]["messages"].as_array().unwrap();
    assert_eq!(messages.iter().filter(|m| m["role"] == "system").count(), 1);
    let users: Vec<_> = messages
        .iter()
        .filter(|m| m["role"] == "user")
        .map(|m| m["content"].as_str().unwrap())
        .collect();
    assert_eq!(users.len(), 3);
    assert_eq!(users[0], format!("{OLD}\n(cited as u1)"));
    // The person's words, exactly; the Session's spend so far rides beside them once a request
    // was observed.
    let (human, _spend) = users[1].split_once("\nNika (not the person):\n").unwrap();
    assert_eq!(human, format!("{REPLACEMENT}\n(cited as u2)"));
    let (human, note) = users[2].split_once("\nNika (not the person):\n").unwrap();
    assert_eq!(human, format!("{CORRECTION}\n(cited as u3)"));
    let (guidance, state) = note.rsplit_once('\n').unwrap();
    assert!(guidance.contains("together with all later human messages"));
    assert!(guidance.contains("grant no new authority"));
    assert_eq!(
        serde_json::from_str::<Value>(state).unwrap(),
        json!({
            "current_request": {"since": since, "first_human_message": {
                "citation": "u2", "text": REPLACEMENT
            }},
            "candidate_revision": null, "expired_question_call": "old_call"
        })
    );
    let closed: Vec<_> = messages
        .iter()
        .filter(|m| m["role"] == "tool" && m["tool_call_id"] == "old_call")
        .collect();
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0]["content"], reopened::EXPIRED);
    let driver = resumed.agent.as_ref().unwrap();
    let tree = driver.tree.as_ref().unwrap();
    let (entry, text) = tree.cited("u3").unwrap();
    assert_eq!(text, CORRECTION);
    assert!(
        matches!(&entry.kind, EntryKind::User { answers: None, nika: Some(held), .. } if held == note)
    );
    assert!(driver.store.text().unwrap().unwrap().starts_with(&before));
    assert!(
        driver
            .toolbox
            .with(|c| c.candidate().is_none() && c.bindings().is_empty())
    );
    assert!(resumed.pending.is_none() && resumed.requested_run.is_none());
    assert!(!root.path().join(".nika/consents.ndjson").exists());
    assert!(!root.path().join(".nika/traces").exists());
}

#[test]
fn recovery_context_preserves_an_independent_run_observation() {
    assert_eq!(
        reopened::with_run_note(
            Some("run failed: input missing".into()),
            Some("retained request".into())
        ),
        Some("run failed: input missing\n\nretained request".into())
    );
    assert_eq!(
        reopened::with_run_note(Some("run failed: input missing".into()), None),
        Some("run failed: input missing".into())
    );
}
