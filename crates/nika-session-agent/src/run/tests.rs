// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use nika_kernel::provider::Role;
use nika_session_change::tools::ToolDef as SessionToolDef;
use serde_json::json;

use super::*;
use crate::steer::QueuedState;
use crate::tree::Entry;

/// A model that answers from a script, records every request and may act while it answers
/// (the person typing, or pressing Stop).
struct Script {
    replies: VecDeque<Result<Reply, ModelError>>,
    seen: Vec<Request>,
    during: Vec<Box<dyn FnMut(usize)>>,
}

impl Script {
    fn new(replies: Vec<Result<Reply, ModelError>>) -> Self {
        Self {
            replies: replies.into(),
            seen: Vec::new(),
            during: Vec::new(),
        }
    }

    fn during(mut self, act: impl FnMut(usize) + 'static) -> Self {
        self.during.push(Box::new(act));
        self
    }
}

impl Model for Script {
    fn complete(
        &mut self,
        request: &Request,
        events: &mut dyn FnMut(AgentEvent),
    ) -> Result<Reply, ModelError> {
        let rank = self.seen.len();
        self.seen.push(request.clone());
        for act in &mut self.during {
            act(rank);
        }
        let reply = self.replies.pop_front();
        let reply = reply.unwrap_or_else(|| Err(ModelError::Failed("no scripted reply".into())));
        if let Ok(reply) = &reply {
            let text = answer_text(&reply.content);
            if !text.is_empty() {
                events(AgentEvent::TextDelta { text });
            }
        }
        reply
    }
}

/// The Session's tools as a test serves them: `read` answers, `ask` ends the turn, and a hook
/// may act during a call.
struct Tools {
    calls: Mutex<Vec<ToolCall>>,
    hook: Box<dyn Fn(&ToolCall) + Send + Sync>,
    /// The observations running now, and the most that ran at once.
    observing: AtomicUsize,
    most: AtomicUsize,
    /// The revision a candidate write answers with, one more per write.
    revision: AtomicUsize,
}

impl Tools {
    fn new() -> Self {
        Self::with_hook(|_| {})
    }

    fn with_hook(hook: impl Fn(&ToolCall) + Send + Sync + 'static) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            hook: Box::new(hook),
            observing: AtomicUsize::new(0),
            most: AtomicUsize::new(0),
            revision: AtomicUsize::new(0),
        }
    }

    fn called(&self) -> Vec<(String, Option<String>)> {
        let calls = self.calls.lock().unwrap();
        calls
            .iter()
            .map(|c| (c.name.clone(), c.meta.clone()))
            .collect()
    }
}

impl SessionTools for Tools {
    fn tools(&self) -> Vec<SessionToolDef> {
        vec![
            SessionToolDef::new("read", "Read a file.", json!({"type": "object"}), true),
            SessionToolDef::new("ask", "Ask the person.", json!({"type": "object"}), false),
        ]
    }

    fn call(&self, call: ToolCall) -> ToolReply {
        (self.hook)(&call);
        self.calls.lock().unwrap().push(call.clone());
        match call.name.as_str() {
            "read" => ToolReply::ok(format!("contents of {}", call.arguments["path"])),
            "ask" => ToolReply::ends_turn("asked q1"),
            // A candidate write refused the same way each time, under a new revision.
            "write" => {
                let revision = self.revision.fetch_add(1, Ordering::SeqCst) + 1;
                let findings = ["INVENTED LITERAL: `https://example.org/item`"];
                ToolReply::ok(json!({"findings": findings, "revision": revision}).to_string())
            }
            "observe" => {
                let now = self.observing.fetch_add(1, Ordering::SeqCst) + 1;
                self.most.fetch_max(now, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(80));
                self.observing.fetch_sub(1, Ordering::SeqCst);
                ToolReply::ok(format!("page of {}", call.arguments["url"]))
            }
            other => ToolReply::error(format!("no tool `{other}`")),
        }
    }
}

/// A script entry is what a model answers, so it is a result even when it succeeds.
#[allow(clippy::unnecessary_wraps)]
fn says(words: &str) -> Result<Reply, ModelError> {
    let text = ContentBlock::Text { text: words.into() };
    Ok(Reply::new(vec![text], StopReason::EndTurn))
}

#[allow(clippy::unnecessary_wraps)]
fn calls_to(made: &[(&str, &str, &str)]) -> Result<Reply, ModelError> {
    let blocks = (made.iter())
        .map(|(id, name, path)| ContentBlock::ToolUse {
            id: (*id).into(),
            name: (*name).into(),
            input: json!({"path": path}),
        })
        .collect();
    Ok(Reply::new(blocks, StopReason::ToolUse).with_usage(TokenUsage::new(10, 5)))
}

