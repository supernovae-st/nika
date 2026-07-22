// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! The F-O1 PR-2 runtime re-gate, end to end (NEP-0004 law 2 ·
//! `NIKA-SEC-004`): an UNTRUSTED value reaching a permitted verb's
//! argument is matched against the step's `permits:` on its RESOLVED,
//! canonical form. The untrusted source is a mocked `nika:fetch` (born
//! untrusted by the shared ingress law — no catalog consult); every
//! effect seam is mocked, so the refusal is the production code path
//! with zero I/O.
//!
//! The fixtures are ENGINE.md's own: the traversal under `fs.read`, the
//! option-injection under an argv allowlist, the in-permit pivot (the
//! re-gate is not a blind deny), and the `mcp:*` border both ways. Every
//! task hoists its tainted read into `with:` (NIKA-VAR-021's idiom — the
//! verb field then reads `${{ with.<name> }}`).

use std::sync::Arc;

use nika_check::check;
use nika_kernel::tool_executor::ToolResult;
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, TaskStatus, VecSink};
use nika_schema::{FileId, ParseMode, parse};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

/// Run one workflow whose untrusted ingress is ONE mocked fetch result.
/// Returns the outcome plus the mock handles (side-effect counts prove
/// the refusal is pre-spawn / pre-dispatch).
async fn run_with_ingress(
    yaml: &str,
    ingress_payload: &str,
    shell: MockShell,
) -> (RunOutcome, Arc<MockToolExecutor>, MockShell) {
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
    let report = check(&wf);
    assert!(
        report.is_clean(),
        "the re-gate is a RUNTIME law — the static ladder defers the dynamic value: {:?}",
        report.findings
    );
    let tools = Arc::new(
        MockToolExecutor::new().enqueue_ok(ToolResult::success("r1", ingress_payload.to_owned())),
    );
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::clone(&tools)));
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
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean run");
    (outcome, tools, shell)
}

/// The workflow spine of the exec fixtures: one fetch (the untrusted
/// ingress), then a `tar`/`make` task whose argv or cwd reads it through
/// a `with:` slot (NIKA-VAR-021's idiom).
fn exec_workflow(permits: &str, task: &str) -> String {
    format!(
        "nika: v1\nworkflow:\n  id: regate\n{permits}\ntasks:\n  dl:\n    invoke:\n      tool: \"nika:fetch\"\n      args: {{ url: \"https://news.example/payload.txt\" }}\n{task}\n"
    )
}

/// ENGINE.md fixture 1 (the ledger row): a caller-supplied value
/// (`inputs.p` — the caller boundary, untrusted by the Perl-taint slot
/// rule; the fetch-driven twin is the f3-03 adversarial fixture, which
/// declares no fs and so stays clear of the NEP-0002 trifecta) resolving
/// to `datasets/../../../etc/passwd` escapes `fs.read: [datasets/**]` —
/// the canonical fold refuses the argv element pre-spawn.
#[tokio::test]
async fn untrusted_traversal_under_fs_read_is_refused() {
    let yaml = "nika: v1\nworkflow:\n  id: regate\ninputs:\n  p: { type: string, default: \"datasets/../../../etc/passwd\" }\npermits:\n  exec: [\"tar\"]\n  fs: { read: [\"datasets/**\"] }\ntasks:\n  untar:\n    with: { p: \"${{ inputs.p }}\" }\n    exec: { command: [\"tar\", \"-xf\", \"${{ with.p }}\"] }\n";
    let (outcome, _, shell) = run_with_ingress(yaml, "unused", MockShell::new()).await;
    assert!(!outcome.ok, "a re-gate refusal fails the run");
    let rec = &outcome.records["untar"];
    assert_eq!(rec.status, TaskStatus::Failure);
    let err = rec.error.as_ref().expect("the refusal record");
    assert_eq!(err.code, "NIKA-SEC-004");
    assert!(
        err.message.contains("taint path: inputs.p -> exec.argv[2]")
            && err.message.contains("../../etc/passwd"),
        "source-first taint path + the canonical form: {}",
        err.message
    );
    assert!(
        shell.executed_commands().is_empty(),
        "the refusal is pre-spawn — the runner is never reached"
    );
}

/// ENGINE.md fixture 2: the untrusted argv tail resolves to a re-entry
/// option token (`--checkpoint-action=…`) where the author wrote a data
/// slot — option-injection, refused even under an argv allowlist.
#[tokio::test]
async fn untrusted_option_injection_is_refused() {
    let yaml = exec_workflow(
        "permits:\n  exec: [\"tar\"]\n  net: { http: [\"news.example\"] }\n  tools: [\"nika:fetch\"]",
        "  untar:\n    after: { dl: success }\n    with: { p: \"${{ tasks.dl.output }}\" }\n    exec: { command: [\"tar\", \"-xf\", \"${{ with.p }}\"] }",
    );
    let (outcome, _, shell) =
        run_with_ingress(&yaml, "--checkpoint-action=exec=sh id", MockShell::new()).await;
    assert!(!outcome.ok);
    let err = outcome.records["untar"]
        .error
        .as_ref()
        .expect("the refusal");
    assert_eq!(err.code, "NIKA-SEC-004");
    assert!(
        err.message.contains("option token"),
        "the refusal names the class: {}",
        err.message
    );
    assert!(shell.executed_commands().is_empty());
}

