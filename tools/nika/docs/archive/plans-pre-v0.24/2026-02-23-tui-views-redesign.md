# TUI Views Redesign Proposal

**Date:** 2026-02-23
**Target:** v0.9.0
**Status:** PROPOSAL

---

## Executive Summary

Redesign the 3 TUI views with:
- Better naming
- Enhanced visual composition
- "Wow" effects (animations, colors, BigText)
- Consistent widget usage

---

## Current State: 3 Views

```
+===============================================================================+
|  CURRENT TUI VIEWS                                                            |
+===============================================================================+
|                                                                               |
|  [1] HOME VIEW (h)              "Workflow Browser"                            |
|      +-------------------------+-------------------------------------+        |
|      | FILES (40%)             | DAG PREVIEW (60%)                   |        |
|      | .nika.yaml list         | Task dependency graph               |        |
|      +-------------------------+-------------------------------------+        |
|      | HISTORY BAR                                                   |        |
|      +---------------------------------------------------------------+        |
|                                                                               |
|  [2] CHAT VIEW (c)              "AI Agent Interface"                          |
|      +---------------------------------------------------------------+        |
|      | SESSION CONTEXT: tokens | cost | MCP status                   |        |
|      +-----------------------------------------+---------------------+        |
|      | CONVERSATION                            | MISSION CONTROL     |        |
|      | Messages + inline MCP/Infer boxes       | Activity + Context  |        |
|      +-----------------------------------------+---------------------+        |
|      | > INPUT FIELD                                    [Cmd+K] palette |     |
|      +---------------------------------------------------------------+        |
|                                                                               |
|  [3] STUDIO VIEW (s)            "YAML Editor"                                 |
|      +-----------------------------------------+---------------------+        |
|      | EDITOR                                  | STRUCTURE           |        |
|      | YAML with syntax highlighting           | Task DAG mini-view  |        |
|      +-----------------------------------------+---------------------+        |
|      | VALIDATION STATUS: Valid YAML | Schema OK | Warnings          |        |
|      +---------------------------------------------------------------+        |
|                                                                               |
+===============================================================================+
```

---

## Proposed Redesign

### View Renaming

| Current | Proposed | Key | Rationale |
|---------|----------|-----|-----------|
| Home | **Launch Pad** | `1` | More dynamic, implies action |
| Chat | **Agent** | `2` | Clearer purpose, shorter |
| Studio | **Editor** | `3` | More universal, familiar |

### Navigation Keys

| Key | Action |
|-----|--------|
| `1` or `l` | Launch Pad |
| `2` or `a` | Agent |
| `3` or `e` | Editor |
| `Tab` | Cycle views |

---

## View 1: LAUNCH PAD (was Home)

### Current Problems
- No branding/identity
- Files list is plain
- Welcome screen is text-only
- No visual hierarchy

### Proposed Design

```
+===============================================================================+
|                                                                               |
|     N   I   K   A                                                             |
|     |   |   |  /|           AI Workflow Engine v0.8.0                         |
|     |   |   |/  |           ---------------------------                       |
|                                                                               |
|  +-- WORKFLOWS -----------------+-- PREVIEW --------------------------------+ |
|  |                              |                                           | |
|  |  * hello-world.nika.yaml     |    +----------+                           | |
|  |  * generate-page.nika.yaml   |    | infer:   |-------+                   | |
|  |  * multi-mcp-agent.nika.yaml |    | prompt   |       v                   | |
|  |  > examples/                 |    +----------+   +----------+            | |
|  |    * basic.nika.yaml         |                   | invoke:  |            | |
|  |    * complex.nika.yaml       |                   | mcp      |            | |
|  |                              |                   +----------+            | |
|  |  ~~~~~~~~~ Recent activity   |                                           | |
|  |                              |                                           | |
|  +------------------------------+-------------------------------------------+ |
|                                                                               |
|  +-- RECENT RUNS -------------------------------------------------------+    |
|  |  [OK] hello-world (2m ago)  |  [OK] generate-page (15m)  |  [X] test (1h) |
|  +-------------------------------------------------------------------+       |
|                                                                               |
|  [Enter] Run  [e] Edit  [/] Search  [?] Help                 MCP: @ novanet  |
+===============================================================================+
```

