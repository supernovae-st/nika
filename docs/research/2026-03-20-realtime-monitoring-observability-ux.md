# Real-Time Monitoring & Observability Dashboard UX Patterns

> Research for Nika TUI Runner view -- visualizing task execution timelines, token throughput, cost accumulation, MCP call latency, DAG progress, and streaming rates in real-time.

**Date**: 2026-03-20
**Scope**: Grafana, Datadog APM, Honeycomb.io, terminal-based tools (btop, k9s, lazydocker, vegeta), visualization patterns, alert design

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Grafana Dashboard Patterns](#grafana-dashboard-patterns)
3. [Datadog APM Visualization](#datadog-apm-visualization)
4. [Honeycomb.io Trace Views](#honeycombio-trace-views)
5. [Terminal-Based Monitoring Tools](#terminal-based-monitoring-tools)
6. [Real-Time Visualization Patterns](#real-time-visualization-patterns)
7. [Alert & Notification Design](#alert--notification-design)
8. [LLM-Specific Monitoring Patterns](#llm-specific-monitoring-patterns)
9. [Unicode Rendering Techniques](#unicode-rendering-techniques)
10. [Ratatui Widget Mapping](#ratatui-widget-mapping)
11. [Synthesis: Nika TUI Runner Recommendations](#synthesis-nika-tui-runner-recommendations)

---

## Executive Summary

The best real-time monitoring tools share these principles:

1. **Z-pattern layout** -- Critical metrics top-left, details flow right and down
2. **Match visualization to data type** -- Trends get time series, single values get stat panels, distributions get heatmaps
3. **Progressive disclosure** -- Overview first, drill-down on demand
4. **Color as information** -- Severity-coded (green/yellow/red), not decorative
5. **Minimize cognitive load** -- Group related metrics, suppress noise, show rate-of-change

For a terminal context, add:
- **Braille characters (U+2800-28FF)** for 2x4 sub-cell resolution graphs
- **Sparklines** for inline trend indicators
- **Partial screen updates** to avoid flicker
- **Event-driven rendering** tied to data arrival, not fixed FPS

---

## Grafana Dashboard Patterns

### Panel Type Selection Matrix

| Metric Type | Panel | When to Use | Nika Equivalent |
|---|---|---|---|
| Single current value | **Stat panel** | CPU load, error rate, active tasks | Task count, total cost, tok/s |
| Bounded range | **Gauge** | Memory %, battery, SLO budget | Cost budget remaining, DAG % |
| Trend over time | **Time series** | Traffic, latency, throughput | Token throughput, streaming rate |
| Distribution/density | **Heatmap** | Error rates by hour, latency buckets | MCP call latency distribution |
| Status/alerts | **Stat + thresholds** | Service health, alert state | Step pass/fail, provider status |

### Layout Best Practices

```
Z-Pattern Reading Flow:
+------------------+------------------+
| STAT: Key metric | STAT: Key metric |  <-- Glanceable KPIs (top row)
+------------------+------------------+
| TIME SERIES: Primary trend          |  <-- Main visualization (middle)
+-------------------------------------+
| HEATMAP / TABLE: Detail breakdown   |  <-- Supporting detail (bottom)
+-------------------------------------+
```

Key rules:
- **Top row**: Large stat panels for the 2-4 most important numbers
- **Middle**: Time series for the primary trend the user is monitoring
- **Bottom**: Detail tables, heatmaps, logs for drill-down
- **Consistent spacing and sizing** establishes visual hierarchy
- **Refresh rate matches data cadence** -- don't poll at 1s for data that updates every 30s
- **Reuse queries** -- dashboard-level variables, not per-panel duplication
- **Library panels** for reusable components across dashboards

### Alert Integration in Panels

- Panels change **background color** on threshold breach (green -> yellow -> red)
- **Annotations** mark events directly on time series (deploy markers, incident start)
- **Stat panels** show colored value + optional sparkline for trend context
- Link panels to detailed views via **template variables and URLs**

---

## Datadog APM Visualization

### Trace Visualization

Datadog uses a **flame graph** as the primary trace visualization:

```
Flame Graph (horizontal bars, depth = call stack):
|----- service-a: /api/users (45ms) -------------------|
  |-- auth-service: validate (12ms) --|
  |---- db-service: query (28ms) --------------------|
    |-- postgres: SELECT (25ms) ------------------|
```

Key design choices:
- **Horizontal bars** proportional to duration
- **Depth** represents call hierarchy (parent -> child)
- **Color** encodes service identity (each service gets a consistent color)
- **Clickable spans** drill into metadata (tags, errors, logs)

### Latency Distribution

- **Histograms** on service dashboards showing request duration buckets
- **P50/P95/P99 lines** overlaid on time series
- Color-coded: green (normal), yellow (elevated), red (breaching SLO)

### Service Maps

- **Directed graph** of service dependencies
- **Edge thickness** = request volume
- **Node color** = health status (green/yellow/red)
- **Volume bars** show request percentage per path
- Clickable nodes drill to traces, metrics, monitors

### Error Rate Visualization

- Dedicated graphs on service-level dashboards
- **Color-coded indicators** on service maps: red dot = errors
- Derived from 100% of traffic (not sampled) via APM metrics
- **Watchdog** AI overlays anomaly detection markers

### Patterns Applicable to Nika

| Datadog Pattern | Nika Application |
|---|---|
| Flame graph | DAG step execution timeline (horizontal bars per step) |
| Service map | Workflow DAG visualization (nodes = steps, edges = deps) |
| Latency histogram | MCP call latency distribution |
| P50/P95/P99 overlay | Token generation speed percentiles |
| Error rate graph | Step failure rate over workflow runs |

---

## Honeycomb.io Trace Views

### Waterfall View

Honeycomb's signature visualization -- a **waterfall of spans**:

```
Waterfall (vertical timeline, horizontal bars for duration):
[0ms]  |====== fetch: GET /api (120ms) ==================|
[5ms]    |=== auth: validate_token (30ms) ===|
[35ms]   |============ db: query (80ms) =============|
[40ms]      |====== pg: SELECT (70ms) ===========|
[115ms]  |= serialize: json (5ms) =|
```

Design choices:
- **Time axis runs left-to-right** within each span
- **Vertical stacking** shows parent-child and sequential relationships
- **Delta time (dt)** shown between spans for gap analysis
- **Trace zoom** -- click a subtree to expand, sharable via permalink
- **Span summaries** for long-running spans collapse detail
- **Span links** connect related traces across services

### Latency Heatmap

- Generated from queries like `HEATMAP(duration_ms)`
- **X-axis**: time, **Y-axis**: latency buckets, **Color intensity**: event count
- **Clickable regions** drill into matching traces
- Reveals patterns invisible in averages (bimodal distributions, periodic spikes)

### BubbleUp Analysis

- Honeycomb's killer feature: select outlier region on heatmap
- System automatically surfaces **which attributes differ** between outliers and baseline
- High-cardinality filtering without predefined schemas
- Fluid transitions: heatmap -> graph -> trace -> raw events

### Session Flow

- Aggregates multiple short traces into user journeys
- Uses `trace.id` + delta time to reconstruct sequences
- Shows frontend interactions (clicks, navigations) as connected spans

### Patterns Applicable to Nika

| Honeycomb Pattern | Nika Application |
|---|---|
| Waterfall view | Sequential step execution in a workflow |
| Latency heatmap | Step duration distribution across runs |
| BubbleUp | Surfacing why certain workflow runs are slow |
| Session flow | Multi-workflow execution sequences |
| Trace zoom | Expand/collapse DAG subtrees in TUI |

---

## Terminal-Based Monitoring Tools

### btop / bottom

**Layout architecture** (btop):

```
+--[1: CPU]------------------------------------------+
| CPU% ██████████████████░░░░░░░░░░░ 68%            |
| Per-core sparklines: ▁▂▅▃▇▅▂▁  ▃▅▇▅▃▂▁▁         |
| Historical graph (braille): ⣿⣷⣶⣴⣤⣠⡀⠀⣀⣠⣤⣴⣶⣷⣿      |
+---[2: MEM]---+---[3: NET]---+---[4: DISK]---------+
| Used: 12.4G  | Up: 1.2MB/s  | Read:  45MB/s      |
| Total: 32G   | Dn: 8.5MB/s  | Write: 12MB/s      |
| ████████░░░  | ▁▂▃▅▇▅▃▂    | ▅▃▂▁▁▂▃▅          |
+--------------+--------------+---------------------+
+--[5: PROCESSES]------------------------------------+
| PID   USER    CPU%  MEM%  NAME                     |
| 1234  root    45.2  12.1  python3                  |
| 5678  thibaut  8.3   4.5  nika                     |
+----------------------------------------------------+
```

Key techniques:
- **Braille patterns (U+2800-28FF)** for smooth high-resolution graphs in CPU/memory history
- **Block elements (U+2580-259F)** for progress bars (upper/lower half blocks)
- **Sparkline characters** `▁▂▃▄▅▆▇█` for compact per-core CPU history
- **24-bit truecolor** with automatic degradation to 256-color and 16-color TTY
- **6 numbered regions** togglable via keypress (1-5, d)
- **Multiple layout presets** cyclable with `p`
- **Real-time graph smoothness** via continuous braille character updates

### k9s (Kubernetes)

**Grid layout pattern**:

```
+--[Context: prod-cluster]--[NS: default]--[View: Pods]-+
| NAME              READY  STATUS   RESTARTS  AGE  CPU  |
| api-server-7f8d   1/1    Running  0         2d   120m |
| worker-a9c2       1/1    Running  0         2d   80m  |
| db-primary-3e1f   1/1    Running  0         5d   200m |
| cache-redis-8b4a  0/1    CrashLoop 5       1h   50m  |  <-- RED ROW
+--------------------------------------------------------+
| [Logs] [Describe] [Shell] [Port-Forward] [Delete]      |
+--------------------------------------------------------+
```

Key patterns:
- **Sortable, filterable table** as primary view
- **Color-coded rows** by status (green=Running, red=CrashLoop, yellow=Pending)
- **Keyboard-driven navigation** (vim-style: j/k/g/G)
- **Context breadcrumb** at top (cluster > namespace > resource)
- **Action bar** at bottom with available operations
- **Live updates** -- rows add/remove/recolor as pod states change

### lazydocker

**Split-pane layout**:

```
+--[Containers]-------+--[Container Detail]-------------+
| > api-server  UP    | Logs:                            |
|   worker      UP    | 2024-01-15 10:23:45 Processing...|
|   redis       UP    | 2024-01-15 10:23:46 Done (200ms) |
|   postgres    DOWN  | Stats:                           |
+---------------------| CPU: ████░░░░ 45%                |
+--[Images]-----------| MEM: ██████░░ 72%                |
| node:18  1.2GB      | NET: 1.2MB/s in, 0.8MB/s out     |
| redis:7  125MB      +----------------------------------+
+---------------------+
```

Key patterns:
- **Master-detail layout** -- list on left, detail on right
- **Status indicators** in list: UP (green), DOWN (red), PAUSED (yellow)
- **Inline metrics** in detail pane: CPU/MEM bars, network rates
- **Log streaming** in detail pane with auto-scroll
- **Tab switching** between Logs/Stats/Config views

### vegeta / hey (HTTP Load Testing)

**vegeta output patterns**:

```
Requests      [total, rate, throughput]  1000, 100.10, 99.87
Duration      [total, attack, wait]     10.013s, 9.99s, 23.045ms
Latencies     [min, mean, 50, 90, 95, 99, max]  1.2ms, 15.3ms, 12ms, 25ms, 45ms, 120ms, 350ms
Bytes In      [total, mean]             1250000, 1250.00
Bytes Out     [total, mean]             0, 0.00
Success       [ratio]                   98.70%
Status Codes  [code:count]              200:987  500:13

Latency Distribution (histogram):
  0ms  [===                         ] 5%
  5ms  [============                ] 25%
 10ms  [====================        ] 42%
 20ms  [========                    ] 18%
 50ms  [===                         ] 7%
100ms  [=                           ] 3%
```

Key patterns:
- **Summary statistics first** (total, rate, throughput)
- **Percentile breakdown** (p50, p90, p95, p99, max) for latency
- **ASCII histogram** for distribution visualization
- **Status code breakdown** with counts
- **Success ratio** as a single prominent number
- `vegeta plot` generates HTML latency plots; `vegeta encode` for JSON streaming

---

## Real-Time Visualization Patterns

### Flame Charts (Execution Traces)

```
Flame Chart (x-axis = time, y-axis = depth):
|== step_1: fetch_data (200ms) ==============================|
  |= http_get (150ms) ==========================|
    |= dns_resolve (20ms) ==|
    |========= tcp_connect + tls (80ms) =========|
    |=== read_response (50ms) ===|
  |= parse_json (50ms) ===|
```

Best practices:
- X-axis is always wall-clock time (not self-time)
- Width proportional to duration
- Color by: module/service (categorical) or hot/cold (performance)
- Zoom + pan for large traces
- Search/filter to highlight specific function names
- **Terminal adaptation**: Use horizontal bars with `=` or `━`, depth via indentation

### Waterfall Diagrams (Sequential Operations)

```
Waterfall (each row = one operation):
dns_resolve   |███|                              12ms
tcp_connect        |██████|                      25ms
tls_handshake             |████████|             35ms
http_request                        |██|          8ms
http_response                          |████|    18ms
                   0    25    50    75   100ms
```

Best practices:
- One row per operation
- Horizontal bar starts at operation start time, width = duration
- Gaps between bars reveal wait time / scheduling overhead
- Color by operation type or status
- Show **critical path** highlighted
- **Terminal adaptation**: Fixed-width bars with `█` characters, aligned to time columns

### Gantt-Style Timelines (Parallel Execution)

```
Gantt (parallel tasks on shared timeline):
step_1 [infer]  |████████████████|
step_2 [fetch]  |██████|
step_3 [exec]          |████████████████████████|
step_4 [infer]                   |████████████████████|
step_5 [mcp]                                          |████|
                0s      1s      2s      3s      4s     5s
```

Best practices:
- One row per task/step
- Concurrent tasks visually overlap on time axis
- Color by: verb type, status, or resource
- Show dependencies as arrows or lines between bars
- **Critical path** highlighted (longest sequential chain)
- **Terminal adaptation**: Per-row bars, verb label prefix, status suffix (checkmark/cross)

### Sparklines (Inline Trends)

```
Token throughput: 142 tok/s ▁▂▃▅▇▅▃▂▃▅▇█▇▅
Cost rate:        $0.003/s  ▁▁▂▂▃▃▄▅▅▆▇▇██
MCP latency:      23ms avg  ▃▂▂▁▃▅▇▃▂▁▁▂▃▂
```

Characters: `▁▂▃▄▅▆▇█` (U+2581-2588, block elements)

Best practices:
- Fixed width (8-20 characters)
- Rightmost = most recent
- Pair with current numeric value
- No axes needed -- trend is the information
- Update by shifting left, appending new value

### Rate Meters (Throughput)

```
Token rate:  [████████████░░░░░░░░] 142/s  (+12/s)
Network:     [██████░░░░░░░░░░░░░░]  1.2 MB/s
Cost:        $0.0234  [+$0.003/s]  budget: 48% remaining
```

Best practices:
- Bar shows current rate relative to peak or target
- Numeric value always visible
- **Rate-of-change indicator**: `+12/s` or arrow `^` / `v`
- For cost: show accumulated total + rate + budget remaining
- Color thresholds: green (normal), yellow (elevated), red (budget alert)

### Cost Accumulators with Rate-of-Change

```
Cost: $1.234 (+$0.003/s)  Budget: ████████░░ 78% left
      ▁▁▂▂▃▃▄▅▅▆  rate trend

Breakdown:
  infer:  $0.892 (72%)  ████████████████████░░░░
  fetch:  $0.234 (19%)  █████░░░░░░░░░░░░░░░░░░
  invoke: $0.108  (9%)  ██░░░░░░░░░░░░░░░░░░░░░
```

Best practices:
- **Running total** prominently displayed
- **Rate** as secondary metric with trend sparkline
- **Budget bar** showing remaining allocation
- **Breakdown by category** (verb type, provider, model)
- Color escalation as budget depletes (green -> yellow -> red)
- **Forecast line** if rate continues (projected total at end)

---

## Alert & Notification Design

### Severity Color System

| Level | Color | Terminal ANSI | Usage |
|---|---|---|---|
| OK / Success | Green | `\x1b[32m` | Completed steps, healthy metrics |
| Info | Blue/Cyan | `\x1b[36m` | Status updates, progress |
| Warning | Yellow | `\x1b[33m` | Elevated latency, approaching limits |
| Error | Red | `\x1b[31m` | Failed steps, API errors |
| Critical | Red + Bold | `\x1b[1;31m` | Budget exceeded, total failure |

### Visual Indicator Patterns

**Grafana approach**:
- Panel background color changes on threshold breach
- Annotations mark events on time series
- Stat panels show colored values

**Datadog approach**:
- Red/yellow/green dots on service maps
- Numeric badges with counts ("3 critical")
- Watchdog AI anomaly markers

**Terminal adaptations**:

```
Status indicators:
  [OK]  Step completed successfully        (green)
  [..]  Step in progress                   (cyan, with spinner)
  [!!]  Step failed                        (red, bold)
  [--]  Step skipped                       (dim/gray)
  [??]  Step waiting for dependency        (yellow)

Inline alerts:
  Token rate: 142/s ▁▂▃▅▇  WARN: rate declining
  Cost: $4.82 (+$0.05/s)   CRIT: 96% of budget used

Toast-style (bottom of screen):
  ┌─ WARNING ──────────────────────────────┐
  │ MCP call latency spike: 450ms (p99)    │
  │ Provider: openai | Model: gpt-4        │
  └────────────────────────────────────────┘
```

### Best Practices for Alert UX

1. **Reduce fatigue**: Suppress duplicate alerts, group by source, dependency-aware (ignore downstream if upstream failed)
2. **Actionable**: Include context -- what happened, what to do
3. **Escalation**: Info -> Warning -> Error -> Critical (don't jump levels)
4. **Non-blocking**: Info/warning as inline indicators; only errors interrupt flow
5. **Persistence**: Errors stay visible until acknowledged; warnings auto-dismiss after resolution
6. **Sound**: Optional terminal bell (`\x07`) for critical only; configurable
7. **Accessibility**: Never rely on color alone -- use prefixes like `[OK]`, `[!!]`, `[CRIT]`

---

## LLM-Specific Monitoring Patterns

### Token Throughput Visualization

From Helicone, vLLM, and similar tools:

```
Model: gpt-4o                    Provider: openai
Tokens:  prompt=1,234  completion=567  total=1,801
Rate:    142 tok/s ▁▂▃▅▇▅▃▂▃▅▇█▇▅
Cost:    $0.0234  (+$0.003/s)
Latency: TTFT=234ms  total=4.2s
```

Key metrics to track:
- **TTFT** (Time To First Token) -- user-perceived responsiveness
- **Tokens/second** -- generation throughput
- **Prompt vs completion tokens** -- cost driver visibility
- **Cost per request** and **accumulated cost**
- **Streaming progress** -- tokens received / estimated total

### Multi-Provider Comparison

```
Provider    Model       TTFT    Tok/s   Cost/1k  Status
openai      gpt-4o      234ms   142     $0.015   [OK]
anthropic   claude-4    189ms   167     $0.012   [OK]
mistral     large       456ms    98     $0.008   [!!]
```

### Streaming Output Display

```
Step: infer "Summarize the data"
Status: streaming... 234/~500 tokens  [████████░░░░░░░░] ~47%

> The analysis reveals three key trends in the data.
> First, user engagement has increased by 23% over the
> past quarter, driven primarily by mobile users who_
                                                    ^ cursor/spinner
```

Pattern: Show streaming text with a visible cursor, token counter above, estimated progress bar if total is predictable.

---

## Unicode Rendering Techniques

### Character Sets for Terminal Graphs

**Sparkline characters** (U+2581-2588):
```
▁ ▂ ▃ ▄ ▅ ▆ ▇ █
```
8 levels of vertical fill. One character = one data point. Ideal for inline trends.

**Block elements** (U+2580-259F):
```
▀ ▄ █ ░ ▒ ▓   (upper half, lower half, full, light/medium/dense shade)
```
Good for progress bars and simple area fills.

**Braille patterns** (U+2800-28FF):
```
Dot positions in a 2x4 grid:
  1 4
  2 5
  3 6
  7 8

Each character = 8 independently addressable dots
= 2 pixels wide x 4 pixels tall per character cell
= Effective 2x resolution horizontal, 4x resolution vertical
```

256 possible patterns. Used by btop, plotille, drawille for high-resolution graphs.

**Box drawing** (U+2500-257F):
```
─ │ ┌ ┐ └ ┘ ├ ┤ ┬ ┴ ┼   (single line)
━ ┃ ┏ ┓ ┗ ┛ ┣ ┫ ┳ ┻ ╋   (heavy line)
╔ ╗ ╚ ╝ ║ ═                (double line)
```
Used for borders, panels, tables.

### Resolution Comparison

| Technique | Horizontal Res | Vertical Res | Use Case |
|---|---|---|---|
| ASCII (`#`, `=`) | 1x | 1x | Fallback, maximum compatibility |
| Block elements | 1x | 2x | Progress bars, simple charts |
| Sparklines | 1x | 8x | Inline trend indicators |
| Braille | 2x | 4x | High-res graphs, heatmaps, plots |

### Color Degradation Strategy

```
24-bit truecolor  ->  256-color  ->  16-color  ->  no color
    (modern terms)    (xterm-256)    (basic TTY)   (pipe/redirect)
```

btop's approach: auto-detect capability, gracefully degrade. Always ensure readability without color (use structure, not just hue).

---

## Ratatui Widget Mapping

### Widget Selection for Nika Metrics

| Nika Metric | Ratatui Widget | Configuration |
|---|---|---|
| DAG progress (%) | `Gauge` | `.ratio(progress)`, color thresholds |
| Token throughput trend | `Sparkline` | `.data(&history)`, 20-40 points |
| Step execution timeline | `BarChart` (horizontal) or `Canvas` | Custom horizontal bars per step |
| Cost accumulator | `Paragraph` (styled) | Formatted text with colored rate |
| MCP latency distribution | `BarChart` (vertical) | Histogram buckets |
| Step status table | `Table` | Colored rows by status |
| Streaming output | `Paragraph` with scroll | Auto-scroll, token counter |
| Active step spinner | `Paragraph` with timer | Cycle spinner chars: `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` |
| Overall layout | `Layout` | Vertical split: KPIs / timeline / detail |
| Log stream | `List` | Scrollable, auto-follow tail |

### Recommended Layout Structure

```rust
// Vertical 3-section layout
let sections = Layout::default()
    .direction(Direction::Vertical)
    .constraints([
        Constraint::Length(3),    // KPI bar (stat panels)
        Constraint::Min(10),     // Main: timeline + sparklines
        Constraint::Length(8),   // Detail: logs + streaming output
    ])
    .split(area);

// KPI bar: horizontal split for stat panels
let kpis = Layout::default()
    .direction(Direction::Horizontal)
    .constraints([
        Constraint::Percentage(25),  // DAG progress gauge
        Constraint::Percentage(25),  // Token rate + sparkline
        Constraint::Percentage(25),  // Cost accumulator
        Constraint::Percentage(25),  // Active step + elapsed
    ])
    .split(sections[0]);

// Main area: left timeline, right detail
let main = Layout::default()
    .direction(Direction::Horizontal)
    .constraints([
        Constraint::Percentage(60),  // Gantt timeline
        Constraint::Percentage(40),  // Step detail / latency chart
    ])
    .split(sections[1]);
```

### Spinner Characters (Braille-based)

```
Frames: ⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏
Cycle at ~80ms per frame = smooth rotation
```

Alternative sets:
```
Dots:    ⣾ ⣽ ⣻ ⢿ ⡿ ⣟ ⣯ ⣷
Line:    | / - \
Braille: ⠁ ⠂ ⠄ ⡀ ⢀ ⠠ ⠐ ⠈
Arc:     ◜ ◝ ◞ ◟
```

---

## Synthesis: Nika TUI Runner Recommendations

### Layout: Three-Tier Dashboard

```
Tier 1 -- KPI Bar (always visible, 3 rows):
┌─ DAG ──────┬─ Tokens ──────┬─ Cost ─────────┬─ Step ─────────┐
│ ████░░ 4/7 │ 142 tok/s     │ $0.023         │ step_3 [infer] │
│ 57%        │ ▁▂▃▅▇▅▃▂▃▅▇ │ +$0.003/s      │ 2.3s elapsed   │
└────────────┴───────────────┴────────────────┴────────────────┘

Tier 2 -- Timeline (scrollable, main content):
step_1 [fetch]  ✓ |████|                           0.2s
step_2 [infer]  ✓      |████████████████████|       2.1s
step_3 [infer]  ⠹      |████████████░░░░░░░░|       1.8s...
step_4 [exec]   ·                                   waiting
step_5 [mcp]    ·                                   waiting
                 0s      1s      2s      3s      4s

Tier 3 -- Detail Panel (context-sensitive):
┌─ step_3: infer "Summarize findings" ─────────────────────┐
│ Provider: anthropic  Model: claude-4  Tokens: 234/~500   │
│ > The analysis reveals three key patterns...             │
│ > First, the correlation between_                        │
└──────────────────────────────────────────────────────────┘
```

### Metric Visualization Choices

| Metric | Visualization | Update Frequency |
|---|---|---|
| DAG progress | Gauge bar + fraction (4/7) | On step state change |
| Token throughput | Number + sparkline (last 20 samples) | Every 500ms |
| Cost | Running total + rate + sparkline | Every 1s |
| Step timeline | Gantt bars (horizontal, proportional) | Every 500ms |
| Step status | Icon: `✓` done, `⠹` running, `✗` failed, `·` pending | On state change |
| Streaming output | Paragraph with auto-scroll + cursor | On token arrival |
| MCP latency | Inline number + sparkline in step row | On call completion |
| Elapsed time | Live counter in KPI bar | Every 100ms |
| Error state | Red row + `[!!]` prefix + detail in Tier 3 | On error |

### Color Palette

```
Verb colors (consistent identification):
  infer:  Magenta/Purple   -- LLM operations
  fetch:  Cyan             -- Network operations
  exec:   Yellow           -- Shell operations
  invoke: Blue             -- MCP operations
  agent:  Green            -- Multi-turn loops

Status colors:
  Success:    Green
  Running:    Cyan (with spinner)
  Warning:    Yellow
  Error:      Red
  Pending:    Dim/Gray
  Skipped:    Dim/Gray + strikethrough
```

### Interaction Model

Inspired by k9s and btop:
- **Keyboard-driven**: `j/k` navigate steps, `Enter` expands detail, `q` quits
- **Tab switching**: Between timeline view, log view, metrics view
- **Live filtering**: `/` to search/filter steps
- **Toggle panels**: Number keys toggle KPI bar, timeline, detail
- **Auto-follow**: Detail panel follows currently-running step (toggle with `f`)

### Performance Considerations

- **Event-driven rendering**: Render on data change, not fixed interval
- **Partial updates**: Only redraw changed regions (ratatui handles diffing)
- **Ring buffers** for sparkline history (fixed-size, no allocation)
- **Channel-based architecture**: Worker threads push metrics via `mpsc`, UI thread renders
- **Frame budget**: Target 30fps for smooth spinners, but skip frames under load

---

## Sources

1. Grafana Dashboard Best Practices 2024-2025 -- Panel types, Z-pattern layout, refresh rate optimization
2. Datadog APM Documentation -- Flame graphs, service maps, latency histograms, Watchdog anomaly detection
3. Honeycomb.io Trace Documentation -- Waterfall views, latency heatmaps, BubbleUp analysis, session flow
4. btop GitHub / Documentation -- Braille rendering, 6-region layout, color degradation
5. k9s Documentation -- Kubernetes pod grid, status coloring, keyboard navigation
6. lazydocker -- Master-detail split pane, inline metrics
7. vegeta/hey -- HTTP load testing output patterns, latency histograms
8. Helicone, vLLM, LiteLLM, LangSmith -- LLM-specific monitoring dashboards
9. Ratatui Documentation -- Widget catalog, layout system, Canvas/Sparkline/Gauge widgets
10. Unicode Standard -- Braille Patterns (U+2800-28FF), Block Elements (U+2580-259F), Box Drawing (U+2500-257F)
11. plotille, drawille -- Braille-based terminal graphing libraries

---

## Methodology

- **Tools used**: Perplexity AI search (8 queries across all topics)
- **Sources analyzed**: 30+ across documentation, tutorials, and tool comparisons
- **Time period covered**: 2024-2026 (current best practices)
- **Confidence level**: High -- patterns are well-established and consistent across tools

## Further Research Suggestions

- Deep-dive into ratatui `Canvas` widget for custom Gantt chart rendering
- Benchmark braille rendering performance in ratatui at high update rates
- Study `indicatif` crate for non-TUI progress bar patterns (useful for `nika run` without TUI)
- Analyze `tracing-flame` crate for generating flame chart data from Nika's tracing spans
- Research `tui-logger` integration for structured log display in Tier 3
