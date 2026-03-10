# Plan A: TaskBox Full Spec Compliance

**Date:** 2026-02-26
**Scope:** 100% implementation of TaskBox visual specification
**Effort:** ~40-50 hours
**Priority:** Complete feature parity with design spec

---

## Overview

Implement ALL features from the TaskBox visual specification across all 5 verb widgets (InferBox, ExecBox, FetchBox, InvokeBox, AgentBox) plus animations.

---

## Phase 1: Wire Existing Code (4h)

### Task 1.1: Wire TokenVelocity to InferBox (1h)
**File:** `src/tui/widgets/task_box/infer.rs`

**Changes:**
1. Add `velocity: TokenVelocity` field to InferBox struct
2. Add `with_velocity()` builder method
3. Add `push_velocity(&mut self, tokens_per_sec: f32)` method
4. Render sparkline in RUNNING state using `velocity.sparkline_data(8)`
5. Display "Token velocity: ▁▂▃▅▇█▇▅▃▂ (45 tok/s)" line

**Test (TDD):**
```rust
#[test]
fn test_infer_box_velocity_sparkline() {
    let mut box_ = InferBox::new("model", "prompt");
    box_.push_velocity(10.0);
    box_.push_velocity(20.0);
    box_.push_velocity(45.0);
    assert_eq!(box_.velocity.samples().len(), 3);
    assert!(box_.velocity.peak() >= 45.0);
}
```

### Task 1.2: Wire TokenVelocity to AgentBox (1h)
**File:** `src/tui/widgets/task_box/agent.rs`

**Changes:**
1. Add `velocity: TokenVelocity` field
2. Wire to render in RUNNING EXPANDED mode
3. Show cumulative velocity across all turns

### Task 1.3: Wire RenderMode to All Boxes (1h)
**Files:** All 5 box files

**Changes:**
1. InferBox.render() - branch on `self.render_mode`
2. ExecBox.render() - branch on `self.render_mode`
3. FetchBox.render() - branch on `self.render_mode`
4. InvokeBox.render() - branch on `self.render_mode`
5. AgentBox.render() - branch on `self.render_mode`

**Render modes:**
- `Compact`: 1-2 lines, icon + status only
- `Expanded`: Default, 5-10 lines with key info
- `Full`: All details, scrollable content

### Task 1.4: Wire KeyAction Handlers (1h)
**File:** `src/tui/widgets/task_box/mod.rs`

**Changes:**
1. `KeyAction::Copy` → Copy response to clipboard (requires clipboard crate)
2. `KeyAction::Json` → Copy as JSON
3. `KeyAction::Thinking` → Toggle thinking section (InferBox, AgentBox)

---

## Phase 2: Cost Display (2h)

### Task 2.1: Add Cost Calculation to InferBox (1h)
**File:** `src/tui/widgets/task_box/infer.rs`

**Changes:**
1. Add `cost: Option<f64>` field
2. Add `calculate_cost(input_tokens: u32, output_tokens: u32, model: &str) -> f64`
3. Pricing table:
   - Claude: $3/M input, $15/M output
   - GPT-4o: $5/M input, $15/M output
   - Mistral: $2/M input, $6/M output
4. Render "💰 $0.03" in metrics line

**Test:**
```rust
#[test]
fn test_infer_box_cost_calculation() {
    let cost = InferBox::calculate_cost(1000, 500, "claude-sonnet");
    assert!((cost - 0.0105).abs() < 0.0001); // $0.003 + $0.0075
}
```

### Task 2.2: Add Cost Display to AgentBox (1h)
**File:** `src/tui/widgets/task_box/agent.rs`

**Changes:**
1. Cost already exists as field
2. Format as "💰 $X.XX" in metrics line
3. Aggregate cost from all children

---

## Phase 3: Shortcuts Footer (2h)

### Task 3.1: Add Shortcuts Display (2h)
**Files:** All 5 box files

**Changes for each box in SUCCESS state:**
1. InferBox: `[c] Copy  [j] JSON  [t] Thinking  [m] Mode`
2. ExecBox: `[c] Copy output  [r] Retry  [m] Mode`
3. FetchBox: `[c] Copy cURL  [j] Export JSON  [r] Retry`
4. InvokeBox: `[j] Export JSON  [c] Copy result  [r] Retry`
5. AgentBox: `[c] Copy response  [j] Export JSON  [t] Thinking`

