# Research Report: Ratatui Widget Ecosystem & TUI Design Trends (2025-2026)

## Summary

The ratatui ecosystem has matured significantly through 2025-2026, with v0.30 introducing
modularized architecture, `no_std` support, and the simplified `ratatui::run()` API. The
community widget ecosystem is anchored by the **tui-widgets** monorepo (by Joshka/ratatui-org),
**tachyonfx** for shader-like effects, and **ratatui-image** for image rendering. Color theming
has converged on **Catppuccin Mocha** and **Tokyo Night** as the dominant dark palettes. This
report provides actionable guidance for Nika's TUI evolution.

---

## 1. Ratatui Core (v0.30) -- What's New

Ratatui 0.30.0 is the largest release in the library's history (released mid-2025), described by
maintainer Orhun Parmaksiz as the culmination of a full year of work.

### Key Features

| Feature | Details |
|---------|---------|
| **`no_std` support** | Embedded targets (e.g., ESP32, Raspberry Pi Pico) via `ratatui-core` |
| **Modularized architecture** | Core split into `ratatui-core` (no_std) + `ratatui-widgets` (std) |
| **`ratatui::run()` API** | Simplified application bootstrap, replaces manual init/restore |
| **Rust 2024 edition** | Full migration to Rust 2024 |
| **Flex layout enhancements** | New `Flex::SpaceAround`, renamed `Flex::SpaceEvenly` |
| **RatatuiMascot widget** | Built-in mascot widget |
| **Serde support** | `Serialize`/`Deserialize` for Constraint, Direction, Layout, Palette |
| **VS16 emoji fix** | Wide emoji clearing from buffer |
| **Braille over Block** | Braille characters render over Block symbols in Chart/Canvas |
| **ScrollbarState** | `get_position()` method added |
| **VerticalAlignment** | New enum for API consistency |

### Version History

| Version | Date | Key Change |
|---------|------|------------|
| 0.29.0 | Early 2025 | OSC52 clipboard, keyboard enhancement flags |
| 0.30.0 | Jul 2025 | no_std, modular crates, Flex::SpaceAround |
| Nika currently uses | **0.30** | with crossterm 0.29 |

---

## 2. Community Widget Ecosystem

### 2.1 tui-widgets Monorepo (Official)

**Repository**: https://github.com/ratatui/tui-widgets (moved from joshka/tui-widgets)
**Status**: Actively maintained by Joshka (ratatui core team)
**Downloads**: ~4,929/month combined
**Latest monorepo version**: 0.7.0 (Jan 2026)

The monorepo consolidates previously standalone crates for unified maintenance:

| Crate | Purpose | Status |
|-------|---------|--------|
| **tui-big-text** | Large pixel text using font8x8 glyphs (splash screens, headers) | Stable |
| **tui-scrollview** | Scrollable viewport widget with Gauge overlay support | Stable |
| **tui-popup** | Modal popup/dialog widget | Stable |
| **tui-prompts** | Interactive prompt widgets (input, confirm, select) | Stable |
| **tui-scrollbar** | Smooth fractional scrollbars with precise thumb feedback | New (Jan 2026) |
| **tui-cards** | Playing card rendering | Stable |
| **tui-bar-graph** | Enhanced bar graph widget | Stable |
| **tui-box-text** | Boxed text rendering | Stable |
| **tui-qrcode** | QR code rendering in terminal | Stable |

**Recommendation for Nika**: `tui-big-text` (splash/headers), `tui-scrollview` (log views),
`tui-popup` (dialogs/confirmations), `tui-scrollbar` (smooth scrolling).

### 2.2 tachyonfx -- Shader-Like Terminal Effects

**Repository**: https://github.com/ratatui/tachyonfx (formerly junkdog/tachyonfx)
**Version**: 0.11.1 (Mar 2025)
**Status**: Official ratatui ecosystem crate, featured at FOSDEM 2025
**Docs**: https://ratatui.rs/ecosystem/tachyonfx/

The most important visual effects crate for ratatui. Provides 40+ composable shader-like effects.

#### Effect Categories

| Category | Effects |
|----------|---------|
| **Color transforms** | `fade_from_fg`, `fade_to_fg`, `fade_from_bg`, `fade_to_bg`, `hsl_shift`, `color_cycle` |
| **Text animations** | `coalesce`, `dissolve`, `sweep_in`, `sweep_out` |
| **Geometric** | `slide_in`, `slide_out`, `translate`, `resize` |
| **Spatial patterns** | Radial, diagonal, checkerboard targeting |
| **Glow/emphasis** | `glow`, `pulse` |
| **Modifiers** | `ping_pong`, `repeat`, `run_once`, `never_complete`, `with_duration` |

