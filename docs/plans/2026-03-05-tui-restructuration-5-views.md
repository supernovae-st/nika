# TUI Restructuration: 8 Views → 5 Views

**Date:** 2026-03-05
**Status:** Draft - Brainstorming
**Author:** Thibaut + Claude

---

## Executive Summary

Simplify Nika TUI from 8 views to 5 views. The new WorkspaceView (3-panel: Tree + Editor + DAG Preview) makes Browse and Editor views redundant. This plan consolidates the TUI into a focused, professional experience while keeping Scheduler as a dedicated mockup/planning view.

---

## Current State (v0.20.x)

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  CURRENT: 8 VIEWS                                                             ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  [1] Browse      - File tree only (REDUNDANT with Workspace)                  ║
║  [2] Editor      - YAML editor only (REDUNDANT with Workspace)                ║
║  [3] Runner      - Workflow execution                                         ║
║  [4] Chat        - Conversational AI                                          ║
║  [5] Scheduler   - Job scheduling                                             ║
║  [6] Settings    - Configuration                                              ║
║  [7] Workspace   - 3-panel: Tree + Editor + DAG (NEW, hidden behind F9)       ║
║  [8] Split       - Side-by-side views (TO REMOVE)                             ║
║                                                                               ║
║  Problems:                                                                    ║
║  • Browse/Editor are now subsets of Workspace                                 ║
║  • Workspace is hidden (F9/F10), should be default                            ║
║  • Split adds complexity without clear value                                  ║
║  • DAG Preview is placeholder ("coming soon...")                              ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Target State (v0.21.0)

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  TARGET: 5 VIEWS                                                              ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  [1] Studio      - 3-panel: Tree + Editor + Live DAG Preview (DEFAULT)        ║
║  [2] Runner      - DAG Animé (horizontal) + TaskBox List + Output             ║
║  [3] Chat        - Conversational AI with TaskBox inline                      ║
║  [4] Scheduler   - Job scheduling mockup (planning/cron)                      ║
║  [5] Settings    - Configuration + Keybindings + Theme                        ║
║                                                                               ║
║  Key Decisions:                                                               ║
║  • DAG layout: HORIZONTAL (left-to-right flow, like StudioView)               ║
║  • Output panel: ALWAYS VISIBLE (fixed height at bottom)                      ║
║  • Scheduler: KEPT as separate view for mockups/planning                      ║
║                                                                               ║
║  Benefits:                                                                    ║
║  • Cleaner mental model: 5 views instead of 8                                 ║
║  • Studio as default = immediate productivity                                 ║
║  • Runner for execution with horizontal DAG visualization                     ║
║  • Scheduler kept separate for job planning/mockups                           ║
║  • No hidden features (all accessible via 1-5)                                ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## View Designs

### [1] Studio View (Default)

The primary editing experience. Tree on LEFT, YAML + DAG on RIGHT (YAML 70%, DAG 30% at bottom).

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│ [1]Studio●  [2]Runner  [3]Chat  [4]Scheduler  [5]Settings   workflow.nika.yaml │
├──────────────────────┬──────────────────────────────────────────────────────────┤
│                      │                                                          │
│  📁 examples/        │  workflow: example                                       │
│  ├── 📄 basic.nika   │  version: "1.0"                                          │
│  ├── 📄 advanced.ni  │                                                          │
│  └── 📁 templates/   │  tasks:                                                  │
│      ├── 📄 seo.nik  │    - id: fetch-data                                      │
│      └── 📄 geo.nik  │      fetch: https://api.com                              │
│                      │      use.ctx: raw_data                                   │
│  📁 workflows/       │                                                          │
│  ├── 📄 pipeline.ni  │    - id: transform           [YAML Editor - 70%]        │
│  └── 📄 generate.ni  │      exec: "jq '.items'"                                 │
│                      │      input: $raw_data                                    │
│  ──────────────────  │      use.ctx: transformed                                │
│  [Tree Browser]      │                                                          │
│                      │    - id: infer-summary                                   │
│  Features:           │      infer: |                                            │
│  • hjkl navigation   │        Summarize the data                                │
│  • Fuzzy search (/)  │      context: $transformed                               │
│  • File preview      │                                                          │
│  • Create/Delete     │    - id: validate                                        │
│  • VS Code-like      │      # ...                                               │
│                      ├──────────────────────────────────────────────────────────┤
│                      │  [Horizontal DAG Preview - 30%]                          │
│                      │                                                          │
│                      │  ┌──────────┐   ┌───────────┐   ┌────────┐   ┌────────┐ │
│                      │  │⚡ fetch  │──►│📟transform│──►│⚡ infer │──►│validate│ │
│                      │  └──────────┘   └───────────┘   └────────┘   └────────┘ │
│                      │                                                          │
│                      │  Updates on each edit │ Shows parse errors │ Live       │
├──────────────────────┴──────────────────────────────────────────────────────────┤
│ Studio │ Parse: ✓ OK │ Tasks: 4 │ DAG: Valid │ Cursor: 12:5        Ctrl+? Help │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Layout:** 2 columns (full height)
- **Left column:** Tree Browser (file navigation)
- **Right column:** Vertically divided
  - **Top 70%:** YAML Editor (syntax highlighting, LSP features)
  - **Bottom 30%:** Horizontal DAG Preview (left-to-right flow)

**Features:**
- **Tree Browser (left):** VS Code-like file navigation with Vim keybindings (hjkl)
- **YAML Editor (top-right):** Syntax highlighting, error underlining, auto-complete, LSP
- **Horizontal DAG Preview (bottom-right):** Real-time parsing, left-to-right flow
- **Panel resizing:** Ctrl+←/→ to adjust widths
- **Quick open:** Ctrl+P for fuzzy file search

**Keyboard Shortcuts:**
| Key | Action |
|-----|--------|
| `Tab` | Cycle focus: Tree → Editor → DAG |
| `Ctrl+B` | Toggle Tree panel |
| `Ctrl+D` | Toggle DAG panel |
| `Ctrl+S` | Save file |
| `Ctrl+R` | Run workflow (switches to Runner) |
| `/` | Fuzzy search in Tree |
| `hjkl` | Vim navigation in Tree |

---

### [2] Runner View

Comprehensive workflow execution with horizontal DAG visualization, TaskBox progress tracking,
MissionControl metrics, and full observability.

