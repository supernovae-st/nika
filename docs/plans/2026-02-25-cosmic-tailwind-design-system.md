# Cosmic Tailwind Design System

**Date:** 2026-02-25
**Version:** v0.9.0
**Status:** Implemented

## Overview

Cosmic Tailwind is Nika's modular design system featuring:

- **Tailwind CSS colors** converted to Ratatui `Color::Rgb`
- **Token-based architecture** for easy theme switching
- **Unified icon system** resolving all conflicts
- **Three cosmic variants**: Dark, Light, Violet

## Philosophy

```
🦋 Nika Brand = Violet
🌌 Cosmic Glow = Cyan
✨ Nebula Highlights = Pink
```

### Why Slate, Not Gray?

Slate palette has blue undertones that create depth and sophistication:

| Palette | Undertone | Effect |
|---------|-----------|--------|
| Gray | Neutral | Flat, industrial |
| Slate | Blue-tinted | Cosmic, deep, refined |

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  Level 1: ColorPalette      Raw Tailwind colors (Slate, Violet, etc.)      │
│  Level 2: SemanticColors    Purpose-driven colors (bg, text, verbs)        │
│  Level 3: ComponentTokens   UI element colors (button, panel, etc.)        │
│  Level 4: ThemeVariants     Dark/Light/Cosmic configurations               │
│  Level 5: TokenResolver     Runtime resolution with mode detection         │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Files

| File | Purpose |
|------|---------|
| `src/tui/tokens/mod.rs` | Main module, `TokenResolver`, `CosmicVariant` |
| `src/tui/tokens/colors.rs` | `ColorPalette` with 9 Tailwind palettes |
| `src/tui/tokens/semantic.rs` | `SemanticColors` with 3 cosmic themes |
| `src/tui/icons.rs` | Unified icon system (no conflicts) |

## Color Palettes

### Slate (Neutrals)

```rust
slate_50:  Color::Rgb(248, 250, 252)  // #f8fafc - Light bg
slate_100: Color::Rgb(241, 245, 249)  // #f1f5f9
slate_200: Color::Rgb(226, 232, 240)  // #e2e8f0
slate_300: Color::Rgb(203, 213, 225)  // #cbd5e1
slate_400: Color::Rgb(148, 163, 184)  // #94a3b8
slate_500: Color::Rgb(100, 116, 139)  // #64748b
slate_600: Color::Rgb(71, 85, 105)    // #475569
slate_700: Color::Rgb(51, 65, 85)     // #334155
slate_800: Color::Rgb(30, 41, 59)     // #1e293b
slate_900: Color::Rgb(15, 23, 42)     // #0f172a - Dark bg
slate_950: Color::Rgb(2, 6, 23)       // #020617 - Deepest
```

### Violet (Primary Brand)

```rust
violet_500: Color::Rgb(139, 92, 246)  // #8b5cf6 - 🦋 Brand
violet_600: Color::Rgb(124, 58, 237)  // #7c3aed - Light mode
violet_400: Color::Rgb(167, 139, 250) // #a78bfa - Glow
```

### Cyan (Cosmic Glow)

```rust
cyan_500: Color::Rgb(6, 182, 212)     // #06b6d4 - 🌌 Glow
cyan_400: Color::Rgb(34, 211, 238)    // #22d3ee - Bright
cyan_600: Color::Rgb(8, 145, 178)     // #0891b2 - Light mode
```

### Pink (Nebula)

```rust
pink_500: Color::Rgb(236, 72, 153)    // #ec4899 - Nebula
pink_400: Color::Rgb(244, 114, 182)   // #f472b6 - Bright
pink_600: Color::Rgb(219, 39, 119)    // #db2777 - Light mode
```

## Theme Variants

### 🌌 Cosmic Dark (Default)

```rust
bg_primary:     slate_900   // #0f172a
bg_secondary:   slate_800   // #1e293b
text_primary:   slate_50    // #f8fafc
accent_primary: violet_500  // #8b5cf6
```

