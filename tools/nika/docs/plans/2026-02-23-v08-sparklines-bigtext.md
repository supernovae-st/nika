# v0.8 Plan: Animated Sparklines + BigText Widget

**Date:** 2026-02-23
**Target Version:** v0.8.0
**Status:** ✅ Complete

---

## Overview

Complete the final 1% of TUI cosmetic features:
1. **Animated Sparklines** - Add visual animation to latency/metrics displays
2. **BigText Widget** - Large ASCII art text for titles and status messages

---

## Part 1: Animated Sparklines

### Current State

`sparkline.rs` has 3 static widgets:
- `LatencySparkline` - Threshold-based coloring (green/amber/red)
- `MiniSparkline` - Compact with inline label
- `BorderedSparkline` - With border and title

Used in:
- `progress.rs:350` - Latency metrics display
- `context.rs:306` - Context panel metrics

### Animation Features

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  ANIMATION TYPES                                                                │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  1. PULSE ANIMATION - Latest bar glows/pulses                                   │
│     Frame 0: ▁▂▃▄▅▆▇█  (normal)                                                 │
│     Frame 5: ▁▂▃▄▅▆▇▓  (highlight latest)                                       │
│     Frame 10: ▁▂▃▄▅▆▇█  (normal)                                                │
│                                                                                 │
│  2. FLOW ANIMATION - Data appears to "flow" right to left                       │
│     Visual effect: bars shift slightly to simulate motion                       │
│                                                                                 │
│  3. WAVE ANIMATION - Subtle sine wave effect on bar heights                     │
│     Bars oscillate ±1 level based on position + frame                           │
│                                                                                 │
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Implementation

```rust
// New: AnimatedLatencySparkline
pub struct AnimatedLatencySparkline<'a> {
    data: &'a [u64],
    frame: u8,                    // Current animation frame (0-59)
    animation: SparklineAnimation, // Animation type
    warn_threshold: u64,
    error_threshold: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum SparklineAnimation {
    #[default]
    None,       // Static (current behavior)
    Pulse,      // Latest bar pulses
    Flow,       // Data flows right to left
    Wave,       // Sine wave effect
}

impl AnimatedLatencySparkline<'a> {
    pub fn new(data: &'a [u64]) -> Self { ... }
    pub fn frame(mut self, frame: u8) -> Self { ... }
    pub fn animation(mut self, anim: SparklineAnimation) -> Self { ... }
}
```

### Files to Modify

| File | Change |
|------|--------|
| `widgets/sparkline.rs` | Add `AnimatedLatencySparkline`, `SparklineAnimation` enum |
| `widgets/mod.rs` | Export new types |
| `panels/progress.rs` | Use animated sparkline with `state.frame` |
| `panels/context.rs` | Use animated sparkline with `state.frame` |

---

## Part 2: BigText Widget

### Purpose

Large ASCII art text for:
- Welcome screen "NIKA"
- Status banners (SUCCESS, FAILED, RUNNING)
- Workflow titles (optional)

### ASCII Font Design

Using 3-line height font (compact but readable):

```
 █▀█   █▀▄   █▀▀   █▀▄   █▀▀   █▀▀   █▀▀   █ █   █     █   █▄▀   █
 █▀█   █▀▄   █     █ █   ██▀   █▀    █ █   █▀█   █     █   █ █   █
 █ █   █▀    ▀▀▀   ▀▀    ▀▀▀   █     ▀▀▀   █ █   █   ▀▄█   █ █   ▀▀▀

 █▄ ▄█   █▄ █   █▀█   █▀█   █▀█   █▀█   █▀▀   ▀█▀   █ █   █ █   █ █ █
 █ █ █   █ ██   █ █   █▀▀   █ █   █▀▄   ▀▀█    █    █ █   █▄█   █▄▀▄█
 █   █   █  █   ▀▀▀   █     ▀▀█   █ █   ▀▀▀    █    ▀▀▀    ▀    ▀   ▀

 ▀▄▀   █ █   ▀█▀   ▄▀█   ▀█   ▀██   ██▀   ▄ █   █▀   ▄▀▀   ▀▀█   ▄▀█
  █    ▀█▀   █▄   ██▄   █▀    ██   ▀▄█   ▀▀█   ██▄  ▀▄█   ▀█▀   ▀▄█
 █ █    █    █▄▄  █ █   ▀▀▀    █   ▀▀     █     ▀    ▀    ▀▀▀   ▀▀▀
```