/// A message cut at the output limit while it wrote `name`'s arguments: the provider passes
/// them on as the text they were.
#[allow(clippy::unnecessary_wraps)]
fn cut_call(id: &str, name: &str) -> Result<Reply, ModelError> {
    let half = ContentBlock::ToolUse {
        id: id.into(),
        name: name.into(),
        input: Value::String(r#"{"path": "a.ni"#.into()),
    };
    Ok(Reply::new(vec![half], StopReason::MaxTokens))
}

/// A message that only reasoned: no word and no call.
#[allow(clippy::unnecessary_wraps)]
fn thinks(thought: &str) -> Result<Reply, ModelError> {
    let thinking = ContentBlock::Thinking {
        text: thought.into(),
    };
    Ok(Reply::new(vec![thinking], StopReason::EndTurn))
}

fn now() -> u64 {
    7
}

fn fresh() -> (Tree, Vec<String>) {
    let mut lines = Vec::new();
    let tree = Tree::new("s", "p", 1, |l| Store::append(&mut lines, l)).unwrap();
    (tree, lines)
}

/// The tool results of the last message of a request: `(call, text, is_error)`.
fn last_results(request: &Request) -> Vec<(String, String, bool)> {
    let last = request.messages.last().unwrap();
    (last.content.iter())
        .filter_map(|block| match block {
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => Some((tool_use_id.clone(), content.clone(), *is_error)),
            _ => None,
        })
        .collect()
}

fn kinds(entries: &[Entry]) -> Vec<String> {
    let wire = serde_json::to_value(entries).unwrap();
    (wire.as_array().unwrap().iter())
        .map(|e| e["kind"]["type"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_run_goes_on_until_the_model_answers_without_a_call() {
    let (mut tree, mut lines) = fresh();
    let tools = Tools::new();
    let mut model = Script::new(vec![
        calls_to(&[("c1", "read", "a.nika")]),
        calls_to(&[("c2", "read", "b.nika")]),
        says("Both read."),
    ]);
    let mut events = Vec::new();
    let outcome =
        Agent::new(&mut tree, &mut lines, &tools, &now)
            .prompt("read both", &mut model, &mut |e| events.push(e));
    assert!(matches!(&outcome, Outcome::Answered { text } if text == "Both read."));
    let meta = |id: &str| Some(id.to_owned());
    assert_eq!(
        tools.called(),
        [("read".into(), meta("c1")), ("read".into(), meta("c2"))]
    );
    assert_eq!(model.seen.len(), 3);
    let names: Vec<&str> = model.seen[0]
        .tools
        .iter()
        .map(|t| t.name.as_str())
        .collect();
    assert_eq!(names, ["read", "ask"]);
    let replied = last_results(&model.seen[1]);
    assert_eq!(
        replied,
        [("c1".into(), "contents of \"a.nika\"".into(), false)]
    );
    assert_eq!(
        kinds(tree.entries()),
        [
            "user",
            "assistant",
            "tool_result",
            "assistant",
            "tool_result",
            "assistant"
        ]
    );
    assert_eq!(
        events.first(),
        Some(&AgentEvent::AgentStart { entry: "e1".into() })
    );
    assert_eq!(
        events.last(),
        Some(&AgentEvent::AgentEnd { end: End::Answered })
    );
    let turns = events
        .iter()
        .filter(|e| matches!(e, AgentEvent::TurnStart { .. }))
        .count();
    assert_eq!(turns, 3);
    assert!(events.contains(&AgentEvent::Usage {
        input_tokens: 10,
        output_tokens: 5
    }));
    assert_eq!(
        Tree::replay(&format!("{}\n", lines.join("\n")))
            .unwrap()
            .entries()
            .len(),
        6
    );
}

/// No step, turn or call quota ends a run: only the model's answer does.
#[test]
fn a_message_cut_at_the_output_limit_runs_none_of_its_calls() {
    let (mut tree, mut lines) = fresh();
    let tools = Tools::new();
    let cut_words = Reply::new(
        vec![ContentBlock::Text {
            text: "Le résumé est".into(),
        }],
        StopReason::MaxTokens,
    );
    let mut model = Script::new(vec![
        cut_call("c1", "read"),
        calls_to(&[("c2", "read", "a.nika")]),
        Ok(cut_words),
    ]);
    let outcome =
        Agent::new(&mut tree, &mut lines, &tools, &now).prompt("read it", &mut model, &mut |_| {});
    // The cut call never ran; the author read why, and its complete call ran.
    assert_eq!(tools.called(), [("read".into(), Some("c2".to_owned()))]);
    let refused = last_results(&model.seen[1]);
    assert_eq!(refused, [("c1".into(), CUT.into(), true)]);
    let marked = format!("Le résumé est{CUT_ANSWER}");
    assert!(matches!(&outcome, Outcome::Answered { text } if *text == marked));
}

/// Two candidate writes in one run, then a new person line: what the second run reads.
fn writes_then_a_line(first: Vec<ContentBlock>) -> (Vec<serde_json::Value>, String) {
    let (mut tree, mut lines) = fresh();
    let tools = Tools::new();
    let write = |id: &str, source: &str| ContentBlock::ToolUse {
        id: id.into(),
        name: "candidate_write".into(),
        input: json!({"source": source}),
    };
    let mut second = first;
    second.push(write("c2", "nika: second"));
    let mut model = Script::new(vec![
        Ok(Reply::new(
            vec![write("c1", "nika: first")],
            StopReason::ToolUse,
        )),
        Ok(Reply::new(second, StopReason::ToolUse)),
        says("Written."),
        says("Still here."),
    ]);
    let run = |tree: &mut Tree, lines: &mut Vec<String>, line: &str, model: &mut Script| {
        Agent::new(tree, lines, &tools, &now).prompt(line, model, &mut |_| {})
    };
    assert!(matches!(
        run(&mut tree, &mut lines, "write it", &mut model),
        Outcome::Answered { .. }
    ));
    assert!(matches!(
        run(&mut tree, &mut lines, "and now?", &mut model),
        Outcome::Answered { .. }
    ));
    let read = model.seen.last().unwrap();
    let inputs = (read.messages.iter())
        .flat_map(|m| &m.content)
        .filter_map(|block| match block {
            ContentBlock::ToolUse { input, .. } => Some(input.clone()),
            _ => None,
        })
        .collect();
    (inputs, serde_json::to_string(tree.entries()).unwrap())
}

#[test]
fn a_new_run_reads_a_superseded_candidate_as_a_stub_and_the_record_keeps_it() {
    let (inputs, kept) = writes_then_a_line(Vec::new());
    let stub = json!({"stub": crate::context_edit::SUPERSEDED});
    assert_eq!(inputs, [stub, json!({"source": "nika: second"})]);
    assert!(kept.contains("nika: first"), "the record keeps it whole");
}

#[test]
fn nothing_before_a_replayed_thinking_block_is_stubbed() {
    let thought = ContentBlock::Thinking {
        text: "the second source".into(),
    };
    let (inputs, _) = writes_then_a_line(vec![thought]);
    let whole = [
        json!({"source": "nika: first"}),
        json!({"source": "nika: second"}),
    ];
    assert_eq!(inputs, whole);
}

#[test]
fn the_observations_of_one_message_run_side_by_side_in_its_order() {
    let (mut tree, mut lines) = fresh();
    let tools = Tools::new();
    let observe = |id: &str, url: &str| ContentBlock::ToolUse {
        id: id.into(),
        name: "observe".into(),
        input: json!({"url": url}),
    };
    let blocks = vec![
        observe("c1", "https://a.example"),
        observe("c2", "https://b.example"),
        observe("c3", "https://c.example"),
    ];
    let mut model = Script::new(vec![
        Ok(Reply::new(blocks, StopReason::ToolUse)),
        says("Read."),
    ]);
    let outcome =
        Agent::new(&mut tree, &mut lines, &tools, &now).prompt("observe", &mut model, &mut |_| {});
    assert!(matches!(outcome, Outcome::Answered { .. }));
    assert!(tools.most.load(Ordering::SeqCst) >= 2, "side by side");
    let results = last_results(&model.seen[1]);
    let order: Vec<&str> = results.iter().map(|(id, ..)| id.as_str()).collect();
    assert_eq!(order, ["c1", "c2", "c3"]);
}

#[test]
fn no_quota_ends_a_long_run() {
    let (mut tree, mut lines) = fresh();
    let tools = Tools::new();
    let ids: Vec<String> = (0..120).map(|k| format!("c{k}")).collect();
    let mut replies: Vec<_> = ids
        .iter()
        .map(|id| calls_to(&[(id.as_str(), "read", id.as_str())]))
        .collect();
    replies.push(says("Done."));
    let mut model = Script::new(replies);
    let outcome =
        Agent::new(&mut tree, &mut lines, &tools, &now).prompt("go", &mut model, &mut |_| {});
    assert!(matches!(outcome, Outcome::Answered { .. }));
    assert_eq!(tools.called().len(), 120);
}

#[test]
fn a_call_that_waits_for_the_person_parks_the_run_until_they_answer() {
    let (mut tree, mut lines) = fresh();
    let tools = Tools::new();
    let mut model = Script::new(vec![
        calls_to(&[("c1", "ask", "-"), ("c2", "read", "a.nika")]),
        says("Thanks, Le Monde it is."),
    ]);
    let mut agent = Agent::new(&mut tree, &mut lines, &tools, &now);
    let outcome = agent.prompt("a news digest", &mut model, &mut |_| {});
    let Outcome::Parked { call, name, queued } = outcome else {
        panic!("parked");
    };
    assert_eq!(
        (call.as_str(), name.as_str(), queued.len()),
        ("c1", "ask", 0)
    );
    assert_eq!(
        tools.called().len(),
        1,
        "the call after the one that waits is not run"
    );

    let refused = agent.prompt("another line", &mut model, &mut |_| {});
    assert!(
        matches!(refused, Outcome::Failed { error: AgentError::Parked { call } } if call == "c1")
    );
    let outcome = agent.answer("Le Monde aussi", &mut model, &mut |_| {});
    assert!(matches!(outcome, Outcome::Answered { .. }));
    let replies = last_results(&model.seen[1]);
    assert_eq!(
        replies[0],
        (
            "c1".into(),
            "The person answered, cited as u2:\nLe Monde aussi".into(),
            false
        )
    );
    assert_eq!(replies[1].0, "c2");
    assert!(replies[1].1.starts_with("Not run: the turn ended on `ask`") && replies[1].2);
    let after = agent.answer("again", &mut model, &mut |_| {});
    assert!(matches!(
        after,
        Outcome::Failed {
            error: AgentError::NotParked
        }
    ));
}

/// A late reply (`with_late_reply`) is a person's line that starts a run while nothing waits:
/// the tree records it under the `ask` call it replies to, and the model reads it as the ordinary
/// line it is, the Session's reading of it beside it, never as a second reply to that call.
/// Without one, a line names no call.
#[test]
fn a_late_reply_is_recorded_under_its_call_as_an_ordinary_line() {
    let (mut tree, mut lines) = fresh();
    let tools = Tools::new();
    let mut model = Script::new(vec![
        calls_to(&[("c1", "ask", "-")]),
        says("Noted."),
        says("Thanks."),
        says("Again."),
    ]);
    let mut agent = Agent::new(&mut tree, &mut lines, &tools, &now);
    agent.prompt("a news digest", &mut model, &mut |_| {});
    let answered = agent.answer("Le Monde", &mut model, &mut |_| {});
    assert!(matches!(answered, Outcome::Answered { .. }));
    let picked = ["picked `le-monde`".to_owned()];
    let mut late = Agent::new(&mut tree, &mut lines, &tools, &now)
        .with_reading(&picked)
        .with_late_reply(Some("c1".into()));
    let outcome = late.prompt("TechCrunch too", &mut model, &mut |_| {});
    assert!(matches!(outcome, Outcome::Answered { text } if text == "Thanks."));
    let request = &model.seen[2];
    let last = request.messages.last().unwrap();
    assert_eq!(last.role, Role::User);
    let words: Vec<&str> = (last.content.iter())
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    let read = "Its reading of this answer:\n- picked `le-monde`";
    let note = format!("Nika (not the person):\n{read}");
    assert_eq!(words, ["TechCrunch too", "(cited as u3)", note.as_str()]);
    let results = (request.messages.iter()).flat_map(|m| &m.content);
    let to_c1 = results
        .filter(
            |b| matches!(b, ContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == "c1"),
        )
        .count();
    assert_eq!(to_c1, 1, "the first answer stays the call's only reply");
    let plain =
        Agent::new(&mut tree, &mut lines, &tools, &now).prompt("and then", &mut model, &mut |_| {});
    assert!(matches!(plain, Outcome::Answered { .. }));
    let people: Vec<(String, Option<String>, Option<String>)> = (tree.branch().iter())
        .filter_map(|entry| match &entry.kind {
            EntryKind::User {
                cite,
                answers,
                nika,
                ..
            } => Some((cite.clone(), answers.clone(), nika.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        people,
        [
            ("u1".into(), None, None),
            ("u2".into(), Some("c1".into()), None),
            ("u3".into(), Some("c1".into()), Some(read.into())),
            ("u4".into(), None, None),
        ]
    );
}

#[test]
fn a_steering_line_enters_after_the_current_call_and_the_rest_wait() {
    let (mut tree, mut lines) = fresh();
    let steering = Steering::new();
    let person = steering.clone();
    let tools = Tools::with_hook(move |call| {
        if call.meta.as_deref() == Some("c1") {
            person
                .steer("use Le Monde too")
                .expect("the run reads its queue");
        }
    });
    let mut model = Script::new(vec![
        calls_to(&[("c1", "read", "a.nika"), ("c2", "ask", "b.nika")]),
        says("Adding Le Monde."),
    ]);
    let mut events = Vec::new();
    let outcome = Agent::new(&mut tree, &mut lines, &tools, &now)
        .with_steering(&steering)
        .prompt("read both", &mut model, &mut |e| events.push(e));
    assert!(matches!(outcome, Outcome::Answered { .. }));
    assert_eq!(tools.called().len(), 1);
    let second = &model.seen[1];
    let results = last_results(&Request::new(
        None,
        second.messages[..3].to_vec(),
        Vec::new(),
    ));
    assert_eq!(results[1], ("c2".into(), STEERED.into(), true));
    let steered = second.messages.last().unwrap();
    assert_eq!(steered.role, Role::User);
    assert!(
        matches!(&steered.content[0], ContentBlock::Text { text } if text == "use Le Monde too")
    );
    let mode = QueueMode::Steer;
    assert!(events.contains(&AgentEvent::Dequeued {
        mode,
        entry: "e5".into()
    }));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::ToolSkipped { call, .. } if call == "c2"))
    );
    let records = steering.records();
    assert_eq!(records[0].id, "l1");
    assert_eq!(records[0].state, QueuedState::Entered { cite: "u2".into() });
    assert!(
        !steering.reading(),
        "the run closed its queue when it ended"
    );
}

#[test]
fn a_steering_line_holds_back_only_the_calls_that_hand_the_turn_over() {
    let (mut tree, mut lines) = fresh();
    let steering = Steering::new();
    let person = steering.clone();
    let tools = Tools::with_hook(move |call| {
        if call.meta.as_deref() == Some("c1") {
            person
                .steer("use Le Monde too")
                .expect("the run reads its queue");
        }
    });
    let mut model = Script::new(vec![
        calls_to(&[("c1", "read", "a.nika"), ("c2", "read", "b.nika")]),
        says("Adding Le Monde."),
    ]);
    let outcome = Agent::new(&mut tree, &mut lines, &tools, &now)
        .with_steering(&steering)
        .prompt("read both", &mut model, &mut |_| {});
    assert!(matches!(outcome, Outcome::Answered { .. }));
    // A read still runs; the person's line enters once the message's calls ran.
    assert_eq!(tools.called().len(), 2);
}

#[test]
fn a_follow_up_waits_until_the_model_would_end() {
    let (mut tree, mut lines) = fresh();
    let steering = Steering::new();
    let person = steering.clone();
    let tools = Tools::new();
    let mut model =
        Script::new(vec![says("Here is the digest."), says("Summarized.")]).during(move |rank| {
            if rank == 0 {
                person
                    .follow_up("and summarize it in French")
                    .expect("the run reads its queue");
            }
        });
    let outcome = Agent::new(&mut tree, &mut lines, &tools, &now)
        .with_steering(&steering)
        .prompt("a digest", &mut model, &mut |_| {});
    assert!(matches!(&outcome, Outcome::Answered { text } if text == "Summarized."));
    assert_eq!(model.seen.len(), 2);
    assert_eq!(
        kinds(tree.entries()),
        ["user", "assistant", "user", "assistant"]
    );
    let follow = serde_json::to_value(&tree.entries()[2]).unwrap();
    assert_eq!(follow["kind"]["queued"], "follow_up");
    assert_eq!(follow["kind"]["cite"], "u2");
}

#[test]
fn stop_returns_the_queued_lines_unsent() {
    let (mut tree, mut lines) = fresh();
    let (steering, cancel) = (Steering::new(), CancelCtx::new());
    let (person, stop) = (steering.clone(), cancel.clone());
    let tools = Tools::new();
    let mut model = Script::new(vec![Err(ModelError::Stopped)]).during(move |_| {
        person
            .steer("not this one")
            .expect("the run reads its queue");
        stop.cancel();
    });
    let mut events = Vec::new();
    let mut run = Agent::new(&mut tree, &mut lines, &tools, &now)
        .with_steering(&steering)
        .with_cancel(&cancel);
    let outcome = run.prompt("a digest", &mut model, &mut |e| events.push(e));
    assert_eq!(run.stop_reach(), Some(StopReach::RequestDropped));
    drop(run);
    let Outcome::Stopped { queued } = outcome else {
        panic!("stopped");
    };
    assert_eq!(steering.records()[0].state, QueuedState::Returned);
    assert_eq!(queued, [(QueueMode::Steer, "not this one".to_owned())]);
    assert_eq!(kinds(tree.entries()), ["user", "stopped"]);
    assert_eq!(model.seen.len(), 1);
    assert_eq!(
        events.last(),
        Some(&AgentEvent::AgentEnd { end: End::Stopped })
    );
}

#[test]
fn stop_during_a_call_leaves_the_next_calls_unrun() {
    let (mut tree, mut lines) = fresh();
    let cancel = CancelCtx::new();
    let stop = cancel.clone();
    let tools = Tools::with_hook(move |_| stop.cancel());
    let mut model = Script::new(vec![calls_to(&[("c1", "read", "a"), ("c2", "read", "b")])]);
    let mut run = Agent::new(&mut tree, &mut lines, &tools, &now).with_cancel(&cancel);
    let outcome = run.prompt("read", &mut model, &mut |_| {});
    assert_eq!(run.stop_reach(), Some(StopReach::BetweenSteps));
    drop(run);
    assert!(matches!(outcome, Outcome::Stopped { .. }));
    assert_eq!(tools.called().len(), 1);
    assert_eq!(
        kinds(tree.entries()),
        ["user", "assistant", "tool_result", "stopped"]
    );
    let context = tree.context();
    let results = last_results(&Request::new(None, context.messages, Vec::new()));
    assert_eq!(results[1].0, "c2");
    assert!(results[1].2, "the unrun call reads as not run");
}

#[test]
fn a_repeated_call_is_said_so_never_stopped() {
    let (mut tree, mut lines) = fresh();
    let tools = Tools::new();
    let mut model = Script::new(vec![
        calls_to(&[("c1", "read", "a.nika")]),
        calls_to(&[("c2", "read", "a.nika")]),
        says("Same file."),
    ]);
    let outcome =
        Agent::new(&mut tree, &mut lines, &tools, &now).prompt("read", &mut model, &mut |_| {});
    assert!(matches!(outcome, Outcome::Answered { .. }));
    let first = last_results(&model.seen[1]);
    let second = last_results(&model.seen[2]);
    assert!(!first[0].1.ends_with(REPEATED));
    assert!(second[0].1.ends_with(REPEATED));
}

/// The same call with the same reply a third time in a row reads as a stall the model is told
/// to break, still never stopped: live gauntlet q03, 80 identical refused calls in 15 minutes.
/// Carriage returns a model slipped into a call's arguments never reach the tool nor the record
/// the model reads back (live gauntlet r05: `schedule\r_zone`, `Indian\r/Mauritius`).
#[test]
fn carriage_returns_are_left_out_of_a_calls_arguments() {
    let (mut tree, mut lines) = fresh();
    let tools = Tools::new();
    let input = json!({"path": "a\r.nika", "lines": ["x\r\ny"], "ke\ry": "toutes\r les\r heures"});
    let call = ContentBlock::ToolUse {
        id: "c1".into(),
        name: "read".into(),
        input,
    };
    let mut model = Script::new(vec![
        Ok(Reply::new(vec![call], StopReason::ToolUse)),
        says("Done."),
    ]);
    let outcome =
        Agent::new(&mut tree, &mut lines, &tools, &now).prompt("read", &mut model, &mut |_| {});
    assert!(matches!(outcome, Outcome::Answered { .. }));
    let calls = tools.calls.lock().unwrap();
    assert_eq!(
        calls[0].arguments,
        json!({"path": "a.nika", "lines": ["x\ny"], "key": "toutes les heures"})
    );
    drop(calls);
    let recorded = serde_json::to_string(tree.entries()).unwrap();
    assert!(!recorded.contains("\\r"), "{recorded}");
}

/// A write refused the same way under a new revision number each time is still the same reply:
/// the revision counts the calls, never what they answered (live gauntlet r04, 2026-10-16: some
/// sixty identical `candidate_write` calls from Grok 4.7, never told they repeated).
#[test]
fn a_reply_that_differs_only_by_its_revision_is_still_a_repetition() {
    let (mut tree, mut lines) = fresh();
    let tools = Tools::new();
    let mut model = Script::new(vec![
        calls_to(&[("c1", "write", "a.nika")]),
        calls_to(&[("c2", "write", "a.nika")]),
        calls_to(&[("c3", "write", "a.nika")]),
        says("Done."),
    ]);
    let outcome =
        Agent::new(&mut tree, &mut lines, &tools, &now).prompt("write", &mut model, &mut |_| {});
    assert!(matches!(outcome, Outcome::Answered { .. }));
    let second = last_results(&model.seen[2]);
    assert!(second[0].1.ends_with(REPEATED), "{}", second[0].1);
    let third = last_results(&model.seen[3]);
    assert!(third[0].1.ends_with(&stalled(3)), "{}", third[0].1);
    assert!(third[0].1.contains("\"revision\":3"), "{}", third[0].1);
}

#[test]
fn a_call_stalled_three_times_is_told_to_change_never_stopped() {
    let (mut tree, mut lines) = fresh();
    let tools = Tools::new();
    let mut model = Script::new(vec![
        calls_to(&[("c1", "read", "a.nika")]),
        calls_to(&[("c2", "read", "a.nika")]),
        calls_to(&[("c3", "read", "a.nika")]),
        calls_to(&[("c4", "read", "b.nika")]),
        says("Done."),
    ]);
    let outcome =
        Agent::new(&mut tree, &mut lines, &tools, &now).prompt("read", &mut model, &mut |_| {});
    assert!(matches!(outcome, Outcome::Answered { .. }));
    let third = last_results(&model.seen[3]);
    assert!(
        third[0].1.ends_with(&stalled(3)) && !third[0].1.contains(REPEATED),
        "{}",
        third[0].1
    );
    let fourth = last_results(&model.seen[4]);
    assert!(
        !fourth[0].1.contains("(Nika:"),
        "another call starts a new count: {}",
        fourth[0].1
    );
}

#[test]
fn a_failing_model_keeps_what_was_recorded() {
    let (mut tree, mut lines) = fresh();
    let tools = Tools::new();
    let failed = Err(ModelError::Failed("rate limited".into()));
    let mut model = Script::new(vec![calls_to(&[("c1", "read", "a")]), failed]);
    let outcome =
        Agent::new(&mut tree, &mut lines, &tools, &now).prompt("read", &mut model, &mut |_| {});
    assert!(
        matches!(outcome, Outcome::Failed { error: AgentError::Model(why) } if why == "rate limited")
    );
    assert_eq!(kinds(tree.entries()), ["user", "assistant", "tool_result"]);
}

struct Refusing;

impl Store for Refusing {
    fn append(&mut self, _: &str) -> io::Result<()> {
        Err(io::Error::other("disk full"))
    }
}

#[test]
fn a_line_that_cannot_be_recorded_starts_nothing() {
    let (mut tree, _) = fresh();
    let tools = Tools::new();
    let mut model = Script::new(vec![says("never")]);
    let outcome =
        Agent::new(&mut tree, &mut Refusing, &tools, &now).prompt("hi", &mut model, &mut |_| {});
    assert!(matches!(
        outcome,
        Outcome::Failed {
            error: AgentError::Tree(TreeError::Write(_))
        }
    ));
    assert!(model.seen.is_empty());
    assert!(tree.entries().is_empty());
}

#[test]
fn the_window_compacts_before_a_request_that_would_not_fit() {
    let (mut tree, mut lines) = fresh();
    let tools = Tools::new();
    let mut model = Script::new(vec![
        says("Done.").map(|reply| reply.with_usage(TokenUsage::new(800, 2))),
        says("SUMMARY"),
        says("Next."),
    ]);
    let mut agent =
        Agent::new(&mut tree, &mut lines, &tools, &now).with_window(Window::new(1_000, 200, 100));
    // The reported 802 tokens plus the next line exceed the 800-token input budget.
    let first = agent.prompt(&"a".repeat(1_200), &mut model, &mut |_| {});
    assert!(matches!(first, Outcome::Answered { .. }));
    assert_eq!(model.seen.len(), 1);
    let mut events = Vec::new();
    let second = agent.prompt("and now?", &mut model, &mut |e| events.push(e));
    assert!(matches!(&second, Outcome::Answered { text } if text == "Next."));
    let summary = &model.seen[1];
    assert!(summary.tools.is_empty());
    let answered = &model.seen[2];
    let ContentBlock::Text { text } = &answered.messages[0].content[0] else {
        panic!("a summary first");
    };
    // The summary, then the person's folded line beside it, verbatim with its citation.
    assert!(text.contains("\n\nSUMMARY\n\n"), "{text}");
    let folded = format!("\"cite\":\"u1\",\"text\":\"{}\"", "a".repeat(1_200));
    assert!(text.contains(&folded), "{text}");
    assert_eq!(
        answered.messages.len(),
        2,
        "the summary, then the kept line"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::Compacted { .. }))
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AgentEvent::TextDelta { text } if text == "SUMMARY"))
    );
}

