# Changelog · the nika plugin

The bundle every marketplace installs (Claude Code · Codex · Cursor).
Versions move together across all manifests (the mirror gate pins it).

## 0.5.2 — 2026-07-12

Two teachings earned by the night's deep-e2e (each caught live before
it was written down):

- authoring: the `outputs:` binding law — bind `${{ tasks.<id>.output }}`,
  never the bare task (the ENVELOPE: status + timestamps → `nika test`
  goldens drift red on every run · the engine now teaches it at check
  time as `[envelope-output]`, the skill says it at write time).
- authoring: the sovereign lane joined the run step — `nika model pull`
  → `nika model serve` (qwen3-family GGUFs; the banner prints the exact
  wiring), beside the ollama route.

## 0.5.1 — 2026-07-12

- session-context: a session opened in a SUBDIR of the workspace now
  gets the map — the script resolves the git toplevel before probing
  the workspace markers (proven lost from `src/deep/`; non-git dirs
  keep the old behavior, silence stays silent).

## 0.5.0 — 2026-07-12

Hooks parity: the three seatbelts reach Claude Code (they were
Cursor-only — `claude plugin details` inventoried Hooks (0), found by
the completeness audit).

- ONE script per concern, TWO dialects, sniffed from stdin
  (`hook_event_name` is Claude Code's): session-context serves
  `hookSpecificOutput.additionalContext` on SessionStart · guard-run
  answers PreToolUse (matcher Bash) with `permissionDecision: deny` +
  the findings as the reason — and `{}` (no opinion) on pass, never
  "allow", which would skip the user's own permission prompt ·
  check-on-edit answers PostToolUse (Edit|Write|MultiEdit) with
  findings on stderr + exit 2, the documented feedback channel.
- `hooks/claude-hooks.json` (`${CLAUDE_PLUGIN_ROOT}` paths) wired via an
  explicit `hooks` field in the Claude Code manifest — symmetric with
  Cursor's explicit `cursor-hooks.json`, no auto-discovery ambiguity
  in either direction.
- Battery: 12 two-dialect cases (deny shapes · no-opinion pass ·
  context envelopes · edit feedback rc contracts · non-nika silence).

## 0.4.2 — 2026-07-12

Deep-verify patch — each fix is a gap found by checking the kit
against the RELEASED binary and its own manifests:

- nika-debugging: traces are addressed by store path
  (`.nika/traces/<name>`) — `trace ls` prints bare names but the
  released readers take paths (bare names join the next release); a
  failed run's card already carries the full path (`autopsy:` line),
  taught as step 0.
- nika-migration: the mapping table learns `retry:`
  (max_attempts · backoff_strategy · jitter — confirmed live) for the
  script-side retry/backoff loop.
- session-context hook: commands named without a slash scheme
  ("check, explain, new, trace, permits — slash-prefixed per your
  client") — the map said /nika:* while the Cursor manifest says
  /check; naming the scheme was the one claim the kit could not prove.

## 0.4.1 — 2026-07-12

Everything in this patch was earned by a fresh-user gauntlet run
against the released binary (real API workflows · real failures ·
real tokens): each teaching below is a friction that actually fired.

- session-context hook: an equipped repo (`nika init` wrote
  `.cursor/rules/nika.mdc`) now gets the map at session start even
  before its first workflow exists — that first session is exactly
  when the map matters.
- nika-debugging: prompts headless FAIL with
  `NIKA-BUILTIN-PROMPT-001` (a terminal blocks, an agent's world does
  not) — same `--resume --answer` line either way; the failed-run
  card's own `autopsy:` line is the entry point.
- nika-operating: the secrets taint FLOWS — every downstream sink of
  secret-derived data needs its own `egress:` entry (worked example);
  `host:`-scoped egress cannot be proven against an interpolated URL;
  mock mocks the MODEL, not the tools (goldens are for hermetic
  workflows — network truth is proven by traces).
- nika-migration: `curl … | jq` collapses into ONE fetch task
  (`mode: jq` + `jq:`); an API fetch needs `mode: raw`/`jq` — the
  default `markdown` mode is for pages and escapes JSON; `nika:jq`'s
  arg is `expression`.

## 0.4.0 — 2026-07-12

The suite release: one component per use case becomes a full crew —
an agent with this bundle authors, debugs, operates and migrates
workflows on its own.

- Three new skills: `nika-debugging` (trace forensics · resume lines ·
  surgical reruns), `nika-operating` (spend caps · permits · secrets ·
  model swaps · CI goldens · OTLP export), `nika-migration` (scripts
  and prompt chains → workflows, mapping table + parity protocol).
- Two new subagents: `nika-debugger` (evidence-first run forensics
  from the hash-chained trace) and `nika-migrator` (inventory →
  native-first mapping → check loop → golden pin).
- Two new commands: `/nika:trace` (verdict · root cause · tamper
  check · the exact resume line) and `/nika:permits` (infer and paste
  the tightest boundary).
- The delegation rule: teaches the agent WHEN to propose a workflow
  (repeatable · multi-step · spend-bound AI work) and which bundled
  surface to reach for.
- Two new Cursor hooks, both fail-open: `sessionStart` injects the
  nika map when the workspace has workflows (surfaces · laws · where
  traces live); `beforeShellExecution` denies a `nika run` on a file
  with live check findings — the denial carries the findings, so the
  agent repairs and reruns (audit-before-run, structurally
  unskippable).

## 0.3.0 — 2026-07-12

- Cursor first-class: the `.cursor-plugin/plugin.json` manifest (logo ·
  explicit component paths) joins the Claude Code and Codex ones.
- `nika-author` subagent: route the intent to a template, fill the
  `# SLOT:` markers, loop `nika check` until rc=0 — read-only, never
  runs the workflow.
- check-on-edit hook (Cursor): every agent edit to a `*.nika.yaml` is
  audited immediately; findings in the hook log, never a block.
- The language rule ships as a bundled file (byte-identical to the
  `nika init` template) and the brand logo replaces the generic tile.

## 0.2.0 — 2026-07-10

- The MCP oracle grows to 8 read-only tools (`nika_check` ·
  `nika_explain` · `nika_schema` · `nika_examples` · `nika_template` ·
  `nika_canon` · `nika_catalog` · `nika_tools`).
- Three slash commands: `/nika:check` · `/nika:explain` · `/nika:new`.

## 0.1.0 — 2026-07-03

- First release: the `nika-authoring` skill (author → check → repair)
  + the read-only MCP oracle (check · explain).