### Implementation

```rust
// widgets/big_text.rs

/// Supported characters for BigText
/// A-Z, 0-9, space, and basic punctuation
pub struct BigText<'a> {
    text: &'a str,
    style: Style,
    alignment: Alignment,
}

impl<'a> BigText<'a> {
    pub fn new(text: &'a str) -> Self { ... }
    pub fn style(mut self, style: Style) -> Self { ... }
    pub fn alignment(mut self, alignment: Alignment) -> Self { ... }

    /// Get character width (most chars are 4 columns, space is 2)
    fn char_width(c: char) -> u16 { ... }

    /// Get the 3-line representation of a character
    fn char_lines(c: char) -> [&'static str; 3] { ... }
}

impl Widget for BigText<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Render each character side by side, 3 lines tall
    }
}
```

### Files to Create/Modify

| File | Change |
|------|--------|
| `widgets/big_text.rs` | **NEW** - BigText widget implementation |
| `widgets/mod.rs` | Add `mod big_text; pub use big_text::BigText;` |
| `views/home.rs` | Use BigText for "NIKA" title (optional) |

---

## Testing Plan

### Sparkline Animation Tests

```rust
#[test]
fn test_animated_sparkline_pulse_frame_cycles() {
    let data = vec![10, 20, 30, 40, 50];
    let spark = AnimatedLatencySparkline::new(&data)
        .frame(0)
        .animation(SparklineAnimation::Pulse);
    // Verify frame affects rendering
}

#[test]
fn test_sparkline_animation_none_matches_static() {
    // AnimatedLatencySparkline with None animation should match LatencySparkline
}
```

### BigText Tests

```rust
#[test]
fn test_big_text_nika_renders() {
    let text = BigText::new("NIKA");
    // Verify 3 lines rendered
}

#[test]
fn test_big_text_width_calculation() {
    assert_eq!(BigText::text_width("OK"), 8); // O=4, K=4
    assert_eq!(BigText::text_width("A B"), 10); // A=4, space=2, B=4
}

#[test]
fn test_big_text_unsupported_char_renders_space() {
    let text = BigText::new("ABC@DEF");
    // @ should render as space
}
```

---

## Execution Order

1. **Phase 1: Animated Sparklines** (~30 min)
   - Add `SparklineAnimation` enum
   - Add `AnimatedLatencySparkline` struct
   - Implement pulse animation
   - Update progress.rs and context.rs
   - Add tests

2. **Phase 2: BigText Widget** (~45 min)
   - Create big_text.rs
   - Define ASCII font for A-Z, 0-9
   - Implement Widget trait
   - Add tests
   - Optional: Use in home.rs

3. **Phase 3: Verification** (~15 min)
   - Run all tests
   - Manual TUI verification
   - Update CHANGELOG.md

---

## Success Criteria

- [ ] `AnimatedLatencySparkline` renders with pulse animation
- [ ] Animation is smooth (uses `state.frame` from tick)
- [ ] `BigText` renders "NIKA", "SUCCESS", "FAILED" correctly
- [ ] All existing tests pass
- [ ] New tests for both features
- [ ] No clippy warnings
- [ ] Performance: <1ms render time for both widgets

---

## Notes

- Keep backward compatibility: existing `LatencySparkline` unchanged
- Animation is opt-in via `.animation()` builder method
- BigText uses block characters (█▀▄) for sharp rendering
- Consider performance: no allocations in render path
