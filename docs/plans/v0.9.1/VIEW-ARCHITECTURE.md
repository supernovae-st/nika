# Nika TUI — View Architecture

> **For Claude:** Complete reference for all 4 views, their panels, and visual effects.

---

## Overview

Nika TUI has **4 views**, each with distinct panels and purposes:

```
    [1]              [2]              [3]              [4]
 ┌─────────┐     ┌─────────┐     ┌─────────┐     ┌─────────┐
 │  CHAT   │ ◄─► │  HOME   │ ◄─► │ STUDIO  │ ◄─► │ MONITOR │
 │  Agent  │     │ Browser │     │  Editor │     │ Execute │
 └─────────┘     └─────────┘     └─────────┘     └─────────┘
      a              h               s               m
```

**Navigation:**
- `Tab` / `Shift+Tab` — Cycle views
- `1-4` or `a/h/s/m` — Direct jump to view

---

## View 1: Chat (Agent)

**Hotkey:** `1` or `a`
**Title:** NIKA AGENT
**File:** `src/tui/views/chat.rs`

### Purpose
Conversational AI interface with inline execution results. **Chat-as-DAG** (v0.9.x) transforms this into a visual workflow builder.

### Current Layout (v0.8.x)
```
┌─────────────────────────────────────────────────────────────────────────────┐
│ SESSION CONTEXT: tokens 1.2k/200k | cost $0.42 | MCP: ◉ novanet | ⏱ 3m 12s │
├─────────────────────────────────────────────────────────────────────────────┤
│ Conversation history                                 │ ACTIVITY STACK       │
│ - User messages                                      │ 🔥 HOT (executing)   │
│ - Nika responses with inline MCP/Infer boxes         │ 🟡 WARM (recent)     │
│ ╭─ 🔧 MCP CALL: novanet_describe ─────── ✅ 1.2s ─╮  │ ⚪ QUEUED (waiting)  │
│ │ 📥 params: { "entity": "qr-code" }              │  │                      │
│ │ 📤 result: { "display_name": "QR Code" }        │  │                      │
│ ╰─────────────────────────────────────────────────╯  │                      │
├──────────────────────────────────────────────────────┴──────────────────────┤
│ > Input field                                                [⌘K] commands │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Planned Layout (v0.9.4+)
```
┌────────────────────────────────────────────┬────────────────────────────────┐
│ Chat (Messages + Activity)                 │ DAG Live (Sidebar)             │
│                                            │                                │
│ > User: "Describe QR Code"                 │   ╭─────╮                      │
│ ╭──────────────────────────╮               │   │ 001 │                      │
│ │ ⚡ msg-001               │               │   ╰──┬──╯                      │
│ │ "QR Code is a 2D..."     │               │      │                         │
│ ╰──────────────────────────╯               │      ▼                         │
│                                            │   ╭━━━━━╮                      │
│ > User: "Generate title @1"                │   ┃ 002 ┃ ◐                    │
│ ╭──────────────────────────╮               │   ╰━━━━━╯                      │
│ │ ⚡ msg-002   ◐           │               │                                │
│ ╰──────────────────────────╯               │ 2 tasks • 2 layers             │
├────────────────────────────────────────────┴────────────────────────────────┤
│ > _                                                   [⌘K] commands         │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Panels (v0.9.4)
| Panel | ID | Focus Order | Purpose |
|-------|-----|-------------|---------|
| MessageList | `chat.messages` | 1 | Conversation history |
| InputBox | `chat.input` | 2 | User input with @mention autocomplete |
| DagPanel | `chat.dag` | 3 | Live DAG sidebar (NEW in v0.9.4) |

### Matrix Effects
| Effect | Trigger | Intensity |
|--------|---------|-----------|
| Matrix Rain | View receives focus | Medium burst |
| Matrix Rain | LLM streaming starts | High burst |
| Matrix Decrypt | Nika response arrives | Per-verb theme |
| Ambient Rain | Idle state | Low density |

### Special Widgets
- `SessionContextBar` — Token/cost/MCP status
- `McpCallBox` — Inline MCP call visualization
- `InferStreamBox` — Streaming LLM inference
- `ActivityStack` — Hot/warm/queued tasks
- `CommandPalette` — ⌘K fuzzy search
- `MentionAutocomplete` — @mention suggestions (v0.9.2)
- `ChatDagPanel` — Live DAG (v0.9.4)

---

## View 2: Home (Browser)

**Hotkey:** `2` or `h`
**Title:** NIKA HOME
**File:** `src/tui/views/home.rs`
**Default view on startup**

### Purpose
Workflow browser with file tree and DAG preview. Entry point for discovering and running `.nika.yaml` files.

