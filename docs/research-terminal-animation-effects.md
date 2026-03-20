# Research Report: Advanced Terminal Animation and Effects for Ratatui TUIs

**Date**: 2026-03-20
**Scope**: tachyonfx, ratatui ecosystem, Unicode visualization, terminal animation techniques
**Nika TUI**: ratatui 0.30.0 (compatible with tachyonfx 0.25.0)

---

## Summary

The terminal animation landscape for ratatui has matured dramatically. **tachyonfx v0.25.0** is the undisputed leader with 40+ composable effects, spatial patterns, a DSL, and WASM support. Combined with ratatui's built-in Canvas/Braille/HalfBlock/Octant rendering modes and the broader widget ecosystem (62,800+ tachyonfx downloads), Nika's TUI can achieve GPU-shader-level visual effects entirely in the terminal. This report catalogs every available technique.

---

## 1. tachyonfx v0.25.0 -- Complete Effect Reference

**Crate**: `tachyonfx = "0.25.0"` (requires `ratatui-core ^0.1.0`, compatible with `ratatui ^0.30.0`)
**Repo**: https://github.com/ratatui/tachyonfx (moved under ratatui org)
**Live demos**: https://junkdog.github.io/tachyonfx-ftl/

### 1.1 Color Effects

| Effect | Signature | Description |
|--------|-----------|-------------|
| `fade_from(fg, bg, timer)` | `C: Into<Color>, T: Into<EffectTimer>` | Fades from specified fg+bg colors |
| `fade_from_fg(fg, timer)` | same | Fades foreground from specified color |
| `fade_to(fg, bg, timer)` | same | Fades to specified fg+bg colors |
| `fade_to_fg(fg, timer)` | same | Fades foreground to specified color |
| `hsl_shift(fg_hsl, bg_hsl, timer)` | `Option<[f32;3]>` for each | Shifts H/S/L over duration |
| `hsl_shift_fg(hsl, timer)` | `[f32;3]` | Shifts foreground H/S/L |
| `paint(fg, bg, timer)` | static | Immediately paints fg+bg colors (no transition) |
| `paint_fg(fg, timer)` | static | Paints foreground only |
| `paint_bg(bg, timer)` | static | Paints background only |
| `lighten(fg, bg, timer)` | `Option<f32>` 0.0-1.0 | Increases lightness toward white |
| `lighten_fg(amount, timer)` | `f32` | Lightens foreground only |
| `darken(fg, bg, timer)` | `Option<f32>` 0.0-1.0 | Decreases lightness toward black |
| `darken_fg(amount, timer)` | `f32` | Darkens foreground only |
| `saturate(fg, bg, timer)` | `Option<f32>` | Adjusts saturation (-1.0 = grayscale, +1.0 = vivid) |
| `saturate_fg(amount, timer)` | `f32` | Saturates foreground only |

### 1.2 Text/Character Effects

| Effect | Description |
|--------|-------------|
| `dissolve(timer)` | Randomly removes foreground characters |
| `dissolve_to(style, timer)` | Dissolves text+bg to target style |
| `coalesce(timer)` | Reverse of dissolve -- characters reform |
| `coalesce_from(style, timer)` | Reforms from a specific style |
| `slide_in(direction, gradient_len, randomness, bg_color, timer)` | Slides content in with gradient |
| `slide_out(direction, gradient_len, randomness, bg_color, timer)` | Slides content out with gradient |
| `sweep_in(direction, gradient_len, randomness, faded_color, timer)` | Color sweep reveals content |
| `sweep_out(direction, gradient_len, randomness, faded_color, timer)` | Color sweep hides content |
| `evolve(symbol_set, timer)` | Transforms chars through progression (e.g., ` ` -> `░` -> `▒` -> `▓` -> `█`) |
| `evolve_into(symbol_set, timer)` | Evolves then reveals underlying content |
| `evolve_from(symbol_set, timer)` | Shows content then evolves away |
| `explode(force, force_rng, timer)` | Explodes content outward from center |

#### EvolveSymbolSet Variants

```
Circles:          [' ', '·', '•', '◉', '●']
CircleFill:       [' ', '◌', '◎', '◍', '●']
BlocksHorizontal: [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉', '█']
BlocksVertical:   [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█']
Quadrants:        [' ', '▖', '▘', '▗', '▝', '▚', '▞', '▙', '▛', '▜', '▟', '█']
Shaded:           [' ', '░', '▒', '▓', '█']
Squares:          [' ', '·', '▫', '▪', '◼', '█']
```

