# VerbColor Consolidation + Theme Migration Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Consolidate two VerbColor enums into one canonical source in theme.rs, then migrate widgets to use Theme.

**Architecture:** theme.rs becomes the single source of truth for VerbColor. task_box/colors.rs re-exports from theme and keeps only domain-specific utilities (http, exit codes).

**Tech Stack:** Rust, ratatui, serde

---

## Phase 1: VerbColor Consolidation (4 tasks)

### Task 1.1: Extend theme.rs VerbColor with Spawn variant

**Files:**
- Modify: `src/tui/theme.rs:274-324`

**Step 1: Write the failing test**

Add test in theme.rs tests module:

```rust
#[test]
fn test_verbcolor_spawn_variant() {
    let color = VerbColor::Spawn;
    assert_eq!(color.rgb(), Color::Rgb(253, 164, 175)); // Rose 300
    assert_eq!(color.glow(), Color::Rgb(254, 205, 211)); // Rose 200
}

#[test]
fn test_verbcolor_has_all_methods() {
    // Verify all 6 variants have all methods
    for verb in [VerbColor::Infer, VerbColor::Exec, VerbColor::Fetch,
                 VerbColor::Invoke, VerbColor::Agent, VerbColor::Spawn] {
        let _ = verb.rgb();
        let _ = verb.glow();
        let _ = verb.muted();
        let _ = verb.subtle();
        let _ = verb.hex();
        let _ = verb.icon();
        let _ = verb.label();
        let _ = verb.border_rgb();
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --lib theme::tests::test_verbcolor_spawn`
Expected: FAIL - `Spawn` variant doesn't exist

**Step 3: Add Spawn variant and missing methods to VerbColor**

```rust
pub enum VerbColor {
    Infer,  // Violet #8B5CF6
    Exec,   // Amber #F59E0B
    Fetch,  // Cyan #06B6D4
    Invoke, // Emerald #10B981
    Agent,  // Rose #F43F5E
    Spawn,  // Rose-300 #FDA4AF (NEW)
}

impl VerbColor {
    // Existing: rgb(), glow(), muted(), subtle()

    /// Hex color string
    pub fn hex(&self) -> &'static str {
        match self {
            Self::Infer => "#8b5cf6",
            Self::Exec => "#f59e0b",
            Self::Fetch => "#06b6d4",
            Self::Invoke => "#10b981",
            Self::Agent => "#f43f5e",
            Self::Spawn => "#fda4af",
        }
    }

    /// Icon for this verb
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Infer => "⚡",
            Self::Exec => "📟",
            Self::Fetch => "🛰️",
            Self::Invoke => "🔌",
            Self::Agent => "🐔",
            Self::Spawn => "🐤",
        }
    }

    /// Label for this verb
    pub fn label(&self) -> &'static str {
        match self {
            Self::Infer => "INFER",
            Self::Exec => "EXEC",
            Self::Fetch => "FETCH",
            Self::Invoke => "INVOKE",
            Self::Agent => "AGENT",
            Self::Spawn => "SPAWN",
        }
    }

    /// Border color (darker variant)
    pub fn border_rgb(&self) -> Color {
        match self {
            Self::Infer => Color::Rgb(124, 58, 237),  // Violet 600
            Self::Exec => Color::Rgb(217, 119, 6),    // Amber 600
            Self::Fetch => Color::Rgb(8, 145, 178),   // Cyan 600
            Self::Invoke => Color::Rgb(5, 150, 105),  // Emerald 600
            Self::Agent => Color::Rgb(225, 29, 72),   // Rose 600
            Self::Spawn => Color::Rgb(251, 113, 133), // Rose 400
        }
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --lib theme::tests`
Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/theme.rs
git commit -m "feat(theme): extend VerbColor with Spawn + missing methods"
```

---

### Task 1.2: Update task_box/colors.rs to re-export from theme

**Files:**
- Modify: `src/tui/widgets/task_box/colors.rs`
- Modify: `src/tui/widgets/task_box/mod.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_verbcolor_reexport_matches_theme() {
    use crate::tui::theme::VerbColor as ThemeVerbColor;
    use crate::tui::widgets::task_box::VerbColor as TaskBoxVerbColor;

    // Both should be the same type after consolidation
    let theme_infer: ThemeVerbColor = ThemeVerbColor::Infer;
    let taskbox_infer: TaskBoxVerbColor = TaskBoxVerbColor::Infer;

    assert_eq!(theme_infer.rgb(), taskbox_infer.rgb());
}
```

**Step 2: Run test - will fail due to type mismatch**

**Step 3: Replace VerbColor enum with re-export**

In `src/tui/widgets/task_box/colors.rs`:

```rust
//! Verb Color Taxonomy
//!
//! Re-exports VerbColor from theme.rs (single source of truth).
//! Provides domain-specific color utilities for HTTP and exit codes.

// Re-export canonical VerbColor from theme
pub use crate::tui::theme::VerbColor;

/// Status colors (shared across all verbs)
pub mod status {
    use crate::tui::theme::Theme;
    use ratatui::style::Color;

    /// Get status color with theme support
    pub fn success(theme: Option<&Theme>) -> Color {
        theme.map(|t| t.status_success).unwrap_or(Color::Rgb(34, 197, 94))
    }

