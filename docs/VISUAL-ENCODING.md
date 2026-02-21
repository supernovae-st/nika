# Nika Visual Encoding System v0.7.0

**Date:** 2026-02-21
**Status:** Design Complete
**Pattern:** Follows NovaNet 3-axis visual encoding (ADR-005, ADR-013)

---

## Overview

Nika's visual encoding maps **4 semantic dimensions** through distinct visual channels:

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  NIKA VISUAL ENCODING AXES                                                      │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Axis 1: VERB (What action?)        → Fill Color + Icon                        │
│          ⚡ infer, 📟 exec, 🛰️ fetch, 🔌 invoke, 🐔 agent                        │
│                                                                                 │
│  Axis 2: STATUS (What state?)       → Border Style + Intensity                 │
│          ○ pending, ◐ running, ● success, ⊗ failed, ◎ paused                   │
│                                                                                 │
│  Axis 3: CONTEXT (What modifier?)   → Badge + Border Thickness                 │
│          🔄 for_each, 🔀 decompose, 💾 output, ↳ spawn                          │
│                                                                                 │
│  Axis 4: PROVIDER (Who executes?)   → Secondary Icon (agent/infer only)        │
│          🧠 Claude, 🤖 OpenAI, 🌬️ Mistral, 🦙 Ollama, ⚡ Groq, 🔍 DeepSeek       │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## 1. Verb Taxonomy (Axis 1)

The 5 semantic verbs form the **primary visual identity** of each task.

### Color Palette (Tailwind-based)

| Verb | Icon | Color Name | RGB | Hex | Muted | Glow |
|------|------|------------|-----|-----|-------|------|
| **infer:** | ⚡ | Violet-500 | (139, 92, 246) | #8B5CF6 | #6140AB | #A78BFA |
| **exec:** | 📟 | Amber-500 | (245, 158, 11) | #F59E0B | #AB6E08 | #FBB324 |
| **fetch:** | 🛰️ | Cyan-500 | (6, 182, 212) | #06B6D4 | #047F94 | #22D3EE |
| **invoke:** | 🔌 | Emerald-500 | (16, 185, 129) | #10B981 | #0B815A | #34D399 |
| **agent:** | 🐔 | Rose-500 | (244, 63, 94) | #F43F5E | #AA2C42 | #FB7185 |

### ASCII Fallbacks (16-color terminals)

| Verb | Emoji | ASCII | ANSI Color |
|------|-------|-------|------------|
| infer | ⚡ | [I] | Magenta |
| exec | 📟 | [X] | Yellow |
| fetch | 🛰️ | [F] | Cyan |
| invoke | 🔌 | [V] | Green |
| agent | 🐔 | [A] | Red |

### Visual Encoding

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  VERB NODES IN DAG                                                              │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   ┌────────────┐    ┌────────────┐    ┌────────────┐    ┌────────────┐         │
│   │ ⚡ INFER   │───▶│ 📟 EXEC    │───▶│ 🔌 INVOKE  │───▶│ 🐔 AGENT   │         │
│   │ task-1     │    │ task-2     │    │ task-3     │    │ task-4     │         │
│   │ [VIOLET]   │    │ [AMBER]    │    │ [EMERALD]  │    │ [ROSE]     │         │
│   └────────────┘    └────────────┘    └────────────┘    └────────────┘         │
│                                                                                 │
│   Border color = Status (see Axis 2)                                           │
│   Fill color = Verb (primary visual)                                           │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Status Taxonomy (Axis 2)

Task execution status encoded via **border style and color**.

### Status Indicators

| Status | Icon | Border Style | Border Color | RGB | Description |
|--------|------|--------------|--------------|-----|-------------|
| **pending** | ○ | Dotted | Gray-500 | (107, 114, 128) | Waiting for deps |
| **scheduled** | ◆ | Dashed | Gray-400 | (156, 163, 175) | Deps resolved |
| **running** | ◐ | Solid + Glow | Amber-500 | (245, 158, 11) | In progress |
| **success** | ● | Solid | Green-500 | (34, 197, 94) | Completed OK |
| **failed** | ⊗ | Double | Red-500 | (239, 68, 68) | Error occurred |
| **paused** | ◎ | Dashed + Glow | Cyan-500 | (6, 182, 212) | User paused |

### Border Style CSS Equivalent

```
pending:   border: 2px dotted #6B7280
scheduled: border: 2px dashed #9CA3AF
running:   border: 2px solid #F59E0B; box-shadow: 0 0 8px #F59E0B
success:   border: 2px solid #22C55E
failed:    border: 4px double #EF4444
paused:    border: 2px dashed #06B6D4; box-shadow: 0 0 4px #06B6D4
```