### New Features

| Feature | Widget | Effect |
|---------|--------|--------|
| **BigText Logo** | `BigText::new("NIKA")` | Brand identity |
| **File Icons** | `*` workflow, `>` folder | Visual type hints |
| **Activity Sparkline** | `AnimatedLatencySparkline` | Live metrics pulse |
| **Run Status Badges** | `[OK]` / `[X]` / `[~]` | Color-coded status |
| **Preview DAG** | `DagAscii` with animations | Live preview |

### Color Palette (Launch Pad)

```rust
// Solarized-inspired with brand colors
const BRAND_CYAN: Color = Color::Rgb(42, 161, 152);     // #2aa198 - NIKA brand
const FILE_YELLOW: Color = Color::Rgb(181, 137, 0);     // #b58900 - workflows
const FOLDER_BLUE: Color = Color::Rgb(38, 139, 210);    // #268bd2 - directories
const SUCCESS_GREEN: Color = Color::Rgb(133, 153, 0);   // #859900 - completed
const ERROR_RED: Color = Color::Rgb(220, 50, 47);       // #dc322f - failed
const RUNNING_ORANGE: Color = Color::Rgb(203, 75, 22);  // #cb4b16 - in progress
```

---

## View 2: AGENT (was Chat)

### Current Problems
- "Chat" is generic, doesn't convey AI agent capability
- Mission Control panel underutilized
- No visual feedback during thinking

### Proposed Design

```
+===============================================================================+
|  AGENT                             claude-sonnet-4  |  @ novanet |  $0.42    |
+===============================================================================+
|                                                                               |
|  +-- CONVERSATION ----------------------+-- MISSION CONTROL ----------------+ |
|  |                                      |                                   | |
|  |  +-- YOU ---------------------------+|  SESSION                          | |
|  |  | Generate a landing page for QR   ||  +- Turns: 3                      | |
|  |  +----------------------------------+|  +- Tokens: 12.4k / 200k          | |
|  |                                      |  +- Time: 2m 34s                  | |
|  |  +-- NIKA --------------------------+|                                   | |
|  |  | I'll help you create that page.  ||  ACTIVITY                         | |
|  |  |                                  ||  +- * infer: generating...        | |
|  |  |  ,-- MCP: novanet_describe [OK] -||  +- @ invoke: queued              | |
|  |  |  | entity: "qr-code"             ||  +- > exec: waiting               | |
|  |  |  | -> display_name: "QR Code"    ||                                   | |
|  |  |  `-------------------------------'|  CONTEXT                          | |
|  |  |                                  ||  +- @qr-code (entity)             | |
|  |  |  ,-- THINKING -------------------||  +- @fr-FR (locale)               | |
|  |  |  | Let me analyze the entity...  ||  +- 3 files attached              | |
|  |  |  `-----------------------------v-'|                                   | |
|  |  |                                  ||  ~~~~~~~~~ latency (pulse)        | |
|  |  +----------------------------------+|                                   | |
|  |                                      |                                   | |
|  +--------------------------------------+-----------------------------------+ |
|                                                                               |
|  +-- INPUT -------------------------------------------------------------+    |
|  | > infer: _                                             [Cmd+K] [Cmd+Enter]|
|  +----------------------------------------------------------------------+    |
+===============================================================================+
```

### New Features

| Feature | Widget | Effect |
|---------|--------|--------|
| **Thinking Block** | Collapsible amber box | Shows Claude's reasoning |
| **MCP Call Inline** | `McpCallBox` with status | Real-time tool calls |
| **Activity Pulse** | `AnimatedLatencySparkline` | Live latency visualization |
| **Verb Indicator** | `*` `>` `~` `@` icons | Color-coded by verb type |
| **Context Pills** | `@entity` mentions | Clickable context refs |

### Color Palette (Agent)

