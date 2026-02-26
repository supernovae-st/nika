# TaskBox Visual Enhancement v0.11 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Enhance all 5 TaskBox widgets with visual effects (BorderPulse, RenderMode), interactivity (keyboard shortcuts), and real-time metrics (token velocity sparkline).

**Architecture:** The existing TaskBox system has solid foundations (BoxState, VerbColor, BRAILLE_SPINNER) but lacks visual polish. We add: (1) BorderPulse effect via AnimationTicker integration, (2) RenderMode enum for detail levels, (3) KeyAction enum for shortcuts, (4) TokenVelocity struct for sparkline data.

**Tech Stack:** ratatui (widgets), parking_lot (state sync), existing AnimationTicker from `widgets/animation.rs`

**Prerequisite:** v0.10.4 ChatDagPanel complete
**Blocks:** v0.12 TUI Polish
**Tasks:** 8 | **Tests:** 40+ | **Effort:** ~6h

---

## Overview

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  TASKBOX ENHANCEMENT SCOPE                                                    ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  1. BorderPulse Effect                                                        ║
║     └── Apply pulse_intensity() to border color in Running state              ║
║                                                                               ║
║  2. RenderMode Enum                                                           ║
║     └── Compact (1 line) | Expanded (current) | Full (all details)           ║
║                                                                               ║
║  3. Keyboard Shortcuts                                                        ║
║     └── KeyAction enum + handle_key() for each box type                       ║
║                                                                               ║
║  4. Token Velocity Sparkline                                                  ║
║     └── Real-time tokens/sec chart for InferBox and AgentBox                 ║
║                                                                               ║
║  5. Subagent Visual                                                           ║
║     └── 🐤 icon and distinct style for spawned child agents                   ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Task 1: Add RenderMode Enum

**Files:**
- Create: `src/tui/widgets/task_box/render_mode.rs`
- Modify: `src/tui/widgets/task_box/mod.rs`

**Step 1: Write the failing test**

```rust
// src/tui/widgets/task_box/render_mode.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_mode_default() {
        let mode = RenderMode::default();
        assert_eq!(mode, RenderMode::Expanded);
    }

    #[test]
    fn test_render_mode_heights() {
        assert_eq!(RenderMode::Compact.base_height(), 1);
        assert_eq!(RenderMode::Expanded.base_height(), 5);
        assert_eq!(RenderMode::Full.base_height(), 10);
    }

    #[test]
    fn test_render_mode_cycle() {
        let mode = RenderMode::Compact;
        assert_eq!(mode.cycle(), RenderMode::Expanded);
        assert_eq!(RenderMode::Expanded.cycle(), RenderMode::Full);
        assert_eq!(RenderMode::Full.cycle(), RenderMode::Compact);
    }

    #[test]
    fn test_render_mode_shows_section() {
        // Compact shows nothing extra
        assert!(!RenderMode::Compact.shows_params());
        assert!(!RenderMode::Compact.shows_result());
        assert!(!RenderMode::Compact.shows_metrics());

        // Expanded shows params and result (collapsed)
        assert!(RenderMode::Expanded.shows_params());
        assert!(RenderMode::Expanded.shows_result());
        assert!(!RenderMode::Expanded.shows_metrics());

        // Full shows everything
        assert!(RenderMode::Full.shows_params());
        assert!(RenderMode::Full.shows_result());
        assert!(RenderMode::Full.shows_metrics());
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test render_mode --lib -- --nocapture`
Expected: FAIL with "cannot find type `RenderMode`"

**Step 3: Write minimal implementation**

```rust
// src/tui/widgets/task_box/render_mode.rs
//! Render Mode for TaskBox Widgets
//!
//! Controls detail level: Compact (1-line), Expanded (default), Full (all).

/// Render mode controlling TaskBox detail level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderMode {
    /// Single line summary
    Compact,
    /// Default view with collapsed sections
    #[default]
    Expanded,
    /// All sections expanded with full metrics
    Full,
}

impl RenderMode {
    /// Base height for this mode (without content)
    pub const fn base_height(&self) -> u16 {
        match self {
            Self::Compact => 1,
            Self::Expanded => 5,
            Self::Full => 10,
        }
    }

    /// Cycle to next mode
    pub const fn cycle(&self) -> Self {
        match self {
            Self::Compact => Self::Expanded,
            Self::Expanded => Self::Full,
            Self::Full => Self::Compact,
        }
    }

    /// Whether to show params section
    pub const fn shows_params(&self) -> bool {
        matches!(self, Self::Expanded | Self::Full)
    }

    /// Whether to show result section
    pub const fn shows_result(&self) -> bool {
        matches!(self, Self::Expanded | Self::Full)
    }

    /// Whether to show detailed metrics
    pub const fn shows_metrics(&self) -> bool {
        matches!(self, Self::Full)
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test render_mode --lib -- --nocapture`
Expected: PASS (4 tests)