### 1.3 Geometry Effects

| Effect | Description |
|--------|-------------|
| `translate(effect, offset, timer)` | Moves effect area by (x, y) offset over time |
| `translate_buf(offset, aux_buffer, timer)` | Moves pre-rendered buffer content |
| `resize_area(effect, initial_size, timer)` | *(deprecated v0.19)* Resizes effect area |
| `stretch(direction, style, timer)` | Stretches using block chars (▏▎▍▌▋▊▉█) |
| `expand(direction, style, timer)` | Expands bidirectionally from center |

#### ExpandDirection

```rust
ExpandDirection::Horizontal  // left+right from center
ExpandDirection::Vertical    // up+down from center
```

### 1.4 Timing and Control Effects

| Effect | Description |
|--------|-------------|
| `parallel(effects)` | Runs effects simultaneously |
| `sequence(effects)` | Runs effects one after another |
| `sleep(duration)` | Pauses for duration |
| `delay(duration, effect)` | Delays effect start |
| `ping_pong(effect)` | Plays forward then backward (2x duration) |
| `repeat(effect, mode)` | Repeats by count, duration, or forever |
| `repeating(effect)` | Repeats indefinitely |
| `never_complete(effect)` | Makes effect run forever (freezes at end) |
| `timed_never_complete(duration, effect)` | Runs forever but marks complete after duration |
| `with_duration(duration, effect)` | Enforces a max duration |
| `prolong_start(duration, effect)` | Holds start state before playing |
| `prolong_end(duration, effect)` | Holds end state after completing |
| `freeze_at(alpha, set_raw, effect)` | Freezes at specific transition point |
| `remap_alpha(start, end, effect)` | Remaps alpha range (e.g., play only 10%-50% of transition) |
| `consume_tick()` | Single-tick effect |
| `run_once(effect)` | Ensures effect runs exactly once |

#### RepeatMode

```rust
RepeatMode::Forever          // loops indefinitely
RepeatMode::Times(n)         // loops n times
RepeatMode::Duration(d)      // loops for duration d
```

### 1.5 Advanced/Custom Effects

| Effect | Description |
|--------|-------------|
| `effect_fn(state, timer, closure)` | Custom per-cell effect via `CellIterator` |
| `effect_fn_buf(state, timer, closure)` | Custom buffer-level effect |
| `offscreen_buffer(effect, buffer)` | Renders to separate buffer for compositing |
| `dynamic_area(effect)` | Wraps effects for responsive layouts |
| `dispatch_event()` | Dispatches events when effects start |
| `Glitch::builder()` | Glitch effect with configurable ratio, delay, duration |

### 1.6 Spatial Patterns (v0.19+)

Patterns transform global alpha into position-specific alpha for spatial progression.
Applied with `.with_pattern(pattern)`.

| Pattern | Description |
|---------|-------------|
| `SweepPattern::left_to_right(width)` | Directional sweep with gradient |
| `SweepPattern::right_to_left(width)` | Reverse sweep |
| `SweepPattern::up_to_down(width)` | Vertical sweep down |
| `SweepPattern::down_to_up(width)` | Vertical sweep up |
| `RadialPattern::center()` | Circular progression from center |
| `RadialPattern::new(cx, cy)` | Radial from custom point (0.0-1.0) |
| `RadialPattern::with_transition(center, width)` | Radial with custom gradient |
| `WavePattern::new(frequency, amplitude)` | Wave interference pattern |
| `DiagonalPattern::new(direction)` | Diagonal sweep |
| `DiamondPattern` | Diamond-shaped progression |
| `SpiralPattern` | Spiral from center outward |
| `CheckerboardPattern` | Alternating cell activation |
| `DissolvePattern::new()` | Random scatter activation |
| `CoalescePattern` | Reform pattern (reverse dissolve) |
| `BlendPattern` | Crossfade between two patterns |
| `CombinedPattern` | Combine patterns with operations |
| `InvertedPattern` | Negates another pattern |
| `InstancedPattern` | Per-cell alpha mapping |

### 1.7 CellFilter System

Selectively apply effects to specific terminal cells:

```rust
CellFilter::All                          // every cell
CellFilter::Text                         // only text (non-whitespace)
CellFilter::FgColor(Color::Red)          // cells with specific fg
CellFilter::BgColor(Color::Black)        // cells with specific bg
CellFilter::Inner(Margin::new(1,1))      // inside margin
CellFilter::Outer(Margin::new(1,1))      // outside margin (borders)
CellFilter::AllOf(vec![f1, f2])          // AND combination
CellFilter::AnyOf(vec![f1, f2])          // OR combination
CellFilter::NoneOf(vec![f1, f2])         // NOR combination
CellFilter::Not(Box::new(filter))        // negation
CellFilter::Layout(layout, index)        // ratatui Layout segment
CellFilter::PositionFn(fn)              // custom predicate
```

### 1.8 Interpolation (32 Easing Functions)

| Category | In | Out | InOut |
|----------|-----|-----|-------|
| Back | `BackIn` | `BackOut` | `BackInOut` |
| Bounce | `BounceIn` | `BounceOut` | `BounceInOut` |
| Circular | `CircIn` | `CircOut` | `CircInOut` |
| Cubic | `CubicIn` | `CubicOut` | `CubicInOut` |
| Elastic | `ElasticIn` | `ElasticOut` | `ElasticInOut` |
| Exponential | `ExpoIn` | `ExpoOut` | `ExpoInOut` |
| Quadratic | `QuadIn` | `QuadOut` | `QuadInOut` |
| Quartic | `QuartIn` | `QuartOut` | `QuartInOut` |
| Quintic | `QuintIn` | `QuintOut` | `QuintInOut` |
| Sine | `SineIn` | `SineOut` | `SineInOut` |
| Special | `Linear` | `Reverse` | -- |

### 1.9 EffectTimer Ergonomics

```rust
fx::fade_to_fg(Color::Red, 1000);                          // 1s, linear
fx::fade_to_fg(Color::Red, (1000, Interpolation::BounceOut)); // 1s, bouncy
fx::fade_to_fg(Color::Red, Duration::from_secs(2));         // 2s, linear
fx::fade_to_fg(Color::Red, EffectTimer::from_ms(1000, Interpolation::Linear)); // explicit
```

### 1.10 Integration Pattern (EffectRenderer)

```rust
// In your render function:
use tachyonfx::{EffectRenderer, Shader};

// 1. Render widget normally
my_widget.render(area, buf);

// 2. Apply effects AFTER rendering
buf.render_effect(&mut self.effect, area, last_tick);

// For pre-render effects (translate, slide_in, resize):
// Apply BEFORE rendering, then get recalculated area:
buf.render_effect(&mut self.pre_fx, area, last_tick);
let actual_area = self.pre_fx.area().unwrap_or(area);
my_widget.render(actual_area, buf);
```

### 1.11 EffectTimeline Widget

Visualize effect composition trees in the terminal:

```rust
use tachyonfx::widget::EffectTimeline;

let timeline = EffectTimeline::builder()
    .effect(&my_complex_effect)
    .hue(0.0..360.0)
    .saturation(52.0)
    .lightness(62.0)
    .build();

// Renders a Gantt-chart-like visualization of effect timing
timeline.render(area, buf);

// Or save to file:
timeline.save_to_file("timeline.txt", 110)?;
```

### 1.12 Features

```toml
[dependencies]
tachyonfx = { version = "0.25.0", features = ["dsl", "sendable"] }
```

| Feature | Description |
|---------|-------------|
| `std` (default) | Standard library support |
| `dsl` (default) | Effect DSL parser (via `anpa` crate) |
| `sendable` | Makes effects `Send` for cross-thread use |
| `std-duration` | Uses `std::time::Duration` instead of custom 32-bit |
| `wasm` | WebAssembly support (via `web-time`) |

---

## 2. Composition Recipes

### 2.1 Glitchy Window Open (from tachyonfx examples)

