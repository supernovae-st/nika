# Nika TUI Redesign — Visual Specification

> **Companion to:** `2026-03-20-tui-redesign.md` (implementation plan)
> **Created:** 2026-03-20
> **Status:** DESIGN VALIDATED
> **Research:** 6 research docs in `docs/research/`

---

## Design Principles (from F1 + NASA + SpaceX research)

### 1. Dark Cockpit Philosophy (Boeing/NASA)
Normal state shows NO alerts. Absence of color = all clear. Only anomalies produce visual signals. Don't show green "OK" on every step — show NOTHING for nominal, show ONLY problems.

### 2. F1 Color Language (4 colors, zero ambiguity)
- **Purple** — all-time best / session fastest (fastest workflow run ever)
- **Green** — personal best / improved (faster than previous)
- **Yellow/Amber** — slower / warning (degraded)
- **Red** — critical / failure (errors, timeouts)

### 3. SpaceX "Big Number" Pattern
3-4 hero metrics displayed LARGE. Everything else secondary/dimmed. Progressive disclosure for detail.

### 4. "Eyes Forward" Invariants (NASA front wall)
5 things ALWAYS visible: workflow name, elapsed time, step count, error count, provider connectivity.

### 5. 3-Tier Information Hierarchy
- **Tier 1 "Glance"**: Fixed header/status bar — state, time, alerts. Always visible.
- **Tier 2 "Scan"**: Status grid + hero metrics. "Is everything OK?" in < 1 second.
- **Tier 3 "Focus"**: Detail on demand — streaming output, full params, raw logs.

---

## Color Palette: Cosmic Blue-Violet-Orange

```
BASE (90% of UI — the "dark cockpit")
  bg-deep:       #0C0E1A   ← main background (blue-tinted black)
  bg-surface:    #141829   ← panels, cards
  bg-elevated:   #1E2340   ← hover, selected, active
  bg-bright:     #283050   ← intense highlight

PRIMARY — Cosmic Blue
  blue-500:      #3B82F6   ← primary accent, focused borders
  blue-400:      #60A5FA   ← hover, glow states
  blue-300:      #93C5FD   ← subtle highlights

SECONDARY — Violet (Nika brand)
  violet-500:    #8B5CF6   ← Nika brand, infer verb, Shaka thinking
  violet-400:    #A78BFA   ← glow, active states

TERTIARY — Orange (Evangelion touch)
  orange-500:    #F59E0B   ← warnings, exec verb, active/running
  orange-400:    #FBBF24   ← glow, hover

SEMANTIC
  emerald-500:   #10B981   ← success, invoke verb, nominal
  cyan-500:      #06B6D4   ← info, fetch verb, data flow
  rose-500:      #F43F5E   ← agent verb, autonomous
  red-500:       #EF4444   ← error, critical

TEXT
  text-primary:  #E2E8F0   ← slate-200, main content
  text-secondary:#94A3B8   ← slate-400, labels
  text-muted:    #475569   ← slate-600, hints
  text-dim:      #334155   ← slate-700, disabled

BORDER
  border-normal: #1E293B   ← slate-800, default
  border-focus:  #3B82F6   ← blue-500, active
  border-subtle: #0F172A   ← slate-900, dividers
```

### Verb Color Mapping

| Verb | Color | Hex | Icon | Border running | Border done |
|------|-------|-----|------|----------------|-------------|
| infer: | Violet | #8B5CF6 | ⚡ | ┈ violet-400 glow pulse | ━ violet-600 solid |
| exec: | Orange | #F59E0B | 📟 | ┈ orange-400 glow pulse | ━ orange-600 solid |
| fetch: | Cyan | #06B6D4 | 🛰️ | ┈ cyan-400 glow pulse | ━ cyan-600 solid |
| invoke: | Emerald | #10B981 | 🔌 | ┈ emerald-400 glow pulse | ━ emerald-600 solid |
| agent: | Rose | #F43F5E | 🐔 | ┈ rose-400 glow pulse | ━ rose-600 solid |

### TaskBox Border States

