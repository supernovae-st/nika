# TUI Refactoring Master Plan

> **Version:** 1.0.0
> **Date:** 2026-03-07
> **Status:** In Progress
> **Estimated Effort:** ~50 hours

---

## Executive Summary

Comprehensive TUI refactoring based on 10-agent audit findings. Four phases executed sequentially with Ralph Wiggum checkpoints between each phase.

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  MASTER PLAN OVERVIEW                                                         ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  Phase A: Split Massive Files (3 files, ~20h)                                 ║
║  ├── A1: chat.rs (9,327 lines → 9 submodules)                                 ║
║  ├── A2: state.rs (6,014 lines → 8 submodules)                                ║
║  └── A3: app.rs (5,028 lines → 6 submodules)                                  ║
║                                                                               ║
║  Phase B: Consolidate Colors (80+ constants, ~10h)                            ║
║  ├── B1: Migrate widgets to tokens/colors.rs                                  ║
║  ├── B2: Remove legacy Theme usages                                           ║
║  └── B3: Delete dead color constants                                          ║
║                                                                               ║
║  Phase C: Fix Keybindings (17 conflicts, ~5h)                                 ║
║  ├── C1: Standardize view switching (Ctrl+1-8)                                ║
║  ├── C2: Fix 'c', 't', 'q' conflicts                                          ║
║  └── C3: Document keybinding matrix                                           ║
║                                                                               ║
║  Phase D: Widget Unification (~15h)                                           ║
║  ├── D1: Create GenericBox trait                                              ║
║  ├── D2: Refactor 6 box variants                                              ║
║  └── D3: Unify state management                                               ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Checkpoint Strategy (Ralph Wiggum)

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  RALPH WIGGUM CHECKPOINTS                                                       │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  CP-0: Pre-flight (before any changes)                                          │
│  ├── cargo check ✓                                                              │
│  ├── cargo test ✓                                                               │
│  ├── cargo clippy -- -D warnings ✓                                              │
│  └── git status clean ✓                                                         │
│                                                                                 │
│  CP-A: After Phase A (file splits)                                              │
│  ├── All tests pass                                                             │
│  ├── No new warnings                                                            │
│  ├── Module structure correct                                                   │
│  └── code-reviewer agent audit                                                  │
│                                                                                 │
│  CP-B: After Phase B (color consolidation)                                      │
│  ├── Zero hardcoded colors outside tokens/                                      │
│  ├── Theme usage reduced to <10                                                 │
│  └── Visual regression check                                                    │
│                                                                                 │
│  CP-C: After Phase C (keybindings)                                              │
│  ├── Zero keybinding conflicts                                                  │
│  ├── Keybinding matrix documented                                               │
│  └── Manual UX verification                                                     │
│                                                                                 │
│  CP-D: After Phase D (widget unification)                                       │
│  ├── GenericBox trait implemented                                               │
│  ├── All box widgets unified                                                    │
│  └── Final codebase-audit                                                       │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Phase A: Split Massive Files

### A1: Split views/chat.rs (9,327 lines)

**Target Structure:**
```
src/tui/views/chat/
├── mod.rs           # ChatView struct, Widget impl, public API
├── state.rs         # ChatViewState, focus management
├── input.rs         # InputHandler, cursor, text editing
├── messages.rs      # MessageList rendering, scroll
├── history.rs       # ConversationHistory, persistence
├── streaming.rs     # StreamingDisplay, token updates
├── commands.rs      # SlashCommands, command parsing
├── agent.rs         # AgentTurnDisplay, tool calls
└── layout.rs        # LayoutCalculator, responsive sizing
```

**Tasks:**
| ID | Task | Est | Status |
|----|------|-----|--------|
| A1.1 | Create chat/ directory and mod.rs | 15m | ⏳ |
| A1.2 | Extract ChatViewState to state.rs | 1h | ⏳ |
| A1.3 | Extract input handling to input.rs | 1h | ⏳ |
| A1.4 | Extract message rendering to messages.rs | 1.5h | ⏳ |
| A1.5 | Extract history management to history.rs | 1h | ⏳ |
| A1.6 | Extract streaming display to streaming.rs | 1.5h | ⏳ |
| A1.7 | Extract slash commands to commands.rs | 1h | ⏳ |
| A1.8 | Extract agent turn display to agent.rs | 1h | ⏳ |
| A1.9 | Extract layout logic to layout.rs | 1h | ⏳ |

### A2: Split state.rs (6,014 lines)

**Target Structure:**
```
src/tui/state/
├── mod.rs           # TuiState struct, re-exports
├── core.rs          # Core fields, initialization
├── navigation.rs    # ViewNavigation, history
├── focus.rs         # FocusManager (SINGLE SOURCE OF TRUTH)
├── provider.rs      # ProviderState, selection
├── mcp.rs           # McpState, connections
├── chat.rs          # ChatState, conversations
└── settings.rs      # SettingsState, preferences
```