**Design inspiration:** Perplexity Sonar, Context7 (comprehensive info display)

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│ [1]Studio  [2]Runner●  [3]Chat  [4]Scheduler  [5]Settings     workflow.nika    │
├─────────────────────────────────────────────────────────────────────────────────┤
│  DAG ANIMÉ (horizontal) ──────────────────────────────────────  ⏱️ 00:02:34    │
│                                                                                 │
│   ┌────────────┐      ┌────────────┐      ┌────────────┐      ┌────────────┐   │
│   │⚡fetch-data│ ───► │📟transform │ ───► │⚡ infer    │ ───► │ validate   │   │
│   │  ✓ 1.2s   │      │  ✓ 0.8s   │      │  ▶ 45%    │      │     ○      │   │
│   └────────────┘      └────────────┘      └────────────┘      └────────────┘   │
│                                                                                 │
├───────────────────────────────┬───────────────────────────┬─────────────────────┤
│  TASK INBOX (scrollable)      │  MISSION CONTROL          │  CONTEXT STORE      │
│  ─────────────────────────    │  ──────────────────────   │  ──────────────────  │
│                               │                           │                     │
│  ┌─────────────────────────┐  │  🔌 MCP Servers           │  📦 Variables       │
│  │⚡ fetch-data         ✓ │  │  ├── novanet: ✓ 2.1s     │  ├── raw_data: 42KB │
│  │  1.2s │ 200 OK          │  │  └── filesystem: ✓ 0.3s  │  ├── items: [...]   │
│  └─────────────────────────┘  │                           │  └── summary: "..." │
│                               │  🧠 Context (ADR-033)     │                     │
│  ┌─────────────────────────┐  │  ├── Entity: qr-code     │  📊 Metrics         │
│  │📟 transform          ✓ │  │  ├── Locale: fr-FR       │  ├── Tasks: 2/4 ✓   │
│  │  0.8s │ 42 items        │  │  └── Forms: text,title   │  ├── Errors: 0      │
│  └─────────────────────────┘  │                           │  └── Retries: 1     │
│                               │  💾 Memory                │                     │
│  ┌─────────────────────────┐  │  ├── Heap: 124 MB        │                     │
│  │⚡ infer              ▶ │  │  └── DataStore: 3 keys   │                     │
│  │  claude-sonnet │ 45%    │  │                           │                     │
│  │  tokens: 1,247/892      │  │  ⏱️ Runtime               │                     │
│  │  $0.023 │ ████████░░    │  │  ├── Elapsed: 2m 34s     │                     │
│  └─────────────────────────┘  │  ├── ETA: ~1m 20s        │                     │
│                               │  └── Velocity: 1.2 t/s   │                     │
│  ┌─────────────────────────┐  │                           │                     │
│  │ validate             ○ │  │  💰 Cost                  │                     │
│  │  pending                │  │  ├── Tokens: 2,139       │                     │
│  └─────────────────────────┘  │  ├── Cost: $0.032        │                     │
│                               │  └── Budget: $0.10       │                     │
├───────────────────────────────┴───────────────────────────┴─────────────────────┤
│  OUTPUT (always visible, fixed height)                                          │
│  ─────────────────────────────────────────────────────────────────────────────  │
│  [14:32:05] fetch-data: 200 OK (42 items received)                              │
│  [14:32:06] transform: jq '.items' completed (42 items)                         │
│  [14:32:07] infer: streaming... ▶                                               │
│  > The analysis shows that the primary factors contributing to market growth    │
│    include digital transformation, mobile adoption, and contactless payments... │
├─────────────────────────────────────────────────────────────────────────────────┤
│ Runner│2/4 ✓│⏱️ 2:34│🧠claude│💰$0.03│📊2.1k tok│[e]Export [?]Help │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Layout:** 3 rows
- **Row 1 (top):** Horizontal DAG Animé with verb icons, timing, progress
- **Row 2 (middle):** 3 columns
  - **Left:** Task Inbox (scrollable TaskBox list)
  - **Center:** MissionControl (MCP, Context, Memory, Runtime, Cost)
  - **Right:** Context Store (variables, metrics)
- **Row 3 (bottom):** Output panel (streaming logs, always visible, fixed height)

**TaskBox Widget (5 Verbs with Colors):**
```
┌─────────────────────────────────────────┐
│ ⚡ task-name                         ▶  │  ← Verb icon + Status (○◎▶✓✗)
├─────────────────────────────────────────┤
│ verb: infer                             │  ← Verb type with color
│ model: claude-sonnet-4                  │  ← Provider info (if applicable)
│ tokens: 1,247 in / 892 out              │  ← Token tracking
│ cost: $0.023                            │  ← Cost estimation
│ duration: 2.3s │ ETA: ~30s              │  ← Timing info
│ progress: ████████░░ 80%                │  ← Progress bar (if streaming)
├─────────────────────────────────────────┤
│ > The analysis reveals several key...   │  ← Streaming output preview
│   patterns in the data that suggest...  │
└─────────────────────────────────────────┘

Verb Colors (existing widgets):
  ⚡ infer  — Violet (#8b5cf6)  — LLM generation
  📟 exec   — Amber (#f59e0b)   — Shell command
  🛰️ fetch  — Cyan (#06b6d4)    — HTTP request
  🔌 invoke — Emerald (#10b981) — MCP tool call
  🐔 agent  — Rose (#f43f5e)    — Multi-turn agentic
```