**Implementation:**
```rust
fn render_shortcuts(&self, buf: &mut Buffer, area: Rect, y: u16) {
    let shortcuts = self.available_actions()
        .iter()
        .map(|a| format!("[{}] {}", a.key_char(), a.label()))
        .collect::<Vec<_>>()
        .join("  ");
    buf.set_string(area.x + 2, y, shortcuts, dim_style);
}
```

---

## Phase 4: Streaming Cursor (2h)

### Task 4.1: Add Streaming Cursor to InferBox (1h)
**File:** `src/tui/widgets/task_box/infer.rs`

**Changes:**
1. Replace `█` cursor with `▌` (half-block)
2. Blink cursor every 500ms using animation ticker
3. Only show when `streaming_cursor == true`

### Task 4.2: Add Streaming Cursor to ExecBox (1h)
**File:** `src/tui/widgets/task_box/exec.rs`

**Changes:**
1. Append `▌` to last stdout line when running
2. Blink cursor

---

## Phase 5: HTTP Method Badges (2h)

### Task 5.1: Add HTTP Method Color Badges (2h)
**File:** `src/tui/widgets/task_box/fetch.rs`

**Changes:**
1. Add `http_method_badge(method: &str) -> (char, Color)`:
   - GET → ('🟢', green)
   - POST → ('🔵', blue)
   - PUT → ('🟡', yellow)
   - DELETE → ('🔴', red)
   - PATCH → ('🟠', orange)
   - HEAD → ('⚪', gray)
2. Render badge before method name in header

**Test:**
```rust
#[test]
fn test_http_method_badges() {
    assert_eq!(http_method_badge("GET").0, '🟢');
    assert_eq!(http_method_badge("DELETE").0, '🔴');
}
```

---

## Phase 6: Signal Exit Codes (3h)

### Task 6.1: Signal Detection in ExecBox (2h)
**File:** `src/tui/widgets/task_box/exec.rs`

**Changes:**
1. Add `signal_name(code: i32) -> Option<&'static str>`:
   - 137 → "SIGKILL"
   - 143 → "SIGTERM"
   - 130 → "SIGINT"
   - 139 → "SIGSEGV"
2. Update `exit_display()` to show signal name
3. Format: "EXIT: 143 (SIGTERM)"

### Task 6.2: Add SIGKILL Shortcut (1h)
**File:** `src/tui/widgets/task_box/key_action.rs`

**Changes:**
1. Add `KeyAction::ForceKill` variant
2. Map `KeyCode::Char('K')` → `ForceKill`
3. Update `actions_for_verb("exec")` to include both Kill and ForceKill

---

## Phase 7: Timeout Progress Bar (3h)

### Task 7.1: Add Timeout Progress to InvokeBox (2h)
**File:** `src/tui/widgets/task_box/invoke.rs`

**Changes:**
1. Add `start_time: Option<Instant>` field
2. Add `timeout_secs: u32` field (default 30)
3. Calculate progress: `elapsed / timeout`
4. Render gauge: `[████████░░░░░░░░] 15.3s / 30s`

### Task 7.2: Add Server Status Indicator (1h)
**File:** `src/tui/widgets/task_box/invoke.rs`

**Changes:**
1. Add `server_connected: bool` field
2. Render `🟢` or `🔴` after server name
3. Show hint on disconnect: "Hint: Run `nika mcp start`"

---

## Phase 8: Error Handling Display (4h)

### Task 8.1: NIKA Error Code Display (2h)
**File:** `src/tui/widgets/task_box/invoke.rs`

**Changes:**
1. Parse NIKA error codes from error string
2. Display: "❌ NIKA-100 — Server not connected"
3. Add contextual hints per error code

### Task 8.2: Retry Countdown Display (2h)
**File:** `src/tui/widgets/task_box/fetch.rs`

**Changes:**
1. Add `retry_countdown: Option<Duration>` field
2. Add `retry_attempt: u8` field
3. Render: "🔄 Retry 1/3 in 2s..."
4. Countdown animation using ticker

---

## Phase 9: Thinking Section UI (4h)

### Task 9.1: Add Thinking Section to InferBox (2h)
**File:** `src/tui/widgets/task_box/infer.rs`

**Changes:**
1. Add `thinking: Option<String>` field
2. Add `expanded_thinking: bool` field
3. Render collapsible section with "THINKING" header
4. Show token count: "▼ 456 tokens"
5. Wire [t] key to toggle

### Task 9.2: Add Thinking Section to AgentBox (2h)
**File:** `src/tui/widgets/task_box/agent.rs`