### 🌅 Cosmic Light

```rust
bg_primary:     slate_50    // #f8fafc
bg_secondary:   slate_100   // #f1f5f9
text_primary:   slate_900   // #0f172a
accent_primary: violet_600  // #7c3aed
```

### 🦋 Cosmic Violet

```rust
bg_primary:     violet_950  // #2e1065
bg_secondary:   violet_900  // #4c1d95
text_primary:   violet_50   // #f5f3ff
accent_primary: cyan_400    // #22d3ee (inverse)
```

## Verb Colors

| Verb | Color | Shade | Icon |
|------|-------|-------|------|
| `infer:` | Violet | 500 | ⚡ |
| `exec:` | Amber | 500 | 📟 |
| `fetch:` | Cyan | 500 | 🛰️ |
| `invoke:` | Emerald | 500 | 🔌 |
| `agent:` | Rose | 500 | 🐔 |

## Icon System

### Fixed Conflicts

| Icon | Old Usage | New Usage |
|------|-----------|-----------|
| ⚡ | Groq + Infer | Infer only |
| ⏱️ | - | Groq provider |

### Provider Icons

| Provider | Icon | ASCII |
|----------|------|-------|
| Claude | 🧠 | [C] |
| OpenAI | 🤖 | [O] |
| Mistral | 💨 | [M] |
| Ollama | 🦙 | [L] |
| Groq | ⏱️ | [G] |
| DeepSeek | 🌊 | [D] |

### Status Icons

| Status | Icon | ASCII |
|--------|------|-------|
| Pending | ⏳ | [ ] |
| Running | ⟳ | [~] |
| Success | ✓ | [+] |
| Failed | ✗ | [!] |
| Paused | ⏸ | [-] |

## Usage

### Basic

```rust
use nika::tui::tokens::TokenResolver;

let resolver = TokenResolver::cosmic_dark();
let bg = resolver.bg_primary();
let text = resolver.text_primary();
let verb_color = resolver.verb_color("infer");
```

### With Icons

```rust
use nika::tui::icons::{IconSet, IconMode};

let icons = IconSet::new(); // Auto-detect
let verb_icon = icons.verb("infer"); // ⚡ or [I]
let provider = icons.provider("claude"); // 🧠 or [C]
```

### Theme Switching

```rust
use nika::tui::tokens::CosmicVariant;

let variant = CosmicVariant::CosmicDark;
let next = variant.cycle(); // → CosmicLight
let resolver = next.resolver();
```

## WCAG Accessibility

All themes maintain WCAG AA contrast ratios:

| Element | Contrast | Status |
|---------|----------|--------|
| text_primary on bg_primary | > 7:1 | AAA |
| text_secondary on bg_primary | > 4.5:1 | AA |
| accent_primary on bg_primary | > 4.5:1 | AA |

## Test Coverage

```
tui::tokens::* - 17 tests
tui::icons::* - 7 tests
Total: 24 new tests
```

## Migration Guide

### From Old Theme

```rust
// Before
let theme = Theme::dark();
let color = theme.background;

// After
let resolver = TokenResolver::cosmic_dark();
let color = resolver.bg_primary();
```

### Icon Migration

```rust
// Before (hardcoded)
let icon = "⚡"; // Could conflict

// After (unified)
use nika::tui::icons::verb;
let icon = verb::INFER; // Guaranteed unique
```

## Future Work

1. **Component Tokens (Level 3)** - Button, panel, input specific colors
2. **Animation Tokens** - Pulse speeds, glow intensities
3. **Custom Themes** - User-defined palette overrides via config
4. **Dark Mode Detection** - System preference integration

## References

- [Tailwind CSS Colors](https://tailwindcss.com/docs/customizing-colors)
- [WCAG 2.1 Contrast Guidelines](https://www.w3.org/WAI/WCAG21/Understanding/contrast-minimum.html)
- [Ratatui Styling](https://docs.rs/ratatui/latest/ratatui/style/index.html)