### Layout
```
┌─────────────────────────────────────────────────────────────────────────────┐
│ SEARCH: [fuzzy search bar]                        (Ctrl+P to activate)      │
├─────────────────────────────┬───────────────────────────────────────────────┤
│ FILES (40%)                 │ DAG PREVIEW (60%)                             │
│                             │                                               │
│ 📁 examples/                │   ╭─────────╮                                 │
│   📄 hello.nika.yaml ◄      │   │ greet   │                                 │
│   📄 fetch-demo.nika.yaml   │   ╰────┬────╯                                 │
│   📄 agent-loop.nika.yaml   │        │                                      │
│ 📁 workflows/               │        ▼                                      │
│   📄 generate.nika.yaml     │   ╭─────────╮                                 │
│                             │   │ respond │                                 │
│                             │   ╰─────────╯                                 │
│                             │                                               │
│                             │ Toggle: [D]AG / [Y]AML                        │
├─────────────────────────────┴───────────────────────────────────────────────┤
│ HISTORY: recent workflow runs (toggle with [H])                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Panels
| Panel | ID | Focus Order | Purpose |
|-------|-----|-------------|---------|
| FileList | `home.files` | 1 | Tree view of .nika.yaml files |
| Preview | `home.preview` | 2 | DAG or YAML preview |
| History | `home.history` | 3 | Recent workflow runs (collapsible) |

### Matrix Effects
| Effect | Trigger | Intensity |
|--------|---------|-----------|
| Matrix Rain | View loads (initial) | Full fade from 1.0→0.0 |
| Matrix Rain | File selection changes | Brief burst |
| Matrix Rain | Workflow runs | Medium burst |

### Key Features
- Fuzzy file search (nucleo matcher)
- Preview toggle: DAG visualization ↔ Verb-colored YAML
- Direct run: `Enter` on selected file
- Open in Studio: `e` on selected file

---

## View 3: Studio (Editor)

**Hotkey:** `3` or `s`
**Title:** NIKA STUDIO
**File:** `src/tui/views/studio.rs`

### Purpose
YAML workflow editor with real-time validation, syntax highlighting, and task DAG mini-view.

### Layout
```
┌─────────────────────────────────────────────────────────────────────────────┐
│ FILE: examples/hello.nika.yaml                              [Ctrl+S] Save   │
├─────────────────────────────────────────────────────┬───────────────────────┤
│ EDITOR                                              │ STRUCTURE             │
│                                                     │                       │
│  1 │ schema: nika/workflow@0.5                      │   ╭───────╮           │
│  2 │ workflow: hello-world                          │   │ greet │ ⚡        │
│  3 │                                                │   ╰───┬───╯           │
│  4 │ tasks:                                         │       │               │
│  5 │   - id: greet                                  │       ▼               │
│  6 │     infer: "Say hello"                         │   ╭───────╮           │
│  7 │                                                │   │respond│ ⚡        │
│  8 │   - id: respond                                │   ╰───────╯           │
│  9 │     infer: "Continue conversation"             │                       │
│ 10 │     use:                                       │ 2 tasks               │
│ 11 │       prev: greet.output                       │ 0 errors              │
│                                                     │                       │
├─────────────────────────────────────────────────────┴───────────────────────┤
│ ✅ Valid YAML │ ✅ Schema OK │ 0 warnings                                    │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Panels
| Panel | ID | Focus Order | Purpose |
|-------|-----|-------------|---------|
| Editor | `studio.editor` | 1 | YAML editor with line numbers |
| Structure | `studio.structure` | 2 | Task DAG mini-view |
| StatusBar | `studio.status` | — | Validation status (not focusable) |

### Matrix Effects
| Effect | Trigger | Intensity |
|--------|---------|-----------|
| Matrix Rain | View receives focus | Brief burst |
| Matrix Rain | File save successful | Success burst (green-tinted) |
| Matrix Rain | Validation error | Error burst (red-tinted) |

### Key Features
- Vim-like modes: Normal (navigation) / Insert (editing)
- Real-time YAML syntax validation
- Schema validation against `nika-workflow.schema.json`
- Edit History (Ctrl+Z/Ctrl+Y) with 500ms coalescing
- Session persistence to `.nika/sessions/`
- Syntax highlighting with verb-specific colors

### Editor Shortcuts
| Shortcut | Mode | Action |
|----------|------|--------|
| `i` | Normal | Enter Insert mode |
| `Esc` | Insert | Return to Normal mode |
| `Ctrl+S` | Any | Save file |
| `Ctrl+Z` | Any | Undo |
| `Ctrl+Y` | Any | Redo |

---

## View 4: Monitor (Execute)

