# Plan C+: Wiring First + Quick Wins

**Date:** 2026-02-26
**Scope:** Wire existing code + minimal high-impact additions
**Effort:** ~8-10 hours
**Priority:** Maximum ROI - use what's already built

---

## Overview

The audit revealed that **significant code already exists but isn't wired**. Plan C+ focuses on:
1. Wire TokenVelocity (code exists, never used)
2. Wire RenderMode (field exists, render ignores it)
3. Add quick visual wins (signal names, cost display)

**Philosophy:** No new widgets until existing code is live.

---

## Phase 1: Wire TokenVelocity (2h)

TokenVelocity already exists in `token_velocity.rs` with full functionality:
- Ring buffer with 32 samples
- Sparkline rendering (`sparkline_chars()`)
- Statistics (average, peak, min)

**Problem:** No TaskBox uses it.

### Task 1.1: Add TokenVelocity to InferBox (45min)
**File:** `src/tui/widgets/task_box/infer.rs`

**Changes:**
```rust
// Add import
use super::TokenVelocity;

// Add field to InferBox
pub struct InferBox {
    // ... existing fields ...
    pub velocity: TokenVelocity,
}

// Initialize in new()
velocity: TokenVelocity::new(32),

// Add builder method
pub fn with_velocity(mut self, velocity: TokenVelocity) -> Self {
    self.velocity = velocity;
    self
}

// Add push method for streaming
pub fn push_velocity_sample(&mut self, tokens_per_sec: f32) {
    self.velocity.push(tokens_per_sec);
}
```

**Test (RED first):**
```rust
#[test]
fn test_infer_box_has_velocity() {
    let mut box_ = InferBox::new("model", "prompt");
    assert_eq!(box_.velocity.samples().len(), 0);

    box_.push_velocity_sample(10.0);
    box_.push_velocity_sample(25.0);

    assert_eq!(box_.velocity.samples().len(), 2);
    assert!((box_.velocity.average() - 17.5).abs() < 0.1);
}
```

### Task 1.2: Add TokenVelocity to AgentBox (45min)
**File:** `src/tui/widgets/task_box/agent.rs`

Same pattern as InferBox. AgentBox aggregates velocity across all turns.

### Task 1.3: Render Sparkline in Running State (30min)
**File:** Both `infer.rs` and `agent.rs`

**In render(), when state is Running and velocity has samples:**
```rust
if self.state.is_running() && self.velocity.samples().len() > 0 {
    let sparkline = self.velocity.sparkline_chars();
    let avg = self.velocity.average();
    // Render: "▁▂▃▅▇█▇▅ 45 tok/s"
    buf.set_string(x, y, format!("{} {:.0} tok/s", sparkline, avg), dim_style);
}
```

---

## Phase 2: Wire RenderMode (1.5h)

RenderMode field exists in all 5 boxes but `render()` ignores it.

### Task 2.1: Implement Compact Mode for InferBox (30min)
**File:** `src/tui/widgets/task_box/infer.rs`

```rust
fn render(self, area: Rect, buf: &mut Buffer) {
    match self.render_mode {
        RenderMode::Compact => self.render_compact(area, buf),
        RenderMode::Expanded => self.render_expanded(area, buf),
        RenderMode::Full => self.render_full(area, buf),
    }
}

fn render_compact(&self, area: Rect, buf: &mut Buffer) {
    // Single line: ⚡ claude-sonnet-4 │ ⣾ "Generate a..." │ 123 tok
    let line = format!(
        "⚡ {} │ {} \"{}\" │ {} tok",
        Self::truncate(&self.model, 15),
        self.state.icon(),
        Self::truncate(&self.prompt, 20),
        self.tokens_out.unwrap_or(0)
    );
    buf.set_string(area.x, area.y, &line, Style::default());
}
```

### Task 2.2: Implement Compact Mode for ExecBox (30min)
**File:** `src/tui/widgets/task_box/exec.rs`

```rust
fn render_compact(&self, area: Rect, buf: &mut Buffer) {
    // Single line: 📟 $ npm run build │ ✅ 0 │ 1.2s
    let exit_icon = match self.exit_code {
        Some(0) => "✅",
        Some(_) => "❌",
        None => "⣾",
    };
    let line = format!(
        "📟 $ {} │ {} {} │ {}",
        Self::truncate(&self.command, 30),
        exit_icon,
        self.exit_code.map(|c| c.to_string()).unwrap_or("?".into()),
        self.state.suffix()
    );
    buf.set_string(area.x, area.y, &line, Style::default());
}
```

### Task 2.3: Implement Compact Mode for Other Boxes (30min)
Apply same pattern to FetchBox, InvokeBox, AgentBox.

---

## Phase 3: Signal Exit Codes (1h)

### Task 3.1: Add signal_name() Function (30min)
**File:** `src/tui/widgets/task_box/exec.rs`

```rust
/// Map exit code to signal name
fn signal_name(code: i32) -> Option<&'static str> {
    match code {
        130 => Some("SIGINT"),   // Ctrl+C
        137 => Some("SIGKILL"),  // kill -9
        139 => Some("SIGSEGV"),  // Segfault
        143 => Some("SIGTERM"),  // kill
        _ => None,
    }
}
```

**Test:**
```rust
#[test]
fn test_signal_name_mapping() {
    assert_eq!(signal_name(143), Some("SIGTERM"));
    assert_eq!(signal_name(137), Some("SIGKILL"));
    assert_eq!(signal_name(130), Some("SIGINT"));
    assert_eq!(signal_name(0), None);
    assert_eq!(signal_name(1), None);
}
```

