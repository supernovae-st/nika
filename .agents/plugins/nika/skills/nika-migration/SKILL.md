---
name: nika-migration
description: Convert existing automation — shell scripts, Python glue, Makefile targets, CI jobs, prompt chains in docs — into checkable .nika.yaml workflows. Use when a script wraps LLM calls or HTTP/file plumbing, a prompt chain lives in a README or notebook, or ad-hoc automation needs audit, cost bounds and replayable traces.
---

# Migrating existing automation to Nika

A workflow makes its inputs, effects and execution evidence inspectable.
Migration re-declares the intent so the checker can see it; a clean check
does not authorize effects or prove that the migrated behavior is correct.

## When to migrate (and when not to)

Migrate when the automation: calls an LLM anywhere · chains
HTTP/file/JSON steps around AI output · is repeated (cron, CI, "run
this every release") · needs a cost bound or an audit trail · is
handed to someone else to run.

Do NOT migrate: one-shot commands · interactive debugging sessions ·
sub-second pure-shell pipelines with zero AI and zero HTTP (a
`Makefile` that only compiles code is already in its best form).

## The mapping table

| In the script | In the workflow |
|---|---|
| a step / function | one task, exactly one verb |
| `curl` / `wget` / `fetch()` helper | `invoke:` `tool: "nika:fetch"` — **for an API, set `mode: raw` or `mode: jq`** (the default `markdown` mode is for pages and escapes JSON bodies) |
| `curl … \| jq` in one breath | ONE fetch task: `mode: jq` + `jq: '<expression>'` — the shape rides the fetch |
| `jq` / `sed` on JSON | `nika:jq` (arg name is `expression`), or an `extract:` binding |
| `cat` / `cp` / `mkdir` / `tee` | `nika:read` / `nika:write` (`create_dirs: true`) |
| in-place file edits | `nika:edit` |
| the LLM call (SDK, `curl` to an API) | `infer:` with `prompt`, `schema?`, `max_tokens` |
| an agent loop (retry-until-good) | `agent:` with `tools` allowlist + `max_turns` |
| a retry/backoff loop around a flaky call | `retry:` on the task — `max_attempts` + `backoff_strategy: exponential` + `jitter: true` (transient provider/network errors only; a wrong prompt never heals by retry) |
| `for item in …` | `for_each:` fan-out |
| `if <condition>` | a `when:` gate |
| `$1` positional parameters | an `inputs:` declaration · supplied with `--var key=value` |
| a value baked into the script | a `const:` entry · read as `${{ const.x }}` |
| `$SOME_SETTING` (non-sensitive) | an `inputs:` declaration with `required: false` and a `default:` · read as `${{ inputs.KEY }}` |
| an env var a CHILD process must see | `permits: { env: [NAME] }` — a child inherits nothing |
| `API_KEY=…` literals | `${{ secrets.X }}` + `secrets:` block with its `egress:` sink |
| step B reads step A's output | `with: { a: "${{ tasks.A.output }}" }` on B — the binding IS the edge — then `${{ with.a }}` in the body |
| step B only waits for step A (no data) | `after: { A: success }` (predicates: `success` · `failure` · `skipped` · `terminal`) |
| the irreversible step (deploy, send, publish) | a confirm gate before it (`nika:prompt`) — human answers at run time |
| what no builtin/MCP covers (git, build tools) | `exec:` with `command:` as ARGV (`["git", "log", "-1"]`) + a row in the exec ledger |
| a pipe, redirect or glob inside the command | `shell:` explicitly — `command:` has no implicit shell |

## The port protocol

1. **Read the source completely.** Inventory: inputs · outputs · side
   effects · credentials · the failure the author feared (that guard
   clause is the intent — keep it).
2. **Route to a template**: `nika new '?'` lists the embedded
   set; pick the OUTER shape (chain · fanout · gate-and-act ·
   etl-state · agent-loop · human-gated-ship) and instantiate with
   `nika new <template> <file>.nika.yaml`.
3. **Map with the table.** Native-first is the law: `invoke: nika:*`
   → `invoke: mcp:<server>/<tool>` → `exec:` last. Every surviving
   `exec:` gets its ledger row (task · command · why no native path ·
   unlock that removes it).
4. **Shape under mock**: `model: mock/echo` while the structure
   settles — `nika check <file>` after every change, repair from the
   diagnostics until exit 0, then `--native-strict`.
5. **Declare the boundary**: `permits:` is mandatory — an effect under
   no block refuses `NIKA-AUTH-006` at check.
   `nika check <file> --infer-permits` prints the tightest block;
   paste it in. The script trusted its author; the workflow trusts
   nobody by default (a pure-compute port still declares
   `permits: {}`).
6. **Prove the intended parity** on controlled inputs and authorized
   destinations. Inspect the old script's effects before running it too.
   `nika run <file> --model mock/echo` changes only the envelope model.
   Per-task model pins are unchanged; tools, subprocesses, writes and
   secret sources remain real. Compare expected artifacts in isolated fixtures;
   do not rerun a production publish or send merely to compare behavior.
   For a workflow requiring no network, subprocess or write effect,
   `nika test <file> --update` can pin its outputs in a golden. That
   simulated plane refuses those effects: an effecting port needs artifact
   assertions and trace inspection, not a promised golden it cannot produce.
7. **Hand off honestly**: the workflow, the parity checks that actually
   passed, a golden only where applicable, and the run line
   (`nika run <file> --var … --max-cost-usd <n>`). Retire the old script
   only when parity covers its intended behavior and the user authorizes
   that removal; preserve any remaining callers until migrated.

## Porting a pre-0.106 workflow file

A `.nika.yaml` written before 0.106 can refuse to check today — the
flag day changed what an existing file MEANS. Run `nika check <file>
--fix` first: it migrates three classes mechanically, comment-
preserving and idempotent.

| Dead form | Becomes | Repair |
|---|---|---|
| `vars:` entry, caller-supplied | `inputs:` (typed · `required:` · `default:`) | `--fix` |
| `vars:` entry, fixed value | `const:` | `--fix` |
| `workflow:` envelope key (scalar or object) | `nika: <kebab-case-name>` + `tasks:` map | `--fix` |
| `after: { t: succeeded / failed }` | `success` / `failure` | `--fix` |
| `env:` entry, non-sensitive | `inputs:` (typed · `required: false` · `default:`) | **yours** |
| `env:` entry, a credential | `secrets:` (a store reference) | **yours** |
| `env:` name a child must see | `permits: { env: [NAME] }` | **yours** |
| no `permits:` block, any effect | the inferred block (`--infer-permits`) | **yours** |

`--fix` is atomic-or-nothing per class: on a credential-shaped name, a
typed-only declaration, a flow-style `vars: {…}` header or an empty
block it leaves the file UNTOUCHED and names the reason — it never
guesses. `env:` has NO mechanical repair by design: re-shaping a flat
string map into typed declarations is a classification, and only you
know whether a name is configuration, a credential, or something a
child process needs to see.

## Traps

- Porting a helper script by wrapping it (`exec: node helper.mjs`) is
  not a migration — that is `native-first/005`. Unbundle the helper
  into fetch/jq/read/write tasks.
- A prompt chain in a doc usually hides implicit state ("then take
  the output and…") — make every handoff an explicit
  `${{ tasks.X.output }}` reference so the checker can trace it.
- Scripts swallow errors (`|| true`); workflows should not. If the
  source ignored a failure, ask whether that was intent or debt —
  default to letting the task fail loudly.
- Credentials in the script's environment become DECLARED secrets
  with sinks — the engine masks them; the script never did.
