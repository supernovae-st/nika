// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The suite body — fixtures plane + assertion harness + the five
//! families as ordinary lib tests. The THREAT MODEL lives in the parent
//! module doc (`mod.rs`) — read it first.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use fixtures::{Containment, ExpectedStatus, Fixture, Verdict};

/// Execute one fixture under its declared verdict.
async fn execute(fx: &Fixture) {
    match fx.sidecar.verdict {
        Verdict::StaticDeny => harness::assert_static_deny(fx),
        Verdict::RuntimeBlock => harness::assert_runtime(fx).await,
        Verdict::Residual => {
            let residual = fx.sidecar.residual.as_ref().unwrap_or_else(|| {
                panic!(
                    "{}: verdict residual REQUIRES a residual block — a \
                     documented gap is a TODO with an owner, never a silent \
                     pass",
                    fx.id()
                )
            });
            assert!(
                !residual.summary.is_empty() && !residual.owner.is_empty(),
                "{}: the residual note names the gap AND its owner",
                fx.id()
            );
            harness::assert_runtime(fx).await;
        }
    }
}

/// Run every fixture of one family directory.
async fn run_family(family_dir: &str) {
    let mut ran = 0usize;
    for fx in fixtures::load_all() {
        if fx.family_dir == family_dir {
            execute(&fx).await;
            ran += 1;
        }
    }
    assert!(
        ran >= 3,
        "{family_dir}: each family carries at least 3 fixtures"
    );
}

#[tokio::test]
async fn f1_direct_injection() {
    run_family("f1-direct-injection").await;
}

#[tokio::test]
async fn f2_indirect_tool_output() {
    run_family("f2-indirect-tool-output").await;
}

#[tokio::test]
async fn f3_schema_parse_smuggling() {
    run_family("f3-schema-parse-smuggling").await;
}

#[tokio::test]
async fn f4_permit_escalation() {
    run_family("f4-permit-escalation").await;
}

#[tokio::test]
async fn f5_multi_hop_laundering() {
    run_family("f5-multi-hop-laundering").await;
}

/// The suite's own contract: sidecars are well-formed, verdicts pair with
/// their containment class, every family is covered, and the residual
/// registry never silently empties.
#[test]
fn suite_contract() {
    let all = fixtures::load_all();
    assert!(
        all.len() >= 15,
        "the suite covers at least 15 attack fixtures — found {}",
        all.len()
    );
    let mut per_family = [0usize; 5];
    let mut residuals = 0usize;
    for (i, (tag, dir)) in fixtures::FAMILIES.iter().enumerate() {
        for fx in &all {
            if fx.family_dir == *dir {
                per_family[i] += 1;
                check_sidecar(fx, tag);
            }
        }
        assert!(
            per_family[i] >= 3,
            "{dir}: at least 3 fixtures per family — found {}",
            per_family[i]
        );
    }
    for fx in &all {
        if fx.sidecar.verdict == Verdict::Residual {
            residuals += 1;
        }
    }
    assert!(
        residuals >= 1,
        "the residual registry is empty — if every gap closed, update the \
         suite doc's residual section deliberately"
    );
}

/// One sidecar's internal consistency (family tag, slug, verdict pairing).
fn check_sidecar(fx: &Fixture, want_family: &str) {
    let sc = &fx.sidecar;
    assert_eq!(
        sc.family,
        want_family,
        "{}: sidecar family matches its directory",
        fx.id()
    );
    assert!(
        !sc.title.is_empty() && !sc.rationale.is_empty(),
        "{}: title and rationale are non-empty",
        fx.id()
    );
    match sc.verdict {
        Verdict::StaticDeny => {
            assert!(
                matches!(
                    sc.containment,
                    Containment::TrifectaStatic
                        | Containment::PermitsStatic
                        | Containment::OrderStatic
                ),
                "{}: static-deny rides a static containment class",
                fx.id()
            );
            assert!(
                sc.static_expect.is_some() && sc.script.is_none() && sc.expect.is_none(),
                "{}: static-deny carries `static` and no runtime blocks",
                fx.id()
            );
        }
        Verdict::RuntimeBlock => {
            assert!(
                matches!(
                    sc.containment,
                    Containment::AgentWhitelist
                        | Containment::ExecPermitGate
                        | Containment::FsBoundary
                ),
                "{}: runtime-block rides a runtime containment class",
                fx.id()
            );
            let expect = sc
                .expect
                .as_ref()
                .unwrap_or_else(|| panic!("{}: runtime-block requires `expect`", fx.id()));
            assert!(
                expect.status != Some(ExpectedStatus::Success) && expect.code.is_some(),
                "{}: a block pins a failure code",
                fx.id()
            );
        }
        Verdict::Residual => {
            assert_eq!(
                sc.containment,
                Containment::None,
                "{}: a residual is containment: none (honesty — never fake a block)",
                fx.id()
            );
        }
    }
}