### Visual Encoding

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  STATUS VISUALIZATION                                                           │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   ┌┄┄┄┄┄┄┄┄┄┄┄┄┐    ┌╌╌╌╌╌╌╌╌╌╌╌╌┐    ┏━━━━━━━━━━━━┓    ╔════════════╗         │
│   ┆ ○ PENDING  ┆    ╎ ◆ SCHEDULED╎    ┃ ◐ RUNNING  ┃    ║ ● SUCCESS  ║         │
│   ┆   task-1   ┆    ╎   task-2   ╎    ┃   task-3   ┃    ║   task-4   ║         │
│   ┆ (gray)     ┆    ╎ (gray-lt)  ╎    ┃ (amber+glow)    ║ (green)    ║         │
│   └┄┄┄┄┄┄┄┄┄┄┄┄┘    └╌╌╌╌╌╌╌╌╌╌╌╌┘    ┗━━━━━━━━━━━━┛    ╚════════════╝         │
│                                                                                 │
│   ╔╦════════════╦╗    ┏┅┅┅┅┅┅┅┅┅┅┅┅┓                                           │
│   ║║ ⊗ FAILED   ║║    ┇ ◎ PAUSED   ┇                                           │
│   ║║   task-5   ║║    ┇   task-6   ┇                                           │
│   ║║ (red)      ║║    ┇ (cyan+glow)┇                                           │
│   ╚╩════════════╩╝    ┗┅┅┅┅┅┅┅┅┅┅┅┅┛                                           │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Context Modifiers (Axis 3)

Task modifiers encoded via **badges and border thickness**.

### Modifier Badges

| Modifier | Badge | Border | Description |
|----------|-------|--------|-------------|
| **for_each** | 🔄 | Thick (3px) | Parallel iteration |
| **decompose** | 🔀 | Pattern (dots) | MCP-driven expansion |
| **output** | 💾 | Normal | Has output policy |
| **use** | 📥 | Normal | Has input bindings |
| **lazy** | ⏳ | Dashed inner | Lazy binding resolution |
| **spawn** | ↳ | Double inner | Spawns nested agent |

### Parallelism Indicators

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  PARALLELISM VISUALIZATION                                                      │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   SINGLE TASK           FOR_EACH (5x)             DECOMPOSE (N items)          │
│   ┌────────────┐        ┏━━━━━━━━━━━━┓            ┌┄┄┄┄┄┄┄┄┄┄┄┄┐               │
│   │ ⚡ task-1   │        ┃ 🔄 task-2  ┃5           ┆ 🔀 task-3  ┆N              │
│   │            │        ┃ for_each   ┃            ┆ decompose  ┆               │
│   └────────────┘        ┗━━━━━━━━━━━━┛            └┄┄┄┄┄┄┄┄┄┄┄┄┘               │
│   1px border            3px thick border          Dotted pattern               │
│                         + count badge             + count badge                │
│                                                                                 │
│   NESTED AGENT (spawn_agent)                                                   │
│   ┌────────────────────────┐                                                   │
│   │ 🐔 parent-agent        │                                                   │
│   │  ↳ 🐤 child-1          │ depth=1                                           │
│   │  ↳ 🐤 child-2          │ depth=1                                           │
│   │    ↳ 🐤 grandchild     │ depth=2                                           │
│   └────────────────────────┘                                                   │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Provider Taxonomy (Axis 4)

LLM provider identity for `infer:` and `agent:` tasks.

### Provider Icons

| Provider | Icon | Color | Env Var | Default Model |
|----------|------|-------|---------|---------------|
| **Claude** | 🧠 | Orange | ANTHROPIC_API_KEY | claude-sonnet-4 |
| **OpenAI** | 🤖 | Green | OPENAI_API_KEY | gpt-4o |
| **Mistral** | 🌬️ | Blue | MISTRAL_API_KEY | mistral-large |
| **Ollama** | 🦙 | Brown | OLLAMA_API_BASE_URL | llama3.2 |
| **Groq** | ⚡ | Purple | GROQ_API_KEY | llama-3.3-70b |
| **DeepSeek** | 🔍 | Teal | DEEPSEEK_API_KEY | deepseek-chat |
| **Mock** | 🧪 | Gray | (none) | mock-model |

