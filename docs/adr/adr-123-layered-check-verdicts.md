---
id: ADR-123
title: "check answers four questions: VALID · ACCESS READY · CAPACITY FIT · RUN READY, and run refuses what check refuses"
status: accepted
date: "2026-09-03"
phase: "pre-1.0 · one door"
deciders: ["@ThibautMelen"]
tags: ["architecture", "check", "access", "capacity", "verdicts", "one-door"]
affects_crates: ["nika-check-analyzer", "nika-check", "nika-display", "nika-cli", "nika-cli-host", "nika-service-execution", "nika-mcp", "nika-dap"]
affects_layers: ["L0", "L3", "L4"]
supersedes: []
superseded_by: []
related: ["ADR-122", "ADR-003"]
requires: ["ADR-122"]
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: ["NIKA-1800", "NIKA-1801", "NIKA-1807"]
timeline: "v0.118"
follow_ups: ["a failed task terminal carries its lane", "a run exit-code table and a code for the resume access refusal", "error on every failed run_settled"]
---

# ADR-123: check answers four questions, and run refuses what check refuses

## Context

A green `nika check` was read as « this will run ». It could not be: the
ladder judged the DEFINITION (grammar · DAG · permits · types · the
resolver's knowledge of the model) and said nothing about THIS machine
(is a path ready?) nor about the seat against the declaration (can it
emit that many tokens?). Two measurements forced the decision:

- The W1 persona gauntlet (2026-09-03) read `✔ MODELS … key presence on
  this machine not judged` as readiness, met `NIKA-INFER-001` on a dead
  key ninety seconds later, and counted three JSON shapes for the same
  access decision (`check --json` `access_plan[].chosen` · `run --dry-run
  --json` `access.plans[].class` · the boot manifest's JSON-encoded map).
- The same rig showed `check` refusing a reasoning seat under
  `max_tokens: 32` (exit 2) while `run` admitted and executed the same
  file: the run's clean gate folded the ladder's findings and skipped the
  MODELS rung's judgments.

The one-door pack's product law: `check` answers four different
questions and never collapses them into one checkmark, and `nika check`
must not spend tokens to prove readiness.

## Decision

1. **Four layered verdicts, computed once beside the exit code.**
   `VerdictLayers { valid, access_ready, capacity_fit, blockers }` with
   `run_ready()` derived. VALID = the ladder + resolution + skills.
   ACCESS READY = every static lane admitted by the frozen
   `ExecutionAccessPlan` (ADR-122) this machine resolves — presence only,
   never a dial; `None` when no static model exists. CAPACITY FIT = the
   thinking laws + the new CAPACITY laws. RUN READY = the three plus any
   known blocker. The text render prints an ACCESS rung under MODELS and
   a `layers ·` line after the audited line; `--json` adds a `verdicts`
   object. `clean` keeps meaning VALID + CAPACITY FIT (what it always
   folded): `REPORT_VERSION` stays 1, the exit codes stay closed, and RUN
   READY false is a `--profile operational` outcome (exit 2), never the
   default.
2. **CAPACITY laws from the catalog's positive knowledge only**
   (`catalog_knows` · the mock never judged): `infer.max_tokens` above
   the seat's max output; `schema:` on a seat the catalog marks without a
   JSON mode; `agent.max_tokens_total` above the context window; `vision:`
   on a seat whose input modalities exclude images. They ride the MODELS
   rung's findings rail on the CLI, the MCP oracle and the run gate.
3. **`run` refuses what `check` refuses.** The run's clean gate folds
   the MODELS rung's judgments (resolution · thinking · capacity), judged
   on the effective model (`--model` applied) exactly like `check`.
4. **One lane-row shape.** `nika_service_execution::access::lane_rows`
   renders `model · provider · resolved · access · chosen · billing ·
   pinned · rejected[]` and every machine surface carries those rows:
   `check --json` `access_plan`, `run --dry-run --json` `access.plans`,
   the boot manifest's `access_plan` (an array now; the resume reader
   still folds the 0.117 map). The text dry-run always prints the plan
   plus one access line per lane.
5. **`check --access <pin>`** judges the plan under the pin `run
   --access` takes. **Presence is worded as presence**: `doctor` says
   `key present · not validated`, the ACCESS rung says `not validated
   (check never dials)`.

## Consequences

- A file that is legal but cannot run here is no longer green on the
  operational profile, and the human surface says which question failed.
- The thinking-budget hint no longer blames a task pinned to the mock
  (the capability defaults read the mock as reasoning-capable; the hint
  now asks the catalog first).
- Proof: `crates/nika-cli/tests/check_run_layers_e2e.rs` on the real
  binary (the reasoning floor red on both doors · capacity red on both
  doors · one lane shape on three surfaces · `check --access` pins like
  `run` · the layers line), the analyzer's capacity laws, the display's
  render tests, the service driver's row test.

## Follow-ups

- Delivered in wave 2b: a failed task terminal carries its lane and a
  note naming the model; `run_settled` carries `error` on every failed
  frame (launch refusals included) and the `access_plan` rows of the
  lanes that served; `run --help` ends on its exit ladder; the resume
  access refusal is `NIKA-1807`; « wrote .nika/traces » prints only
  when a trace exists; the MODELS rung's judges (`verdict_layers` ·
  capacity · resolution · the boot access stamps) are hosted in
  `nika_cli_host::models_rung`, the door the MCP oracle reaches (wave 3).
- The run-time ACCESS READY probe (a non-billable credential check) and
  the RUN READY preflight at the runtime's admission belt stay open.
