# nika-session — crate spec

| Field | Value |
|---|---|
| Status | **WIP → ADMISSION** (One Door · wave 4 · ADR-125). In `workspace.metadata.diamond.wip` until the 12 gates land (Gate 5 mutation and Gate 11 swarm owed). |
| Layer | **L4 — interface** (the human's terminal) · a host runtime over the installed engine · **sync** on the terminal, one current-thread runtime per inference · lateral `nika-session → nika-cli-host` for the ONE probe and the ONE oracle facade (the ADR-124 precedent). |
| Sub-tier | L4-surface — bare `nika` on an interactive terminal. The first run asks how Nika should think with the human (an AI app they already have · an API · a local engine · none · in that order, in human words) and keeps the answer at `~/.nika/session-intelligence.json`; the session observes the project once, answers Nika facts from the engine, hands the chosen intelligence a minimal typed bundle, and reads every reply through the hallucination guard. |
| Design | Seven modules, one law each: `identity` (the six laws + the language digest) · `snapshot` (the proven root · the project file · the ONE walker) · `intelligence` (the census · the persisted choice · the resolution that refuses, never replaces · the data locus) · `reasoner` (ONE inference over the seat, the provider registry, or none — never a temporary workflow) · `broker` (the bundle: named files inside the root, bounded, redacted, with provenance · the environment never injected) · `guard` (builtins · models · codes · MCP servers · verbs · fields · claimed ignorance, corrected under the reply) · `facts` (the workflows · the builtins · the providers · a verdict through the facade · a code through the ladder · a shape through the ONE router) · `runtime` (the loop). Owns nothing the engine owns. |
| LOC budget | ≤15k crate · ≤1500/file · ≤100/fn (Diamond caps) |
| IMPL | ~1900 LOC src (2026-09-03 live · `scripts/crate-metrics.sh nika-session`) |
| Crate version | tracks workspace · License `AGPL-3.0-or-later` · Edition 2024 · Publish `false` (Foundation crate · ADR-022) |
| ADRs | ADR-003 (12-gate admission) · **ADR-125 (the native session)** · ADR-124 (the oracle facade the facts read) · ADR-122 / ADR-123 (the access plan and the layered verdicts the verdict fact carries) |
| Error range | **none user-facing** — `ReasonError` is the reasoner's refusal (no intelligence · the seat · the provider · the runtime), spoken in the session as a refusal with its fix; the engine's own codes travel through the facts (`explain`) untouched. |
| Reference | the one-door pack 08 (the session runtime) · 09 (knowledge and grounding) · 13 (the first run) · 27 (the system contract) · 37 (the context firewall) · `crates/nika-cli/src/verbs/session.rs` (the door) · `crates/nika-cli/tests/session_pty.rs` (the door on a real terminal) |

---

## What it must NOT own

The workflow grammar · the builtin catalog · the model catalog · the error definitions · the check semantics · the runtime · the ARM semantics · the trace verification · the project file grammar. It queries those authorities (`nika_pack` · `nika_builtin` · `nika_catalog` · `nika_error` · `nika_cli_host::oracle` · `nika_dap::inventory` · `nika_vocab::project` · `nika_onboard::routing`).

## The tests that admit it

- a chat turn writes nothing (no temp workflow · no `.nika/` · no trace);
- the reasoner receives only the bundle (the identity core · the facts · the file the human named, redacted · never the environment · never an unnamed file);
- the pack's adversarial corpus (an invented builtin · model · code · MCP server · verb · field · a claim of ignorance) is corrected before the human sees it;
- an explicit intelligence this machine cannot serve is refused with its fix and never replaced;
- the facts answer without any model;
- the preference round-trips under the home and a corrupt file is « never chosen »;
- the first screen speaks the atelier order in human words, never a class name;
- on the real binary (`session_pty.rs`): a pipe is the concierge, the TTY is the session, the first run asks once, the kept choice never asks again, `nika thread` is the parser's own refusal.
