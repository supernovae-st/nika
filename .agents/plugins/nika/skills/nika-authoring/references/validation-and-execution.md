# Validation and execution

Read this reference for the matching task; return to [the skill](../SKILL.md) for scope and completion.

## The oracle is evidence, not permission

Use the installed engine's `nika check --json` report, not a second
checker assembled from this prose. Keep its engine/spec identity with the
result. Judge `clean` (the exit's verdict under the flags you passed —
under `--native-strict` an exec a builtin covers is a `findings[]` row and
`native_strict_clean` repeats `clean`) separately from `paid_ready`:
a file can be paid-ready and still fail a permits finding. For composition,
inspect `judged` and the children's findings; a report that did not resolve
children cannot certify the tree.

Check readiness never authorizes execution, spend, a wider permit, or a
publication. Carry out execution already authorized by the user without
asking again; preserve its scope and the engine and host gates. If a
business decision or human-gate answer is still missing, ask for that
answer rather than inferring it from permission to run.
`--model mock/echo` changes the envelope model, not per-task
model pins, subprocesses, network tools, file writes, or secret sources.
Before an authorized rehearsal, review those effects and use an isolated
workspace with the intended boundary. In an editor, workspace trust is a
separate host decision; an offline model does not bypass it.

## Validation and execution

1. **Reuse the relevant workflow or example.** For a new structure,
   inspect the shelf with `nika try` and read a matching `nika new <slug>`.
   For a small repair, keep the existing file and change only what the
   task and diagnostics require.
2. **Write the file.** The envelope is `nika: <id>` (kebab-case — the
   workflow id lives ON the tag since 2026-08-12; that one key carries
   BOTH the mark and the name, and `description:` died with the
   `workflow:` object, which is no longer an envelope key at all) + a
   `tasks:` MAP keyed by task id —
   the key IS the identity, never a `- id:` sequence. Pick
   models and builtins from the embedded catalogs — `nika catalog`
   (providers · models · capabilities · which env var each needs) and
   `nika catalog --tools` (the `nika:*` builtins an `invoke` reaches
   without MCP); before a run, `nika inspect <file>` shows the anatomy:
   tasks · waves · the cost floor.
3. **Check it**: `nika check <file>` (exit 0 = clean · 2 = findings),
   then `nika check --native-strict <file>` — it fails on any
   `native-first` hint (an `exec:` a builtin covers).
4. **Repair**: `nika check <file> --fix` applies the machine-applicable
   repairs first (typo'd fields · tools · args · `after:` targets ·
   `${{ }}` references — typed did-you-mean only, ambiguity is skipped
   with a note, never guessed) and re-audits; repair what remains from
   the diagnostics — they name the exact task, reference and fix.
   Unknown code? `nika explain NIKA-XXXX`.
5. Repeat 3–4 until clean, or report the concrete unresolved dependency
   or decision with the checked file and its diagnostics. An incomplete
   handoff is not a runnable result. For an execution-ready handoff,
   require a clean `nika check --native-strict`; for paid inference,
   also inspect `paid_ready` and its blockers.
   `--native-strict` is the run-gate bar (an `exec:` a builtin covers).
   `.paid_ready` is the paid-infer bar
   (`nika check --json <file> | jq .paid_ready`).
   A green exit with leftover `infer-as-law` / `digit-string-enum` /
   `glob-readme` / `jq-as-map` / `unproven-law` is
   legal, not the one-way. The MCP `nika_check` oracle fails
   `infer-as-law` and `digit-string-enum` by default.
   The exec ledger does NOT buy an exemption (measured: a `.py` wrapper
   fails with a complete ledger) — it documents intent for a reviewer.
   What passes is an `exec:` of a real tool (`git`, `docker`); what
   fails is an `exec:` of a `.py`/`.mjs`/`.sh` wrapper, ledger or not.
