# Changelog · the nika plugin

The bundle every marketplace installs (Claude Code · Codex · Cursor).
Versions move together across all manifests (the mirror gate pins it).

## Unreleased

Authoring now reads the workspace before creating another file:
`nika list` discovers local candidates, then `explain` · `inspect` · `check`
establish what each one does and whether it is clean. The skill states the
boundary explicitly: listing is discovery, never certification.

The `when:` map now names the full closed CEL callable set instead of
incorrectly teaching `size()` as the only function.

The authoring skill teaches the door the engine has: `lift:` (one
construct, two laws · `law: taint` with `from:` · `law: data-as-code`).
Its live yaml fences are complete nine-key files (`nika: <id>` ·
`inputs` · `permits` · `tasks`). Extracted and fed to the 0.109 oracle
they check rc=0 (the composition parent needs its child beside it —
that is the COMP-001 law the same section teaches).
The 0.109.0 and 0.109.1 bundles still printed a `declassify:` fence with
`to: trusted` and listed `fail_workflow:` among the live `on_error:`
actions · three forms the 0.109 engine refuses at parse
(`NIKA-PARSE-005`), so a file written from the skill died before a token
was spent. `on_error:` is `recover:` or `skip:`; the default IS failure
and has no keyword. The engine-side kit gate
(`the_kit_never_teaches_a_form_the_engine_refuses`) now holds those
spellings, proven by mutation.

## 0.112.0 — 2026-08-21

Lockstep on the engine wave.

## 0.111.0 — 2026-08-19

Lockstep on the engine wave.

## 0.110.0 — 2026-08-19

Lockstep on the engine wave.

## 0.109.2 — 2026-08-19

Lockstep on the engine wave.

## 0.109.1 — 2026-08-19

Lockstep on the engine wave.

## 0.109.0 — 2026-08-18

Lockstep on the engine wave.

## 0.108.0 — 2026-08-07

The bundle becomes portable. [Agent Plugins 1.0.0](https://agent-plugins.org)
published 2026-08-06 — the package format Cursor, GitHub Copilot, Kiro,
VS Code and ChatGPT/Codex read — and the kit now carries its manifest at
the plugin root beside the three client-native ones: `plugin.json` and
`mcp.json`.

Purely additive, and that is the format's own doing. v1 standardises
exactly two component types, skills and MCP servers, and says why in its
own text: commands, hooks, agents, rules and LSP are « too
client-specific for a stable portable contract ». So `.claude-plugin/`
and `.cursor-plugin/` keep every reason they had — a conformant client
never looks there, and the client that reads the richer layout still
gets all of it.

`mcp.json` is not a copy of `.mcp.json`. The portable form carries its
own `$schema` and a `type` discriminant per server that the Claude Code
shape has no field for, and the two schema versions must agree or the
MCP half is refused on its own.

What this deliberately did NOT do: rename anything to a reverse-domain
namespace (that namespace belongs to the client owning the domain, and
we own none of them), and claim GitHub Copilot — it reads a root
`plugin.json` too, but in its own format. Same filename, different
contract. VS Code and Cursor were verified from their own documentation
before a single matrix cell moved.

Guarded, not hoped: the portable manifest fails CLOSED — one wrong
character in `name` and a conformant client drops the whole kit in
silence — so hygiene vector 48 checks the manifest, the MCP config and
every skill's frontmatter against the encoded 1.0.0 rules, and vector 49
runs the mutation proof that keeps 48 honest.

Same wave, the repo this kit ships from was renamed
`supernovae-st/nika-agents` → `supernovae-st/nika-plugins`. The old name
said what the kit is made of, not what it is, and « agent » already meant
three other things here: the frozen `agent:` verb, other vendors'
products, and the repo. Every install line, marketplace card and teaching
page follows; GitHub redirects the rest.

## 0.107.0 — 2026-08-01

The agent-run contract lands (the friction was WRITTEN, not
technical). The old law — « running is the human's move » — predates
`nika guard`: prose said the agent never launches while the structure
already judged every launch at the hook. Three laws replace it, in
the injected session context and the delegation rule alike: (1) the
agent MAY run, capped, when the human asks — ceiling announced first,
its own terminal, `nika guard` judging; (2) the human gate is theirs
alone — a paused run's question goes back to the conversation
verbatim, never a pre-filled `--answer`; (3) cost honesty before the
gesture. The authoring/porting/debugging subagents keep their
read-only nature FOR THE STATED REASON (their craft is writing and
diagnosis; launching belongs to the conversation), the doctor's fixes
stay the human's (credentials · env that dies with the process), and
the MCP oracle gains NO run tool — the agent launches in its own
tool-use, visible and interruptible, never through a hidden channel.
Installed kits keep the old prose until the next release train picks
this up. The law beneath: an agent may SPEND under a ceiling; it may
never CONSENT in the human's place.

The kit learns to keep itself honest about its substrate. The
session-context hook grows two probes: a missing nika binary teaches
the install line in every workspace (every surface of this kit invokes
it — silence would cost more than noise), and a kit/binary divergence
at major.minor names both versions with the direction-aware align
command. A sixth slash command lands: `/nika:doctor` — the agent-side
"is my suite coherent?" gesture, riding the binary's kit↔binary
handshake (`nika doctor` now probes the three kit landings and prints
the per-client refresh, both rungs named for Claude Code).

## 0.106.0 — 2026-07-27

Lockstep on the engine wave.

## 0.105.0 — 2026-07-20

Lockstep on the engine wave.

## 0.103.0 — 2026-07-13

Lockstep on the engine wave.

## 0.102.0 — 2026-07-13

Lockstep on the engine wave.

## 0.101.0 — 2026-07-13

Lockstep on the engine wave (the sovereign lane ships whole — every
release binary serves local models). Kit content = the 0.100.1
readonly patch, renumbered onto the wave.

## 0.100.1 — 2026-07-12

- The three subagents declare `readonly: true` — Cursor ENFORCES what the
  prose always promised (« read-only oracle · it never runs the workflow »);
  Claude Code tolerates the unknown key, live-probed first (a probe agent
  carrying the key loads and is offered next to a control). Kit-only patch
  on the 0.100 wave.

## 0.100.0 — 2026-07-12

The galaxy shares one wave number: engine · extension · client-sdk ·
kit ship the same version per wave from 0.100 on (0.5.3 → 0.100.0 —
alignment only, no functional change).

## 0.5.3 — 2026-07-12

The seatbelts reach Codex (issue #505 closed by a live probe, not a doc):

- `.codex-plugin/plugin.json` now declares `"hooks"` — Codex prefers that
  manifest over `.claude-plugin/plugin.json` when both exist, so the field's
  absence silently shadowed the three hooks on the one surface that needed
  its own manifest. One line, three seatbelts.
- The probe that earned it (headless `codex exec`, payloads dumped verbatim):
  Codex emits the **Claude Code dialect exactly** — `hook_event_name`,
  `tool_name: "Bash"`, `tool_input.command`, `tool_response` — and honors the
  same output envelopes (`permissionDecision: deny` blocks with the reason
  relayed to the model; SessionStart `additionalContext` is injected as
  context). The kit's scripts run UNCHANGED; behavioral proof: guard-run
  denied a red `nika run` and the model quoted NIKA-VAR-001 verbatim.
- Codex notes: hooks ride the `[features] hooks = true` flag (or
  `--enable hooks`), and a first interactive run asks to trust new hooks —
  both are Codex-side gates, not kit config.

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
