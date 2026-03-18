# CLI Output Design Research: World-Class Workflow Runner UX

**Date**: 2026-03-18
**Goal**: Make `nika run` output best-in-class, inspired by the finest CLI tools in 2025-2026.

---

## 1. Industry Landscape Analysis

### 1.1 Cargo (Rust's own tooling)

**What it looks like:**
```
   Compiling serde v1.0.210
   Compiling tokio v1.42.0
   Compiling nika v0.12.0 (/Users/thibaut/dev/nika)
    Finished `release` profile [optimized] target(s) in 45.2s
```

**Key patterns worth copying:**
- **Right-aligned verb labels** (`Compiling`, `Finished`) in green/bold -- creates a clean left gutter
- **Verb + target** on each line, no noise
- **Single summary line** with profile, target, and total time
- **Color-coded verbs**: `Compiling` green, `warning` yellow, `error` red
- Progress bar only shows when there are many crates: `[==================>   ] 156/203`

**What makes it great:**
- Extreme information density with zero clutter
- The right-aligned verbs create a scannable left margin
- Duration only at the end, not per-crate (avoids noise)

**What makes it mediocre:**
- No per-task timing (you need `cargo build --timings` separately)
- Progress bar is basic (no ETA)


### 1.2 Dagger.io (Pipeline/DAG runner)

**What it looks like:**
```
  dagger run go run ./dagger
  ┣━━┓
  ┃  ████████  ls          (2.1s)
  ┃  ┣━━ install-deps      (12.3s)
  │  │
  ▼ ubuntu
     ┃
     ████████  build        (8.7s)
     │
     ┗━━ test               (3.2s)
```

**Key patterns worth copying:**
- **Live DAG tree** with `┃` `┣` `┗` Unicode box-drawing
- **Blinking animation** `(chars)` on active operations
- **Split-screen TUI**: DAG tree on top, streaming logs on bottom
- **Forks marked with bold names** at branch points
- `--progress=plain` for CI (no TUI, just lines)

**What makes it great:**
- You can SEE the DAG shape in real-time
- Parallel branches are visually obvious
- Completed tasks collapse, active ones blink

**What makes it mediocre:**
- Can be overwhelming for simple pipelines
- TUI collapses on success (hard to inspect after)


### 1.3 Turborepo (Parallel task runner)

**What it looks like:**
```
turbo 2.4.4

 Packages in scope: @repo/api, @repo/ui, @repo/shared, web
 Running build in 4 packages
 Remote caching enabled

@repo/shared:build: cache hit, replaying logs 4e6f3a2b1c...
@repo/shared:build:
@repo/shared:build: > shared@1.0.0 build
@repo/shared:build: > tsc
@repo/shared:build:

@repo/api:build: cache miss, executing 8a2b3c4d5e...
@repo/api:build:
@repo/api:build: > api@1.0.0 build
@repo/api:build: > tsc && vite build
@repo/api:build: src/index.ts -> dist/index.js  (2.1s)

 Tasks:    4 successful, 4 total
 Cached:   1 cached, 4 total
   Time:    2.3s
```

**Key patterns worth copying:**
- **Prefixed log lines**: `@repo/api:build:` prefix on every line -- you always know which task produced what
- **Cache status per task**: `cache hit` vs `cache miss` with hash
- **Clean 3-line summary**: Tasks / Cached / Time
- **Grouped output mode**: each task's output is a contiguous block

**What makes it great:**
- The prefix system is genius for parallel output (never lose context)
- Cache hits are instant visual reward ("free speed")
- The summary is scannable in <1 second

**What makes it mediocre:**
- Verbose for many packages (lots of repeated prefixes)
- No per-task timing in default mode


### 1.4 Nx (Monorepo task runner)

**What it looks like:**
```
 NX   Running target build for 8 projects:

- shared-utils
- ui-components
- api-gateway
- web-app
...

 NX   Running target build for project shared-utils   [1/8]
      shared-utils:build  (1.2s)

 NX   Running target build for project ui-components  [2/8]
      ui-components:build  (3.1s)

--------------------------------------------------------------
 NX   Successfully ran target build for 8 projects (2 cached)

      Cached: shared-utils, api-types
      Time:   8.4s
```

**Key patterns worth copying:**
- **`[1/8]` progress counter** -- instantly shows where you are
- **Cached projects listed by name** in summary
- **"Successfully ran" summary** with cache count
- **Separator line** before final summary

**What makes it great:**
- The `[X/N]` counter is the single most useful progress indicator
- Cached vs computed split in summary tells the optimization story
- Clean project-name listing (not hashes)

