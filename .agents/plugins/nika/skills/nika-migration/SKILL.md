---
name: nika-migration
description: Convert existing automation — shell scripts, Python glue, Makefile targets, CI jobs, prompt chains in docs — into checkable .nika.yaml workflows. Use when a script wraps LLM calls or HTTP/file plumbing, a prompt chain lives in a README or notebook, or ad-hoc automation needs audit, cost bounds and replayable traces.
---

# Migrating existing automation to Nika

A script runs; a workflow is **audited before it runs, bounded while
it runs, and proven after it runs**. Migration is not translation —
it is re-declaring the intent so the checker can see it.

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
| `jq` / `sed` on JSON | `nika:jq` (arg name is `expression`), or an `output:` binding |
| `cat` / `cp` / `mkdir` / `tee` | `nika:read` / `nika:write` (`create_dirs: true`) |
| in-place file edits | `nika:edit` |
| the LLM call (SDK, `curl` to an API) | `infer:` with `prompt`, `schema?`, `max_tokens` |
| an agent loop (retry-until-good) | `agent:` with `tools` allowlist + `max_turns` |
| a retry/backoff loop around a flaky call | `retry:` on the task — `max_attempts` + `backoff_strategy: exponential` + `jitter: true` (transient provider/network errors only; a wrong prompt never heals by retry) |
| `for item in …` | `for_each:` fan-out |
| `if <condition>` | a `when:` gate |
| `$1`, `$ENV_VAR` parameters | `vars:` + `--var key=value` · `${{ env.KEY }}` |
| `API_KEY=…` literals | `${{ secrets.X }}` + `secrets:` block with its `egress:` sink |
| step B reads step A's output | `${{ tasks.A.output }}` + `depends_on: [A]` |
| the irreversible step (deploy, send, publish) | a confirm gate before it (`nika:prompt`) — human answers at run time |
| what no builtin/MCP covers (git, build tools) | `exec:` + a row in the exec ledger |

## The port protocol

1. **Read the source completely.** Inventory: inputs · outputs · side
   effects · credentials · the failure the author feared (that guard
   clause is the intent — keep it).
2. **Route to a template**: `nika new --from '?'` lists the embedded
   set; pick the OUTER shape (chain · fanout · gate-and-act ·
   etl-state · agent-loop · human-gated-ship) and instantiate with
   `nika new --from <template> <file>.nika.yaml`.
3. **Map with the table.** Native-first is the law: `invoke: nika:*`
   → `invoke: mcp:<server>/<tool>` → `exec:` last. Every surviving
   `exec:` gets its ledger row (task · command · why no native path ·
   unlock that removes it).
4. **Shape under mock**: `model: mock/echo` while the structure
   settles — `nika check <file>` after every change, repair from the
   diagnostics until exit 0, then `--native-strict`.
5. **Declare the boundary**: `nika check <file> --infer-permits` →
   paste the `permits:` block. The script trusted its author; the
   workflow trusts nobody by default.
6. **Prove parity once**: run the old script and
   `nika run <file> --model mock/echo` (or a local model) side by
   side on the same input; compare the artifacts. Then pin:
   `nika test <file> --update` writes the golden.
7. **Hand off honestly**: the workflow file + the golden + the run
   line (`nika run <file> --var … --max-cost-usd <n>`). The human
   decides when the old script retires — never delete it yourself.

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