```rust
let margin = Margin::new(1, 1);
let border_text = CellFilter::AllOf(vec![
    CellFilter::Outer(margin),
    CellFilter::Text,
]);
let border_decorations = CellFilter::AllOf(vec![
    CellFilter::Outer(margin),
    CellFilter::Not(Box::new(CellFilter::Text)),
]);

repeating(parallel(&[
    // Window borders: dissolve then coalesce with bounce
    parallel(&[
        sequence(&[
            with_duration(220.into(), never_complete(fx::dissolve(0))),
            fx::coalesce((320, BounceOut)),
        ]),
        fx::fade_from(gray, gray, 640),
    ]).with_cell_selection(border_decorations),

    // Title: hide then fade in
    sequence(&[
        with_duration(640.into(), never_complete(fx::fade_to(gray, gray, 0))),
        fx::fade_from(gray, gray, (640, QuadOut)),
    ]).with_cell_selection(border_text),

    // Content: dissolve, coalesce, pause, dissolve out
    sequence(&[
        with_duration(540.into(), parallel(&[
            never_complete(fx::dissolve(0)),
            never_complete(fx::fade_to(bg, bg, 0)),
        ])),
        parallel(&[
            fx::coalesce(440),
            fx::fade_from(bg, bg, (500, QuadOut)),
        ]),
        fx::sleep(3000),
        parallel(&[
            fx::fade_to(bg, bg, (500, BounceIn)),
            fx::dissolve((440, ElasticOut)),
        ]),
    ]).with_cell_selection(CellFilter::Inner(margin)),
]));
```

### 2.2 Fire Effect (pattern + evolve + translate)

```rust
let fire_style = Style::default()
    .bg(Color::from_u32(0x32302F))
    .fg(Color::from_u32(0x1D2021));

let timer = (1000, Interpolation::QuadIn);
let inner = fx::evolve_from((EvolveSymbolSet::Quadrants, fire_style), timer)
    .with_pattern(DissolvePattern::new());
fx::translate(inner, Offset { x: 0, y: -8 }, timer);
```

### 2.3 Rainbow Cycle (custom effect_fn, never-ending)

```rust
fx::never_complete(fx::effect_fn(Instant::now(), 1000, |state, _ctx, cell_iter| {
    let cycle: f32 = (state.elapsed().as_millis() % 3600) as f32;
    cell_iter
        .filter(|(_, cell)| cell.symbol() != " ")
        .enumerate()
        .for_each(|(i, (_pos, cell))| {
            let hue = (2.0 * i as f32 + cycle * 0.2) % 360.0;
            cell.set_fg(Color::from_hsl(hue as f64, 100.0, 50.0));
        });
}));
```

### 2.4 View Transition (slide + fade)

```rust
// Slide current view out, slide new view in
let transition_out = fx::parallel(&[
    fx::slide_out(Motion::LeftToRight, 10, 0, Color::Black, (400, CubicInOut)),
    fx::fade_to(Color::Black, Color::Black, (300, CubicIn)),
]);

let transition_in = fx::parallel(&[
    fx::slide_in(Motion::RightToLeft, 10, 0, Color::Black, (400, CubicInOut)),
    fx::fade_from(Color::Black, Color::Black, (300, CubicOut)),
]);

// Sequence them with a brief pause
fx::sequence(&[transition_out, fx::sleep(50), transition_in]);
```

### 2.5 Pulsing Shimmer (ping_pong + lighten + sweep)

```rust
fx::repeating(fx::ping_pong(
    fx::lighten_fg(0.4, (800, Interpolation::SineInOut))
        .with_pattern(SweepPattern::left_to_right(15))
));
```

---

## 3. Unicode Art Techniques for Data Visualization

### 3.1 Braille Dot Patterns (2x4 resolution per cell)

ratatui's `Canvas` widget with `Marker::Braille` provides 2x4 dot resolution per terminal cell.
The 256 braille characters (U+2800..U+28FF) each represent an 8-bit pattern.

```
Bit layout per cell:     Characters:
[0][3]                   ⠀ = 0x00 (blank)
[1][4]                   ⠁ = bit 0 only
[2][5]                   ⣿ = 0xFF (all dots)
[6][7]
```

**Use in ratatui**:
```rust
use ratatui::widgets::canvas::{Canvas, Line, Points};
use ratatui::symbols::Marker;

Canvas::default()
    .marker(Marker::Braille)    // 2x4 resolution
    .x_bounds([0.0, 100.0])
    .y_bounds([0.0, 50.0])
    .paint(|ctx| {
        ctx.draw(&Points {
            coords: &data_points,
            color: Color::Cyan,
        });
    });
```

### 3.2 Block Elements (8-level vertical bars)

```
Vertical bars:  ▁ ▂ ▃ ▄ ▅ ▆ ▇ █    (U+2581..U+2588)
Horizontal bars: ▏ ▎ ▍ ▌ ▋ ▊ ▉ █   (U+258F..U+2588)
Shading:        ░ ▒ ▓ █              (U+2591..U+2593, U+2588)
```

