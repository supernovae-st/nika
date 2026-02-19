# Nika TUI Mission Control - Design Document

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a full mission control TUI for Nika workflow execution with real-time observability, NovaNet integration transparency, and interactive debugging.

**Architecture:** 4-panel layout with event-driven state updates, 60 FPS animation system, and modal overlays for inspection/editing. Uses ratatui for rendering and integrates with Nika's existing event system (16 EventKind variants).

**Tech Stack:** Rust, ratatui 0.29, crossterm 0.28, tokio (async event loop)

---

## 1. Visual System

### 1.1 Color Taxonomy (from NovaNet visual-encoding.yaml)

| Category | Name | Hex | Usage |
|----------|------|-----|-------|
| **Realms** | shared | `#3B82F6` | Shared realm nodes |
| | org | `#10B981` | Org realm nodes |
| **Traits** | defined | `#6B7280` | Invariant content |
| | authored | `#8B5CF6` | Locale-specific |
| | imported | `#F59E0B` | External data |
| | generated | `#10B981` | LLM output |
| | retrieved | `#06B6D4` | API fetched |
| **Status** | pending | `#6B7280` | Task waiting |
| | running | `#F59E0B` | Task executing |
| | success | `#22C55E` | Task completed |
| | failed | `#EF4444` | Task errored |
| **MCP Tools** | describe | `#3B82F6` | Entity info |
| | traverse | `#EC4899` | Graph walk |
| | search | `#F59E0B` | Fulltext search |
| | atoms | `#8B5CF6` | Knowledge atoms |
| | generate | `#10B981` | Generation context |

### 1.2 Animation System

| Animation | Cycle | Frames | Usage |
|-----------|-------|--------|-------|
| `steady_glow` | 2000ms | `░▒▓█▓▒░` | Pending tasks |
| `breathing_pulse` | 1500ms | `▓█▓░░░▓█▓` | Running tasks |
| `shimmer_sweep` | 800ms | `░▒▓█████▓▒░` | MCP calls |
| `matrix_pulse` | 500ms | `▓░█░▓░█░▓` | Agent thinking |
| `radar_scan` | 1200ms | `●━━━━━━━●` | Streaming |
| `power_conduit` | 300ms | `═══►═══►` | Data flow |

### 1.3 Typography

- **Headers:** FIGlet-style block letters for "NIKA TUI"
- **Panel titles:** Bold unicode with icons (`◉ ⎔ ⊛ ⊕`)
- **Status indicators:** `◉ ◎ ○` (active/complete/pending)
- **Sparklines:** `▁▂▃▄▅▆▇█` for metrics

### 1.4 Space Theme

| Phase | Icon | Description |
|-------|------|-------------|
| PREFLIGHT | `◦` | DAG validation |
| COUNTDOWN | `⊙` | Loading configs |
| LAUNCH | `⊛` | First task |
| ORBITAL | `◉` | Nominal execution |
| RENDEZVOUS | `◈` | MCP docking |
| MISSION SUCCESS | `✦` | Completed |
| ABORT | `⊗` | Failed |

---

## 2. Panel Architecture

### 2.1 Layout

```
┌─────────────────────────────┬─────────────────────────────┐
│                             │                             │
│  Panel 1: MISSION CONTROL   │  Panel 2: DAG EXECUTION     │
│  (Progress + Timeline)      │  (Graph visualization)      │
│                             │                             │
│  - Workflow header          │  - Task nodes with status   │
│  - Task timeline            │  - Dependency edges         │
│  - Active task details      │  - Data flow annotations    │
│  - Task queue               │  - Tree/Graph/List views    │
│                             │                             │
├─────────────────────────────┼─────────────────────────────┤
│                             │                             │
│  Panel 3: NOVANET STATION   │  Panel 4: AGENT REASONING   │
│  (MCP + Context)            │  (Turns + Streaming)        │
│                             │                             │
│  - MCP call log             │  - Turn history             │
│  - Context assembly         │  - Live streaming           │
│  - Token budget bar         │  - Tool call display        │
│  - Node details             │  - Metrics per turn         │
│                             │                             │
└─────────────────────────────┴─────────────────────────────┘
```

### 2.2 Panel 1: Mission Control

**Components:**
- Mission header with workflow name, phase, elapsed/ETA
- Task timeline with markers at task boundaries
- Active task card (type, provider, turn count, tokens, progress)
- Task queue (scrollable list with status icons)

**Events handled:**
- `WorkflowStarted` → Initialize timeline
- `TaskStarted` → Highlight active task
- `TaskCompleted` → Mark complete, update progress

### 2.3 Panel 2: DAG Execution

**Components:**
- DAG visualization (tree or graph mode)
- Task nodes with status colors and animations
- Dependency edges with data flow labels
- View toggle (Tree/Graph/List)

**Events handled:**
- `TaskScheduled` → Add node with dependencies
- `TaskStarted` → Animate node as running
- `TaskCompleted` → Show success state

### 2.4 Panel 3: NovaNet Station