### Provider in Task Box

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  PROVIDER VISUALIZATION                                                         │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   ┌──────────────────────┐    ┌──────────────────────┐                         │
│   │ ⚡ infer: task-1      │    │ 🐔 agent: task-2      │                         │
│   │ 🧠 claude-sonnet-4    │    │ 🤖 gpt-4o             │                         │
│   │ prompt: "Generate..." │    │ tools: [novanet]     │                         │
│   │ ──────────────────── │    │ ──────────────────── │                         │
│   │ 150→45 tk | 0.8s     │    │ T1→T2→T3 | 2.3s      │                         │
│   └──────────────────────┘    └──────────────────────┘                         │
│                                                                                 │
│   Small provider icon in secondary line                                        │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## 5. Event Taxonomy (22 Variants)

Events grouped into 6 categories with distinct visual treatment.

### Event Categories

| Category | Color | Icon Prefix | Events |
|----------|-------|-------------|--------|
| **Workflow** | Blue | ◆ | Started, Completed, Failed, Aborted, Paused, Resumed |
| **Task** | Amber | ► | Scheduled, Started, Completed, Failed |
| **Provider** | Violet | ⊛ | Called, Responded, TemplateResolved |
| **Context** | Cyan | ◈ | ContextAssembled |
| **MCP** | Emerald | 🔌 | Invoke, Response, Connected, Error |
| **Agent** | Rose | 🐔 | Start, Turn, Complete, Spawned |

### Event Timeline Visualization

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  EVENT TIMELINE                                                                 │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  TIME   EVENT                                                                   │
│  ─────  ──────────────────────────────────────────────────────────              │
│  0.0s   ◆ WorkflowStarted (tasks: 4, gen: abc123)                [BLUE]        │
│  0.1s   ► TaskScheduled (task-1, deps: [])                       [AMBER]       │
│  0.1s   ► TaskStarted (task-1, verb: infer)                      [AMBER]       │
│  0.2s   ⊛ ProviderCalled (claude, prompt: 150 chars)             [VIOLET]      │
│  0.8s   ⊛ ProviderResponded (150→45 tk, $0.003)                  [VIOLET]      │
│  0.8s   ► TaskCompleted (task-1, 0.7s)                           [AMBER]       │
│  0.9s   ► TaskStarted (task-2, verb: invoke)                     [AMBER]       │
│  0.9s   🔌 McpInvoke (novanet_describe, entity: qr-code)         [EMERALD]     │
│  1.1s   🔌 McpResponse (call-1, 0.2s)                            [EMERALD]     │
│  1.1s   ► TaskCompleted (task-2, 0.2s)                           [AMBER]       │
│  1.2s   🐔 AgentStart (task-3, max_turns: 5)                     [ROSE]        │
│  1.5s   🐔 AgentTurn (T1, thinking: 342 tk)                      [ROSE]        │
│  2.0s   🐔 AgentTurn (T2, tool_use: novanet_traverse)            [ROSE]        │
│  2.5s   🐔 AgentComplete (3 turns, stop: end_turn)               [ROSE]        │
│  2.5s   ◆ WorkflowCompleted (2.5s total)                         [BLUE]        │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## 6. MCP Tool Colors

NovaNet MCP tools with semantic colors.

| Tool | Color | RGB | Purpose |
|------|-------|-----|---------|
| **novanet_describe** | Blue | (59, 130, 246) | Entity information |
| **novanet_traverse** | Pink | (236, 72, 153) | Graph navigation |
| **novanet_search** | Amber | (245, 158, 11) | Query operations |
| **novanet_atoms** | Violet | (139, 92, 246) | Knowledge atoms |
| **novanet_generate** | Emerald | (16, 185, 129) | Content generation |
| **novanet_assemble** | Cyan | (6, 182, 212) | Context assembly |
| **novanet_query** | Gray | (107, 114, 128) | Raw queries |
| **novanet_introspect** | Rose | (244, 63, 94) | Schema introspection |

---

## 7. Spinner Animations

Unified spinner system for consistent visual rhythm.

### Spinner Styles

| Style | Frames | Use Case | Speed |
|-------|--------|----------|-------|
| **Braille** | ⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ | General loading | 60ms/frame |
| **Orbital** | ◐◓◑◒ | Compact spaces | 100ms/frame |
| **Dots** | ⣾⣽⣻⢿⡿⣟⣯⣷ | MCP/Infer boxes | 80ms/frame |
| **Mission** | Per-phase emoji | Progress panel | 150ms/frame |

### Mission Phase Spinners

| Phase | Frames | Purpose |
|-------|--------|---------|
| Countdown | 3️⃣ 2️⃣ 1️⃣ 🔥 | Preflight sequence |
| Launch | 🚀 🔥 💨 ✨ | First task starting |
| Orbital | 🛰️ 📡 🌐 💫 | Nominal execution |
| Rendezvous | 🔌 ⚡ ✨ 💫 | MCP connection |
| Agent Active | 🐔 🔥 ✨ 💫 | Agent loop running |

---