ratatui's `Sparkline` widget uses `symbols::bar::NINE_LEVELS`:
```rust
Sparkline::default()
    .data(&[0, 2, 3, 4, 1, 4, 10])
    .max(10)
    .bar_set(symbols::bar::NINE_LEVELS)  // ▁▂▃▄▅▆▇█
    .style(Style::default().red());
```

### 3.3 Half-Block Rendering (2x vertical resolution)

`Marker::HalfBlock` uses `▀` (upper half) and `▄` (lower half) plus `█` (full) to achieve 2 vertical pixels per cell. Because terminal cells are ~2:1 height:width, this creates square pixels.

```rust
Canvas::default()
    .marker(Marker::HalfBlock)  // square pixels
    .paint(|ctx| { /* ... */ });
```

### 3.4 Quadrant Characters (2x2 resolution)

`Marker::Quadrant` uses `▖▗▘▝▀▄▌▐▙▛▜▟█` for 2x2 pixel grid per cell.

### 3.5 Sextant Characters (2x3 resolution)

`Marker::Sextant` from Unicode Symbols for Legacy Computing Supplement provides 2x3 resolution per cell. More recent addition, less broadly supported.

### 3.6 Octant Characters (2x4 resolution, block-based)

`Marker::Octant` provides same 2x4 resolution as Braille but with solid block fills instead of dots -- no visible gaps between cells.

### 3.7 Box Drawing Characters

```
Light:   ─ │ ┌ ┐ └ ┘ ├ ┤ ┬ ┴ ┼
Heavy:   ━ ┃ ┏ ┓ ┗ ┛ ┣ ┫ ┳ ┻ ╋
Double:  ═ ║ ╔ ╗ ╚ ╝ ╠ ╣ ╦ ╩ ╬
Rounded: ╭ ╮ ╰ ╯
Mixed:   ┍ ┑ ┕ ┙ (heavy top, light bottom, etc.)
```

ratatui's `BorderType` enum:
```rust
BorderType::Plain     // ─│┌┐└┘
BorderType::Rounded   // ─│╭╮╰╯
BorderType::Double    // ═║╔╗╚╝
BorderType::Thick     // ━┃┏┓┗┛
BorderType::QuadrantInside   // quadrant chars
BorderType::QuadrantOutside  // quadrant chars (inverted)
```

### 3.8 Powerline-Style Separators

```
Triangles:      (U+E0B0)  (U+E0B2)
Soft triangles: (U+E0B1)  (U+E0B3)
Flames:         (U+E0C0)  (U+E0C2)
Pixelated:      (U+E0C4)  (U+E0C6)
Rounded:        (U+E0B4)  (U+E0B6)
```

Or use standard Unicode:
```
▶ ◀ ▷ ◁          Arrow heads
◢ ◣ ◤ ◥          Triangle quadrants
╱ ╲               Diagonal lines
```

---

## 4. Color Gradient Techniques

### 4.1 True-Color HSL Gradients

```rust
// Horizontal gradient across a line
for x in 0..width {
    let hue = (x as f64 / width as f64) * 360.0;
    let color = Color::from_hsl(hue, 80.0, 55.0);
    buf.cell_mut(Position::new(area.x + x, y))
        .unwrap()
        .set_bg(color);
}
```

### 4.2 Animated Color Cycles (via tachyonfx)

```rust
// Infinite rainbow cycle
fx::repeating(fx::ping_pong(
    fx::hsl_shift_fg([360.0, 0.0, 0.0], (3000, Interpolation::Linear))
));
```

### 4.3 Per-Cell Color Mapping (effect_fn)

```rust
fx::never_complete(fx::effect_fn(Instant::now(), 0, |state, _ctx, cell_iter| {
    let t = state.elapsed().as_millis() as f32;
    cell_iter.enumerate().for_each(|(i, (pos, cell))| {
        let hue = ((pos.x as f32 * 7.0 + pos.y as f32 * 13.0 + t * 0.1) % 360.0);
        cell.set_bg(Color::from_hsl(hue as f64, 60.0, 15.0));  // subtle bg
        cell.set_fg(Color::from_hsl(hue as f64, 90.0, 70.0));  // bright fg
    });
}));
```