**Tasks:**
| ID | Task | Est | Status |
|----|------|-----|--------|
| A2.1 | Create state/ directory and mod.rs | 15m | ⏳ |
| A2.2 | Extract core fields to core.rs | 1h | ⏳ |
| A2.3 | Extract navigation to navigation.rs | 1h | ⏳ |
| A2.4 | Extract focus management to focus.rs | 1.5h | ⏳ |
| A2.5 | Extract provider state to provider.rs | 1h | ⏳ |
| A2.6 | Extract MCP state to mcp.rs | 1h | ⏳ |
| A2.7 | Extract chat state to chat.rs | 1h | ⏳ |
| A2.8 | Extract settings state to settings.rs | 1h | ⏳ |
| A2.9 | Remove 8 unused/write-only fields | 30m | ⏳ |

### A3: Split app.rs (5,028 lines)

**Target Structure:**
```
src/tui/app/
├── mod.rs           # App struct, run loop
├── routing.rs       # ViewRouter, navigation
├── events.rs        # EventHandler, key dispatch
├── lifecycle.rs     # Lifecycle hooks, init/cleanup
├── commands.rs      # CommandDispatcher, actions
└── render.rs        # RenderOrchestrator, frame
```

**Tasks:**
| ID | Task | Est | Status |
|----|------|-----|--------|
| A3.1 | Create app/ directory and mod.rs | 15m | ⏳ |
| A3.2 | Extract routing to routing.rs | 1h | ⏳ |
| A3.3 | Extract event handling to events.rs | 1.5h | ⏳ |
| A3.4 | Extract lifecycle to lifecycle.rs | 1h | ⏳ |
| A3.5 | Extract commands to commands.rs | 1h | ⏳ |
| A3.6 | Extract rendering to render.rs | 1h | ⏳ |

---

## Phase B: Consolidate Colors

### B1: Migrate to Token System

**Files to migrate (priority order):**
| File | Hardcoded Colors | Priority |
|------|------------------|----------|
| widgets/session_context.rs | 14 | P1 |
| widgets/dag_node_box.rs | 11 | P1 |
| widgets/activity_stack.rs | 9 | P1 |
| widgets/dag.rs | 7 | P2 |
| widgets/mcp_log.rs | 6 | P2 |
| widgets/status_bar.rs | 6 | P2 |
| views/settings.rs | 5 | P2 |
| views/chat.rs | 5 | P3 |
| (6 more files) | 17 total | P3 |

**Tasks:**
| ID | Task | Est | Status |
|----|------|-----|--------|
| B1.1 | Add missing colors to tokens/colors.rs | 30m | ⏳ |
| B1.2 | Migrate session_context.rs | 1h | ⏳ |
| B1.3 | Migrate dag_node_box.rs | 1h | ⏳ |
| B1.4 | Migrate activity_stack.rs | 45m | ⏳ |
| B1.5 | Migrate dag.rs | 45m | ⏳ |
| B1.6 | Migrate mcp_log.rs | 30m | ⏳ |
| B1.7 | Migrate status_bar.rs | 30m | ⏳ |
| B1.8 | Migrate remaining P2 files | 1.5h | ⏳ |
| B1.9 | Migrate remaining P3 files | 1h | ⏳ |

### B2: Remove Legacy Theme

**Tasks:**
| ID | Task | Est | Status |
|----|------|-----|--------|
| B2.1 | Identify essential Theme usages | 30m | ⏳ |
| B2.2 | Create migration shim if needed | 1h | ⏳ |
| B2.3 | Update 27 files using Theme | 2h | ⏳ |
| B2.4 | Mark Theme as deprecated | 15m | ⏳ |

### B3: Cleanup

**Tasks:**
| ID | Task | Est | Status |
|----|------|-----|--------|
| B3.1 | Remove unused color constants | 30m | ⏳ |
| B3.2 | Update documentation | 30m | ⏳ |
| B3.3 | Verify visual consistency | 30m | ⏳ |

---

## Phase C: Fix Keybindings

### C1: Standardize View Switching

**Current conflicts:**
- Keys 1-5: View switching VS Settings panel focus
- Need: Ctrl+1-8 for global view switching

**Tasks:**
| ID | Task | Est | Status |
|----|------|-----|--------|
| C1.1 | Change view switching to Ctrl+1-8 | 1h | ⏳ |
| C1.2 | Update Settings to use 1-5 locally | 30m | ⏳ |
| C1.3 | Update help text and docs | 30m | ⏳ |

### C2: Fix Key Conflicts

**Conflicts to resolve:**
| Key | Current Meanings | Resolution |
|-----|------------------|------------|
| 'c' | Copy, Create, Cancel, Close | c=copy, Ctrl+n=create, Esc=cancel |
| 't' | Toggle, Tab, Theme, Test, Tree | t=toggle, Tab=cycle, Ctrl+t=theme |
| 'q' | Quit app, Exit view | q=back, Q=quit app |

