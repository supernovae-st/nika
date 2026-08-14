// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The crate-root test module — moved out of `lib.rs` 2026-07-10 at
//! 1461/1500 LOC (the #291 recover-await design lands in that file's
//! settle spine; the headroom must exist BEFORE the feature). `super`
//! still resolves to the crate root — semantics unchanged.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use super::*;

/// The F-O1 integrity label for the pre-existing frame tests below —
/// they exercise the frame surface, not the label: a trusted settle
/// emits NO `integrity` field (the additive law: absent = trusted).
const TRUSTED: nika_cap::Integrity = nika_cap::Integrity::trusted();

/// #412 test seam — the gate: a `nika:jq` call whose `expression` arg is
/// `".gate"` polls the sink's flag (1ms yields — tokio's `sync` feature
/// is off in this workspace, and a flag poll needs only `time`); every
/// other call answers instantly. Bounded at 5s so a regression fails
/// loudly, never hangs the suite.
struct GateExecutor {
    unblock: Arc<std::sync::atomic::AtomicBool>,
}

impl nika_kernel::tool_executor::ToolExecuteDyn for GateExecutor {
    async fn execute(
        &self,
        call: nika_kernel::tool_executor::ToolCall,
    ) -> Result<nika_kernel::tool_executor::ToolResult, nika_kernel::tool_executor::ToolExecError>
    {
        use std::sync::atomic::Ordering;
        if call.input.get("expression").and_then(Value::as_str) == Some(".gate") {
            let mut waited_ms = 0u32;
            while !self.unblock.load(Ordering::Acquire) {
                if waited_ms > 5_000 {
                    return Err(nika_kernel::tool_executor::ToolExecError::NotAvailable {
                        reason: "settles did not stream — the gate starved (#412 \
                                 regression: frames held to the wave join)"
                            .into(),
                    });
                }
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                waited_ms += 1;
            }
        }
        Ok(nika_kernel::tool_executor::ToolResult::success(
            call.id.as_str(),
            "\"done\"",
        ))
    }
}

/// #412 test seam — the observer: forwards every event to a [`VecSink`]
/// and flips the gate's flag when `fast`'s terminal frame arrives.
struct NotifyOnFastSettle {
    inner: VecSink,
    unblock: Arc<std::sync::atomic::AtomicBool>,
}

impl EventSink for NotifyOnFastSettle {
    fn emit(&mut self, event: Event) {
        let is_fast_completed = event.kind == EventKind::TaskCompleted
            && event.fields.iter().any(|kv| {
                kv.key == "task" && matches!(&kv.value, FieldValue::String(s) if s == "fast")
            });
        if is_fast_completed {
            self.unblock
                .store(true, std::sync::atomic::Ordering::Release);
        }
        self.inner.emit(event);
    }
}

/// #412 · settles STREAM through the ordered spine: a settled sibling's
/// frames reach the sink at ITS settle, not the wave join. Proof by
/// construction: `gate` (same wave, declared after `fast`) BLOCKS until
/// the sink has seen fast's `task_completed` — join-granularity frames
/// ADR-095 Layer 6 — a declared boundary attaches the OS-confinement
/// spec to every exec child (grants absolutized at the config root ·
/// network denied unless `net.http` names an allowlist).
#[tokio::test]
async fn declared_boundary_attaches_the_sandbox_spec_to_exec() {
    use nika_kernel::process::NetPolicy;
    use nika_kernel_mock::{
        MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
    };
    use nika_providers::{ProviderRegistry, ProvidersConfig};

    let yaml = "nika: jail\npermits:\n  fs: { read: [\"./data/**\"], write: [\"./out/**\"] }\n  exec: [\"echo\"]\ntasks:\n  t:\n    exec: { command: [\"echo\", \"x\"] }\n";
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture passes the ladder: {report:?}");
    let shell = MockShell::new().enqueue_ok("ok");
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(shell.clone())),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::default().with_sandbox_root(std::path::PathBuf::from("/repo")),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean run");
    let sent = shell.executed_commands();
    assert_eq!(sent.len(), 1);
    let spec = sent[0]
        .sandbox
        .as_ref()
        .expect("a declared boundary attaches the spec");
    assert_eq!(spec.fs_read, vec!["/repo/data/**".to_owned()]);
    assert_eq!(spec.fs_write, vec!["/repo/out/**".to_owned()]);
    assert_eq!(spec.net, NetPolicy::Deny, "no net.http = the deny holds");
    assert!(
        sent[0].env_passthrough.is_empty(),
        "no env: category = zero declared passthrough (NEP-0005 law 1)"
    );
}