#[test]
fn a_request_carries_the_branch_instructions_and_the_persons_citations() {
    let (mut tree, mut lines) = fresh();
    let system = EntryKind::System {
        text: "Author with the person.".into(),
    };
    tree.append(system, 1, |l| Store::append(&mut lines, l))
        .unwrap();
    let tools = Tools::new();
    let mut model = Script::new(vec![says("Hello.")]);
    let outcome =
        Agent::new(&mut tree, &mut lines, &tools, &now).prompt("hi", &mut model, &mut |_| {});
    assert!(matches!(outcome, Outcome::Answered { .. }));
    let request = &model.seen[0];
    assert_eq!(request.system.as_deref(), Some("Author with the person."));
    let cited = &request.messages[0].content[1];
    assert!(matches!(cited, ContentBlock::Text { text } if text == "(cited as u1)"));
}

/// A message with no word and no call is asked once more, in Nika's words; the answer to that
/// ends the run.
#[test]
fn a_message_that_only_reasoned_is_asked_once_more() {
    let (mut tree, mut lines) = fresh();
    let tools = Tools::new();
    let mut model = Script::new(vec![thinks("which feed first?"), says("Voici le plan.")]);
    let outcome =
        Agent::new(&mut tree, &mut lines, &tools, &now).prompt("go", &mut model, &mut |_| {});
    assert!(matches!(&outcome, Outcome::Answered { text } if text == "Voici le plan."));
    assert_eq!(model.seen.len(), 2);
    let asked = model.seen[1].messages.last().unwrap();
    assert_eq!(asked.role, Role::User);
    let text = answer_text(&asked.content);
    assert!(text.starts_with("Nika (not the person):"), "{text}");
    assert!(text.contains("no words and no call"), "{text}");
    assert_eq!(
        kinds(tree.entries()),
        ["user", "assistant", "note", "assistant"]
    );
}

