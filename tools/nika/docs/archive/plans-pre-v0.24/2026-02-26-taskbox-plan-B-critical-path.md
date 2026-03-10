# Plan B: TaskBox Critical Path Only

**Date:** 2026-02-26
**Scope:** Core features that directly impact user experience
**Effort:** ~15-20 hours
**Priority:** High-impact features, skip polish

---

## Overview

Focus on features that users will notice and need most:
- Token/cost tracking (core metrics)
- Thinking UI (Claude's reasoning)
- Keyboard shortcuts (productivity)
- Error handling (debugging experience)

**NOT included:** DecryptEffect, GlitchShake, HTTP badges, COMPACT mode

---

## Phase 1: Wire TokenVelocity (2h)

### Task 1.1: Wire TokenVelocity to InferBox (1h)
**File:** `src/tui/widgets/task_box/infer.rs`

**Current state:** TokenVelocity exists in `token_velocity.rs` but is never used.

**Changes:**
```rust
// Add field to InferBox struct
pub struct InferBox {
    // ... existing fields ...
    pub velocity: TokenVelocity,
}

// Initialize in new()
velocity: TokenVelocity::new(32),

// Add method to push samples
pub fn push_velocity(&mut self, tokens_per_sec: f32) {
    self.velocity.push(tokens_per_sec);
}

// Render in RUNNING state
if self.state.is_running() && self.velocity.samples().len() > 0 {
    let sparkline = self.velocity.sparkline_chars();
    let avg = self.velocity.average();
    buf.set_string(x, y, format!("Token velocity: {} ({:.0} tok/s)", sparkline, avg), dim_style);
}
```

**Test (RED first):**
```rust
#[test]
fn test_infer_box_velocity_tracking() {
    let mut box_ = InferBox::new("claude", "prompt");
    assert_eq!(box_.velocity.samples().len(), 0);

    box_.push_velocity(10.0);
    box_.push_velocity(25.0);
    box_.push_velocity(40.0);

    assert_eq!(box_.velocity.samples().len(), 3);
    assert!((box_.velocity.average() - 25.0).abs() < 0.1);
    assert!((box_.velocity.peak() - 40.0).abs() < 0.1);
}
```

### Task 1.2: Wire TokenVelocity to AgentBox (1h)
**File:** `src/tui/widgets/task_box/agent.rs`

**Changes:**
```rust
pub struct AgentBox {
    // ... existing fields ...
    pub velocity: TokenVelocity,
}

// Aggregate velocity across turns
pub fn push_velocity(&mut self, tokens_per_sec: f32) {
    self.velocity.push(tokens_per_sec);
}
```

---

## Phase 2: Cost Display (2h)

### Task 2.1: Add Cost Calculation (1h)
**File:** `src/tui/widgets/task_box/infer.rs`

**Changes:**
```rust
// Add cost field
pub cost: Option<f64>,

// Pricing function (Claude-focused)
pub fn calculate_cost(input_tokens: u32, output_tokens: u32, model: &str) -> f64 {
    let (input_rate, output_rate) = match model {
        m if m.contains("claude") => (3.0, 15.0),   // $3/M in, $15/M out
        m if m.contains("gpt-4") => (5.0, 15.0),    // $5/M in, $15/M out
        m if m.contains("gpt-3.5") => (0.5, 1.5),   // $0.5/M in, $1.5/M out
        m if m.contains("mistral") => (2.0, 6.0),   // $2/M in, $6/M out
        _ => (1.0, 3.0), // Default fallback
    };

    let input_cost = (input_tokens as f64 / 1_000_000.0) * input_rate;
    let output_cost = (output_tokens as f64 / 1_000_000.0) * output_rate;
    input_cost + output_cost
}

// Update cost after tokens change
pub fn update_cost(&mut self) {
    if let (Some(input), Some(output)) = (self.tokens_in, self.tokens_out) {
        self.cost = Some(Self::calculate_cost(input, output, &self.model));
    }
}
```

**Test:**
```rust
#[test]
fn test_cost_calculation_claude() {
    let cost = InferBox::calculate_cost(1000, 500, "claude-sonnet-4");
    // 1000 * $3/M + 500 * $15/M = $0.003 + $0.0075 = $0.0105
    assert!((cost - 0.0105).abs() < 0.0001);
}

#[test]
fn test_cost_calculation_gpt4() {
    let cost = InferBox::calculate_cost(1000, 500, "gpt-4o");
    // 1000 * $5/M + 500 * $15/M = $0.005 + $0.0075 = $0.0125
    assert!((cost - 0.0125).abs() < 0.0001);
}
```

### Task 2.2: Render Cost Display (1h)
**File:** `src/tui/widgets/task_box/infer.rs`

**Changes:**
```rust
// In render(), add to metrics line:
let cost_str = self.cost
    .map(|c| format!(" │ 💰 ${:.2}", c))
    .unwrap_or_default();

let metrics = format!(
    "│ {} {} │ 📊 {} in / {} out{}│",
    self.provider_icon(),
    self.model,
    format_tokens(self.tokens_in),
    format_tokens(self.tokens_out),
    cost_str
);
```

---

## Phase 3: Thinking Section UI (4h)

### Task 3.1: Add Thinking Fields to InferBox (1h)
**File:** `src/tui/widgets/task_box/infer.rs`

**Changes:**
```rust
pub struct InferBox {
    // ... existing fields ...
    pub thinking: Option<String>,
    pub expanded_thinking: bool,
}

// Builder method
pub fn with_thinking(mut self, thinking: impl Into<String>) -> Self {
    self.thinking = Some(thinking.into());
    self
}

// Toggle method
pub fn toggle_thinking(&mut self) {
    self.expanded_thinking = !self.expanded_thinking;
}
```

### Task 3.2: Render Thinking Section (2h)
**File:** `src/tui/widgets/task_box/infer.rs`

**Changes:**
```rust
// Add to render() after response section:
if let Some(ref thinking) = self.thinking {
    // Separator
    let sep = format!("├{}┤", "─".repeat(inner_width));
    buf.set_string(area.x, y, &sep, border_style);
    y += 1;

    // Header with toggle indicator
    let thinking_tokens = thinking.split_whitespace().count() as u32;
    let toggle_icon = if self.expanded_thinking { "▼" } else { "▶" };
    let header = format!(
        "│ {} THINKING                                         {} {} │",
        toggle_icon,
        "💭",
        thinking_tokens
    );
    buf.set_string(area.x, y, &header, thinking_style);
    y += 1;

    // Content (if expanded)
    if self.expanded_thinking {
        let lines: Vec<&str> = thinking.lines().take(10).collect();
        for line in lines {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);
            let truncated = Self::truncate(line, inner_width - 4);
            buf.set_string(area.x + 2, y, format!("┊ {}", truncated), thinking_style);
            y += 1;
        }
    }
}
```

### Task 3.3: Wire [t] Key Handler (1h)
**File:** `src/tui/widgets/task_box/mod.rs`

**Changes:**
```rust
// In handle_key():
KeyAction::Thinking => {
    match self {
        TaskBox::Infer(b) => {
            b.toggle_thinking();
            Some(action)
        }
        TaskBox::Agent(b) => {
            b.toggle_thinking();
            Some(action)
        }
        _ => None,
    }
}
```

**Test:**
```rust
#[test]
fn test_infer_box_thinking_toggle() {
    let mut box_ = TaskBox::Infer(
        InferBox::new("claude", "prompt")
            .with_thinking("Let me analyze this...")
    );

    // Initially collapsed
    if let TaskBox::Infer(ref inner) = box_ {
        assert!(!inner.expanded_thinking);
    }

    // Toggle with 't'
    let action = box_.handle_key(KeyCode::Char('t'));
    assert_eq!(action, Some(KeyAction::Thinking));

    // Now expanded
    if let TaskBox::Infer(ref inner) = box_ {
        assert!(inner.expanded_thinking);
    }
}
```

---

## Phase 4: Shortcuts Footer (2h)

### Task 4.1: Add Shortcuts Render Method (1h)
**File:** `src/tui/widgets/task_box/mod.rs`

**Changes:**
```rust
impl TaskBox {
    /// Render shortcuts footer for SUCCESS state
    pub fn shortcuts_line(&self) -> String {
        self.available_actions()
            .iter()
            .filter(|a| a.show_in_footer())
            .map(|a| format!("[{}] {}", a.key_char(), a.label()))
            .collect::<Vec<_>>()
            .join("  ")
    }
}
```

### Task 4.2: Integrate into All Boxes (1h)
**Files:** All 5 box files

**Changes for each box in SUCCESS render:**
```rust
// Before bottom border, if success:
if self.state.is_success() {
    let shortcuts = /* get from TaskBox wrapper or inline */;
    buf.set_string(area.x, y, "│", border_style);
    buf.set_string(area.x + area.width - 1, y, "│", border_style);
    buf.set_string(area.x + 2, y, &shortcuts, dim_style);
    y += 1;
}
```

---

## Phase 5: Error Code Display (3h)

### Task 5.1: Parse NIKA Error Codes (1h)
**File:** `src/tui/widgets/task_box/invoke.rs`

**Changes:**
```rust
/// Extract NIKA error code from error message
fn parse_nika_code(error: &str) -> Option<(&str, &str)> {
    // Pattern: "[NIKA-XXX]" or "NIKA-XXX"
    let re = regex::Regex::new(r"NIKA-(\d{3})").ok()?;
    if let Some(captures) = re.captures(error) {
        let code = captures.get(0)?.as_str();
        let hint = match captures.get(1)?.as_str() {
            "100" => "Check if MCP server is running",
            "102" => "Increase timeout or retry",
            "109" => "Server response timeout",
            _ => "",
        };
        return Some((code, hint));
    }
    None
}
```

### Task 5.2: Display Error with Hint (1h)
**File:** `src/tui/widgets/task_box/invoke.rs`

**Changes:**
```rust
// In render() for Failed state:
if let Some(ref error) = self.error {
    buf.set_string(area.x + 2, y, "ERROR", error_style);
    y += 1;

    // Show error message
    let truncated = Self::truncate(error, inner_width - 4);
    buf.set_string(area.x + 2, y, format!("┊ {}", truncated), error_style);
    y += 1;

    // Show hint if NIKA error code found
    if let Some((code, hint)) = parse_nika_code(error) {
        if !hint.is_empty() {
            buf.set_string(area.x + 2, y, format!("┊ 💡 {}", hint), hint_style);
            y += 1;
        }
    }
}
```

### Task 5.3: Add Server Status Indicator (1h)
**File:** `src/tui/widgets/task_box/invoke.rs`

**Changes:**
```rust
pub struct InvokeBox {
    // ... existing fields ...
    pub server_connected: bool,
}

// In render():
let status_emoji = if self.server_connected { "🟢" } else { "🔴" };
buf.set_string(area.x + 2, y, format!("{} server: {}", status_emoji, self.server), dim_style);
```

---

## Phase 6: Signal Exit Codes (2h)

### Task 6.1: Signal Name Mapping (1h)
**File:** `src/tui/widgets/task_box/exec.rs`

**Changes:**
```rust
/// Map exit code to signal name if applicable
fn signal_name(code: i32) -> Option<&'static str> {
    match code {
        130 => Some("SIGINT"),   // Ctrl+C
        137 => Some("SIGKILL"),  // kill -9
        139 => Some("SIGSEGV"),  // Segfault
        143 => Some("SIGTERM"),  // kill
        _ => None,
    }
}

/// Updated exit display
fn exit_display(&self) -> String {
    match self.exit_code {
        Some(0) => "EXIT: 0 ✓".to_string(),
        Some(code) => {
            if let Some(signal) = signal_name(code) {
                format!("EXIT: {} ({})", code, signal)
            } else {
                format!("EXIT: {} ✗", code)
            }
        }
        None => "exit: ?".to_string(),
    }
}
```

**Test:**
```rust
#[test]
fn test_signal_exit_codes() {
    assert_eq!(signal_name(143), Some("SIGTERM"));
    assert_eq!(signal_name(137), Some("SIGKILL"));
    assert_eq!(signal_name(0), None);
    assert_eq!(signal_name(1), None);
}

#[test]
fn test_exit_display_with_signal() {
    let box_ = ExecBox::new("cmd").with_exit_code(143);
    assert!(box_.exit_display().contains("SIGTERM"));
}
```

### Task 6.2: Add SIGKILL Shortcut (1h)
**File:** `src/tui/widgets/task_box/key_action.rs`

**Changes:**
```rust
pub enum KeyAction {
    // ... existing variants ...
    Kill,       // [k] SIGTERM
    ForceKill,  // [K] SIGKILL
}

impl KeyAction {
    pub fn from_key(key: KeyCode) -> Option<Self> {
        match key {
            // ... existing mappings ...
            KeyCode::Char('k') => Some(Self::Kill),
            KeyCode::Char('K') => Some(Self::ForceKill),
            // ...
        }
    }
}
```

---

## Phase 7: Streaming Cursor (2h)

### Task 7.1: Add Cursor to InferBox (1h)
**File:** `src/tui/widgets/task_box/infer.rs`

**Changes:**
```rust
// Use ▌ instead of █
const CURSOR_CHAR: char = '▌';

// In render(), when showing streaming response:
if self.state.is_running() && self.streaming_cursor {
    let last_line = response_lines.last().map(|s| *s).unwrap_or("");
    let with_cursor = format!("{}{}", last_line, CURSOR_CHAR);
    buf.set_string(area.x + 2, y, format!("┊ {}", with_cursor), content_style);
} else {
    // Normal rendering without cursor
}
```

### Task 7.2: Add Cursor to ExecBox (1h)
**File:** `src/tui/widgets/task_box/exec.rs`

**Changes:**
```rust
// Add streaming_cursor field
pub streaming_cursor: bool,

// In render() for STDOUT section:
if self.state.is_running() && self.streaming_cursor && !self.stdout.is_empty() {
    // Append cursor to last line
    let last_idx = stdout_lines.len() - 1;
    if last_idx < stdout_lines.len() {
        let with_cursor = format!("{}▌", stdout_lines[last_idx]);
        buf.set_string(area.x + 2, y, format!("┊ {}", with_cursor), content_style);
    }
}
```

---

## Verification Checklist

- [ ] TokenVelocity wired to InferBox
- [ ] TokenVelocity wired to AgentBox
- [ ] Cost calculation implemented
- [ ] Cost display shows 💰 $X.XX
- [ ] Thinking section renders in InferBox
- [ ] [t] key toggles thinking section
- [ ] Shortcuts footer shows in SUCCESS state
- [ ] NIKA error codes parsed and displayed
- [ ] Error hints shown for known codes
- [ ] Server status indicator 🟢/🔴
- [ ] Signal exit codes (SIGTERM, SIGKILL) displayed
- [ ] [K] SIGKILL shortcut added
- [ ] Streaming cursor ▌ in InferBox
- [ ] Streaming cursor ▌ in ExecBox
- [ ] All tests pass (TDD)
- [ ] Zero clippy warnings

---

## Estimated Effort

| Phase | Hours | Tasks |
|-------|-------|-------|
| 1. Wire TokenVelocity | 2h | InferBox, AgentBox |
| 2. Cost Display | 2h | Calculation + render |
| 3. Thinking Section UI | 4h | Fields, render, key handler |
| 4. Shortcuts Footer | 2h | Method + integration |
| 5. Error Code Display | 3h | Parse + hints + server status |
| 6. Signal Exit Codes | 2h | Mapping + SIGKILL shortcut |
| 7. Streaming Cursor | 2h | InferBox + ExecBox |
| **TOTAL** | **17h** | 14 tasks |

---

## What's NOT Included (deferred to Plan A)

- DecryptEffect animation (queued state)
- GlitchShake animation (failure)
- HTTP method badges (🟢🔵🔴)
- RenderMode (Compact/Expanded/Full)
- AgentBox COMPACT mode
- Turn progress bar
- Retry countdown animation
- Timeout progress bar