// ── the fixture plane (was fixtures.rs) ──────────────────────────────

mod fixtures {
    //! The fixture plane of the adversarial suite (see `mod.rs` for the threat
    //! model): on-disk attack workflows (`attack.nika.yaml`) plus their
    //! expected-verdict sidecars (`expected.json`), walked from
    //! `fixtures/adversarial/<family-dir>/<slug>/`.
    //!
    //! The sidecar is the CONTRACT. The runner enforces not just the expected
    //! outcome but the verdict's hygiene: a `runtime-block`/`residual` fixture
    //! must check CLEAN (if the static lane starts catching it, the fixture
    //! goes red and must be reclassified), and a `residual` fixture must carry
    //! a non-empty residual note with an owner — never a silent pass.

    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    use serde::Deserialize;

    /// The five attack families: (sidecar `family` tag, on-disk directory).
    /// Vocabulary is the suite's own — no pre-existing house taxonomy (the
    /// lethal-trifecta legs and the realized-flow wording come from NEP-0002 /
    /// `nika_cap::trifecta`, the honesty forms from ADR-095).
    pub(crate) const FAMILIES: [(&str, &str); 5] = [
        ("F1", "f1-direct-injection"),
        ("F2", "f2-indirect-tool-output"),
        ("F3", "f3-schema-parse-smuggling"),
        ("F4", "f4-permit-escalation"),
        ("F5", "f5-multi-hop-laundering"),
    ];

    /// One fixture: the attack workflow source plus its parsed sidecar.
    pub(crate) struct Fixture {
        /// The family directory name (e.g. `f2-indirect-tool-output`).
        pub(crate) family_dir: &'static str,
        /// The sidecar contract.
        pub(crate) sidecar: Sidecar,
        /// The `attack.nika.yaml` source.
        pub(crate) yaml: String,
    }

    impl Fixture {
        /// `<family-dir>/<slug>` — the stable id failure messages lead with.
        pub(crate) fn id(&self) -> String {
            format!("{}/{}", self.family_dir, self.sidecar.slug)
        }
    }

    /// The expected-verdict sidecar (`expected.json`).
    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct Sidecar {
        /// `F1`..`F5` — must match the on-disk family directory.
        pub(crate) family: String,
        /// The fixture slug — must match the on-disk directory name.
        pub(crate) slug: String,
        /// One line: what the attack does.
        pub(crate) title: String,
        /// One line: WHY the asserted layer is the right one.
        pub(crate) rationale: String,
        /// The expected outcome class.
        pub(crate) verdict: Verdict,
        /// The machinery the verdict rides (informational + contract-checked).
        pub(crate) containment: Containment,
        /// `static-deny` only: the finding the static lane must emit.
        #[serde(rename = "static")]
        pub(crate) static_expect: Option<StaticExpect>,
        /// Runtime fixtures only: how to script the mocks.
        pub(crate) script: Option<Script>,
        /// Runtime fixtures only: the expected run outcome + pinned counts.
        pub(crate) expect: Option<RunExpect>,
        /// `residual` only (REQUIRED there): the documented gap + its owner.
        pub(crate) residual: Option<Residual>,
    }

    /// The expected outcome class of an attack fixture.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
    #[serde(rename_all = "kebab-case")]
    pub(crate) enum Verdict {
        /// `nika check` refuses the workflow (the security lane, not a
        /// conformance break — the runner asserts `conformance` is empty).
        StaticDeny,
        /// The workflow checks clean; the runtime boundary refuses the
        /// injected action (task failure with the expected code).
        RuntimeBlock,
        /// DOCUMENTED GAP: the engine does NOT stop this today. The fixture
        /// pins the current (unblocked) behavior so the day the gap closes
        /// the test goes red and the fixture gets reclassified.
        Residual,
    }