/// Asked once more, a model that reasons without a word again ends the run as it is: no third
/// call, an empty answer the Session says in its own words.
#[test]
fn a_model_that_only_reasons_twice_ends_the_run_without_a_third_call() {
    let (mut tree, mut lines) = fresh();
    let tools = Tools::new();
    let mut model = Script::new(vec![thinks("hmm"), thinks("still hmm")]);
    let outcome =
        Agent::new(&mut tree, &mut lines, &tools, &now).prompt("go", &mut model, &mut |_| {});
    assert!(matches!(&outcome, Outcome::Answered { text } if text.is_empty()));
    assert_eq!(model.seen.len(), 2);
    assert_eq!(
        kinds(tree.entries()),
        ["user", "assistant", "note", "assistant"]
    );
}

/// A summary cut at the provider's output limit is no summary: the turn says why, nothing is
/// folded, no request is sent over the cut meaning, and every original entry stays in the branch.
#[test]
fn a_summary_cut_at_the_output_limit_folds_nothing() {
    let (mut tree, mut lines) = fresh();
    let tools = Tools::new();
    let cut = Reply::new(
        vec![ContentBlock::Text {
            text: "The person asked for".into(),
        }],
        StopReason::MaxTokens,
    );
    let mut model = Script::new(vec![
        says("Done.").map(|reply| reply.with_usage(TokenUsage::new(800, 2))),
        Ok(cut),
    ]);
    let mut events = Vec::new();
    let second = {
        let mut agent = Agent::new(&mut tree, &mut lines, &tools, &now)
            .with_window(Window::new(1_000, 200, 100));
        let first = agent.prompt(&"a".repeat(1_200), &mut model, &mut |_| {});
        assert!(matches!(first, Outcome::Answered { .. }));
        agent.prompt("and now?", &mut model, &mut |e| events.push(e))
    };
    let refused = matches!(&second, Outcome::Failed { error: AgentError::Model(why) }
        if why.contains("cut at the output limit"));
    assert!(refused, "the turn says why");
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AgentEvent::Compacted { .. })),
        "nothing folded"
    );
    assert_eq!(model.seen.len(), 2, "no request after the cut summary");
    let context = tree.context();
    assert_eq!(
        context.messages.len(),
        3,
        "the line, its answer and the new line stay"
    );
}

