// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The spawn e2e — a REAL child process speaking wire v1 over stdio.
//!
//! The fake agent is a python3 script (the same interpreter the estate
//! belt already requires in CI): it plays the scripted dialogue AND
//! echoes into its message chunk whether any credential-shaped
//! variable reached its environment — the negative env proof runs
//! LIVE through the actual spawn seam, not only through the pure
//! `compose_env` unit.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::pin::Pin;

use futures_core::Stream as _;

use nika_harness::{HarnessAdapter, SpawnedHarness};
use nika_kernel::ai::harness::{AgentBackend as _, HarnessEvent, HarnessRequest};

const FAKE_AGENT: &str = r#"
import json, os, sys

def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()

def recv():
    line = sys.stdin.readline()
    return json.loads(line)

init = recv()
assert init["method"] == "initialize", init
assert init["params"]["protocolVersion"] == 1, init
send({"jsonrpc": "2.0", "id": init["id"], "result": {"protocolVersion": 1}})

new = recv()
assert new["method"] == "session/new", new
send({"jsonrpc": "2.0", "id": new["id"], "result": {"sessionId": "s-e2e"}})

prompt = recv()
assert prompt["method"] == "session/prompt", prompt

leaked = sorted(
    k for k in os.environ
    if k.startswith("NIKA_") or k.endswith("_API_KEY")
    or k.endswith("_TOKEN") or k.endswith("_SECRET")
)
report = "leaked:" + (",".join(leaked) if leaked else "none")
send({"jsonrpc": "2.0", "method": "session/update", "params": {
    "sessionId": "s-e2e",
    "update": {"sessionUpdate": "agent_message_chunk",
               "content": {"type": "text", "text": report}}}})
send({"jsonrpc": "2.0", "id": prompt["id"], "result": {"stopReason": "end_turn"}})
"#;

fn fake_agent_adapter() -> (HarnessAdapter, tempfile_guard::Guard) {
    let dir = std::env::temp_dir().join(format!("nika-harness-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmpdir");
    let script = dir.join("fake_agent.py");
    std::fs::write(&script, FAKE_AGENT).expect("script written");
    let adapter = HarnessAdapter::new("fake-agent", "python3")
        .expect("id ok")
        .with_args(vec![script.to_string_lossy().into_owned()]);
    (adapter, tempfile_guard::Guard(dir))
}

mod tempfile_guard {
    pub(crate) struct Guard(pub(crate) std::path::PathBuf);
    impl Drop for Guard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[tokio::test]
async fn a_real_child_completes_the_dialogue_and_no_key_var_crosses() {
    let (adapter, _guard) = fake_agent_adapter();
    let harness = SpawnedHarness::new(adapter);
    let mut stream = harness
        .run_agent(HarnessRequest::new("hello child", "/tmp"))
        .await
        .expect("the spawn succeeds — python3 rides CI already (estate belt)");

    let mut chunks = Vec::new();
    let mut completed = false;
    loop {
        let next = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await;
        match next {
            Some(Ok(HarnessEvent::MessageChunk { text })) => chunks.push(text),
            Some(Ok(HarnessEvent::Completed { outcome })) => {
                completed = true;
                // The child REPORTED its own environment: no
                // credential-shaped variable reached it (A-3 · the
                // live half of the compose_env negative proof).
                assert_eq!(
                    outcome.output, "leaked:none",
                    "a credential-shaped var crossed the spawn boundary"
                );
            }
            Some(Ok(_)) => {}
            Some(Err(e)) => panic!("stream error: {e}"),
            None => break,
        }
    }
    assert!(completed, "the turn must complete");
    assert_eq!(chunks, vec!["leaked:none"]);
}

#[tokio::test]
async fn dropping_the_stream_reaps_the_child() {
    // A fake agent that answers the handshake then sleeps forever —
    // dropping the stream must end it (kill-on-drop), not orphan it.
    let dir = std::env::temp_dir().join(format!("nika-harness-reap-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmpdir");
    let script = dir.join("sleepy_agent.py");
    std::fs::write(
        &script,
        r#"
import json, sys, time
line = sys.stdin.readline()
init = json.loads(line)
sys.stdout.write(json.dumps({"jsonrpc":"2.0","id":init["id"],
    "result":{"protocolVersion":1}}) + "\n")
sys.stdout.flush()
time.sleep(3600)
"#,
    )
    .expect("script written");
    let adapter = HarnessAdapter::new("sleepy-agent", "python3")
        .expect("id ok")
        .with_args(vec![script.to_string_lossy().into_owned()]);
    let harness = SpawnedHarness::new(adapter);
    let stream = harness
        .run_agent(HarnessRequest::new("hi", "/tmp"))
        .await
        .expect("spawn succeeds");
    // Drop immediately: the driver task and the child handle go down
    // with the stream; kill_on_drop reaps the sleeper. The absence of
    // a hang IS the assertion — a leaked child would keep the test
    // process's stdio pipes open and the runtime alive.
    drop(stream);
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let _ = std::fs::remove_dir_all(&dir);
}