### 4.4 Gradient Fills with Block Elements

Combine half-block chars with foreground/background colors for double-resolution gradients:

```rust
// Each cell shows TWO colors using ▀ (upper half block)
// fg = top pixel color, bg = bottom pixel color
for y in (0..height).step_by(2) {
    for x in 0..width {
        let color_top = gradient_color(x, y);
        let color_bot = gradient_color(x, y + 1);
        let cell = buf.cell_mut(Position::new(area.x + x, area.y + y / 2)).unwrap();
        cell.set_symbol("▀");
        cell.set_fg(color_top);
        cell.set_bg(color_bot);
    }
}
```

---

## 5. Real-Time Chart Rendering

### 5.1 Sparklines (built-in)

```rust
Sparkline::default()
    .data(&rolling_buffer)
    .max(max_value)
    .direction(RenderDirection::RightToLeft)
    .style(Style::default().green())
    .absent_value_style(Style::default().dark_gray())
    .absent_value_symbol("·");
```

### 5.2 Canvas Charts (Braille/HalfBlock)

```rust
// High-resolution line chart with Braille
Canvas::default()
    .marker(Marker::Braille)
    .x_bounds([0.0, data.len() as f64])
    .y_bounds([min_val, max_val])
    .paint(|ctx| {
        // Draw line segments between points
        for window in data.windows(2) {
            ctx.draw(&canvas::Line {
                x1: window[0].0,
                y1: window[0].1,
                x2: window[1].0,
                y2: window[1].1,
                color: Color::Cyan,
            });
        }
    });
```

### 5.3 Area Charts

Fill below the line using Canvas with filled rectangles or custom Shape:

```rust
impl Shape for FilledArea {
    fn draw(&self, painter: &mut Painter) {
        for x in 0..self.data.len() {
            let y_val = self.data[x];
            // Draw vertical line from 0 to y_val at each x
            for y_step in 0..=(y_val * resolution) as i32 {
                if let Some((cx, cy)) = painter.get_point(x as f64, y_step as f64 / resolution) {
                    painter.paint(cx, cy, self.color);
                }
            }
        }
    }
}
```

### 5.4 Heatmaps

```rust
// Use background colors on space characters for heatmap cells
for (row_idx, row) in heatmap_data.iter().enumerate() {
    for (col_idx, &value) in row.iter().enumerate() {
        let intensity = value / max_value;
        // Cool-to-hot gradient: blue -> cyan -> green -> yellow -> red
        let hue = (1.0 - intensity) * 240.0;  // 240=blue, 0=red
        let color = Color::from_hsl(hue as f64, 90.0, 45.0);
        let cell = buf.cell_mut(pos).unwrap();
        cell.set_symbol(" ");
        cell.set_bg(color);
    }
}
```

### 5.5 Candlestick Charts

Combine box drawing and block elements:
```
  │        <- wick (thin line using │)
  ┃        <- body top (using ┃ for thick)
  █        <- body (filled block)
  █
  ┃        <- body bottom
  │        <- wick
```

Green/red coloring for up/down candles.

---

## 6. Progress Bar Animations

### 6.1 Wave/Pulse Progress Bar

```rust
fx::repeating(
    fx::lighten_fg(0.3, (600, SineInOut))
        .with_pattern(SweepPattern::left_to_right(8))
        .with_cell_selection(CellFilter::BgColor(accent_color))
);
```

### 6.2 Shimmer Effect (tui-shimmer crate)

```toml
tui-shimmer = "0.1.3"
```

```rust
use tui_shimmer::shimmer_spans_with_style;

let spans = shimmer_spans_with_style("Loading...", base_style);
// Returns Vec<Span> with per-character color animation
// Uses cosine-based intensity with sweeping highlight band
```

### 6.3 Throbber/Spinner Animations (throbber-widgets-tui)

```toml
throbber-widgets-tui = "0.11.0"
```

Available spinner sets:
```
ASCII:            | / - \
BoxDrawing:       │ ╱ ─ ╲
Arrow:            ↑ ↗ → ↘ ↓ ↙ ← ↖
VerticalBlock:    ▁ ▂ ▃ ▄ ▅ ▆ ▇ █
HorizontalBlock:  ▏ ▎ ▍ ▌ ▋ ▊ ▉ █
QuadrantBlock:    ▝ ▗ ▖ ▘
BlackCircle:      ◑ ◒ ◐ ◓
Braille6Dots:     ⠁ ⠂ ⠄ ⡀ ⢀ ⠠ ⠐ ⠈
OghamA:           ᚁ ᚂ ᚃ ᚄ ᚅ
```

