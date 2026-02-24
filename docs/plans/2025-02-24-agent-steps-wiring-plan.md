# Plan: Agent Steps Widget Wiring + Matrix Effect

**Date:** 2025-02-24
**Version:** v0.8.1
**Effort:** ~4-5h
**Approach:** TDD + Subagent-Driven Development

## Objective

Wire the existing `AgentStepsWidget` into `ChatView` to show real-time agent phases instead of generic "Generating..." placeholder. Add Matrix effect on status text.

## Current State

- ✅ `AgentStepsWidget` exists (~2000 lines, fully tested)
- ✅ Exported from `mod.rs`
- ❌ Not used in `chat.rs`
- ❌ "Generating..." hardcoded placeholder

## Target State

```
┌─ Chat ──────────────────────────────────────────────────────────────────┐
│                                                                         │
│  You: Generate a landing page for QR codes                              │
│                                                                         │
│  ┌─ 🐔 Nika ─────────────────────────────────────────────────────────┐  │
│  │                                                                   │  │
│  │  🦋⟷🔌 カタInvokingカナ novanet_describe...                        │  │
│  │  ├── server: novanet                                              │  │
│  │  ├── params: { entity: "qr-code" }                                │  │
│  │  └── ⏱ 0.8s                                                       │  │
│  │                                                                   │  │
│  └───────────────────────────────────────────────────────────────────┘  │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

## Phase Vocabulary (Nika-branded)

| Phase | Icon | Label | Trigger Event |
|-------|------|-------|---------------|
| Connecting | 🦋 | "Syncing..." | `AgentStart` / API call initiated |
| Planning | 🐔 | "Planning..." | `AgentTurn` (start of turn) |
| Routing | 🔀 | "Routing..." | Tool selection in progress |
| Invoking | 🔌 | "Invoking {tool}..." | `McpInvoke` |
| Processing | ⚙️ | "Processing..." | `McpResponse` received |
| Inferring | ⚡ | "Inferring..." | `ProviderCalled` (infer verb) |
| Composing | ✍️ | "Composing..." | Response generation |
| Streaming | 📡 | (no label) | First token received |

## Tasks

### Task 1: Add AgentPhase Enum (TDD)
**File:** `src/tui/widgets/agent_steps.rs`
**Effort:** 30min

```rust
/// Agent execution phase for real-time status display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentPhase {
    #[default]
    Idle,
    Syncing,      // 🦋 Connecting to API
    Planning,     // 🐔 Analyzing context
    Routing,      // 🔀 Deciding tool
    Invoking,     // 🔌 Calling MCP tool
    Processing,   // ⚙️ Handling result
    Inferring,    // ⚡ LLM generation
    Composing,    // ✍️ Formulating response
    Streaming,    // 📡 Outputting tokens
}

impl AgentPhase {
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Idle => "",
            Self::Syncing => "🦋",
            Self::Planning => "🐔",
            Self::Routing => "🔀",
            Self::Invoking => "🔌",
            Self::Processing => "⚙️",
            Self::Inferring => "⚡",
            Self::Composing => "✍️",
            Self::Streaming => "📡",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "",
            Self::Syncing => "Syncing",
            Self::Planning => "Planning",
            Self::Routing => "Routing",
            Self::Invoking => "Invoking",
            Self::Processing => "Processing",
            Self::Inferring => "Inferring",
            Self::Composing => "Composing",
            Self::Streaming => "",
        }
    }

    /// For Matrix effect: alternate between 🦋 and phase icon
    pub fn animated_icon(&self, frame: u8) -> &'static str {
        if frame % 10 < 5 {
            "🦋" // Nika brand
        } else {
            self.icon()
        }
    }
}
```

**Tests:**
- `test_agent_phase_icons` - all phases have icons
- `test_agent_phase_labels` - all phases have labels
- `test_agent_phase_animated_icon` - alternates correctly

---

### Task 2: Add AgentPhaseIndicator Widget (TDD)
**File:** `src/tui/widgets/agent_steps.rs`
**Effort:** 1h

A lightweight widget that shows current phase with Matrix effect.

```rust
/// Compact phase indicator with Matrix effect
pub struct AgentPhaseIndicator {
    phase: AgentPhase,
    tool_name: Option<String>,  // For Invoking phase
    frame: u8,
    matrix_chars: Vec<char>,    // Chaos characters
}

impl AgentPhaseIndicator {
    pub fn new(phase: AgentPhase) -> Self { ... }
    pub fn with_tool(mut self, tool: &str) -> Self { ... }
    pub fn tick(&mut self) { ... }

