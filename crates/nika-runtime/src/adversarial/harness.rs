// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The assertion engine of the adversarial suite (threat model: `mod.rs`).
//!
//! Three verdict lanes, all against REAL machinery:
//!
//! - `static-deny` runs the true `parse` → `check` pipeline and matches the
//!   unified `findings` surface (gate + wire code + the named task) — and
//!   asserts `conformance` is empty, so the deny provably rides the
//!   security lane and never a broken fixture.
//! - `runtime-block` runs the true `Runtime` (the L3 orchestrator, the four
//!   verbs) with the hijack scripted on `MockProvider` and every other
//!   effect seam mocked (`MockToolExecutor` — or the REAL
//!   `BuiltinDispatcher` over `MockFs`/`MockHttp` when the fixture's
//!   boundary is the builtin's fs confinement). The run must check CLEAN
//!   first: if the static lane starts catching the shape, the fixture goes
//!   red and must be reclassified — the two lanes can never both sleep.
//! - `residual` is the same run, pinning today's UNBLOCKED behavior.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use nika_check::{CheckReport, check};
use nika_kernel::ai::provider::{ProviderInferDyn, ProviderMeta};
use nika_kernel::ai::tool_defs::ToolDefinitionProviderDyn;
use nika_kernel::clock::ClockDyn;
use nika_kernel::http::HttpPostDyn;
use nika_kernel::process::ShellRunDyn;
use nika_kernel::provider::{ContentBlock, InferResponse, StopReason, TokenUsage, ToolDef};
use nika_kernel::tool_executor::{ToolExecuteDyn, ToolResult};
use nika_kernel_mock::{
    MockClock, MockFs, MockHttp, MockProvider, MockShell, MockToolDefinitionProvider,
    MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_schema::raw::action::RawAction;
use nika_schema::raw::workflow::RawWorkflow;
use nika_schema::{FileId, ParseMode, parse};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

use crate::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, TaskStatus, VecSink};

use super::fixtures::{
    ExpectedStatus, Fixture, ProviderStep, RunExpect, Script, ShellStep, ToolStep,
};

/// Parse (Strict) + check — the shared preflight of both lanes.
fn preflight(fx: &Fixture) -> (RawWorkflow, CheckReport) {
    let wf = parse(&fx.yaml, FileId::new(0), ParseMode::Strict)
        .unwrap_or_else(|e| panic!("{}: attack.nika.yaml parses (Strict): {e}", fx.id()));
    let report = check(&wf);
    (wf, report)
}

/// `static-deny`: the security lane refuses the workflow at `nika check`.
pub(crate) fn assert_static_deny(fx: &Fixture) {
    let (_, report) = preflight(fx);
    let want =
        fx.sidecar.static_expect.as_ref().unwrap_or_else(|| {
            panic!("{}: verdict static-deny requires a `static` block", fx.id())
        });
    assert!(
        report.conformance.is_empty(),
        "{}: the deny must ride the security lane, not a broken fixture — conformance: {:?}",
        fx.id(),
        report.conformance
    );
    assert!(
        !report.is_clean(),
        "{}: the workflow must NOT check clean",
        fx.id()
    );
    let row = report
        .findings
        .iter()
        .find(|f| f.gate == want.gate && f.code.as_deref() == Some(want.code.as_str()))
        .unwrap_or_else(|| {
            panic!(
                "{}: expected a {} finding carrying {} — findings: {:?}",
                fx.id(),
                want.gate,
                want.code,
                finding_summary(&report)
            )
        });
    if let Some(task) = &want.task {
        assert_eq!(
            row.task.as_deref(),
            Some(task.as_str()),
            "{}: the finding must name the realized sink",
            fx.id()
        );
    }
}

/// The compact `(gate, code, task)` view of a report for failure messages.
fn finding_summary(report: &CheckReport) -> Vec<(&'static str, Option<&str>, Option<&str>)> {
    report
        .findings
        .iter()
        .map(|f| (f.gate, f.code.as_deref(), f.task.as_deref()))
        .collect()
}