/// NEP-0006 law 3 — the data-as-code sink's RUN twin: a fetch URL the
/// static classifier deferred (a `tasks.*` derivation) resolves at run to
/// a code-bearing artifact and is refused BEFORE the tool executor ever
/// sees the call; the task's declared `inert:` door lets it through.
#[tokio::test]
async fn code_bearing_fetch_refuses_at_run_and_the_inert_door_opens() {
    use nika_kernel::tool_executor::ToolResult;
    use nika_kernel_mock::{
        MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
    };
    use nika_providers::{ProviderRegistry, ProvidersConfig};

    let base = "nika: sinkrun\npermits:\n  net: { http: [\"data.example.com\"] }\n  tools: [\"nika:jq\", \"nika:fetch\"]\ntasks:\n  name:\n    invoke:\n      tool: \"nika:jq\"\n      args: { input: \"https://data.example.com/models/legacy.pkl\", expression: \".\" }\n  grab:\n    with: { u: \"${{ tasks.name.output }}\" }\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"${{ with.u }}\" }\n";
    for (inert, expect_calls) in [(false, 1_usize), (true, 2_usize)] {
        let yaml = if inert {
            base.replace(
                "      args: { url: \"${{ with.u }}\" }\n",
                "      args: { url: \"${{ with.u }}\" }\n    lift:\n      - law: data-as-code\n        because: \"archived for provenance · never loaded\"\n",
            )
        } else {
            base.to_owned()
        };
        let wf = nika_schema::parse(
            &yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_check::check(&wf);
        assert!(
            report.is_clean(),
            "the dynamic URL defers at check (law 3): {report:?}"
        );
        let executor = MockToolExecutor::new()
            .enqueue_ok(ToolResult::success(
                "tc1",
                "https://data.example.com/models/legacy.pkl",
            ))
            .enqueue_ok(ToolResult::success("tc2", "bytes"));
        let invoke = Arc::new(InvokeVerb::new(Arc::new(executor.clone())));
        let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
        let runtime = Runtime::new(
            ExecVerb::new(Arc::new(MockShell::new())),
            Arc::clone(&invoke),
            InferVerb::new(registry, "mock/echo"),
            AgentVerb::new(
                Arc::new(MockProvider::new("mock")),
                invoke,
                Arc::new(MockToolDefinitionProvider::new()),
                "mock/echo",
            ),
            MockClock::new(),
            RuntimeConfig::default().with_sandbox_root(std::path::PathBuf::from("/repo")),
        );
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        let outcome = runtime.run(&wf, &report, &mut stamper, &mut sink).await;
        let calls = executor.captured_calls();
        assert_eq!(
            calls.len(),
            expect_calls,
            "inert={inert} · the refusal fires BEFORE the executor (calls: {:?})",
            calls.iter().map(|c| c.name.clone()).collect::<Vec<_>>()
        );
        let outcome = outcome.expect("the run completes either way");
        if inert {
            assert!(outcome.ok, "the declared door lets the fetch through");
        } else {
            assert!(!outcome.ok, "the code-bearing fetch fails the run");
            let record = outcome.records.get("grab").expect("grab settled");
            let error = record.error.as_ref().expect("a typed refusal");
            assert_eq!(error.code, "NIKA-SEC-004");
            assert!(
                error.message.contains("code-bearing"),
                "the refusal teaches the sink: {}",
                error.message
            );
        }
    }
}

/// NEP-0007 — the permit witness end to end: every dispatch-boundary
/// decision becomes ONE `permit_checked` frame between `task_started`
/// and the terminal — the exec program gate and the env composition on
/// the allow side; the SAME channel carries the deny when the RUNTIME
/// gate refuses a check-deferred program (granted and refused alike ·
/// spec 17 §the permit witness).
#[tokio::test]
async fn permit_witness_frames_ride_the_journal() {
    use nika_kernel::tool_executor::ToolResult;
    use nika_kernel_mock::{
        MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
    };
    use nika_providers::{ProviderRegistry, ProvidersConfig};

    // (allowed, yaml): the allow case is a static echo under its permit;
    // the deny case derives the program from a task output — the static
    // judge DEFERS (dynamic command), the runtime gate refuses `curl`.
    let allow_yaml = "nika: witness\npermits:\n  exec: [\"echo\"]\ntasks:\n  stamp:\n    exec: { command: [\"echo\", \"ok\"] }\n";
    let deny_yaml = "nika: witness\npermits:\n  exec: [\"echo\"]\n  tools: [\"nika:jq\"]\ntasks:\n  name:\n    invoke:\n      tool: \"nika:jq\"\n      args: { input: \"curl ok\", expression: \".\" }\n  stamp:\n    with: { c: \"${{ tasks.name.output }}\" }\n    exec: { command: [\"${{ with.c }}\", \"ok\"] }\n";
    for (allowed, yaml) in [(true, allow_yaml), (false, deny_yaml)] {
        let wf = nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_check::check(&wf);
        assert!(report.is_clean(), "both shapes defer to run: {report:?}");
        let executor = MockToolExecutor::new().enqueue_ok(ToolResult::success("tc1", "curl ok"));
        let invoke = Arc::new(InvokeVerb::new(Arc::new(executor)));
        let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
        let runtime = Runtime::new(
            ExecVerb::new(Arc::new(MockShell::new().enqueue_ok("ok"))),
            Arc::clone(&invoke),
            InferVerb::new(registry, "mock/echo"),
            AgentVerb::new(
                Arc::new(MockProvider::new("mock")),
                invoke,
                Arc::new(MockToolDefinitionProvider::new()),
                "mock/echo",
            ),
            MockClock::new(),
            RuntimeConfig::default().with_sandbox_root(std::path::PathBuf::from("/repo")),
        );
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        let outcome = runtime.run(&wf, &report, &mut stamper, &mut sink).await;
        let outcome = outcome.expect("the run completes either way");
        assert_eq!(outcome.ok, allowed, "the gate decides the run");

        let events = sink.events();
        let field = |e: &nika_event::Event, key: &str| -> String {
            e.fields
                .iter()
                .find(|f| f.key == key)
                .map(|f| match &f.value {
                    FieldValue::String(s) => s.clone(),
                    other => format!("{other:?}"),
                })
                .unwrap_or_default()
        };
        let witness: Vec<(String, String)> = events
            .iter()
            .filter(|e| e.kind == EventKind::PermitChecked)
            .map(|e| (field(e, "plane"), field(e, "decision")))
            .collect();
        if allowed {
            assert!(
                witness.contains(&("exec".to_owned(), "allow".to_owned())),
                "the exec program gate witnesses its allow: {witness:?}"
            );
            assert!(
                witness.iter().any(|(p, d)| p == "env" && d == "allow"),
                "the env composition witnesses the passed names: {witness:?}"
            );
        } else {
            assert!(
                witness.contains(&("exec".to_owned(), "deny".to_owned())),
                "the refused gate rides the SAME channel: {witness:?}"
            );
        }
        // Position law (spec 17): the frames land between task_started
        // and the terminal.
        let kinds: Vec<EventKind> = events.iter().map(|e| e.kind).collect();
        let started = kinds
            .iter()
            .position(|k| *k == EventKind::TaskStarted)
            .expect("a task_started frame");
        let first_witness = kinds
            .iter()
            .position(|k| *k == EventKind::PermitChecked)
            .expect("at least one permit_checked frame");
        assert!(
            started < first_witness,
            "witness frames follow task_started"
        );
    }
}

/// NEP-0005 — the declared `permits.env:` passthrough rides every exec
/// command to the spawn site (which composes floor ∪ these names ∪ the
/// authored map from a cleared slate), and the authored task `env:` map
/// stays exactly the authored entries.
#[tokio::test]
async fn declared_env_passthrough_rides_the_exec_command() {
    use nika_kernel_mock::{
        MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
    };
    use nika_providers::{ProviderRegistry, ProvidersConfig};

    let yaml = "nika: envpass\npermits:\n  exec: [\"echo\"]\n  env: [\"CI_COMMIT_SHA\", \"CI_JOB_ID\"]\ntasks:\n  t:\n    exec:\n      command: [\"echo\", \"x\"]\n      env:\n        AUTHORED: \"lit\"\n";
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture passes the ladder: {report:?}");
    let shell = MockShell::new().enqueue_ok("ok");
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(shell.clone())),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::default().with_sandbox_root(std::path::PathBuf::from("/repo")),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean run");
    let sent = shell.executed_commands();
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0].env_passthrough,
        vec!["CI_COMMIT_SHA".to_owned(), "CI_JOB_ID".to_owned()],
        "the declared names ride the command to the spawn site"
    );
    assert_eq!(
        sent[0].env.get("AUTHORED").map(String::as_str),
        Some("lit"),
        "the authored task env: map stays the authored entries"
    );
}

