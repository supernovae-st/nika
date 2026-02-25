# Cosmic Theme Wiring Plan

**Date:** 2026-02-25
**Target Version:** v0.9.1
**Status:** Planning
**Estimated Effort:** 4-6 hours

## Executive Summary

Wire the new Cosmic Tailwind Design System (`tokens/`, `icons.rs`) into the existing theme infrastructure, replacing hardcoded colors with semantic tokens while maintaining backward compatibility.

## Current State Analysis

### Files Impacted

| File | `theme.` usages | Impact |
|------|-----------------|--------|
| `app.rs` | 17 | App struct holds Theme |
| `views/chat.rs` | 92 | Heaviest user |
| `widgets/status_bar.rs` | 19 | Status colors |
| `views/home.rs` | 18 | File browser |
| `widgets/dag.rs` | 15 | DAG visualization |
| `views/studio.rs` | 15 | Editor |
| `widgets/header.rs` | 14 | Header bar |
| `panels/graph.rs` | 20 | Graph panel |
| Other (8 files) | ~30 | Misc widgets |
| **Total** | **306** | 16 files |

### Key Structures

```rust
// Current (app.rs:207)
pub struct App {
    theme: Theme,           // Replace with CosmicTheme
    theme_mode: ThemeMode,  // Map to CosmicVariant
    // ...
}

// Current (theme.rs)
pub struct Theme {
    pub background: Color,
    pub text_primary: Color,
    // ... 35 fields
}

pub enum ThemeMode {
    Dark,
    Light,
    Solarized,
}

pub enum VerbColor {
    Infer, Exec, Fetch, Invoke, Agent
}
```

## Architecture Decision

### Option A: Full Replacement (Breaking)
Replace `Theme` with `TokenResolver` everywhere.
- Pros: Clean architecture
- Cons: 306 changes, high risk

### Option B: Adapter Pattern (Recommended)
Keep `Theme` API, use `TokenResolver` internally.
- Pros: Zero breaking changes, incremental migration
- Cons: Thin wrapper overhead

### Option C: Dual System
Run both systems in parallel.
- Pros: Safe rollback
- Cons: Code duplication

**Decision: Option B** - Adapter pattern with incremental migration.

## Implementation Plan

### Phase 1: Bridge Layer (1 hour)

Create `CosmicTheme` adapter that wraps `TokenResolver` while exposing `Theme`-compatible API.

```rust
// src/tui/cosmic_theme.rs (NEW)

use super::tokens::{TokenResolver, CosmicVariant};
use super::theme::{Theme, ThemeMode, VerbColor};

/// Bridge between TokenResolver and legacy Theme API
pub struct CosmicTheme {
    resolver: TokenResolver,
    variant: CosmicVariant,
}

impl CosmicTheme {
    pub fn new(variant: CosmicVariant) -> Self {
        Self {
            resolver: variant.resolver(),
            variant,
        }
    }

    /// Get legacy Theme for backward compatibility
    pub fn as_theme(&self) -> Theme {
        Theme::from_resolver(&self.resolver)
    }

    /// Get resolver for new code
    pub fn resolver(&self) -> &TokenResolver {
        &self.resolver
    }

    pub fn cycle(&mut self) {
        self.variant = self.variant.cycle();
        self.resolver = self.variant.resolver();
    }
}

impl From<ThemeMode> for CosmicVariant {
    fn from(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Dark => CosmicVariant::CosmicDark,
            ThemeMode::Light => CosmicVariant::CosmicLight,
            ThemeMode::Solarized => CosmicVariant::CosmicDark,
        }
    }
}
```

#### Tasks

- [ ] Create `src/tui/cosmic_theme.rs`
- [ ] Implement `CosmicTheme` struct
- [ ] Implement `From<ThemeMode> for CosmicVariant`
- [ ] Add `Theme::from_resolver()` constructor
- [ ] Add to `tui/mod.rs` exports
- [ ] Write 5 unit tests

### Phase 2: Theme.rs Integration (1 hour)

Update `Theme` to use `TokenResolver` internally.