/// `runtime-block` / `residual`: the workflow checks clean; the run pins
/// the injected action's fate plus every side-effect count.
pub(crate) async fn assert_runtime(fx: &Fixture) {
    let script = fx
        .sidecar
        .script
        .as_ref()
        .unwrap_or_else(|| panic!("{}: a runtime verdict requires a `script` block", fx.id()));
    let expect = fx
        .sidecar
        .expect
        .as_ref()
        .unwrap_or_else(|| panic!("{}: a runtime verdict requires an `expect` block", fx.id()));
    let (wf, report) = preflight(fx);
    assert!(
        report.is_clean(),
        "{}: a runtime fixture must check CLEAN — if the static lane now \
         catches it, RECLASSIFY the fixture as static-deny (never weaken \
         this assertion): {:?}",
        fx.id(),
        finding_summary(&report)
    );
    let provider = Arc::new(script_provider(script));
    let shell = script_shell(script);
    if script.real_tools {
        run_real_plane(fx, script, expect, &wf, &report, provider, shell).await;
    } else {
        run_mock_plane(fx, script, expect, &wf, &report, provider, shell).await;
    }
}

/// The mock-tool plane: every invoke/agent tool call rides a FIFO
/// `MockToolExecutor`; counts prove what dispatched and what did not.
async fn run_mock_plane(
    fx: &Fixture,
    script: &Script,
    expect: &RunExpect,
    wf: &RawWorkflow,
    report: &CheckReport,
    provider: Arc<MockProvider>,
    shell: MockShell,
) {
    let tools = Arc::new(script_tools(script));
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::clone(&tools)));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(shell.clone())),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::clone(&provider),
            invoke,
            Arc::new(MockToolDefinitionProvider::with_defs(universe(wf))),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::default(),
    );
    let outcome = settle(fx, &runtime, wf, report).await;
    assert_record(fx, expect, &outcome);
    assert_provider(fx, expect, &provider);
    assert_shell(fx, expect, &shell);
    if let Some(want) = expect.tool_calls {
        assert_eq!(
            tools.captured_calls().len(),
            want,
            "{}: tool dispatches pinned — an injected call that escaped the \
             whitelist would appear here",
            fx.id()
        );
    }
}

/// The real-builtin plane: the TRUE `BuiltinDispatcher` over mock fs/http
/// seams — the builtin's fs confinement (canonicalize-then-confine,
/// NIKA-SEC-004) genuinely fires, with zero I/O.
async fn run_real_plane(
    fx: &Fixture,
    script: &Script,
    expect: &RunExpect,
    wf: &RawWorkflow,
    report: &CheckReport,
    provider: Arc<MockProvider>,
    shell: MockShell,
) {
    let fs = MockFs::new();
    for (path, content) in &script.fs {
        fs.seed(path, content.clone().into_bytes());
    }
    let mut http = MockHttp::new();
    for step in &script.http {
        http = http.enqueue_ok(step.status, step.body.clone());
    }
    let dispatcher = Arc::new(
        nika_builtin::BuiltinDispatcher::new(
            Arc::new(fs.clone()),
            Arc::new(http.clone()),
            Arc::new(MockClock::new()),
            Arc::new(nika_builtin::NullEmitter::default()),
            Arc::new(nika_builtin::NonInteractive::default()),
            Arc::new(nika_builtin::NoWorkflow::default()),
        )
        .with_fs_boundary(fs_boundary_of(wf)),
    );
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::clone(&dispatcher)));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(shell.clone())),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::clone(&provider),
            invoke,
            Arc::clone(&dispatcher),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::default(),
    );
    let outcome = settle(fx, &runtime, wf, report).await;
    assert_record(fx, expect, &outcome);
    assert_provider(fx, expect, &provider);
    assert_shell(fx, expect, &shell);
    assert_net_and_fs(fx, expect, &http, &fs, script);
}

/// The real-plane side-effect pins: http request count + URL needles and
/// the mock fs's untouched-ness.
fn assert_net_and_fs(
    fx: &Fixture,
    expect: &RunExpect,
    http: &MockHttp,
    fs: &MockFs,
    script: &Script,
) {
    if let Some(want) = expect.http_calls {
        assert_eq!(
            http.sent_requests().len(),
            want,
            "{}: http requests pinned",
            fx.id()
        );
    }
    if let Some(needle) = &expect.http_url_contains {
        let sent = http.sent_requests();
        let urls: Vec<&str> = sent.iter().map(|r| r.url.as_str()).collect();
        assert!(
            urls.iter().any(|u| u.contains(needle)),
            "{}: some sent request URL must contain `{needle}` — sent: {urls:?}",
            fx.id()
        );
    }
    if expect.fs_untouched {
        let mut now: Vec<PathBuf> = fs.file_paths();
        let mut seeded: Vec<PathBuf> = script.fs.keys().map(PathBuf::from).collect();
        now.sort();
        seeded.sort();
        assert_eq!(
            now,
            seeded,
            "{}: the fs boundary must refuse BEFORE any write — the mock fs \
             holds exactly its seeded files",
            fx.id()
        );
    }
}