**Export Modal (triggered by `e`):**
```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  📤 EXPORT WORKFLOW EXECUTION                                                   │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Format:                                                                        │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐                        │
│  │ ● NDJSON │  │ ○ JSON   │  │ ○ YAML   │  │ ○ Markdown│                       │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘                        │
│                                                                                 │
│  Include:                                                                       │
│  [✓] Full event trace (24 event types)                                          │
│  [✓] Task outputs and context                                                   │
│  [✓] MCP tool call details                                                      │
│  [✓] Token usage and costs                                                      │
│  [ ] Raw LLM responses (verbose)                                                │
│                                                                                 │
│  Output: .nika/traces/workflow-2026-03-05-143205.ndjson                        │
│                                                                                 │
│  [Export]  [Cancel]                                                             │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Keyboard Shortcuts:**
| Key | Action |
|-----|--------|
| `Space` | Pause/Resume execution |
| `Ctrl+C` | Cancel current task |
| `r` | Restart workflow |
| `e` | Export trace (opens modal) |
| `Tab` | Cycle focus: DAG → TaskBox → MissionControl → Output |
| `j/k` | Navigate TaskBox list |
| `Enter` | Expand TaskBox details |
| `Ctrl+L` | Clear output |
| `?` | Help overlay |

---

### [3] Chat View

Conversational AI with inline TaskBox visualization.

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│ [1]Studio  [2]Runner  [3]Chat●  [4]Scheduler  [5]Settings                      │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  ┌─ User ───────────────────────────────────────────────────────────────────┐  │
│  │ Generate a QR code landing page for the French market                    │  │
│  └──────────────────────────────────────────────────────────────────────────┘  │
│                                                                                 │
│  ┌─ Assistant ──────────────────────────────────────────────────────────────┐  │
│  │ I'll help you generate a localized landing page. Let me gather the       │  │
│  │ context from NovaNet first.                                              │  │
│  │                                                                          │  │
│  │ ┌─────────────────────────────────────────────────────────────────────┐  │  │
│  │ │ [TaskBox] novanet_generate                                       ✓ │  │  │
│  │ │ verb: invoke │ duration: 2.1s │ entity: qr-code │ locale: fr-FR    │  │  │
│  │ └─────────────────────────────────────────────────────────────────────┘  │  │
│  │                                                                          │  │
│  │ Great! I have the denomination forms and context. Now generating...      │  │
│  │                                                                          │  │
│  │ ┌─────────────────────────────────────────────────────────────────────┐  │  │
│  │ │ [TaskBox] generate-landing                                       ▶ │  │  │
│  │ │ verb: infer │ model: claude-sonnet │ tokens: 3,421 in              │  │  │
│  │ │ progress: ████████░░░░ 65%                                         │  │  │
│  │ │ > # QR Code : La Révolution du Marketing Digital                   │  │  │
│  │ │ >                                                                  │  │  │
│  │ │ > Les codes QR transforment la manière dont les entreprises...     │  │  │
│  │ └─────────────────────────────────────────────────────────────────────┘  │  │
│  │                                                                          │  │
│  └──────────────────────────────────────────────────────────────────────────┘  │
│                                                                                 │
├─────────────────────────────────────────────────────────────────────────────────┤
│ > _                                                                             │
├─────────────────────────────────────────────────────────────────────────────────┤
│ Chat │ Model: claude-sonnet-4 │ Tokens: 5.2k │ Tools: 8           Ctrl+? Help  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Features:**
- Inline TaskBox widgets show tool calls and their progress
- Streaming responses with live token counting
- Full conversation history with search (Ctrl+F)
- Export conversation to markdown

---

### [4] Scheduler View

Job scheduling mockup for planning and cron-style execution.

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│ [1]Studio  [2]Runner  [3]Chat  [4]Scheduler●  [5]Settings                      │
├───────────────────────────────┬─────────────────────────────────────────────────┤
│                               │                                                 │
│  SCHEDULED JOBS               │  JOB DETAILS                                    │
│  ─────────────────────────    │  ─────────────────────────────────────────────  │
│                               │                                                 │
│  ┌─────────────────────────┐  │  Name: daily-seo-pipeline                       │
│  │ ● daily-seo-pipeline    │  │  Schedule: 0 6 * * * (daily at 6:00 AM)         │
│  │   ⏰ 0 6 * * *           │  │  Workflow: workflows/seo-pipeline.nika.yaml    │
│  │   Next: 2026-03-06 06:00 │  │                                                 │
│  └─────────────────────────┘  │  Last Runs:                                      │
│                               │  ├── 2026-03-05 06:00 ✓ (2m 34s)                │
│  ┌─────────────────────────┐  │  ├── 2026-03-04 06:00 ✓ (2m 12s)                │
│  │ ○ weekly-report         │  │  └── 2026-03-03 06:00 ✗ (timeout)               │
│  │   ⏰ 0 9 * * 1           │  │                                                 │
│  │   Next: 2026-03-10 09:00 │  │  Actions:                                       │
│  └─────────────────────────┘  │  [Run Now]  [Edit]  [Disable]  [Delete]         │
│                               │                                                 │
│  ┌─────────────────────────┐  │                                                 │
│  │ ○ geo-sync              │  │                                                 │
│  │   ⏰ */30 * * * *        │  │                                                 │
│  │   Next: 2026-03-05 15:00 │  │                                                 │
│  └─────────────────────────┘  │                                                 │
│                               │                                                 │
│  [+ Add Job]                  │                                                 │
│                               │                                                 │
├───────────────────────────────┴─────────────────────────────────────────────────┤
│ Scheduler │ Jobs: 3 │ Active: 2 │ Next run: 14:30                 Ctrl+? Help  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Features:**
- List of scheduled jobs with cron expressions
- Job details panel with run history
- Quick actions: Run Now, Edit, Disable, Delete
- Visual indicators: ● active, ○ inactive

**Note:** This is a mockup view for planning. Actual scheduling backend to be implemented later.

---

### [5] Settings View

Configuration, keybindings reference, and theme selection.

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│ [1]Studio  [2]Runner  [3]Chat  [4]Scheduler  [5]Settings●                      │
├────────────────────────┬────────────────────────────────────────────────────────┤
│                        │                                                        │
│  ▸ General             │  GENERAL SETTINGS                                      │
│  ▸ Editor              │  ─────────────────────────────────────────────────     │
│  ▸ Theme               │                                                        │
│  ▸ Providers           │  Default View    [Studio ▾]                            │
│  ▸ MCP Servers         │  Auto-save       [✓] Every 30 seconds                  │
│  ▸ Keybindings         │  Session Restore [✓] Restore tabs on startup           │
│  ▸ About               │                                                        │
│                        │  EDITOR                                                │
│                        │  ─────────────────────────────────────────────────     │
│                        │                                                        │
│                        │  Tab Width       [2 ▾]                                 │
│                        │  Line Numbers    [✓]                                   │
│                        │  Word Wrap       [ ]                                   │
│                        │  Vim Mode        [✓]                                   │
│                        │                                                        │
│                        │  THEME                                                 │
│                        │  ─────────────────────────────────────────────────     │
│                        │                                                        │
│                        │  Color Scheme    [Solarized Dark ▾]                    │
│                        │  UI Density      [Normal ▾]                            │
│                        │                                                        │
├────────────────────────┴────────────────────────────────────────────────────────┤
│ Settings │ Config: ~/.nika/config.toml │ Changes: unsaved       Ctrl+? Help    │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Migration Plan

### Removed Views

| Old View | Replacement | Migration |
|----------|-------------|-----------|
| Browse | Studio (Tree panel) | Tree logic → Studio.tree |
| Editor | Studio (Editor panel) | Editor logic → Studio.editor |
| Split | Removed | No replacement needed |
| Workspace | Renamed to Studio | Direct rename |

### New Keyboard Shortcuts

| Key | Old (v0.20.x) | New (v0.21.0) |
|-----|---------------|---------------|
| `1` | Browse | Studio |
| `2` | Editor | Runner |
| `3` | Runner | Chat |
| `4` | Chat | Scheduler |
| `5` | Scheduler | Settings |
| `6` | Settings | (removed) |
| `7` | Workspace | (removed, now default) |
| `8` | Split | (removed) |

---

## Existing Widget Inventory (Reuse Strategy)

The following widgets already exist and MUST be integrated into the new views.

### TaskBox Widgets (src/tui/widgets/task_box/)

Already fully implemented with 5 verbs:

```rust
pub enum TaskBox {
    Infer(InferBox),   // ⚡ Violet - LLM generation
    Exec(ExecBox),     // 📟 Amber - Shell command
    Fetch(FetchBox),   // 🛰️ Cyan - HTTP request
    Invoke(InvokeBox), // 🔌 Emerald - MCP tool call
    Agent(AgentBox),   // 🐔 Rose - Multi-turn agentic loop
}
```

**Files:**
- `mod.rs` - TaskBox enum and trait
- `infer_box.rs` - InferBox implementation
- `exec_box.rs` - ExecBox implementation
- `fetch_box.rs` - FetchBox implementation
- `invoke_box.rs` - InvokeBox implementation
- `agent_box.rs` - AgentBox implementation

### DAG Widgets (src/tui/widgets/)

```
dag.rs           — VerbType enum with icons and colors
dag_ascii.rs     — ASCII art DAG rendering
dag_layout.rs    — Horizontal node positioning algorithm
dag_node_box.rs  — Individual DAG node boxes
chat_dag_panel.rs— ChatDagPanel combining nodes + edges
```

### MissionControl Widgets (src/tui/widgets/)

```
mission_control.rs  — Shows MCP servers, Context, Memory, Runtime
activity_stack.rs   — Activity tracking
session_context.rs  — Session state display
timeline.rs         — Timeline visualization
mcp_log.rs          — MCP call logging
```

### Tree Widget (src/tui/widgets/tree/)

```
mod.rs           — TreeWidget main component
tree_state.rs    — Expanded/collapsed state
tree_node.rs     — Individual tree nodes
tree_render.rs   — Rendering logic
tree_nav.rs      — Navigation (hjkl, search)
tree_filter.rs   — Filtering/search
```

### Other Widgets

```
sparkline.rs        — Real-time metrics visualization
gauge.rs            — Progress bars
status_bar.rs       — Status bar rendering
chat_task_queue.rs  — Task queue in chat
streaming_decrypt.rs— Matrix-style text reveal
border_pulse.rs     — Running state animation
```

### Widget Integration Matrix

| Widget | Studio | Runner | Chat | Status |
|--------|--------|--------|------|--------|
| TreeWidget | ✅ Tree panel | - | - | Existing |
| TaskBox (5) | - | ✅ Task Inbox | ✅ Inline | Existing |
| DagLayout | ✅ Preview | ✅ DAG Animé | ✅ ChatDagPanel | Existing |
| MissionControl | - | ✅ Center column | - | Existing |
| ActivityStack | - | ✅ Runtime info | - | Existing |
| Sparkline | - | ✅ Velocity | - | Existing |
| StatusBar | ✅ | ✅ | ✅ | Existing |
| BorderPulse | - | ✅ Running tasks | ✅ | Existing |
| StreamingDecrypt | - | - | ✅ Infer | Existing |

### Widget Power Integration Patterns

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🔌 WIDGET POWER — LEVERAGING 39 EXISTING WIDGETS                             ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  PHILOSOPHY: "Never reinvent — compose, configure, and enhance."              ║
║                                                                               ║
║  Every widget is production-tested with comprehensive tests.                  ║
║  The v0.21 restructure REUSES these widgets with new compositions.            ║
║  New views are built by ARRANGING existing widgets, not creating new ones.    ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

#### Integration Pattern 1: Widget Embedding

Existing widgets embed directly into new view layouts:

```rust
/// Studio View composes existing widgets
impl StudioView {
    fn render_workspace(&mut self, frame: &mut Frame, area: Rect) {
        // Tree widget in left panel
        let tree_widget = TreeWidget::new(&self.tree_state)
            .with_ecosystem_detection(true)
            .with_glow_animation(self.animation.tick());
        frame.render_stateful_widget(tree_widget, panels[0], &mut self.tree_state);

        // Editor in center (new component)
        frame.render_widget(&self.editor, panels[1]);

        // DAG widget in right panel
        let dag_widget = DagAscii::new(&self.workflow.dag)
            .with_task_boxes(true)  // Embeds TaskBox widgets
            .with_live_status(true);
        frame.render_widget(dag_widget, panels[2]);
    }
}
```

#### Integration Pattern 2: TaskBox Verb Switching

The same TaskBox infrastructure renders all 5 verbs:

```rust
/// Runner View uses TaskBox for all verb types
impl RunnerView {
    fn render_task(&self, task: &Task) -> impl Widget {
        match task.verb {
            VerbType::Infer => InferBox::new(task)
                .with_streaming(&self.stream_buffer)
                .with_token_counter(true),
            VerbType::Exec => ExecBox::new(task)
                .with_output_tail(10),
            VerbType::Fetch => FetchBox::new(task)
                .with_status_code(true),
            VerbType::Invoke => InvokeBox::new(task)
                .with_mcp_server_name(true),
            VerbType::Agent => AgentBox::new(task)
                .with_turn_counter(true)
                .with_subagent_tree(true),
        }
    }
}
```

#### Integration Pattern 3: Animation System Reuse

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  ANIMATION WIDGETS → VIEW MAPPING                                               │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  BorderPulse (60fps)                                                            │
│  ├── Studio: File save indicator, validation running                            │
│  ├── Runner: Active task highlighting                                           │
│  ├── Chat: Thinking indicator                                                   │
│  ├── Scheduler: Running job indicator                                           │
│  └── Settings: Config save in progress                                          │
│                                                                                 │
│  StreamingDecrypt (character reveal)                                            │
│  ├── Chat: LLM response streaming                                               │
│  ├── Runner: Infer task output                                                  │
│  └── Studio: Preview panel streaming                                            │
│                                                                                 │
│  Sparkline (real-time metrics)                                                  │
│  ├── Runner: Token velocity, task throughput                                    │
│  ├── Chat: Response latency                                                     │
│  └── Scheduler: Job completion rate                                             │
│                                                                                 │
│  Gauge (progress)                                                               │
│  ├── Runner: Workflow progress                                                  │
│  ├── Chat: Token usage                                                          │
│  └── Scheduler: Job queue depth                                                 │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

#### Integration Pattern 4: MissionControl Decomposition

MissionControl splits across views in v0.21:

```rust
/// MissionControl panels distributed to views
pub struct MissionControlComponents {
    // These components exist — redistribute to views
    mcp_panel: McpPanel,         // → Runner (MCP status)
    context_panel: ContextPanel, // → Chat (context display)
    memory_panel: MemoryPanel,   // → Settings (memory config)
    runtime_panel: RuntimePanel, // → Runner (runtime stats)
}

