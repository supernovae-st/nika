---
id: ADR-096
title: "The agent-loop intelligence layer — routing, stall guard, compose, telemetry"
status: accepted
date: 2026-06-12
phase: ""
deciders: ["@ThibautMelen"]
tags: [nika-verb-agent, agent-loop, react, stall-guard, tool-routing, sota-2040]
affects_crates: [nika-verb-agent, nika-event]
affects_layers: [L2, L0]
supersedes: []
superseded_by: []
related: ["ADR-092", "ADR-097", "ADR-105"]
requires: []
---

# ADR-096 — The agent-loop intelligence layer

- **Status**: Accepted (2026-06-12)
- **Layer**: L2 (`nika-verb-agent`) + L0 vocabulary (`nika-event`)
- **Relates**: ADR-092 (static-analysis ladder · the compose gate's checker),
  spec `02-verbs.md` §agent (the YAML contract — UNTOUCHED by this ADR)

## Context

The s12 `agent` verb shipped the canonical ReAct loop: infer → whitelisted
dispatch → feed back, bounded by turns/tokens. Production agent traces across
the field show four recurring gaps, each with published evidence:

1. **No-progress loops** — repetitive-action cycles are a dominant agentic
   failure class (TRAIL, arXiv:2505.08638); a turn budget alone burns the
   whole budget before stopping.
2. **Tool-context crowding** — injecting every tool schema every turn
   degrades context budget and selection quality at scale (MCP-Zero,
   arXiv:2506.01056; Gorilla, arXiv:2305.15334).
3. **Self-composition without verification** — agents that generate
   artifacts (here: workflows) need «generation is not permission»
   discipline (PCE, arXiv:2605.24462; CodeAct, arXiv:2402.01030).
4. **Opaque internals** — observability must expose the agent's DECISIONS,
   not just its I/O (AgentOps, arXiv:2411.05285).

## Decision

Four orthogonal, deterministic mechanisms inside the loop — **zero new YAML**
(the spec's `agent:` field table is untouched; tuning is engine-internal
`AgentConfig`, embedder-set with production defaults):

### 1 · Stall guard (`guard.rs` · NIKA-467)

Turn signatures hash the turn's tool calls (key-order-canonical args) AND
their observations. Windowed max-repeats cycle scan (W=16 · ≤W²/4 u64
compares/turn). Ladder: `nudge_after` (3) occurrences → ONE Reflexion-style
corrective message in the transcript (arXiv:2303.11366 · bounded by
`max_reflections`, default 1; an all-error streak shares the budget);
`stall_after` (5) → stop as `NIKA-467 Stalled {period, repeats}`.

Because observations are part of the signature, **legitimate polling (same
call, changing result) never trips it** — only byte-identical
action+outcome cycles do. Stalls are terminal verdicts (`is_transient() =
false`): a blind rerun replays the proof.

### 2 · Per-turn tool routing (`router.rs`)

Below 24 whitelisted definitions: passthrough (stable prompts win —
provider prompt-caches stay warm). At ≥24: rank definitions against the
live context (prompt + last assistant text + last observations) with
**`nika-bm25`** (L2→L1 downward dep — the engine's own BM25S/eager
satellite; sovereign, zero LLM calls, bit-reproducible) and offer top-12
∪ pinned (intrinsics + `nika:done`) ∪ recently-used (≤2 turns).
**Fail-open**: zero lexical overlap ships the full universe — a blind
model is worse than a crowded one.

### 3 · Intrinsics (`intrinsic.rs` · `nika:compose`)

> **Amendment 2026-06-13 · `nika:compose`, not `agent:compose`.** The
> first cut put this in an `agent:` tool namespace — but the spec's tool
> namespace set is CLOSED at `{nika:, mcp:}` (`02-verbs` §218, enforced in
> the prose + the JSON schema + `validate_tool_ref`), so a third namespace
> was a grammar violation that only "worked" through a gap in the agent
> whitelist's namespace check. The precedent `nika:done` already shows the
> right shape: a loop-served tool is a `nika:` builtin, marked loop-only.
> So `compose` is now `nika:compose` — the 23rd canonical builtin
> (Introspection, the static sibling of `inspect`), catalogued in
> `nika-catalog`, rejected standalone in `nika-builtin` (NIKA-BUILTIN-
> COMPOSE-001), documented in the public spec. The whitelist gap is
> closed (the parser now enforces the closed set). `ToolSource` collapses
> to `{Builtin, Mcp, Other}` (the spec's set + a catch-all); the `Skill`/
> `Intrinsic` variants — which anticipated namespaces the spec doesn't
> have — are removed, re-added when those namespaces actually land.

`nika:compose` is a loop-served `nika:` builtin (like `nika:done`):
synthesized by the loop (any upstream def of the same name is dropped —
poison-shadow proof), whitelist-gated, NEVER dispatched to the executor
seam. It takes a full workflow YAML draft and returns the complete
`nika check` verdict as JSON — conformance violations with prescriptive
codes, secret flows, permits escapes, and the AARA cost/termination
**certificate** (ADR-092 · via `nika-schema`, L2→L0). The draft never
executes here: composition yields an artifact + its certificate; running
it stays a separate, gated decision. Drafts are size-capped (256 KiB)
ahead of the parser.

### 4 · Telemetry (`observe.rs` + 5 `nika-event` kinds)

An injected `AgentObserver` seam (4th seam · optional · `Arc<dyn>` — a
generic would infect every embedder signature for a non-data-path
concern). Every decision is a typed `AgentEvent`: `RunStarted ·
TurnStarted · ToolsSelected{offered, universe, by_source} ·
ToolCompleted · ComposeChecked · Nudged · Stalled{period, repeats} ·
BudgetCheckpoint · Finished`. The L3 runtime maps these 1:1 onto the new
`nika-event` kinds (`agent_tools_selected · agent_nudge · agent_stalled ·
agent_compose_checked · agent_budget_checkpoint`, `EventClass::Agent`) —
INV-024 holds: the runtime adapter is the ONE emission site.

## The `skill:` forward direction (NOT a v0.1 namespace)

> **Amended 2026-06-13.** `ToolSource` classifies ONLY the spec's closed
> v1 set (`nika:` · `mcp:`) + a catch-all. Stored-workflow skills
> (Voyager-style · arXiv:2305.16291; AWM · arXiv:2409.07429) are a real
> FUTURE direction, but `skill:` is not a v0.1 tool namespace (the spec's
> set is closed; the parser rejects it). When `skill:` lands — a future
> additive MINOR per `02-verbs` §218 — its `ToolSource` variant + the
> parser acceptance + the served definitions land together. The earlier
> "routed and counted today" framing was the same ahead-of-contract
> mistake `agent:compose` was, now corrected.

## Alternatives rejected

- **Tree search (LATS, arXiv:2310.04406)** — multiplies provider spend
  against the budget-first posture; revisit post-v0.81 behind the same
  observer seam.
- **Result caching for repeated identical calls** — breaks legitimate
  polling semantics (a cached poll lies); the observation-aware stall
  guard covers the pathological case without faking results.
- **LLM-based reflection/self-evaluation** — costs a provider call to
  decide whether to spend provider calls; the deterministic detector is
  free and evidence-carrying.
- **New YAML fields** — the spec is a public contract with its own
  4-surface cascade; everything here has sane engine defaults and zero
  author-facing surface.

## Verification

`tests/research_conformance.rs` — one executable property per claim,
through the REAL loop with mock seams: the nudge lands in the transcript
exactly once and the loop recovers · identical cycles stall as NIKA-467
with evidence and the queued 6th turn is never requested · polling with
changing observations never trips the guard · routing measurably narrows
the request's tool list (telemetry mirrors the request) · zero-overlap
fails open · the compose check-repair loop converges in 2 rounds with the
certificate riding the verdict and ZERO executor dispatches · a poisoned
upstream `agent:compose` def never shadows the loop-owned one · the
observer sequence is bracketed (RunStarted … Finished) with per-turn
routing + budget events. Unit property tests pin the detector (injected
p-cycles detected; unique streams never fire) and the canonical hash
(key-order invariance · observation sensitivity).