## 8. Complete Component Hierarchy

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  NIKA COMPONENT TAXONOMY                                                        │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  WORKFLOW                                                                       │
│  ├── Task                                                                       │
│  │   ├── Verb: infer | exec | fetch | invoke | agent                          │
│  │   ├── Status: pending | scheduled | running | success | failed | paused     │
│  │   ├── Modifiers: for_each | decompose | output | use | lazy                 │
│  │   └── (agent only) spawn_agent → child agents                               │
│  │                                                                              │
│  ├── Flow (DAG edges)                                                          │
│  │   ├── source → target (single)                                              │
│  │   └── [sources] → [targets] (fan-in/fan-out)                                │
│  │                                                                              │
│  └── MCP Config                                                                 │
│      └── servers: { name → McpConfigInline }                                   │
│                                                                                 │
│  RUNTIME                                                                        │
│  ├── DataStore (task results)                                                  │
│  ├── FlowGraph (DAG validation)                                                │
│  ├── TaskExecutor (verb dispatch)                                              │
│  ├── RigAgentLoop (multi-turn)                                                 │
│  │   └── SpawnAgentTool (nesting)                                              │
│  └── RigProvider (6 LLM backends)                                              │
│                                                                                 │
│  EVENTS (22 variants)                                                          │
│  ├── Workflow (6): Started, Completed, Failed, Aborted, Paused, Resumed        │
│  ├── Task (4): Scheduled, Started, Completed, Failed                           │
│  ├── Provider (3): Called, Responded, TemplateResolved                         │
│  ├── Context (1): ContextAssembled                                             │
│  ├── MCP (4): Invoke, Response, Connected, Error                               │
│  └── Agent (4): Start, Turn, Complete, Spawned                                 │
│                                                                                 │
│  BINDINGS                                                                       │
│  ├── UseEntry: alias → path (eager)                                            │
│  ├── LazyBinding: alias → path (deferred, v0.5)                                │
│  └── Template: {{use.alias}} resolution                                        │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## 9. Color Mode Degradation

Support for terminals with limited color.

### Detection Priority

1. `COLORTERM=truecolor|24bit` → **TrueColor (24-bit)**
2. `TERM` contains `256color` → **Color256 (8-bit)**
3. Default → **Color16 (ANSI)**

### Conversion Algorithms

```
RGB → 256-color:
  r6 = (r * 6) / 256  // 0-5
  g6 = (g * 6) / 256  // 0-5
  b6 = (b * 6) / 256  // 0-5
  index = 16 + (36 * r6) + (6 * g6) + b6

RGB → 16-color:
  luminance = 0.299*r + 0.587*g + 0.114*b
  if luminance > 200: WHITE
  else if luminance < 50: BLACK
  else: map to nearest ANSI (red, green, blue, cyan, magenta, yellow)
```

---

## 10. Accessibility

### Colorblind Safety

- **Never use color alone** — always pair with icon or shape
- **Border style** encodes status (solid/dashed/dotted)
- **Icon** encodes verb (distinct shapes)
- **Text label** always present for screen readers

### Contrast Requirements (WCAG AA)

| Combination | Ratio | Status |
|-------------|-------|--------|
| Text on Background | 4.5:1 | Required |
| Large Text | 3:1 | Required |
| UI Elements | 3:1 | Required |

### ASCII Mode

Full ASCII fallback for terminals without emoji support:

```
VERB ASCII:     [I] [X] [F] [V] [A]
STATUS ASCII:   [ ] [/] [*] [!] [-]
SPINNER ASCII:  - \ | /
```

---

## 11. Implementation Files

| File | Purpose | Status |
|------|---------|--------|
| `src/tui/theme.rs` | Master color definitions | ✅ Complete |
| `src/tui/unicode.rs` | Width calculations | ✅ Complete |
| `src/ast/task.rs` | Verb icons | ✅ Complete |
| `src/tui/widgets/*.rs` | Widget-specific | ⚠️ Needs consolidation |

### Recommended Consolidation

1. Move all spinner definitions to `theme.rs`
2. Add `SpinnerStyle` enum
3. Add `IconSet` struct for centralized icons
4. Pass `Theme` to all widget render methods

---

## Summary

Nika's visual encoding system provides:

- **4 semantic axes** (verb, status, context, provider)
- **5 verb types** with distinct colors and icons
- **6 status states** with border styles
- **6 context modifiers** with badges
- **7 providers** with icons
- **22 event types** across 6 categories
- **4 spinner styles** for animations
- **3 color modes** with graceful degradation
- **Full accessibility** with ASCII fallbacks

This system mirrors NovaNet's approach while being optimized for workflow execution visualization.
