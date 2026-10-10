// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The contract's own judgments: what a command is, what bytes bind its identity, and the
//! frames' exact spelling.

use super::*;

fn submit(command: &str, snapshot: &str, line: &str) -> String {
    serde_json::json!({
        "contract": CONTRACT, "op": "submit", "command": command,
        "snapshot": snapshot, "line": line,
    })
    .to_string()
}

#[test]
fn a_command_is_one_object_of_this_contract_and_nothing_else() {
    let line = "résumé de mes notes\nchaque matin à 8 h";
    let parsed = Command::parse(submit("c-1", "snp_a", line).as_bytes()).expect("submit");
    assert_eq!(
        parsed,
        Command::Submit {
            command: "c-1".to_owned(),
            snapshot: "snp_a".to_owned(),
            line: line.to_owned(),
        }
    );
    let stop = format!(r#"{{"contract":"{CONTRACT}","op":"stop","command":"s.1:x-y_z"}}"#);
    assert!(matches!(
        Command::parse(stop.as_bytes()),
        Ok(Command::Stop { .. })
    ));
    for read in ["close", "snapshot", "details"] {
        let text = format!(r#"{{"contract":"{CONTRACT}","op":"{read}"}}"#);
        assert!(Command::parse(text.as_bytes()).is_ok(), "{read}");
    }
    let refused = [
        // another contract, an unknown op or field, a missing or forbidden field
        submit("c-1", "s", "x").replace(CONTRACT, "nika/session-host@2"),
        format!(r#"{{"contract":"{CONTRACT}","op":"consent","command":"c"}}"#),
        format!(r#"{{"contract":"{CONTRACT}","op":"stop","command":"c","why":"x"}}"#),
        format!(r#"{{"contract":"{CONTRACT}","op":"submit","command":"c","line":"x"}}"#),
        format!(r#"{{"contract":"{CONTRACT}","op":"stop","command":"c","line":"x"}}"#),
        format!(r#"{{"contract":"{CONTRACT}","op":"close","command":"c"}}"#),
        // an identity outside the alphabet or the length
        submit("c 1", "s", "x"),
        submit("", "s", "x"),
        submit(&"c".repeat(MAX_COMMAND + 1), "s", "x"),
        // not one object
        format!(r#"[{{"contract":"{CONTRACT}","op":"close"}}]"#),
        format!(r#"{{"contract":"{CONTRACT}","op":"stop","command":"a","command":"b"}}"#),
    ];
    for text in refused {
        assert!(Command::parse(text.as_bytes()).is_err(), "accepted {text}");
    }
    assert!(Command::parse(submit(&"c".repeat(MAX_COMMAND), "s", "x").as_bytes()).is_ok());
}

#[test]
fn an_identity_is_bound_to_its_op_its_snapshot_and_its_exact_line() {
    let digest = |text: String| Command::parse(text.as_bytes()).expect("command").digest();
    let base = digest(submit("c", "snp_a", "yes"));
    assert_eq!(base, digest(submit("c", "snp_a", "yes")));
    assert_ne!(base, digest(submit("c", "snp_b", "yes")));
    assert_ne!(base, digest(submit("c", "snp_a", "yes ")));
    // Length prefixes: moving bytes between fields is another command.
    assert_ne!(
        digest(submit("c", "ab", "c")),
        digest(submit("c", "a", "bc"))
    );
    let stop = digest(format!(
        r#"{{"contract":"{CONTRACT}","op":"stop","command":"c"}}"#
    ));
    assert_ne!(base, stop);
}

#[test]
fn frames_spell_their_kind_and_only_events_carry_a_number() {
    let snapshot = Snapshot {
        handle: "snp_1".to_owned(),
        seq: 2,
        busy: Some(Busy::new("c-1", TurnPhase::Preparing, true)),
        work: serde_json::json!({"contract": "nika/session-work@0"}),
    };
    let event = Frame::new(
        "ses_1",
        Some(3),
        Body::Stopped {
            command: "s-1".to_owned(),
            op: "stop",
            replayed: false,
            receipt: "stop_requested",
            target: Some("c-1".to_owned()),
            snapshot: snapshot.clone(),
        },
    );
    let value = serde_json::to_value(&event).expect("json");
    assert_eq!(value["contract"], CONTRACT);
    assert_eq!(value["frame"], "result");
    assert_eq!(value["session"], "ses_1");
    assert_eq!(value["event"], 3);
    assert_eq!(value["receipt"], "stop_requested");
    assert_eq!(value["snapshot"]["busy"]["phase"], "preparing");
    assert_eq!(value["snapshot"]["busy"]["stop_requested"], true);
    let again = serde_json::to_value(event.replayed()).expect("json");
    assert_eq!(again["replayed"], true);
    assert_eq!(again["event"], 3, "a replay names the same event");
    let refused = Frame::refused(
        "ses_1",
        Refused::Busy,
        "busy",
        Some("c-2"),
        Some("oui\n"),
        Some(snapshot),
    );
    let value = serde_json::to_value(&refused).expect("json");
    assert!(
        value.get("event").is_none(),
        "a direct reply carries no event"
    );
    assert_eq!(value["error"], "busy");
    assert_eq!(value["line"], "oui\n", "the refused line is returned whole");
    assert_eq!(refused.status(), 409);
    let lines = refused.to_line();
    assert!(!lines.contains('\n'), "one frame is one line");
}

#[test]
fn outcomes_are_projected_word_for_word_and_runs_become_effects() {
    use nika_session::outcome::{Refusal, RefusalClass};
    let mut wire = Vec::new();
    let mut effects = Vec::new();
    let run = RunRequest {
        workflow: "w.nika".into(),
        vars: vec!["city=Paris".to_owned(), "token=secret".to_owned()],
        max_cost_usd: 0.5,
        access_pin: None,
        bytes: None,
        closure: None,
    };
    project(
        TurnOutcome::Resumed {
            notice: "chosen".to_owned(),
            outcome: Box::new(TurnOutcome::RunRequested {
                report: "saved".to_owned(),
                run,
            }),
        },
        &mut wire,
        &mut effects,
    );
    project(
        TurnOutcome::Refusal(Refusal::new(RefusalClass::StaleRevision, "stale")),
        &mut wire,
        &mut effects,
    );
    let value = serde_json::to_value(&wire).expect("json");
    assert_eq!(
        value[0],
        serde_json::json!({"kind": "resumed", "text": "chosen"})
    );
    assert_eq!(value[1]["kind"], "run_requested");
    assert_eq!(value[1]["inputs"], serde_json::json!(["city", "token"]));
    assert!(
        !value.to_string().contains("secret"),
        "input values never travel"
    );
    assert_eq!(value[2]["class"], "stale_revision");
    assert!(matches!(effects.as_slice(), [Effect::Run(run)] if run.vars.len() == 2));
}

/// A line for the conversation's run under way is a command of the contract, by its mode, with
/// its identity and its line; its bytes bind it to its op as any command's do.
#[test]
fn steer_and_follow_up_are_commands_bound_to_their_line() {
    for op in ["steer", "follow_up"] {
        let text = format!(
            r#"{{"contract":"{CONTRACT}","op":"{op}","command":"c-7","line":"use b instead"}}"#
        );
        let (command, line) = ("c-7".to_owned(), "use b instead".to_owned());
        let expected = if op == "steer" {
            Command::Steer { command, line }
        } else {
            Command::FollowUp { command, line }
        };
        assert_eq!(Command::parse(text.as_bytes()), Ok(expected));
        let missing = format!(r#"{{"contract":"{CONTRACT}","op":"{op}","command":"c-7"}}"#);
        assert!(
            Command::parse(missing.as_bytes()).is_err(),
            "{op} without a line"
        );
        let snapshot = format!(
            r#"{{"contract":"{CONTRACT}","op":"{op}","command":"c","snapshot":"s","line":"x"}}"#
        );
        assert!(
            Command::parse(snapshot.as_bytes()).is_err(),
            "{op} with a snapshot"
        );
    }
    let steer = Command::Steer {
        command: "c".to_owned(),
        line: "x".to_owned(),
    };
    let follow = Command::FollowUp {
        command: "c".to_owned(),
        line: "x".to_owned(),
    };
    let submitted = Command::Submit {
        command: "c".to_owned(),
        snapshot: String::new(),
        line: "x".to_owned(),
    };
    assert_ne!(steer.digest(), follow.digest());
    assert_ne!(steer.digest(), submitted.digest());
}

/// A stopped turn of a conversation is typed on the wire: how the stop reached the intelligence,
/// the lines returned unsent with their identities, the draft kept; a tool step is typed beside
/// its words, never with its arguments.
#[test]
fn a_stopped_turn_and_a_tool_step_are_typed_on_the_wire() {
    use nika_session::outcome::{StopReach, Stopped};
    use nika_session::steer::{QueueMode, Queued, QueuedState};

    let mut unsent = Queued::new("l2", QueueMode::FollowUp, "and c");
    unsent.state = QueuedState::Returned;
    let stopped = Stopped::new(StopReach::AgentCancelled, vec![unsent], Some(2));
    let (mut wire, mut effects) = (Vec::new(), Vec::new());
    project(
        TurnOutcome::Stopped(stopped.clone()),
        &mut wire,
        &mut effects,
    );
    assert!(effects.is_empty());
    let expected = serde_json::json!([{
        "kind": "stopped", "reach": "agent_cancelled", "text": stopped.text(),
        "unsent": [{"id": "l2", "mode": "follow_up", "line": "and c", "state": "returned"}],
        "candidate": 2
    }]);
    assert_eq!(serde_json::to_value(&wire).expect("outcomes"), expected);
    let text = stopped.text();
    assert!(text.contains("the agent was asked to stop") && text.contains("« and c »"));
    let tool =
        nika_session::activity::tool_activity("toolu_1", "verify", ToolState::Finished, Some(42));
    let expected = serde_json::json!({
        "phase": "checking", "note": "verify · 42 ms", "done": true,
        "tool": {"call": "toolu_1", "name": "verify", "state": "finished", "elapsed_ms": 42}
    });
    let shown = serde_json::to_value(ActivityWire::of(&tool)).expect("activity");
    assert_eq!(shown, expected);
}