/// A line steered while the summary is being made enters the next request, once: the context the
/// model reads right after the compaction carries the correction, and nothing asks again.
#[test]
fn a_line_steered_during_compaction_enters_the_next_request_once() {
    let (mut tree, mut lines) = fresh();
    let steering = Steering::new();
    let person = steering.clone();
    let tools = Tools::new();
    let mut model = Script::new(vec![
        says("Done.").map(|reply| reply.with_usage(TokenUsage::new(800, 2))),
        says("SUMMARY"),
        says("Next."),
    ])
    .during(move |rank| {
        if rank == 1 {
            person
                .steer("use Le Monde too")
                .expect("the run reads its queue");
        }
    });
    let second = {
        let mut agent = Agent::new(&mut tree, &mut lines, &tools, &now)
            .with_window(Window::new(1_000, 200, 100))
            .with_steering(&steering);
        let first = agent.prompt(&"a".repeat(1_200), &mut model, &mut |_| {});
        assert!(matches!(first, Outcome::Answered { .. }));
        agent.prompt("and now?", &mut model, &mut |_| {})
    };
    assert!(matches!(&second, Outcome::Answered { text } if text == "Next."));
    assert_eq!(model.seen.len(), 3, "no request after the answer");
    let steered = |request: &Request| {
        (request.messages.iter())
            .flat_map(|message| message.content.iter())
            .filter(
                |block| matches!(block, ContentBlock::Text { text } if text == "use Le Monde too"),
            )
            .count()
    };
    assert_eq!(
        steered(&model.seen[2]),
        1,
        "the request after the summary carries it once"
    );
}