**Hotkey:** `4` or `m`
**Title:** NIKA MONITOR
**File:** `src/tui/views/mod.rs` (enum only, no dedicated file)

### Purpose
Real-time workflow execution monitoring with 4-panel display for traces, events, task graph, and details.

### Layout
```
┌─────────────────────────────────────────────────────────────────────────────┐
│ RUNNING: examples/generate-page.nika.yaml                    [Ctrl+C] Stop  │
├────────────────────────────────────────┬────────────────────────────────────┤
│ MISSION CONTROL                        │ DAG LIVE                           │
│ [Progress] [IO] [Output]               │ [Graph] [YAML]                     │
│                                        │                                    │
│ ⚡ Task: generate_header    ◐ 1.2s     │     ╭────────╮                     │
│ ⚡ Task: generate_body      ○ pending  │     │ header │ ✓                   │
│ 🔌 Task: fetch_context      ✓ 0.8s    │     ╰────┬───╯                     │
│                                        │          │                         │
│ Progress: 2/5 tasks                    │          ▼                         │
│ ████████░░░░░░░░░░░░ 40%               │     ╭────────╮                     │
│                                        │     │  body  │ ◐                   │
│                                        │     ╰────────╯                     │
├────────────────────────────────────────┼────────────────────────────────────┤
│ REASONING                              │ NOVANET                            │
│ [Turns] [Thinking] [Steps]             │ [Summary] [Full JSON]              │
│                                        │                                    │
│ Turn 1: "Generate header..."           │ Entity: qr-code                    │
│ 💭 Thinking: I need to consider        │ Locale: fr-FR                      │
│    the brand voice and SEO...          │ Forms: text, title                 │
│                                        │                                    │
│ Turn 2: Using novanet_describe         │ { "display_name": "QR Code",       │
│ 🔌 invoke: novanet_describe            │   "description": "2D barcode..." } │
└────────────────────────────────────────┴────────────────────────────────────┘
```

### Panels
| Panel | ID | Focus Order | Purpose |
|-------|-----|-------------|---------|
| MissionControl | `monitor.mission` | 1 | Task progress, I/O, output |
| DagLive | `monitor.dag` | 2 | Live task graph |
| Reasoning | `monitor.reasoning` | 3 | Agent turns, thinking, steps |
| NovaNet | `monitor.novanet` | 4 | MCP context from NovaNet |

### Panel Tabs
| Panel | Tabs |
|-------|------|
| MissionControl | Progress, IO, Output |
| DagLive | Graph, YAML |
| Reasoning | Turns, Thinking, Steps |
| NovaNet | Summary, Full JSON |

### Matrix Effects
| Effect | Trigger | Intensity |
|--------|---------|-----------|
| Matrix Rain | Workflow starts | High burst |
| Matrix Rain | Task completes | Medium burst per task |
| Matrix Decrypt | Agent response streaming | Per-verb theme |
| Matrix Rain | Workflow completes | Success cascade |
| Matrix Rain | Workflow fails | Error cascade (red) |

---

## Matrix Rain — Cross-View Configuration

### Where Matrix Rain Appears

| View | Location | Trigger | Default State |
|------|----------|---------|---------------|
| **Chat** | Full background | Focus, streaming, idle | Ambient (low density) |
| **Home** | Full background | Load, selection, run | Fade from 1.0→0.0 |
| **Studio** | Full background | Focus, save, errors | Burst on events |
| **Monitor** | Full background | Start, complete, fail | Active during execution |

### Configuration (All Views)

```rust
pub struct MatrixRainConfig {
    /// Background density (0.0 = invisible, 1.0 = full)
    pub density: f32,
    /// Animation speed (ms per frame)
    pub speed: u16,
    /// Trail length (character fade)
    pub fade_length: u8,
    /// Primary color (Solarized green)
    pub color: Color,
    /// Enable/disable toggle
    pub enabled: bool,
}

impl Default for MatrixRainConfig {
    fn default() -> Self {
        Self {
            density: 0.3,
            speed: 50,
            fade_length: 8,
            color: Color::Rgb(133, 153, 0), // #859900
            enabled: true,
        }
    }
}
```

### Intensity Levels

| Level | Density | Speed | Use Case |
|-------|---------|-------|----------|
| **Ambient** | 0.1 | 100ms | Idle state |
| **Low** | 0.2 | 75ms | Minor events |
| **Medium** | 0.4 | 50ms | Focus changes, selections |
| **High** | 0.6 | 30ms | Streaming, execution |
| **Burst** | 0.8 | 20ms | Completions, errors |
| **Full** | 1.0 | 20ms | Initial load only |

### Glyph Distribution