```
QUEUED:    ╭┄┄┄┄┄┄┄┄┄┄┄┄┄╮   dashed, slate-600
           ┆ ⚪ task_name  ┆
           ╰┄┄┄┄┄┄┄┄┄┄┄┄┄╯

RUNNING:   ╭┈┈┈┈┈┈┈┈┈┈┈┈┈╮   dotted, verb-400 with sine pulse
           ┊ ⣻ task_name  ┊   spinner: ⣾⣽⣻⢿⡿⣟⣯⣷ @ 80ms
           ╰┈┈┈┈┈┈┈┈┈┈┈┈┈╯

DONE:      ╭━━━━━━━━━━━━━━╮   bold, verb-600
           ┃ ✅ task_name  ┃
           ╰━━━━━━━━━━━━━━╯

FAILED:    ╭━━━━━━━━━━━━━━╮   bold, red-500
           ┃ ✗ task_name   ┃
           ╰━━━━━━━━━━━━━━╯
```

---

## Animation Spec (tachyonfx v0.25.0)

### Integration
```toml
tachyonfx = { version = "0.25.0", optional = true, features = ["dsl"] }
```

### Animation Recipes

| Trigger | Effect | Duration | Easing |
|---------|--------|----------|--------|
| View switch (1→2→3) | `slide_out` + `fade_to` → `slide_in` + `fade_from` | 150ms | SineInOut |
| Task starts running | Border glow pulse: `ping_pong(lighten_fg(0.3))` with `SweepPattern::left_to_right` | 800ms loop | SineInOut |
| Task completes ✅ | `evolve_into(Circles)` + `RadialPattern::center()` | 500ms | QuadOut |
| Task fails ✗ | `Glitch::builder().ratio(0.3).delay(2)` | 300ms | Linear |
| Streaming starts | Matrix decrypt: chaos → reveal with `WavePattern` | per-char 40ms | CubicOut |
| Error notification | `parallel(dissolve, fade_from)` with `CellFilter::Inner` | 200ms | ExpoOut |
| Instruments collapse `[` | `slide_out(Right)` + resize | 200ms | QuadInOut |
| Workflow complete | `sweep_in(Bottom)` + `fade_from_fg` on summary bar | 400ms | BounceOut |

### Frame Rates
- **Animations active**: 60 FPS (16ms tick)
- **Idle/typing**: 10 FPS (100ms tick)
- **Background (no focus)**: 4 FPS (250ms tick)

---

## TaskBox v2 Designs (F1 Telemetry Edition)

### INFER BOX — Expanded (running)

```
╭┈ ⚡ INFER ┈ summarize ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈ ⣻ 2.6s ┈╮
┊  🧠 claude-sonnet-4-6          42.7 tok/s ▁▂▃▅▇█▇▅▃▂     TTFT 0.34s    ┊
┊  tokens 1,204 → 847            ████████████████████░░░░ 78%    $0.018    ┊
┊┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┊
┊  ▸ prompt (1,204 tok)                                                    ┊
┊  ▸ thinking (438 tok)                                                    ┊
┊  ▾ response ─────────────────────────────────────────────────────────    ┊
┊  │ The analysis reveals three primary subjects positioned in a          ┊
┊  │ triangular composition. Key observations:                            ┊
┊  │  1. Subject clarity: high (94% confidence)                           ┊
┊  │  2. Composition: rule-of-thirds aligned█                             ┊
╰┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈ 2.6s │ $0.018 ┈╯
```

### FETCH BOX — Expanded (done)

```
╭━ 🛰️ FETCH ━ crawl ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ ✅ 477ms ━╮
┃  POST https://api.example.com/v1/data          200 OK  │  attempt 1/3    ┃
┃━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┃
┃  dns ██░░░░░░░░░░░░░░ 12ms                                               ┃
┃  tls ░░████░░░░░░░░░░ 41ms                                               ┃
┃  ttfb░░░░░░░░████████ 187ms                                              ┃
┃  xfer░░░░░░░░░░░░░░██ 203ms                                              ┃
┃  ▸ headers (6 req / 11 resp)                                              ┃
┃  ▸ body (application/json, 48.2 KB, gzip 4:1)                            ┃
╰━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ 477ms │ 48.2 KB │ 200 OK ━╯
```

### INVOKE BOX — Expanded (done)

```
╭━ 🔌 INVOKE ━ novanet_search ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ ✅ 312ms ━╮
┃  novanet › novanet_search           312ms  ▂▃▅▃▂▄▆▃▂● avg 340ms  MISS   ┃
┃━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┃
┃  ▸ params { query: "AI trends 2026 fact check" }                          ┃
┃  ▾ result ──────────────────────────────────────────────────────────────  ┃
┃  │ { nodes: [{ id: "img_001", label: "ImageAnalysis" }], edges: 7 }     ┃
╰━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ 312ms │ 218B → 1.4KB │ MISS ━━━━━╯
```

