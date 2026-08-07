// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! **The B5 authority bridge, proven over the REAL wire** (P3 · the
//! quarantine is where the instruments live): a spawned python3 harness
//! asks `session/request_permission`, and the engine's answer crosses
//! the whole chain — declaration, seat, verb bridge, kernel seam,
//! hand-rolled client — back to the child's own ears.
//!
//! Three laws, three dialogues:
//!
//! 1. inside the workflow's `permits:` grants, the bridge answers
//!    `allow_once` ITSELF (never `allow_always` · A-5) and the child
//!    reads its own `allow_once` option id selected;
//! 2. outside every grant with no operator answer, the run PAUSES
//!    (NIKA-1806 · the verb's `HarnessGate` error carries the question
//!    VERBATIM) and the child reads `cancelled` — fail-closed, zero
//!    action before a human speaks;
//! 3. the operator's bound `--answer` verdict decides the re-asked
//!    question (the resumed run's shape), witnessed as operator-granted.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use nika_harness::seat_from_lookup;
use nika_verb_agent::{AgentEvent, AgentInput, AgentObserver, AgentVerb};

/// The asking harness: handshake, then ONE permission ask whose answer
/// it REPORTS BACK in its final message (the proof the engine's verdict
/// crossed the wire), then the turn ends. argv[1] selects the asked
/// program; the options put `allow_always` FIRST so the allow_once
/// selection is proven against the trap order.
const ASKING_AGENT: &str = r#"
import json, sys
def send(o):
    sys.stdout.write(json.dumps(o) + "\n"); sys.stdout.flush()
def recv():
    return json.loads(sys.stdin.readline())

if "--version" in sys.argv:
    print("asking-harness 1.0.0")
    sys.exit(0)

program = sys.argv[1]
init = recv()
send({"jsonrpc":"2.0","id":init["id"],"result":{"protocolVersion":1}})
new = recv()
send({"jsonrpc":"2.0","id":new["id"],"result":{"sessionId":"s-gate"}})
prompt = recv()
send({"jsonrpc":"2.0","id":"ask-1","method":"session/request_permission","params":{
    "sessionId":"s-gate",
    "toolCall":{"title":"run `" + program + "`","kind":"execute",
                "rawInput":{"command":[program]}},
    "options":[
        {"optionId":"opt-always","name":"Always","kind":"allow_always"},
        {"optionId":"opt-once","name":"Once","kind":"allow_once"},
        {"optionId":"opt-reject","name":"No","kind":"reject_once"}]}})
answer = recv()
outcome = answer["result"]["outcome"]
verdict = outcome["outcome"]
if verdict == "selected":
    verdict = verdict + ":" + outcome["optionId"]
send({"jsonrpc":"2.0","method":"session/update","params":{
    "sessionId":"s-gate",
    "update":{"sessionUpdate":"agent_message_chunk",
              "content":{"type":"text","text":"the engine said: " + verdict}}}})
send({"jsonrpc":"2.0","id":prompt["id"],"result":{"stopReason":"end_turn"}})
"#;

