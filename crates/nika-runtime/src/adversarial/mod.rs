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

#[cfg(test)]
mod tests;