/// Stop while one message's observations run side by side: each one already started settles with
/// its own reply, the message's later write never runs and reads as not run.
#[test]
fn stop_in_an_observation_batch_keeps_the_started_replies() {
    let (mut tree, mut lines) = fresh();
    let cancel = CancelCtx::new();
    let stop = cancel.clone();
    let tools = Tools::with_hook(move |_| stop.cancel());
    let observe = |id: &str, url: &str| ContentBlock::ToolUse {
        id: id.into(),
        name: "observe".into(),
        input: json!({"url": url}),
    };
    let write = ContentBlock::ToolUse {
        id: "c4".into(),
        name: "candidate_write".into(),
        input: json!({"source": "nika: x\n"}),
    };
    let blocks = vec![
        observe("c1", "https://a.example"),
        observe("c2", "https://b.example"),
        observe("c3", "https://c.example"),
        write,
    ];
    let mut model = Script::new(vec![Ok(Reply::new(blocks, StopReason::ToolUse))]);
    let mut run = Agent::new(&mut tree, &mut lines, &tools, &now).with_cancel(&cancel);
    let outcome = run.prompt("observe", &mut model, &mut |_| {});
    drop(run);
    assert!(matches!(outcome, Outcome::Stopped { .. }));
    let called: Vec<String> = (tools.called().into_iter()).map(|(name, _)| name).collect();
    assert_eq!(
        called,
        ["observe", "observe", "observe"],
        "the write never ran"
    );
    let context = tree.context();
    let results = last_results(&Request::new(None, context.messages, Vec::new()));
    let ids: Vec<&str> = results.iter().map(|(id, ..)| id.as_str()).collect();
    assert_eq!(ids, ["c1", "c2", "c3", "c4"]);
    assert!(
        results[..3].iter().all(|(_, _, error)| !error),
        "the started observations settled with their replies"
    );
    assert!(results[3].2, "the write reads as not run");
}

/// An answer its provider interrupted is no answer: nothing of it is kept or run, and the turn
/// fails as the transient failure it is, so the line can be sent again.
#[test]
fn an_answer_its_provider_interrupted_is_neither_kept_nor_run() {
    let (mut tree, mut lines) = fresh();
    let tools = Tools::new();
    let interrupted = Reply::new(
        vec![
            ContentBlock::Text {
                text: "Je lis".into(),
            },
            ContentBlock::ToolUse {
                id: "c1".into(),
                name: "read".into(),
                input: json!({"path": "a.nika"}),
            },
        ],
        StopReason::Interrupted,
    );
    let mut model = Script::new(vec![Ok(interrupted)]);
    let outcome =
        Agent::new(&mut tree, &mut lines, &tools, &now).prompt("read it", &mut model, &mut |_| {});
    let failed = matches!(&outcome, Outcome::Failed { error: AgentError::Model(why) }
        if why.contains("ended the answer with an error") && why.contains("retry"));
    assert!(
        failed,
        "the turn says its provider interrupted it, and how to go on"
    );
    assert!(tools.called().is_empty(), "none of its calls ran");
    assert_eq!(kinds(tree.entries()), ["user"], "nothing of it was kept");
}

