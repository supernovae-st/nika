// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! # The deterministic adversarial suite (B8)
//!
//! Attack fixtures for prompt-injection-class threats, executed against the
//! REAL engine with a MOCKED model hijack — wired into the merge gate as
//! ordinary lib tests (`cargo test --workspace --lib`).
//!
//! ## Threat model
//!
//! The workflow YAML is the *trusted control plane*; everything it pulls in
//! (fetched pages, tool results, jq-extracted fields) is *untrusted data*
//! (ADR-095's control/data-plane split). The attacker authors content that
//! reaches the model and instructs it to exfiltrate private data, widen the
//! declared `permits:` boundary, or launder the payload through hops until
//! the egress looks innocent. The hijack is SCRIPTED, not hoped for: each
//! fixture's sidecar scripts the `MockProvider` to emit exactly the
//! malicious action a compromised model would emit the moment the untrusted
//! content reaches it. What the suite then asserts is the part that must
//! hold no matter how convincing the injection is — the boundary:
//!
//! - the **static lanes** (`nika check`): the lethal-trifecta realized-flow
//!   judge (`NIKA-SEC-009` — private data ∧ untrusted ingress ∧ egress,
//!   ungated by a blocking `nika:prompt`) and the capability-escape scan
//!   (`NIKA-SEC-004`/`005`, gate `PERMITS`);
//! - the **runtime lanes**: the agent tool whitelist (`NIKA-SEC-002`,
//!   never fed back to the model), the exec permit gate (`NIKA-SEC-004`,
//!   pre-spawn), the F-O1 permit-parameterization re-gate (`NIKA-SEC-004`,
//!   a tainted argv/mcp argument matched on its resolved, canonical form
//!   against the step's permit), and the real builtin fs confinement
//!   (`NIKA-SEC-004`, canonicalize-then-confine) — the last wired through
//!   the true `BuiltinDispatcher` over mock fs/http seams, so the refusal
//!   is the production code path with zero I/O.
//!
//! ## The five families
//!
//! - **F1 — direct injection**: untrusted fetched content instructs the
//!   model to exfiltrate; the mock obeys and the boundary must stop the
//!   call. Proves the prompt channel alone cannot cross the boundary.
//! - **F2 — indirect via tool output**: the instruction rides a tool
//!   RESULT inside the agent loop. Proves the loop treats tool output as
//!   data (whitelist refuses the injected call; a boundary refusal
//!   hard-stops the loop and is never fed back — `provider_calls` pins it).
//! - **F3 — schema/parse smuggling**: the instruction hides in structured
//!   fields the workflow passes through jq/template hops toward an egress
//!   sink. Proves the parse hop does not launder the taint: the static
//!   realized-flow judge sees through it, and the runtime fs confinement
//!   resolves smuggled paths at effect time.
//! - **F4 — permit escalation**: the hijacked model (or the laundered
//!   content) tries to widen exec/fs/net beyond the declared boundary.
//!   Proves the permit checks fail closed — statically for literal escapes,
//!   pre-spawn/pre-dispatch at runtime for rendered ones.
//! - **F5 — multi-hop laundering**: untrusted content crosses ≥2 tasks
//!   (infer rewrites, fan-out/fan-in) before egress. Proves hop count is
//!   not a defense: the realized-flow judge names the original source at
//!   the sink, and the runtime whitelist is hop-count-blind.
//!
//! ## Verdict classes and the honesty rule
//!
//! Each fixture is an `attack.nika.yaml` plus an `expected.json` sidecar
//! declaring one of three verdicts:
//!
//! - `static-deny` — `nika check` refuses (conformance must be empty: the
//!   deny provably rides the security lane);
//! - `runtime-block` — the workflow checks CLEAN and the runtime boundary
//!   refuses the injected action (a fixture the static lane starts
//!   catching goes red and must be reclassified — the lanes cannot both
//!   sleep);
//! - `residual` — **the engine does not stop this today.** The fixture
//!   pins the current unblocked behavior and MUST carry a `residual`
//!   block (`summary` + `owner`). A residual is a TODO with an owner,
//!   never a silent pass: the day the gap closes, the pinned behavior
//!   changes, the test goes red, and the fixture is reclassified to the
//!   lane that started catching it.
//!
//! Current residuals (kept honest by `suite_contract`):
//!
//! - `f1-03-in-boundary-egress-args` — model-chosen arguments ride INSIDE
//!   the declared net boundary: the F-O1 PR-2 re-gate (NEP-0004 law 2)
//!   matches resolved values against the step's permit at the task-level
//!   seams, but this egress is a model-chosen arg inside the agent loop
//!   whose host the permit COVERS — a re-gate matches, it cannot judge
//!   (owner: F-O10 / the confidentiality axis · the trifecta lane).
//!
//! Closed by F-O1 PR-2 (reclassified `runtime-block`):
//!
//! - `f3-03-allowlisted-program-tainted-argv` — the exec allowlist gated
//!   the program, never the argv; the re-gate now labels the tainted slot
//!   and matches its RESOLVED value against the step's permit
//!   (NIKA-SEC-004, pre-spawn).
//!
//! ## Determinism guarantee
//!
//! No real LLM, no network, no filesystem, no API keys, no wall clock, no
//! randomness: the model is `MockProvider` (FIFO script), tools are
//! `MockToolExecutor` or the real `BuiltinDispatcher` over
//! `MockFs`/`MockHttp`, shell is `MockShell`, time is `MockClock`, ids are
//! sequential, and the runtime's wave schedule is deterministic. Every
//! assertion is on outcomes and exact counts, never on timing. A fixture
//! that flakes is a bug in the engine or the suite — never tolerated.
//!
//! **Real-LLM evals must NEVER gate merges.** Injection success rates of
//! live models are eval territory (run out-of-band, tracked separately);
//! this suite pins the deterministic guarantees the engine makes
//! regardless of which model is plugged in.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod fixtures;
mod harness;

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
                    Containment::TrifectaStatic | Containment::PermitsStatic
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