**Step 5: Wire into mod.rs**

```rust
// Add to src/tui/widgets/task_box/mod.rs after line 1
mod render_mode;
pub use render_mode::RenderMode;
```

**Step 6: Commit**

```bash
git add src/tui/widgets/task_box/render_mode.rs src/tui/widgets/task_box/mod.rs
git commit -m "$(cat <<'EOF'
feat(tui): add RenderMode enum for TaskBox detail levels

Compact (1-line) | Expanded (default) | Full (all details)

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 2: Add BorderPulse Effect Utility

**Files:**
- Modify: `src/tui/widgets/task_box/state.rs`

**Step 1: Write the failing test**

```rust
// Add to src/tui/widgets/task_box/state.rs tests module

#[test]
fn test_border_color_with_pulse() {
    use ratatui::style::Color;

    let verb_color = Color::Rgb(139, 92, 246); // Violet

    // Running state with pulse_intensity 0.0 → base color
    let running = BoxState::running();
    let base_color = running.border_color(verb_color);
    assert_eq!(base_color, verb_color);

    // With pulse_intensity 0.5 → brightened color
    let pulsed = running.border_color_with_pulse(verb_color, 0.5);
    if let Color::Rgb(r, g, b) = pulsed {
        // Should be brighter than base
        assert!(r >= 139 || g >= 92 || b >= 246);
    }

    // With pulse_intensity 1.0 → maximum brightness
    let max_pulsed = running.border_color_with_pulse(verb_color, 1.0);
    if let Color::Rgb(r, g, b) = max_pulsed {
        // Should be significantly brighter
        assert!(r > 139 || g > 92 || b > 246);
    }
}

