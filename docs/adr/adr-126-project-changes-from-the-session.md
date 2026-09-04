---
id: ADR-126
title: "project changes from the session: one typed change set, preview == apply, consent outside the model"
status: accepted
date: "2026-09-03"
phase: "pre-1.0 · one door"
deciders: ["@ThibautMelen"]
tags: ["architecture", "session", "authoring", "consent", "one-door"]
affects_crates: ["nika-session", "nika-cli"]
affects_layers: ["L4"]
supersedes: []
superseded_by: []
related: ["ADR-124", "ADR-125", "ADR-113", "ADR-132"]
requires: ["ADR-125"]
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.118"
follow_ups: ["a bounded automatic repair loop (today the human asks « fix it » and consents to the witnessed update)", "deletion, renames and integration wiring as change classes when one needs them", "the same change object rendered by VS Code as a diff and approval"]
---

# ADR-126 · Project changes from the session: one typed change set, preview == apply, consent outside the model

## Context

The session (ADR-125) reasons and answers; it does not yet change the project. A conversation that ends in « here is your workflow » as prose the human must paste is the plugin's shape, not ours: the human then runs a check the session never saw, on bytes the session never wrote. The pack's law is exact: a durable project mutation is a typed change consumed by BOTH the preview and the apply, so the two cannot diverge; the preview derives its effects from the engine's own audit of the exact bytes; the human's consent is a trusted session event the model can neither issue nor see as a tool; every workflow mutation is followed by the real check, automatically; a « create and run once » is one consent covering apply → check → run, and findings stop the run.

## Decision

### One typed change set

```
ProjectChangeSet { root, goal, changes: Vec<ProjectChange>, run: Option<RunRequest>, preview: Preview }
ProjectChange::CreateWorkflow { path, content }
ProjectChange::UpdateWorkflow { path, before: Witness, content }
ProjectChange::CreateProjectFile { content } · UpdateProjectFile { before, content }
ProjectChange::CreateSupportingFile { path, content } · UpdateSupportingFile { path, before, content }
RunRequest { workflow, vars, max_cost_usd }
Witness = the blake3 of the bytes the preview was built over
```

No delete, no move, no rename in wave 5.a: a change set can only add or replace bytes the human has seen in full.

### The change set is built from the reply, never from the prose

The reasoner's reply may carry fenced blocks. A block whose fence names a path (```` ```yaml path=daily.nika.yaml ````) or whose first line is `# path: <p>` proposes a change at that path; the path must be relative, inside the proven root, and end in `.nika.yaml` (a workflow), be exactly `nika.yaml` (the project file), or be a supporting file the human named. Anything else is prose. A workflow's bytes go through the fix ladder's mechanical prepass (ADR-113) BEFORE the preview: the model's dead forms are repaired the way `nika check --fix` repairs them, and the repairs are listed in the preview.

### preview == apply, proven

The preview renders from the change set: for each change the exact bytes (a create) or a unified diff against the witnessed bytes (an update) and, for a workflow, the engine's audit of those exact bytes through the oracle facade (`audit_source` on the in-memory source: valid · access ready · capacity fit · findings · hints · the effect rows — reads · writes · network · secrets · models · spend · human gates — from the report's own permits and inventory). The apply consumes the SAME change set: it re-reads every update target, refuses when the bytes no longer match the witness (« the project changed since this preview · nothing was applied · rebuild the preview »), writes each file atomically (temp file + rename, 0644, parent created), and never touches a file outside the set. `preview(set) == apply(set)` is a test, not a promise: the bytes the preview printed are the bytes on disk after apply, byte for byte.

### Consent is a trusted session event

The session runtime returns `TurnOutcome::Proposal(preview)` and holds the set as `pending`. The door prints the preview and reads the next line; `yes` is consent, anything else discards the set. Consent is a method on the runtime called by the door with the human's line (`SessionRuntime::consent(answer)`); no reasoner ever sees a tool that applies, and no reply can contain a consent. A pending set is dropped by the next reasoning turn (a new proposal replaces it; a question discards it).

### The automatic check, the run, the observation

After apply, every workflow the set wrote is checked on disk through the facade; the verdicts are returned to the human as facts. When the set carries a `RunRequest` (the human asked to create AND run), the runtime returns `TurnOutcome::RunRequested { workflow, vars, max_cost_usd }` ONLY when the on-disk check is clean; the DOOR executes the same `nika run` path the CLI owns (the session sits below the CLI and never executes) and then tells the runtime what it observed (`SessionRuntime::observe_run(exit, trace)`). Attaching a run is observation: the session never re-authorizes credentials, never re-runs. The latest trace is then a fact (« what happened in the last run » reads the trace's outputs through the trace crate, never the runtime's memory).

### What the model can and cannot do (the lease by state)

| state | the model may | the model may not |
|---|---|---|
| Ready / Understanding | answer · propose a change set (fenced blocks) | write · run · consent |
| Previewing → AwaitingConsent | nothing (the human decides) | see the consent line |
| Applying → Checking | nothing (mechanical) | edit files outside the set |
| RunReady → Running | nothing (the door runs) | answer a human gate |

## Consequences

- The plugin's « paste this file » disappears from the native door; the session writes, checks and reports, and the human consents once with the exact bytes in view.
- The MCP oracle stays read-only: the change set is a session-only authority in this wave (the pack's « no general project write over the read-only oracle without a dedicated authority design »).
- The repair round is the same machinery: the findings ride the session's recent turns, « fix it » brings the reasoner's repaired file back as a witnessed update, the consent lands it, the check reads clean. A bounded automatic loop is a follow-up.
- A run that pauses at a human gate (exit 4) returns to the session as a pending decision: the gate is read from the trace's own pause event, the human's line becomes the resume the door runs through `nika run --resume`, an empty line is refused, nothing answers for them, and a gate is answered once. On a terminal the CLI's own ask (sugar over the durable pause) may take the answer first; the session's path is the headless fallback.
- Deletion, renames and integration wiring (`WireIntegration`) stay out until a change class needs them.

## Tests (the closing conditions)

- exact create preview: the bytes printed are the bytes applied.
- exact update preview: a diff against the witnessed bytes; a stale witness refuses and applies nothing.
- a path outside the root, an absolute path, a path with `..`, a non-workflow extension the human never named: refused before preview.
- the preview's effect rows come from the audit of the exact bytes (a `permits.fs.read` grant is listed as a runtime read · an `exec` as a program · an `infer` as a model).
- the fix ladder's prepass runs before the preview and its repairs are listed.
- consent: `yes` applies, anything else discards; a reply carrying « yes » is not consent; a new proposal replaces a pending one.
- the automatic check follows every workflow write; combined create+run stops on findings (`RunRequested` never returned on a dirty check).
- no hidden network during preview (the audit is offline; the reasoner is not called between proposal and consent).
- the read-only root refuses at apply with the OS error and applies nothing (partial sets never land: the witnesses are checked for every update before the first write).