// Runner gets MCP + Runtime
impl RunnerView {
    fn status_bar(&self) -> StatusBar {
        StatusBar::new()
            .with_mcp_status(&self.mission_control.mcp_panel)
            .with_runtime_info(&self.mission_control.runtime_panel)
    }
}

// Chat gets Context
impl ChatView {
    fn render_context(&self) -> ContextDisplay {
        self.mission_control.context_panel.as_widget()
    }
}
```

#### Integration Pattern 5: Tree Widget View Modes

The Tree widget adapts its display per view:

```rust
/// Tree rendering modes for different views
pub enum TreeDisplayMode {
    /// Studio: Full ecosystem detection + glow + .nika expansion
    Studio {
        ecosystem_detection: bool,  // true
        glow_animation: bool,       // true
        auto_expand_nika: bool,     // true
    },
    /// Runner: Workflow-focused with task status
    Runner {
        show_task_status: bool,     // true
        filter_nika_workflows: bool, // true
    },
    /// Scheduler: Job-focused with schedule info
    Scheduler {
        show_schedules: bool,       // true
        show_last_run: bool,        // true
    },
}
```

#### Integration Pattern 6: Shared StatusBar

All views share a unified StatusBar with view-specific sections:

```rust
/// StatusBar adapts content per view
impl StatusBar {
    fn render_for_view(&self, view: &View) -> impl Widget {
        match view {
            View::Studio => self.with_sections(&[
                Section::EditorPosition,  // Line:Col
                Section::ValidationStatus, // ✅ Valid / ❌ 3 errors
                Section::McpStatus,        // 🔌 MCP: 2 servers
            ]),
            View::Runner => self.with_sections(&[
                Section::WorkflowProgress, // 3/7 tasks
                Section::TokenUsage,       // 💰 $0.02
                Section::RuntimeStats,     // ⏱️ 2.1s
            ]),
            View::Chat => self.with_sections(&[
                Section::Provider,         // 🧠 claude-sonnet-4
                Section::TokenBudget,      // 💬 4.2k tokens
                Section::ContextSize,      // 📄 Context: 12k
            ]),
            View::Scheduler => self.with_sections(&[
                Section::NextJob,          // Next: deploy in 5m
                Section::QueueDepth,       // Queue: 3 jobs
                Section::SuccessRate,      // ✅ 98% success
            ]),
            View::Settings => self.with_sections(&[
                Section::ConfigPath,       // ~/.nika/config.toml
                Section::LastSaved,        // Saved 2m ago
            ]),
        }
    }
}
```

#### Widget Reuse Summary

| Widget Category | Widget Count | Primary Views | Reuse Strategy |
|-----------------|--------------|---------------|----------------|
| **TaskBox** | 5 | Runner, Chat | Embed directly with verb dispatch |
| **DAG** | 5 | Studio, Runner, Chat | DagAscii for preview, full for execution |
| **MissionControl** | 5 | Runner, Settings | Decompose panels to relevant views |
| **Tree** | 6 | Studio, Scheduler | Mode-based rendering |
| **Animation** | 4 | All views | Shared AnimationTicker |
| **Utility** | 14 | All views | StatusBar composition |
| **TOTAL** | **39** | — | **100% reuse, 0 new widgets** |

---

## Tree View Design (Advanced) — VS Code-Class File Browser

The Tree Widget provides a **VS Code-class file browsing experience** with ecosystem-aware
styling, premium glow animations, and intelligent detection of SuperNovae project structures.

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🌲 TREE WIDGET — DESIGN PHILOSOPHY                                           ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  1. ECOSYSTEM-FIRST: Recognize SuperNovae files and treat them special        ║
║  2. VISUAL HIERARCHY: Important files stand out with glow + premium icons     ║
║  3. IDE PARITY: Match VS Code/JetBrains navigation patterns                   ║
║  4. PERFORMANCE: Lazy loading, virtual scrolling for large directories        ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Ecosystem Detection (35 NodeKind Variants)

The tree widget automatically detects file types and applies visual treatment:

```rust
/// Complete NodeKind enum for ecosystem-aware file rendering
pub enum NodeKind {
    // ═══════════════════════════════════════════════════════════════════════
    // NIKA ECOSYSTEM (Premium glow treatment — Yellow #b58900)
    // ═══════════════════════════════════════════════════════════════════════
    NikaWorkflow,       // *.nika.yaml     → ✨ Gold sparkle + bold
    NikaFolder,         // .nika/          → 🦋 Butterfly + auto-expand
    NikaConfig,         // .nika/config.toml → ⚙️ Gear (green accent)
    NikaSessions,       // .nika/sessions/ → 💬 Chat bubble
    NikaTraces,         // .nika/traces/   → 📊 Chart
    NikaArtifacts,      // .nika/artifacts/ → 📦 Package
    NikaCache,          // .nika/cache/    → 💾 Disk (muted)
    SonAgent,           // *.son           → 🐔 Space Chicken + glow
    SkillFile,          // *.skill.md      → 📜 Scroll + glow

    // ═══════════════════════════════════════════════════════════════════════
    // SPN CLI ECOSYSTEM (Orange #cb4b16)
    // ═══════════════════════════════════════════════════════════════════════
    SpnFolder,          // .spn/           → ⚡ Lightning
    SpnManifest,        // spn.yaml        → 📦 Package + glow
    SpnMcpConfig,       // mcp.yaml        → 🔌 Plug
    SpnPackages,        // .spn/packages/  → 📚 Library
    SpnLockfile,        // spn.lock        → 🔒 Lock

