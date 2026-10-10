// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::collections::VecDeque;

use nika_kernel::CancelCtx;
use serde_json::json;

use super::*;
use crate::event::End;
use crate::run::Store;
use crate::steer::{QueuedState, Steering};
use crate::tree::Entry;

/// The Session's tools as a test serves them: `read` answers, `ask` ends the turn.
#[derive(Default)]
struct Tools {
    calls: Mutex<Vec<ToolCall>>,
}

impl Tools {
    fn called(&self) -> Vec<String> {
        let calls = self.calls.lock().unwrap();
        calls.iter().map(|c| c.name.clone()).collect()
    }
}

impl SessionTools for Tools {
    fn tools(&self) -> Vec<ToolDef> {
        vec![
            ToolDef::new("read", "Read a file.", json!({"type": "object"}), true),
            ToolDef::new("ask", "Ask the person.", json!({"type": "object"}), false),
        ]
    }

    fn call(&self, call: ToolCall) -> ToolReply {
        self.calls.lock().unwrap().push(call.clone());
        match call.name.as_str() {
            "read" => ToolReply::ok(format!("contents of {}", call.arguments["path"])),
            "ask" => ToolReply::ends_turn("asked q1"),
            other => ToolReply::error(format!("no tool `{other}`")),
        }
    }
}

/// One scripted turn of the agent: it may call tools through the relay, stream beats and ask
/// whether to stop, then ends.
type Script =
    Box<dyn FnMut(&Relay, &mut dyn FnMut(Beat), &mut dyn FnMut() -> bool) -> Result<Led, String>>;

fn turn(
    script: impl FnMut(&Relay, &mut dyn FnMut(Beat), &mut dyn FnMut() -> bool) -> Result<Led, String>
    + 'static,
) -> Script {
    Box::new(script)
}

/// An agent with its own loop, from a script: the prompts it received, one turn each.
struct Scripted {
    relay: Arc<Relay>,
    turns: VecDeque<Script>,
    prompts: Vec<String>,
}