#### Composition API

```rust
use tachyonfx::fx;

// Parallel: both run simultaneously
let intro = fx::parallel(&[
    fx::fade_from_fg(Color::Black, 500),
    fx::slide_in(Direction::LeftToRight, 800),
]);

// Sequential: first completes, then second
let transition = fx::sequence(&[
    fx::fade_to_fg(Color::Black, 300),
    fx::coalesce(500),
]);

// Apply in render loop
effects.tick(Instant::now());
```

**Recommendation for Nika**: Essential for view transitions, splash screen, and the
"space/supernova" aesthetic. Star field backgrounds can be built with spatial patterns +
color cycling.

### 2.3 ratatui-image

**Repository**: https://github.com/ratatui/ratatui-image
**Downloads**: ~31,756/month (16th in CLI crates)
**Used by**: 100+ crates
**Status**: Actively maintained

Renders images in terminal using multiple protocols:

| Protocol | Terminal Support | Quality |
|----------|-----------------|---------|
| **Halfblocks** | Universal (any terminal) | Medium (2x vertical resolution) |
| **Sixel** | xterm, foot, WezTerm, mlterm | High |
| **Kitty graphics** | Kitty, WezTerm | Highest |
| **iTerm2** | iTerm2, WezTerm | High |

```rust
use ratatui_image::{picker::Picker, StatefulImage, protocol::StatefulProtocol};

let mut picker = Picker::from_termios().unwrap();
let image = picker.new_resize_protocol(img);
let widget = StatefulImage::new(None);
f.render_stateful_widget(widget, area, &mut image);
```

**Recommendation for Nika**: Useful for `nika:thumbnail` preview in TUI, workflow
visualization, or a branded splash screen.

### 2.4 ratatui-splash-screen

**Repository**: https://github.com/orhun/ratatui-splash-screen
**Version**: 0.1.5
**Author**: Orhun Parmaksiz (ratatui maintainer)
**Dependencies**: ratatui >=0.25.0, image 0.25, sha256

Converts any image into an ASCII/halfblock splash screen widget. Simpler than
ratatui-image for the specific use case of startup splash screens.

**Recommendation for Nika**: Perfect for `nika ui` startup -- render the SuperNovae
logo as a splash before the main TUI loads.

### 2.5 Other Notable Crates

| Crate | Purpose | Downloads/Status |
|-------|---------|------------------|
| **edtui** | Vim-inspired editor widget for ratatui | Listed in awesome-ratatui |
| **ratatui-explorer** | File explorer widget | Listed in awesome-ratatui |
| **ratatui-fretboard** | Musical fretboard display | Niche |
| **ratatui-textarea** | Rich text editor widget (fork) | Listed in awesome-ratatui |
| **tui-input** | Text input widget | **Already used by Nika** (0.15) |
| **rat-widget** | Extended widgets with event/focus/scrolling | v3.2.1 (Mar 2026), active |
| **ansi-to-tui** | Convert ANSI escape codes to ratatui Text | Useful for `exec:` output |
| **tui-logger** | Logger UI widget | Useful for trace/event display |
| **tui-term** | Terminal emulator widget | Embed terminal in TUI pane |
| **ratatui-macros** | Declarative widget macros | QoL improvement |
| **bevy_ratatui** | Bevy ECS integration with ratatui | Feb 2026, experimental |
| **ratatui-elm** | Async ELM architecture framework | Dec 2025, community |

### 2.6 rat-widget -- The "Full Framework" Option

**Repository**: https://github.com/thscharler/rat-widget
**Version**: 3.2.1 (Mar 2026)
**Philosophy**: Speed, no-alloc, event/focus management built-in

Provides an extended widget set with built-in event handling via `rat-event`:

- Enhanced tables with row/column selection
- Focus management (tab order, focus rings)
- Scrolling containers
- Layout managers
- Date/time pickers

**Trade-off**: Heavier dependency, more opinionated than individual tui-widgets crates.
Better for apps that need full widget toolkit. May conflict with Nika's existing
Component architecture.

---

## 3. Color Palettes for Dark Themes

### 3.1 Catppuccin Mocha (Recommended Primary)

