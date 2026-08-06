// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! **The end-to-end proof the arc owed** (P3 B4.5) — it lives in the
//! quarantine because that is where the instruments live (and because
//! the diamond's 15k crate counter counts `tests/` too). A real task,
//! composed by the production composer, whose `agent:` task runs on a
//! real spawned harness child and comes back with the harness's own
//! answer.
//!
//! Until this test the seam was dead: `with_harness_seat` existed and
//! nothing called it, so nothing proved an `agent:` task could reach a
//! harness at all. Here the whole chain fires: env declaration →
//! `seat_from_env` → `production_runtime` → `AgentVerb` → the kernel
//! seam → the hand-rolled wire client → a spawned python3 agent.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use nika_harness::seat_from_lookup;

const FAKE_AGENT: &str = r#"
import json, sys
def send(o):
    sys.stdout.write(json.dumps(o) + "\n"); sys.stdout.flush()
def recv():
    return json.loads(sys.stdin.readline())

if "--version" in sys.argv:
    print("fake-harness 1.2.0")
    sys.exit(0)

init = recv()
send({"jsonrpc":"2.0","id":init["id"],"result":{"protocolVersion":1}})
new = recv()
send({"jsonrpc":"2.0","id":new["id"],"result":{"sessionId":"s-e2e"}})
prompt = recv()
# Echo the user's own text back so the test proves the PROMPT crossed
# the whole chain, not just that some string came home.
asked = prompt["params"]["prompt"][0]["text"]
send({"jsonrpc":"2.0","method":"session/update","params":{
    "sessionId":"s-e2e",
    "update":{"sessionUpdate":"agent_message_chunk",
              "content":{"type":"text","text":"harness heard: " + asked}}}})
send({"jsonrpc":"2.0","id":prompt["id"],"result":{"stopReason":"end_turn"}})
"#;

struct Guard(std::path::PathBuf);
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The declaration a machine would carry in its environment — driven
/// through the injected lookup (no process env is written: Rust 2024
/// needs `unsafe` for that and the workspace forbids it).
fn declare_fake_harness() -> (BTreeMap<String, String>, Guard) {
    let dir = std::env::temp_dir().join(format!("nika-seat-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmpdir");
    let script = dir.join("fake_harness.py");
    std::fs::write(&script, FAKE_AGENT).expect("script");
    let path = script.to_string_lossy().into_owned();
    let declared = [
        ("NIKA_HARNESS_ADAPTER", "fake-harness".to_owned()),
        ("NIKA_HARNESS_COMMAND", "python3".to_owned()),
        ("NIKA_HARNESS_ARGS", path.clone()),
        // The wrapper shape the gauntlet taught: probe the SCRIPT, not
        // the interpreter.
        ("NIKA_HARNESS_VERSION_ARGS", format!("{path} --version")),
        ("NIKA_HARNESS_MIN", "1.0".to_owned()),
        ("NIKA_HARNESS_MAX_MAJOR", "1".to_owned()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect();
    (declared, Guard(dir))
}

fn lookup_of(map: &BTreeMap<String, String>) -> impl Fn(&str) -> Option<String> + '_ {
    move |name: &str| map.get(name).cloned()
}

#[tokio::test]
async fn an_agent_task_runs_on_a_real_spawned_harness_end_to_end() {
    let (declared, _guard) = declare_fake_harness();

    // 1 · the composer's half: the declaration becomes a seat.
    let backend = seat_from_lookup(&lookup_of(&declared))
        .expect("a well-formed declaration composes")
        .expect("a declared adapter yields a backend");
    let seat = nika_verb_agent::harness_path::HarnessSeat::new(
        std::sync::Arc::new(backend),
        std::env::current_dir().expect("cwd"),
    );

    // 2 · the verb's half: seat it exactly as production_runtime does.
    let verb = nika_verb_agent::AgentVerb::new(
        std::sync::Arc::new(nika_kernel_mock::MockProvider::new("mock")),
        std::sync::Arc::new(nika_verb_invoke::InvokeVerb::new(std::sync::Arc::new(
            nika_kernel_mock::MockToolExecutor::new(),
        ))),
        std::sync::Arc::new(nika_kernel_mock::MockToolDefinitionProvider::new()),
        "mock/echo",
    )
    .with_harness_seat(seat);

    // 3 · a real task through the real chain.
    let out = verb
        .run(nika_verb_agent::AgentInput::new("summarize the arc"))
        .await
        .expect("the harness answers");

    let nika_verb_agent::AgentValue::Text(text) = &out.output else {
        panic!("a harness turn returns text in P3");
    };
    assert_eq!(
        text, "harness heard: summarize the arc",
        "the prompt must cross the WHOLE chain and the answer come back"
    );
    // Honesty: no usage reported by this harness, so none is invented.
    assert_eq!(out.total_tokens, 0);
    assert!(out.model_resolved.is_none());
}

#[tokio::test]
async fn a_declared_adapter_with_no_command_refuses_at_composition() {
    let declared: BTreeMap<String, String> = [(
        "NIKA_HARNESS_ADAPTER".to_owned(),
        "half-declared".to_owned(),
    )]
    .into_iter()
    .collect();
    let err = seat_from_lookup(&lookup_of(&declared)).expect_err("a half declaration must refuse");
    assert!(err.contains("NIKA_HARNESS_COMMAND"), "{err}");
    // The law: it refuses, it does not fall back to the native loop —
    // the operator asked for a harness (A-4).
    assert!(!err.contains("native"), "{err}");
}

#[tokio::test]
async fn no_declaration_means_no_seat_and_the_native_loop_keeps_the_task() {
    let empty = BTreeMap::new();
    assert!(
        seat_from_lookup(&lookup_of(&empty))
            .expect("no declaration is not an error")
            .is_none(),
        "an undeclared machine seats nothing"
    );
}