```rust
// src/tui/theme.rs - UPDATE

impl Theme {
    /// Create Theme from TokenResolver (bridge method)
    pub fn from_resolver(resolver: &TokenResolver) -> Self {
        Self {
            // Map token to legacy field
            background: resolver.bg_primary(),
            text_primary: resolver.text_primary(),
            text_secondary: resolver.text_secondary(),
            text_muted: resolver.text_muted(),
            border_normal: resolver.border_default(),
            border_focused: resolver.border_focused(),
            highlight: resolver.accent_primary(),

            // Status colors
            status_pending: resolver.semantic().status_warning,
            status_running: resolver.status_warning(),
            status_success: resolver.status_success(),
            status_failed: resolver.status_error(),
            status_paused: resolver.accent_secondary(),

            // Scrollbar
            scrollbar_thumb: resolver.semantic().scrollbar_thumb,
            scrollbar_track: resolver.semantic().scrollbar_track,
            scrollbar_arrows: resolver.semantic().scrollbar_arrows,

            // ... map all 35 fields
        }
    }

    /// Create Cosmic Dark (NEW default)
    pub fn cosmic_dark() -> Self {
        Self::from_resolver(&TokenResolver::cosmic_dark())
    }

    /// Create Cosmic Light
    pub fn cosmic_light() -> Self {
        Self::from_resolver(&TokenResolver::cosmic_light())
    }

    /// Create Cosmic Violet
    pub fn cosmic_violet() -> Self {
        Self::from_resolver(&TokenResolver::cosmic_violet())
    }
}
```

#### Tasks

- [ ] Add `from_resolver()` to Theme
- [ ] Add `cosmic_dark()`, `cosmic_light()`, `cosmic_violet()` constructors
- [ ] Map all 35 Theme fields from TokenResolver
- [ ] Keep `dark()`, `light()`, `solarized()` as aliases
- [ ] Update tests to verify color consistency

### Phase 3: VerbColor Migration (30 min)

Update `VerbColor` to use unified icons from `icons.rs`.

```rust
// src/tui/theme.rs - UPDATE VerbColor

use super::icons::verb;

impl VerbColor {
    /// Get icon (delegate to icons module)
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Infer => verb::INFER,
            Self::Exec => verb::EXEC,
            Self::Fetch => verb::FETCH,
            Self::Invoke => verb::INVOKE,
            Self::Agent => verb::AGENT,
        }
    }

    /// Get ASCII icon (delegate to icons module)
    pub fn icon_ascii(&self) -> &'static str {
        match self {
            Self::Infer => verb::INFER_ASCII,
            Self::Exec => verb::EXEC_ASCII,
            Self::Fetch => verb::FETCH_ASCII,
            Self::Invoke => verb::INVOKE_ASCII,
            Self::Agent => verb::AGENT_ASCII,
        }
    }

    /// Get color from resolver
    pub fn color(&self, resolver: &TokenResolver) -> Color {
        match self {
            Self::Infer => resolver.verb_infer(),
            Self::Exec => resolver.verb_exec(),
            Self::Fetch => resolver.verb_fetch(),
            Self::Invoke => resolver.verb_invoke(),
            Self::Agent => resolver.verb_agent(),
        }
    }
}
```

#### Tasks

- [ ] Import `icons::verb` in theme.rs
- [ ] Update `VerbColor::icon()` to delegate
- [ ] Update `VerbColor::icon_ascii()` to delegate
- [ ] Add `VerbColor::color(&TokenResolver)` method
- [ ] Remove duplicated icon constants

### Phase 4: App.rs Migration (1 hour)

Update App to use CosmicTheme.

```rust
// src/tui/app.rs - UPDATE

use super::cosmic_theme::CosmicTheme;
use super::tokens::CosmicVariant;

pub struct App {
    // OLD: theme: Theme,
    // OLD: theme_mode: ThemeMode,
    cosmic_theme: CosmicTheme,  // NEW: unified theme
    // ...
}

impl App {
    pub fn new() -> Self {
        Self {
            cosmic_theme: CosmicTheme::new(CosmicVariant::CosmicDark),
            // ...
        }
    }

    /// Get theme for legacy widgets
    fn theme(&self) -> Theme {
        self.cosmic_theme.as_theme()
    }

    /// Get resolver for new widgets
    fn resolver(&self) -> &TokenResolver {
        self.cosmic_theme.resolver()
    }

    fn handle_toggle_theme(&mut self) {
        self.cosmic_theme.cycle();
    }
}
```

#### Tasks

- [ ] Replace `theme: Theme` with `cosmic_theme: CosmicTheme`
- [ ] Remove `theme_mode: ThemeMode` (now in CosmicTheme)
- [ ] Add `theme()` helper method for backward compat
- [ ] Add `resolver()` helper for new code
- [ ] Update `handle_toggle_theme` to use `cosmic_theme.cycle()`
- [ ] Update all render functions to use `self.theme()`

### Phase 5: Widget Migration (1.5 hours)

Migrate widgets incrementally. Priority order:

#### Tier 1: High-visibility (Critical)

| Widget | File | Usages | Migration |
|--------|------|--------|-----------|
| StatusBar | `widgets/status_bar.rs` | 19 | Use resolver for status colors |
| Header | `widgets/header.rs` | 14 | Use resolver for accents |
| DAG | `widgets/dag.rs` | 15 | Use verb colors from resolver |

#### Tier 2: Views (Important)

| View | File | Usages | Migration |
|------|------|--------|-----------|
| ChatView | `views/chat.rs` | 92 | Largest - incremental |
| HomeView | `views/home.rs` | 18 | File browser |
| StudioView | `views/studio.rs` | 15 | Editor |

#### Tier 3: Panels (Secondary)

| Panel | File | Usages |
|-------|------|--------|
| GraphPanel | `panels/graph.rs` | 20 |
| ReasoningPanel | `panels/reasoning.rs` | 1 |
| ContextPanel | `panels/context.rs` | 1 |
| ProgressPanel | `panels/progress.rs` | 1 |

#### Migration Pattern

```rust
// BEFORE
fn render(frame: &mut Frame, theme: &Theme, area: Rect) {
    let style = Style::default().fg(theme.text_primary);
}

// AFTER (Option 1: Use Theme helper)
fn render(frame: &mut Frame, theme: &Theme, area: Rect) {
    let style = theme.text_style();  // Uses resolver internally
}

// AFTER (Option 2: Use resolver directly)
fn render(frame: &mut Frame, resolver: &TokenResolver, area: Rect) {
    let style = Style::default().fg(resolver.text_primary());
}
```

#### Tasks

- [ ] Migrate StatusBar (high visibility)
- [ ] Migrate Header (branding)
- [ ] Migrate DAG widget (verb colors)
- [ ] Migrate ChatView (largest)
- [ ] Migrate HomeView
- [ ] Migrate StudioView
- [ ] Migrate panels (batch)

### Phase 6: Cleanup and Tests (30 min)

Remove deprecated code and add comprehensive tests.

#### Tasks

- [ ] Remove duplicated color constants from theme.rs
- [ ] Remove `solarized` module duplicate colors
- [ ] Update theme.rs docs
- [ ] Add integration tests for theme cycling
- [ ] Add snapshot tests for color consistency
- [ ] Update CLAUDE.md with new theme API

## File Changes Summary

| File | Action | LOC Change |
|------|--------|------------|
| `tui/cosmic_theme.rs` | CREATE | +80 |
| `tui/theme.rs` | MODIFY | -200, +100 |
| `tui/mod.rs` | MODIFY | +5 |
| `tui/app.rs` | MODIFY | ~50 |
| `tui/widgets/*.rs` | MODIFY | ~100 |
| `tui/views/*.rs` | MODIFY | ~150 |
| `tui/panels/*.rs` | MODIFY | ~30 |
| **Total** | | ~400 net change |

## Verification Checklist

### Functional

- [ ] Theme cycling (Shift+T) works
- [ ] All 3 variants render correctly
- [ ] Verb colors match icons
- [ ] Status colors visible
- [ ] Scrollbar styled

### Visual

- [ ] Cosmic Dark has Slate-900 background
- [ ] Cosmic Light has Slate-50 background
- [ ] Cosmic Violet has Violet-950 background
- [ ] All text readable (WCAG AA)
- [ ] Accents pop (Violet, Cyan, Pink)

### Tests

- [ ] `cargo test --features tui` passes
- [ ] No regressions in existing tests
- [ ] New tests for CosmicTheme
- [ ] Snapshot tests updated

## Rollback Plan

If issues arise:

1. Revert `cosmic_theme.rs` changes
2. Restore `theme: Theme` in App
3. Keep tokens/ and icons.rs (no harm)

## Timeline

| Phase | Time | Cumulative |
|-------|------|------------|
| Phase 1: Bridge | 1h | 1h |
| Phase 2: Theme.rs | 1h | 2h |
| Phase 3: VerbColor | 30m | 2.5h |
| Phase 4: App.rs | 1h | 3.5h |
| Phase 5: Widgets | 1.5h | 5h |
| Phase 6: Cleanup | 30m | 5.5h |
| Buffer | 30m | **6h total** |

## Dependencies

- [x] `tokens/` module complete (24 tests)
- [x] `icons.rs` module complete (7 tests)
- [ ] This plan approved

## Next Steps After Completion

1. Document theme API in CLAUDE.md
2. Add user preference to config.toml
3. Consider system dark mode detection
4. Add custom theme support (future)