    /// The containment machinery a verdict rides.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
    #[serde(rename_all = "kebab-case")]
    pub(crate) enum Containment {
        /// The lethal-trifecta realized-flow judge (`NIKA-SEC-009`).
        TrifectaStatic,
        /// The unconditional order law (`NIKA-SEC-015`) — an `exec:`
        /// downstream of a net-effecting task. It reaches files the
        /// trifecta cannot: that judge's first leg wants a non-empty
        /// `permits.fs.read`, and this one wants nothing at all.
        OrderStatic,
        /// The static capability-escape scan (`NIKA-SEC-004`/`005`).
        PermitsStatic,
        /// The agent loop's tool whitelist (`NIKA-SEC-002`).
        AgentWhitelist,
        /// The runtime exec permit gate (`NIKA-SEC-004`, pre-spawn).
        ExecPermitGate,
        /// The builtin fs confinement (`NIKA-SEC-004`, canonicalize-then-confine).
        FsBoundary,
        /// Nothing contains it today (residual fixtures only).
        None,
    }

    /// The finding a `static-deny` fixture must produce.
    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct StaticExpect {
        /// The unified-finding gate (`TRIFECTA` · `PERMITS` · `ORDER`).
        pub(crate) gate: String,
        /// The wire code (`NIKA-SEC-009` · `NIKA-SEC-004`).
        pub(crate) code: String,
        /// The task the finding must name (the realized sink / the escape).
        pub(crate) task: Option<String>,
    }

    /// How to script the mock plane for a runtime fixture.
    #[derive(Debug, Default, Deserialize)]
    #[serde(deny_unknown_fields, default)]
    pub(crate) struct Script {
        /// Wire the REAL `BuiltinDispatcher` over mock fs/http seams (the fs
        /// boundary then genuinely confines) instead of a `MockToolExecutor`.
        pub(crate) real_tools: bool,
        /// Files to seed into the `MockFs` (`real_tools` only).
        pub(crate) fs: BTreeMap<String, String>,
        /// FIFO http responses for the `MockHttp` (`real_tools` only).
        pub(crate) http: Vec<HttpStep>,
        /// FIFO model turns for the `MockProvider` (the scripted hijack).
        pub(crate) provider: Vec<ProviderStep>,
        /// FIFO tool results for the `MockToolExecutor` (mock plane only).
        pub(crate) tools: Vec<ToolStep>,
        /// FIFO shell results for the `MockShell`.
        pub(crate) shell: Vec<ShellStep>,
    }

    /// One canned http response.
    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct HttpStep {
        pub(crate) status: u16,
        pub(crate) body: String,
    }

    /// One scripted model turn.
    #[derive(Debug, Deserialize)]
    #[serde(untagged)]
    pub(crate) enum ProviderStep {
        /// The model ends its turn with a text message.
        Text { text: String },
        /// The model calls a tool (the hijacked action).
        ToolUse { tool_use: ToolUseStep },
    }

    /// The tool call the (hijacked) model emits.
    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct ToolUseStep {
        pub(crate) name: String,
        pub(crate) input: serde_json::Value,
    }

    /// One canned tool result (mock plane).
    #[derive(Debug, Deserialize)]
    #[serde(untagged)]
    pub(crate) enum ToolStep {
        Ok { ok: String },
        Err { err: String },
    }

    /// One canned shell result.
    #[derive(Debug, Deserialize)]
    #[serde(untagged)]
    pub(crate) enum ShellStep {
        Ok { ok: String },
        Fail { fail: ShellFail },
    }

    /// A failing shell result.
    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct ShellFail {
        pub(crate) status: i32,
        pub(crate) stderr: String,
    }