```rust
// Conversation colors
const USER_BUBBLE: Color = Color::Rgb(38, 139, 210);    // #268bd2 - blue
const NIKA_BUBBLE: Color = Color::Rgb(42, 161, 152);    // #2aa198 - cyan
const THINKING_BG: Color = Color::Rgb(181, 137, 0);     // #b58900 - amber
const MCP_BOX: Color = Color::Rgb(133, 153, 0);         // #859900 - green
const INFER_BOX: Color = Color::Rgb(108, 113, 196);     // #6c71c4 - violet
```

---

## View 3: EDITOR (was Studio)

### Current Problems
- "Studio" sounds heavyweight
- Validation status bar is minimal
- No visual feedback on errors

### Proposed Design

```
+===============================================================================+
|  EDITOR                           example.nika.yaml  |  Modified  |  Ln 12   |
+===============================================================================+
|                                                                               |
|  +-- YAML --------------------------------+-- DAG --------------------------+ |
|  |                                        |                                 | |
|  |   1| schema: nika/workflow@0.5         |   +----------+                  | |
|  |   2| workflow: generate-page           |   | * infer  |                  | |
|  |   3|                                   |   | headline |                  | |
|  |   4| tasks:                            |   +----+-----+                  | |
|  |   5|   - id: headline                  |        |                        | |
|  |   6|     infer: "Generate headline"    |        v                        | |
|  |   7|                                   |   +----------+                  | |
|  |   8|   - id: content                   |   | * infer  |                  | |
|  |   9|     infer: "Generate body"        |   | content  |                  | |
|  |  10|     needs: [headline]             |   +----------+                  | |
|  |  11|                                   |                                 | |
|  |  12|   - id: format_                   |   Tasks: 3                      | |
|  |  13|     exec: "prettier"              |   Flows: 2                      | |
|  |                                        |   Verbs: infer(2), exec(1)      | |
|  |  --------------------------------      |                                 | |
|  |  ~~~~~~~~~ Edit activity               |  ~~~ Complexity: LOW            | |
|  |                                        |                                 | |
|  +----------------------------------------+---------------------------------+ |
|                                                                               |
|  +-- DIAGNOSTICS -------------------------------------------------------+    |
|  |  [OK] YAML Valid  |  [OK] Schema OK  |  [!] 1 warning: unused "debug"    |
|  +----------------------------------------------------------------------+    |
+===============================================================================+
```

### New Features

| Feature | Widget | Effect |
|---------|--------|--------|
| **Line Numbers** | Gutter with syntax colors | Easy navigation |
| **Verb Highlighting** | `infer:` cyan, `exec:` yellow | Instant verb recognition |
| **Live DAG** | Updates as you type | Real-time structure feedback |
| **Diagnostics Bar** | `[OK]` `[!]` `[X]` with inline | Click to jump to error |
| **Edit Sparkline** | `AnimatedLatencySparkline` | Edit activity visualization |

### Color Palette (Editor)

```rust
// Syntax highlighting (Solarized)
const KEYWORD: Color = Color::Rgb(133, 153, 0);      // #859900 - green (schema, workflow)
const STRING: Color = Color::Rgb(42, 161, 152);      // #2aa198 - cyan (strings)
const VERB_INFER: Color = Color::Rgb(108, 113, 196); // #6c71c4 - violet
const VERB_EXEC: Color = Color::Rgb(181, 137, 0);    // #b58900 - yellow
const VERB_FETCH: Color = Color::Rgb(38, 139, 210);  // #268bd2 - blue
const VERB_INVOKE: Color = Color::Rgb(133, 153, 0);  // #859900 - green
const VERB_AGENT: Color = Color::Rgb(211, 54, 130);  // #d33682 - magenta
const ERROR_LINE: Color = Color::Rgb(220, 50, 47);   // #dc322f - red bg
const WARNING_LINE: Color = Color::Rgb(181, 137, 0); // #b58900 - yellow bg
```

---

## Widget Inventory

### Available Widgets (32)