/// Run a check-clean fixture to settlement on the injected plane.
async fn settle<S, T, H, P, D, C>(
    fx: &Fixture,
    runtime: &Runtime<S, T, H, P, D, C>,
    wf: &RawWorkflow,
    report: &CheckReport,
) -> RunOutcome
where
    S: ShellRunDyn + Sync,
    T: ToolExecuteDyn,
    H: HttpPostDyn + Send + Sync + 'static,
    P: ProviderInferDyn + ProviderMeta,
    D: ToolDefinitionProviderDyn,
    C: ClockDyn + Sync,
{
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    runtime
        .run(wf, report, &mut stamper, &mut sink)
        .await
        .unwrap_or_else(|e| {
            panic!(
                "{}: the run settles (the clean preflight makes DirtyReport \
                 unreachable): {e}",
                fx.id()
            )
        })
}

/// The expected terminal record: status, wire code, never-transient.
fn assert_record(fx: &Fixture, expect: &RunExpect, outcome: &RunOutcome) {
    let task = expect
        .task
        .as_deref()
        .unwrap_or_else(|| panic!("{}: expect.task is required", fx.id()));
    let record = outcome.records.get(task).unwrap_or_else(|| {
        panic!(
            "{}: a record for task `{task}` — records: {:?}",
            fx.id(),
            outcome.records.keys().collect::<Vec<_>>()
        )
    });
    match expect.status.unwrap_or(ExpectedStatus::Failure) {
        ExpectedStatus::Success => assert_eq!(
            record.status,
            TaskStatus::Success,
            "{}: `{task}` settles green today (the residual pin) — error: {:?}",
            fx.id(),
            record.error
        ),
        ExpectedStatus::Failure => {
            assert_eq!(
                record.status,
                TaskStatus::Failure,
                "{}: `{task}` must fail — status: {:?}",
                fx.id(),
                record.status
            );
            let want = expect.code.as_deref().unwrap_or_else(|| {
                panic!("{}: a failure expectation requires expect.code", fx.id())
            });
            let err = record
                .error
                .as_ref()
                .unwrap_or_else(|| panic!("{}: `{task}` carries an error record", fx.id()));
            assert_eq!(
                err.code,
                want,
                "{}: the refusal speaks the expected wire code",
                fx.id()
            );
            assert!(
                !err.transient,
                "{}: a security refusal is never transient",
                fx.id()
            );
        }
    }
}

/// The provider pins: exact turn count (a refusal fed back would add a
/// turn) and the verbatim-payload needle.
fn assert_provider(fx: &Fixture, expect: &RunExpect, provider: &MockProvider) {
    if let Some(want) = expect.provider_calls {
        assert_eq!(
            provider.captured_requests().len(),
            want,
            "{}: model calls pinned — a refusal fed back to the model would \
             show up as an extra turn",
            fx.id()
        );
    }
    if let Some(rc) = &expect.request_contains {
        let requests = provider.captured_requests();
        let req = requests
            .get(rc.index)
            .unwrap_or_else(|| panic!("{}: a provider request #{}", fx.id(), rc.index));
        let dump = format!("{req:?}");
        assert!(
            dump.contains(&rc.needle),
            "{}: request #{} must carry the injected payload verbatim (the \
             proof it rode the channel under test)",
            fx.id(),
            rc.index
        );
    }
}