**Tasks:**
| ID | Task | Est | Status |
|----|------|-----|--------|
| C2.1 | Resolve 'c' key conflict | 30m | ⏳ |
| C2.2 | Resolve 't' key conflict | 30m | ⏳ |
| C2.3 | Resolve 'q' key conflict | 30m | ⏳ |
| C2.4 | Fix remaining 14 conflicts | 1.5h | ⏳ |

### C3: Documentation

**Tasks:**
| ID | Task | Est | Status |
|----|------|-----|--------|
| C3.1 | Create keybinding matrix doc | 30m | ⏳ |
| C3.2 | Update CLAUDE.md | 15m | ⏳ |
| C3.3 | Update help view | 30m | ⏳ |

---

## Phase D: Widget Unification

### D1: Create GenericBox Trait

**Design:**
```rust
pub trait BoxWidget {
    fn icon(&self) -> &str;
    fn title(&self) -> &str;
    fn status(&self) -> BoxStatus;
    fn content(&self) -> &str;
    fn border_style(&self, theme: &ColorPalette) -> Style;
}

pub struct GenericBox<T: BoxWidget> {
    inner: T,
    max_width: u16,
    show_border: bool,
}
```

**Tasks:**
| ID | Task | Est | Status |
|----|------|-----|--------|
| D1.1 | Design BoxWidget trait | 1h | ⏳ |
| D1.2 | Implement GenericBox struct | 2h | ⏳ |
| D1.3 | Write trait tests | 1h | ⏳ |

### D2: Refactor Box Variants

**Widgets to refactor:**
1. InferBox → implements BoxWidget
2. AgentBox → implements BoxWidget
3. McpCallBox → implements BoxWidget
4. TaskBox → implements BoxWidget
5. DagNodeBox → implements BoxWidget
6. InferStreamBox → implements BoxWidget

**Tasks:**
| ID | Task | Est | Status |
|----|------|-----|--------|
| D2.1 | Refactor InferBox | 1h | ⏳ |
| D2.2 | Refactor AgentBox | 1h | ⏳ |
| D2.3 | Refactor McpCallBox | 1h | ⏳ |
| D2.4 | Refactor TaskBox | 1.5h | ⏳ |
| D2.5 | Refactor DagNodeBox | 1h | ⏳ |
| D2.6 | Refactor InferStreamBox | 1h | ⏳ |

### D3: Unify State Management

**Issues to fix:**
- 3 sources of focus truth → 1
- 27 frame counters → centralized
- 5 scroll states → unified

**Tasks:**
| ID | Task | Est | Status |
|----|------|-----|--------|
| D3.1 | Centralize focus management | 2h | ⏳ |
| D3.2 | Centralize frame counter | 1h | ⏳ |
| D3.3 | Unify scroll state | 1.5h | ⏳ |

---

## Execution Protocol

### Before Each Task

```bash
# 1. Ensure clean state
git status  # Must be clean
cargo check # Must pass
cargo test  # Must pass

# 2. Create checkpoint
git stash push -m "checkpoint-before-TASK_ID"
```

### After Each Task

```bash
# 1. Verify
cargo check
cargo test
cargo clippy -- -D warnings

# 2. Commit granularly
git add <specific-files>
git commit -m "refactor(tui): <description>

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <nika@supernovae.studio>"

# 3. Update this document
# Mark task as ✅
```

### Ralph Wiggum Checkpoint

After completing each phase, run full audit:

```bash
# Full verification
cargo check
cargo test
cargo clippy -- -D warnings
cargo doc --no-deps

# Agent audit
# Launch code-reviewer agent with phase-specific focus
```

---

## Success Criteria

| Metric | Before | Target | After |
|--------|--------|--------|-------|
| Largest file | 9,327 lines | <1,000 lines | ⏳ |
| Hardcoded colors | 80+ | 0 | ⏳ |
| Keybinding conflicts | 17 | 0 | ⏳ |
| Box widget duplication | 564 lines | <100 lines | ⏳ |
| Focus sources of truth | 3 | 1 | ⏳ |
| Frame counter instances | 27 | 1 | ⏳ |

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Breaking changes | Granular commits, easy revert |
| Visual regressions | Manual testing after color changes |
| Test failures | Run tests after each extraction |
| Scope creep | Strict adherence to task list |

---

## Progress Tracker

```
Phase A: ░░░░░░░░░░░░░░░░░░░░ 0%
Phase B: ░░░░░░░░░░░░░░░░░░░░ 0%
Phase C: ░░░░░░░░░░░░░░░░░░░░ 0%
Phase D: ░░░░░░░░░░░░░░░░░░░░ 0%
─────────────────────────────
Overall: ░░░░░░░░░░░░░░░░░░░░ 0%
```

---

## Changelog

| Date | Phase | Change |
|------|-------|--------|
| 2026-03-07 | - | Master plan created |