**Components:**
- MCP call log (scrollable, color-coded by tool)
- Context assembly breakdown (sources, tokens, %)
- Excluded items with reason
- Token budget progress bar

**Events handled:**
- `McpInvoke` → Add call entry with animation
- `McpResponse` → Update entry with result
- `ContextAssembled` → Show breakdown

### 2.5 Panel 4: Agent Reasoning

**Components:**
- Turn history (collapsible cards)
- Live streaming view (real-time text)
- Tool call display within turns
- Turn metrics (tokens, latency)

**Events handled:**
- `AgentStart` → Initialize view
- `AgentTurn` → Stream content, add to history
- `AgentComplete` → Show final state

---

## 3. Keyboard Controls

### 3.1 Navigation

| Key | Action |
|-----|--------|
| `Tab` / `Shift+Tab` | Cycle panels |
| `1-4` | Jump to panel |
| `h/j/k/l` | Vim navigation |
| `↑↓←→` | Arrow navigation |
| `g/G` | Top/Bottom |
| `/` | Search |

### 3.2 Execution Control

| Key | Action |
|-----|--------|
| `Space` | Pause/Resume |
| `Enter` | Step (when paused) |
| `b` | Set breakpoint |
| `c` | Continue to next break |
| `r` | Restart workflow |
| `q` | Quit (with confirm) |

### 3.3 Inspection

| Key | Action |
|-----|--------|
| `i` | Inspect selected item |
| `e` | Edit data (debug mode) |
| `x` | Export to file |
| `v` | View raw JSON |
| `y` | Copy to clipboard |

### 3.4 Display

| Key | Action |
|-----|--------|
| `m` | Metrics overlay |
| `?` / `F1` | Help screen |
| `+/-` | Zoom |
| `Esc` | Close overlay |

---

## 4. Debug Features

### 4.1 Breakpoints

Types:
- `[B]` Before task starts
- `[A]` After task completes
- `[E]` On error only
- `[M]` On MCP call
- `[T]` On agent turn N

### 4.2 Inspect Mode

- View any task's output as JSON
- Syntax highlighting
- Copy/Export options

### 4.3 Edit Mode

- Modify task output while paused
- Shows downstream impact
- Undo support
- Save applies changes to subsequent tasks

### 4.4 Error Handling

- Error overlay with stack trace
- Context at failure point
- Actions: Retry / Skip / Edit & retry / Quit

---

## 5. Event Integration

### 5.1 Event Flow

```
Runtime → EventKind → Channel (mpsc) → TuiState → Render
```

### 5.2 Event → Panel Mapping

| Event | Panels Updated |
|-------|----------------|
| `WorkflowStarted` | 1 (header) |
| `WorkflowCompleted` | 1 (success state) |
| `WorkflowFailed` | All (error overlay) |
| `TaskScheduled` | 2 (add DAG node) |
| `TaskStarted` | 1, 2 (activate) |
| `TaskCompleted` | 1, 2 (complete) |
| `TaskFailed` | All (error) |
| `McpInvoke` | 3 (add call) |
| `McpResponse` | 3 (update call) |
| `ContextAssembled` | 3 (show breakdown) |
| `AgentStart` | 4 (initialize) |
| `AgentTurn` | 4 (stream) |
| `AgentComplete` | 4 (finalize) |

### 5.3 Render Loop

Target: 60 FPS (16.6ms frame budget)

1. Poll runtime events (non-blocking)
2. Poll keyboard input
3. Tick animations
4. Render frame (diff buffer)
5. Sleep to maintain frame rate

---

## 6. File Structure

```
tools/nika/src/tui/
├── mod.rs                    # TUI module entry, feature gate
├── app.rs                    # App struct, main event loop
├── state.rs                  # TuiState: panels, focus, mode
├── theme.rs                  # Colors, styles, NovaNet taxonomy
├── animation.rs              # Animation system, frame timing
│
├── panels/
│   ├── mod.rs                # Panel trait, registry
│   ├── progress.rs           # Panel 1: Mission Control
│   ├── dag.rs                # Panel 2: DAG Execution
│   ├── novanet.rs            # Panel 3: NovaNet Context
│   └── agent.rs              # Panel 4: Agent Reasoning
│
├── widgets/
│   ├── mod.rs                # Widget exports
│   ├── timeline.rs           # Task timeline with markers
│   ├── sparkline.rs          # Metrics sparklines
│   ├── dag_graph.rs          # DAG visualization
│   ├── mcp_log.rs            # MCP call log with animations
│   ├── context_bar.rs        # Context assembly budget bar
│   ├── turn_viewer.rs        # Agent turn history
│   ├── stream_view.rs        # Live streaming text
│   └── big_text.rs           # FIGlet-style headers
│
├── overlays/
│   ├── mod.rs                # Overlay system
│   ├── help.rs               # Help screen (F1)
│   ├── metrics.rs            # Metrics overlay (m)
│   ├── inspect.rs            # Inspect modal (i)
│   ├── edit.rs               # Edit modal (e)
│   ├── error.rs              # Error overlay
│   └── breakpoint.rs         # Breakpoint config (b)
│
├── input/
│   ├── mod.rs                # Input handling
│   ├── keybindings.rs        # Keyboard shortcuts map
│   └── handler.rs            # Event → Action dispatch
│
└── render/
    ├── mod.rs                # Render orchestration
    ├── layout.rs             # Panel layout calculation
    └── frame.rs              # Frame diffing, draw calls
```