The most popular dark theme in the terminal ecosystem (2025-2026). Pastel colors
designed for readability and reduced eye strain.

```rust
// Catppuccin Mocha -- Complete palette as ratatui Color::Rgb
pub mod catppuccin_mocha {
    use ratatui::style::Color;

    // Accent colors
    pub const ROSEWATER:  Color = Color::Rgb(245, 224, 220);  // #f5e0dc
    pub const FLAMINGO:   Color = Color::Rgb(242, 205, 205);  // #f2cdcd
    pub const PINK:       Color = Color::Rgb(245, 194, 231);  // #f5c2e7
    pub const MAUVE:      Color = Color::Rgb(203, 166, 247);  // #cba6f7
    pub const RED:        Color = Color::Rgb(243, 139, 168);  // #f38ba8
    pub const MAROON:     Color = Color::Rgb(235, 160, 172);  // #eba0ac
    pub const PEACH:      Color = Color::Rgb(250, 179, 135);  // #fab387
    pub const YELLOW:     Color = Color::Rgb(249, 226, 175);  // #f9e2af
    pub const GREEN:      Color = Color::Rgb(166, 227, 161);  // #a6e3a1
    pub const TEAL:       Color = Color::Rgb(148, 226, 213);  // #94e2d5
    pub const SKY:        Color = Color::Rgb(137, 220, 235);  // #89dceb
    pub const SAPPHIRE:   Color = Color::Rgb(116, 199, 236);  // #74c7ec
    pub const BLUE:       Color = Color::Rgb(137, 180, 250);  // #89b4fa
    pub const LAVENDER:   Color = Color::Rgb(180, 190, 254);  // #b4befe

    // Text hierarchy
    pub const TEXT:       Color = Color::Rgb(205, 214, 244);  // #cdd6f4
    pub const SUBTEXT1:   Color = Color::Rgb(186, 194, 222);  // #bac2de
    pub const SUBTEXT0:   Color = Color::Rgb(166, 173, 200);  // #a6adc8

    // Overlay / muted
    pub const OVERLAY2:   Color = Color::Rgb(147, 153, 178);  // #9399b2
    pub const OVERLAY1:   Color = Color::Rgb(127, 132, 156);  // #7f849c
    pub const OVERLAY0:   Color = Color::Rgb(108, 112, 134);  // #6c7086

    // Surface layers (for panels, borders, selection)
    pub const SURFACE2:   Color = Color::Rgb(88, 91, 112);    // #585b70
    pub const SURFACE1:   Color = Color::Rgb(69, 71, 90);     // #45475a
    pub const SURFACE0:   Color = Color::Rgb(49, 50, 68);     // #313244

    // Background layers
    pub const BASE:       Color = Color::Rgb(30, 30, 46);     // #1e1e2e
    pub const MANTLE:     Color = Color::Rgb(24, 24, 37);     // #181825
    pub const CRUST:      Color = Color::Rgb(17, 17, 27);     // #11111b
}
```

#### Semantic Mapping for Nika TUI