    // ═══════════════════════════════════════════════════════════════════════
    // NOVANET ECOSYSTEM (Magenta #d33682)
    // ═══════════════════════════════════════════════════════════════════════
    NovanetFolder,      // .novanet/       → 🧠 Brain
    BrainFolder,        // brain/          → 💡 Lightbulb
    ModelsFolder,       // brain/models/   → 📐 Compass
    SeedFolder,         // brain/seed/     → 🌱 Seedling
    SchemaYaml,         // *.schema.yaml   → 🗂️ Schema

    // ═══════════════════════════════════════════════════════════════════════
    // CLAUDE CODE DX (Violet #6c71c4)
    // ═══════════════════════════════════════════════════════════════════════
    ClaudeFolder,       // .claude/        → 🤖 Robot
    ClaudeMd,           // CLAUDE.md       → 📋 Clipboard + glow
    ClaudeRules,        // .claude/rules/  → 📏 Ruler
    ClaudeSkills,       // .claude/skills/ → ⚡ Skills
    ClaudeSettings,     // .claude/settings.json → ⚙️ Settings

    // ═══════════════════════════════════════════════════════════════════════
    // STANDARD FILES (Muted Solarized colors)
    // ═══════════════════════════════════════════════════════════════════════
    Directory,          // folder          → 📁 Blue (#268bd2)
    YamlFile,           // *.yaml          → 📄 Base0 (#839496)
    RustFile,           // *.rs            → 🦀 Orange (#cb4b16)
    TypeScriptFile,     // *.ts, *.tsx     → 🔷 Blue (#268bd2)
    JsonFile,           // *.json          → ⚙️ Green (#859900)
    TomlFile,           // *.toml          → 🔧 Cyan (#2aa198)
    MarkdownFile,       // *.md            → 📝 Cyan (#2aa198)
    ShellScript,        // *.sh            → 🐚 Green (#859900)
    Gitignore,          // .gitignore      → 🚫 Muted
    Hidden,             // .* (other)      → 👻 Base01 (#586e75) dimmed
    Unknown,            // fallback        → 📄 Gray
}

impl NodeKind {
    /// Returns true if this file type gets premium glow animation
    pub fn is_ecosystem(&self) -> bool {
        matches!(self,
            Self::NikaWorkflow | Self::SonAgent | Self::SkillFile |
            Self::SpnManifest | Self::ClaudeMd
        )
    }

    /// Returns true if folder should auto-expand on first load
    pub fn should_auto_expand(&self) -> bool {
        matches!(self, Self::NikaFolder | Self::ClaudeFolder)
    }
}
```

### `.nika` Folder Structure Presentation

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🦋 .NIKA FOLDER — NIKA'S HOME DIRECTORY                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  The .nika/ folder is Nika's local state directory. Like VS Code's            ║
║  .vscode/ or Git's .git/, it stores configuration, session data, and          ║
║  execution artifacts. Tree Widget gives it PREMIUM TREATMENT.                 ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  📁 .NIKA FOLDER STRUCTURE                                                      │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  🦋 .nika/                         ← Root folder (always expanded by default)   │
│  │                                                                              │
│  ├── ⚙️ config.toml                ← User preferences (theme, editor, etc.)     │
│  │   ├── [tui] theme, font_size, ui_density                                    │
│  │   ├── [chat] auto_save, session_limit, history_limit                        │
│  │   ├── [studio] auto_format, tab_width, line_numbers                         │
│  │   └── [paths] custom session/trace directories                              │
│  │                                                                              │
│  ├── 📁 sessions/                  ← Chat session persistence                   │
│  │   ├── 📄 chat-2026-03-05.json   ← Today's session                           │
│  │   ├── 📄 chat-2026-03-04.json   ← Yesterday's session                       │
│  │   └── ... (max 50, auto-cleanup by LRU)                                     │
│  │                                                                              │
│  ├── 📁 traces/                    ← Workflow execution traces (NDJSON)         │
│  │   ├── 📊 workflow-1709654321.ndjson  ← 24 event types                       │
│  │   ├── 📊 workflow-1709654200.ndjson                                         │
│  │   └── ... (searchable with `nika trace list`)                               │
│  │                                                                              │
│  ├── 📁 artifacts/                 ← Task output files                          │
│  │   ├── 📦 {{task_id}}/           ← Per-task output directory                 │
│  │   │   ├── output.json                                                       │
│  │   │   └── metadata.yaml                                                     │
│  │   └── manifest.json             ← Artifact index                            │
│  │                                                                              │
│  └── 📁 cache/                     ← MCP server cache                           │
│      ├── 💾 novanet-schema.json    ← Schema cache (TTL: 5min)                  │
│      ├── 💾 tool-definitions.json  ← Tool def cache                            │
│      └── ... (auto-invalidated on server restart)                              │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  🎨 .NIKA FOLDER VISUAL TREATMENT                                               │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Premium Treatment (Ecosystem Files):                                           │
│  ─────────────────────────────────────────────────────────────────────────────  │
│  • .nika/ root       → 🦋 Butterfly + Yellow glow + AUTO-EXPAND                │
│  • config.toml       → ⚙️ Gear + Green (#859900) + Bold                        │
│  • *.nika.yaml       → ✨ Gold sparkle + Yellow glow on selection              │
│                                                                                 │
│  Standard Treatment (Data Files):                                               │
│  ─────────────────────────────────────────────────────────────────────────────  │
│  • sessions/         → 💬 Chat bubble + Blue (#268bd2)                         │
│  • traces/           → 📊 Chart + Cyan (#2aa198)                               │
│  • artifacts/        → 📦 Package + Orange (#cb4b16)                           │
│  • cache/            → 💾 Disk + Muted gray (Base01)                           │
│                                                                                 │
│  File Type Styling:                                                             │
│  ─────────────────────────────────────────────────────────────────────────────  │
│  • *.json inside .nika/  → Muted cyan, smaller font weight                     │
│  • *.ndjson traces       → Cyan with chart icon                                │
│  • manifest.json         → Orange (important metadata)                         │
│                                                                                 │
│  Interaction Patterns:                                                          │
│  ─────────────────────────────────────────────────────────────────────────────  │
│  • .nika/ auto-expands on TUI launch (shouldAutoExpand() → true)               │
│  • sessions/ shows date badges inline (e.g., "Today", "Yesterday")             │
│  • traces/ shows success/failure badge (✅/❌) from last event                 │
│  • artifacts/ shows file count badge                                           │
│  • cache/ collapsed by default (low priority)                                  │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

```rust
/// .nika folder-specific rendering logic
impl TreeWidget<'_> {
    fn render_nika_folder(&self, node: &TreeNode) -> Span<'static> {
        match node.kind {
            NodeKind::NikaFolder => {
                // Premium: Butterfly + yellow glow
                Span::styled("🦋 .nika/", Style::default()
                    .fg(SOLARIZED_YELLOW)
                    .add_modifier(Modifier::BOLD))
            }
            NodeKind::NikaConfig => {
                // Config: Gear + green accent
                Span::styled("⚙️ config.toml", Style::default()
                    .fg(SOLARIZED_GREEN))
            }
            NodeKind::NikaSessions => {
                // Sessions: Chat bubble + badge
                let badge = self.get_session_count_badge();
                Span::raw(format!("💬 sessions/ {}", badge))
            }
            NodeKind::NikaTraces => {
                // Traces: Chart + last status
                let status = self.get_last_trace_status();
                Span::raw(format!("📊 traces/ {}", status))
            }
            NodeKind::NikaArtifacts => {
                // Artifacts: Package + count
                let count = self.get_artifact_count();
                Span::raw(format!("📦 artifacts/ ({})", count))
            }
            NodeKind::NikaCache => {
                // Cache: Disk + muted (low priority)
                Span::styled("💾 cache/", Style::default()
                    .fg(SOLARIZED_BASE01))
            }
            _ => Span::raw(node.name.clone()),
        }
    }

    fn get_session_count_badge(&self) -> &'static str {
        // Returns "Today" / "2 sessions" / etc.
        "Today"
    }

    fn get_last_trace_status(&self) -> &'static str {
        // Returns ✅ or ❌ based on last trace
        "✅"
    }

    fn get_artifact_count(&self) -> usize {
        // Count files in artifacts/
        0
    }
}
```

### Glow Animation System

Premium ecosystem files get 60fps glow animation when selected:

```rust
// AnimationTicker provides coordinated 60fps timing
impl TreeWidget<'_> {
    fn render_node(&self, node: &TreeNode, is_selected: bool) {
        let icon_color = self.colors.icon_color(&node.kind);

        // Apply glow for ecosystem files on selection
        if node.kind.is_ecosystem() && is_selected {
            if let Some(ticker) = self.ticker {
                let glow = ticker.glow_factor();  // 0.0-1.0 pulsing
                let interpolated = interpolate_color(icon_color, Color::White, glow - 0.7);
                // Apply bold + glow color
            }
        }
    }
}
```

**Glow treatment for:**
- `NikaWorkflow` (✨ `.nika.yaml` files)
- `SonAgent` (🐔 `.son` files)
- `SkillFile` (📜 `.skill.md` files)
- `SpnManifest` (📦 `spn.yaml`)
- `ClaudeMd` (📋 `CLAUDE.md`)

### Solarized Color Palette

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  🎨 TREE WIDGET COLOR SCHEME (Solarized)                                        │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Ecosystem Colors (bright, glow-enabled):                                       │
│  ├── NIKA ecosystem      → Yellow (#b58900)   🦋 ✨ 🐔 📜                       │
│  ├── SPN CLI             → Orange (#cb4b16)   ⚡ 📦 🔌                          │
│  ├── NovaNet             → Magenta (#d33682)  🧠 💡 📐 🌱                       │
│  └── Claude Code         → Violet (#6c71c4)   🤖 📋                            │
│                                                                                 │
│  Standard Colors (muted):                                                       │
│  ├── Directories         → Blue (#268bd2)                                       │
│  ├── Config files        → Green (#859900)                                      │
│  ├── Data files          → Cyan (#2aa198)                                       │
│  ├── Code files          → Base0 (#839496)                                      │
│  └── Hidden files        → Base01 (#586e75) (dimmed)                            │
│                                                                                 │
│  Theme Support:                                                                 │
│  ├── Solarized Dark      → Dark background (#002b36)                            │
│  └── Solarized Light     → Light background (#fdf6e3)                           │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Tree Widget Features

| Feature | Shortcut | Description |
|---------|----------|-------------|
| **Navigation** | `h/j/k/l` | Vim-style tree navigation |
| **Expand/Collapse** | `Enter` / `Space` | Toggle folder expansion |
| **Fuzzy Search** | `/` | Filter files by name (nucleo) |
| **Quick Open** | `Ctrl+P` | Jump to file by path |
| **Ecosystem Filter** | `W` | Show only workflows (*.nika.yaml) |
| **Agents Filter** | `A` | Show only agents (*.son) |
| **Errors Filter** | `E` | Show files with errors |
| **Clear Filter** | `0` | Reset all filters |
| **Create File** | `n` | New file dialog |
| **Delete File** | `d` | Delete with confirmation |
| **Rename** | `r` | Rename file/folder |
| **Copy Path** | `y` | Yank path to clipboard |

### NerdFont Icons (Optional)

If the terminal supports NerdFont, the tree uses icon font instead of emoji:

```
Emoji Mode:                    NerdFont Mode:
🦋 .nika/                      󰈸 .nika/
├── ✨ workflow.nika.yaml      ├──  workflow.nika.yaml
├── 🐔 agent.son               ├──  agent.son
├── 📁 workflows/              ├──  workflows/
│   └── 📄 pipeline.yaml       │   └──  pipeline.yaml
└── 📁 .claude/                └──  .claude/
    └── 📋 CLAUDE.md               └──  CLAUDE.md