/// The shell pins: exact execution count (0 = the refusal is pre-spawn)
/// and the smuggled-argv needle for residual fixtures.
fn assert_shell(fx: &Fixture, expect: &RunExpect, shell: &MockShell) {
    let ran = shell.executed_commands();
    if let Some(want) = expect.shell_calls {
        assert_eq!(
            ran.len(),
            want,
            "{}: shell executions pinned (0 = the refusal is pre-spawn)",
            fx.id()
        );
    }
    if let Some(needle) = &expect.shell_argv_contains {
        let lines: Vec<String> = ran
            .iter()
            .map(|c| format!("{} {}", c.program, c.args.join(" ")))
            .collect();
        assert!(
            lines.iter().any(|line| line.contains(needle)),
            "{}: the executed command carries the smuggled payload `{needle}` \
             — ran: {lines:?}",
            fx.id()
        );
    }
}

/// The canned-token-usage helper (the mock plane never bills real spend).
fn usage() -> TokenUsage {
    let mut usage = TokenUsage::default();
    usage.input_tokens = 10;
    usage.output_tokens = 5;
    usage
}

/// The scripted hijack: FIFO model turns — text (`EndTurn`) or a tool call
/// (`ToolUse`, ids `c1..cN` in emission order).
fn script_provider(script: &Script) -> MockProvider {
    let mut provider = MockProvider::new("mock");
    let mut calls = 0usize;
    for step in &script.provider {
        match step {
            ProviderStep::Text { text } => {
                provider = provider.enqueue_response(InferResponse::new(
                    vec![ContentBlock::Text { text: text.clone() }],
                    usage(),
                    StopReason::EndTurn,
                ));
            }
            ProviderStep::ToolUse { tool_use } => {
                calls += 1;
                provider = provider.enqueue_response(InferResponse::new(
                    vec![
                        ContentBlock::Text {
                            text: format!("calling {}", tool_use.name),
                        },
                        ContentBlock::ToolUse {
                            id: format!("c{calls}"),
                            name: tool_use.name.clone(),
                            input: tool_use.input.clone(),
                        },
                    ],
                    usage(),
                    StopReason::ToolUse,
                ));
            }
        }
    }
    provider
}

/// The mock-tool FIFO (`r1..rN` result ids — the loop pairs by position).
fn script_tools(script: &Script) -> MockToolExecutor {
    let mut tools = MockToolExecutor::new();
    let mut n = 0usize;
    for step in &script.tools {
        n += 1;
        let id = format!("r{n}");
        tools = match step {
            ToolStep::Ok { ok } => tools.enqueue_ok(ToolResult::success(id, ok.clone())),
            ToolStep::Err { err } => tools.enqueue_ok(ToolResult::error(id, err.clone())),
        };
    }
    tools
}

/// The mock-shell FIFO.
fn script_shell(script: &Script) -> MockShell {
    let mut shell = MockShell::new();
    for step in &script.shell {
        shell = match step {
            ShellStep::Ok { ok } => shell.enqueue_ok(ok.clone()),
            ShellStep::Fail { fail } => shell.enqueue_fail(fail.status, fail.stderr.clone()),
        };
    }
    shell
}

/// The agent tool universe, taken from the workflow's own `tools:` lists
/// (the same names the loop's whitelist will admit).
fn universe(wf: &RawWorkflow) -> Vec<ToolDef> {
    let mut names: Vec<&str> = Vec::new();
    for task in &wf.tasks {
        if let RawAction::Agent(agent) = &task.value.action {
            for tool in &agent.tools {
                if !names.contains(&tool.value.as_str()) {
                    names.push(tool.value.as_str());
                }
            }
        }
    }
    names
        .into_iter()
        .map(|name| {
            ToolDef::new(
                name.to_owned(),
                format!("{name} tool"),
                serde_json::json!({}),
            )
        })
        .collect()
}

/// The production mapping (`nika-cli`'s `fs_boundary_of_permits`) mirrored:
/// a declared `permits:` block ⇒ default-deny glob lists; none ⇒ the
/// pre-permits floor (unbounded). Mirroring (not importing L4) keeps the
/// layer rule — and the boundary under test is the builtin's, not this
/// six-line projection.
fn fs_boundary_of(wf: &RawWorkflow) -> nika_builtin::FsBoundary {
    let Some(permits) = wf.permits.as_ref().map(|p| &p.value) else {
        return nika_builtin::FsBoundary::unbounded();
    };
    let (read, write) = permits
        .fs
        .as_ref()
        .map(|fs| (fs.read.clone(), fs.write.clone()))
        .unwrap_or_default();
    nika_builtin::FsBoundary::declared(read, write)
}