impl Conversant for Scripted {
    fn prompt(
        &mut self,
        text: &str,
        beats: &mut dyn FnMut(Beat),
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<Led, String> {
        self.prompts.push(text.to_owned());
        let mut next = (self.turns.pop_front()).ok_or_else(|| "no scripted turn".to_owned())?;
        next(&self.relay, beats, stop)
    }
}

/// A script entry is how a turn ends, so it is a result even when it succeeds.
#[allow(clippy::unnecessary_wraps)]
fn led(end: LedEnd) -> Result<Led, String> {
    Ok(Led::new(end, json!({"stop": {"requested": 0}})))
}

fn call(relay: &Relay, name: &str, path: &str, id: &str) -> ToolReply {
    relay.call(ToolCall::new(name, json!({"path": path})).with_meta(id))
}

/// A world: the tools, the relay over them and an agent scripted with `turns`.
fn world(turns: Vec<Script>) -> (Arc<Tools>, Arc<Relay>, Scripted) {
    let tools = Arc::new(Tools::default());
    let relay = Arc::new(Relay::new(Arc::clone(&tools) as Arc<dyn SessionTools>));
    let agent = Scripted {
        relay: Arc::clone(&relay),
        turns: turns.into(),
        prompts: Vec::new(),
    };
    (tools, relay, agent)
}

fn now() -> u64 {
    7
}

fn fresh() -> (Tree, Vec<String>) {
    let mut lines = Vec::new();
    let tree = Tree::new("s", "p", 1, |l| Store::append(&mut lines, l)).unwrap();
    (tree, lines)
}

fn kinds(entries: &[Entry]) -> Vec<String> {
    let wire = serde_json::to_value(entries).unwrap();
    (wire.as_array().unwrap().iter())
        .map(|e| e["kind"]["type"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_led_line_records_the_calls_the_agent_made_and_its_answer() {
    let (mut tree, mut lines) = fresh();
    let (tools, relay, mut agent) = world(vec![turn(|relay, beats, _| {
        let reply = call(relay, "read", "a.nika", "toolu_1");
        assert_eq!(reply.text, "contents of \"a.nika\"");
        beats(Beat::Thought("reading".into()));
        beats(Beat::Answer("Read ".into()));
        beats(Beat::Answer("it.".into()));
        led(LedEnd::Answered)
    })]);
    let mut events = Vec::new();
    let outcome = Agent::new(&mut tree, &mut lines, &*relay, &now).lead(
        "read a",
        Some("Instructions."),
        &mut agent,
        &relay,
        &mut |e| events.push(e),
    );
    assert!(matches!(&outcome, Outcome::Answered { text } if text == "Read it."));
    assert_eq!(agent.prompts, ["Instructions.\n\nread a\n\n(cited as u1)"]);
    assert_eq!(tools.called(), ["read"]);
    assert_eq!(
        kinds(tree.entries()),
        ["user", "assistant", "tool_result", "fact", "assistant"]
    );
    let entries = tree.entries();
    assert!(
        matches!(&entries[1].kind, EntryKind::Assistant { content, stop, .. }
        if *stop == StopReason::ToolUse
            && matches!(&content[..], [ContentBlock::ToolUse { id, name, .. }]
                if id == "toolu_1" && name == "read"))
    );
    assert!(matches!(&entries[3].kind, EntryKind::Fact { name, .. } if name == LED_TURN));
    assert_eq!(tree.last_said(), "Read it.");
    assert!(events.contains(&AgentEvent::ThinkingDelta {
        text: "reading".into()
    }));
    assert_eq!(
        events.last(),
        Some(&AgentEvent::AgentEnd { end: End::Answered })
    );
}

#[test]
fn an_ask_parks_the_run_and_the_next_line_answers_it() {
    let (mut tree, mut lines) = fresh();
    let (tools, relay, mut agent) = world(vec![
        turn(|relay, beats, _| {
            let asked = call(relay, "ask", "", "toolu_ask");
            assert!(asked.text.starts_with("asked q1") && asked.text.ends_with(ASKED));
            assert!(
                !asked.ends_turn,
                "over MCP the reply is text like any other"
            );
            let refused = call(relay, "read", "b.nika", "toolu_late");
            assert_eq!((refused.text.as_str(), refused.is_error), (WAITING, true));
            beats(Beat::Answer("One question.".into()));
            led(LedEnd::Answered)
        }),
        turn(|relay, beats, _| {
            call(relay, "read", "b.nika", "toolu_2");
            beats(Beat::Answer("Done.".into()));
            led(LedEnd::Answered)
        }),
    ]);
    let mut agent_run = Agent::new(&mut tree, &mut lines, &*relay, &now);
    let outcome = agent_run.lead("ask me", None, &mut agent, &relay, &mut |_| {});
    assert!(matches!(&outcome, Outcome::Parked { call, name, queued }
        if call == "toolu_ask" && name == "ask" && queued.is_empty()));
    let outcome = agent_run.lead("b, please", None, &mut agent, &relay, &mut |_| {});
    assert!(matches!(&outcome, Outcome::Answered { text } if text == "Done."));
    assert_eq!(tools.called(), ["ask", "read"], "the late call never ran");
    assert_eq!(
        agent.prompts[1],
        "The person answered, cited as u2:\nb, please"
    );
    assert_eq!(tree.parked(), None);
    assert!(tree.entries().iter().any(|e| matches!(&e.kind,
        EntryKind::User { cite, answers: Some(call), .. } if cite == "u2" && call == "toolu_ask")));
    assert_eq!(
        kinds(tree.entries()),
        [
            "user",
            "assistant",
            "parked",
            "assistant",
            "tool_result",
            "fact",
            "assistant",
            "user",
            "assistant",
            "tool_result",
            "fact",
            "assistant"
        ]
    );
}

#[test]
fn a_call_between_turns_reaches_no_tool() {
    let (tools, relay, _) = world(Vec::new());
    let reply = call(&relay, "read", "a.nika", "toolu_1");
    assert_eq!((reply.text.as_str(), reply.is_error), (NO_TURN, true));
    assert!(tools.called().is_empty());
    assert_eq!(
        relay.tools().len(),
        2,
        "the relay lists the Session's tools"
    );
}

#[test]
fn stop_asks_the_agent_once_and_returns_the_queued_lines() {
    let (mut tree, mut lines) = fresh();
    let cancel = CancelCtx::new();
    let steering = Steering::new();
    let (on_stop, queue) = (cancel.clone(), steering.clone());
    let asked = Arc::new(Mutex::new(0_u32));
    let counted = Arc::clone(&asked);
    let (_, relay, mut agent) = world(vec![turn(move |relay, _, stop| {
        call(relay, "read", "a.nika", "toolu_1");
        queue.follow_up("then c").expect("the run reads its queue");
        on_stop.cancel();
        for _ in 0..3 {
            if stop() {
                *counted.lock().unwrap() += 1;
                break;
            }
        }
        led(LedEnd::Stopped)
    })]);
    let mut run = Agent::new(&mut tree, &mut lines, &*relay, &now)
        .with_cancel(&cancel)
        .with_steering(&steering);
    let outcome = run.lead("read a", None, &mut agent, &relay, &mut |_| {});
    assert_eq!(run.stop_reach(), Some(StopReach::AgentCancelled));
    drop(run);
    assert!(matches!(&outcome, Outcome::Stopped { queued }
        if *queued == [(QueueMode::FollowUp, "then c".to_owned())]));
    assert_eq!(steering.records()[0].state, QueuedState::Returned);
    assert_eq!(*asked.lock().unwrap(), 1);
    assert_eq!(
        kinds(tree.entries()),
        ["user", "assistant", "tool_result", "fact", "stopped"],
        "the call that ran is recorded; no answer is"
    );
}

#[test]
fn a_steering_line_stops_the_turn_and_enters_as_the_next_prompt() {
    let (mut tree, mut lines) = fresh();
    let steering = Steering::new();
    let queue = steering.clone();
    let (_, relay, mut agent) = world(vec![
        turn(move |_, beats, stop| {
            beats(Beat::Answer("Using a".into()));
            queue
                .steer("use b instead")
                .expect("the run reads its queue");
            assert!(stop(), "a steering line stops the turn");
            led(LedEnd::Stopped)
        }),
        turn(|_, beats, stop| {
            assert!(!stop());
            beats(Beat::Answer("Using b.".into()));
            led(LedEnd::Answered)
        }),
    ]);
    let mut events = Vec::new();
    let outcome = Agent::new(&mut tree, &mut lines, &*relay, &now)
        .with_steering(&steering)
        .lead("use a", None, &mut agent, &relay, &mut |e| events.push(e));
    assert!(matches!(&outcome, Outcome::Answered { text } if text == "Using b."));
    assert_eq!(agent.prompts[1], "use b instead\n\n(cited as u2)");
    let entered = QueuedState::Entered { cite: "u2".into() };
    assert_eq!(steering.records()[0].state, entered);
    assert!(tree.entries().iter().any(|e| matches!(&e.kind,
        EntryKind::User { cite, queued: Some(QueueMode::Steer), .. } if cite == "u2")));
    assert_eq!(
        kinds(tree.entries()),
        ["user", "fact", "user", "fact", "assistant"],
        "the stopped turn's partial words are not recorded as an answer"
    );
    assert!(events.iter().any(|e| matches!(
        e,
        AgentEvent::Dequeued {
            mode: QueueMode::Steer,
            ..
        }
    )));
}

#[test]
fn a_follow_up_line_enters_when_the_agent_ends_its_turn() {
    let (mut tree, mut lines) = fresh();
    let steering = Steering::new();
    let queue = steering.clone();
    let (_, relay, mut agent) = world(vec![
        turn(move |_, beats, stop| {
            queue.follow_up("and c").expect("the run reads its queue");
            assert!(!stop(), "a follow-up line waits for the end of the turn");
            beats(Beat::Answer("a done".into()));
            led(LedEnd::Answered)
        }),
        turn(|_, beats, _| {
            beats(Beat::Answer("c done".into()));
            led(LedEnd::Answered)
        }),
    ]);
    let outcome = Agent::new(&mut tree, &mut lines, &*relay, &now)
        .with_steering(&steering)
        .lead("do a", None, &mut agent, &relay, &mut |_| {});
    assert!(matches!(&outcome, Outcome::Answered { text } if text == "c done"));
    assert_eq!(agent.prompts[1], "and c\n\n(cited as u2)");
    assert_eq!(
        kinds(tree.entries()),
        ["user", "fact", "assistant", "user", "fact", "assistant"]
    );
}

#[test]
fn an_unusual_end_or_a_lost_transport_fails_the_run_keeping_what_ran() {
    let (mut tree, mut lines) = fresh();
    let (_, relay, mut agent) = world(vec![
        turn(|_, _, _| led(LedEnd::Other("max_tokens".into()))),
        turn(|relay, _, _| {
            call(relay, "read", "a.nika", "toolu_1");
            Err("the agent left".to_owned())
        }),
    ]);
    let mut run = Agent::new(&mut tree, &mut lines, &*relay, &now);
    let outcome = run.lead("one", None, &mut agent, &relay, &mut |_| {});
    assert!(
        matches!(&outcome, Outcome::Failed { error: AgentError::Model(why) }
        if why.contains("max_tokens"))
    );
    let outcome = run.lead("two", None, &mut agent, &relay, &mut |_| {});
    assert!(
        matches!(&outcome, Outcome::Failed { error: AgentError::Model(why) }
        if why == "the agent left")
    );
    assert_eq!(
        kinds(tree.entries()),
        ["user", "fact", "user", "assistant", "tool_result"],
        "a call that ran is recorded even when the turn's transport failed"
    );
}

#[test]
fn a_question_asked_before_the_transport_failed_still_waits_for_the_person() {
    let (mut tree, mut lines) = fresh();
    let (_, relay, mut agent) = world(vec![turn(|relay, _, _| {
        call(relay, "ask", "", "toolu_ask");
        Err("the agent left".to_owned())
    })]);
    let outcome = Agent::new(&mut tree, &mut lines, &*relay, &now).lead(
        "ask me",
        None,
        &mut agent,
        &relay,
        &mut |_| {},
    );
    assert!(matches!(&outcome, Outcome::Parked { call, .. } if call == "toolu_ask"));
    assert_eq!(tree.parked().map(|(call, _)| call), Some("toolu_ask"));
    assert_eq!(
        kinds(tree.entries()),
        ["user", "assistant", "parked", "fact"]
    );
    let failed = tree.entries().last().map(|e| &e.kind);
    assert!(matches!(failed, Some(EntryKind::Fact { name, data })
        if name == LED_TURN && data["error"] == "the agent left"));
}

#[test]
fn a_call_without_an_identity_gets_one_the_tree_keeps_unique() {
    let (mut tree, mut lines) = fresh();
    let (_, relay, mut agent) = world(vec![turn(|relay, _, _| {
        relay.call(ToolCall::new("read", json!({"path": "a"})));
        relay.call(ToolCall::new("read", json!({"path": "b"})));
        led(LedEnd::Answered)
    })]);
    Agent::new(&mut tree, &mut lines, &*relay, &now).lead(
        "x",
        None,
        &mut agent,
        &relay,
        &mut |_| {},
    );
    let ids: Vec<&str> = (tree.entries().iter())
        .filter_map(|e| match &e.kind {
            EntryKind::ToolResult { call, .. } => Some(call.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(ids, ["led-2", "led-4"]);
}

#[test]
fn the_transcript_gives_a_new_agent_the_cited_lines_and_what_was_asked() {
    let (mut tree, mut lines) = fresh();
    assert_eq!(transcript(&tree), None);
    let (_, relay, mut agent) = world(vec![
        turn(|relay, beats, _| {
            let question = json!({"questions": [{"key": "out", "question": "Which file?"}]});
            relay.call(ToolCall::new("ask", question).with_meta("toolu_ask"));
            beats(Beat::Answer("I need the file.".into()));
            led(LedEnd::Answered)
        }),
        turn(|relay, _, _| {
            call(relay, "read", "out.md", "toolu_r");
            led(LedEnd::Answered)
        }),
    ]);
    let mut run = Agent::new(&mut tree, &mut lines, &*relay, &now);
    run.lead("write a digest", None, &mut agent, &relay, &mut |_| {});
    run.lead("out.md", None, &mut agent, &relay, &mut |_| {});
    let text = transcript(&tree).unwrap();
    let expected = "The person (cited as u1): write a digest\n\
        The author asked: Which file?\n\
        The author said: I need the file.\n\
        The person answered (cited as u2): out.md\n\
        The author called `read`.";
    assert_eq!(text, expected);
}