```
┌─────────────────────────────────────────────────────────────┐
│  GLYPH MIX                                                  │
├─────────────────────────────────────────────────────────────┤
│  80%  Katakana (ア-ン range: U+30A0-U+30FF)                  │
│  15%  ASCII symbols (!, @, #, $, %, ^, &, *, etc.)          │
│   5%  Nika mascots (🐔, 🐤, ⚡, 🔌, 🌌, 🔮)                  │
└─────────────────────────────────────────────────────────────┘
```

---

## Matrix Decrypt — Verb Themes

Each verb has a unique emoji chaos pool for the decrypt effect:

| Verb | Theme | Emoji Pool |
|------|-------|------------|
| `fetch:` | Pirate | 🏴‍☠️ ⚓ 🦜 💎 🗺️ 🏝️ ⛵ |
| `infer:` | Cosmic | 🌌 ✨ 🌟 💫 🔭 🪐 ☄️ |
| `exec:` | Robot | 🤖 ⚙️ 🔧 💾 🖥️ 🔩 📟 |
| `invoke:` | Electric | 🔌 ⚡ 🔋 💡 🌩️ ⚡ 🔦 |
| `agent:` | Magic | 🔮 🪄 ✨ 🌙 🦉 🧙 🎭 |
| (creative) | Unicorn | 🦄 🌈 💖 ✨ 🎨 🎪 🎠 |

---

## One Panel At A Time — Focus System

### Rule
**Only ONE panel is active at any time.** Active panel receives all keyboard input.

### Navigation
| Key | Action |
|-----|--------|
| `Tab` | Next panel in current view |
| `Shift+Tab` | Previous panel in current view |
| `1-4` | Switch to view (and focus first panel) |
| `a/h/s/m` | Switch to view (and focus first panel) |

### Visual Indicators
| State | Border Color | Border Style |
|-------|--------------|--------------|
| **Active** | Solarized blue (#268bd2) | Bold |
| **Inactive** | Solarized base01 (#586e75) | Normal |
| **Error** | Solarized red (#dc322f) | Bold |
| **Success** | Solarized green (#859900) | Bold (brief) |

### Panel IDs by View

```rust
pub enum PanelId {
    // Chat (3 panels)
    ChatMessages,
    ChatInput,
    ChatDag,         // v0.9.4

    // Home (3 panels)
    HomeFiles,
    HomePreview,
    HomeHistory,

    // Studio (2 panels)
    StudioEditor,
    StudioStructure,

    // Monitor (4 panels)
    MonitorMission,
    MonitorDag,
    MonitorReasoning,
    MonitorNovanet,
}
```

---

## Provider Modal

**Hotkey:** `Shift+P` (available in all views)

### Tabs
| Tab | Content |
|-----|---------|
| Cloud | Claude, OpenAI, Mistral, Groq, DeepSeek |
| Ollama | Local model management |
| Keys | API key configuration (masked) |
| Config | Model parameters |
| Status | Connection status, usage |

### Layout
```
╭───────────────────────────────────────────────────────────────╮
│ PROVIDER CONFIGURATION                                        │
├───────────────────────────────────────────────────────────────┤
│ [Cloud] [Ollama] [Keys] [Config] [Status]                     │
├───────────────────────────────────────────────────────────────┤
│                                                               │
│  ◉ Claude        claude-sonnet-4-6          ✓ Connected       │
│  ○ OpenAI        gpt-4o                     ✓ Connected       │
│  ○ Mistral       mistral-large-latest       ✗ No key          │
│  ○ Groq          llama-3.3-70b-versatile    ✓ Connected       │
│  ○ DeepSeek      deepseek-chat              ✗ No key          │
│  ○ Ollama        llama3.2                   ✓ Running         │
│                                                               │
├───────────────────────────────────────────────────────────────┤
│ [Enter] Select  [Tab] Next tab  [Esc] Close                   │
╰───────────────────────────────────────────────────────────────╯
```

---

## Summary Table

| View | Panels | Matrix Rain | Primary Purpose |
|------|--------|-------------|-----------------|
| **Chat** | 3 (v0.9.4) | Ambient + streaming bursts | AI conversation |
| **Home** | 3 | Fade on load + selection bursts | Workflow discovery |
| **Studio** | 2 | Bursts on save/errors | YAML editing |
| **Monitor** | 4 | Active during execution | Real-time monitoring |

---

## Version History

| Version | Changes |
|---------|---------|
| v0.8.0 | 4 views, Edit History, Session Persistence |
| v0.8.8 | Provider Modal with 5 tabs |
| v0.9.4 | ChatDagPanel added (3rd panel in Chat) |
| v0.9.5 | Matrix Rain refinements, animations |