#[test]
fn test_border_color_pulse_non_running_unchanged() {
    use ratatui::style::Color;

    let verb_color = Color::Rgb(139, 92, 246);

    // Queued state ignores pulse
    let queued = BoxState::Queued;
    let color = queued.border_color_with_pulse(verb_color, 1.0);
    assert_eq!(color, queued.border_color(verb_color)); // Unchanged

    // Success state ignores pulse
    let success = BoxState::success(1000);
    let color = success.border_color_with_pulse(verb_color, 1.0);
    assert_eq!(color, success.border_color(verb_color)); // Unchanged
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test border_color_with_pulse --lib -- --nocapture`
Expected: FAIL with "method `border_color_with_pulse` not found"

**Step 3: Write minimal implementation**

```rust
// Add to BoxState impl in src/tui/widgets/task_box/state.rs

/// Get border color with pulse effect applied
///
/// Only applies pulse to Running state; other states return normal border_color.
/// Pulse brightens the color by interpolating toward white.
pub fn border_color_with_pulse(&self, verb_color: Color, pulse_intensity: f32) -> Color {
    let base = self.border_color(verb_color);

    // Only pulse when running
    if !self.is_running() {
        return base;
    }

    // Brighten color based on pulse_intensity (0.0-1.0)
    if let Color::Rgb(r, g, b) = base {
        let factor = 1.0 + (pulse_intensity * 0.3); // Max 30% brighter
        let brighten = |c: u8| -> u8 {
            ((c as f32 * factor).min(255.0)) as u8
        };
        Color::Rgb(brighten(r), brighten(g), brighten(b))
    } else {
        base
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test border_color --lib -- --nocapture`
Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/widgets/task_box/state.rs
git commit -m "$(cat <<'EOF'
feat(tui): add border_color_with_pulse() for running animation

Brightens border by up to 30% based on pulse_intensity

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 3: Add KeyAction Enum for Shortcuts

**Files:**
- Create: `src/tui/widgets/task_box/key_action.rs`
- Modify: `src/tui/widgets/task_box/mod.rs`

**Step 1: Write the failing test**

```rust
// src/tui/widgets/task_box/key_action.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyCode;

    #[test]
    fn test_common_actions() {
        assert_eq!(KeyAction::from_key(KeyCode::Char('c')), Some(KeyAction::Copy));
        assert_eq!(KeyAction::from_key(KeyCode::Char('j')), Some(KeyAction::Json));
        assert_eq!(KeyAction::from_key(KeyCode::Enter), Some(KeyAction::Toggle));
        assert_eq!(KeyAction::from_key(KeyCode::Esc), Some(KeyAction::Close));
    }

    #[test]
    fn test_verb_specific_actions() {
        // Exec-specific
        assert_eq!(KeyAction::from_key(KeyCode::Char('k')), Some(KeyAction::Kill));

        // Fetch-specific
        assert_eq!(KeyAction::from_key(KeyCode::Char('r')), Some(KeyAction::Retry));
    }

    #[test]
    fn test_unknown_key() {
        assert_eq!(KeyAction::from_key(KeyCode::Char('z')), None);
        assert_eq!(KeyAction::from_key(KeyCode::F(12)), None);
    }

    #[test]
    fn test_action_label() {
        assert_eq!(KeyAction::Copy.label(), "Copy");
        assert_eq!(KeyAction::Json.label(), "JSON");
        assert_eq!(KeyAction::Kill.label(), "Kill");
    }

    #[test]
    fn test_action_hint() {
        assert_eq!(KeyAction::Copy.hint(), "c");
        assert_eq!(KeyAction::Json.hint(), "j");
        assert_eq!(KeyAction::Toggle.hint(), "↵");
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test key_action --lib -- --nocapture`
Expected: FAIL with "cannot find type `KeyAction`"

**Step 3: Write minimal implementation**

```rust
// src/tui/widgets/task_box/key_action.rs
//! Keyboard Actions for TaskBox Widgets
//!
//! Unified shortcut system across all 5 verb boxes.

use crossterm::event::KeyCode;

/// Keyboard action for TaskBox interaction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    // === Common Actions ===
    /// Copy content to clipboard
    Copy,
    /// Export as JSON
    Json,
    /// Toggle expand/collapse
    Toggle,
    /// Close/deselect
    Close,
    /// Cycle render mode
    Mode,

    // === Exec-specific ===
    /// Kill running process
    Kill,
    /// Re-run command
    Rerun,

    // === Fetch-specific ===
    /// Retry request
    Retry,

    // === Agent-specific ===
    /// Expand/collapse children
    Children,
    /// Show thinking
    Thinking,
}

impl KeyAction {
    /// Parse key code into action
    pub fn from_key(key: KeyCode) -> Option<Self> {
        match key {
            // Common
            KeyCode::Char('c') => Some(Self::Copy),
            KeyCode::Char('j') => Some(Self::Json),
            KeyCode::Enter => Some(Self::Toggle),
            KeyCode::Esc => Some(Self::Close),
            KeyCode::Char('m') => Some(Self::Mode),

            // Exec
            KeyCode::Char('k') => Some(Self::Kill),
            KeyCode::Char('!') => Some(Self::Rerun),

            // Fetch
            KeyCode::Char('r') => Some(Self::Retry),

            // Agent
            KeyCode::Char('e') => Some(Self::Children),
            KeyCode::Char('t') => Some(Self::Thinking),

            _ => None,
        }
    }

    /// Human-readable label
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Copy => "Copy",
            Self::Json => "JSON",
            Self::Toggle => "Toggle",
            Self::Close => "Close",
            Self::Mode => "Mode",
            Self::Kill => "Kill",
            Self::Rerun => "Re-run",
            Self::Retry => "Retry",
            Self::Children => "Children",
            Self::Thinking => "Thinking",
        }
    }

    /// Keyboard hint for display
    pub const fn hint(&self) -> &'static str {
        match self {
            Self::Copy => "c",
            Self::Json => "j",
            Self::Toggle => "↵",
            Self::Close => "esc",
            Self::Mode => "m",
            Self::Kill => "k",
            Self::Rerun => "!",
            Self::Retry => "r",
            Self::Children => "e",
            Self::Thinking => "t",
        }
    }

    /// Get available actions for a verb type
    pub fn actions_for_verb(verb: &str) -> &'static [Self] {
        match verb {
            "infer" => &[Self::Copy, Self::Json, Self::Toggle, Self::Mode],
            "exec" => &[Self::Copy, Self::Kill, Self::Rerun, Self::Toggle, Self::Mode],
            "fetch" => &[Self::Copy, Self::Json, Self::Retry, Self::Toggle, Self::Mode],
            "invoke" => &[Self::Copy, Self::Json, Self::Toggle, Self::Mode],
            "agent" => &[Self::Copy, Self::Json, Self::Children, Self::Thinking, Self::Mode],
            _ => &[Self::Copy, Self::Toggle, Self::Mode],
        }
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test key_action --lib -- --nocapture`
Expected: PASS (5 tests)

**Step 5: Wire into mod.rs**

```rust
// Add to src/tui/widgets/task_box/mod.rs
mod key_action;
pub use key_action::KeyAction;
```

**Step 6: Commit**

```bash
git add src/tui/widgets/task_box/key_action.rs src/tui/widgets/task_box/mod.rs
git commit -m "$(cat <<'EOF'
feat(tui): add KeyAction enum for TaskBox keyboard shortcuts

Common: c=copy, j=json, m=mode, Enter=toggle, Esc=close
Exec: k=kill, !=rerun | Fetch: r=retry | Agent: e=children, t=thinking

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 4: Add TokenVelocity for Sparkline Data

**Files:**
- Create: `src/tui/widgets/task_box/token_velocity.rs`
- Modify: `src/tui/widgets/task_box/mod.rs`

**Step 1: Write the failing test**

```rust
// src/tui/widgets/task_box/token_velocity.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_velocity_new() {
        let vel = TokenVelocity::new(20);
        assert_eq!(vel.capacity(), 20);
        assert_eq!(vel.len(), 0);
        assert!(vel.is_empty());
    }

    #[test]
    fn test_token_velocity_push() {
        let mut vel = TokenVelocity::new(5);
        vel.push(10.0);
        vel.push(20.0);
        vel.push(15.0);

        assert_eq!(vel.len(), 3);
        assert_eq!(vel.samples(), &[10.0, 20.0, 15.0]);
    }

    #[test]
    fn test_token_velocity_overflow() {
        let mut vel = TokenVelocity::new(3);
        vel.push(1.0);
        vel.push(2.0);
        vel.push(3.0);
        vel.push(4.0); // Overwrites oldest

        assert_eq!(vel.len(), 3);
        assert_eq!(vel.samples(), &[2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_token_velocity_average() {
        let mut vel = TokenVelocity::new(10);
        vel.push(10.0);
        vel.push(20.0);
        vel.push(30.0);

        assert!((vel.average() - 20.0).abs() < 0.01);
    }

    #[test]
    fn test_token_velocity_peak() {
        let mut vel = TokenVelocity::new(10);
        vel.push(10.0);
        vel.push(50.0);
        vel.push(30.0);

        assert!((vel.peak() - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_token_velocity_sparkline_data() {
        let mut vel = TokenVelocity::new(5);
        for i in 1..=5 {
            vel.push(i as f32 * 10.0);
        }

        let data = vel.sparkline_data(8);
        assert_eq!(data.len(), 5); // Only 5 samples
        assert!(data.iter().all(|&v| v <= 8)); // Normalized to max 8
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test token_velocity --lib -- --nocapture`
Expected: FAIL with "cannot find type `TokenVelocity`"

**Step 3: Write minimal implementation**

```rust
// src/tui/widgets/task_box/token_velocity.rs
//! Token Velocity Tracking for Sparkline Display
//!
//! Ring buffer of tokens/sec samples for real-time visualization.

use std::collections::VecDeque;

/// Token velocity tracker with fixed-size ring buffer
#[derive(Debug, Clone)]
pub struct TokenVelocity {
    /// Samples (tokens per second)
    samples: VecDeque<f32>,
    /// Maximum capacity
    capacity: usize,
}

impl TokenVelocity {
    /// Create new velocity tracker with given capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Push a new sample (tokens/sec)
    pub fn push(&mut self, tokens_per_sec: f32) {
        if self.samples.len() >= self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(tokens_per_sec);
    }

    /// Get all samples as slice
    pub fn samples(&self) -> Vec<f32> {
        self.samples.iter().copied().collect()
    }

    /// Number of samples
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Average velocity
    pub fn average(&self) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        self.samples.iter().sum::<f32>() / self.samples.len() as f32
    }

    /// Peak velocity
    pub fn peak(&self) -> f32 {
        self.samples.iter().copied().fold(0.0, f32::max)
    }

    /// Get data normalized for sparkline widget (0..max_height)
    pub fn sparkline_data(&self, max_height: u64) -> Vec<u64> {
        if self.samples.is_empty() {
            return Vec::new();
        }

        let peak = self.peak().max(1.0); // Avoid division by zero
        self.samples
            .iter()
            .map(|&v| ((v / peak) * max_height as f32) as u64)
            .collect()
    }

    /// Clear all samples
    pub fn clear(&mut self) {
        self.samples.clear();
    }
}

impl Default for TokenVelocity {
    fn default() -> Self {
        Self::new(30) // 30 samples = ~0.5s at 60fps
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test token_velocity --lib -- --nocapture`
Expected: PASS (6 tests)

**Step 5: Wire into mod.rs**

```rust
// Add to src/tui/widgets/task_box/mod.rs
mod token_velocity;
pub use token_velocity::TokenVelocity;
```

**Step 6: Commit**

```bash
git add src/tui/widgets/task_box/token_velocity.rs src/tui/widgets/task_box/mod.rs
git commit -m "$(cat <<'EOF'
feat(tui): add TokenVelocity ring buffer for sparkline display

Fixed-size sample buffer with average, peak, and normalized sparkline data

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 5: Integrate BorderPulse into InferBox Render

**Files:**
- Modify: `src/tui/widgets/task_box/infer.rs`

**Step 1: Write the failing test**

```rust
// Add to src/tui/widgets/task_box/infer.rs tests module

#[test]
fn test_infer_box_with_pulse() {
    let box_ = InferBox::new("claude-sonnet-4-20250514", "Generate text")
        .with_state(BoxState::running())
        .with_pulse_intensity(0.5);

    assert!((box_.pulse_intensity() - 0.5).abs() < 0.01);
}

#[test]
fn test_infer_box_pulse_default_zero() {
    let box_ = InferBox::new("claude-sonnet-4-20250514", "Generate text");
    assert!((box_.pulse_intensity() - 0.0).abs() < 0.01);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test infer_box_with_pulse --lib -- --nocapture`
Expected: FAIL with "method `with_pulse_intensity` not found"

**Step 3: Write minimal implementation**

```rust
// Add field to InferBox struct (around line 17)
    /// Pulse intensity for animation (0.0-1.0)
    pub pulse_intensity: f32,

// Add to new() (around line 30)
    pulse_intensity: 0.0,

// Add builder method (after with_streaming_cursor)
    /// Set pulse intensity for border animation
    pub fn with_pulse_intensity(mut self, intensity: f32) -> Self {
        self.pulse_intensity = intensity.clamp(0.0, 1.0);
        self
    }

    /// Get current pulse intensity
    pub fn pulse_intensity(&self) -> f32 {
        self.pulse_intensity
    }

// Modify render() to use pulse - change line ~100:
// FROM:
    let border_color = self.state.border_color(verb.rgb());
// TO:
    let border_color = self.state.border_color_with_pulse(verb.rgb(), self.pulse_intensity);
```

**Step 4: Run test to verify it passes**

Run: `cargo test infer_box --lib -- --nocapture`
Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/widgets/task_box/infer.rs
git commit -m "$(cat <<'EOF'
feat(tui): integrate BorderPulse into InferBox render

pulse_intensity field + with_pulse_intensity() builder + render integration

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 6: Integrate BorderPulse into Remaining Boxes

**Files:**
- Modify: `src/tui/widgets/task_box/exec.rs`
- Modify: `src/tui/widgets/task_box/fetch.rs`
- Modify: `src/tui/widgets/task_box/invoke.rs`
- Modify: `src/tui/widgets/task_box/agent.rs`

**Step 1: Write the failing tests**

Add to each file's tests module:

```rust
// exec.rs tests
#[test]
fn test_exec_box_with_pulse() {
    let box_ = ExecBox::new("echo hello")
        .with_state(BoxState::running())
        .with_pulse_intensity(0.7);
    assert!((box_.pulse_intensity() - 0.7).abs() < 0.01);
}

// fetch.rs tests
#[test]
fn test_fetch_box_with_pulse() {
    let box_ = FetchBox::new("GET", "https://api.example.com")
        .with_state(BoxState::running())
        .with_pulse_intensity(0.3);
    assert!((box_.pulse_intensity() - 0.3).abs() < 0.01);
}

// invoke.rs tests
#[test]
fn test_invoke_box_with_pulse() {
    let box_ = InvokeBox::new("novanet_describe", "novanet")
        .with_state(BoxState::running())
        .with_pulse_intensity(0.9);
    assert!((box_.pulse_intensity() - 0.9).abs() < 0.01);
}

// agent.rs tests
#[test]
fn test_agent_box_with_pulse() {
    let box_ = AgentBox::new("task-1", "Research competitors")
        .with_state(BoxState::running())
        .with_pulse_intensity(0.6);
    assert!((box_.pulse_intensity() - 0.6).abs() < 0.01);
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test _box_with_pulse --lib -- --nocapture`
Expected: FAIL with "method `with_pulse_intensity` not found" for each

**Step 3: Apply same pattern to each box**

For each of `exec.rs`, `fetch.rs`, `invoke.rs`, `agent.rs`:

1. Add field: `pub pulse_intensity: f32,`
2. Initialize in `new()`: `pulse_intensity: 0.0,`
3. Add builder:
```rust
pub fn with_pulse_intensity(mut self, intensity: f32) -> Self {
    self.pulse_intensity = intensity.clamp(0.0, 1.0);
    self
}

pub fn pulse_intensity(&self) -> f32 {
    self.pulse_intensity
}
```
4. Update render():
```rust
// FROM:
let border_color = self.state.border_color(verb.rgb());
// TO:
let border_color = self.state.border_color_with_pulse(verb.rgb(), self.pulse_intensity);
```

**Step 4: Run tests to verify they pass**

Run: `cargo test _box_with_pulse --lib -- --nocapture`
Expected: PASS (4 tests)

**Step 5: Commit**

```bash
git add src/tui/widgets/task_box/exec.rs src/tui/widgets/task_box/fetch.rs \
        src/tui/widgets/task_box/invoke.rs src/tui/widgets/task_box/agent.rs
git commit -m "$(cat <<'EOF'
feat(tui): integrate BorderPulse into all 5 TaskBox types

ExecBox, FetchBox, InvokeBox, AgentBox now support pulse_intensity

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 7: Add Subagent Visual to AgentBox

**Files:**
- Modify: `src/tui/widgets/task_box/agent.rs`

**Step 1: Write the failing test**

```rust
// Add to agent.rs tests module

#[test]
fn test_agent_box_is_subagent() {
    let parent = AgentBox::new("task-1", "Research");
    assert!(!parent.is_subagent());

    let child = AgentBox::new("task-1.1", "Sub-task").as_subagent();
    assert!(child.is_subagent());
}

#[test]
fn test_agent_box_subagent_icon() {
    let parent = AgentBox::new("task-1", "Research");
    assert!(parent.icon().contains("🐔"));

    let child = AgentBox::new("task-1.1", "Sub-task").as_subagent();
    assert!(child.icon().contains("🐤"));
}

#[test]
fn test_agent_box_with_depth() {
    let box_ = AgentBox::new("task-1", "Research").with_depth(2);
    assert_eq!(box_.depth(), 2);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test agent_box_is_subagent --lib -- --nocapture`
Expected: FAIL with "method `as_subagent` not found"

**Step 3: Write minimal implementation**

```rust
// Add fields to AgentBox struct (after expanded_children)
    /// Whether this is a spawned subagent (not root)
    pub is_subagent: bool,
    /// Nesting depth (0 = root)
    pub depth: u32,

// Add to new()
    is_subagent: false,
    depth: 0,

// Add methods
    /// Mark as subagent (spawned by parent)
    pub fn as_subagent(mut self) -> Self {
        self.is_subagent = true;
        self
    }

    /// Check if this is a subagent
    pub fn is_subagent(&self) -> bool {
        self.is_subagent
    }

    /// Set nesting depth
    pub fn with_depth(mut self, depth: u32) -> Self {
        self.depth = depth;
        self.is_subagent = depth > 0;
        self
    }

    /// Get nesting depth
    pub fn depth(&self) -> u32 {
        self.depth
    }

    /// Get icon based on parent/child status
    pub fn icon(&self) -> &'static str {
        if self.is_subagent {
            "🐤 SUBAGENT"
        } else {
            "🐔 AGENT"
        }
    }

// Modify render() title line to use icon():
// FROM:
    let title = format!(
        "╭─ {} {} {} {} ─╮",
        verb.icon_label(),
// TO:
    let title = format!(
        "╭─ {} {} {} {} ─╮",
        self.icon(),
```

**Step 4: Run test to verify it passes**

Run: `cargo test agent_box --lib -- --nocapture`
Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/widgets/task_box/agent.rs
git commit -m "$(cat <<'EOF'
feat(tui): add subagent visual (🐤) for spawned child agents

is_subagent flag, depth tracking, distinct icon for nested agents

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 8: Add handle_key() to TaskBox Enum

**Files:**
- Modify: `src/tui/widgets/task_box/mod.rs`

**Step 1: Write the failing test**

```rust
// Add to mod.rs tests module

#[test]
fn test_taskbox_handle_key_toggle() {
    use crossterm::event::KeyCode;

    let mut box_ = TaskBox::Infer(InferBox::new("model", "prompt"));

    // Initially not expanded
    if let TaskBox::Infer(ref inner) = box_ {
        assert!(!inner.expanded_response);
    }

    // Toggle with Enter
    let action = box_.handle_key(KeyCode::Enter);
    assert_eq!(action, Some(KeyAction::Toggle));

    // Verify state changed
    if let TaskBox::Infer(ref inner) = box_ {
        assert!(inner.expanded_response);
    }
}

#[test]
fn test_taskbox_handle_key_mode() {
    use crossterm::event::KeyCode;

    let mut box_ = TaskBox::Exec(ExecBox::new("echo hello"));

    // Get initial mode
    let initial_mode = box_.render_mode();
    assert_eq!(initial_mode, RenderMode::Expanded);

    // Cycle mode with 'm'
    let action = box_.handle_key(KeyCode::Char('m'));
    assert_eq!(action, Some(KeyAction::Mode));

    // Verify mode changed
    assert_eq!(box_.render_mode(), RenderMode::Full);
}

#[test]
fn test_taskbox_available_actions() {
    let infer = TaskBox::Infer(InferBox::new("model", "prompt"));
    let actions = infer.available_actions();
    assert!(actions.contains(&KeyAction::Copy));
    assert!(actions.contains(&KeyAction::Json));
    assert!(!actions.contains(&KeyAction::Kill)); // Exec-only

    let exec = TaskBox::Exec(ExecBox::new("echo hello"));
    let actions = exec.available_actions();
    assert!(actions.contains(&KeyAction::Kill));
    assert!(actions.contains(&KeyAction::Rerun));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test taskbox_handle_key --lib -- --nocapture`
Expected: FAIL with "method `handle_key` not found"

**Step 3: Write minimal implementation**

Add to TaskBox impl in mod.rs:

```rust
use crossterm::event::KeyCode;

impl TaskBox {
    /// Handle keyboard input, return action if recognized
    pub fn handle_key(&mut self, key: KeyCode) -> Option<KeyAction> {
        let action = KeyAction::from_key(key)?;

        match action {
            KeyAction::Toggle => {
                match self {
                    TaskBox::Infer(b) => b.expanded_response = !b.expanded_response,
                    TaskBox::Exec(b) => b.expanded_stdout = !b.expanded_stdout,
                    TaskBox::Fetch(b) => b.expanded_body = !b.expanded_body,
                    TaskBox::Invoke(b) => b.toggle_result(),
                    TaskBox::Agent(b) => b.toggle_response(),
                }
                Some(action)
            }
            KeyAction::Mode => {
                self.cycle_render_mode();
                Some(action)
            }
            KeyAction::Children => {
                if let TaskBox::Agent(b) = self {
                    b.toggle_children();
                    Some(action)
                } else {
                    None
                }
            }
            KeyAction::Kill => {
                if matches!(self, TaskBox::Exec(_)) {
                    Some(action) // Caller handles actual kill
                } else {
                    None
                }
            }
            KeyAction::Retry => {
                if matches!(self, TaskBox::Fetch(_)) {
                    Some(action) // Caller handles actual retry
                } else {
                    None
                }
            }
            // Copy, Json, etc. return action for caller to handle
            _ => Some(action),
        }
    }

    /// Get available actions for this box type
    pub fn available_actions(&self) -> &'static [KeyAction] {
        match self {
            TaskBox::Infer(_) => KeyAction::actions_for_verb("infer"),
            TaskBox::Exec(_) => KeyAction::actions_for_verb("exec"),
            TaskBox::Fetch(_) => KeyAction::actions_for_verb("fetch"),
            TaskBox::Invoke(_) => KeyAction::actions_for_verb("invoke"),
            TaskBox::Agent(_) => KeyAction::actions_for_verb("agent"),
        }
    }

    /// Get current render mode
    pub fn render_mode(&self) -> RenderMode {
        match self {
            TaskBox::Infer(b) => b.render_mode,
            TaskBox::Exec(b) => b.render_mode,
            TaskBox::Fetch(b) => b.render_mode,
            TaskBox::Invoke(b) => b.render_mode,
            TaskBox::Agent(b) => b.render_mode,
        }
    }

    /// Cycle to next render mode
    pub fn cycle_render_mode(&mut self) {
        match self {
            TaskBox::Infer(b) => b.render_mode = b.render_mode.cycle(),
            TaskBox::Exec(b) => b.render_mode = b.render_mode.cycle(),
            TaskBox::Fetch(b) => b.render_mode = b.render_mode.cycle(),
            TaskBox::Invoke(b) => b.render_mode = b.render_mode.cycle(),
            TaskBox::Agent(b) => b.render_mode = b.render_mode.cycle(),
        }
    }
}
```

**Note:** This requires adding `render_mode: RenderMode` field to each box struct. Follow same pattern as pulse_intensity:

For each box (infer.rs, exec.rs, fetch.rs, invoke.rs, agent.rs):
1. Add field: `pub render_mode: RenderMode,`
2. Initialize in new(): `render_mode: RenderMode::default(),`
3. Add use: `use super::RenderMode;`

**Step 4: Run test to verify it passes**

Run: `cargo test taskbox --lib -- --nocapture`
Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/widgets/task_box/mod.rs \
        src/tui/widgets/task_box/infer.rs \
        src/tui/widgets/task_box/exec.rs \
        src/tui/widgets/task_box/fetch.rs \
        src/tui/widgets/task_box/invoke.rs \
        src/tui/widgets/task_box/agent.rs
git commit -m "$(cat <<'EOF'
feat(tui): add handle_key() and render_mode to TaskBox

Unified keyboard handling across all 5 verb boxes
RenderMode (Compact/Expanded/Full) per box with 'm' to cycle

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Comprehensive Testing Protocol

### Unit Tests (40+ tests)

| Category | Test Names | Count |
|----------|-----------|-------|
| **RenderMode** | `test_render_mode_default`, `test_render_mode_heights`, `test_render_mode_cycle`, `test_render_mode_shows_section` | 4 |
| **BorderPulse** | `test_border_color_with_pulse`, `test_border_color_pulse_non_running_unchanged` | 2 |
| **KeyAction** | `test_common_actions`, `test_verb_specific_actions`, `test_unknown_key`, `test_action_label`, `test_action_hint` | 5 |
| **TokenVelocity** | `test_token_velocity_new`, `test_token_velocity_push`, `test_token_velocity_overflow`, `test_token_velocity_average`, `test_token_velocity_peak`, `test_token_velocity_sparkline_data` | 6 |
| **InferBox Pulse** | `test_infer_box_with_pulse`, `test_infer_box_pulse_default_zero` | 2 |
| **Other Boxes Pulse** | `test_exec_box_with_pulse`, `test_fetch_box_with_pulse`, `test_invoke_box_with_pulse`, `test_agent_box_with_pulse` | 4 |
| **Subagent** | `test_agent_box_is_subagent`, `test_agent_box_subagent_icon`, `test_agent_box_with_depth` | 3 |
| **TaskBox Handle** | `test_taskbox_handle_key_toggle`, `test_taskbox_handle_key_mode`, `test_taskbox_available_actions` | 3 |

**Total new tests:** 29 minimum (existing tests remain)

---

## Success Criteria

- [ ] 29+ new tests pass
- [ ] `cargo test task_box --lib` passes
- [ ] BorderPulse visually animates running state borders
- [ ] RenderMode cycles with 'm' key
- [ ] KeyAction routes shortcuts correctly
- [ ] TokenVelocity provides sparkline data
- [ ] Subagent shows 🐤 instead of 🐔
- [ ] Zero clippy warnings
- [ ] `cargo clippy -- -D warnings` clean

---

## File Summary

| File | Action | Lines Added |
|------|--------|-------------|
| `task_box/render_mode.rs` | Create | ~60 |
| `task_box/key_action.rs` | Create | ~120 |
| `task_box/token_velocity.rs` | Create | ~80 |
| `task_box/state.rs` | Modify | ~20 |
| `task_box/infer.rs` | Modify | ~15 |
| `task_box/exec.rs` | Modify | ~15 |
| `task_box/fetch.rs` | Modify | ~15 |
| `task_box/invoke.rs` | Modify | ~15 |
| `task_box/agent.rs` | Modify | ~40 |
| `task_box/mod.rs` | Modify | ~60 |

**Total:** ~440 lines of new code + tests
