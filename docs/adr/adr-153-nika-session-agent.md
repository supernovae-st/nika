---
id: ADR-153
title: "nika-session-agent: the conversation the Session's selected intelligence leads"
status: accepted
date: "2026-10-09"
phase: "pre-1.0 · session architecture"
deciders: ["@ThibautMelen"]
tags: ["architecture", "crates", "session", "agent", "conversation"]
affects_crates: ["nika-session-agent", "nika-session", "nika-session-change"]
affects_layers: ["L4"]
supersedes: []
superseded_by: []
related: ["ADR-125", "ADR-133", "ADR-144", "ADR-148"]
requires: ["ADR-144"]
enables: []
amends: []
fci: []
inv: ["INV-019", "INV-025", "INV-027"]
shadow_zones: []
nika_codes: []
timeline: "v0.123"
follow_ups: ["done (2d): the Session serves its tools through `nika_session_change::tools::SessionTools` and drives this loop for an API or local route; `NIKA_SESSION_DRIVER=rounds` keeps the round driver during the transition; an ACP agent leading over MCP remains to wire", "done (2d): the Session's history owner keeps the tree beside its journal, keyed by the project like the history itself, so no project pointer is written", "admission with the nika-session unit: mutation floor, canary through the Session, three-perspective review"]
---

# ADR-153: nika-session-agent, the conversation the Session's selected intelligence leads

## Context

The native Session authors `.nika` workflows with a person. Until now each line went through a
fixed pipeline: a classifier routed the line, bounded prompts read it, one compile call made a
candidate, and an authoring round asked one question at a time. The person could not steer a
turn under way, a question the author needed could not wait for the answer inside the same
reasoning, and the conversation kept a fixed window of recent turns.

The Session is now led by the selected intelligence itself: the model reads the conversation,
calls the Session's tools (read, check, write or edit the candidate, ask the person, propose a
revision), reads their replies and goes on until it answers. The same conversation must reach
every host (the terminal, the machine door, Serve, the SDK) and every intelligence: an API or
local route through Nika's own loop, an ACP agent through its own loop and Nika's tools over MCP.

Authority does not move: the Session's tools act on the Session, and only the Session's doors
save, run or record a consent; only the person's own lines authorize. The loop that carries the
calls has no state of its own beyond the conversation, and `nika-session` sits at its size
budget.

## Decision

A new member of the `nika-session` unit, `nika-session-agent` (L4, WIP with its unit, the
ADR-144 pattern), owns the conversation and nothing else:

- `run` — the loop over one `Model` (the selected intelligence, asked one request at a time)
  and the Session's `SessionTools`. It runs until the model answers without a call. No step,
  turn, token or time quota ends a run: Stop (the Session's `CancelCtx`), a call whose reply
  ends the turn, or a failure does. A call that waits for the person (`ask`) parks the run; the
  person's next line answers that call, and the run goes on. A call that repeats the previous
  one exactly, reply included, is said so to the model, never stopped.
- `steer` — the person's lines while a run is under way: a steering line enters after the
  current calls, the calls not yet run are skipped so the model reads it first; a follow-up
  line enters when the model would end; Stop returns both unsent.
- `tree` — the Session tree: every entry with its parent, one line per entry, each line bound
  to the line before it by a digest. The branch from the root to the leaf is what a model
  reads; the person's lines carry a citation (`u1`, `u2`, …) the model uses to name their
  words, and the Session resolves a citation against the tree. Model messages, tool replies,
  summaries and Session facts never authorize anything.
- `compact` — when the route's real context window requires it, earlier entries are folded
  into a summary the model writes (the person's words, decisions and the values' provenance
  kept), the recent entries stay verbatim, and every entry stays in the tree.
- `event` — what a run tells a host while it happens (`nika/session-events@0`).

The tool contract (`nika/author-tools@0`) is owned by `nika-session-change::tools`, the Session's
portable host contract: Nika's loop and the MCP server that serves the same tools to an ACP agent
call the one trait the Session implements. Edges: `nika-session → nika-session-agent →
nika-session-change`, never back; production dependencies are the kernel's provider types and
cancel context, the tool contract, `serde`, `serde_json`, `thiserror` and `blake3`. The crate
performs no I/O: the Session's history owner implements `Store` (append and sync one line) and
reads a tree back with `Tree::replay`.

## Consequences

- One conversation shape for every host and intelligence; the hosts render one event stream.
- The loop is testable without a provider: a scripted `Model` and `SessionTools` exercise
  parking, steering, follow-ups, Stop, compaction and the tree's integrity.
- A damaged tree is refused whole with the line it fails at; nothing is reset, as for the
  Session's history journal (ADR-133).
- The crate joins the workspace as WIP with its unit and is admitted with it; its spec records
  the gate evidence as it lands.

## Alternatives considered

- **Inside `nika-session`.** Rejected: the Session is at its size budget, and the loop is
  independent of the Session's state and authority; mixing them would let orchestration reach
  authority.
- **Reuse the `agent:` verb's loop (`nika-verb-agent`).** Rejected: it runs a workflow task
  under that task's budgets (turn and token limits are failures there), a completion sentinel
  and a tool whitelist over workflow tools; a conversation with a person has none of these, and
  the verb's contract must not bend to it. Both reuse the kernel's provider types.
- **Inside `nika-harness`.** Rejected: API and local routes are not harnesses; an ACP agent
  keeps its own loop and reaches the same tools over MCP.

## Evidence

- `crates/nika-session-agent/` — the member, its unit and property tests.
- `crates/nika-session-change/src/tools.rs` — the tool contract the loop calls.
- `docs/crate-specs/nika-session-agent.md` — the spec and its gate evidence.
