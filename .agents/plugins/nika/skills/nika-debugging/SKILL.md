---
name: nika-debugging
description: Diagnose and repair failed, paused or suspicious Nika runs from their traces (.nika/traces). Use when nika run exited red, a run paused on a prompt, a NIKA-XXXX runtime finding needs a root cause, a trace must be read or tamper-verified, or a fixed workflow needs a surgical partial rerun.
---

# Debugging Nika runs

Every run writes a hash-chained journal to `.nika/traces/`. That journal
is the ONLY truth about what happened — debug from the trace, never from
a memory of the terminal scroll.

## The forensic loop (evidence first)

1. **Locate the run**: `nika trace ls` — age · size · workflow ·
   terminal state (completed/failed/paused) · `★` marks the newest
   trace of each workflow (the resume candidate).
2. **Read the card**: `nika trace show <trace>` — the final verdict,
   the waves, per-task outcome. `nika trace replay <trace>` re-renders
   the run live (replay = re-render, NEVER re-execute).
3. **Find the failing task**: `nika trace outputs <trace>` — verb ·
   duration · tokens · a bounded preview per task (full value:
   `nika trace peek`). The first red task is the root; everything
   downstream is fallout.
4. **Decode the finding**: `nika explain NIKA-XXXX` teaches the cause ·
   category · fix-form of any code the trace carries.
5. **Re-audit the file**: `nika check <file>` — a run that failed often
   fails again at check once you know what to look for (a model that no
   longer resolves, a missing env var, a permits violation).
6. **Fix minimally, rerun surgically** (below). Re-check before any
   rerun.

## Paused runs (prompts and confirm gates)

A run paused on `nika:prompt` is not failed — it is waiting. The card
names the unanswered prompt. Resume it:

```
nika run <file> --resume <trace> --answer <task>=<value>
```

Confirm gates take booleans (`--answer approve=true`). Removing a
paused trace refuses without `--force` and names the prompt it would
destroy — that refusal is protecting an answer, not being difficult.

## Surgical reruns (never restart what already worked)

- `nika run <file> --from <task-id>` — rerun from one task onward,
  keeping upstream results.
- `nika run <file> --task <task-id>` — rerun exactly one task.
- After an intentional behavior change, refresh the pin:
  `nika test <file> --update` rewrites the golden from an offline mock
  run — never hand-edit a `.golden.json` to make red green.

## Common root causes (check these before anything exotic)

- **Model does not resolve**: `nika check <file> --json` →
  `models_resolve` says whether every `model:` runs in THIS binary;
  `nika catalog` names the env var each provider needs.
- **Missing credential**: secrets ride `${{ secrets.X }}` /
  `${{ env.KEY }}` — the trace shows the task, the shell shows the
  env. `nika doctor` audits the machine side.
- **Timeout too tight**: local providers need `timeout: "300s"` or
  more — thinking models routinely think past 30s.
- **Permits violation**: the run was blocked by its own declared
  boundary — read the finding, then either the task is wrong or the
  boundary is (widen it consciously, never delete it).
- **Cost cap hit**: `--max-cost-usd` blocks BEFORE the call that would
  cross the cap — that is the feature working, not a bug. Raise the
  cap deliberately or shrink the task.

## Tamper evidence

`nika trace verify <trace>` checks the hash chain: any edited,
inserted, dropped or reordered line breaks every hash after it.
Exit 0 intact · 2 broken · 3 unchained (pre-chain journal). Cite the
trace in any report — a verified chain is proof, prose is not.

## Honesty lines

- The trace tells you what the engine did. It cannot tell you WHY a
  provider returned garbage — a provider-side outage or a model
  regression is named as a hypothesis, never asserted as fact.
- Never edit a trace. Never delete a paused trace to "clean up".
- If the binary is missing: `brew install supernovae-st/tap/nika` —
  do not reconstruct runs from memory.