/// A summary its provider interrupted folds nothing either: the turn says why, and every
/// original entry stays the context.
#[test]
fn a_summary_its_provider_interrupted_folds_nothing() {
    let (mut tree, mut lines) = fresh();
    let tools = Tools::new();
    let interrupted = Reply::new(
        vec![ContentBlock::Text {
            text: "The person asked for".into(),
        }],
        StopReason::Interrupted,
    );
    let mut model = Script::new(vec![
        says("Done.").map(|reply| reply.with_usage(TokenUsage::new(800, 2))),
        Ok(interrupted),
    ]);
    let mut events = Vec::new();
    let second = {
        let mut agent = Agent::new(&mut tree, &mut lines, &tools, &now)
            .with_window(Window::new(1_000, 200, 100));
        let first = agent.prompt(&"a".repeat(1_200), &mut model, &mut |_| {});
        assert!(matches!(first, Outcome::Answered { .. }));
        agent.prompt("and now?", &mut model, &mut |e| events.push(e))
    };
    let refused = matches!(&second, Outcome::Failed { error: AgentError::Model(why) }
        if why.contains("interrupted"));
    assert!(refused, "the turn says why");
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AgentEvent::Compacted { .. })),
        "nothing folded"
    );
    assert_eq!(
        tree.context().messages.len(),
        3,
        "every original entry stays"
    );
}

/// A request over the window folds the conversation once and asks the same step again: one
/// summary, one compaction, then the answer.
#[test]
fn an_overflow_compacts_once_and_asks_again() {
    let (mut tree, mut lines) = fresh();
    let tools = Tools::new();
    let over = Err(ModelError::Overflow {
        window: Some(1_000),
    });
    let mut model = Script::new(vec![says("First."), over, says("SUMMARY"), says("ok")]);
    let mut agent =
        Agent::new(&mut tree, &mut lines, &tools, &now).with_window(Window::new(1_000, 200, 1));
    let first = agent.prompt("one", &mut model, &mut |_| {});
    assert!(matches!(first, Outcome::Answered { .. }));
    let second = agent.prompt("two", &mut model, &mut |_| {});
    assert!(matches!(&second, Outcome::Answered { text } if text == "ok"));
    drop(agent);
    assert_eq!(model.seen.len(), 4);
    assert!(
        model.seen[2].tools.is_empty(),
        "the summary request offers no tool"
    );
    let folds = (tree.entries().iter())
        .filter(|e| matches!(e.kind, EntryKind::Compaction { .. }))
        .count();
    assert_eq!(folds, 1);
}

/// A reply that filled the window is no answer: nothing of it is kept, and the conversation is
/// folded once before the same step is asked again.
#[test]
fn a_reply_that_filled_the_window_compacts_like_an_overflow() {
    let (mut tree, mut lines) = fresh();
    let tools = Tools::new();
    let full = ContentBlock::Text {
        text: "CUT-AT-THE-WINDOW".into(),
    };
    let filled = Ok(Reply::new(vec![full], StopReason::MaxTokens).with_window_full(true));
    let mut model = Script::new(vec![says("First."), filled, says("SUMMARY"), says("ok")]);
    let mut agent =
        Agent::new(&mut tree, &mut lines, &tools, &now).with_window(Window::new(1_000, 200, 1));
    let first = agent.prompt("one", &mut model, &mut |_| {});
    assert!(matches!(first, Outcome::Answered { .. }));
    let second = agent.prompt("two", &mut model, &mut |_| {});
    assert!(matches!(&second, Outcome::Answered { text } if text == "ok"));
    drop(agent);
    let kept = serde_json::to_string(tree.entries()).unwrap();
    assert!(!kept.contains("CUT-AT-THE-WINDOW"));
    assert!(kept.contains("SUMMARY"));
}

/// An overflow after the conversation was folded once for this step fails, naming the window;
/// no second summary is asked.
#[test]
fn a_second_overflow_fails_naming_the_window() {
    let (mut tree, mut lines) = fresh();
    let tools = Tools::new();
    let over = || {
        Err(ModelError::Overflow {
            window: Some(1_000),
        })
    };
    let mut model = Script::new(vec![says("First."), over(), says("SUMMARY"), over()]);
    let mut agent =
        Agent::new(&mut tree, &mut lines, &tools, &now).with_window(Window::new(1_000, 200, 1));
    let first = agent.prompt("one", &mut model, &mut |_| {});
    assert!(matches!(first, Outcome::Answered { .. }));
    let second = agent.prompt("two", &mut model, &mut |_| {});
    assert!(matches!(
        &second,
        Outcome::Failed { error: AgentError::Model(why) }
            if why.contains("1000 tokens") && why.contains("already folded")
    ));
    drop(agent);
    assert_eq!(model.seen.len(), 4, "no second summary");
}

/// A long run from one line folds between its balanced steps: the kept part starts at a later
/// message of the model whose calls have their replies, and the run goes on to its answer.
#[test]
fn a_long_run_from_one_line_compacts_between_its_balanced_steps() {
    let (mut tree, mut lines) = fresh();
    let tools = Tools::new();
    let mut model = Script::new(vec![
        calls_to(&[("c1", "read", "a.nika")]),
        calls_to(&[("c2", "read", "b.nika")]),
        calls_to(&[("c3", "read", "c.nika")]),
        Err(ModelError::Overflow {
            window: Some(1_000),
        }),
        says("SUMMARY"),
        says("Done."),
    ]);
    let outcome = Agent::new(&mut tree, &mut lines, &tools, &now)
        .with_window(Window::new(1_000, 200, 1))
        .prompt("go", &mut model, &mut |_| {});
    assert!(matches!(&outcome, Outcome::Answered { text } if text == "Done."));
    assert_eq!(tools.called().len(), 3);
    let first_kept = (tree.entries().iter()).find_map(|e| match &e.kind {
        EntryKind::Compaction { first_kept, .. } => Some(first_kept.clone()),
        _ => None,
    });
    let kept = (tree.entries().iter()).find(|e| Some(&e.id) == first_kept.as_ref());
    assert!(kept.is_some_and(|e| matches!(
        &e.kind,
        EntryKind::Assistant { content, .. } if calls(content).any(|(id, _, _)| id == "c3")
    )));
}

