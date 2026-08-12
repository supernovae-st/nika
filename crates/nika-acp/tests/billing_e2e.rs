// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! **The B7 billing honesty e2e** (P3 · the receipt's harness row): a
//! full run whose `agent:` task rides a real spawned harness, then the
//! `task_completed` frame is read as the receipt it is — `access:
//! harness` · `billing: unknown` · `cost_unpriced: subscription_quota`
//! and NO `cost_usd` (a fabricated $0 is the one unforgivable line).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::sync::Arc;

use nika_harness::seat_from_lookup;

/// The quiet harness: handshake, one chunk, done (no ask — the billing
/// frame must fire on the happy path).
const QUIET_AGENT: &str = r#"
import json, sys
def send(o):
    sys.stdout.write(json.dumps(o) + "\n"); sys.stdout.flush()
def recv():
    return json.loads(sys.stdin.readline())

if "--version" in sys.argv:
    print("quiet-harness 1.0.0")
    sys.exit(0)

init = recv()
send({"jsonrpc":"2.0","id":init["id"],"result":{"protocolVersion":1}})
new = recv()
send({"jsonrpc":"2.0","id":new["id"],"result":{"sessionId":"s-bill"}})
prompt = recv()
send({"jsonrpc":"2.0","method":"session/update","params":{
    "sessionId":"s-bill",
    "update":{"sessionUpdate":"agent_message_chunk",
              "content":{"type":"text","text":"quiet answer"}}}})
send({"jsonrpc":"2.0","id":prompt["id"],"result":{"stopReason":"end_turn"}})
"#;

struct Guard(std::path::PathBuf);
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn field(event: &nika_event::Event, key: &str) -> Option<String> {
    event
        .fields
        .iter()
        .find(|kv| kv.key == key)
        .map(|kv| match &kv.value {
            nika_types::resource::Value::String(s) => s.clone(),
            other => format!("{other:?}"),
        })
}

#[tokio::test]
async fn a_harness_run_receipt_names_the_subscription_lane_never_a_fake_zero() {
    let dir = std::env::temp_dir().join(format!("nika-billing-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmpdir");
    let script = dir.join("quiet_harness.py");
    std::fs::write(&script, QUIET_AGENT).expect("script");
    let path = script.to_string_lossy().into_owned();
    let declared: BTreeMap<String, String> = [
        ("NIKA_HARNESS_ADAPTER", "quiet-harness".to_owned()),
        ("NIKA_HARNESS_COMMAND", "python3".to_owned()),
        ("NIKA_HARNESS_ARGS", path.clone()),
        ("NIKA_HARNESS_VERSION_ARGS", format!("{path} --version")),
        ("NIKA_HARNESS_MIN", "1.0".to_owned()),
        ("NIKA_HARNESS_MAX_MAJOR", "1".to_owned()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect();
    let _guard = Guard(dir);

    let backend = seat_from_lookup(&|name| declared.get(name).cloned())
        .expect("the declaration composes")
        .expect("a declared adapter yields a backend");
    let seat = nika_verb_agent::harness_path::HarnessSeat::new(
        Arc::new(backend),
        std::env::current_dir().expect("cwd"),
    );
    let invoke = Arc::new(nika_verb_invoke::InvokeVerb::new(Arc::new(
        nika_kernel_mock::MockToolExecutor::new(),
    )));
    let registry = Arc::new(nika_providers::ProviderRegistry::without_http(
        nika_providers::ProvidersConfig::default(),
    ));
    let agent = nika_verb_agent::AgentVerb::new(
        Arc::new(nika_kernel_mock::MockProvider::new("mock")),
        Arc::clone(&invoke),
        Arc::new(nika_kernel_mock::MockToolDefinitionProvider::new()),
        "mock/echo",
    )
    .with_harness_seat(seat);
    let runtime = nika_runtime::Runtime::new(
        nika_verb_exec::ExecVerb::new(Arc::new(nika_kernel_mock::MockShell::new())),
        invoke,
        nika_verb_infer::InferVerb::new(registry, "mock/echo"),
        agent,
        nika_kernel_mock::MockClock::new(),
        nika_runtime::RuntimeConfig::default(),
    );

    let wf = nika_schema::parse(
        "nika: billing\ntasks:\n  ask:\n    agent: { prompt: \"hi\" }\n",
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean());
    let mut stamper = nika_runtime::DeterministicStamper::new();
    let mut sink = nika_runtime::VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("the harness run completes");
    assert!(outcome.ok);

    let completed = sink
        .events()
        .iter()
        .find(|e| e.kind == nika_event::EventKind::TaskCompleted)
        .expect("a task_completed frame");
    assert_eq!(
        field(completed, "access").as_deref(),
        Some("harness"),
        "the receipt names the access class"
    );
    assert_eq!(
        field(completed, "billing").as_deref(),
        Some("unknown"),
        "the lane is unknown until an adapter attests it (A-7)"
    );
    assert_eq!(
        field(completed, "cost_unpriced").as_deref(),
        Some("subscription_quota"),
        "the subscription absorbs the spend — named, never metered"
    );
    assert!(
        field(completed, "cost_usd").is_none(),
        "never a fabricated $0 (the ledger law)"
    );
}