| Widget | View(s) | Purpose |
|--------|---------|---------|
| `BigText` | Launch Pad | ASCII art logo |
| `AnimatedLatencySparkline` | All | Live metrics with pulse |
| `DagAscii` | Launch Pad, Editor | Task dependency graph |
| `NodeBox` | DAG views | Individual task boxes |
| `McpCallBox` | Agent | Inline MCP call display |
| `InferStreamBox` | Agent | Streaming inference |
| `ThinkingBlock` | Agent | Claude reasoning |
| `MessageBubble` | Agent | Chat bubbles |
| `ProStatusBar` | Agent | Session metrics bar |
| `MissionControlPanel` | Agent | Activity + context |
| `CommandPalette` | All | Cmd+K command search |
| `HelpOverlay` | All | ? help screen |
| `ActivityStack` | Agent | Hot/warm/queued tasks |
| `VerbIndicator` | Agent, Editor | Verb type icons |
| `ProviderSelector` | Agent | Model picker popup |
| `StatusMessage` | All | Toast notifications |
| `Header` | All | View title bar |
| `StatusBar` | All | Bottom status |

### Widget Effects

| Effect | Implementation | Views |
|--------|----------------|-------|
| **Pulse** | `SparklineAnimation::Pulse` | Activity indicators |
| **Flow** | `SparklineAnimation::Flow` | Data streaming |
| **Wave** | `SparklineAnimation::Wave` | Background activity |
| **Glow** | `Modifier::BOLD` on highlight | Focus indicator |
| **Fade** | Gray -> Color transition | Completed items |

---

## Color System

### Solarized Base

```
BASE03  #002b36  dark background
BASE02  #073642  background highlight
BASE01  #586e75  comments, secondary
BASE00  #657b83  body text
BASE0   #839496  body text (light)
BASE1   #93a1a1  optional emphasis
BASE2   #eee8d5  background (light)
BASE3   #fdf6e3  background highlight
```

### Accent Colors

```
YELLOW  #b58900  warnings, exec:
ORANGE  #cb4b16  running, alerts
RED     #dc322f  errors, failures
MAGENTA #d33682  agent:, special
VIOLET  #6c71c4  infer:, prompts
BLUE    #268bd2  fetch:, links
CYAN    #2aa198  NIKA brand, strings
GREEN   #859900  invoke:, success
```

### Semantic Mapping

| Semantic | Color | Usage |
|----------|-------|-------|
| `brand` | Cyan #2aa198 | Logo, highlights |
| `success` | Green #859900 | Completed, valid |
| `warning` | Yellow #b58900 | Warnings, caution |
| `error` | Red #dc322f | Errors, failures |
| `running` | Orange #cb4b16 | In progress |
| `muted` | Base01 #586e75 | Secondary text |

---

## Implementation Plan

### Phase 1: BigText in Launch Pad (Now)
- [x] Create `BigText` widget
- [ ] Add to HomeView welcome screen
- [ ] Style with brand cyan color

### Phase 2: View Renaming (v0.9)
- [ ] Rename `HomeView` -> `LaunchPadView`
- [ ] Rename `ChatView` -> `AgentView`
- [ ] Rename `StudioView` -> `EditorView`
- [ ] Update keybindings (`1`/`2`/`3`)

### Phase 3: Visual Enhancements (v0.9)
- [ ] Add sparklines to all views
- [ ] Implement verb-colored syntax highlighting
- [ ] Add activity indicators
- [ ] Polish color palette

### Phase 4: Animation Integration (v0.9)
- [ ] Connect `AnimatedLatencySparkline` to real metrics
- [ ] Add thinking block animations
- [ ] Implement status transitions

---

## Success Metrics

- [ ] Brand identity visible on launch (BigText "NIKA")
- [ ] All verbs have distinct visual colors
- [ ] Animation smoothness at 60 FPS during activity
- [ ] Consistent Solarized palette across views
- [ ] "Wow" reaction on first launch

---

## Notes

- Keep backward compat: old keybindings (`h`, `c`, `s`) still work
- Performance budget: less than 16ms frame time
- Accessibility: All colors meet WCAG AA contrast
- Theme support: Light/Dark/Solarized presets