/// No `permits:` block = ZERO AUTHORITY (F-O8): the check flags the
/// undeclared exec (NIKA-AUTH-006 → the report is dirty) and the run is
/// refused BEFORE the prologue (NIKA-1700 · audit-before-run) — zero
/// process spawned. The old « unconfined floor » (spec unset · blocklist
/// only) is retired: absent is never unconfined anymore.
#[tokio::test]
async fn absent_permits_refuses_the_exec_before_spawn() {
    use nika_kernel_mock::{
        MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
    };
    use nika_providers::{ProviderRegistry, ProvidersConfig};

    let yaml = "nika: floor\ntasks:\n  t:\n    exec: { command: [\"echo\", \"x\"] }\n";
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(
        !report.is_clean(),
        "absent permits + an exec effect = dirty (NIKA-AUTH-006): {report:?}"
    );
    let shell = MockShell::new().enqueue_ok("ok");
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(shell.clone())),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::default(),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let err = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect_err("audit-before-run refuses the dirty report");
    assert!(
        matches!(err, RuntimeError::DirtyReport),
        "NIKA-1700 · refused before the prologue: {err:?}"
    );
    assert!(
        shell.executed_commands().is_empty(),
        "zero process spawned — the exec died before spawn"
    );
}

/// would starve it forever (they'd only exist after gate itself
/// finished); the streamed spine settles fast first and unblocks the
/// wave. A 5s timeout turns a regression into a loud failure, never a
/// hung suite.
#[tokio::test]
async fn wave_settles_stream_before_the_join() {
    use nika_kernel_mock::{MockClock, MockProvider, MockShell, MockToolDefinitionProvider};
    use nika_providers::{ProviderRegistry, ProvidersConfig};
    use std::sync::atomic::AtomicBool;

    let yaml = "nika: stream-settle\npermits: { exec: [\"true\"], tools: [\"nika:jq\"] }\ntasks:\n  fast:\n    exec: { command: [\"true\"] }\n  gate:\n    invoke: { tool: \"nika:jq\", args: { input: [], expression: \".gate\" } }\n";
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture must check clean");
    assert_eq!(report.waves.len(), 1, "ONE wave — the whole point");

    let unblock = Arc::new(AtomicBool::new(false));
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(GateExecutor {
        unblock: Arc::clone(&unblock),
    })));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new().enqueue_ok("ok"))),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::default(),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = NotifyOnFastSettle {
        inner: VecSink::new(),
        unblock,
    };
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean run");
    assert!(outcome.ok, "both tasks settle green: {:?}", outcome.records);

    // The settle ORDER is unchanged (submission order — the spine):
    // fast's terminal frame precedes gate's.
    let completed: Vec<&str> = sink
        .inner
        .events()
        .iter()
        .filter(|e| e.kind == EventKind::TaskCompleted)
        .filter_map(|e| {
            e.fields
                .iter()
                .find(|kv| kv.key == "task")
                .and_then(|kv| match &kv.value {
                    FieldValue::String(s) => Some(s.as_str()),
                    _ => None,
                })
        })
        .collect();
    assert_eq!(completed, ["fast", "gate"], "the ordered spine holds");
}

/// S1 journal hygiene — the dynamic-flow backstop, end to end: an exec
/// The dynamic-leak fixture. Nothing here says `${{ secrets.X }}` — the
/// static IFC has nothing to sanction, the report is clean — and the
/// shell mock's OUTPUT is the secret's value, the same bytes a
/// file-sourced `cat` or an mcp echo would surface. `outputs:` returns
/// that value, so ONE fixture exercises both lanes it can ride.
const LEAK_YAML: &str = "nika: journal-hygiene\npermits: { exec: [\"echo\"] }\nsecrets:\n  tok: { source: env, key: NIKA_TOK }\ntasks:\n  leak:\n    exec: { command: [\"echo\", \"data\"] }\noutputs:\n  returned: ${{ tasks.leak.output }}\n";