### EXEC BOX — Compact (done)

```
╭━ 📟 EXEC ━ publish ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ ✅ exit 0 ━╮
┃  $ npm run deploy --production                           0.84s │ exit 0   ┃
┃  ▸ stdout (12 lines)  ▸ stderr (0 lines)                                 ┃
╰━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ 0.84s │ exit 0 ━╯
```

### AGENT BOX — Running with nested children

```
╭┈ 🐔 AGENT ┈ analyze_and_enhance ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈ ⣻ turn 3/10 ┈╮
┊  tokens ████████████░░░░░░░░ 12,847/32K   $0.042  ▁▂▃▅▇▅▃ per turn      ┊
┊  tools: ⚡×2 🛰️×1 🔌×3 📟×1                              elapsed 14.2s   ┊
┊┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┊
┊  ▸ turn 1  ⚡+🔌  ─────────────────────────────── 2.13s  $0.011         ┊
┊  ▸ turn 2  🛰️+🔌×2 ────────────────────────────── 3.41s  $0.009         ┊
┊  ▾ turn 3  ⚡+📟  ─────────────────────────────────── ⣻ active          ┊
┊  │ reasoning: "Comparing embeddings to identify targets..."              ┊
┊  │ ╭┈ ⚡ infer ── streaming ── 412 tok ── 87 tok/s ┈╮                    ┊
┊  │ ┊  "Enhancement should focus on contrast +12%█"  ┊                    ┊
┊  │ ╰┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈╯                    ┊
┊  │ ╭┈ 📟 exec ── ███████████░░░ 65% ── pid 48312 ┈╮                     ┊
┊  │ ┊  $ convert input.jpg -enhance output.jpg      ┊                     ┊
┊  │ ╰┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈╯                    ┊
╰┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈ 3/10 turns │ 14.2s │ $0.042 │ conf 0.81 │ ⣻ active ┈╯
```

---

## Instruments Panel Designs

### DAG LIVE (workflow running)

```
╭─ DAG ─────────── 4/6 ███░ ─╮
│                              │
│  ╭━━━━━━━━━━━━━━━━━━━━━╮    │
│  ┃ ⣻ ⚡ summarize  62% ┃    │
│  ┃ ▓▓▓▓▓▓▓▓▓▒░░░  2.1s┃    │
│  ╰━━━━━━━┯━━━━━━━━━━━━━╯    │
│     ┌────┴────┐              │
│  ╭══╧═══════╮ ╭══╧══════╮   │
│  ║✅ 🛰️ a  ║ ║✅ 📟 b  ║   │
│  ║    1.2s  ║ ║    0.3s  ║   │
│  ╰══════════╯ ╰═════════╯   │
│     └────┬────┘              │
│  ╭┄┄┄┄┄┄┴┄┄┄┄┄┄┄┄┄┄┄┄╮     │
│  ┆ ⚪ 🔌 invoke_mcp   ┆     │
│  ╰┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄╯     │
│          │                   │
│  ╭┄┄┄┄┄┄┴┄┄┄┄┄┄┄┄┄┄┄┄╮     │
│  ┆ ⚪ ⚡ final_infer   ┆     │
│  ╰┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄╯     │
│                              │
╰──────────────────────────────╯
```

### METRICS (F1 steering wheel pattern)

```
╭─ METRICS ────────────────────╮
│                               │
│     ⏱  00:06.847             │
│                               │
│  Tasks    ████░░ 4/6   66%   │
│  ✅3  ⣻1  ⚪2                 │
│ ─────────────────────────── │
│  THROUGHPUT                   │
│  42.7 tok/s                   │
│  ▁▂▃▅▇█▇▅▆▇███▇▅▃▂▁▂       │
│  peak: 68   avg: 39          │
│ ─────────────────────────── │
│  COST          $0.042        │
│  in   ████████░░  3,841      │
│  out  ███░░░░░░░  1,203      │
│  rate $0.37/min              │
│ ─────────────────────────── │
│  LATENCY  p50:320 p99:1.8s  │
│  ▂█▇▅▃▂▁                    │
│                               │
╰───────────────────────────────╯
```

### MCP STATUS (network operations center)

