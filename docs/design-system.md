# Nika Design System -- Icon, Color & Typography Reference

> **Single source of truth** for Nika's visual language across CLI and TUI.
> Zero icon conflicts. Consistent terminal rendering. Predictable alignment.
>
> Status: **CANONICAL** -- all new code MUST reference this document.
> Location: `docs/design-system.md`

---

## Table of Contents

1. [Design Philosophy](#1-design-philosophy)
2. [Icon Registry (Master List)](#2-icon-registry-master-list)
3. [Verb Icons (Sacred)](#3-verb-icons-sacred)
4. [Status Icons](#4-status-icons)
5. [Keys Icons](#5-keys-icons)
6. [Category Header Icons](#6-category-header-icons)
7. [Subsystem Icons](#7-subsystem-icons)
8. [TUI-Specific Icons](#8-tui-specific-icons)
9. [Color Palette](#9-color-palette)
10. [Typography Rules](#10-typography-rules)
11. [Anti-Patterns](#11-anti-patterns)
12. [Implementation Reference](#12-implementation-reference)

---

## 1. Design Philosophy

### Core Principles

1. **One icon = one meaning.** No character is reused across categories.
2. **Narrow East Asian Width (eaw=N) by default.** Every icon used in aligned
   columns MUST be 1 terminal column wide. Wide emoji (eaw=W) are permitted
   ONLY in section headers and TUI decorative contexts where alignment is not
   critical.
3. **Shape before color.** Status is conveyed by icon shape first, color second.
   A colorblind user can distinguish success (checkmark) from failure (cross)
   without seeing green vs. red.
4. **Two icon systems, one truth.** CLI (`nika-display/src/icons.rs`) uses
   Narrow Unicode for alignment-critical output. TUI (`nika-tui/src/icons.rs`)
   uses emoji for richer visual display with ASCII fallbacks. Both systems
   share the same semantic mapping documented here.
5. **Rule of 4.** No single view uses more than 4 distinct colors. More than 4
   becomes noise.

### Width Classification

| Class | East Asian Width | Terminal Columns | Where Used |
|-------|-----------------|-----------------|------------|
| Narrow (N) | `eaw=N` | 1 | CLI aligned output, DAG boxes, spinners |
| Wide (W) | `eaw=W` | 2 | TUI verb icons, provider icons, section headers |
| ASCII | 7-bit ASCII | 1-3 | Fallback mode (`IconMode::Ascii`) |

---

## 2. Icon Registry (Master List)

### 2.1 CLI Icons (Narrow -- `nika-display`)

Every icon below is `eaw=N` (1 terminal column) unless marked otherwise.

| Codepoint | Char | Name | Category | Meaning | Color | Source |
|-----------|------|------|----------|---------|-------|--------|
| `U+2727` | ✧ | Four-pointed star | Verb | `infer` | magenta | `icons::verb("infer")` |
| `U+2388` | ⎈ | Helm | Verb | `exec` | yellow | `icons::verb("exec")` |
| `U+2604` | ☄ | Comet | Verb | `fetch` | cyan | `icons::verb("fetch")` |
| `U+229B` | ⊛ | Circled asterisk | Verb | `invoke` | green | `icons::verb("invoke")` |
| `U+274B` | ❋ | Propeller | Verb | `agent` | red | `icons::verb("agent")` |
| `U+2713` | ✓ | Check mark | Status | Success | green bold | `icons::success()` |
| `U+2717` | ✗ | Ballot X | Status | Failed | red bold | `icons::failed()` |
| `U+2298` | ⊘ | Circled division | Status | Skipped | dim | `icons::skipped()` |
| `U+25CB` | ○ | White circle | Status | Pending | dim | `icons::pending()` |
| `U+25CF` | ● | Black circle | Status | Running / fallback | white bold | `icons::running()` |
| `U+22C8` | ⋈ | Bowtie | Subsystem | Provider | blue | `icons::provider()` |
| `U+229E` | ⊞ | Squared plus | Subsystem | MCP | green | `icons::mcp()` |
| `U+22A0` | ⊠ | Squared times | Subsystem | Guardrails | yellow | `icons::guardrail()` |
| `U+229A` | ⊚ | Circled ring | Subsystem | Artifact | cyan | `icons::artifact()` |
| `U+22A1` | ⊡ | Squared dot | Subsystem | Media | magenta | `icons::media()` |
| `U+2B21` | ⬡ | Hexagon | Subsystem | Structured output | blue | `icons::structured()` |
| `U+27D0` | ⟐ | Diamond dot | Subsystem | Vision | purple | `icons::vision()` |
| `U+21C4` | ⇄ | Bidirectional arrows | Subsystem | HTTP | cyan | `icons::http()` |
| `U+21AF` | ↯ | Zigzag arrow | Subsystem | Retry | yellow | `icons::retry()` |
| `U+2297` | ⊗ | Circled times | Subsystem | Agent metadata | red | `icons::agent_meta()` |
| `U+25AA` | ▪ | Small square | Subsystem | Log entry | dim | `icons::log()` |
| `U+26A0` | ⚠ | Warning sign | CLI Status | Warning | yellow | `StatusIcon::Warn` |
| `U+2139` | ℹ | Info | CLI Status | Informational | cyan | `StatusIcon::Info` |
| `U+2B07` | ⬇ | Downward arrow | CLI Status | Download | cyan | `StatusIcon::Download` |
| `U+2192` | → | Right arrow | CLI Status | Hint / action | dim | `StatusIcon::Hint` |
| `U+2934` | ⤋ | Downward arrow | Event | Agent spawned | magenta | `format_event.rs` |
| `U+21B3` | ↳ | Hook arrow | Event | Tool use sub-line | dim | `format_event.rs` |
| `U+03A3` | Σ | Sigma | Summary | Totals row | normal | `summary.rs` |
| `U+2026` | ... | Ellipsis | Formatting | Truncation | normal | `colors.rs` |
| `U+00B5` | µ | Micro sign | Formatting | Microseconds | green | `colors.rs` |
| `U+00B7` | · | Middle dot | Formatting | Separator | dim | Multiple |

### 2.2 Box Drawing (CLI)

| Codepoint | Char | Usage |
|-----------|------|-------|
| `U+256D` | ╭ | Rounded top-left corner |
| `U+256E` | ╮ | Rounded top-right corner |
| `U+2570` | ╰ | Rounded bottom-left corner |
| `U+256F` | ╯ | Rounded bottom-right corner |
| `U+2500` | ─ | Horizontal line |
| `U+2502` | │ | Vertical line / tree pipe |
| `U+251C` | ├ | Tree branch / panel separator |
| `U+2514` | └ | Tree last item |
| `U+250C` | ┌ | Light top-left (doctor) |
| `U+2510` | ┐ | Light top-right (doctor) |
| `U+2518` | ┘ | Light bottom-right (doctor) |
| `U+254C` | ╌ | Dashed (hint box) |
| `U+2550` | ═ | Double horizontal (DAG box) |
| `U+2551` | ║ | Double vertical (DAG box) |
| `U+2554` | ╔ | Double top-left (DAG box) |
| `U+2557` | ╗ | Double top-right (DAG box) |
| `U+255A` | ╚ | Double bottom-left (DAG box) |
| `U+255D` | ╝ | Double bottom-right (DAG box) |
| `U+2524` | ┤ | Panel separator right |

### 2.3 Progress & Bar Characters (CLI)

| Codepoint | Char | Usage |
|-----------|------|-------|
| `U+2501` | ━ | Progress bar filled |
| `U+257A` | ╺ | Progress bar tip |
| `U+2581`-`U+2588` | ▁-█ | Sparkline blocks (8 levels) |
| `U+2591` | ░ | Empty bar / light shade |
| `U+2593` | ▓ | Budget bar filled |
| `U+25BC` | ▼ | DAG edge arrow |

### 2.4 Spinner (CLI)

Braille dots -- 10-frame animation at 80ms/tick (~12.5 fps):

```
⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏
```

All Braille characters are `eaw=N`. Used exclusively in `LiveRenderer` task bars.

### 2.5 TUI Icons (Wide -- `nika-tui`)

These are emoji (2 terminal columns) with ASCII fallbacks. Used in TUI
graphical views where alignment tolerates wider characters.

| Emoji | ASCII | Category | Meaning |
|-------|-------|----------|---------|
| ⚡ | `[I]` | Verb | `infer` |
| 📟 | `[X]` | Verb | `exec` |
| 🛰️ | `[F]` | Verb | `fetch` |
| 🔌 | `[V]` | Verb | `invoke` |
| 🐔 | `[A]` | Verb | `agent` |
| 🐤 | `[a]` | Verb | `subagent` |
| 👤 | `[U]` | Verb | User message |
| 🧠 | `[C]` | Provider | Claude / Anthropic |
| 🤖 | `[O]` | Provider | OpenAI |
| 💨 | `[M]` | Provider | Mistral |
| 💎 | `[Gm]` | Provider | Gemini |
| 🦋 | `[N]` | Provider | Native (local GGUF) |
| ⏱️ | `[G]` | Provider | Groq |
| 🌊 | `[D]` | Provider | DeepSeek |
| 🧪 | `[T]` | Provider | Mock (testing) |
| ⏳ | `[ ]` | Status | Pending |
| ⟳ | `[~]` | Status | Running |
| ✓ | `[+]` | Status | Success |
| ✗ | `[!]` | Status | Failed |
| ⏸ | `[-]` | Status | Paused |
| ⏭ | `[>]` | Status | Skipped |
| 📁 | -- | UI | Folder closed |
| 📂 | -- | UI | Folder open |
| 📄 | -- | UI | File |
| 📋 | -- | UI | YAML file / clipboard |
| 📜 | -- | UI | Code file |
| ✎ | -- | UI | Edit |
| 💾 | -- | UI | Save |
| 🗑 | -- | UI | Delete |
| ↻ | -- | UI | Refresh |
| 🔍 | -- | UI | Search |
| ⚙ | -- | UI | Filter |
| 📎 | -- | UI | Paste |
| 🔗 | -- | UI | MCP server |
| 🔧 | -- | UI | MCP tool |

### 2.6 Cosmic Theme (TUI Decorative)

| Icon | Name | Usage |
|------|------|-------|
| 🦋 | Butterfly | Nika brand identity |
| 🌌 | Galaxy | Cosmic background theme |
| ✦ | Star | Decorative star |
| ★ | Filled star | Rating / highlight |
| ✨ | Sparkle | Activity |
| 🚀 | Rocket | Launch / start |
| 🛰️ | Satellite | Fetch decoration |
| 🌙 | Moon | Nighttime theme |
| 🪐 | Planet | Cosmic decoration |
| ☄️ | Comet | Fetch decoration |
| ◐◓◑◒ | Half-circles | Orbit spinner (4-frame) |

---

## 3. Verb Icons (Sacred)

**These are FROZEN. Never change, never reassign.**

The 5 verb icons are the visual DNA of Nika. Every user learns to associate
these symbols with specific execution semantics. Changing them is a breaking
UX change equivalent to renaming a keyword.

### CLI (Narrow, 1 column)

```
✧  U+2727  infer     magenta     LLM generation
⎈  U+2388  exec      yellow      Shell command
☄  U+2604  fetch     cyan        HTTP request
⊛  U+229B  invoke    green       MCP / builtin tool
❋  U+274B  agent     red         Multi-turn agentic loop
```

### TUI (Wide, 2 columns)

```
⚡  infer     magenta     LLM generation
📟  exec      yellow      Shell command
🛰️  fetch     cyan        HTTP request
🔌  invoke    green       MCP / builtin tool
🐔  agent     red         Multi-turn agentic loop
```

### Rules

- Each verb has exactly ONE color. That color is used consistently in timeline
  bars, DAG boxes, icon rendering, and summary sections.
- The verb icon appears:
  - Before the task ID during execution (`✧ research   running  +2.3s`)
  - In DAG visualization boxes (`╔═✓═══════╗ ║ ✧ research ║ ╚════════╝`)
  - In timeline Gantt bars (colored by verb)
  - In the TUI task tree and detail views

---

## 4. Status Icons

### CLI (Narrow)

```
✓  U+2713  success    green bold      Task completed successfully
✗  U+2717  failed     red bold        Task failed
⊘  U+2298  skipped    dim             Task skipped (dependency failed)
○  U+25CB  pending    dim             Task not yet started
●  U+25CF  running    white bold      Task in progress
```

### CLI Extended (`StatusIcon` enum)

```
✓  Ok         green bold     Pass / configured / valid
✗  Fail       red bold       Error / not configured / invalid
⚠  Warn       yellow         Warning / partial
ℹ  Info       cyan           Informational note
⊘  Skip       dim            Skipped / not applicable
⬇  Download   cyan           In progress / downloading
→  Hint       dim            Action suggestion
```

### TUI Status

```
⏳  pending    dim       Waiting / queued
⟳   running    cyan      Active / in progress
✓   success    green     Completed
✗   failed     red       Error
⏸   paused     yellow    Suspended
⏭   skipped    dim       Cancelled / skipped
```

### Rules

- Status icons MUST appear before the entity they describe (left of task name,
  left of provider name).
- The same status icon is NEVER used to mean different things in different
  contexts. A ✓ always means success. A ✗ always means failure.

---

## 5. Keys Icons

Icons for API key and credential state in `nika provider list` and related
views. All Narrow (`eaw=N`) to align in tree-style lists.

```
✓  U+2713  configured     green bold    Key present and valid
✗  U+2717  not configured red bold      Key missing (action needed)
●  U+25CF  system/builtin white         Always available (mock, native)
○  U+25CB  offline        dim           Not loaded / unavailable
⚠  U+26A0  env-only       yellow        Key in env var (lost on reboot)
⊘  U+2298  unverified     dim           Key present but not tested
↯  U+21AF  stale/expired  yellow        Key failed last connection test
```

### Display Pattern

```text
  LLM Providers (4/7 configured)
  ──────────────────────────────────────────────────

  ├── ✓ anthropic    [sk-ant-a...Kx] (vault)  claude-sonnet-4-6, claude-haiku-4-5
  ├── ✓ openai       [sk-proj-...Mz] (env) ⚠ lost on reboot
  ├── ✗ mistral      → nika keys set mistral
  ├── ✓ groq         [gsk_...Ty] (daemon)
  ├── ✗ deepseek     → nika keys set deepseek
  ├── ✓ gemini       [AI...9f] (vault)
  └── ✗ xai          → nika keys set xai

  Other (always available)
  ──────────────────────────────────────────────────

  ├── ✓ mock         deterministic test responses, no API key
  └── ✗ native       local GGUF models → nika model pull <name>
```

### Rules

- `(env) ⚠ lost on reboot` warning always follows env-sourced keys
- `→` (hint arrow) precedes the fix command for unconfigured providers
- Key source in parentheses: `(vault)`, `(daemon)`, `(env)`
- Masked key in brackets: `[sk-ant-a...Kx]`

---

## 6. Category Header Icons

Section headers for grouping providers, tools, and capabilities in list views.
These are Wide emoji (2 columns) because they appear in header lines where
column alignment with content below is handled by indentation, not character
width matching.

```
🧠  INFERENCE     LLM providers (Anthropic, OpenAI, Mistral, etc.)
🔍  SEARCH        Web discovery tools (Firecrawl, Perplexity, Brave)
💾  DATA          Databases, knowledge graphs (Neo4j, NovaNet)
🎵  MEDIA         Audio, image, video processing
🔧  TOOLS         Developer tools (GitHub, Slack, filesystem)
🌐  SERVICES      Third-party SaaS integrations
🏠  LOCAL         Local resources (mock provider, native GGUF, filesystem)
```

### Usage Pattern

```text
  🧠 INFERENCE
  ─────────────────────────────────
  ├── anthropic     claude-sonnet-4-6
  ├── openai        gpt-4.1
  └── gemini        gemini-2.5-pro

  🔧 TOOLS
  ─────────────────────────────────
  ├── filesystem    3 tools
  └── github        12 tools
```

### Rules

- Category names are UPPERCASE
- Category emoji appears before the name, separated by one space
- Narrow Unicode (like ⊞) is NOT used for category headers -- headers use
  Wide emoji for visual prominence
- Maximum 7 categories. If a tool does not fit, it goes in SERVICES

---

## 7. Subsystem Icons

Internal engine subsystems that appear in verbose execution output (`--detail max`).
All Narrow (`eaw=N`) for alignment in indented sub-event lines.

```
⋈  U+22C8  provider         blue       LLM provider communication
⊞  U+229E  mcp              green      MCP protocol operations
⊠  U+22A0  guardrail        yellow     Guardrail checks
⊚  U+229A  artifact         cyan       Artifact writes
⊡  U+22A1  media            magenta    Media pipeline operations
⬡  U+2B21  structured       blue       Structured output validation
⟐  U+27D0  vision           purple     Vision/multimodal content
⇄  U+21C4  http             cyan       HTTP request/response
↯  U+21AF  retry            yellow     Retry attempts
⊗  U+2297  agent_meta       red        Agent lifecycle events
▪  U+25AA  log              dim        Log entries, boot phases
```

### Sub-Event Line Format

Sub-events are indented 6 spaces with a dimmed vertical bar:

```text
           │ ⋈ ← in:2.4k out:856 cache:0 · ttft:142ms
           │ ⊞ novanet → novanet_search call:abc123
           │ ⬡ L0: tool-inject ✓
           │ ⊚ → output/report.md 4.2KB · markdown
```

---

## 8. TUI-Specific Icons

### Navigation

| Icon | Name | Usage |
|------|------|-------|
| ▲ | Arrow up | Scroll / navigate up |
| ▼ | Arrow down | Scroll / navigate down |
| ◀ | Arrow left | Navigate left / collapse |
| ▶ | Arrow right | Navigate right / expand |
| › | Chevron right | Collapsed tree node |
| ˅ | Chevron down | Expanded tree node |

### Tree Structure

| Icon | Constant | Usage |
|------|----------|-------|
| ├ | `TREE_BRANCH` | Non-last child |
| └ | `TREE_LAST` | Last child |
| │ | `TREE_PIPE` | Continuation line |
| (space) | `TREE_SPACE` | No continuation |

### MCP Connection State

| Icon | Meaning |
|------|---------|
| ● | Connected (solid dot, green context) |
| ○ | Disconnected (hollow dot, dim context) |
| 🔗 | MCP server (label) |
| 🔧 | MCP tool (label) |

---

## 9. Color Palette

### 9.1 Semantic Color Assignments

| Color | Meaning | Used For |
|-------|---------|----------|
| **green** | Success, positive, fast | ✓ icons, <1s durations, <$0.01 costs, env vars found |
| **green bold** | Confirmed success | Status checkmark, VALID label, DONE label |
| **yellow** | Caution, moderate | ⚠ warnings, 1-5s durations, $0.01-$0.10 costs, retries |
| **yellow bold** | Active warning | INVALID label, failed count |
| **red** | Error, failure, slow | ✗ icons, >5s durations, >$0.10 costs, error messages |
| **red bold** | Critical failure | FAILED label, blocked operations, expensive costs |
| **cyan** | Informational, HTTP | ℹ icons, fetch verb, HTTP methods, URLs, boot phases |
| **magenta** | LLM, creative | infer verb, media operations, content types |
| **blue** | Provider, structure | ⋈ provider icon, JSON keys, structured output icon |
| **purple** | Vision, special | ⟐ vision icon |
| **white** | Primary emphasis | Task IDs, model names, running icon |
| **white bold** | Top emphasis | Section titles, N I K A header |
| **dim** | De-emphasized | Timestamps, separators, hints, tree connectors, source labels |
| **bold** | Emphasis | Provider names, workflow names, labels |
| **underline** | Links | URLs in HTTP events |

### 9.2 Duration Thresholds

```
< 1ms     green    (microseconds)
< 1s      green    (milliseconds)
1-5s      yellow   (seconds)
> 5s      red      (seconds or minutes)
```

### 9.3 Cost Thresholds

```
< $0.001  dim      (negligible)
< $0.01   green    (cheap)
< $0.10   yellow   (moderate)
>= $0.10  red bold (expensive)
```

### 9.4 TTFT (Time-to-First-Token) Thresholds

```
< 200ms   green    (fast)
200-500ms yellow   (acceptable)
> 500ms   red      (slow)
```

### 9.5 HTTP Status Code Colors

```
1xx-2xx   green    (success)
3xx       yellow   (redirect)
4xx-5xx   red      (error)
```

### 9.6 Budget Bar Colors

```
< 70%     green    (healthy)
70-90%    yellow   (approaching limit)
> 90%     red      (critical)
```

### 9.7 Log Level Colors

```
error     red
warn      yellow
info      green
debug     dim
trace     dim
```

### 9.8 Rule of 4

Each view should use at most 4 colors for content differentiation.
Infrastructure colors (dim for structure, bold for headers) do not count
toward this limit.

**Good example** -- run summary:
1. green (success counts)
2. yellow (warning counts)
3. red (failure counts)
4. cyan (informational stats)
Plus dim for separators and bold for labels.

**Bad example** -- a view using green + yellow + red + cyan + magenta + blue
+ purple simultaneously would be overwhelming.

### 9.9 Accessibility

- **Shape + color:** Every status is distinguishable by icon shape alone.
  ✓ vs ✗ vs ⊘ vs ○ vs ● are all visually distinct without color.
- **Bold for emphasis:** Critical states (success, failure) use bold in
  addition to color, providing a second visual channel.
- **No color-only conveyed information:** Numbers, labels, and icons always
  accompany colored output.
- **Light/dark terminal compatibility:** All colors are from the standard
  ANSI 8-color palette (via the `colored` crate). They adapt to the terminal's
  color scheme automatically. No 256-color or RGB escapes are used.

---

## 10. Typography Rules

### 10.1 Weight Hierarchy

| Weight | Purpose | Examples |
|--------|---------|---------|
| **bold** | Primary nouns, section titles | Provider names, workflow name, `N I K A` |
| **normal** | Content, values, descriptions | Task output, model names in lists |
| **dim** | Structure, metadata, hints | Tree connectors, timestamps, `gen:7f3a2b`, separators |

### 10.2 Case Rules

| Case | Usage |
|------|-------|
| SPACED UPPERCASE | Major labels: `N I K A`, `D O N E`, `F A I L E D`, `V A L I D` |
| UPPERCASE | Category headers: `INFERENCE`, `TOOLS`, `MEDIA` |
| Title Case | Section names: `LLM Providers`, `Custom Endpoints`, `Nika Doctor` |
| lowercase | Content, task IDs, provider IDs, model names |

### 10.3 Indentation

| Indent | Usage |
|--------|-------|
| 0 | Box borders (`╭`, `╰`) |
| 2 | Section headers, tree roots, panel content |
| 4 | Key-value pairs, status lines |
| 6 | Sub-event indent (before dimmed `│`) |
| 6+5 | Sub-event content (after `│`) |

### 10.4 Separator Patterns

```
╭─────────────╮   Rounded corners -- run header, run summary, check header
│             │
╰─────────────╯

┌─────────────┐   Square corners -- doctor header
│             │
└─────────────┘

╔═✓═══════════╗   Double-line -- DAG visualization boxes
║ ✧ research  ║
╚═════════════╝

──────────────     Single line -- section separators (dimmed)

╌╌╌╌╌╌╌╌╌╌╌╌╌   Dashed line -- hint boxes (dimmed)
```

### 10.5 Numeric Formatting

| Format | Rule |
|--------|------|
| Tokens | `< 1k`: raw number, `< 10k`: `1.2k`, `< 1M`: `42k`, `>= 1M`: `1.2M` |
| Duration | `< 1ms`: `342µs`, `< 1s`: `342ms`, `< 60s`: `2.3s`, `>= 60s`: `1m23.4s` |
| Cost | `< $0.001`: `$0.0001` (4dp), `< $0.01`: `$0.0042` (4dp), `< $1.00`: `$0.123` (3dp), `>= $1.00`: `$2.34` (2dp) |
| Bytes | Human-readable: `1.2KB`, `3.4MB` |
| Percentages | Integer: `33%`, `100%` |

---

## 11. Anti-Patterns

### 11.1 Forbidden Icon Reuse

| Icon | Reserved For | NEVER Use For |
|------|-------------|---------------|
| ✧ | `infer` verb | Any other verb or status |
| ⎈ | `exec` verb | Navigation or settings |
| ☄ | `fetch` verb | Errors or warnings |
| ⊛ | `invoke` verb | Success indicators |
| ❋ | `agent` verb | Decorative purposes |
| ✓ | Success status | "Configured" without actual validation |
| ✗ | Failed status | "Not applicable" (use ⊘) |
| ● | Running | Static "enabled" indicators in CLI |

### 11.2 Known Conflict: CLI vs TUI Verb Icons

The CLI and TUI use DIFFERENT characters for the same verbs. This is
intentional -- CLI needs Narrow (`eaw=N`) for alignment, TUI uses Wide
emoji for visual richness. NEVER mix them:

| Verb | CLI (Narrow) | TUI (Wide) | WRONG |
|------|-------------|-----------|-------|
| infer | ✧ `U+2727` | ⚡ | Using ⚡ in CLI aligned output |
| exec | ⎈ `U+2388` | 📟 | Using 📟 in CLI columns |
| fetch | ☄ `U+2604` | 🛰️ | Using 🛰️ in CLI progress bars |

### 11.3 Known Conflict: Groq Provider

Groq's TUI icon was changed from ⚡ to ⏱️ to avoid conflict with `infer`.
NEVER use ⚡ for Groq. The comment in `nika-tui/src/icons.rs` documents this.

### 11.4 Wide Emoji in Aligned Columns

**NEVER** place Wide emoji (`eaw=W`, 2 terminal columns) in:
- Progress bar labels
- DAG box content
- Key-value pairs with fixed-width labels
- Table columns
- Spinner tick characters

Wide emoji will misalign columns by 1 character, breaking visual grids.
Use only Narrow Unicode or ASCII in these contexts.

### 11.5 Color Combinations That Fail

| Combination | Problem | Fix |
|-------------|---------|-----|
| dim on dim | Invisible on dark terminals | Use normal weight |
| blue text on dark blue background | Common dark terminal default | ANSI blue adapts, but test |
| red + green adjacent | Colorblind users cannot distinguish | Always pair with icon shape |
| yellow bold for errors | Reads as warning, not error | Use red bold for errors |
| magenta for errors | Reads as infer/creative | Use red for errors |

### 11.6 Emoji Without ASCII Fallback (TUI)

Every TUI emoji icon MUST have a corresponding ASCII fallback defined in
the same module. The `IconMode::Ascii` path ensures Nika works in:
- SSH sessions to servers without Unicode fonts
- CI/CD pipelines with minimal terminals
- Windows Command Prompt (legacy)
- `LANG=C` environments

### 11.7 Inline Emoji in Error Messages

**NEVER** embed emoji in error messages, NIKA error codes, or log output
that may be parsed by machines. Emoji in these contexts:
- Break `grep` workflows
- Render as `?` in non-UTF-8 logs
- Consume extra bytes in structured logging

Use the Narrow icon set or plain ASCII for machine-readable output.

---

## 12. Implementation Reference

### Source Files

| File | Crate | Purpose |
|------|-------|---------|
| `tools/nika-display/src/icons.rs` | nika-display | CLI Narrow icon palette (verbs, status, subsystems) |
| `tools/nika-display/src/colors.rs` | nika-display | Color helpers (duration, cost, tokens, sparklines, JSON highlight) |
| `tools/nika-display/src/cli_format.rs` | nika-display | StatusIcon enum, panels, trees, section headers |
| `tools/nika-display/src/format_event.rs` | nika-display | Event-specific formatters (44+ functions) |
| `tools/nika-display/src/header.rs` | nika-display | Workflow run header box |
| `tools/nika-display/src/summary.rs` | nika-display | Run summary, doctor summary |
| `tools/nika-display/src/dag_render.rs` | nika-display | DAG visualization with double-line boxes |
| `tools/nika-display/src/spinner.rs` | nika-display | Braille spinner + progress bar templates |
| `tools/nika-display/src/detail.rs` | nika-display | Verbosity levels controlling what is shown |
| `tools/nika-display/src/check.rs` | nika-display | `nika check` validation checklist |
| `tools/nika-display/src/bench.rs` | nika-display | `nika bench` benchmark formatting |
| `tools/nika-tui/src/icons.rs` | nika-tui | TUI icon set (Wide emoji + ASCII fallbacks) |
| `tools/nika-cli/src/provider.rs` | nika-cli | Provider list display using StatusIcon + tree connectors |

### Adding a New Icon

1. Check this document's Master List -- is the codepoint already taken?
2. Verify East Asian Width: `eaw=N` for CLI, `eaw=W` acceptable for TUI only.
3. Add to `icons.rs` (CLI) or `nika-tui/src/icons.rs` (TUI) with doc comment.
4. Add ASCII fallback if TUI icon.
5. Update this document's Master List with codepoint, char, name, category,
   meaning, color, and source.
6. Run `cargo test --workspace --lib` to verify no alignment regressions.

### Verifying Terminal Width

```rust
use unicode_width::UnicodeWidthChar;
// Must return 1 for all CLI icons:
assert_eq!(UnicodeWidthChar::width('✧'), Some(1));
assert_eq!(UnicodeWidthChar::width('⎈'), Some(1));
```

Tests in `dag_render.rs` already verify this for verb icons.

---

*This document is the canonical reference. When code and document disagree,
update the code.*