    /// Build Line with Matrix effect on label
    pub fn build_line(&self) -> Line<'static> {
        // Example output: "🦋⟷🔌 カタInvokingカナ novanet_describe..."
    }
}
```

**Tests:**
- `test_phase_indicator_syncing` - shows 🦋 Syncing
- `test_phase_indicator_invoking_with_tool` - shows tool name
- `test_phase_indicator_matrix_effect` - chaos chars in label
- `test_phase_indicator_tick_animates` - frame advances

---

### Task 3: Add State to ChatView
**File:** `src/tui/views/chat.rs`
**Effort:** 30min

```rust
// In ChatView struct
pub struct ChatView {
    // ... existing fields ...

    // v0.8.1: Agent phase tracking
    agent_phase: AgentPhase,
    agent_phase_tool: Option<String>,
    agent_steps: AgentStepGroup,
    phase_indicator: AgentPhaseIndicator,
}

// In ChatView::new()
agent_phase: AgentPhase::Idle,
agent_phase_tool: None,
agent_steps: AgentStepGroup::new("Agent"),
phase_indicator: AgentPhaseIndicator::new(AgentPhase::Idle),
```

**Tests:**
- `test_chat_view_initial_phase_idle` - starts as Idle
- `test_chat_view_has_agent_steps` - AgentStepGroup exists

---

### Task 4: Event → Phase Mapping Methods
**File:** `src/tui/views/chat.rs`
**Effort:** 45min

```rust
impl ChatView {
    /// Called when agent starts
    pub fn on_agent_start(&mut self) {
        self.agent_phase = AgentPhase::Syncing;
        self.phase_indicator = AgentPhaseIndicator::new(AgentPhase::Syncing);
        self.agent_steps.clear();
        self.agent_steps.add_step(
            AgentStep::for_verb(VerbType::Agent, "Starting agent...")
                .with_status(StepStatus::Running)
        );
    }

    /// Called when agent turn begins
    pub fn on_agent_turn(&mut self, turn: u32, max_turns: u32) {
        self.agent_phase = AgentPhase::Planning;
        self.phase_indicator = AgentPhaseIndicator::new(AgentPhase::Planning);
        // Update step
    }

    /// Called when MCP tool is invoked
    pub fn on_mcp_invoke(&mut self, tool: &str, server: &str) {
        self.agent_phase = AgentPhase::Invoking;
        self.agent_phase_tool = Some(tool.to_string());
        self.phase_indicator = AgentPhaseIndicator::new(AgentPhase::Invoking)
            .with_tool(tool);
        // Add step with tool metadata
    }

    /// Called when MCP response received
    pub fn on_mcp_response(&mut self, result_preview: &str) {
        self.agent_phase = AgentPhase::Processing;
        self.phase_indicator = AgentPhaseIndicator::new(AgentPhase::Processing);
        // Update step with result
    }

    /// Called when provider/LLM called
    pub fn on_provider_called(&mut self, model: &str) {
        self.agent_phase = AgentPhase::Inferring;
        self.phase_indicator = AgentPhaseIndicator::new(AgentPhase::Inferring);
    }

    /// Called when first streaming token arrives
    pub fn on_streaming_start(&mut self) {
        self.agent_phase = AgentPhase::Streaming;
        self.phase_indicator = AgentPhaseIndicator::new(AgentPhase::Streaming);
    }

    /// Called when agent completes
    pub fn on_agent_complete(&mut self) {
        self.agent_phase = AgentPhase::Idle;
        // Mark all steps completed
    }
}
```

**Tests:**
- `test_on_agent_start_sets_syncing`
- `test_on_mcp_invoke_sets_invoking_with_tool`
- `test_on_streaming_start_sets_streaming`
- `test_on_agent_complete_resets_idle`

---

### Task 5: Wire Events in TuiState
**File:** `src/tui/state.rs`
**Effort:** 45min

Find where events are processed and call ChatView methods.

```rust
// In handle_event or process_workflow_event
match &event.kind {
    EventKind::AgentStart { task_id, .. } => {
        if let Some(chat) = &mut self.chat_view {
            chat.on_agent_start();
        }
    }
    EventKind::AgentTurn { turn, max_turns, .. } => {
        if let Some(chat) = &mut self.chat_view {
            chat.on_agent_turn(*turn, *max_turns);
        }
    }
    EventKind::McpInvoke { tool, server, .. } => {
        if let Some(chat) = &mut self.chat_view {
            chat.on_mcp_invoke(tool, server);
        }
    }
    EventKind::McpResponse { .. } => {
        if let Some(chat) = &mut self.chat_view {
            chat.on_mcp_response("...");
        }
    }
    // ... etc
}
```

**Tests:**
- `test_event_agent_start_updates_chat_phase`
- `test_event_mcp_invoke_updates_chat_phase`

---

### Task 6: Replace "Generating..." with PhaseIndicator
**File:** `src/tui/views/chat.rs`
**Effort:** 1h

Find the "Generating..." render code and replace with AgentPhaseIndicator.

```rust
// BEFORE (around line 3200)
if self.partial_response.is_empty() {
    // "Generating..." with dots
}