/// The FIRST lane: every event the journal, the `--json` trace and the
/// live fold mirror byte for byte. Extracted so its sibling below has
/// a peer · two lanes, two helpers, neither able to rot alone.
fn assert_event_stream_is_scrubbed(sink: &VecSink, secret: &str) {
    // The journal mirrors this stream byte-for-byte (plus its `chain`
    // key) · serialize the way the NDJSON lanes do.
    let bytes = sink
        .events()
        .iter()
        .map(|e| serde_json::to_string(e).expect("event serializes"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !bytes.contains(secret),
        "no event carries the resolved value: {bytes}"
    );
    assert!(
        bytes.contains(secret::REDACTED),
        "the marker stands where the value surfaced: {bytes}"
    );
    // And precisely: the terminal frame's `outcome` payload · the leak
    // vector the audit named · carries the marker, not the plaintext.
    let outcome_field = sink
        .events()
        .iter()
        .find(|e| e.kind == EventKind::TaskCompleted)
        .and_then(|e| e.fields.iter().find(|kv| kv.key == "outcome"))
        .and_then(|kv| match &kv.value {
            FieldValue::String(s) => Some(s.as_str()),
            _ => None,
        })
        .expect("a terminal frame carries its outcome");
    assert!(outcome_field.contains(secret::REDACTED), "{outcome_field}");
}

/// What the run RETURNS is a second lane. `--output json` serializes
/// `RunOutcome.outputs` verbatim, and that map is not an event, so the
/// redacting sink never saw it: the same value was `***` in the trace
/// and in the clear on stdout (2026-08-02 · an adversarial pass).
fn assert_returned_outputs_are_scrubbed(outcome: &RunOutcome, secret: &str) {
    let returned = serde_json::to_string(&outcome.outputs).expect("outputs serialize");
    assert!(
        !returned.contains(secret),
        "the returned outputs carry the resolved value: {returned}"
    );
    assert!(
        returned.contains(secret::REDACTED),
        "the marker stands in the returned outputs too: {returned}"
    );
}

/// ECHOES a resolved secret (the static IFC sanctioned the declared
/// flow; a file-sourced `cat` or an mcp echo takes the same dynamic
/// path), so the value lands in the task's OUTPUT and would ride the
/// terminal frame's `outcome` payload into the journal in plaintext.
/// The stream every lane mirrors must carry the marker, never the
/// value.
#[tokio::test]
async fn resolved_secret_is_scrubbed_from_every_lane_it_can_ride() {
    use nika_kernel_mock::{
        MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
    };
    use nika_providers::{ProviderRegistry, ProvidersConfig};

    /// The composer's resolver, scripted (one env-sourced value).
    struct MapResolver(&'static str, &'static str);
    impl WorkflowSecretResolver for MapResolver {
        fn resolve(
            &self,
            name: &str,
            _reference: &nika_schema::types::SecretRef,
        ) -> Result<String, SecretResolveError> {
            if name == self.0 {
                Ok(self.1.to_owned())
            } else {
                Err(SecretResolveError {
                    name: name.to_owned(),
                    reason: "absent".to_owned(),
                })
            }
        }
    }

    const SECRET: &str = "sk-live-9f2c7e4a1b6d"; // redacted by provenance — never a length floor
    // The leak is DYNAMIC by construction: argv carries no `${{ secrets.X }}`
    // (the static IFC sees nothing to sanction — the report is clean), and
    // the shell mock's OUTPUT is the secret value — the same bytes a
    // file-sourced `cat` or an mcp echo would surface. What the scrub must
    // catch is the value, wherever it came from.
    // `outputs:` returns the task's own value, which is the OTHER lane
    // the same bytes ride: `RunOutcome.outputs` is not an event, so the
    // redacting sink never saw it and `--output json` printed it in the
    // clear while the trace said `***` (2026-08-02 · an adversarial pass
    // over the day's work). One fixture, both lanes, so neither can be
    // fixed while the other rots.
    let wf = nika_schema::parse(
        LEAK_YAML,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture passes the ladder: {report:?}");

    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new().enqueue_ok(SECRET))),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::default(),
    )
    .with_secret_resolver(Arc::new(MapResolver("tok", SECRET)));
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean run");
    assert!(outcome.ok, "the workflow itself succeeds");

    assert_event_stream_is_scrubbed(&sink, SECRET);
    assert_returned_outputs_are_scrubbed(&outcome, SECRET);

    // The SECOND lane: what the run RETURNS. `--output json` serializes
    // this map verbatim, so a value redacted in the trace and printed
    // here would be the same leak on a different channel.
    let returned = serde_json::to_string(&outcome.outputs).expect("outputs serialize");
    assert!(
        !returned.contains(SECRET),
        "the returned outputs carry the resolved value: {returned}"
    );
    assert!(
        returned.contains(secret::REDACTED),
        "the marker stands in the returned outputs too: {returned}"
    );
}

#[test]
fn runtime_config_default_is_wave_width_seed_zero() {
    let cfg = RuntimeConfig::default();
    assert!(cfg.wave_parallelism.is_none());
    assert_eq!(cfg.jitter_seed, 0);
}

#[test]
fn envelope_values_carries_typed_defaults_and_containers() {
    // The v1 string-only view dropped typed list defaults — the
    // value model must carry them (for_each collections · spec 03).
    let yaml = r#"
nika: vals
inputs:
  API_BASE: { type: string, required: false, default: "https://api.example.test" }
  topic: { type: string, required: false, default: "news" }
const:
  plain: "text"
  urls: ["a", "b"]
tasks:
  t:
    exec: { command: ["true"] }
"#;
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("parses");
    let EnvelopeValues {
        inputs,
        consts,
        workflow_name: name,
    } = envelope_values(&wf, &BTreeMap::new());
    assert_eq!(name, "vals");
    assert_eq!(
        inputs["API_BASE"],
        Value::String("https://api.example.test".into())
    );
    assert_eq!(inputs["topic"], Value::String("news".into()));
    assert_eq!(consts["plain"], Value::String("text".into()));
    assert_eq!(consts["urls"], serde_json::json!(["a", "b"]));
}

#[test]
fn typed_output_type_mismatch_is_a_var009() {
    // `outputs.n: { type: string }` — when the resolved value is a number
    // the callable contract is broken (spec 01 §engine-MUST rule 6).
    let yaml = r#"
nika: typed-out
tasks:
  t:
    invoke: { tool: "nika:jq", args: { input: { x: 42 }, expression: ".x" } }
outputs:
  n:
    value: ${{ tasks.t.output }}
    type: string
"#;
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("parses");
    // A number where `string` is declared → NIKA-VAR-009.
    let bad = BTreeMap::from([("n".to_owned(), serde_json::json!(42))]);
    let v =
        first_output_type_violation(&wf, &bad).expect("number vs declared string is a violation");
    assert_eq!(v.name, "n");
    assert_eq!(v.expected, "string");
    assert_eq!(v.actual, "number");
    // The declared type → no violation.
    let good = BTreeMap::from([("n".to_owned(), serde_json::json!("hello"))]);
    assert!(first_output_type_violation(&wf, &good).is_none());
    // An unresolved output (omitted upstream) is NOT a type error.
    assert!(first_output_type_violation(&wf, &BTreeMap::new()).is_none());
}

#[test]
fn declared_output_types_fit_lenient_floats_strict_cross_type() {
    use nika_types::types::{fits, parse_type};
    use serde_json::json;
    // The one type core owns the judgment (R3b): a whole float inhabits
    // `integer` (the lenient half), a genuine cross-type mismatch refuses.
    let type_names = std::collections::BTreeSet::new();
    let named = std::collections::BTreeMap::new();
    let ty = |expr: serde_json::Value| parse_type(&expr, &type_names, "t").expect("in-grammar");
    let int = ty(json!("integer"));
    assert!(fits(&json!(42), &int, &named));
    assert!(fits(&json!(42.0), &int, &named));
    assert!(!fits(&json!(42.5), &int, &named));
    // number: any JSON number, but NOT a numeric string.
    let num = ty(json!("number"));
    assert!(fits(&json!(42), &num, &named));
    assert!(!fits(&json!("42"), &num, &named));
    // array vs object are distinct · bool is the one boolean spelling.
    let arr = ty(json!({ "array": "number" }));
    assert!(fits(&json!([1, 2]), &arr, &named));
    assert!(!fits(&json!({}), &arr, &named));
    assert!(!fits(&json!(["x"]), &arr, &named), "element misfit");
    let obj = ty(json!({ "object": { "k": "number" }, "additional": true }));
    assert!(fits(&json!({ "k": 1 }), &obj, &named));
    assert!(fits(&json!("x"), &ty(json!("string")), &named));
    assert!(fits(&json!(true), &ty(json!("bool")), &named));
    // `boolean` / bare `array` are out of the grammar (LAW-GRAMMAR-0211).
    assert!(parse_type(&json!("boolean"), &type_names, "t").is_err());
    assert!(parse_type(&json!("array"), &type_names, "t").is_err());
}