### 6.4 Block-Element Progress with Sub-Cell Precision

Use horizontal block elements for 1/8th-cell precision:

```rust
fn render_precise_bar(area: Rect, buf: &mut Buffer, progress: f64) {
    let total_eighths = (area.width as f64 * 8.0 * progress) as u32;
    let full_cells = total_eighths / 8;
    let remainder = total_eighths % 8;

    // Fill complete cells
    for x in 0..full_cells as u16 {
        buf.cell_mut(Position::new(area.x + x, area.y))
            .unwrap()
            .set_symbol("█");
    }

    // Partial cell using block elements
    if remainder > 0 && (full_cells as u16) < area.width {
        let partial = match remainder {
            1 => "▏", 2 => "▎", 3 => "▍", 4 => "▌",
            5 => "▋", 6 => "▊", 7 => "▉", _ => " ",
        };
        buf.cell_mut(Position::new(area.x + full_cells as u16, area.y))
            .unwrap()
            .set_symbol(partial);
    }
}
```

---

## 7. Terminal Animation Techniques

### 7.1 Smooth Scrolling

Use `translate` effect or manually interpolate scroll offset with fractional positioning:

```rust
// Smooth scroll using tachyonfx translate
let scroll_effect = fx::translate(
    content_effect,
    Offset { x: 0, y: -scroll_lines },
    (200, Interpolation::CubicOut)
);

// Manual sub-line scrolling with half-blocks
// Render at offset, use ▀/▄ for half-line precision at scroll edges
```

### 7.2 View Transitions

```rust
struct ViewTransition {
    current: ViewId,
    target: ViewId,
    slide_out: Effect,
    slide_in: Effect,
    transitioning: bool,
}

// Trigger transition:
fn switch_view(&mut self, target: ViewId) {
    let bg = Color::from_u32(0x1d2021);
    self.slide_out = fx::parallel(&[
        fx::slide_out(Motion::LeftToRight, 15, 2, bg, (350, CubicIn)),
        fx::fade_to(bg, bg, (250, QuadIn)),
    ]);
    self.slide_in = fx::parallel(&[
        fx::slide_in(Motion::RightToLeft, 15, 2, bg, (350, CubicOut)),
        fx::fade_from(bg, bg, (250, QuadOut)),
    ]);
    self.transitioning = true;
}
```

### 7.3 Panel Resize Animations

```rust
// Animated panel resize using stretch/expand
let resize_fx = fx::expand(
    ExpandDirection::Horizontal,
    Style::default().bg(panel_bg),
    (300, Interpolation::CubicInOut)
);

// Or manual interpolation of Layout constraints:
let split_pct = lerp(old_pct, new_pct, eased_alpha);
let layout = Layout::horizontal([
    Constraint::Percentage(split_pct),
    Constraint::Percentage(100 - split_pct),
]).split(area);
```

### 7.4 Particle Effects

```rust
// Particle system via effect_fn_buf
struct Particle {
    x: f32, y: f32,
    vx: f32, vy: f32,
    life: f32,
    color: Color,
}

fx::never_complete(fx::effect_fn_buf(
    (particles, Instant::now()),
    0,
    |(particles, last_time), ctx, buf| {
        let dt = last_time.elapsed().as_secs_f32();
        *last_time = Instant::now();

        for p in particles.iter_mut() {
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.vy += 9.8 * dt;  // gravity
            p.life -= dt;

            if p.life > 0.0 && ctx.area.contains(Position::new(p.x as u16, p.y as u16)) {
                let cell = &mut buf[Position::new(p.x as u16, p.y as u16)];
                let alpha = (p.life / PARTICLE_MAX_LIFE).clamp(0.0, 1.0);
                cell.set_symbol(if alpha > 0.5 { "●" } else { "·" });
                cell.set_fg(p.color);
            }
        }
    }
));
```

---

## 8. Ecosystem Crates