### Task 3.2: Update exit_display() to Show Signal (30min)
**File:** `src/tui/widgets/task_box/exec.rs`

```rust
fn exit_display(&self) -> String {
    match self.exit_code {
        Some(0) => "EXIT: 0 ✓".to_string(),
        Some(code) => {
            if let Some(signal) = signal_name(code) {
                format!("EXIT: {} ({}) ✗", code, signal)
            } else {
                format!("EXIT: {} ✗", code)
            }
        }
        None => "exit: ?".to_string(),
    }
}
```

---

## Phase 4: Cost Display (1h)

### Task 4.1: Add Cost Calculation (30min)
**File:** `src/tui/widgets/task_box/infer.rs`

```rust
pub cost: Option<f64>,

/// Calculate cost based on token counts and model
pub fn calculate_cost(input_tokens: u32, output_tokens: u32, model: &str) -> f64 {
    let (input_rate, output_rate) = match model {
        m if m.contains("claude") => (3.0, 15.0),   // $3/M in, $15/M out
        m if m.contains("gpt-4") => (5.0, 15.0),
        m if m.contains("gpt-3.5") => (0.5, 1.5),
        _ => (1.0, 3.0),
    };

    let input_cost = (input_tokens as f64 / 1_000_000.0) * input_rate;
    let output_cost = (output_tokens as f64 / 1_000_000.0) * output_rate;
    input_cost + output_cost
}

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
```

### Task 4.2: Display Cost in Footer (30min)
**File:** `src/tui/widgets/task_box/infer.rs`

In render(), add to metrics line:
```rust
let cost_str = self.cost
    .map(|c| format!(" │ 💰 ${:.4}", c))
    .unwrap_or_default();
```

---

## Phase 5: Quick Keyboard Wins (1h)

### Task 5.1: Add ForceKill KeyAction (30min)
**File:** `src/tui/widgets/task_box/key_action.rs`

```rust
pub enum KeyAction {
    // ... existing ...
    ForceKill,  // [K] SIGKILL
}

impl KeyAction {
    pub fn from_key(key: KeyCode) -> Option<Self> {
        match key {
            // ... existing ...
            KeyCode::Char('K') => Some(Self::ForceKill),
            // ...
        }
    }

    pub fn key_char(&self) -> char {
        match self {
            // ... existing ...
            Self::ForceKill => 'K',
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            // ... existing ...
            Self::ForceKill => "SIGKILL",
        }
    }
}
```

### Task 5.2: Wire ForceKill to ExecBox (30min)
**File:** `src/tui/widgets/task_box/mod.rs`

```rust
KeyAction::ForceKill => {
    if matches!(self, TaskBox::Exec(_)) {
        Some(action) // Caller handles SIGKILL
    } else {
        None
    }
}
```

---

## Phase 6: Shortcuts Footer (1.5h)

### Task 6.1: Add shortcuts_line() Method (30min)
**File:** `src/tui/widgets/task_box/mod.rs`

```rust
impl TaskBox {
    /// Generate shortcuts footer string
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

### Task 6.2: Add show_in_footer() to KeyAction (30min)
**File:** `src/tui/widgets/task_box/key_action.rs`

```rust
impl KeyAction {
    pub fn show_in_footer(&self) -> bool {
        matches!(self,
            Self::Copy | Self::Json | Self::Thinking | Self::Mode |
            Self::Retry | Self::Kill | Self::Rerun
        )
    }
}
```

### Task 6.3: Render Footer in All Boxes (30min)
**Files:** All 5 box files

In SUCCESS state render:
```rust
if self.state.is_success() {
    let shortcuts = self.shortcuts_line(); // via TaskBox wrapper
    buf.set_string(area.x + 2, y, &shortcuts, dim_style);
}
```

---

## Verification Checklist

- [ ] TokenVelocity wired to InferBox
- [ ] TokenVelocity wired to AgentBox
- [ ] Sparkline renders in Running state
- [ ] RenderMode::Compact works for all 5 boxes
- [ ] Signal names shown for exit codes 130, 137, 143
- [ ] Cost calculation implemented
- [ ] Cost displays as 💰 $X.XXXX
- [ ] ForceKill (K) action added
- [ ] Shortcuts footer shows in SUCCESS state
- [ ] All tests pass (TDD)
- [ ] Zero clippy warnings

---

## Estimated Effort

| Phase | Hours | Tasks |
|-------|-------|-------|
| 1. Wire TokenVelocity | 2h | InferBox, AgentBox, sparkline render |
| 2. Wire RenderMode | 1.5h | Compact mode for all 5 boxes |
| 3. Signal Exit Codes | 1h | signal_name() + exit_display() |
| 4. Cost Display | 1h | Calculation + render |
| 5. Keyboard Wins | 1h | ForceKill + wiring |
| 6. Shortcuts Footer | 1.5h | Method + render integration |
| **TOTAL** | **8h** | 12 tasks |

---

## What's NOT Included (requires Plan B or A)

- DecryptEffect animation (queued state)
- GlitchShake animation (failure)
- HTTP method badges
- Thinking section UI
- Timeout progress bar
- Server status indicator 🟢/🔴
- Streaming cursor ▌
- NIKA error code hints
- AgentBox turn progress bar
- Retry countdown animation

---

## Why Plan C+ First?

1. **Maximize existing investment:** TokenVelocity and RenderMode are already built
2. **Quick visual impact:** Signal names and cost display are high-value, low-effort
3. **Foundation for Plans B/A:** Wiring must happen before new features
4. **Lower risk:** Using tested code, not writing new widgets

After Plan C+ completes, Plan B adds user-facing features, then Plan A adds polish.