| UI Element | Color | Rationale |
|------------|-------|-----------|
| Background | `BASE` (#1e1e2e) | Main background |
| Panel background | `MANTLE` (#181825) | Slightly darker for depth |
| Deepest background | `CRUST` (#11111b) | Status bar, bottom rail |
| Primary text | `TEXT` (#cdd6f4) | Body text |
| Secondary text | `SUBTEXT1` (#bac2de) | Labels, hints |
| Disabled/muted | `OVERLAY0` (#6c7086) | Inactive items |
| Borders | `SURFACE1` (#45475a) | Panel borders |
| Selection bg | `SURFACE0` (#313244) | Selected item |
| Active border | `LAVENDER` (#b4befe) | Focused panel |
| Success | `GREEN` (#a6e3a1) | Completed tasks |
| Error | `RED` (#f38ba8) | Failed tasks |
| Warning | `YELLOW` (#f9e2af) | Warnings |
| Info/link | `BLUE` (#89b4fa) | Informational |
| Running/active | `MAUVE` (#cba6f7) | In-progress, agent |
| Accent/brand | `PEACH` (#fab387) | SuperNovae brand accent |
| AI/inference | `SKY` (#89dceb) | LLM-related |
| MCP/invoke | `TEAL` (#94e2d5) | MCP operations |
| Keyword | `MAUVE` (#cba6f7) | YAML verb keywords |

### 3.2 Tokyo Night (Alternative)

Neon-inspired, slightly higher contrast than Catppuccin. Popular in Neovim ecosystem.

```rust
pub mod tokyo_night {
    use ratatui::style::Color;

    // Backgrounds
    pub const BG:           Color = Color::Rgb(26, 27, 38);    // #1a1b26
    pub const BG_DARK:      Color = Color::Rgb(31, 35, 53);    // #1f2335
    pub const BG_HIGHLIGHT: Color = Color::Rgb(41, 46, 66);    // #292e42

    // Foreground
    pub const FG:           Color = Color::Rgb(192, 202, 245); // #c0caf5
    pub const FG_DARK:      Color = Color::Rgb(169, 177, 214); // #a9b1d6
    pub const FG_GUTTER:    Color = Color::Rgb(59, 66, 82);    // #3b4252
    pub const DARK_BLUE:    Color = Color::Rgb(65, 72, 104);   // #414868

    // Accent colors
    pub const BLUE:         Color = Color::Rgb(122, 162, 247); // #7aa2f7
    pub const CYAN:         Color = Color::Rgb(125, 207, 255); // #7dcfff
    pub const GREEN:        Color = Color::Rgb(158, 206, 106); // #9ece6a
    pub const MAGENTA:      Color = Color::Rgb(187, 154, 247); // #bb9af7
    pub const ORANGE:       Color = Color::Rgb(255, 158, 100); // #ff9e64
    pub const RED:          Color = Color::Rgb(247, 118, 142); // #f7768e
    pub const YELLOW:       Color = Color::Rgb(224, 175, 104); // #e0af68
}
```

### 3.3 Nord (Conservative Alternative)

Arctic-inspired, subdued tones. Lower contrast, very calm.

```rust
pub mod nord {
    use ratatui::style::Color;

    pub const POLAR0:  Color = Color::Rgb(46, 52, 64);    // #2e3440 (bg)
    pub const POLAR1:  Color = Color::Rgb(59, 66, 82);    // #3b4252
    pub const POLAR2:  Color = Color::Rgb(67, 76, 94);    // #434c5e
    pub const POLAR3:  Color = Color::Rgb(76, 86, 106);   // #4c566a (comment)
    pub const SNOW0:   Color = Color::Rgb(216, 222, 233); // #d8dee9 (fg)
    pub const SNOW1:   Color = Color::Rgb(229, 233, 240); // #e5e9f0
    pub const SNOW2:   Color = Color::Rgb(236, 239, 244); // #eceff4
    pub const FROST0:  Color = Color::Rgb(143, 188, 187); // #8fbcbb (cyan)
    pub const FROST1:  Color = Color::Rgb(136, 192, 208); // #88c0d0
    pub const FROST2:  Color = Color::Rgb(129, 161, 193); // #81a1c1 (blue)
    pub const FROST3:  Color = Color::Rgb(94, 129, 172);  // #5e81ac
    pub const RED:     Color = Color::Rgb(191, 97, 106);  // #bf616a
    pub const ORANGE:  Color = Color::Rgb(208, 135, 112); // #d08770
    pub const YELLOW:  Color = Color::Rgb(235, 203, 139); // #ebcb8b
    pub const GREEN:   Color = Color::Rgb(163, 190, 140); // #a3be8c
    pub const PURPLE:  Color = Color::Rgb(180, 142, 173); // #b48ead
}
```

### 3.4 Palette Comparison

| Aspect | Catppuccin Mocha | Tokyo Night | Nord |
|--------|-----------------|-------------|------|
| **Vibe** | Warm pastel | Neon cyberpunk | Arctic calm |
| **Contrast** | Medium | Medium-High | Low |
| **Eye strain** | Low | Medium | Very Low |
| **Accent count** | 14 colors | 7 colors | 5 + 4 frost |
| **Community size** | Largest (300+ ports) | Large (200+ ports) | Large (100+ ports) |
| **Best for** | General purpose | Code-heavy UIs | Minimal/clean UIs |
| **Nika fit** | Best (warm + many accents for status states) | Good (cyberpunk matches space theme) | Too subdued |

**Recommendation**: Catppuccin Mocha as default, with Tokyo Night as switchable alternative.
The 14 accent colors map perfectly to Nika's verb/status/provider color coding needs.

---

## 4. Modern TUI Design Patterns

### 4.1 Layout Patterns from Top TUIs

#### gitui (12k+ stars)
- **Pattern**: Modal stacked panels with popup dialogs
- **Layout**: Status bar (top) | Main content (3 columns) | Keybindings (bottom)
- **Navigation**: Tab between panels, popup for details
- **Theme**: Configurable via TOML (supports Catppuccin, Tokyo Night)

#### helix (35k+ stars)
- **Pattern**: Code editor with floating overlays
- **Layout**: Editor (center) | Gutter (left) | Status line (bottom) | Command palette (popup)
- **Navigation**: Modal (vim-like), fuzzy picker overlays
- **Theme**: TOML config, ships Catppuccin/Tokyo Night natively

#### bottom / btm (10k+ stars)
- **Pattern**: Dashboard with real-time charts
- **Layout**: 4-row grid of charts/graphs, responsive to terminal size
- **Navigation**: Process table + chart toggle
- **Theme**: JSON/YAML config files

#### lazygit (55k+ stars)
- **Pattern**: Multi-panel with context-sensitive views
- **Layout**: 5-panel layout: Files | Branches | Commits | Stash | Diff
- **Navigation**: Tab between panels, enter for detail, popup for actions
- **Theme**: Configurable, auto-detects terminal colors

#### zellij (23k+ stars)
- **Pattern**: Tiling window manager for terminal
- **Layout**: Dynamic panes, tabs, floating windows
- **Navigation**: Modes (normal, pane, tab, resize, move)
- **Theme**: KDL config, ships multiple themes

### 4.2 Common UX Patterns Across Top TUIs

| Pattern | Used By | Implementation |
|---------|---------|----------------|
| **Status bar** | All | Bottom row: mode, file, branch, keybindings |
| **Breadcrumbs** | helix, zellij | Top row showing context path |
| **Command palette** | helix, zellij | Fuzzy-filtered popup (nucleo/fzf) |
| **Tab bar** | gitui, lazygit, zellij | Numbered tabs with highlight |
| **Floating windows** | lazygit, zellij | Centered overlay with shadow/border |
| **Split panes** | All | Horizontal/vertical splits via Layout |
| **Focus indicator** | All | Brighter border color on active panel |
| **Loading spinners** | gitui, lazygit | Braille or line spinner characters |
| **Progress bars** | bottom | Gauge with percentage + ETA |
| **Responsive resize** | All | `Constraint::Percentage` + min/max |
| **Keybinding hints** | All | Footer bar or popup showing available keys |

### 4.3 Responsive Layout Strategy

Best practice from the ecosystem:

```rust
// Responsive breakpoints based on terminal width
let layout = if area.width >= 120 {
    // Wide: 3-column layout
    Layout::horizontal([
        Constraint::Percentage(25),
        Constraint::Percentage(50),
        Constraint::Percentage(25),
    ])
} else if area.width >= 80 {
    // Medium: 2-column layout
    Layout::horizontal([
        Constraint::Percentage(35),
        Constraint::Percentage(65),
    ])
} else {
    // Narrow: single column, stacked
    Layout::vertical([
        Constraint::Percentage(40),
        Constraint::Percentage(60),
    ])
};
```

---

## 5. Visual Effects & Animation Techniques

### 5.1 Star Field / Space Background

No dedicated crate exists, but achievable with tachyonfx spatial patterns + manual rendering:

```rust
// Approach 1: Manual star field using Canvas widget
use ratatui::widgets::canvas::{Canvas, Points};

struct StarField {
    stars: Vec<(f64, f64, f64)>,  // x, y, brightness (0.0-1.0)
    speed: f64,
}

impl StarField {
    fn tick(&mut self) {
        for star in &mut self.stars {
            star.2 += self.speed;           // Move "toward" viewer
            if star.2 > 1.0 {
                *star = (rand_x(), rand_y(), 0.0);  // Reset
            }
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        let canvas = Canvas::default()
            .x_bounds([0.0, area.width as f64])
            .y_bounds([0.0, area.height as f64])
            .paint(|ctx| {
                for &(x, y, brightness) in &self.stars {
                    let grey = (brightness * 255.0) as u8;
                    ctx.print(x, y, Span::styled(
                        if brightness > 0.8 { "*" }
                        else if brightness > 0.5 { "." }
                        else { "," },
                        Style::new().fg(Color::Rgb(grey, grey, grey + 20)),
                    ));
                }
            });
        frame.render_widget(canvas, area);
    }
}

// Approach 2: Braille-based star field (higher resolution)
// Use Unicode Braille characters (U+2800-U+28FF) for 2x4 pixel blocks
// Each character cell becomes an 8-pixel grid
```

### 5.2 Gradient Text Rendering

```rust
use ratatui::text::{Line, Span};
use ratatui::style::{Color, Style};

/// Render text with a horizontal color gradient
fn gradient_text(text: &str, from: (u8, u8, u8), to: (u8, u8, u8)) -> Line<'_> {
    let len = text.len().max(1) as f32;
    Line::from(
        text.chars()
            .enumerate()
            .map(|(i, ch)| {
                let t = i as f32 / len;
                let r = lerp(from.0, to.0, t);
                let g = lerp(from.1, to.1, t);
                let b = lerp(from.2, to.2, t);
                Span::styled(
                    ch.to_string(),
                    Style::new().fg(Color::Rgb(r, g, b)),
                )
            })
            .collect::<Vec<_>>()
    )
}

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t) as u8
}

// Usage: SuperNovae gradient (peach -> mauve)
let title = gradient_text("SUPERNOVAE", (250, 179, 135), (203, 166, 247));
```

### 5.3 Modern Progress Bars

```rust
// Braille-resolution progress bar (8x finer than block chars)
fn braille_progress(progress: f64, width: u16) -> Line<'static> {
    let filled = (progress * width as f64 * 2.0) as usize;
    let full_blocks = filled / 2;
    let half = filled % 2;

    let mut spans = vec![];
    // Full blocks
    for _ in 0..full_blocks {
        spans.push(Span::styled("⣿", Style::new().fg(catppuccin_mocha::GREEN)));
    }
    // Half block
    if half > 0 {
        spans.push(Span::styled("⣷", Style::new().fg(catppuccin_mocha::GREEN)));
    }
    // Empty
    for _ in (full_blocks + half)..width as usize {
        spans.push(Span::styled("⣀", Style::new().fg(catppuccin_mocha::SURFACE0)));
    }
    Line::from(spans)
}
```

### 5.4 Animated Transitions Between Views

Using tachyonfx for view transitions:

```rust
use tachyonfx::{fx, EffectManager, Direction};

struct ViewTransition {
    effects: EffectManager,
}

impl ViewTransition {
    fn switch_to(&mut self, new_view: View) {
        // Fade out current view, slide in new view
        self.effects.add(fx::sequence(&[
            fx::fade_to_fg(Color::Rgb(17, 17, 27), 150),  // Fade to crust
            fx::slide_in(Direction::LeftToRight, 200),      // Slide in new
            fx::fade_from_fg(Color::Rgb(17, 17, 27), 150), // Fade in
        ]));
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        self.effects.tick(Instant::now());
        // Render current view with effects applied
    }
}
```

### 5.5 Loading/Spinner Animations

```rust
// Braille spinner (smooth 8-frame)
const BRAILLE_SPINNER: &[&str] = &["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];

// Dots spinner (modern feel)
const DOTS_SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

// Moon phase spinner
const MOON_SPINNER: &[&str] = &["🌑", "🌒", "🌓", "🌔", "🌕", "🌖", "🌗", "🌘"];

// Block building spinner
const BLOCK_SPINNER: &[&str] = &["▏", "▎", "▍", "▌", "▋", "▊", "▉", "█"];
```

---

## 6. Design Recommendations for Nika TUI

### 6.1 Immediate Wins (Low Effort, High Impact)

| Action | Crate | Effort |
|--------|-------|--------|
| Add Catppuccin Mocha palette constants | None (inline) | 1 hour |
| Replace hard-coded colors with palette | None | 2-3 hours |
| Add tui-big-text for splash header | `tui-big-text` | 1 hour |
| Add tui-scrollview for log/trace view | `tui-scrollview` | 2 hours |
| Gradient text for "NIKA" title | None (inline fn) | 30 min |

### 6.2 Medium-Term Enhancements

| Action | Crate | Effort |
|--------|-------|--------|
| tachyonfx view transitions | `tachyonfx` | 1-2 days |
| Splash screen with SuperNovae logo | `ratatui-splash-screen` | 1 day |
| Star field background (space theme) | Manual + Canvas | 1-2 days |
| Responsive layout breakpoints | None | 1 day |
| Theme switching (Catppuccin/Tokyo Night) | None (trait) | 1 day |

### 6.3 Long-Term Vision

| Action | Crate | Effort |
|--------|-------|--------|
| Full animation system with tachyonfx | `tachyonfx` | 1 week |
| ratatui-image for media preview | `ratatui-image` | 2-3 days |
| Floating command palette (nucleo) | Already have nucleo | 2-3 days |
| DAG visualization with Canvas | None | 1 week |
| Notification toast system | tachyonfx + popup | 2-3 days |

### 6.4 Cargo.toml Additions

```toml
# In [features] tui section, add:
# tui = ["dep:ratatui", "dep:crossterm", "dep:tui-input", ...,
#         "dep:tui-big-text", "dep:tui-scrollview", "dep:tachyonfx"]

# In [dependencies]:
tui-big-text = { version = "0.7", optional = true }
tui-scrollview = { version = "0.7", optional = true }
tui-popup = { version = "0.7", optional = true }
tachyonfx = { version = "0.11", optional = true }
ratatui-image = { version = "5", optional = true }      # For media preview
ratatui-splash-screen = { version = "0.1", optional = true }
```

---

## 7. Reference TUI Applications

### Showcase Gallery

The ratatui.rs website maintains an app showcase (https://ratatui.rs/showcase/) with
screenshots and descriptions. Notable apps built with ratatui include:

- **gitui** -- Git client (12k stars)
- **bottom** -- System monitor (10k stars)
- **spotify-tui** -- Spotify client
- **diskonaut** -- Disk usage explorer
- **bandwhich** -- Network utilization
- **kmon** -- Linux kernel module manager
- **gpg-tui** -- GPG key management
- **ox** -- Code editor
- **oatmeal** -- LLM terminal chat
- **Exabind** -- Keybinding manager (tachyonfx showcase, web demo via ratzilla)

### Design Inspiration Ranked

1. **Exabind** (https://junkdog.github.io/exabind/) -- Best animation/effects showcase
2. **gitui** -- Best multi-panel layout reference
3. **bottom** -- Best dashboard/chart layout
4. **helix** -- Best editor/command palette patterns
5. **lazygit** -- Best floating window + panel interaction

---

## Sources

1. https://ratatui.rs/highlights/v030/ -- Ratatui v0.30 release notes
2. https://github.com/ratatui/tui-widgets -- Official widget monorepo
3. https://github.com/ratatui/tachyonfx -- Effects and animation library
4. https://github.com/ratatui/ratatui-image -- Image rendering widget
5. https://github.com/orhun/ratatui-splash-screen -- Splash screen widget
6. https://github.com/ratatui/awesome-ratatui -- Curated ecosystem list
7. https://github.com/catppuccin/catppuccin -- Catppuccin palette source
8. https://github.com/tokyo-night/tokyo-night-vscode-theme -- Tokyo Night palette
9. https://ratatui.rs/ecosystem/tachyonfx/ -- TachyonFX official docs
10. https://lib.rs/crates/ratatui-image -- ratatui-image stats
11. https://lib.rs/crates/tui-widgets -- tui-widgets stats
12. https://lib.rs/crates/rat-widget -- rat-widget alternative
13. https://blog.orhun.dev/2025-wrapped/ -- Orhun's 2025 retrospective
14. https://crates.io/crates/tachyonfx/0.11.1 -- TachyonFX v0.11.1

## Methodology

- **Tools used**: Perplexity AI search (9 queries), crates.io metadata, GitHub repos
- **Pages analyzed**: 30+
- **Time period covered**: Jan 2025 -- Mar 2026
- **Cross-referenced**: Multiple sources for color hex values, crate versions

## Confidence Level

**High** -- Color palettes are verified against official repos. Crate versions confirmed
via crates.io. Architecture patterns validated against ratatui official templates and
existing Nika research docs. TachyonFX details confirmed via official ratatui ecosystem page.

## Relationship to Existing Research

This report complements the following existing Nika research docs:
- `ratatui-architecture-patterns-2025.md` -- Component trait architecture (still valid)
- `research-ratatui-widget-techniques.md` -- Border types, widget rendering (still valid)
- `terminal-ui-visual-techniques.md` -- Sci-fi inspiration (eDEX-UI, FUI patterns)
- `2026-03-20-evangelion-cosmic-hacker-tui-aesthetic.md` -- Aesthetic direction
- `2026-03-20-mission-control-ui-patterns.md` -- Dashboard layout patterns