struct Guard(std::path::PathBuf);
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Seat the asking harness exactly as the production composer would.
#[allow(clippy::type_complexity)] // the seated verb's full type, named once
fn seated_verb(
    program: &str,
) -> (
    AgentVerb<
        nika_kernel_mock::MockProvider,
        nika_kernel_mock::MockToolExecutor,
        nika_kernel_mock::MockToolDefinitionProvider,
    >,
    Guard,
) {
    let dir =
        std::env::temp_dir().join(format!("nika-gate-e2e-{}-{}", std::process::id(), program));
    std::fs::create_dir_all(&dir).expect("tmpdir");
    let script = dir.join("asking_harness.py");
    std::fs::write(&script, ASKING_AGENT).expect("script");
    let path = script.to_string_lossy().into_owned();
    let declared: BTreeMap<String, String> = [
        ("NIKA_HARNESS_ADAPTER", "asking-harness".to_owned()),
        ("NIKA_HARNESS_COMMAND", "python3".to_owned()),
        ("NIKA_HARNESS_ARGS", format!("{path} {program}")),
        ("NIKA_HARNESS_VERSION_ARGS", format!("{path} --version")),
        ("NIKA_HARNESS_MIN", "1.0".to_owned()),
        ("NIKA_HARNESS_MAX_MAJOR", "1".to_owned()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect();
    let backend = seat_from_lookup(&|name| declared.get(name).cloned())
        .expect("the declaration composes")
        .expect("a declared adapter yields a backend");
    let seat = nika_verb_agent::harness_path::HarnessSeat::new(
        Arc::new(backend),
        std::env::current_dir().expect("cwd"),
    );
    let verb = AgentVerb::new(
        Arc::new(nika_kernel_mock::MockProvider::new("mock")),
        Arc::new(nika_verb_invoke::InvokeVerb::new(Arc::new(
            nika_kernel_mock::MockToolExecutor::new(),
        ))),
        Arc::new(nika_kernel_mock::MockToolDefinitionProvider::new()),
        "mock/echo",
    )
    .with_harness_seat(seat);
    (verb, Guard(dir))
}

#[derive(Default)]
struct VecObserver(Mutex<Vec<AgentEvent>>);
impl AgentObserver for VecObserver {
    fn on_event(&self, event: &AgentEvent) {
        self.0.lock().expect("lock").push(event.clone());
    }
}

fn permits_with_exec_git() -> nika_schema::types::Permits {
    let mut p = nika_schema::types::Permits::new();
    p.exec = Some(nika_schema::types::ExecPermit::Programs(vec![
        "git".to_owned(),
    ]));
    p
}

/// 1 · inside the grants: the bridge answers allow_once ITSELF — the
/// child reads its own `opt-once` selected (never the `allow_always`
/// sitting first), and the allow is witnessed.
#[tokio::test]
async fn an_in_grants_ask_is_auto_answered_allow_once_and_witnessed() {
    let (verb, _guard) = seated_verb("git");
    let mut input = AgentInput::new("check the tree");
    input.permits = Some(permits_with_exec_git());
    let observer = VecObserver::default();
    let out = verb
        .run_observed(input, &observer)
        .await
        .expect("an in-grants ask completes the run");
    let nika_verb_agent::AgentValue::Text(text) = &out.output else {
        panic!("text output");
    };
    assert_eq!(
        text, "the engine said: selected:opt-once",
        "allow_once crossed the wire — never the allow_always first in the list"
    );
    let events = observer.0.lock().expect("lock");
    assert!(
        events.iter().any(|e| matches!(
            e,
            AgentEvent::PermissionJudged {
                plane: "exec",
                decision: "allow",
                ..
            }
        )),
        "the auto-answer is witnessed: {events:?}"
    );
}

/// 2 · outside every grant, no answer bound: the run pauses — the verb
/// returns the HarnessGate error carrying the question VERBATIM, the
/// child read `cancelled` (fail-closed), and zero allow was witnessed.
#[tokio::test]
async fn an_out_of_grants_ask_pauses_with_the_question_verbatim() {
    let (verb, _guard) = seated_verb("rm");
    let observer = VecObserver::default();
    let err = verb
        .run_observed(AgentInput::new("clean everything"), &observer)
        .await
        .expect_err("an out-of-grants ask pauses the run");
    assert_eq!(
        err.to_string(),
        "harness gate: run `rm`",
        "the gate question surfaces VERBATIM"
    );
    let events = observer.0.lock().expect("lock");
    assert!(
        !events.iter().any(|e| matches!(
            e,
            AgentEvent::PermissionJudged {
                decision: "allow",
                ..
            }
        )),
        "zero allow before the refusal: {events:?}"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            AgentEvent::PermissionJudged {
                decision: "escalate",
                ..
            }
        )),
        "the escalation is witnessed: {events:?}"
    );
}

/// 3 · the operator's bound verdict decides: `--answer <task>=true`
/// grants the re-asked action ONCE (witnessed as operator-granted, and
/// the child reads its allow_once selected).
#[tokio::test]
async fn the_operators_bound_answer_grants_the_reask_once() {
    let (verb, _guard) = seated_verb("rm");
    let mut input = AgentInput::new("clean everything");
    input.gate_answer = Some(serde_json::Value::Bool(true));
    let observer = VecObserver::default();
    let out = verb
        .run_observed(input, &observer)
        .await
        .expect("an operator-answered gate completes");
    let nika_verb_agent::AgentValue::Text(text) = &out.output else {
        panic!("text output");
    };
    assert_eq!(text, "the engine said: selected:opt-once");
    let events = observer.0.lock().expect("lock");
    assert!(
        events.iter().any(|e| matches!(
            e,
            AgentEvent::PermissionJudged { decision: "allow", why, .. }
            if why.contains("operator granted")
        )),
        "the grant is witnessed AS the operator's: {events:?}"
    );
}
