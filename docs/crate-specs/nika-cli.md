# Crate spec · `nika-cli` (the `nika` binary surface)

| Field | Value |
|---|---|
| Status | **PRE-BUILD SPEC** (Gate 1 authored 2026-06-11, ahead of the S6 build — the experience contract is locked before the first line of Rust) |
| Layer | L4 interface (composition root `nika` stays L5 · <500 LOC) |
| Decision | D-2026-06-10-N6 (first-15-min suite · self-contained binary) · ADR-092 (check ladder) · experience contract below |
| LOC budget | ≤15k crate · ≤1500/file · ≤100/fn (Diamond caps) |
| err_prefix | reuses engine registries; CLI-only failures = exit codes, not NIKA codes |

## 1. Purpose

The human surface of the engine. One binary, self-contained (spec + examples
+ JSON Schema embedded at build), that makes the engine's core property —
**auditable before it runs** — *visible* at every interaction. The design law
for everything below: **the animation IS the data** (semantic rendering,
chrome ≤30%, zero decorative noise). And the human always keeps the hand.

## 2. Verb surface

### 1.0 launch floor (locked · D-2026-06-10-N6 · amended D-2026-06-20-N1 — was "v0.81")

| Verb | Does | Exit codes |
|---|---|---|
| `nika run <file>` | execute a workflow · live render (§3) | 0 ok · 1 workflow failed |
| `nika check <file>` | the ADR-092 static ladder (schema→DAG→CEL→effects→permits→cost) | 0 clean · 2 findings |
| `nika init` | scaffold a repo (.vscode schema wiring · AGENTS.md templates) | 0 · 3 env |
| `nika inspect <file>` | static anatomy: tasks · verbs · DAG (ASCII §6) · permits · cost interval | 0 · 2 |
| `nika graph <file> --mermaid\|dot\|json` | the ONE graph projector (§6) | 0 · 2 |
| `nika doctor` | environment diagnosis (PATH · providers reachable · keys present-not-printed · config) | 0 · 3 |
| `nika explain NIKA-XXXX` | teach one error code (cause · fix-form · doc link) | 0 · 2 unknown code |
| `nika completions <shell>` | shell completions (clap-generated) | 0 |
| `nika new --from <template>` | instantiate one of the 6 skeletons | 0 |
| `nika spec` / `nika examples list\|show\|run` / `nika schema` | the embedded self-contained surface | 0 |
| `nika lsp` | the in-binary language server (stdio) | — |
| `nika mcp` | the in-binary MCP server | — |

### v0.82 wave-2 (proposed 2026-06-11 · additive)

| Verb | Does |
|---|---|
| `nika fmt <file>` | canonical formatting (gofmt law: zero config · parser round-trips) |
| `nika trace list\|show <run>\|replay <run>` | the flight-recorder reader (§7) |
| `nika run --resume <run-id>` | resume from the last `checkpoint_written` event |
| `nika upgrade [--check]` | sovereign self-update (manifest + minisign · atomic swap · zero phone-home) |

Refused (Rams 10): `logs` · `ps` (daemon-shaped — arrives with `serve`, not
before) · a full-screen TUI app (the live render + webview cover it).

## 3. The `display` module — the render architecture

**One law: render = a pure fold over the event stream.** The runtime emits
`nika-event` events; every surface is a consumer of the same stream:

```
EventLog stream ──┬── TTY live renderer (this module · the wow)
                  ├── --json (NDJSON events verbatim · CI/agents)
                  ├── SSE (nika serve · v0.82+)
                  └── DAG webview overlay (nika-vscode · same glyph grammar)
```

No renderer owns private state derivation; a new surface costs a consumer,
never a fork in truth.

### 3.1 State glyphs (the state machine, drawn)

| State | Unicode | ASCII | Colour (semantic only) |
|---|---|---|---|
| pending | `○` | `.` | dim |
| running | `◐` (spinner) | `>` (dot pulse) | cyan (the ONE accent) |
| ok | `✔` | `ok` | green |
| failed | `✖` | `X` | red |
| retrying | `↻` | `r` | yellow |
| skipped (when:false) | `⊘` | `-` | dim |
| cancelled | `◼` | `x` | dim |