---

## 7. Core Types

### 7.1 TuiState

```rust
pub struct TuiState {
    // Execution state
    pub workflow: WorkflowState,
    pub tasks: HashMap<String, TaskState>,
    pub current_task: Option<String>,

    // MCP tracking
    pub mcp_calls: Vec<McpCall>,
    pub context_assembly: ContextAssembly,

    // Agent tracking
    pub agent_turns: Vec<AgentTurn>,
    pub streaming_buffer: String,

    // UI state
    pub focus: PanelId,
    pub mode: TuiMode,
    pub overlay: Option<OverlayKind>,

    // Debug state
    pub breakpoints: HashSet<Breakpoint>,
    pub paused: bool,
    pub step_mode: bool,

    // Metrics
    pub metrics: Metrics,
}
```

### 7.2 TuiMode

```rust
pub enum TuiMode {
    Normal,           // Default navigation
    Streaming,        // Live agent output
    Inspect(String),  // Viewing task output
    Edit(String),     // Modifying task output
    Search,           // Searching
}
```

### 7.3 Theme

```rust
pub struct Theme {
    // Realms
    pub realm_shared: Color,
    pub realm_org: Color,

    // Traits
    pub trait_defined: Color,
    pub trait_authored: Color,
    pub trait_imported: Color,
    pub trait_generated: Color,
    pub trait_retrieved: Color,

    // Status
    pub status_pending: Color,
    pub status_running: Color,
    pub status_success: Color,
    pub status_failed: Color,

    // MCP tools
    pub mcp_describe: Color,
    pub mcp_traverse: Color,
    pub mcp_search: Color,
    pub mcp_atoms: Color,
    pub mcp_generate: Color,
}
```

### 7.4 AnimationSystem

```rust
pub struct AnimationSystem {
    pub frame: u64,
    pub animations: HashMap<AnimationId, Animation>,
}

pub enum AnimationKind {
    SteadyGlow,
    BreathingPulse,
    ShimmerSweep,
    MatrixPulse,
    RadarScan,
    PowerConduit,
}
```

---

## 8. Dependencies

```toml
[dependencies]
ratatui = "0.29"
crossterm = "0.28"
tokio = { version = "1", features = ["sync", "time"] }
unicode-width = "0.2"

[features]
tui = ["ratatui", "crossterm"]
```

---

## 9. Implementation Phases

### Phase 1: Foundation (Core Loop)
- `app.rs`: Event loop with 60 FPS render
- `state.rs`: TuiState with basic fields
- `theme.rs`: NovaNet taxonomy colors
- `layout.rs`: 4-panel layout calculation
- **Verify:** Blank panels render, keyboard quits

### Phase 2: Progress Panel
- `panels/progress.rs`: Mission control header
- `widgets/timeline.rs`: Task timeline
- `widgets/big_text.rs`: FIGlet headers
- Event handling: WorkflowStarted, TaskStarted/Completed
- **Verify:** Timeline updates with mock events

### Phase 3: DAG Panel
- `panels/dag.rs`: DAG visualization
- `widgets/dag_graph.rs`: Node/edge rendering
- Event handling: TaskScheduled, dependencies
- **Verify:** DAG shows task flow with status colors

### Phase 4: NovaNet Panel
- `panels/novanet.rs`: MCP call log + context
- `widgets/mcp_log.rs`: Call entries with colors
- `widgets/context_bar.rs`: Token budget visualization
- Event handling: McpInvoke, McpResponse, ContextAssembled
- **Verify:** MCP calls stream with tool colors

### Phase 5: Agent Panel
- `panels/agent.rs`: Turn history + streaming
- `widgets/turn_viewer.rs`: Turn cards
- `widgets/stream_view.rs`: Live text streaming
- Event handling: AgentStart, AgentTurn, AgentComplete
- **Verify:** Agent turns display with tool calls

### Phase 6: Animations
- `animation.rs`: Animation system
- All 6 animation types from visual-encoding
- **Verify:** Smooth 60 FPS animations on status changes

### Phase 7: Debug Features
- `overlays/inspect.rs`: View task output
- `overlays/edit.rs`: Modify task output
- `overlays/breakpoint.rs`: Set breakpoints
- Pause/Step/Continue execution control
- **Verify:** Can pause, inspect, modify, continue

### Phase 8: Polish
- `overlays/metrics.rs`: Full metrics overlay
- `overlays/help.rs`: Keyboard shortcuts help
- `overlays/error.rs`: Error handling overlay
- `widgets/sparkline.rs`: Metrics sparklines
- **Verify:** Complete mission control experience