    pub fn error(theme: Option<&Theme>) -> Color {
        theme.map(|t| t.status_failed).unwrap_or(Color::Rgb(239, 68, 68))
    }

    pub fn warning(theme: Option<&Theme>) -> Color {
        theme.map(|t| t.status_running).unwrap_or(Color::Rgb(245, 158, 11))
    }

    pub fn muted(theme: Option<&Theme>) -> Color {
        theme.map(|t| t.text_muted).unwrap_or(Color::Rgb(100, 116, 139))
    }
}

/// HTTP status code colors (keep as-is, domain-specific)
pub mod http { /* unchanged */ }

/// Exit code colors (keep as-is, domain-specific)
pub mod exit { /* unchanged */ }
```

**Step 4: Update mod.rs export**

```rust
pub use colors::{exit, http, status, VerbColor};
```

**Step 5: Run tests**

Run: `cargo test --lib task_box`
Expected: PASS

**Step 6: Commit**

```bash
git add src/tui/widgets/task_box/
git commit -m "refactor(task_box): re-export VerbColor from theme.rs"
```

---

### Task 1.3: Update all VerbColor imports across codebase

**Files:**
- Search: `grep -r "task_box::VerbColor\|task_box::colors::VerbColor" src/`
- Update each to use `crate::tui::theme::VerbColor` or `crate::tui::VerbColor`

**Step 1: Find all imports**

```bash
grep -rn "use.*task_box.*VerbColor" src/tui/
```

**Step 2: Update each file**

Replace:
```rust
use crate::tui::widgets::task_box::VerbColor;
```

With:
```rust
use crate::tui::theme::VerbColor;
```

**Step 3: Run full test suite**

Run: `cargo test --lib`
Expected: All 2827+ tests pass

**Step 4: Commit**

```bash
git add -A
git commit -m "refactor(tui): unify VerbColor imports to theme.rs"
```

---

### Task 1.4: Remove duplicate VerbColor from task_box/colors.rs

**Files:**
- Modify: `src/tui/widgets/task_box/colors.rs`

**Step 1: Verify no direct VerbColor definition remains**

The file should only have:
- `pub use crate::tui::theme::VerbColor;`
- `pub mod status { ... }`
- `pub mod http { ... }`
- `pub mod exit { ... }`

**Step 2: Run tests**

Run: `cargo test --lib`
Expected: PASS

**Step 3: Commit**

```bash
git add src/tui/widgets/task_box/colors.rs
git commit -m "refactor(task_box): remove duplicate VerbColor enum"
```

---

## Phase 2: Widget Theme Migration (6 tasks)

### Task 2.1: Migrate agent_steps.rs (11 Color::Rgb)

**Files:**
- Modify: `src/tui/widgets/agent_steps.rs:56-65`

**Step 1: Remove hardcoded constants, use Theme/VerbColor**

Replace:
```rust
const COLOR_RUNNING: Color = Color::Rgb(250, 204, 21);
const COLOR_SUCCESS: Color = Color::Rgb(34, 197, 94);
// etc.
```

With theme-aware functions or direct VerbColor usage.

**Step 2: Run tests**

Run: `cargo test --lib agent_steps`

**Step 3: Commit**

```bash
git commit -m "refactor(agent_steps): migrate to Theme colors"
```

---

### Task 2.2: Migrate dag.rs (13 Color::Rgb)

**Files:**
- Modify: `src/tui/widgets/dag.rs`

**Step 1: Add Theme reference or use VerbColor**

**Step 2: Run tests**

**Step 3: Commit**

---

### Task 2.3: Migrate session_context.rs (14 Color::Rgb)

Already has theme support with fallbacks - verify and clean up.

---

### Task 2.4: Migrate pro_status_bar.rs (10 Color::Rgb)

---

### Task 2.5: Migrate mission_control.rs (11 Color::Rgb)

---

### Task 2.6: Migrate dag_node_box.rs (11 Color::Rgb)

---

## Phase 3: Provider Modal Theme (10 files, 101 Color::Rgb)

### Task 3.1: Add Theme to ProviderModal struct

**Files:**
- Modify: `src/tui/widgets/provider_modal/mod.rs`

Add `theme: &'a Theme` field to ProviderModal struct.

### Task 3.2-3.10: Migrate each provider_modal component

One task per file:
- components/provider_card.rs (26)
- tabs/keys.rs (14)
- components/verification_effect.rs (12)
- tabs/ollama.rs (11)
- components/footer_stats.rs (9)
- tabs/config.rs (7)
- tabs/cloud.rs (6)
- components/animated_header.rs (6)
- components/download_gauge.rs (5)

---

## Verification Checklist

After all tasks:

- [ ] `cargo test --lib` passes (2827+ tests)
- [ ] `cargo clippy -- -D warnings` passes
- [ ] Only ONE VerbColor enum exists (in theme.rs)
- [ ] task_box/colors.rs re-exports from theme
- [ ] grep -r "Color::Rgb" src/tui/widgets/ shows reduced count

---

## Execution Order

1. **Phase 1 first** - VerbColor consolidation is prerequisite
2. **Phase 2 parallel** - Widget migrations are independent
3. **Phase 3 after 2** - Provider modal depends on pattern from Phase 2

**Estimated time:** 2-3 hours with subagent-driven development