// AFTER
if self.partial_response.is_empty() && self.agent_phase != AgentPhase::Idle {
    // Render phase indicator with Matrix effect
    let indicator_line = self.phase_indicator.build_line();
    // ... render indicator_line
}
```

**Tests:**
- `test_render_shows_phase_indicator_not_generating`
- `test_render_shows_tool_name_when_invoking`

---

### Task 7: Add Matrix Effect to PhaseIndicator
**File:** `src/tui/widgets/agent_steps.rs`
**Effort:** 45min

Integrate StreamingDecrypt-style chaos into the label.

```rust
impl AgentPhaseIndicator {
    pub fn build_line(&self) -> Line<'static> {
        let mut spans = vec![];

        // Animated icon (🦋 ⟷ phase_icon)
        let icon = self.phase.animated_icon(self.frame);
        spans.push(Span::styled(icon, Style::default()));
        spans.push(Span::raw(" "));

        // Label with Matrix chaos effect
        let label = self.phase.label();
        for (i, ch) in label.chars().enumerate() {
            if self.should_show_chaos(i) {
                // Show random katakana
                let chaos = self.matrix_chars[i % self.matrix_chars.len()];
                spans.push(Span::styled(
                    chaos.to_string(),
                    Style::default().fg(solarized::CYAN)
                ));
            } else {
                // Show actual character
                spans.push(Span::styled(
                    ch.to_string(),
                    Style::default().fg(solarized::BASE1)
                ));
            }
        }

        // Tool name (if invoking)
        if let Some(tool) = &self.tool_name {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                tool.clone(),
                Style::default().fg(solarized::BLUE).add_modifier(Modifier::BOLD)
            ));
        }

        spans.push(Span::styled("...", Style::default().fg(solarized::BASE1)));

        Line::from(spans)
    }
}
```

**Tests:**
- `test_matrix_effect_on_label`
- `test_chaos_decreases_over_time`

---

### Task 8: Tick Animation in Main Loop
**File:** `src/tui/views/chat.rs` or `src/tui/app.rs`
**Effort:** 15min

Ensure `phase_indicator.tick()` is called each frame.

```rust
// In tick() or update()
if self.agent_phase != AgentPhase::Idle {
    self.phase_indicator.tick();
}
```

**Tests:**
- `test_tick_advances_animation_frame`

---

### Task 9: Export AgentPhase from mod.rs
**File:** `src/tui/widgets/mod.rs`
**Effort:** 5min

```rust
pub use agent_steps::{
    AgentPhase, AgentPhaseIndicator,  // NEW
    AgentStep, AgentStepGroup, AgentStepsWidget, StepStatus, TokenUsage, ToolCallMetadata,
};
```

---

### Task 10: Integration Test
**File:** `tests/tui_agent_phase_test.rs`
**Effort:** 30min

End-to-end test that verifies:
1. Agent start → Syncing phase
2. MCP invoke → Invoking phase with tool name
3. First token → Streaming phase
4. Agent complete → Idle phase

---

## Execution Order (TDD)

```
Task 1 (AgentPhase enum)
    ↓
Task 2 (AgentPhaseIndicator widget)
    ↓
Task 9 (Export)
    ↓
Task 3 (ChatView state)
    ↓
Task 4 (Event mapping methods)
    ↓
Task 5 (Wire in TuiState)
    ↓
Task 6 (Replace "Generating...")
    ↓
Task 7 (Matrix effect)
    ↓
Task 8 (Tick animation)
    ↓
Task 10 (Integration test)
```

## Success Criteria

- [ ] All unit tests pass
- [ ] "Generating..." replaced with real phase indicator
- [ ] Phase changes based on actual events
- [ ] Matrix effect visible on label text
- [ ] 🦋 ⟷ phase_icon animation works
- [ ] Tool name shown when invoking MCP
- [ ] No regressions in existing tests

## Files Modified

| File | Changes |
|------|---------|
| `src/tui/widgets/agent_steps.rs` | +AgentPhase, +AgentPhaseIndicator |
| `src/tui/widgets/mod.rs` | Export new types |
| `src/tui/views/chat.rs` | State, methods, render |
| `src/tui/state.rs` | Event → ChatView wiring |
| `tests/tui_agent_phase_test.rs` | Integration test |

## Estimated Effort

| Task | Effort |
|------|--------|
| Task 1 | 30min |
| Task 2 | 1h |
| Task 3 | 30min |
| Task 4 | 45min |
| Task 5 | 45min |
| Task 6 | 1h |
| Task 7 | 45min |
| Task 8 | 15min |
| Task 9 | 5min |
| Task 10 | 30min |
| **Total** | **~5-6h** |