The ASCII column is a **first-class theme** (CI logs, legacy conhost, screen
readers via `--no-progress`), not a degraded mode — snapshot tests pin BOTH.

### 3.2 The spinner

Braille 10-frame cycle `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` · 80ms cadence · ticks ONLY on the
running line(s) · collapses to a static `◐` under `NIKA_REDUCED_MOTION=1`,
non-TTY, or `--no-progress`. Never more than one animated region per task
line (cognitive budget).

### 3.3 The run card anatomy (frame-by-frame)

```
frame 0 (T+0ms · the audit-as-greeting · BEFORE any effect)
┌──────────────────────────────────────────────────────────────────┐
  🦋 nika 1.0.0 · veille-news · 6 tasks · ceiling ≤ $0.04
     permits ✓ network:read(hn.algolia.com) · fs:write(./out)
└──────────────────────────────────────────────────────────────────┘
   ↑ 2 lines · the static proof (ADR-092) shown EVERY run · the trust moment

frame N (during · one line per task · topological order · stable rows)
  ✔ fetch_top        http 200 · 1.2s · 34 KB
  ✔ extract_ai       jq · 0.1s · 12 items
  ◐ summarize        claude-sonnet · 3.1s · ~$0.011 ▁▃▅
  ○ write_md         waiting on summarize
  ⊘ notify_slack     skipped (when: false)
  ── 2/6 done · $0.011 of ≤$0.04 · elapsed 4.4s ──────────────────
   ↑ footer meter: count · LIVE cost vs static ceiling · wall clock

frame final (success)
  ✔ veille-news · 6 tasks · 11.2s · $0.018 of ≤$0.04
    trace: .nika/traces/2026-06-11T14-02-33Z-a3f2.ndjson

frame final (failure · miette card)
  ✖ summarize failed · NIKA-431 provider refused (429 rate-limit)
    ┌─ veille-news.nika.yaml:23:9
    │   model: anthropic/claude-sonnet-4-6
    ╰─ retried 2× · budget exhausted
    fix: add retry.backoff_ms or switch provider — nika explain NIKA-431
```

Sparkline `▁▃▅` = token arrival rate (last 3 ticks · running infer tasks
only). The live `~$` re-folds on every `cost_incurred`/`infer_chunk` event —
honest because the ceiling is *statically proven* before the run; the meter
can only land at-or-under it.

### 3.4 Colour seam

One module (`display::term`), one rule: **semantic, never decorative**.
cyan = the single accent (running) · green/red/yellow = verdicts · dim =
metadata. Resolution order: `--color=never|always` → `NO_COLOR` → TTY
detect → `TERM=dumb`. Glyphs survive colour loss; meaning never lives in
colour alone (a11y).

### 3.5 Reduced surfaces

| Mode | Trigger | Renders |
|---|---|---|
| rich TTY | default | frames above · spinner · live meter |
| plain | `--no-progress` / non-TTY | one line per state TRANSITION (append-only · CI-stable) |
| json | `--json` | the event stream verbatim (NDJSON · the machine contract) |
| quiet | `--quiet` | final card only · errors always |

## 4. Exit-code contract (LOCKED by this spec)

```
0    success (run completed · check clean · verb done)
1    workflow failed        — a task reached failed; engine itself healthy
2    validation findings    — check/inspect/graph found errors in the FILE
3    environment error      — config · missing provider key · doctor findings
101  engine panic           — never deliberate; the trace is the crash report
```

Per-verb mapping lives in §2 tables. Rules: scripts may rely on these
forever (additive-only — new codes get new numbers); `--json` mode never
changes codes; `1 vs 2` is the *run-vs-file* distinction (CI gates on 2,
alerting gates on 1).

## 5. Crash posture

No crash reporter, no phone-home, ever (alignment Rule 1). A panic prints:
the trace path (the flight recorder IS the crash report, owned by the user)
+ `nika doctor` hint + the issue URL. Trace appends are line-atomic
(write+flush per event) so a killed run leaves a readable prefix.