/// A fold larger than the window is summarized in parts: every summary request stays within
/// the window less its reserve, each carries the summary written so far, and one compaction
/// keeps the last.
#[test]
fn a_fold_larger_than_the_window_is_summarized_in_parts() {
    let (mut tree, mut lines) = fresh();
    let tools = Tools::new();
    let mut model = Script::new(vec![
        says(&"x".repeat(1_800)),
        says(&"y".repeat(1_800)),
        says("PART1"),
        says("PART2"),
        says("ok"),
    ]);
    let mut agent =
        Agent::new(&mut tree, &mut lines, &tools, &now).with_window(Window::new(2_000, 400, 1));
    let first = agent.prompt(&"a".repeat(1_800), &mut model, &mut |_| {});
    assert!(matches!(first, Outcome::Answered { .. }));
    let second = agent.prompt(&"b".repeat(1_800), &mut model, &mut |_| {});
    assert!(matches!(second, Outcome::Answered { .. }));
    let third = agent.prompt("c", &mut model, &mut |_| {});
    assert!(matches!(&third, Outcome::Answered { text } if text == "ok"));
    drop(agent);
    let summaries: Vec<&Request> = (model.seen.iter()).filter(|r| r.tools.is_empty()).collect();
    assert_eq!(summaries.len(), 2);
    for request in &summaries {
        let text: usize = (request.messages.iter())
            .flat_map(|m| &m.content)
            .map(|block| match block {
                ContentBlock::Text { text } => text.len(),
                _ => 0,
            })
            .sum();
        let chars = text + request.system.as_deref().map_or(0, str::len);
        assert!(chars.div_ceil(4) <= 1_600, "{chars} characters");
    }
    let carried = matches!(
        &summaries[1].messages[0].content[0],
        ContentBlock::Text { text } if text.contains("PART1")
    );
    assert!(carried, "the second part carries the first part's summary");
    let kept = (tree.entries().iter()).find_map(|e| match &e.kind {
        EntryKind::Compaction { summary, .. } => Some(summary.clone()),
        _ => None,
    });
    assert_eq!(kept.as_deref(), Some("PART2"));
}

/// An indivisible transcript over budget is refused before any summary call; originals remain.
#[test]
fn an_indivisible_summary_object_over_budget_folds_nothing() {
    let (mut tree, mut lines) = fresh();
    let tools = Tools::new();
    let mut model = Script::new(vec![says("Done."), says("NEVER")]);
    let mut events = Vec::new();
    let outcome = {
        let mut agent = Agent::new(&mut tree, &mut lines, &tools, &now)
            .with_window(Window::new(1_000, 200, 100));
        assert!(matches!(
            agent.prompt(&"a".repeat(3_200), &mut model, &mut |_| {}),
            Outcome::Answered { .. }
        ));
        agent.prompt("and now?", &mut model, &mut |event| events.push(event))
    };
    let expected = "the summary request needs an estimated 1107 tokens, \
                    over the 800-token budget of the 1000-token window: nothing was folded";
    let refused = matches!(outcome, Outcome::Failed { error: AgentError::Model(why) }
        if why == expected);
    assert!(refused);
    assert_eq!(model.seen.len(), 1, "no oversized summary sent");
    assert_eq!(kinds(tree.entries()), ["user", "assistant", "user"]);
    assert_eq!(tree.context().messages.len(), 3);
    assert!(
        matches!(&tree.entries()[0].kind, EntryKind::User { text, .. }
        if text == &"a".repeat(3_200))
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AgentEvent::Compacted { .. }))
    );
}

/// Fixed focus/instructions over budget leave the history and its durable store byte-identical.
#[test]
fn a_fixed_summary_context_over_budget_calls_no_model_and_folds_nothing() {
    let (mut tree, mut lines) = fresh();
    let tools = Tools::new();
    let mut model = Script::new(vec![says("First."), says("Second."), says("NEVER")]);
    {
        let mut agent = Agent::new(&mut tree, &mut lines, &tools, &now);
        assert!(matches!(
            agent.prompt("one", &mut model, &mut |_| {}),
            Outcome::Answered { .. }
        ));
        assert!(matches!(
            agent.prompt("two", &mut model, &mut |_| {}),
            Outcome::Answered { .. }
        ));
    }
    let before = serde_json::to_string(tree.entries()).expect("original entries");
    let stored = lines.clone();
    let mut events = Vec::new();
    let outcome = Agent::new(&mut tree, &mut lines, &tools, &now)
        .with_window(Window::new(1_000, 200, 100))
        .compact(Some(&"f".repeat(3_200)), &mut model, &mut |event| {
            events.push(event);
        });
    let expected = "the summary request needs an estimated 1105 tokens, \
                    over the 800-token budget of the 1000-token window: nothing was folded";
    assert!(matches!(outcome, Err(AgentError::Model(why)) if why == expected));
    assert_eq!(model.seen.len(), 2, "no fixed context over budget sent");
    assert_eq!(
        serde_json::to_string(tree.entries()).expect("original entries"),
        before
    );
    assert_eq!(lines, stored);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AgentEvent::Compacted { .. }))
    );
}

/// A first part's summary may exhaust the next budget; no partial compaction or retry is kept.
#[test]
fn a_carried_summary_over_budget_keeps_all_originals_without_another_call() {
    let (mut tree, mut lines) = fresh();
    let tools = Tools::new();
    let mut model = Script::new(vec![
        says(&"x".repeat(1_800)),
        says(&"y".repeat(1_800)),
        says(&"s".repeat(6_400)),
        says("NEVER"),
    ]);
    let mut events = Vec::new();
    let outcome = {
        let mut agent =
            Agent::new(&mut tree, &mut lines, &tools, &now).with_window(Window::new(2_000, 400, 1));
        assert!(matches!(
            agent.prompt(&"a".repeat(1_800), &mut model, &mut |_| {}),
            Outcome::Answered { .. }
        ));
        assert!(matches!(
            agent.prompt(&"b".repeat(1_800), &mut model, &mut |_| {}),
            Outcome::Answered { .. }
        ));
        agent.prompt("c", &mut model, &mut |event| events.push(event))
    };
    let expected = "the summary request needs an estimated 1905 tokens, \
                    over the 1600-token budget of the 2000-token window: nothing was folded";
    let refused = matches!(outcome, Outcome::Failed { error: AgentError::Model(why) }
        if why == expected);
    assert!(refused);
    assert_eq!(model.seen.len(), 3, "only the first part was asked");
    assert_eq!(
        kinds(tree.entries()),
        ["user", "assistant", "user", "assistant", "user"]
    );
    assert_eq!(tree.context().messages.len(), 5);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AgentEvent::Compacted { .. }))
    );
    let recorded = serde_json::to_string(tree.entries()).expect("original entries");
    assert!(!recorded.contains(&"s".repeat(6_400)));
}