6. When execution is authorized, run it: `nika run <file>`. Simulate envelope inference
   with `--model mock/echo`; its other effects remain real; run locally with `--model ollama/<model>` —
   or fully in-binary: `nika model pull <owner/repo-GGUF>` then
   `nika model serve --model <id>` (qwen3-family GGUFs today; the
   serve banner prints the exact env + `model:` line workflows use).
   Inputs ride `--var key=value` (repeatable · the flag names an
   `inputs:` declaration · unknown keys refused); a run paused on a
   `nika:prompt` resumes with
   `nika run <file> --resume <trace> --answer <task>=<value>`
   (confirm gates take booleans: `--answer approve=true`).
7. Pin it for CI **only when the mock run needs no network, subprocess,
   or write effect**: `nika test <file> --update` writes
   `<file>.golden.json`; `nika test <file>` replays and compares —
   deterministic, zero keys. The simulated test plane refuses those effects
   deliberately. For an effecting workflow, rehearse with
   `nika run <file> --model mock/echo` in scratch, inspect the artifacts, and
   verify its trace; never promise a golden that cannot run.
8. **Prove a run that mattered**: execution journals are enabled by default
   under `.nika/traces/`; a refusal before execution, disabled recording or
   lost ownership can leave no complete trace. `nika trace verify <trace>` climbs a
   four-tier ladder and reports the highest tier honestly attained —
   chain OK · **SEALED** (the run signature verifies against a custody
   key) · **ANCHORED** (the detached transparency-log sidecar verifies
   fully offline) · **REPLAYED** (`--replay <fresh-trace>` compares a fresh run;
   verify never re-executes). `nika trace show <trace>` reads the card;
   `nika trace evidence <trace>` exports the pack an auditor reads without
   trusting your summary. Verification never creates or seals the journal;
   an internally consistent chain does not itself prove producer honesty.
   Cite the trace and the actual proof tier, never a memory of the run.

## Cost honesty (never hide unknown spend)

- `nika check` prints the cost ceiling BEFORE any token: `≤ $X` is a
  ceiling · `≥ $X FLOOR` means at least one task is unbounded — name
  the reason (a missing `max_tokens`, an uncataloged model, an
  expression fan-out), never round it to $0.
- **The ceiling covers OUTPUT tokens only. The prompt is not in it.**
  `max_tokens` is the max OUTPUT tokens, and that is what the sum
  prices; `input_per_million` has no reader in the checker. Measured
  2026-07-28: a workflow that fetches a 3.2 MB document and
  interpolates it into one prompt reports `$0.0075` and would bill
  about `$2.46` in input alone. **When a prompt interpolates fetched or
  file content, say so at handoff and do not quote the ceiling as the
  bill** — quote it as the output half, and name the unpriced input.
  (The same repro is 4x over that model's context window, which nothing
  reports either: check the window yourself when a prompt carries a
  document.)
- A local model (`ollama/…`) is **unpriced compute, not « free »** —
  say "unpriced", never "$0" or "free".
- **A `FLOOR` on a CLOUD model is not a cheap model, it is a missing
  price row, and the short catalog id is often the reason.** Measured
  2026-07-28: `mistral/small` (the id `nika catalog` prints) reports
  `$0.0000 FLOOR · no catalog price (local/unknown model)` while
  `mistral/mistral-small-latest`, the same model, reports a
  `$0.0024 worst-case output ceiling`. The wording says "local/unknown" about
  a cataloged cloud model. Try the full model string before believing a
  cloud floor; if it still floors, hand the human the word "unpriced"
  and never a number.
- A spend cap rides the run: `nika run <file> --max-cost-usd <n>`
  refuses a known over-budget floor before execution. During execution,
  crossing the metered budget stops new admissions; already-started calls
  finish and count, so a concurrent wave can overshoot. Unpriced calls
  have no measured USD bound. Do not promise a hard invoice ceiling.
- `nika explain <file>` narrates all of this (waves · cost · touches ·
  how to run) — use it before handing a workflow to a human.