```

### Git Status Integration

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  GIT STATUS BADGES                                                              │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  ✨ workflow.nika.yaml  M      ← Modified (yellow badge)                        │
│  🐔 new-agent.son       A      ← Added/Staged (green badge)                     │
│  📄 old-file.yaml       D      ← Deleted (red badge)                            │
│  📄 untracked.yaml      ?      ← Untracked (gray badge)                         │
│  📄 conflict.yaml       !      ← Conflict (red+bold badge)                      │
│                                                                                 │
│  Badge Positioning:                                                             │
│  └── Right-aligned after filename, before scroll indicator                      │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Editor Design (IDE-Class)

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  📝 EDITOR — DESIGN PHILOSOPHY                                                ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  The Studio Editor aims to be a professional IDE-class experience,            ║
║  rivaling VS Code for YAML workflow editing.                                  ║
║                                                                               ║
║  PRINCIPLES:                                                                  ║
║  1. ZERO-CONFIG IDE: Works perfectly out of the box, no setup needed         ║
║  2. REAL-TIME FEEDBACK: Every keystroke triggers validation pipeline         ║
║  3. CONTEXTUAL INTELLIGENCE: Autocomplete knows your workflow structure      ║
║  4. VISUAL DAG: See your workflow graph update as you type                   ║
║  5. ERROR RECOVERY: Parser continues on errors, shows partial results        ║
║                                                                               ║
║  PARITY TARGETS:                                                              ║
║  • VS Code YAML extension (Red Hat) — Schema validation, hover docs          ║
║  • IntelliJ YAML support — Smart completion, refactoring                     ║
║  • Helix editor — Modal editing, multiple cursors (future)                   ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Editor Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  📝 STUDIO EDITOR ARCHITECTURE                                                  │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  ┌─────────────────────────────────────────────────────────────────────────┐   │
│  │  TAB BAR                                                                │   │
│  │  [workflow.nika.yaml ●] [agent.son] [config.toml]        [+]  [×]      │   │
│  └─────────────────────────────────────────────────────────────────────────┘   │
│  ┌─────────────────────────────────────────────────────────────────────────┐   │
│  │  LINE │ CONTENT                                              │ MINIMAP │   │
│  │  ─────┼──────────────────────────────────────────────────────┼─────────│   │
│  │    1  │ schema: "nika/workflow@0.9"                          │ ▓▓▓▓▓▓  │   │
│  │    2  │ provider: claude                                     │ ▓▓▓▓    │   │
│  │    3  │                                                      │         │   │
│  │    4  │ tasks:                                               │ ▓▓▓▓▓   │   │
│  │    5  │   - id: fetch-data                                   │ ▓▓▓▓▓▓▓ │   │
│  │    6  │     fetch: https://api.example.com~~error~~          │ ▓▓▓▓▓▓  │   │
│  │         └── 🔴 Invalid URL: missing protocol                 │         │   │
│  │    7  │     use.ctx: raw_data                                │ ▓▓▓▓▓   │   │
│  │    8  │                                                      │         │   │
│  │    9  │   - id: transform                                    │ ▓▓▓▓▓▓▓ │   │
│  │   10  │     exec: "jq '.items'"                              │ ▓▓▓▓▓▓  │   │
│  │        └── 💡 Consider: shell: false for security            │         │   │
│  └─────────────────────────────────────────────────────────────┴─────────┘   │
│  ┌─────────────────────────────────────────────────────────────────────────┐   │
│  │  DIAGNOSTICS PANEL (toggle with Ctrl+`)                                 │   │
│  │  ─────────────────────────────────────────────────────────────────────  │   │
│  │  🔴 Error [L6:12] Invalid URL: missing protocol (NIKA-042)             │   │
│  │  💡 Hint  [L10:5] Consider shell: false for exec: commands             │   │
│  │  ⚠️ Warn  [L15:3] Task 'validate' has no dependencies                   │   │
│  └─────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### LSP Integration Features

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  🔧 LSP FEATURES                                                                │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  1. SYNTAX HIGHLIGHTING (Tree-sitter based)                                     │
│  ───────────────────────────────────────────────────────────────────────────    │
│  • YAML keywords     → Purple (schema:, provider:, tasks:)                      │
│  • String values     → Green                                                    │
│  • Numbers           → Orange                                                   │
│  • Comments          → Gray/italic                                              │
│  • Nika verbs        → Bold + Verb color (infer: violet, exec: amber, etc.)     │
│  • Template vars     → Cyan ({{use.alias}})                                     │
│  • MCP references    → Magenta (novanet_generate, filesystem)                   │
│                                                                                 │
│  2. INLINE DIAGNOSTICS (miette v7.6)                                            │
│  ───────────────────────────────────────────────────────────────────────────    │
│  • Error underlines  → Red wavy underline with hover tooltip                    │
│  • Warnings          → Yellow wavy underline                                    │
│  • Hints             → Blue dotted underline                                    │
│  • Info              → Gray dotted underline                                    │
│  • Gutter icons      → 🔴 ⚠️ 💡 ℹ️ in line number column                        │
│                                                                                 │
│  3. AUTOCOMPLETE (Ctrl+Space)                                                   │
│  ───────────────────────────────────────────────────────────────────────────    │
│  • Schema fields     → provider:, tasks:, mcp:, flows:                          │
│  • Verb completion   → infer:, exec:, fetch:, invoke:, agent:                   │
│  • Task references   → use: { alias: <task_id> } completion                     │
│  • MCP tools         → invoke: <mcp_tool_name> from connected servers           │
│  • Template vars     → {{use.alias}} from defined bindings                      │
│  • Schema values     → provider: [claude, openai, mistral, ...]                 │
│                                                                                 │
│  4. GO-TO-DEFINITION (Ctrl+Click / F12)                                         │
│  ───────────────────────────────────────────────────────────────────────────    │
│  • use: alias        → Jump to source task definition                           │
│  • include: path     → Open included workflow file                              │
│  • context: file     → Open context file                                        │
│  • mcp: server       → Jump to MCP config in mcp: block                         │
│                                                                                 │
│  5. HOVER DOCUMENTATION                                                         │
│  ───────────────────────────────────────────────────────────────────────────    │
│  • Schema fields     → Show JSON Schema description + type                      │
│  • Nika verbs        → Show verb documentation + examples                       │
│  • Task references   → Show task output type + last value                       │
│  • MCP tools         → Show tool description + parameters                       │
│                                                                                 │
│  6. CODE ACTIONS (Ctrl+.)                                                       │
│  ───────────────────────────────────────────────────────────────────────────    │
│  • Quick fix         → Auto-fix known error patterns                            │
│  • Add missing field → Insert required field with default                       │
│  • Extract task      → Move selection to new task with use: binding             │
│  • Add flow edge     → Generate flows: entry for dependency                     │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Real-Time Validation

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  ✅ REAL-TIME VALIDATION PIPELINE                                               │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  On Every Keystroke (debounced 100ms):                                          │
│  ┌────────────┐   ┌────────────┐   ┌────────────┐   ┌────────────┐             │
│  │  YAML      │──►│  AST       │──►│  Schema    │──►│  DAG       │             │
│  │  Parse     │   │  Validate  │   │  Validate  │   │  Validate  │             │
│  └────────────┘   └────────────┘   └────────────┘   └────────────┘             │
│       │                │                │                │                      │
│       ▼                ▼                ▼                ▼                      │
│  ┌────────────────────────────────────────────────────────────────┐            │
│  │                    DIAGNOSTICS COLLECTOR                       │            │
│  │  Errors: 2  │  Warnings: 1  │  Hints: 3  │  Info: 0            │            │
│  └────────────────────────────────────────────────────────────────┘            │
│       │                                                                         │
│       ▼                                                                         │
│  ┌────────────────────────────────────────────────────────────────┐            │
│  │                    UI UPDATE                                   │            │
│  │  • Update inline underlines                                    │            │
│  │  • Update gutter icons                                         │            │
│  │  • Update diagnostics panel                                    │            │
│  │  • Update DAG preview (highlight error nodes)                  │            │
│  │  • Update status bar (Parse: ✓ OK / ✗ 2 errors)               │            │
│  └────────────────────────────────────────────────────────────────┘            │
│                                                                                 │
│  Validation Types:                                                              │
│  ─────────────────────────────────────────────────────────────────────────────  │
│  • YAML syntax      → serde_yaml parse errors                                   │
│  • Schema @0.9      → JSON Schema validation (jsonschema crate)                 │
│  • Task references  → Verify use: aliases exist                                 │
│  • DAG cycles       → petgraph cycle detection                                  │
│  • MCP tools        → Verify tool names against connected servers               │
│  • Template syntax  → Verify {{use.alias}} is valid                             │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

