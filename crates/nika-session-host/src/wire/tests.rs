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

/// A line that is not a command still names the valid identity its JSON carries, so its
/// refusal can name it; an invalid identity, none, or a line that is not one object names none.
#[test]
fn a_line_that_does_not_parse_still_names_its_valid_identity() {
    let unknown = format!(r#"{{"contract":"{CONTRACT}","op":"rewind","command":"c-9"}}"#);
    assert!(Command::parse(unknown.as_bytes()).is_err());
    assert_eq!(
        Command::identity_of(unknown.as_bytes()).as_deref(),
        Some("c-9")
    );
    let forbidden =
        format!(r#"{{"contract":"{CONTRACT}","op":"stop","command":"s-1","line":"x"}}"#);
    assert!(Command::parse(forbidden.as_bytes()).is_err());
    assert_eq!(
        Command::identity_of(forbidden.as_bytes()).as_deref(),
        Some("s-1")
    );
    for line in [
        r#"{"op":"rewind","command":"not an identity"}"#,
        r#"{"op":"rewind","command":7}"#,
        r#"{"op":"rewind"}"#,
        r#"["c-9"]"#,
        "not json",
    ] {
        assert_eq!(Command::identity_of(line.as_bytes()), None, "{line}");
    }
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

/// A reply bound to one question names it beside its snapshot: `answer` with the line, exactly
/// as typed, `delegate` with none. Its bytes bind the question too, so the same line for another
/// question, or as a plain submit, is another command; a question identity is a witness, never
/// free text, and no other op takes one.
#[test]
fn a_reply_bound_to_one_question_names_it_and_is_bound_to_it() {
    let q2 = "9f".repeat(32);
    let text = |op: &str, extra: &str| {
        format!(
            r#"{{"contract":"{CONTRACT}","op":"{op}","command":"c-8","snapshot":"snp_a"{extra}}}"#
        )
    };
    let answer = |question: &str, line: &str| Command::Answer {
        command: "c-8".to_owned(),
        snapshot: "snp_a".to_owned(),
        question: question.to_owned(),
        line: line.to_owned(),
    };
    let named = format!(r#","question":"{q2}","line":"a""#);
    assert_eq!(
        Command::parse(text("answer", &named).as_bytes()),
        Ok(answer(&q2, "a"))
    );
    let delegated = Command::Delegate {
        command: "c-8".to_owned(),
        snapshot: "snp_a".to_owned(),
        question: q2.clone(),
    };
    let left = format!(r#","question":"{q2}""#);
    assert_eq!(
        Command::parse(text("delegate", &left).as_bytes()),
        Ok(delegated.clone())
    );
    let refused = [
        // a missing or forbidden field
        text("answer", r#","line":"a""#),
        text("answer", &left),
        text("delegate", &named),
        text("delegate", ""),
        text("submit", &named),
        format!(
            r#"{{"contract":"{CONTRACT}","op":"steer","command":"c","line":"a","question":"{q2}"}}"#
        ),
        format!(r#"{{"contract":"{CONTRACT}","op":"close","question":"{q2}"}}"#),
        // a question identity outside the alphabet or the length
        text("answer", r#","question":"q 2","line":"a""#),
        text("answer", r#","question":"","line":"a""#),
        text(
            "delegate",
            &format!(r#","question":"{}""#, "a".repeat(MAX_QUESTION + 1)),
        ),
    ];
    for text in refused {
        assert!(Command::parse(text.as_bytes()).is_err(), "accepted {text}");
    }
    let q1 = "1e".repeat(32);
    let base = answer(&q2, "a").digest();
    assert_eq!(base, answer(&q2, "a").digest());
    assert_ne!(base, answer(&q1, "a").digest(), "another question");
    assert_ne!(base, answer(&q2, "b").digest(), "another line");
    let submitted = Command::Submit {
        command: "c-8".to_owned(),
        snapshot: "snp_a".to_owned(),
        line: "a".to_owned(),
    };
    assert_ne!(base, submitted.digest(), "a plain submit of the same line");
    assert_ne!(base, delegated.digest());
    // Length prefixes: moving bytes between the line and the question is another command.
    assert_ne!(answer("ab", "c").digest(), answer("a", "bc").digest());
    let refusal = Frame::refused(
        "ses_1",
        Refused::UnknownQuestion,
        "no",
        Some("c-8"),
        None,
        None,
    );
    assert_eq!(refusal.refusal(), Some("unknown_question"));
    assert_eq!(refusal.status(), 409);
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

/// A model call names when it left, how long it took and its tokens, and a tool call when it
/// began and its reply's size, on the wire; a mark that reported none of them names none.
#[test]
fn a_call_and_a_tool_name_their_time_and_size_only_when_known() {
    use nika_session::activity::{CallState, ToolMark, call_activity};

    let mut call = call_activity(
        2,
        "conversation",
        "deepseek/deepseek-v4-pro",
        CallState::Finished,
    );
    call.call = call
        .call
        .map(|mark| mark.timed(1_000, Some(250)).with_tokens(4_570, 41));
    let shown = serde_json::to_value(ActivityWire::of(&call)).expect("activity");
    let expected = serde_json::json!({"ordinal": 2, "role": "conversation",
        "model": "deepseek/deepseek-v4-pro", "state": "finished", "at_ms": 1000,
        "elapsed_ms": 250, "input_tokens": 4570, "output_tokens": 41});
    assert_eq!(shown["call"], expected);
    let mut tool =
        nika_session::activity::tool_activity("toolu_1", "verify", ToolState::Finished, Some(42));
    tool.tool = tool.tool.map(|mark: ToolMark| mark.timed(900, Some(312)));
    let shown = serde_json::to_value(ActivityWire::of(&tool)).expect("activity");
    assert_eq!(
        (
            shown["tool"]["at_ms"].as_u64(),
            shown["tool"]["reply_bytes"].as_u64()
        ),
        (Some(900), Some(312))
    );
    let bare = call_activity(
        1,
        "conversation",
        "deepseek/deepseek-v4-pro",
        CallState::Started,
    );
    let shown = serde_json::to_value(ActivityWire::of(&bare)).expect("activity");
    let keys: Vec<&str> = shown["call"]
        .as_object()
        .expect("a call")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["model", "ordinal", "role", "state"]);
}

/// A finished call names what it settled, as the Session priced it: `priced` with its nano-USD
/// as a decimal string, or `unpriced`; a call that settled nothing (or still runs) names no cost.
/// The published schema describes the frame's call and that cost, its two kinds alike.
#[test]
fn a_finished_call_names_what_it_settled_and_the_schema_describes_it() {
    use nika_session::activity::{CallCost, CallState, call_activity};

    let finished = |cost: Option<CallCost>| {
        let mut call = call_activity(
            3,
            "conversation",
            "deepseek/deepseek-v4-pro",
            CallState::Finished,
        );
        call.call = call.call.map(|mark| match cost {
            Some(cost) => mark.costing(cost),
            None => mark,
        });
        serde_json::to_value(ActivityWire::of(&call)).expect("activity")
    };
    assert_eq!(
        finished(Some(CallCost::Priced(120_000_000)))["call"]["cost"],
        serde_json::json!({"kind": "priced", "nano_usd": "120000000"})
    );
    assert_eq!(
        finished(Some(CallCost::Unpriced))["call"]["cost"],
        serde_json::json!({"kind": "unpriced"})
    );
    let bare = finished(None);
    assert!(bare["call"].get("cost").is_none(), "{bare}");
    let document: serde_json::Value =
        serde_json::from_str(crate::http::OPENAPI).expect("the patch is JSON");
    let schemas = &document["components"]["schemas"];
    assert_eq!(
        schemas["SessionFrame"]["properties"]["call"]["$ref"],
        "#/components/schemas/SessionCall"
    );
    assert_eq!(
        schemas["SessionCall"]["properties"]["cost"]["$ref"],
        "#/components/schemas/SessionCallCost"
    );
    let kinds: Vec<&str> = (schemas["SessionCallCost"]["oneOf"].as_array())
        .expect("one branch per kind")
        .iter()
        .filter_map(|branch| branch["properties"]["kind"]["const"].as_str())
        .collect();
    assert_eq!(kinds, ["priced", "unpriced"]);
}

/// A gate shows the values its message renders and names the keys it withheld (ADR-148 point
/// 12), as the pause wrote them; a gate whose pause wrote none carries neither member.
#[test]
fn a_gate_outcome_carries_the_values_it_shows_and_names_the_withheld() {
    let trace = std::path::Path::new(".nika/traces/t.ndjson");
    let id = nika_session::outcome::GateId::new(trace, "approve");
    let values = serde_json::json!({"sno": 7, "to": "finance@example.test", "amount": 1250.5});
    let withheld = serde_json::json!({"credential": "secret"});
    let gate = |values, withheld| TurnOutcome::GateAsk {
        id: id.clone(),
        question: "Release #7 to finance@example.test?".to_owned(),
        values,
        withheld,
    };
    let (mut wire, mut effects) = (Vec::new(), Vec::new());
    project(
        gate(
            Some(Box::new(values.clone())),
            Some(Box::new(withheld.clone())),
        ),
        &mut wire,
        &mut effects,
    );
    project(gate(None, None), &mut wire, &mut effects);
    let shown = serde_json::to_value(&wire).expect("outcomes");
    assert_eq!(
        (shown[0]["kind"].as_str(), shown[0]["task"].as_str()),
        (Some("gate"), Some("approve"))
    );
    assert_eq!(
        (&shown[0]["values"], &shown[0]["withheld"]),
        (&values, &withheld)
    );
    assert!(shown[1].get("values").is_none() && shown[1].get("withheld").is_none());
}

/// A failed tool step names why on the wire (`reply`, the excerpt its run reported); a step
/// that did not fail carries none, as both doors write the same frame.
#[test]
fn a_failed_tool_step_names_why_on_the_wire() {
    let failed =
        nika_session::activity::tool_activity("toolu_c", "compose", ToolState::Failed, Some(9));
    let why = Some("no block serves `feeds`: the key [redacted] expired".to_owned());
    let mut told = failed.clone();
    told.tool = (told.tool).map(|mark| mark.with_reply(why));
    let expected = serde_json::json!({
        "phase": "authoring", "note": "compose failed · 9 ms", "done": true,
        "tool": {"call": "toolu_c", "name": "compose", "state": "failed", "elapsed_ms": 9,
            "reply": "no block serves `feeds`: the key [redacted] expired"}
    });
    let shown = serde_json::to_value(ActivityWire::of(&told)).expect("activity");
    assert_eq!(shown, expected);
    let silent = serde_json::to_value(ActivityWire::of(&failed)).expect("activity");
    assert!(silent["tool"].get("reply").is_none());
}

#[test]
fn proposal_verification_keeps_its_full_records_beside_the_wire_excerpt() {
    let evidence = serde_json::json!({"schema": "nika/semantic-verification@1",
        "candidate_sha256": "a".repeat(64), "candidate_revision": 2, "reused": false,
        "records": [{"request": "original request ".repeat(60), "consumed": 1}]});
    let mut activity =
        nika_session::activity::tool_activity("call-p", "propose", ToolState::Finished, Some(8));
    activity.tool = activity
        .tool
        .map(|mark| mark.with_verification(Some(evidence.clone())));
    let value = serde_json::to_value(ActivityWire::of(&activity)).expect("activity");
    assert_eq!(value["tool"]["verification"], evidence);
    assert_eq!(value["tool"]["state"], "finished");
    assert_eq!(value["tool"]["call"], "call-p");
    assert!(value["tool"].get("reply").is_none());
    assert!(
        !value["note"]
            .as_str()
            .expect("note")
            .contains("original request")
    );
}
