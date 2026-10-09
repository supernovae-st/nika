// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::collections::VecDeque;
use std::sync::Mutex;

use nika_kernel::provider::Role;
use nika_session_change::tools::ToolDef as SessionToolDef;
use serde_json::json;

use super::*;
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
}

impl Tools {
    fn new() -> Self {
        Self::with_hook(|_| {})
    }

    fn with_hook(hook: impl Fn(&ToolCall) + Send + Sync + 'static) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            hook: Box::new(hook),
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

#[test]
fn a_steering_line_enters_after_the_current_call_and_the_rest_wait() {
    let (mut tree, mut lines) = fresh();
    let steering = Steering::new();
    let person = steering.clone();
    let tools = Tools::with_hook(move |call| {
        if call.meta.as_deref() == Some("c1") {
            person.steer("use Le Monde too");
        }
    });
    let mut model = Script::new(vec![
        calls_to(&[("c1", "read", "a.nika"), ("c2", "read", "b.nika")]),
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
                person.follow_up("and summarize it in French");
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
        person.steer("not this one");
        stop.cancel();
    });
    let mut events = Vec::new();
    let outcome = Agent::new(&mut tree, &mut lines, &tools, &now)
        .with_steering(&steering)
        .with_cancel(&cancel)
        .prompt("a digest", &mut model, &mut |e| events.push(e));
    let Outcome::Stopped { queued } = outcome else {
        panic!("stopped");
    };
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
    let outcome = Agent::new(&mut tree, &mut lines, &tools, &now)
        .with_cancel(&cancel)
        .prompt("read", &mut model, &mut |_| {});
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
    let mut model = Script::new(vec![says("Done."), says("SUMMARY"), says("Next.")]);
    let mut agent =
        Agent::new(&mut tree, &mut lines, &tools, &now).with_window(Window::new(1_000, 200, 100));
    // The first line alone outweighs the window, and nothing comes before it to fold.
    let first = agent.prompt(&"a".repeat(3_200), &mut model, &mut |_| {});
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
    assert!(text.ends_with("SUMMARY"));
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
fn a_request_carries_the_branchs_instructions_and_the_persons_citations() {
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