/// A recovered success emits `task_recovered` BEFORE the terminal
/// `task_completed` (engine#301 · D-2026-07-08-N4 sequence lock:
/// `… > task_recovered > task_completed`) — and carries WHAT it
/// recovered from as a `code` field. A clean success emits no such
/// frame (pinned by every other Success test in this file).
#[test]
fn recovered_success_emits_task_recovered_before_completed() {
    let ran = task::RanTask {
        decisions: Vec::new(),
        note: "exec · sh".to_owned(),
        retries: Vec::new(),
        agent_events: Vec::new(),
        evidence: None,
        duration_ms: 0,
        result: task::RunResult::Success {
            value: Value::Number(99.into()),
            tokens: None,
            recovered_from: Some(crate::record::TaskErrorRecord {
                code: "NIKA-EXEC-001".to_owned(),
                message: "exit 9".to_owned(),
                transient: false,
            }),
            warning: None,
            child: None,
            cost_usd: None,
            cost_unpriced: None,
            model: None,
        },
    };
    let mut ok = true;
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    settle::settle_ran(
        "risky",
        ran,
        None,
        &TRUSTED,
        &[],
        None,
        &mut ok,
        &mut stamper,
        &mut sink,
    );

    let kinds: Vec<EventKind> = sink.events().iter().map(|e| e.kind).collect();
    let rec = kinds
        .iter()
        .position(|k| *k == EventKind::TaskRecovered)
        .expect("a TaskRecovered frame");
    let done = kinds
        .iter()
        .position(|k| *k == EventKind::TaskCompleted)
        .expect("a TaskCompleted frame — completed STAYS the one success terminal");
    assert!(rec < done, "task_recovered inserts BEFORE the terminal");

    let frame = &sink.events()[rec];
    assert!(
        frame.fields.iter().any(|f| f.key == "code"
            && matches!(&f.value, FieldValue::String(s) if s == "NIKA-EXEC-001")),
        "the frame names what was recovered FROM"
    );
    assert!(ok, "a recovered task is a SUCCESS at workflow level");
}

/// A settled success carrying a non-fatal `warning` puts it on the
/// `TaskCompleted` frame as a `warning` field — the wiring proof that
/// a dispatch diagnostic actually reaches the event stream. (The OBS-E
/// empty-answer producer left this channel at #651 — promoted to the
/// typed NIKA-INFER-004 failure; the channel itself stays.)
#[test]
fn obs_e_warning_rides_task_completed() {
    let ran = task::RanTask {
        decisions: Vec::new(),
        note: "infer · gemini/flash".to_owned(),
        retries: Vec::new(),
        agent_events: Vec::new(),
        evidence: None,
        duration_ms: 0,
        result: task::RunResult::Success {
            value: Value::String(String::new()),
            tokens: Some(84),
            recovered_from: None,
            warning: Some("infer produced an empty answer · …".to_owned()),
            child: None,
            cost_usd: Some(0.0125),
            cost_unpriced: None,
            model: None,
        },
    };
    let mut ok = true;
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    settle::settle_ran(
        "think",
        ran,
        None,
        &TRUSTED,
        &[],
        None,
        &mut ok,
        &mut stamper,
        &mut sink,
    );

    let completed = sink
        .events()
        .iter()
        .find(|e| e.kind == EventKind::TaskCompleted)
        .expect("a TaskCompleted frame");
    let warning = completed
        .fields
        .iter()
        .find(|f| f.key == "warning")
        .expect("the warning field rides the success frame");
    assert!(
        matches!(&warning.value, FieldValue::String(s) if s.contains("empty answer")),
        "the diagnostic text is carried verbatim"
    );
    // Real spend rides the same frame · absent-when-unpriced is pinned
    // by the sibling test below (its cost_usd is None · no field).
    let cost = completed
        .fields
        .iter()
        .find(|f| f.key == "cost_usd")
        .expect("the cost_usd field rides the priced success frame");
    assert!(
        matches!(&cost.value, FieldValue::Float(c) if (*c - 0.0125).abs() < f64::EPSILON),
        "the priced spend is carried verbatim"
    );
}

/// The common path · a success with no diagnostic emits NO `warning`
/// field (zero false-alarm noise on the happy stream).
#[test]
fn no_warning_field_on_a_clean_success() {
    let ran = task::RanTask {
        decisions: Vec::new(),
        note: "exec · true".to_owned(),
        retries: Vec::new(),
        agent_events: Vec::new(),
        evidence: None,
        duration_ms: 0,
        result: task::RunResult::Success {
            value: Value::String("ok".to_owned()),
            tokens: None,
            recovered_from: None,
            warning: None,
            child: None,
            cost_usd: None,
            cost_unpriced: None,
            model: None,
        },
    };
    let mut ok = true;
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    settle::settle_ran(
        "t",
        ran,
        None,
        &TRUSTED,
        &[],
        None,
        &mut ok,
        &mut stamper,
        &mut sink,
    );

    let completed = sink
        .events()
        .iter()
        .find(|e| e.kind == EventKind::TaskCompleted)
        .expect("a TaskCompleted frame");
    assert!(
        !completed.fields.iter().any(|f| f.key == "warning"),
        "no warning on a clean success"
    );
    assert!(
        !completed.fields.iter().any(|f| f.key == "cost_usd"),
        "an unpriced success carries NO cost field — absent is honest, never a fake zero"
    );
    assert!(
        !completed.fields.iter().any(|f| f.key == "cost_unpriced"),
        "an exec success is not COST-unpriced — no reason noise on verbs that spend nothing"
    );
    assert!(
        !completed
            .fields
            .iter()
            .any(|f| f.key == "model" || f.key == "access"),
        "a modelless verb carries no access facts — the fields ride ONLY where a model ran"
    );
}