| Crate | Version | Downloads | Purpose |
|-------|---------|-----------|---------|
| **tachyonfx** | 0.25.0 | 62.8k | Shader-like terminal effects |
| **throbber-widgets-tui** | 0.11.0 | 504k | Spinner/throbber widgets |
| **tui-big-text** | 0.8.2 | 320k | Large ASCII art text |
| **ratatui-image** | 10.0.6 | 298k | Sixel/Kitty/iTerm2 images |
| **tui-scrollview** | 0.6.2 | 291k | Scrollable views |
| **tui-shimmer** | 0.1.3 | 0.7k | Shimmer text animation |
| **ratatui-splash-screen** | 0.1.5 | 60k | Image-to-splash-screen |
| **bevy_ratatui** | 0.11.1 | 51k | Bevy ECS integration |
| **ratzilla** | 0.3.0 | 50k | Ratatui for web/WASM |

---

## 9. Nika TUI Integration Notes

### 9.1 Dependency Addition

```toml
# In tools/nika/Cargo.toml [dependencies]
tachyonfx = { version = "0.25.0", optional = true, features = ["dsl"] }
```

Add to the `tui` feature gate.

### 9.2 Architecture Pattern

```rust
struct NikaView {
    // Each view holds its own effects
    enter_fx: Option<Effect>,
    exit_fx: Option<Effect>,
    ambient_fx: Option<Effect>,  // always-running (glitch, shimmer)
    last_frame: Instant,
}

impl NikaView {
    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let tick = self.last_frame.elapsed().into();
        self.last_frame = Instant::now();

        // Pre-render effects (geometry)
        if let Some(fx) = &mut self.enter_fx {
            frame.render_effect(fx, area, tick);
            if fx.done() { self.enter_fx = None; }
        }

        // Render widgets...
        self.render_content(frame, area);

        // Post-render effects (color/text)
        if let Some(fx) = &mut self.ambient_fx {
            frame.render_effect(fx, area, tick);
        }
    }
}
```

### 9.3 View Transition System for Nika's 4 TUI Views

```
1/s Studio | 2/r Runner | 3/c Chat | 4/, Settings
```

Each view switch can trigger:
1. `slide_out` + `fade_to` on current view
2. `slide_in` + `fade_from` + `coalesce` on new view
3. Direction based on view order (left/right)

---

## Sources

1. **tachyonfx v0.25.0 source** (GitHub tag `tachyonfx-v0.25.0`) -- Complete API, patterns, effects
   - https://github.com/ratatui/tachyonfx
2. **tachyonfx interactive demos** -- Live WASM demos of every effect
   - https://junkdog.github.io/tachyonfx-ftl/
3. **ratatui v0.30.0 symbols module** -- Braille, block, half_block, marker definitions
   - https://github.com/ratatui/ratatui/tree/main/ratatui-core/src/symbols
4. **ratatui Canvas widget** -- Braille/HalfBlock/Quadrant/Sextant/Octant rendering
   - https://github.com/ratatui/ratatui/blob/main/ratatui-widgets/src/canvas.rs
5. **tui-shimmer v0.1.3** -- Shimmer sweep effect
   - https://github.com/vinhnx/tui-shimmer
6. **throbber-widgets-tui v0.11.0** -- Spinner symbol sets
   - https://github.com/arkbig/throbber-widgets-tui
7. **crates.io search results** -- Ecosystem survey (ratatui animation, widget, chart crates)

## Methodology

- Tools used: GitHub API, raw file fetching from repos, crates.io API
- Files analyzed: ~25 source files across tachyonfx, ratatui-core, tui-shimmer, throbber-widgets-tui
- Version verified: tachyonfx 0.25.0 confirmed compatible with ratatui 0.30.0

## Confidence Level

**High** -- All effect signatures verified from source code. Pattern system verified from v0.25.0 tagged release. Composition recipes validated from working example code in the repository. The tachyonfx crate has moved under the official ratatui GitHub organization, confirming its status as the canonical terminal effects library.

## Key Findings for Nika

1. **tachyonfx 0.25.0 is a direct drop-in** for ratatui 0.30.0 -- zero version conflicts
2. **Spatial patterns** (radial, wave, spiral, diamond) are the game-changer since v0.19 -- they turn basic fades into cinematic reveals
3. **evolve + patterns** can create fire, matrix rain, loading screens with just 2-3 lines
4. The **DSL feature** means effects could eventually be defined in `.nika.yaml` workflows
5. **Glitch effect** is perfect for error states; **shimmer/lighten** for loading states
6. **Octant marker** (2x4 block-based) is the highest-fidelity chart rendering mode available
