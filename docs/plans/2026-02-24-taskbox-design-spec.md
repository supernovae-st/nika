# TaskBox Design Specification

> **For Claude:** This spec defines the visual language and UX patterns for TaskBox widgets across all Nika views.

**Version:** 1.0.0
**Date:** 2026-02-24
**Status:** Design Complete

---

## Table of Contents

1. [Overview](#overview)
2. [Core Architecture](#core-architecture)
3. [Effects Library](#effects-library)
4. [State Machine](#state-machine)
5. [Verb Designs](#verb-designs)
6. [Interaction Model](#interaction-model)
7. [Rendering Modes](#rendering-modes)
8. [Implementation Guidelines](#implementation-guidelines)

---

## Overview

TaskBox widgets are the primary visual representation of Nika's 5 semantic verbs during execution. They provide rich feedback, animations, and interaction affordances across all views where workflows execute.

### Design Goals

1. **Absolute Feedback** — Every micro-event visible with detail levels
2. **Rich Effects** — Matrix/cyber aesthetics layered by state
3. **Consistent Language** — Same widget patterns across Chat, Runner, Editor preview
4. **DAG-Ready** — Reusable components for v0.9+ StableGraph migration
5. **Performance** — 60fps rendering with cached strings, no allocations in render loop

### Verb Color Taxonomy

| Verb | Icon | Color (Tailwind) | Hex | Use |
|------|------|------------------|-----|-----|
| **Infer** | ⚡ | Violet 500 | `#8b5cf6` | LLM generation |
| **Exec** | 📟 | Amber 500 | `#f59e0b` | Shell commands |
| **Fetch** | 🛰️ | Cyan 500 | `#06b6d4` | HTTP requests |
| **Invoke** | 🔌 | Emerald 500 | `#10b981` | MCP tool calls |
| **Agent** | 🐔 | Rose 500 | `#f43f5e` | Agentic loops |
| **Spawn** | 🐤 | Rose 300 | `#fda4af` | Child agents |

---

## Core Architecture

### TaskBox Enum

```rust
pub enum TaskBox {
    Infer(InferBox),
    Exec(ExecBox),
    Fetch(FetchBox),
    Invoke(InvokeBox),
    Agent(AgentBox),
}
```

### Shared Traits

```rust
pub trait TaskBoxWidget {
    /// Current execution state
    fn state(&self) -> &BoxState;

    /// Required height for rendering
    fn required_height(&self, mode: RenderMode) -> u16;

    /// Whether widget is expanded
    fn is_expanded(&self) -> bool;

    /// Toggle expansion state
    fn toggle_expand(&mut self);

    /// Handle keypress when focused
    fn handle_key(&mut self, key: KeyEvent) -> TaskBoxAction;
}

pub enum TaskBoxAction {
    None,
    Expand,
    Collapse,
    Retry,
    Copy(CopyTarget),
    OpenExternal,
    DrillDown,
}

pub enum RenderMode {
    Compact,   // 4-10 lines for inline chat
    Expanded,  // 15-60 lines for focused view
    Full,      // Unlimited for drill-down panel
}
```

---

## Effects Library

### 1. Decrypt/Reveal Animation

Progressive character reveal simulating decryption:

```
Frame 0: ░░░░░░░░░░░░░░░░░░░░
Frame 1: ░▒░░░░░░░░░░░░░░░░░░
Frame 2: ░▒▓░░░░░░░░░░░░░░░░░
Frame 3: H▒▓░░░░░░░░░░░░░░░░░
Frame 4: He▓█░░░░░░░░░░░░░░░░
Frame 5: Hel█░░░░░░░░░░░░░░░░
Frame 6: Hell░░░░░░░░░░░░░░░░
Frame 7: Hello░░░░░░░░░░░░░░░
...
```

**Characters:** `░▒▓█` (block elements, 4 frames per char)

**Implementation:**
```rust
pub struct DecryptEffect {
    text: String,
    revealed_chars: usize,
    frame: usize,
}

impl DecryptEffect {
    const CHARS: [char; 4] = ['░', '▒', '▓', '█'];
    const FRAMES_PER_CHAR: usize = 4;

    pub fn render(&self) -> String {
        let mut result = String::new();
        for (i, c) in self.text.chars().enumerate() {
            if i < self.revealed_chars {
                result.push(c);
            } else if i == self.revealed_chars {
                let frame_idx = self.frame % Self::CHARS.len();
                result.push(Self::CHARS[frame_idx]);
            } else {
                result.push('░');
            }
        }
        result
    }

    pub fn tick(&mut self) {
        self.frame += 1;
        if self.frame >= Self::FRAMES_PER_CHAR {
            self.frame = 0;
            self.revealed_chars += 1;
        }
    }
}
```

### 2. Braille Spinner

8-frame rotation for running state:

```
⣾ ⣽ ⣻ ⢿ ⡿ ⣟ ⣯ ⣷
```

**Implementation:**
```rust
pub const BRAILLE_SPINNER: [char; 8] = ['⣾', '⣽', '⣻', '⢿', '⡿', '⣟', '⣯', '⣷'];

pub fn spinner_char(frame: usize) -> char {
    BRAILLE_SPINNER[frame % BRAILLE_SPINNER.len()]
}
```

### 3. Progress Bar

Granular progress with percentage:

```
████████████████░░░░░░░░░░░░░░░░░░░░░░░░ 45%
████████████████████████████████████████ 100%
```

**Implementation:**
```rust
pub fn progress_bar(progress: f32, width: usize) -> String {
    let filled = (progress * width as f32) as usize;
    let empty = width - filled;
    format!(
        "{}{} {:>3}%",
        "█".repeat(filled),
        "░".repeat(empty),
        (progress * 100.0) as u8
    )
}
```

### 4. Sparkline

Token generation speed visualization:

```
▁▂▃▅▇█▇▅▃▂▁▂▃▅▇█
```

**Characters:** `▁▂▃▄▅▆▇█` (8 levels)

**Implementation:**
```rust
pub const SPARKLINE_CHARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

pub fn sparkline(values: &[f32], width: usize) -> String {
    let max = values.iter().cloned().fold(f32::MIN, f32::max);
    values.iter()
        .take(width)
        .map(|v| {
            let idx = ((v / max) * 7.0) as usize;
            SPARKLINE_CHARS[idx.min(7)]
        })
        .collect()
}
```

### 5. Blinking Cursor

Text insertion point animation:

```
Frame 0-2: Hello World▌
Frame 3-5: Hello World
```

**Implementation:**
```rust
pub fn cursor(frame: usize) -> &'static str {
    if (frame / 3) % 2 == 0 { "▌" } else { " " }
}
```

### 6. Glitch/Shake Effect

Error state visual feedback:

```
Frame 0: ├──!ERROR!──┤
Frame 1: ├─!ERROR!───┤  (shift left)
Frame 2: ├───!ERROR!─┤  (shift right)
Frame 3: ├──!ERROR!──┤  (center)
```

**Implementation:**
```rust
pub fn glitch_offset(frame: usize) -> i8 {
    match frame % 6 {
        0 | 5 => 0,
        1 | 2 => -1,
        3 | 4 => 1,
        _ => 0,
    }
}
```

### 7. Border Pulse

State-synced border color intensity:

```rust
pub fn pulse_color(base: Color, frame: usize) -> Color {
    let intensity = ((frame % 20) as f32 / 20.0 * std::f32::consts::PI).sin();
    let factor = 0.7 + (intensity * 0.3); // 70-100% brightness
    match base {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as f32 * factor) as u8,
            (g as f32 * factor) as u8,
            (b as f32 * factor) as u8,
        ),
        _ => base,
    }
}
```

### 8. Fade-In Content

New content appears progressively:

```rust
pub fn fade_style(age_frames: usize) -> Style {
    let alpha = (age_frames.min(10) as f32 / 10.0);
    let gray = (200.0 * alpha) as u8;
    Style::default().fg(Color::Rgb(gray, gray, gray))
}
```

---

## State Machine

### BoxState Enum

```rust
pub enum BoxState {
    /// Waiting in queue
    Queued,

    /// Actively executing
    Running {
        start: Instant,
        frame: usize,
    },

    /// Completed successfully
    Success {
        duration_ms: u64,
    },

    /// Failed with error
    Failed {
        error: String,
        duration_ms: u64,
    },

    /// Skipped (dependency failed or condition not met)
    Skipped {
        reason: String,
    },
}
```

### State Transitions

```
                    ┌─────────┐
                    │ QUEUED  │
                    └────┬────┘
                         │
                         ▼
                    ┌─────────┐
              ┌─────│ RUNNING │─────┐
              │     └─────────┘     │
              ▼                     ▼
        ┌─────────┐           ┌─────────┐
        │ SUCCESS │           │ FAILED  │
        └─────────┘           └─────────┘

                    ┌─────────┐
                    │ SKIPPED │ (from QUEUED only)
                    └─────────┘
```

### Visual Indicators by State

| State | Icon | Border | Background | Animation |
|-------|------|--------|------------|-----------|
| Queued | ⏳ | Dim (30%) | None | Subtle pulse |
| Running | ⣾ | Verb color | None | Spinner + progress |
| Success | ✅ | Green flash → verb color | Brief green tint | Checkmark scale |
| Failed | ❌ | Red | Error highlight | Glitch shake |
| Skipped | ⏭️ | Gray | Strikethrough | None |

---

## Verb Designs

### InferBox

**Purpose:** LLM text generation with streaming support

**Fields:**
```rust
pub struct InferBox {
    pub model: String,
    pub prompt: String,
    pub response: String,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub thinking_tokens: Option<u32>,
    pub thinking_content: Option<String>,
    pub streaming_cursor: usize,
    pub state: BoxState,
    pub expanded_prompt: bool,
    pub expanded_response: bool,
    pub expanded_thinking: bool,
}
```

**Compact Layout (6 lines):**
```
╭─ ⚡ INFER ─────────────────────────────────── ⣾ 2.3s ──╮
│ 🧠 claude-sonnet-4-6          📊 1.2K↓ / 156↑         │
├────────────────────────────────────────────────────────┤
│ ┊ Generating landing page headline...█                 │
│ ████████████░░░░░░░░░░ 312/500 tokens                 │
╰────────────────────────────────────────────────────────╯
```

**Expanded Layout (15-20 lines):**
- Full prompt section (collapsible)
- Full response with decrypt animation
- Thinking section if enabled
- Token sparkline
- Cost calculation

### ExecBox

**Purpose:** Shell command execution with stdout/stderr

**Fields:**
```rust
pub struct ExecBox {
    pub command: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub pid: Option<u32>,
    pub cwd: Option<String>,
    pub state: BoxState,
    pub expanded_stdout: bool,
    pub expanded_stderr: bool,
}
```

**Compact Layout (5 lines):**
```
╭─ 📟 EXEC ──────────────────────────────────── ⣾ 12.4s ─╮
│ $ npm run build                              pid:4521  │
├────────────────────────────────────────────────────────┤
│ ┊ ✓ Generating static pages (23/47)█                   │
╰────────────────────────────────────────────────────────╯
```

**Expanded Layout:**
- Full command with cwd
- STDOUT section (auto-scroll, line count)
- STDERR section (amber highlight)
- Exit code badge
- CPU/Memory sparklines (if available)

### FetchBox

**Purpose:** HTTP requests with retry support

**Fields:**
```rust
pub struct FetchBox {
    pub method: String,
    pub url: String,
    pub request_headers: Vec<(String, String)>,
    pub request_body: Option<String>,
    pub status_code: Option<u16>,
    pub response_headers: Vec<(String, String)>,
    pub response_body: Option<String>,
    pub response_size: Option<usize>,
    pub ttfb_ms: Option<u64>,
    pub retries: u32,
    pub max_retries: u32,
    pub state: BoxState,
    pub expanded_request: bool,
    pub expanded_response: bool,
}
```

**Compact Layout (5 lines):**
```
╭─ 🛰️ FETCH ─────────────────────────────────── ⣾ 0.8s ──╮
│ POST https://api.example.com/v1/generate               │
├────────────────────────────────────────────────────────┤
│ DNS ✓ │ TLS ✓ │ SEND ⣾ │ RECV ░                       │
╰────────────────────────────────────────────────────────╯
```

**Expanded Layout:**
- 4-phase pipeline visualization
- Request headers (sensitive masked)
- Request body (JSON highlighted)
- Response headers
- Response body (JSON highlighted)
- Retry history if retried

### InvokeBox

**Purpose:** MCP tool calls with JSON params/results

**Fields:**
```rust
pub struct InvokeBox {
    pub tool: String,
    pub server: String,
    pub params: Value,
    pub result: Option<Value>,
    pub error: Option<McpError>,
    pub state: BoxState,
    pub expanded_params: bool,
    pub expanded_result: bool,
    // Cached strings to avoid serde in render loop
    pub params_oneline_cached: Option<String>,
    pub params_pretty_cached: Option<String>,
    pub result_oneline_cached: Option<String>,
    pub result_pretty_cached: Option<String>,
}
```

**Compact Layout (5 lines):**
```
╭─ 🔌 INVOKE ────────────────────────────────── ⣾ 0.3s ──╮
│ novanet::novanet_describe                     ◉ live   │
├────────────────────────────────────────────────────────┤
│ ┊ { entity: "qr-code", locale: "fr-FR" }               │
╰────────────────────────────────────────────────────────╯
```

**Expanded Layout:**
- Client ↔ Server animation
- Params JSON (syntax highlighted, folded)
- Result JSON (syntax highlighted, folded)
- Error details if failed

### AgentBox

**Purpose:** Multi-turn agentic loops with nested children

**Fields:**
```rust
pub struct AgentBox {
    pub task_id: String,
    pub prompt: String,
    pub turn: u32,
    pub max_turns: u32,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub cost: f64,
    pub tool_calls: u32,
    pub children: Vec<TaskBox>,
    pub thinking: Option<String>,
    pub final_response: Option<String>,
    pub state: BoxState,
    pub expanded_children: bool,
    pub expanded_response: bool,
    pub expanded_thinking: bool,
}
```

**Compact Layout (8 lines):**
```
╭─ 🐔 AGENT ─────────────────────────────── ⣾ Turn 3/10 ─╮
│ Research and generate landing page                     │
├────────────────────────────────────────────────────────┤
│ 📊 4.2K↓/1.1K↑ │ 💰 $0.028 │ 🔌 5 tools │ ⏱️ 34.2s   │
│ [1✓][2✓][3⣾][4░][5░][6░][7░][8░][9░][10░]             │
│   ▶ 5 nested tasks (Enter to expand)                   │
╰────────────────────────────────────────────────────────╯
```

**Expanded Layout:**
- Turn progress tracker
- Metrics bar
- Nested children (recursive TaskBox)
- Thinking section
- Final response (markdown preview)
- Action bar

---

## Interaction Model

### Keyboard Controls

| Key | Scope | Action |
|-----|-------|--------|
| `j` / `↓` | List | Move focus down |
| `k` / `↑` | List | Move focus up |
| `Enter` | Focused box | Toggle expand / drill-down |
| `Tab` | View | Next TaskBox |
| `Shift+Tab` | View | Previous TaskBox |
| `Esc` | Drill-down | Back to list |
| `y` | Focused box | Yank/copy result |
| `o` | Focused box | Open external (file, URL) |
| `r` | Failed box | Retry |
| `/` | List | Search/filter |
| `1-5` | Focused box | Expand section N |

### Focus Behavior

1. **Unfocused:** Compact view, no action bar
2. **Focused:** Highlighted border, action bar visible
3. **Expanded:** Full content, sections collapsible
4. **Drill-down:** Full panel takeover, all details

### Action Bar

Contextual actions shown in footer when focused:

```
┌──────────────────────────────────────────────────────────────┐
│  [R] Retry    [Y] Copy    [O] Open    [E] Expand    [Esc]   │
└──────────────────────────────────────────────────────────────┘
```

Actions vary by verb and state:
- **Infer:** Copy response, Copy prompt, Regenerate
- **Exec:** Copy stdout, Copy stderr, Retry, Open in editor
- **Fetch:** Copy response, Copy as cURL, Retry
- **Invoke:** Copy result, Edit params, Retry
- **Agent:** Copy final, Export, Continue (+turns)

---

## Rendering Modes

### Compact Mode (Chat Inline)

- Height: 4-10 lines
- Single-line status bar
- Truncated content with `[+more]`
- Minimal sections (no headers for empty sections)
- Auto-collapse on scroll-out

### Expanded Mode (Runner View)

- Height: 15-60 lines
- Full section headers
- Collapsible sections with `[+]`/`[-]`
- Scroll within box
- Persistent until manually collapsed

### Full Mode (Drill-Down Panel)

- Height: Unlimited (scrollable panel)
- All sections expanded by default
- Syntax highlighting for code/JSON
- Copy buttons per section
- Link/path detection and opening

---

## Implementation Guidelines

### Performance Rules

1. **No allocations in render loop**
   - Pre-cache all computed strings
   - Use `Cow<str>` for borrowed/owned
   - Reuse buffers

2. **JSON caching**
   - Cache `params_oneline_cached`, `params_pretty_cached`
   - Update cache only when params change
   - Use `serde_json::to_string` outside render

3. **Animation frame management**
   - Single `frame: usize` counter per widget
   - Tick at 30fps (33ms intervals)
   - Modulo for cyclic animations

### Accessibility

1. **Color contrast**
   - All colors meet WCAG AA (4.5:1)
   - State never communicated by color alone
   - Icons always accompany status

2. **Screen reader compatibility**
   - Semantic structure in render output
   - Status announcements for state changes

### Testing

1. **Snapshot tests**
   - Each verb × each state × each mode
   - Ensure consistent output

2. **Performance benchmarks**
   - Render time < 1ms for compact
   - Render time < 5ms for expanded
   - Zero allocations in hot path

---

## Migration Notes

### DAG Integration (v0.9+)

TaskBox widgets will be referenced by `NodeIndex` in StableGraph:

```rust
pub struct DagNode {
    pub task_id: String,
    pub task_box: TaskBox,
    pub index: NodeIndex,
}
```

Widgets must support:
- Unique ID for graph node binding
- State updates via message passing
- Dependency visualization (incoming/outgoing edges)

### Current Implementation Gaps

| Component | Current | Enhanced |
|-----------|---------|----------|
| Decrypt effect | None | Add `DecryptEffect` |
| Progress bars | Basic | Add estimated completion |
| Phase pipeline | None (Fetch) | Add 4-phase visualization |
| Turn tracker | None (Agent) | Add box grid |
| Action bar | Partial | Full contextual actions |
| Drill-down | None | Add panel takeover |

---

## Appendix: ASCII Art Reference

### Box Drawing Characters

```
╭ ╮ ╯ ╰  Rounded corners
─ │        Horizontal/vertical lines
├ ┤ ┬ ┴ ┼  Connectors
┌ ┐ └ ┘  Square corners
═ ║        Double lines
```

### Progress Characters

```
░ ▒ ▓ █  Block elements (decrypt)
▁ ▂ ▃ ▄ ▅ ▆ ▇ █  Sparkline
─────────────────────  Empty progress
████████████████████  Full progress
```

### Status Icons

```
⏳  Queued
⣾ ⣽ ⣻ ⢿ ⡿ ⣟ ⣯ ⣷  Spinner (8 frames)
✅  Success
❌  Failed
⏭️  Skipped
🔄  Retry
◉   Live indicator
```

---

## Changelog

- **1.0.0** (2026-02-24): Initial design specification