```rust
/// Validation pipeline implementation
pub struct ValidationPipeline {
    debounce: Duration,          // 100ms default
    last_input: Instant,
    pending_validation: Option<JoinHandle<ValidationResult>>,
}

/// Diagnostic severity levels (LSP compatible)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,      // 🔴 Blocks execution
    Warning,    // ⚠️ Potential issues
    Hint,       // 💡 Suggestions
    Info,       // ℹ️ Informational
}

/// Single diagnostic with span information
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub span: Span,              // line:col start..end
    pub code: Option<String>,    // NIKA-042, etc.
    pub source: &'static str,    // "yaml", "schema", "dag", "mcp"
    pub fix: Option<QuickFix>,   // Auto-fix suggestion
}

/// Quick fix for automatic error resolution
pub struct QuickFix {
    pub title: String,
    pub edit: TextEdit,
}

impl ValidationPipeline {
    /// Run 4-stage validation pipeline
    pub async fn validate(&self, content: &str) -> ValidationResult {
        let mut diagnostics = Vec::new();

        // Stage 1: YAML Parse
        let yaml_result = self.parse_yaml(content);
        if let Err(e) = &yaml_result {
            diagnostics.push(self.yaml_error_to_diagnostic(e));
            // Continue with partial AST if possible
        }

        // Stage 2: AST Validation (semantic)
        if let Ok(ast) = &yaml_result {
            diagnostics.extend(self.validate_ast(ast));
        }

        // Stage 3: Schema Validation (JSON Schema @0.9)
        if let Ok(ast) = &yaml_result {
            diagnostics.extend(self.validate_schema(ast));
        }

        // Stage 4: DAG Validation (cycles, references)
        if let Ok(ast) = &yaml_result {
            diagnostics.extend(self.validate_dag(ast));
        }

        ValidationResult {
            diagnostics,
            ast: yaml_result.ok(),
            dag: self.build_dag_preview(&yaml_result),
        }
    }
}
```

### Editor Keyboard Shortcuts

| Category | Shortcut | Action |
|----------|----------|--------|
| **Navigation** | `Ctrl+G` | Go to line number |
| | `Ctrl+F` | Find in file |
| | `Ctrl+H` | Find and replace |
| | `Ctrl+P` | Quick open file |
| | `F12` | Go to definition |
| | `Alt+←` | Go back |
| | `Alt+→` | Go forward |
| **Editing** | `Ctrl+Z` | Undo |
| | `Ctrl+Y` | Redo |
| | `Ctrl+/` | Toggle comment |
| | `Ctrl+D` | Duplicate line |
| | `Alt+↑/↓` | Move line up/down |
| | `Ctrl+Shift+K` | Delete line |
| **Autocomplete** | `Ctrl+Space` | Trigger autocomplete |
| | `Tab` | Accept suggestion |
| | `Esc` | Dismiss popup |
| **Diagnostics** | `Ctrl+`` ` | Toggle diagnostics panel |
| | `F8` | Go to next error |
| | `Shift+F8` | Go to previous error |
| | `Ctrl+.` | Quick fix |
| **View** | `Ctrl+B` | Toggle tree panel |
| | `Ctrl+D` | Toggle DAG panel |
| | `Ctrl+M` | Toggle minimap |
| | `Ctrl+\` | Split editor |

### Undo/Redo with Intelligent Coalescing

```rust
// EditHistory coalesces rapid keystrokes (500ms timeout)
pub struct EditHistory {
    history: Vec<EditState>,
    current: usize,
    coalesce_timeout: Duration,  // 500ms default
    last_edit: Instant,
}