/// D-2026-08-04-N1 · the access facts ride the infer terminal as
/// STRUCTURED fields (`model` · `provider` · `access` · `billing`) —
/// the display's note-string parse becomes a historical-trace fallback,
/// never the carrier.
#[test]
fn access_facts_ride_the_infer_terminal() {
    let ran = task::RanTask {
        decisions: Vec::new(),
        note: "infer · mock/echo".to_owned(),
        retries: Vec::new(),
        agent_events: Vec::new(),
        evidence: None,
        duration_ms: 0,
        result: task::RunResult::Success {
            value: Value::String("hi".to_owned()),
            tokens: Some(3),
            recovered_from: None,
            warning: None,
            child: None,
            cost_usd: None,
            cost_unpriced: Some(nika_types::cost::UnpricedReason::MockProvider),
            model: Some("mock/echo".to_owned()),
        },
    };
    let mut ok = true;
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    settle::settle_ran(
        "ask",
        ran,
        None,
        &TRUSTED,
        &[],
        None,
        &mut ok,
        &mut stamper,
        &mut sink,
    );
    let completed = sink
        .events()
        .iter()
        .find(|e| e.kind == EventKind::TaskCompleted)
        .expect("a TaskCompleted frame");
    let sfield = |k: &str| {
        completed
            .fields
            .iter()
            .find(|f| f.key == k)
            .map(|f| match &f.value {
                crate::FieldValue::String(v) => v.clone(),
                other => panic!("{k} is not a string field: {other:?}"),
            })
    };
    assert_eq!(sfield("model").as_deref(), Some("mock/echo"));
    assert_eq!(sfield("provider").as_deref(), Some("mock"));
    assert_eq!(sfield("access").as_deref(), Some("mock"));
    assert_eq!(
        sfield("billing").as_deref(),
        Some("local"),
        "mock compute is the local lane — never presented as free, never metered"
    );
    // the note keeps its historical render — now a render, not the carrier
    assert!(sfield("note").as_deref() == Some("infer · mock/echo"));
}

/// The WHY channel: an unpriced INFER success carries the reason —
/// `unknown` is never masked (a local model says « local compute ·
/// not priced », never a blank).
#[test]
fn cost_unpriced_reason_rides_task_completed() {
    let ran = task::RanTask {
        decisions: Vec::new(),
        note: "infer · ollama/llama3.2".to_owned(),
        retries: Vec::new(),
        agent_events: Vec::new(),
        evidence: None,
        duration_ms: 0,
        result: task::RunResult::Success {
            value: Value::String("bonjour".to_owned()),
            tokens: Some(12),
            recovered_from: None,
            warning: None,
            child: None,
            cost_usd: None,
            cost_unpriced: Some(nika_types::cost::UnpricedReason::LocalModel),
            model: Some("ollama/llama3.2".to_owned()),
        },
    };
    let mut ok = true;
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    settle::settle_ran(
        "ask",
        ran,
        None,
        &TRUSTED,
        &[],
        None,
        &mut ok,
        &mut stamper,
        &mut sink,
    );

    let completed = sink
        .events()
        .iter()
        .find(|e| e.kind == EventKind::TaskCompleted)
        .expect("a TaskCompleted frame");
    assert!(
        !completed.fields.iter().any(|f| f.key == "cost_usd"),
        "no fake zero next to the reason"
    );
    let reason = completed
        .fields
        .iter()
        .find(|f| f.key == "cost_unpriced")
        .expect("the WHY rides the frame");
    assert!(
        matches!(&reason.value, FieldValue::String(s) if s == "local_model"),
        "snake_case wire form"
    );
}

