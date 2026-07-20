// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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
    /// The unified-finding gate (`TRIFECTA` · `PERMITS`).
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