impl EditHistory {
    pub fn push(&mut self, state: EditState) {
        // If within coalesce window and same type, merge
        if self.last_edit.elapsed() < self.coalesce_timeout {
            if let Some(last) = self.history.last_mut() {
                if last.can_merge(&state) {
                    last.merge(state);
                    return;
                }
            }
        }
        // Otherwise push new state
        self.history.truncate(self.current + 1);
        self.history.push(state);
        self.current += 1;
        self.last_edit = Instant::now();
    }
}
```

**Coalescing rules:**
- Sequential character insertions → Merged into single "typing" action
- Sequential deletions → Merged into single "delete" action
- Paste operations → Always separate action
- Cursor jumps → Start new action group

### Minimap (Optional)

```
┌────────────────────────────────────────────────────┐
│  MINIMAP (right edge, 80px width)                 │
├────────────────────────────────────────────────────┤
│                                                    │
│  ▓▓▓▓▓▓▓▓    ← schema: line (darker)              │
│  ▓▓▓▓▓▓      ← provider: line                     │
│               ← empty line                         │
│  ▓▓▓▓▓▓▓     ← tasks: line                        │
│  ▓▓▓▓▓▓▓▓▓   ← task definition                    │
│  ▓▓▓▓▓▓▓▓    ← verb line                          │
│  ████████    ← CURRENT VIEWPORT (highlighted)     │
│  ████████                                          │
│  ▓▓▓▓▓▓▓▓▓                                        │
│  🔴 ▓▓▓▓▓▓   ← Error marker (red dot)             │
│  ▓▓▓▓▓▓▓                                          │
│  ⚠️ ▓▓▓▓▓▓   ← Warning marker (yellow dot)        │
│                                                    │
│  Click to jump to position                         │
│  Drag to scroll                                    │
│                                                    │
└────────────────────────────────────────────────────┘
```

### Status Bar Integration

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│ Studio│Parse: ✓ OK│Tasks: 4│DAG: Valid│Ln 12, Col 5│UTF-8│YAML│ Ctrl+? Help │
└─────────────────────────────────────────────────────────────────────────────────┘
         │           │        │          │             │     │
         │           │        │          │             │     └── File type
         │           │        │          │             └── Encoding
         │           │        │          └── Cursor position
         │           │        └── DAG validation status
         │           └── Task count in workflow
         └── Parse status (✓ OK / ✗ N errors)
```

---

## Implementation Phases

### Phase 1: View Consolidation (v0.21.0-alpha.1)

**Files to modify:**
- `src/tui/views/mod.rs` - Reduce TuiView enum from 8 to 5
- `src/tui/views/workspace.rs` → `src/tui/views/studio.rs` - Rename
- `src/tui/views/runner.rs` - Add horizontal DAG + TaskBox List panels
- Delete: `src/tui/views/browse.rs`, `src/tui/views/editor.rs`, `src/tui/views/split.rs`
- Keep: `src/tui/views/scheduler.rs` (for mockup)

**Tasks:**
- [ ] Rename WorkspaceView → StudioView
- [ ] Update TuiView enum to 5 variants
- [ ] Update keyboard shortcuts (1-5)
- [ ] Remove Browse, Editor, Split views
- [ ] Make Studio the default view
- [ ] Update help text and documentation

### Phase 2: Live DAG Preview (v0.21.0-alpha.2)

**Current state:** Placeholder in `workspace.rs:220-258`
```rust
fn render_dag_panel(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
    // Placeholder content - will be replaced with actual DAG visualization
    let content = vec![
        Line::from("  DAG visualization coming soon..."),
    ];
}
```

**Implementation:**
- [ ] Create `src/tui/widgets/dag/` module
- [ ] Implement DAG parser (AST → visual nodes)
- [ ] Real-time parsing on editor changes
- [ ] Error highlighting (red nodes for parse errors)
- [ ] Dependency arrows between nodes
- [ ] **Horizontal layout** (left-to-right flow)

**Files:**
- `src/tui/widgets/dag/mod.rs` - Module root
- `src/tui/widgets/dag/parser.rs` - AST → DAG nodes
- `src/tui/widgets/dag/render.rs` - Ratatui rendering
- `src/tui/widgets/dag/layout.rs` - Horizontal node positioning

### Phase 3: TaskBox Widget (v0.21.0-alpha.3)

**Create reusable TaskBox widget for Runner and Chat views.**

**Files:**
- `src/tui/widgets/taskbox/mod.rs` - TaskBox widget
- `src/tui/widgets/taskbox/render.rs` - Rendering logic
- `src/tui/widgets/taskbox/state.rs` - Expand/collapse state

**TaskBox variants:**
```rust
pub enum TaskBoxState {
    Pending,      // ○ Gray, waiting
    Ready,        // ◎ Yellow, dependencies met
    Running(f32), // ▶ Blue, with progress 0.0-1.0
    Done,         // ✓ Green, completed
    Failed(String), // ✗ Red, with error message
}

pub struct TaskBox {
    pub id: String,
    pub verb: VerbType,
    pub state: TaskBoxState,
    pub duration: Option<Duration>,
    pub tokens: Option<TokenUsage>,
    pub output_preview: Option<String>,
    pub expanded: bool,
}
```

### Phase 4: Runner Layout (v0.21.0-alpha.4)

**Implement the full Runner view with horizontal DAG.**

**Layout:**
```
┌─────────────────────────────────────────────────────────────────┐
│  HORIZONTAL DAG ANIMÉ                                           │
│  [fetch] ──► [transform] ──► [infer] ──► [validate]            │
├───────────────────────────────┬─────────────────────────────────┤
│  TASKBOX LIST                 │  OUTPUT (always visible)        │
│  (scrollable)                 │  (fixed height)                 │
└───────────────────────────────┴─────────────────────────────────┘
```

**Tasks:**
- [ ] Implement 2-row layout in Runner
- [ ] Row 1: Horizontal DAG with boxes and arrows
- [ ] Row 2 left: TaskBox List (scrollable)
- [ ] Row 2 right: Output panel (always visible, fixed height)
- [ ] Sync DAG node selection with TaskBox highlight

### Phase 5: Scheduler Mockup (v0.21.0-alpha.5)

**Keep Scheduler as mockup view for planning.**

**Tasks:**
- [ ] Design job list UI
- [ ] Design job details panel
- [ ] Add placeholder actions (Run Now, Edit, etc.)
- [ ] Display cron expressions and next run times

### Phase 6: Polish & Testing (v0.21.0)

- [ ] Update all tests for new view structure
- [ ] Add integration tests for view navigation
- [ ] Update CLAUDE.md and README
- [ ] Update keybindings documentation
- [ ] Performance testing with large workflows
- [ ] Accessibility review (screen readers, color contrast)

---

## Success Criteria

### Must Have (v0.21.0)

- [ ] 5 views only: Studio, Runner, Chat, Scheduler, Settings
- [ ] Studio is default view on launch
- [ ] Live DAG Preview updates on editor changes (horizontal layout)
- [ ] Runner shows horizontal DAG + TaskBox List + Output
- [ ] TaskBox widget used in Runner and Chat
- [ ] Output panel always visible in Runner
- [ ] All tests passing (target: 3,800+ tests)

### Nice to Have (v0.21.x)

- [ ] DAG zoom/pan in Runner
- [ ] TaskBox animations (smooth transitions)
- [ ] Export DAG as image/SVG
- [ ] Scheduler backend (actual cron execution)

---

## Resolved Questions

1. **DAG Layout Algorithm:** ✅ **Horizontal** (like StudioView, left-to-right flow)

2. **TaskBox scroll sync:** ✅ **Independent** (DAG and TaskBox scroll separately)

3. **Output Panel:** ✅ **Always visible** (fixed height at bottom)

4. **Architecture:** ✅ **5 VIEWS** (keep Scheduler as mockup)
   - Studio, Runner, Chat, Scheduler, Settings

---

## References

- Current WorkspaceView: `src/tui/views/workspace.rs` (627 lines)
- Tree Widget: `src/tui/widgets/tree/` (6 modules)
- TaskBox concept: Chat view inline task visualization
- DAG execution: `src/runtime/executor.rs`