## 6. The graph projector (one projector · N renderers)

`nika graph --json` is the canonical projection; mermaid/dot/ASCII/webview
all derive from it. Versioned envelope (`graph_format: 1`):

```json
{
  "graph_format": 1,
  "workflow": "veille-news",
  "nodes": [
    {"id": "fetch_top", "verb": "invoke", "tool": "nika:fetch",
     "when": null, "fan_out": null,
     "permits": ["network:read(hn.algolia.com)"],
     "cost_interval": null}
  ],
  "edges": [
    {"from": "fetch_top", "to": "extract_ai", "kind": "depends_on"}
  ]
}
```

Rules: topologically sorted `nodes` (stable order = stable layouts · no
jitter) · `edges.kind` closed enum (`depends_on` today · `on_failure`/
`on_finally` when the spec grows them) · run overlays (states/durations/
costs) come from the EVENT stream joined on `id` — the static graph never
carries run state (the two truths stay separate, joined at render).

`nika inspect` terminal DAG (box-drawing · derived from the same JSON):

```
veille-news · 6 tasks · ≤ $0.04
├─ fetch_top         invoke · nika:fetch
│  └─ extract_ai     exec · jq
│     └─ summarize   infer · anthropic/claude-sonnet-4-6 · ~$0.011-0.04
│        ├─ write_md invoke · nika:write → ./out
│        └─ notify   invoke · x-corp:slack · when: env.CI != 'true'
└─ (no orphans · DAG check NIKA-DAG-001 clean)
```

## 7. `nika trace` (the flight-recorder verbs · v0.82)

- `list` — table of runs (id · workflow · verdict · tasks · cost · when).
- `show <run>` — the final card + per-task table re-folded from the NDJSON.
- `replay <run>` — re-renders the live view by replaying events with
  compressed timing (`--speed 10x` default · `--step` = one event per
  keypress). **Replay = re-render, not re-execute** (zero effects · honest
  by construction). Pairs with the DAG webview's replay scrubber.

## 8. `nika doctor` posture

Diagnose-only + **print the exact fix command** (never auto-mutate):

```
✔ binary        1.0.0 (self-contained · spec v1 embedded)
✔ config        ~/.nika/config.toml
✖ provider      anthropic — ANTHROPIC_API_KEY unset
  fix: export ANTHROPIC_API_KEY=…   (or: nika doctor --explain providers)
⚠ completions   not installed for zsh
  fix: nika completions zsh > ~/.zfunc/_nika
```

`--fix` stays REFUSED v1 (auto-mutating PATH/shell config from a CLI is the
class of magic we refuse; the printed command is the contract). Revisit only
on repeated operator demand.

## 9. Man pages + completions

Both generated from the clap model at build (`xtask dist`): `man/nika.1` +
per-verb pages ship in the brew/deb artifacts; `completions` covers
bash/zsh/fish/powershell. One source of truth (the clap tree), zero
hand-maintained docs.

## 10. Keep-control flags (the human override surface)

`--dry-run` (plan only · zero effects) · `prompt: confirm` gates render as
a blocking card (TTY) or refuse with exit 3 (`--yes` required in CI) ·
`Ctrl-C` = graceful cancel (running tasks get cancel-safe teardown · trace
records `cancelled`) · budgets always visible in the footer meter. No
surface auto-escalates permits, ever.

## 11. The 12 gates (pre-build readiness)

| Gate | Status |
|---|---|
| 1 SPEC | ✅ this file (2026-06-11 · ahead of build) |
| 2-12 | at S6 build — TDD against §3 frames (snapshot tests pin BOTH glyph themes), property tests on the fold (event permutations → consistent terminal states), golden `--json` contract |

Prototype: `scripts/dev/render-trace.py` renders this exact grammar from a
trace NDJSON today (`--demo` included) — the design is runnable before the
Rust exists.

🦋 Nika — workflow engine for AI, AGPL, SuperNovae Studio.