/// `permits.tools` enforcement, TWO layers (spec 01 §permits · NIKA-SEC-004
/// at the task level · NIKA-1707 at the run level).
///
/// - **The trust gate** (run start · this module's forged fixtures) — a
///   clean report forged for a different (permit-free) workflow never
///   reaches dispatch: the runtime re-derives the boundary lanes from the
///   workflow BYTES and refuses the mismatch (NIKA-1707) before any event
///   is emitted. The tool name is always literal, so the tools axis is
///   statically decidable — the trust gate owns it.
/// - **The dispatch gate** (in-run · fail-closed backstop) — an effect
///   the STATIC scan cannot see (a `${{ }}`-built value) is judged at
///   dispatch against the workflow's OWN declared boundary, never the
///   report. The dynamic half is pinned in `tests/exec_permits.rs` and
///   `tests/boundary_differential.rs`.
///
/// The honest path (check THEN run the same file) is covered by the
/// granted fixtures, which ride their REAL clean report.
#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tools_permits_tests {
    use std::sync::Arc;

    use nika_kernel::tool_executor::ToolResult;
    use nika_kernel_mock::{
        MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
    };
    use nika_verb_agent::AgentVerb;
    use nika_verb_exec::ExecVerb;
    use nika_verb_infer::InferVerb;
    use nika_verb_invoke::InvokeVerb;

    use crate::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, TaskStatus, VecSink};
    use crate::{EventKind, FieldValue};

    type MockRuntime = Runtime<
        MockShell,
        MockToolExecutor,
        nika_providers::NoHttp,
        MockProvider,
        MockToolDefinitionProvider,
        MockClock,
    >;

    fn runtime_with(executor: MockToolExecutor, provider: MockProvider) -> MockRuntime {
        let invoke = Arc::new(InvokeVerb::new(Arc::new(executor)));
        Runtime::new(
            ExecVerb::new(Arc::new(MockShell::new())),
            Arc::clone(&invoke),
            InferVerb::new(
                Arc::new(nika_providers::ProviderRegistry::without_http(
                    nika_providers::ProvidersConfig::new(),
                )),
                "mock/echo",
            ),
            AgentVerb::new(
                Arc::new(provider),
                invoke,
                Arc::new(MockToolDefinitionProvider::new()),
                "mock/echo",
            ),
            MockClock::new(),
            RuntimeConfig::default(),
        )
    }

    fn parse(yaml: &str) -> nika_schema::raw::RawWorkflow {
        nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses")
    }

    /// The embedder-bypass shape: `check` refuses the REAL workflow (its
    /// body escapes the declared boundary), so the run is fed the CLEAN
    /// report of its wide-boundary twin — same tasks, same waves. A skipped
    /// check and a forged report are indistinguishable to the runtime:
    /// the trust gate re-derives the boundary from the WORKFLOW bytes and
    /// unmasks the twin's report at run start (NIKA-1707), before dispatch.
    /// Every fixture writes `permits:` as ONE line so the twin is a
    /// line-swap away. (F-O8: the pre-F-O8 permit-free twin is dirty now —
    /// absent = zero authority — so the clean donor is the WIDE boundary
    /// that admits the body.)
    fn forged_clean_report(
        yaml: &str,
        wide_permits: &str,
    ) -> (nika_schema::raw::RawWorkflow, nika_check::CheckReport) {
        let wf = parse(yaml);
        assert!(
            !nika_check::check(&wf).is_clean(),
            "the honest check refuses the real workflow (the static half)"
        );
        let twin_yaml = yaml
            .lines()
            .map(|line| {
                if line.starts_with("permits:") {
                    wide_permits
                } else {
                    line
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let report = nika_check::check(&parse(&twin_yaml));
        assert!(
            report.is_clean(),
            "the wide-boundary twin checks clean (same tasks → same waves)"
        );
        (wf, report)
    }

    async fn run(
        runtime: &MockRuntime,
        wf: &nika_schema::raw::RawWorkflow,
        report: &nika_check::CheckReport,
    ) -> RunOutcome {
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        runtime
            .run(wf, report, &mut stamper, &mut sink)
            .await
            .expect("run settles")
    }

    /// A forged-report run ABORTS at the trust gate — the error, for
    /// inspection. The refusal precedes the prologue (fail-closed): the
    /// sink MUST stay empty, pinned here once for every caller.
    async fn run_refused(
        runtime: &MockRuntime,
        wf: &nika_schema::raw::RawWorkflow,
        report: &nika_check::CheckReport,
    ) -> crate::RuntimeError {
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        let err = runtime
            .run(wf, report, &mut stamper, &mut sink)
            .await
            .expect_err("a forged report never reaches dispatch");
        assert!(
            sink.events().is_empty(),
            "refused BEFORE any event — not even the prologue: {:?}",
            sink.events()
        );
        err
    }

    /// A tool OUTSIDE `permits.tools` under a forged clean report is
    /// unmasked at the TRUST GATE — the run aborts NIKA-1707 before any
    /// event, and the executor is NEVER reached. (The static checker
    /// speaks NIKA-SEC-004 for the same body; a forged report is the
    /// audit-before-run class, same family as a dirty one.)
    #[tokio::test]
    async fn invoke_outside_tools_boundary_is_unmasked_at_the_trust_gate() {
        let (wf, report) = forged_clean_report(
            "nika: tools-deny\npermits: { tools: [\"nika:read\"] }\ntasks:\n  danger:\n    invoke: { tool: \"nika:write\", args: { path: \"x\", content: \"y\" } }\n",
            "permits: { tools: [\"nika:read\", \"nika:write\"], fs: { write: [\"x\"] } }",
        );
        let executor = MockToolExecutor::new(); // EMPTY — any call is a bug
        let probe = executor.clone();
        let runtime = runtime_with(executor, MockProvider::new("mock"));
        let err = run_refused(&runtime, &wf, &report).await;
        let crate::RuntimeError::ReportMismatch { detail } = &err else {
            panic!("expected ReportMismatch, got {err:?}");
        };
        assert_eq!(err.spec_code(), "NIKA-1707");
        assert!(
            detail.contains("capability escape · task `danger`"),
            "the re-derived escape is named: {detail}"
        );
        assert!(
            probe.captured_calls().is_empty(),
            "the refused tool NEVER executed"
        );
    }

    /// A declared block that omits `tools:` grants NO tool (default-deny —
    /// the exact `Permits::allows_tool` verdict the static scan pins: an
    /// omitted category is not an allow-all) — the twin's clean report is
    /// unmasked at the trust gate.
    #[tokio::test]
    async fn declared_block_with_omitted_tools_denies_every_tool() {
        let (wf, report) = forged_clean_report(
            "nika: tools-omitted\npermits: { exec: true }\ntasks:\n  t:\n    invoke: { tool: \"nika:read\", args: { path: \"x\" } }\n",
            "permits: { exec: true, tools: [\"nika:read\"], fs: { read: [\"x\"] } }",
        );
        let executor = MockToolExecutor::new();
        let probe = executor.clone();
        let runtime = runtime_with(executor, MockProvider::new("mock"));
        let err = run_refused(&runtime, &wf, &report).await;
        let crate::RuntimeError::ReportMismatch { detail } = &err else {
            panic!("expected ReportMismatch, got {err:?}");
        };
        assert!(
            detail.contains("capability escape · task `t`"),
            "the omitted-category escape is named: {detail}"
        );
        assert!(probe.captured_calls().is_empty());
    }

    /// The honest path: a granted tool passes check AND run — the gate is
    /// inert on a body that fits its declared boundary.
    #[tokio::test]
    async fn invoke_inside_tools_boundary_runs_on_the_real_report() {
        let wf = parse(
            "nika: tools-allow\npermits: { tools: [\"nika:read\"], fs: { read: [\"x\"] } }\ntasks:\n  ok:\n    invoke: { tool: \"nika:read\", args: { path: \"x\" } }\n",
        );
        let report = nika_check::check(&wf);
        assert!(report.is_clean(), "the fixture fits its boundary");
        let executor = MockToolExecutor::new().enqueue_ok(ToolResult::success("t1", "file-bytes"));
        let probe = executor.clone();
        let runtime = runtime_with(executor, MockProvider::new("mock"));
        let outcome = run(&runtime, &wf, &report).await;
        assert!(outcome.ok, "a granted tool runs to success");
        assert_eq!(outcome.records["ok"].status, TaskStatus::Success);
        assert_eq!(probe.captured_calls().len(), 1, "exactly one tool call");
    }

    /// F-O1 PR-1 · the coarse integrity label end to end (ADDITIVE — the
    /// verdicts and the frames' pre-existing fields are untouched):
    /// a fetch output is born untrusted · the taint propagates through
    /// `with:` + `${{ }}` reads · an `inputs.` read is the caller
    /// boundary · a literal-only task stays trusted. The terminal frame
    /// carries the label ONLY when untrusted (old journals stay
    /// readable); no gate consumes it yet (PR-2).
    #[tokio::test]
    async fn integrity_label_flows_from_ingress_to_the_records_and_frames() {
        let wf = parse(
            "nika: integ-label\ninputs:\n  q: { type: string, required: false, default: \"authored-default\" }\npermits: { tools: [\"nika:fetch\", \"nika:jq\"], net: { http: [\"example.com\"] } }\ntasks:\n  dl:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://example.com/page\" } }\n  probe:\n    with: { page: \"${{ tasks.dl.output }}\" }\n    invoke: { tool: \"nika:jq\", args: { expression: \".\", input: \"${{ with.page }}\" } }\n  plain:\n    invoke: { tool: \"nika:jq\", args: { expression: \".\", input: \"authored\" } }\n  inp:\n    invoke: { tool: \"nika:jq\", args: { expression: \".\", input: \"${{ inputs.q }}\" } }\n",
        );
        let report = nika_check::check(&wf);
        assert!(report.is_clean(), "the fixture fits its boundary");
        // Wave 0 = dl + plain + inp (any dispatch interleaving — the
        // results are interchangeable) · wave 1 = probe.
        let executor = MockToolExecutor::new()
            .enqueue_ok(ToolResult::success("t1", "attacker-controlled page"))
            .enqueue_ok(ToolResult::success("t2", "\"ok\""))
            .enqueue_ok(ToolResult::success("t3", "\"ok\""))
            .enqueue_ok(ToolResult::success("t4", "\"ok\""));
        let runtime = runtime_with(executor, MockProvider::new("mock"));
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        let outcome = runtime
            .run(&wf, &report, &mut stamper, &mut sink)
            .await
            .expect("run settles");
        assert!(outcome.ok, "the label changes NO verdict: {outcome:?}");

        // The records carry the coarse label + the born-origin witness.
        assert_eq!(
            outcome.records["dl"].integrity,
            nika_cap::Integrity::untrusted("dl"),
            "a fetch output is born untrusted"
        );
        assert_eq!(
            outcome.records["probe"].integrity,
            nika_cap::Integrity::untrusted("dl"),
            "the taint propagates through with: + the effect read"
        );
        assert_eq!(
            outcome.records["inp"].integrity,
            nika_cap::Integrity::untrusted("inputs.q"),
            "an inputs read is the caller boundary"
        );
        assert_eq!(
            outcome.records["plain"].integrity,
            nika_cap::Integrity::trusted(),
            "a literal-only task stays trusted"
        );

        // The terminal frame carries the label ONLY when untrusted.
        let completed_for = |task: &str| {
            sink.events()
                .iter()
                .find(|e| {
                    e.kind == EventKind::TaskCompleted
                        && e.fields.iter().any(|f| {
                            f.key == "task"
                                && matches!(&f.value, FieldValue::String(s) if s == task)
                        })
                })
                .expect("a TaskCompleted frame per task")
        };
        let dl = completed_for("dl");
        assert!(
            dl.fields.iter().any(|f| f.key == "integrity"
                && matches!(&f.value, FieldValue::String(s) if s == "untrusted")),
            "the untrusted frame names the label"
        );
        assert!(
            dl.fields.iter().any(|f| f.key == "integrity_source"
                && matches!(&f.value, FieldValue::String(s) if s == "dl")),
            "the frame names the born origin"
        );
        let plain = completed_for("plain");
        assert!(
            !plain.fields.iter().any(|f| f.key == "integrity"),
            "a trusted task emits NO integrity field — old journals stay readable"
        );
    }

    /// The agent verb's declared `tools:` universe is gated the same way —
    /// unmasked at the trust gate BEFORE any provider call (the in-loop
    /// whitelist governs the model's picks only within a boundary-fitting
    /// universe).
    #[tokio::test]
    async fn agent_universe_outside_tools_boundary_is_refused() {
        let (wf, report) = forged_clean_report(
            "nika: agent-tools-deny\nmodel: mock/echo\npermits: { tools: [\"nika:read\"] }\ntasks:\n  go:\n    agent:\n      prompt: \"go\"\n      tools: [\"nika:read\", \"nika:write\"]\n",
            "permits: { tools: [\"nika:read\", \"nika:write\"] }",
        );
        let provider = MockProvider::new("mock").enqueue_text("never reached");
        let probe = provider.clone();
        let runtime = runtime_with(MockToolExecutor::new(), provider);
        let err = run_refused(&runtime, &wf, &report).await;
        let crate::RuntimeError::ReportMismatch { detail } = &err else {
            panic!("expected ReportMismatch, got {err:?}");
        };
        assert!(
            detail.contains("capability escape · task `go`"),
            "the out-of-boundary universe is named: {detail}"
        );
        assert!(
            probe.captured_requests().is_empty(),
            "no token is spent on an out-of-boundary universe"
        );
    }

    /// A universe INSIDE the boundary runs untouched — the gate defers to
    /// the loop's whitelist for the model's picks (behavior unchanged).
    #[tokio::test]
    async fn agent_universe_inside_tools_boundary_runs() {
        let wf = parse(
            "nika: agent-tools-allow\nmodel: mock/echo\npermits: { tools: [\"nika:read\"] }\ntasks:\n  go:\n    agent:\n      prompt: \"go\"\n      tools: [\"nika:read\"]\n",
        );
        let report = nika_check::check(&wf);
        assert!(report.is_clean(), "the fixture fits its boundary");
        let provider = MockProvider::new("mock").enqueue_text("done");
        let probe = provider.clone();
        let runtime = runtime_with(MockToolExecutor::new(), provider);
        let outcome = run(&runtime, &wf, &report).await;
        assert!(outcome.ok, "a granted universe runs to success");
        assert_eq!(outcome.records["go"].status, TaskStatus::Success);
        assert_eq!(probe.captured_requests().len(), 1, "one provider turn");
    }
}

// ─── the F-P2 boot manifest (attestation inputs follow the declaration) ─────

/// The boot manifest claims only what exists, per the `run:`
/// declaration: absent → the system stamper + the system clock, no seed
/// claim · `entropy: none` → deterministic + virtual + the zero seed ·
/// `seeded(42)` → seed 42 · a lone `clock: virtual` is a test
/// configuration (system stamper · virtual clock · no seed). `spec_pin`
/// always rides (the workspace `SPEC_PIN` carries a hash line).
#[test]
fn the_boot_manifest_follows_the_run_declaration() {
    const HEAD: &str =
        "nika: w\npermits: { exec: [\"x\"] }\ntasks:\n  t:\n    exec: { command: [\"x\"] }\n";
    let dump = |yaml: &str| {
        let wf = nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        format!("{:?}", crate::prologue::boot_attestation_fields(&wf))
    };
    let ambient = dump(HEAD);
    assert!(ambient.contains("spec_pin"), "{ambient}");
    assert!(
        ambient.contains("stamper_kind") && ambient.contains("system"),
        "{ambient}"
    );
    assert!(ambient.contains("clock"), "{ambient}");
    assert!(
        !ambient.contains("seed"),
        "no seed claim on ambient: {ambient}"
    );
    let strict = dump(&format!("{HEAD}run: {{ entropy: none }}\n"));
    assert!(strict.contains("deterministic"), "{strict}");
    assert!(
        strict.contains("virtual"),
        "the forced virtual clock: {strict}"
    );
    assert!(
        strict.contains("seed"),
        "the zero stream is a claim: {strict}"
    );
    let seeded = dump(&format!("{HEAD}run: {{ entropy: {{ seeded: 42 }} }}\n"));
    assert!(seeded.contains("42"), "{seeded}");
    let lone_clock = dump(&format!("{HEAD}run: {{ clock: virtual }}\n"));
    assert!(
        lone_clock.contains("virtual") && !lone_clock.contains("seed"),
        "a lone virtual clock claims no determinism: {lone_clock}"
    );
}