/// ENGINE.md fixture 4 (the pivot): the SAME slot canonicalizes INSIDE
/// the permit (`datasets/2026/report.csv` under `datasets/**`) — the
/// re-gate is not a blind deny, the step RUNS.
#[tokio::test]
async fn untrusted_value_inside_the_permit_runs() {
    let yaml = "nika: v1\nworkflow:\n  id: regate\ninputs:\n  p: { type: string, default: \"datasets/2026/report.csv\" }\npermits:\n  exec: [\"tar\"]\n  fs: { read: [\"datasets/**\"] }\ntasks:\n  untar:\n    with: { p: \"${{ inputs.p }}\" }\n    exec: { command: [\"tar\", \"-xf\", \"${{ with.p }}\"] }\n";
    let (outcome, _, shell) =
        run_with_ingress(yaml, "unused", MockShell::new().enqueue_ok("extracted\n")).await;
    assert!(outcome.ok, "a covered value runs: {:?}", outcome.records);
    assert_eq!(outcome.records["untar"].status, TaskStatus::Success);
    assert_eq!(
        shell.executed_commands().len(),
        1,
        "the in-permit exec reached the runner exactly once"
    );
}

/// The `mcp:*` border: an untrusted URL in a permitted mcp tool's args
/// escapes `net.http` — refused BEFORE the tool dispatch (the grant of
/// the tool is the category, never the resolved value).
#[tokio::test]
async fn untrusted_mcp_arg_escaping_net_is_refused() {
    let yaml = "nika: v1\nworkflow:\n  id: regate-mcp\npermits:\n  net: { http: [\"news.example\", \"api.example.com\"] }\n  tools: [\"nika:fetch\", \"mcp:store/put\"]\ntasks:\n  dl:\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://news.example/payload.txt\" }\n  put:\n    after: { dl: success }\n    with: { u: \"${{ tasks.dl.output }}\" }\n    invoke:\n      tool: \"mcp:store/put\"\n      args: { url: \"${{ with.u }}\" }\n";
    let (outcome, tools, _) =
        run_with_ingress(yaml, "https://evil.example/x", MockShell::new()).await;
    assert!(!outcome.ok);
    let err = outcome.records["put"].error.as_ref().expect("the refusal");
    assert_eq!(err.code, "NIKA-SEC-004");
    assert!(
        err.message.contains("dl -> mcp:store/put.args.url")
            && err.message.contains("host \"evil.example\""),
        "the mcp sink + the escaped host: {}",
        err.message
    );
    assert_eq!(
        tools.captured_calls().len(),
        1,
        "only the fetch dispatched — the mcp call never reached the executor"
    );
}

/// The `mcp:*` border, in-permit: the untrusted path arg canonicalizes
/// inside `fs.read` — the tool call runs. (Caller-supplied here: a fetch
/// source would complete the NEP-0002 trifecta with `fs.read` + mcp
/// egress — the static human-gate, a different law.)
#[tokio::test]
async fn untrusted_mcp_path_arg_inside_the_permit_runs() {
    let yaml = "nika: v1\nworkflow:\n  id: regate-mcp-ok\ninputs:\n  p: { type: string, default: \"datasets/report.csv\" }\npermits:\n  fs: { read: [\"datasets/**\"] }\n  tools: [\"mcp:fs/read\"]\ntasks:\n  read:\n    with: { p: \"${{ inputs.p }}\" }\n    invoke:\n      tool: \"mcp:fs/read\"\n      args: { path: \"${{ with.p }}\" }\n";
    let tools =
        Arc::new(MockToolExecutor::new().enqueue_ok(ToolResult::success("r1", "file contents")));
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::clone(&tools)));
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
    let report = check(&wf);
    assert!(report.is_clean(), "static ladder: {:?}", report.findings);
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
        RuntimeConfig::default(),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean run");
    assert!(
        outcome.ok,
        "the covered mcp arg runs: {:?}",
        outcome.records
    );
    assert_eq!(outcome.records["read"].status, TaskStatus::Success);
    assert_eq!(tools.captured_calls().len(), 1, "the mcp call dispatched");
}

/// A tainted `cwd:` re-gates as a path (ENGINE.md step 5 · « cwd idem »):
/// the payload climbing out of `src/**` is refused pre-spawn.
#[tokio::test]
async fn untrusted_cwd_escaping_is_refused() {
    let yaml = exec_workflow(
        "permits:\n  exec: [\"make\"]\n  fs: { read: [\"src/**\"] }\n  net: { http: [\"news.example\"] }\n  tools: [\"nika:fetch\"]",
        "  build:\n    after: { dl: success }\n    with: { dir: \"${{ tasks.dl.output }}\" }\n    exec: { command: [\"make\"], cwd: \"${{ with.dir }}\" }",
    );
    let (outcome, _, shell) = run_with_ingress(&yaml, "src/../../etc", MockShell::new()).await;
    assert!(!outcome.ok);
    let err = outcome.records["build"]
        .error
        .as_ref()
        .expect("the refusal");
    assert_eq!(err.code, "NIKA-SEC-004");
    assert!(
        err.message.contains("-> exec.cwd"),
        "the cwd sink speaks: {}",
        err.message
    );
    assert!(shell.executed_commands().is_empty());
}
