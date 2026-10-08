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