```
╭─ MCP ────────────────────────╮
│                               │
│  ● novanet       7 tools     │
│    12 calls   avg 340ms      │
│    ▁▂▃▂▁▃▅▇▅▃▂▁▂▃▅▇▅▃▂▁    │
│                               │
│  ● perplexity    1 tool      │
│    3 calls    avg 1.2s       │
│    ░░░░░░░░░░░░▁▃▅█▅▃       │
│                               │
│  ○ redis-cache   offline     │
│    last seen: 2m ago         │
│ ─────────────────────────── │
│  ACTIVE  ◉ 1                 │
│  ⣻ novanet_search  2.1s     │
│                               │
╰───────────────────────────────╯
```

### TIMELINE (F1 Gantt-style)

```
╭─ TIMELINE ───────────────────╮
│                        6.847 │
│  0s   2s   4s   6s   8s     │
│  ├────┼────┼────┼────┤      │
│                        ┊     │
│  🛰️ fetch_page               │
│  ████████░░░░░░░░░░░░░┊     │
│                        ┊     │
│  🛰️ fetch_api                │
│  ██████░░░░░░░░░░░░░░░┊     │
│                        ┊     │
│  ⚡ extract                   │
│  ░░░░░░████████░░░░░░░┊     │
│                        ┊     │
│  🔌 check_facts              │
│  ░░░░░░░░░░████████▒▒▒┊     │
│                        ┊     │
│  ⚡ summarize                 │
│  ░░░░░░░░░░░░░░████▒▒▒┊     │
│                        ┊     │
│  📟 publish                   │
│  ░░░░░░░░░░░░░░░░░░░░░┊     │
│                              │
╰───────────────────────────────╯
```

---

## Status Bar (Always Visible — "Eyes Forward")

```
┌──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ◉ ORBITAL │ 4/6 tasks │ ⚡2 🛰️2 🔌1 📟0 │ 🧠 sonnet-4-6 │ 🔢 5.1k │ 💰 $0.042 │ ⏱ 6.8s │ MCP ● ●          │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

Elements (left to right):
1. **Mission phase**: ◉ ORBITAL (amber pulse when running, green when done, red on error)
2. **Task count**: 4/6 tasks
3. **Verb breakdown**: icon + count per verb type
4. **Active model**: provider icon + model name
5. **Token count**: total tokens (compact: 5.1k)
6. **Cost**: cumulative cost
7. **Elapsed time**: workflow duration
8. **MCP status**: dots per connected server

---

## Responsive Layout

```
COMPACT (< 80 cols):
┌─ Conversation only ──────────────┐
│ (instruments hidden, use [ key)  │
└──────────────────────────────────┘

STANDARD (80-120 cols):
┌─ Conversation ──┬─ Instruments ──┐
│      65%        │     35%        │
└─────────────────┴────────────────┘

WIDE (> 120 cols):
┌─ Conversation ──────┬─ Instruments ──┐
│        65%          │      35%       │
│                     │ (more detail)  │
└─────────────────────┴────────────────┘
```

---

## Dependencies to Add

```toml
[dependencies]
tachyonfx = { version = "0.25.0", optional = true, features = ["dsl"] }

[features]
tui = ["dep:ratatui", "dep:crossterm", "dep:tachyonfx", ...]
```

---

## Research Documents

All research is saved in `docs/research/`:

| File | Content |
|------|---------|
| `f1-telemetry-dashboard-patterns.md` | F1 pit wall, timing tower, Gantt, steering wheel patterns |
| `2026-03-20-mission-control-ui-patterns.md` | NASA MCC, SpaceX, ISS, glass cockpit patterns |
| `terminal-ui-visual-techniques.md` | eDEX-UI, CRT effects, sci-fi FUI, Unicode graphics |
| `research-terminal-animation-effects.md` | tachyonfx 40+ effects, Unicode resolution, Braille charts |
| `2026-03-20-realtime-monitoring-observability-ux.md` | Grafana, Datadog, Honeycomb, terminal monitoring |
| `ratatui-architecture-patterns-2025.md` | Component trait, Action enum, overlay pattern |
| `ratatui-component-architecture-2025.md` | Official template, responsive layout, tachyonfx integration |
| `2026-03-20-evangelion-cosmic-hacker-tui-aesthetic.md` | Evangelion NERV, anime terminals, color palettes |
| `tui-orchestrator-ux-patterns.md` | Chat-as-orchestrator, Claude Code UX, mission control |