**Changes:**
1. Add `thinking: Option<String>` field
2. Add `expanded_thinking: bool` field
3. Aggregate thinking from all turns
4. Collapsible UI with [t] toggle

---

## Phase 10: AgentBox Compact Mode (4h)

### Task 10.1: Implement Compact Mode (2h)
**File:** `src/tui/widgets/task_box/agent.rs`

**Changes:**
1. Add render_compact() method (10 lines max)
2. Show: Turn progress bar, nested task icons, streaming text
3. Nested task icons: "✅✅✅⣾❌" (state icons in sequence)

### Task 10.2: Implement Turn Progress Bar (2h)
**File:** `src/tui/widgets/task_box/agent.rs`

**Changes:**
1. Add gauge widget for turn progress
2. Format: `[████████████░░░░░░░░] 60% (turn 3 of 5)`
3. Use `ratatui::widgets::Gauge`

---

## Phase 11: DecryptEffect Animation (4h)

### Task 11.1: Create DecryptEffect (2h)
**File:** `src/tui/widgets/animation.rs`

**Changes:**
1. Add `DecryptEffect` struct:
   - `reveal_progress: f32` (0.0 → 1.0)
   - `render(text: &str, width: usize) -> String`
2. Characters reveal left-to-right
3. Unrevealed chars shown as `░`

### Task 11.2: Wire to Queued State (2h)
**Files:** All 5 box files

**Changes:**
1. Render DecryptEffect in QUEUED state
2. Animate on tick

---

## Phase 12: GlitchShake Animation (4h)

### Task 12.1: Create GlitchShake Effect (2h)
**File:** `src/tui/widgets/animation.rs`

**Changes:**
```rust
pub struct GlitchShake {
    start: Instant,
    duration: Duration,
    intensity: f32,
}

impl GlitchShake {
    pub fn offset(&self, frame: u8) -> (i16, i16) {
        // Decay from intensity to 0 over duration
        // Random jitter based on frame
    }
}
```

### Task 12.2: Wire to Failed State (2h)
**Files:** All 5 box files

**Changes:**
1. Trigger GlitchShake on transition to Failed state
2. Apply offset to render position
3. Duration: 500ms, then return to normal

---

## Verification Checklist

- [ ] All 5 boxes have TokenVelocity wired (InferBox, AgentBox)
- [ ] RenderMode affects all 5 boxes (Compact/Expanded/Full)
- [ ] All KeyAction handlers wired ([c] [j] [t] [k] [K] [r])
- [ ] Cost display shows 💰 $X.XX
- [ ] Shortcuts footer visible in SUCCESS state
- [ ] Streaming cursor ▌ visible during RUNNING
- [ ] HTTP method badges colored correctly
- [ ] Signal exit codes show SIGTERM/SIGKILL names
- [ ] Timeout progress bar in InvokeBox
- [ ] Server status 🟢/🔴 indicator
- [ ] NIKA error codes displayed with hints
- [ ] Retry countdown animation
- [ ] Thinking section collapsible with [t]
- [ ] AgentBox COMPACT mode works
- [ ] Turn progress bar in AgentBox
- [ ] DecryptEffect on QUEUED state
- [ ] GlitchShake on FAILED state
- [ ] All tests pass (TDD)
- [ ] Zero clippy warnings

---

## Estimated Effort

| Phase | Hours | Tasks |
|-------|-------|-------|
| 1. Wire Existing | 4h | TokenVelocity, RenderMode, KeyAction |
| 2. Cost Display | 2h | InferBox, AgentBox pricing |
| 3. Shortcuts Footer | 2h | All 5 boxes |
| 4. Streaming Cursor | 2h | InferBox, ExecBox |
| 5. HTTP Badges | 2h | FetchBox |
| 6. Signal Codes | 3h | ExecBox + SIGKILL shortcut |
| 7. Timeout Progress | 3h | InvokeBox + server status |
| 8. Error Display | 4h | NIKA codes + retry countdown |
| 9. Thinking UI | 4h | InferBox, AgentBox |
| 10. AgentBox Compact | 4h | Compact mode + turn bar |
| 11. DecryptEffect | 4h | Animation + wiring |
| 12. GlitchShake | 4h | Animation + wiring |
| **TOTAL** | **38h** | 24 tasks |

---

## Dependencies

- `arboard` crate for clipboard (Copy action)
- `ratatui::widgets::Gauge` for progress bars
- No other external dependencies needed
