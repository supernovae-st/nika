---
name: nika-debugging
description: Diagnose and repair failed, paused or suspicious Nika runs from their traces (.nika/traces). Use when nika run exited red, a run paused on a prompt, a NIKA-XXXX runtime finding needs a root cause, a trace must be read or tamper-verified, or a fixed workflow needs a surgical partial rerun.
---

# Debugging Nika runs

Execution journals are enabled by default under `.nika/traces/`. A refusal
before execution, disabled recording or lost ownership can leave no complete
journal. Start from the available runtime evidence, not a memory of terminal
scroll; a receipt alone does not prove a sealed journal exists.

## The forensic loop (evidence first)

0. **A failed run already names its trace**: the card's `autopsy:`
   line carries the FULL trace path — start there when you have it.
1. **Locate the run**: `nika trace ls` — age · size · workflow ·
   terminal state (completed/failed/paused) · `★` marks the newest
   trace of each workflow (the resume candidate). Address a trace by
   its store path — `.nika/traces/<name>` — everywhere below (`ls`
   prints bare names; the readers take the path form on every
   version).
2. **Read the card**: `nika trace show <trace>` — the final verdict,
   the waves, per-task outcome. `nika trace replay <trace>` re-renders
   the run live (replay = re-render, NEVER re-execute).
3. **Find the failing task**: `nika trace outputs <trace>` — verb ·
   duration · tokens · a bounded preview per task (full value:
   `nika trace peek`). Follow its recorded dependencies and diagnostics;
   concurrent tasks can fail independently, so display order alone does
   not identify a root cause.
4. **Decode the finding**: `nika explain NIKA-XXXX` teaches the cause ·
   category · fix-form of any code the trace carries.
5. **Re-audit the file**: `nika check <file>` — a run that failed often
   fails again at check once you know what to look for (a model that no
   longer resolves, a missing env var, a permits violation).
6. **Fix minimally, rerun surgically** (below). Re-check before any
   rerun.

## Prompts and confirm gates

At a terminal, `nika:prompt` asks the human directly and the run
continues. Headless — which is where an agent lives — a prompt
without a `default:` **pauses durably** (exit 4 · `workflow_paused`
in the trace · never a failure frame) and the frame prints its exact
resume line. Three ways to answer, all recorded tamper-evident the same way:

```
nika run <file> --answer <task>=<value>                    # pre-answer at launch
nika run <file> --resume <trace> --answer <task>=<value>   # resume the pause
args: { …, default: <value> }                              # unattended default
```

Use only answers or defaults the user supplied or authorized. A paused run
does not authorize choosing its answer. Adding a default changes the
workflow's approval policy; it is not a mechanical repair for a pause.

Confirm gates take booleans (`--answer approve=true`). A recorded success
is reused only when the runtime's resume eligibility and identity checks
match; changed definitions or resolved inputs can require execution again.
Inspect the reported cache hits rather than predicting them from a success
line alone. Removing a paused trace refuses
without `--force` and names the prompt it would destroy — that
refusal is protecting an answer, not being difficult.

A failed run's card prints its own forensics line (`autopsy:
nika trace peek <trace> <task>`) — start there, it points at the
exact failing task.

## Surgical reruns and uncertain effects

Preserve prior results when the runtime admits their reuse. Before a rerun,
check the intended changes, resolved inputs, available evidence and effects
already observed. A lost response can follow a successful remote write:
reconcile it with the destination before resubmission. If its state remains
unknown, report that uncertainty; a retry or resume key alone does not prove
deduplication. Keep the existing execution authorization and ask only for a
missing decision or gate answer.

- `nika run <file> --resume <trace> --from <task-id>` forces that task
  and its transitive downstream to rerun; upstream reuse still depends on
  eligibility. Review effects before choosing this override.
- `nika run <file> --task <task-id>` includes that task and its transitive
  upstream dependencies. It can execute their effects too; it does not
  mean exactly one task runs.
- After an intentional behavior change, refresh the pin:
  `nika test <file> --update` rewrites the golden from an offline mock
  run only when the workflow fits the simulated plane (no network,
  subprocess or write effects). Otherwise use an authorized isolated
  rehearsal and artifact assertions; `--model mock/echo` does not disable
  tools or per-task model pins. Never hand-edit a `.golden.json` to make
  red green, or repeat irreversible effects merely to obtain a green run.

## Common root causes (check these before anything exotic)

- **Model does not resolve**: `nika check <file> --json` →
  `models_resolve` says whether every `model:` runs in THIS binary;
  `nika catalog` names the env var each provider needs.
- **Missing credential**: secrets ride `${{ secrets.X }}`, declared in
  the `secrets:` block (`source: env` + `key:`) — the trace shows the
  task, the shell shows the variable. `nika doctor` audits the machine
  side. Non-sensitive settings ride an `inputs:` entry with
  `required: false` and a `default:`.
- **Timeout too tight**: compare observed latency with the task's timeout,
  model and workload. Change the limit within the admitted budget; a longer
  timeout is not a general repair for a stalled or uncertain effect.
- **Permits violation**: the run was blocked by its own declared
  boundary — read the finding, then either the task is wrong or the
  boundary is (widen it consciously, never delete it). Read the recorded
  grant/refusal events when available; an admission refusal before the
  prologue can have no journal, so retain its typed diagnostic too.
  A workflow with NO `permits:` block has
  zero authority (`NIKA-AUTH-006`), and check refuses it before the
  run ever starts.
- **A child process cannot see a variable**: the environment is
  composed from a cleared slate, so an unnamed variable simply is not
  there. Name it in `permits: { env: [NAME] }` or in the task's own
  `env:` map.
- **Cost cap hit**: a known over-budget floor refuses before execution.
  During execution, crossing the metered budget stops new admissions;
  already-started calls finish and count, so the total can exceed the cap.
  Inspect the recorded spend and unpriced calls before deliberately changing
  the cap, concurrency or task limits.

## Tamper evidence

`nika trace verify <trace>` checks the recorded hash links. A consistent
unkeyed chain alone does not rule out rewriting the entire journal; compare
the head with independently trusted evidence or verify its trusted seal.
Exit 0 verified · 2 broken · 3 unchained or missing input · 5 incomplete. The
verdict also names the highest tier honestly attained — chain OK ·
**SEALED** (the `run_sealed` signature verifies against a custody
key) · **ANCHORED** (the detached sidecar verifies fully offline) ·
**REPLAYED** (`--replay` compares a fresh run; verify never
re-executes). A journal that never reached a lifecycle-terminal frame
verifies **INCOMPLETE**: the verifier's finding about a run that died
mid-flight — not a pass, and not a tamper claim. Say which one you
have. Cite the trace and the actual proof tier; `nika trace evidence <trace>`
exports the pack an auditor can inspect independently. Verification reads
existing evidence: it never creates or seals a journal, and a signature
does not prove that the producer told the truth.

## Honesty lines

- The trace tells you what the engine did. It cannot tell you WHY a
  provider returned garbage — a provider-side outage or a model
  regression is named as a hypothesis, never asserted as fact.
- Never edit a trace. Never delete a paused trace to "clean up".
- If the binary is missing: `brew install supernovae-st/tap/nika` —
  do not reconstruct runs from memory.