**What makes it mediocre:**
- Verbose `NX   ` prefix on every meta-line
- No per-task cost/token metrics (not applicable to build tools)


### 1.5 Buck2 (Meta's build system)

**What it looks like:**
```
Build ID: 530a4620-bfb2-454d-bae1-e937ae9e764f
Analyzing targets. Remaining 0/53
75 actions, 101 artifacts declared
Executing actions. Remaining 0/11  1.1s exec time total
Command: run. Finished 3 local
Time elapsed: 0.7s
BUILD SUCCEEDED
```

**Key patterns worth copying:**
- **"Remaining X/Y"** counter for each phase
- **Phase names**: "Analyzing targets", "Executing actions" -- clear state machine
- **Build ID** for traceability
- **Streaming JSON** option (`--streaming-build-report`) for tooling

**What makes it great:**
- Phase-based progress (analysis vs execution) maps perfectly to DAG workflows
- "Remaining" counter is more useful than "completed" counter (you care about what's left)
- Build ID links to traces


### 1.6 GitHub CLI (`gh run watch`)

**What it looks like:**
```
Refreshing run status every 3 seconds. Press Ctrl+C to quit.

JOBS
* build (ID 12345)
  ✓ Set up job                    (2s)
  ✓ Checkout                      (1s)
  * Run tests                     (running)
  - Deploy                        (pending)

ANNOTATIONS
No annotations

Updated 2s ago
```

**Key patterns worth copying:**
- **Status icons**: `*` running, `✓` done, `-` pending, `X` failed
- **Right-aligned durations** in parentheses
- **Refresh timer** at bottom
- **Pending tasks shown** (not just running/done)

**What makes it great:**
- Shows the full pipeline state at a glance (pending + running + done)
- Clean indentation under job names
- Duration only on completed steps

---

## 2. AI Tool Telemetry Patterns

### 2.1 Aider

```
Tokens: 12.4k sent, 3.2k received. Cost: $0.04
```
Single line, post-response. Compact.

### 2.2 Claude Code

```
  Cost: $0.03 | Duration: 2.1s
  Context: 12.4k tokens (8.2k cached)
```
Inline status bar with cost, duration, context window usage.

### 2.3 Observability Patterns (OpenTelemetry-style)

```
Input: 850 tokens | Output: 420 tokens | Latency: 1.8s | Cost: $0.008
Span: Prompt -> Model -> Response
```

### 2.4 Best Practices for AI Telemetry Display

| Metric | Display | Placement |
|--------|---------|-----------|
| Tokens (input) | `1.2k in` | Per-task, dimmed |
| Tokens (output) | `0.8k out` | Per-task, dimmed |
| Cost | `$0.003` | Per-task dimmed, total bold |
| Latency / TTFT | `1.2s` | Per-task, color-coded |
| Cache hits | `(cached)` | Per-task badge |
| Model used | `claude-3.5-sonnet` | Header only |
| Total cost | `$0.042` | Summary, bold |

---

## 3. Spinner & Progress Patterns

### 3.1 Best Unicode Spinner Sequences

**Braille dots (best for modern terminals, 10 frames, 80ms interval):**
```
  ⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏
```

**Aesthetic arc (8 frames, smooth):**
```
  ◐ ◓ ◑ ◒
```

**Minimal dots (6 frames, subtle):**
```
  ⠋ ⠙ ⠚ ⠒ ⠂ ⠂ ⠒ ⠚ ⠙ ⠋
```

### 3.2 Progress Bar Characters

```
Filled:   ━ or ═ or █
Partial:  ╸ or ▓
Empty:    ─ or ░
```

### 3.3 Rust Crate Comparison

| Crate | Strengths | Weaknesses |
|-------|-----------|------------|
| `indicatif` | Full-featured, multi-progress, tree, styles | Heavy dependency |
| `console` | Lightweight styling, TTY detection, auto-color | No progress bars |
| `colored` (current) | Simple, zero-config | No spinners, no progress |
| `crossterm` | Low-level control, cursor | Verbose API |

**Recommendation**: Add `indicatif` for progress, keep `colored` for text styling.
`indicatif` + `console` is the canonical Rust CLI stack (both by @mitsuhiko).

---

## 4. Design Principles Distilled

### The 5 Rules of Great CLI Output

1. **Scannable left margin**: Use consistent-width prefixes (verbs, icons, counters)
2. **Progressive disclosure**: Normal mode is compact; `--verbose` expands
3. **Semantic color, not decorative**: Green=success, Red=error, Yellow=warning, Dim=metadata
4. **Duration/cost are metadata, not content**: Dimmed, right-aligned or parenthesized
5. **Summary is king**: The last 3 lines matter most (Tasks / Time / Cost)

### The Color Budget (max 5-6 colors)

| Color | Meaning | Usage |
|-------|---------|-------|
| Green | Success | `✓`, "Done", durations < 1s |
| Red | Error | `✗`, error messages |
| Yellow | Warning/Running | `⟳`, warnings, durations 1-5s |
| Cyan | Info/Labels | "Output:", verb labels, artifact paths |
| Dimmed | Metadata | Durations, token counts, cost, descriptions |
| Bold | Emphasis | Workflow name, final summary |

---

## 5. Concrete Design for Nika

### 5.1 Current Output (What We Have)

```
┌─ blog-pipeline ─────────────────────────────────┐
│ Provider: claude | Model: claude-3.5-sonnet | Tasks: 4 │
└─────────────────────────────────────────────────┘

  [⟳] research running...
  [⟳] outline running...
  🧠 research ✓ 2.3s — Research the topic
      → The latest findings show that...
  🧠 outline ✓ 1.8s — Create blog outline
      → # Introduction\n## Section 1...
  🧠 draft ✓ 5.1s — Write the draft
      → # How AI Changes Everything...
      artifact: .nika/artifacts/blog-pipeline/draft.md
  ⚡ publish ✓ 0.3s

──────────────────────────────────────────────────
✓ Done! (9.5s | 12420 tokens | $0.042)
```

**Problems:**
- `[⟳]` "running..." lines appear and never update (no spinner, no clear)
- No `[1/4]` progress counter
- No per-task token/cost breakdown
- Verb icons (emoji) can misalign in some terminals
- "running..." and completion on separate lines (wastes vertical space)
- No visual distinction between DAG layers
- Output preview is useful but inconsistent formatting

### 5.2 Proposed Output: "Nika v2" Design

#### OPTION A: Cargo-style (Minimal, Clean)

```
┌─ blog-pipeline ─────────────────────────────────────────────────┐
│ claude/claude-3.5-sonnet | 4 tasks | 2 layers                  │
└─────────────────────────────────────────────────────────────────┘

   Inferring research ..................... ✓ 2.3s (1.2k in, 0.4k out, $0.008)
   Inferring outline ...................... ✓ 1.8s (0.9k in, 0.3k out, $0.005)
   Inferring draft ........................ ✓ 5.1s (2.1k in, 1.8k out, $0.024)
   Executing publish ...................... ✓ 0.3s
                                              wrote .nika/artifacts/draft.md
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
✓ Done in 9.5s | 4/4 tasks | 6.7k tokens | $0.037
```

**Features:**
- Right-aligned verbs (`Inferring`, `Executing`, `Fetching`, `Invoking`)
- Dot leaders connect task name to status (like a table of contents)
- Per-task token + cost in dimmed parentheses
- Single summary line with everything
- Artifact paths indented under their task

#### OPTION B: Turbo-style (Grouped, Verbose)

```
┌─ blog-pipeline ─────────────────────────────────────────────────┐
│ claude/claude-3.5-sonnet | 4 tasks | 2 layers                  │
└─────────────────────────────────────────────────────────────────┘

── Layer 1 ──────────────────────────────────────────── [1/2]
  ⠹ research [infer] .................. running
  ⠹ outline  [infer] .................. running
  ✓ research [infer]  2.3s  1.2k in  0.4k out  $0.008
      Research the topic deeply
      → The latest findings show that quantum computing...
  ✓ outline  [infer]  1.8s  0.9k in  0.3k out  $0.005

── Layer 2 ──────────────────────────────────────────── [2/2]
  ✓ draft    [infer]  5.1s  2.1k in  1.8k out  $0.024
      artifact: .nika/artifacts/draft.md
  ✓ publish  [exec]   0.3s

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
✓ Done in 9.5s | 4/4 tasks | 6.7k tokens | $0.037
```

**Features:**
- DAG layers shown as sections with `[1/2]` counter
- Spinner (`⠹`) replaces task line in-place when done
- Verb type as `[infer]` badge, not emoji (more terminal-safe)
- Per-task telemetry as aligned columns
- Descriptions and output preview indented under task

#### OPTION C: "The Hybrid" (RECOMMENDED)

```
┌─ blog-pipeline ─────────────────────────────────────────────────┐
│ claude · claude-3.5-sonnet · 4 tasks · 2 layers                 │
└─────────────────────────────────────────────────────────────────┘

  ⠹ research  running...                                  [1/4]
  ⠹ outline   running...                                  [2/4]
  ✓ research  2.3s  infer  1.2k/0.4k tok  $0.008         [1/4]
  ✓ outline   1.8s  infer  0.9k/0.3k tok  $0.005         [2/4]
  ✓ draft     5.1s  infer  2.1k/1.8k tok  $0.024         [3/4]
      → .nika/artifacts/draft.md
  ✓ publish   0.3s  exec                                  [4/4]

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
✓ Done in 9.5s · 4/4 tasks · 6,700 tokens · $0.037
```

**Features:**
- **Spinner lines are replaced in-place** when task completes (using `\r` or indicatif)
- **`[X/N]` counter** right-aligned on every line (Nx-inspired)
- **Compact telemetry**: `1.2k/0.4k tok` (input/output) + cost
- **Verb as text badge** not emoji: `infer`, `exec`, `fetch`, `invoke`, `agent`
- **Middle-dot separators** in header and summary (cleaner than `|`)
- **Artifact paths** indented with `→` prefix
- **No output preview by default** (use `--verbose` for previews)
- In `--verbose` mode, add description + output preview lines

### 5.3 Verbose Mode (`nika run --verbose`)

```
┌─ blog-pipeline ─────────────────────────────────────────────────┐
│ claude · claude-3.5-sonnet · 4 tasks · 2 layers                 │
│ schema: @0.12 · trace: gen-a1b2c3d4                             │
└─────────────────────────────────────────────────────────────────┘

── layer 1/2 ─────────────────────────────────────────────────────
  ✓ research  2.3s  infer  1.2k/0.4k tok  $0.008         [1/4]
    desc: Research the topic deeply
    out:  The latest findings show that quantum computing has made
          significant strides in error correction, with Google's...
  ✓ outline   1.8s  infer  0.9k/0.3k tok  $0.005         [2/4]
    desc: Create blog outline from research
    out:  # Introduction\n## Background\n## Key Findings...

── layer 2/2 ─────────────────────────────────────────────────────
  ✓ draft     5.1s  infer  2.1k/1.8k tok  $0.024         [3/4]
    desc: Write the full blog post
    out:  # How AI Changes Everything\n\nIn the past year...
    artifact: .nika/artifacts/draft.md (4.2 KB)
  ✓ publish   0.3s  exec                                  [4/4]
    cmd:  rsync -avz dist/ prod:/var/www/

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
✓ Done in 9.5s · 4/4 tasks · 6,700 tokens · $0.037
  trace: .nika/traces/gen-a1b2c3d4.ndjson
```

### 5.4 Quiet Mode (`nika run --quiet`)

```
✓ 9.5s · 4/4 · $0.037
```

One line. Exit code tells the rest.

### 5.5 JSON Mode (`nika run --json`)

```json
{
  "status": "success",
  "duration_ms": 9512,
  "tasks": { "total": 4, "succeeded": 4, "failed": 0 },
  "tokens": { "input": 4200, "output": 2500, "total": 6700 },
  "cost_usd": 0.037,
  "trace": ".nika/traces/gen-a1b2c3d4.ndjson"
}
```

### 5.6 Error Output

```
┌─ blog-pipeline ─────────────────────────────────────────────────┐
│ claude · claude-3.5-sonnet · 4 tasks · 2 layers                 │
└─────────────────────────────────────────────────────────────────┘

  ✓ research  2.3s  infer  1.2k/0.4k tok  $0.008         [1/4]
  ✓ outline   1.8s  infer  0.9k/0.3k tok  $0.005         [2/4]
  ✗ draft     5.1s  infer  NIKA-060: Invalid JSON output  [3/4]
      error: Schema validation failed after 3 attempts:
             - Path '/title': "title" is a required property
             - Path '/sections': Expected array, got null
  ⊘ publish   --    exec   skipped (dependency failed)    [4/4]

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
✗ Failed in 9.2s · 2/4 tasks · 4,300 tokens · $0.013
  root cause: draft (NIKA-060)
  trace: .nika/traces/gen-a1b2c3d4.ndjson
```

**Error design principles:**
- Failed task has `✗` in red
- Error message indented, with NIKA code
- Skipped downstream tasks show `⊘` in yellow with reason
- Summary changes to red `✗ Failed` with root cause reference
- Trace path always shown on failure (for debugging)

---

## 6. Implementation Plan

### Phase 1: Foundation (indicatif + console)

**Add dependencies:**
```toml
[dependencies]
indicatif = "0.17"
console = "0.16"
```

**Create `src/display/mod.rs`** (replace current `display.rs`):
- `WorkflowHeader` - box-drawing header
- `TaskProgress` - per-task line with spinner
- `WorkflowSummary` - final summary
- `VerbBadge` - text-based verb labels
- `Telemetry` - token/cost formatting

### Phase 2: Live Progress

- Use `indicatif::MultiProgress` for parallel task spinners
- Replace in-place when task completes (spinner -> check/cross)
- Add `[X/N]` counter to each line (right-aligned)
- Add layer separators when `--verbose`

### Phase 3: Telemetry Integration

- Wire `ProviderResponded` events to per-task token/cost display
- Format tokens as `1.2k` (compact) with `k`/`M` suffixes
- Show cost with 3 significant digits: `$0.008`, `$0.12`, `$1.23`
- Color-code cost: green < $0.01, yellow < $0.10, red >= $0.10

### Phase 4: Output Modes

- `--verbose`: Layer headers, descriptions, output previews
- `--quiet`: Single summary line
- `--json`: Structured JSON output
- `--progress=plain`: No spinners, no colors (for CI/pipes)
- Auto-detect: if not TTY, use plain mode

---

## 7. Token/Cost Formatting Spec

### Token Display

```rust
fn format_tokens(n: u64) -> String {
    if n < 1_000 { format!("{}", n) }           // "850"
    else if n < 10_000 { format!("{:.1}k", n as f64 / 1000.0) }  // "1.2k"
    else if n < 1_000_000 { format!("{:.0}k", n as f64 / 1000.0) }  // "42k"
    else { format!("{:.1}M", n as f64 / 1_000_000.0) }  // "1.2M"
}
```

### Cost Display

```rust
fn format_cost(usd: f64) -> String {
    if usd < 0.001 { format!("${:.4}", usd) }      // "$0.0002"
    else if usd < 0.01 { format!("${:.3}", usd) }   // "$0.008"
    else if usd < 1.0 { format!("${:.2}", usd) }    // "$0.04"
    else { format!("${:.2}", usd) }                   // "$1.23"
}
```

### Duration Display (current is good, keep it)

```
< 100ms:  "42ms"   (green)
< 1s:     "0.8s"   (green)
< 60s:    "5.1s"   (yellow if > 1s, red if > 5s)
>= 60s:   "2m 13s" (red)
```

---

## 8. Summary: Patterns to Steal

| Pattern | Source | Priority |
|---------|--------|----------|
| `[X/N]` progress counter | Nx | **P0** |
| In-place spinner replacement | indicatif, Dagger | **P0** |
| Per-task token/cost telemetry | AI tools, Aider | **P0** |
| Compact summary line | Turborepo | **P0** |
| Layer/phase separators | Buck2 | P1 |
| Verb as text badge (not emoji) | -- | P1 |
| `--progress=plain` for CI | Dagger | P1 |
| `--json` structured output | Buck2 | P1 |
| Dot leaders for alignment | Cargo | P2 |
| Cache hit badges | Turbo, Nx | P2 (future) |
| DAG shape visualization | Dagger | P2 (future, for TUI) |

---

## 9. Recommended Crate Stack

```toml
# Terminal progress (spinners, multi-progress, in-place replacement)
indicatif = "0.17"

# Terminal styling (colors, styles, TTY detection) - replaces colored
console = "0.16"

# Keep for now (already used everywhere), migrate later
colored = "2.1"
```

Both `indicatif` and `console` are by @mitsuhiko (Armin Ronacher, creator of Flask).
They work together seamlessly. `indicatif` uses `console` internally.

---

## Sources

1. Cargo source & progress bars - https://doc.rust-lang.org/cargo/reference/config.html
2. Dagger TUI progress (v0.6.0 announcement) - https://docs.dagger.io/
3. Turborepo output modes - https://turbo.build/repo/docs/reference/run
4. Nx task runner output - https://nx.dev/features/run-tasks
5. Buck2 build reports - https://buck2.build/docs/
6. GitHub CLI `gh run` - https://cli.github.com/manual/gh_run_watch
7. indicatif crate - https://crates.io/crates/indicatif
8. console crate - https://crates.io/crates/console
9. Aider token display - https://aider.chat/
10. CLI design best practices (Railway, Vercel, Fly.io patterns)

---

## Methodology

- Tools used: Perplexity AI search (10 queries), source code analysis of Nika display.rs + runner.rs
- Industry tools analyzed: Cargo, Dagger, Turborepo, Nx, Buck2, GitHub CLI, Aider, Claude Code
- Rust crates evaluated: indicatif, console, colored, crossterm, termcolor
- Pages analyzed: ~20 documentation pages + source code
- Confidence: **High** -- patterns are well-established across the industry