    /// The expected run outcome plus the pinned side-effect counts.
    #[derive(Debug, Default, Deserialize)]
    #[serde(deny_unknown_fields, default)]
    pub(crate) struct RunExpect {
        /// The task whose record carries the verdict (default: the run must
        /// be green — used only implicitly; every fixture names its task).
        pub(crate) task: Option<String>,
        /// Expected terminal status of `task` (default `failure`).
        pub(crate) status: Option<ExpectedStatus>,
        /// Expected `error.code` when `status` is `failure`.
        pub(crate) code: Option<String>,
        /// Exact number of model calls (proves a refusal was never fed back).
        pub(crate) provider_calls: Option<usize>,
        /// Exact number of tool dispatches (mock plane only).
        pub(crate) tool_calls: Option<usize>,
        /// Exact number of shell executions (0 proves pre-spawn refusal).
        pub(crate) shell_calls: Option<usize>,
        /// Exact number of http requests (real-tools plane only).
        pub(crate) http_calls: Option<usize>,
        /// Assert the mock fs received no writes (real-tools plane only).
        pub(crate) fs_untouched: bool,
        /// Substring that must appear in some sent http request URL.
        pub(crate) http_url_contains: Option<String>,
        /// Substring that must appear in the executed shell command.
        pub(crate) shell_argv_contains: Option<String>,
        /// Substring that must appear in a captured provider request (proves
        /// the injection really rode the expected channel).
        pub(crate) request_contains: Option<RequestContains>,
    }

    /// The expected terminal status.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
    #[serde(rename_all = "lowercase")]
    pub(crate) enum ExpectedStatus {
        Success,
        Failure,
    }

    /// A needle that must appear in the `index`-th captured provider request.
    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct RequestContains {
        pub(crate) index: usize,
        pub(crate) needle: String,
    }

    /// The documented residual risk (mandatory for `residual` fixtures).
    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct Residual {
        /// What the engine does NOT stop, stated plainly.
        pub(crate) summary: String,
        /// Where the fix is tracked (ADR / shadow zone / issue).
        pub(crate) owner: String,
    }

    /// The suite root: `crates/nika-runtime/fixtures/adversarial`.
    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/adversarial")
    }

    /// Read every fixture under every family directory, sorted for
    /// deterministic failure output.
    pub(crate) fn load_all() -> Vec<Fixture> {
        let mut out = Vec::new();
        for (_, dir) in FAMILIES {
            let fam = root().join(dir);
            let mut entries: Vec<PathBuf> = std::fs::read_dir(&fam) // seam-bypass-ok: test-only walk of the suite's own fixture tree — the engine under test never touches fs
            .unwrap_or_else(|e| panic!("fixture family dir {} reads: {e}", fam.display()))
            .map(|e| {
                e.unwrap_or_else(|err| panic!("fixture dir entry under {}: {err}", fam.display()))
                    .path()
            })
            .filter(|p| p.is_dir())
            .collect();
            entries.sort();
            for fixture_dir in entries {
                out.push(load(dir, &fixture_dir));
            }
        }
        out
    }

    /// Load one fixture directory (`attack.nika.yaml` + `expected.json`).
    fn load(family_dir: &'static str, dir: &Path) -> Fixture {
        let yaml =
        std::fs::read_to_string(dir.join("attack.nika.yaml")) // seam-bypass-ok: test-only fixture read from the crate dir — no engine I/O
            .unwrap_or_else(|e| panic!("{}: attack.nika.yaml reads: {e}", dir.display()));
        let raw = std::fs::read_to_string(dir.join("expected.json")) // seam-bypass-ok: test-only fixture read from the crate dir — no engine I/O
        .unwrap_or_else(|e| panic!("{}: expected.json reads: {e}", dir.display()));
        let sidecar: Sidecar = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("{}: expected.json parses: {e}", dir.display()));
        Fixture {
            family_dir,
            sidecar,
            yaml,
        }
    }
}

// ── the assertion harness (was harness.rs) ───────────────────────────

mod harness {
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
        let want = fx.sidecar.static_expect.as_ref().unwrap_or_else(|| {
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
        let script =
            fx.sidecar.script.as_ref().unwrap_or_else(|| {
                panic!("{}: a runtime verdict requires a `script` block", fx.id())
            });
        let expect =
            fx.sidecar.expect.as_ref().unwrap_or_else(|| {
                panic!("{}: a runtime verdict requires an `expect` block", fx.id())
            });
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
}
