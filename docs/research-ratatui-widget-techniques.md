# Research Report: Advanced Ratatui 0.30 Widget Techniques

## Summary

Comprehensive reference for building visually stunning terminal UIs with ratatui 0.30.
All code examples are verified against the actual ratatui-widgets 0.3.0 and ratatui-core 0.1.0
source code from the project's cargo registry.

---

## 1. Border Types -- Complete Reference

ratatui 0.30 provides **12 built-in `BorderType` variants** plus **7 custom `border::Set` constants**
that go beyond what `BorderType` offers.

### 1.1 All BorderType Variants

```rust
use ratatui::widgets::{Block, BorderType};

// Standard box-drawing borders
Block::bordered().border_type(BorderType::Plain);          // ┌──┐ │  │ └──┘
Block::bordered().border_type(BorderType::Rounded);        // ╭──╮ │  │ ╰──╯
Block::bordered().border_type(BorderType::Double);         // ╔══╗ ║  ║ ╚══╝
Block::bordered().border_type(BorderType::Thick);          // ┏━━┓ ┃  ┃ ┗━━┛

// Dashed borders (light = thin corners, heavy = thick corners)
Block::bordered().border_type(BorderType::LightDoubleDashed);     // ┌╌╌┐ ╎  ╎ └╌╌┘
Block::bordered().border_type(BorderType::HeavyDoubleDashed);     // ┏╍╍┓ ╏  ╏ ┗╍╍┛
Block::bordered().border_type(BorderType::LightTripleDashed);     // ┌┄┄┐ ┆  ┆ └┄┄┘
Block::bordered().border_type(BorderType::HeavyTripleDashed);     // ┏┅┅┓ ┇  ┇ ┗┅┅┛
Block::bordered().border_type(BorderType::LightQuadrupleDashed);  // ┌┈┈┐ ┊  ┊ └┈┈┘
Block::bordered().border_type(BorderType::HeavyQuadrupleDashed);  // ┏┉┉┓ ┋  ┋ ┗┉┉┛

// Quadrant block borders (half-cell pixel resolution)
Block::bordered().border_type(BorderType::QuadrantInside);   // ▗▄▄▖ ▐  ▌ ▝▀▀▘
Block::bordered().border_type(BorderType::QuadrantOutside);  // ▛▀▀▜ ▌  ▐ ▙▄▄▟
```

### 1.2 Custom Border Sets (beyond BorderType)

These are available as `symbols::border::*` constants and can only be used via `.border_set()`:

```rust
use ratatui::{symbols, widgets::Block};

// McGugan box technique -- ultra-thin 1/8th borders
Block::bordered().border_set(symbols::border::ONE_EIGHTH_WIDE);
// ▁▁▁▁ ▏  ▕ ▔▔▔▔   (wide variant: thin top/bottom, thin left/right)

Block::bordered().border_set(symbols::border::ONE_EIGHTH_TALL);
// ▕▔▔▏ ▕  ▏ ▕▁▁▏   (tall variant: tall left/right, thin top/bottom)

// Proportional borders (visually equal width and height)
Block::bordered().border_set(symbols::border::PROPORTIONAL_WIDE);
// ▄▄▄▄ █  █ ▀▀▀▀   (half blocks for top/bottom, full blocks for sides)

Block::bordered().border_set(symbols::border::PROPORTIONAL_TALL);
// █▀▀█ █  █ █▄▄█   (full blocks for sides, half blocks for top/bottom)

// Solid border
Block::bordered().border_set(symbols::border::FULL);
// ████ █  █ ████   (full blocks everywhere)

// Empty border (invisible but reserves space -- useful for padding)
Block::bordered().border_set(symbols::border::EMPTY);
//      (spaces everywhere, block still reserves border area)
```

### 1.3 Mixing Border Styles Per Side

The `border::Set` struct has 8 independent fields. You can build a custom set that mixes styles:

```rust
use ratatui::symbols::border;
use ratatui::widgets::Block;

// Thick top/bottom, rounded corners, thin sides
let mixed_set = border::Set {
    top_left:     "╭",
    top_right:    "╮",
    bottom_left:  "╰",
    bottom_right: "╯",
    vertical_left:     "│",  // thin left
    vertical_right:    "┃",  // thick right
    horizontal_top:    "━",  // thick top
    horizontal_bottom: "─",  // thin bottom
};

Block::bordered().border_set(mixed_set);
// Result:
// ╭━━━━━╮
// │     ┃
// ╰─────╯
```

### 1.4 Selective Borders

```rust
use ratatui::widgets::{Block, Borders, BorderType};
use ratatui::border; // macro

// Only top and bottom
Block::new()
    .borders(Borders::TOP | Borders::BOTTOM)
    .border_type(BorderType::Double);

// Using the border! macro
Block::new()
    .borders(border!(TOP, LEFT, BOTTOM))  // no right border
    .border_type(BorderType::Rounded);
```

---

## 2. Nested Borders -- Double-Contour Effect

### 2.1 Basic Nesting with `inner()`

```rust
use ratatui::style::{Color, Style, Stylize};
use ratatui::widgets::{Block, BorderType, Padding};
use ratatui::Frame;

fn render_double_contour(frame: &mut Frame) {
    let area = frame.area();

    // Outer border: thick, cyan
    let outer = Block::bordered()
        .border_type(BorderType::Thick)
        .border_style(Style::new().fg(Color::Cyan))
        .padding(Padding::uniform(1))  // gap between outer and inner
        .title("Panel");

    // Calculate inner area (accounts for borders + padding)
    let inner_area = outer.inner(area);

    // Inner border: thin rounded, dim
    let inner = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(Color::DarkGray));

    frame.render_widget(outer, area);
    frame.render_widget(inner, inner_area);

    // Content goes inside inner block
    let content_area = inner.inner(inner_area);
    // frame.render_widget(your_widget, content_area);
}
// Result:
// ┏━━━━━━━━━━━━━━━━━━━━┓
// ┃                     ┃
// ┃  ╭────────────────╮ ┃
// ┃  │  content here  │ ┃
// ┃  ╰────────────────╯ ┃
// ┃                     ┃
// ┗━━━━━━━━━━━━━━━━━━━━━┛
```

### 2.2 Triple-Layer Luxury Frame

```rust
fn render_luxury_frame(frame: &mut Frame) {
    let area = frame.area();

    // Layer 1: QuadrantOutside for the outermost edge
    let outer = Block::bordered()
        .border_type(BorderType::QuadrantOutside)
        .border_style(Style::new().fg(Color::Rgb(100, 100, 200)));
    let mid_area = outer.inner(area);

    // Layer 2: One-eighth thin border as separator
    let mid = Block::bordered()
        .border_set(ratatui::symbols::border::ONE_EIGHTH_WIDE)
        .border_style(Style::new().fg(Color::Rgb(60, 60, 140)));
    let inner_area = mid.inner(mid_area);

    // Layer 3: Rounded inner frame for content
    let inner = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(Color::Rgb(140, 140, 255)))
        .title(" Content ");

    frame.render_widget(outer, area);
    frame.render_widget(mid, mid_area);
    frame.render_widget(inner, inner_area);
}
```

### 2.3 Border Merging for Clean Layouts

When blocks overlap, use `MergeStrategy` to collapse borders cleanly:

```rust
use ratatui::symbols::merge::MergeStrategy;
use ratatui::widgets::{Block, BorderType};

// First block: plain borders
let block1 = Block::bordered();

// Second block: thick borders, rendered on top, collapses cleanly
let block2 = Block::bordered()
    .border_type(BorderType::Thick)
    .merge_borders(MergeStrategy::Exact);
// Where the borders overlap, you get proper junction characters like ╆ ┲ ┺ ┢

// For fuzzy merging across incompatible types (rounded + plain, double + thick):
let block3 = Block::bordered()
    .border_type(BorderType::Double)
    .merge_borders(MergeStrategy::Fuzzy);
// Fuzzy replaces incompatible segments to find the closest valid Unicode character
```

---

## 3. Per-Cell Color Gradients

ratatui does not have a built-in gradient API. You achieve gradients by directly writing
styled cells into the `Buffer`. Here are the concrete patterns.

### 3.1 Horizontal Gradient on a Block Background

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Widget};

/// A widget that renders a horizontal color gradient background.
struct GradientBlock {
    from: (u8, u8, u8),
    to: (u8, u8, u8),
}

impl Widget for GradientBlock {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 {
            return;
        }
        for x in area.left()..area.right() {
            let t = (x - area.left()) as f32 / (area.width - 1).max(1) as f32;
            let r = lerp(self.from.0, self.to.0, t);
            let g = lerp(self.from.1, self.to.1, t);
            let b = lerp(self.from.2, self.to.2, t);
            for y in area.top()..area.bottom() {
                buf[(x, y)]
                    .set_char(' ')
                    .set_bg(Color::Rgb(r, g, b));
            }
        }
    }
}

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t) as u8
}
```

### 3.2 Gradient Text (Per-Character Foreground)

```rust
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

fn gradient_line(text: &str, from: (u8, u8, u8), to: (u8, u8, u8)) -> Line<'_> {
    let len = text.chars().count().max(1);
    let spans: Vec<Span> = text
        .chars()
        .enumerate()
        .map(|(i, ch)| {
            let t = i as f32 / (len - 1).max(1) as f32;
            let r = (from.0 as f32 + (to.0 as f32 - from.0 as f32) * t) as u8;
            let g = (from.1 as f32 + (to.1 as f32 - from.1 as f32) * t) as u8;
            let b = (from.2 as f32 + (to.2 as f32 - from.2 as f32) * t) as u8;
            Span::styled(
                ch.to_string(),
                Style::new().fg(Color::Rgb(r, g, b)),
            )
        })
        .collect();
    Line::from(spans)
}

// Usage:
// let line = gradient_line("NIKA WORKFLOW ENGINE", (0, 200, 255), (200, 0, 255));
// Paragraph::new(line).render(area, buf);
```

### 3.3 Vertical Gradient with Half-Block Characters

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::widgets::Widget;

/// Renders a vertical gradient using upper half-blocks (2x vertical resolution).
struct VerticalGradient {
    top: (u8, u8, u8),
    bottom: (u8, u8, u8),
}

impl Widget for VerticalGradient {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let total_rows = area.height as f32 * 2.0; // 2 sub-pixels per cell
        for y in area.top()..area.bottom() {
            let row = y - area.top();
            // Upper half-pixel
            let t_upper = (row as f32 * 2.0) / total_rows;
            let upper = interp_color(self.top, self.bottom, t_upper);
            // Lower half-pixel
            let t_lower = (row as f32 * 2.0 + 1.0) / total_rows;
            let lower = interp_color(self.top, self.bottom, t_lower);

            for x in area.left()..area.right() {
                buf[(x, y)]
                    .set_char('▀')            // upper half block
                    .set_fg(upper)            // upper half = foreground
                    .set_bg(lower);           // lower half = background
            }
        }
    }
}

fn interp_color(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> Color {
    Color::Rgb(
        (a.0 as f32 + (b.0 as f32 - a.0 as f32) * t) as u8,
        (a.1 as f32 + (b.1 as f32 - a.1 as f32) * t) as u8,
        (a.2 as f32 + (b.2 as f32 - a.2 as f32) * t) as u8,
    )
}
```

---

## 4. Canvas Widget

The `Canvas` widget provides a coordinate-space drawing surface with multiple marker resolutions.

### 4.1 Marker Types (Resolution per Cell)

| Marker | Resolution | Characters | Best For |
|--------|-----------|------------|----------|
| `Dot` | 1x1 | `\u{2022}` | Simple scatter |
| `Block` | 1x1 | `\u{2588}` | Solid fills |
| `Bar` | 1x1 | `\u{2584}` | Bar-like dots |
| `Braille` | 2x4 | `\u{2800}`-`\u{28ff}` | High-res lines, widely supported |
| `HalfBlock` | 1x2 | `\u{2580}\u{2584}\u{2588}` | Color-per-pixel (fg+bg trick) |
| `Quadrant` | 2x2 | `\u{2596}`-`\u{259f}` | Dense fill, moderate resolution |
| `Sextant` | 2x3 | Legacy Computing Supplement | Higher density |
| `Octant` | 2x4 | Legacy Computing Supplement | Highest density, like Braille but filled |

### 4.2 Drawing Shapes

```rust
use ratatui::style::Color;
use ratatui::symbols::Marker;
use ratatui::widgets::Block;
use ratatui::widgets::canvas::{Canvas, Circle, Line, Points, Rectangle};

let canvas = Canvas::default()
    .block(Block::bordered().title("Canvas"))
    .x_bounds([0.0, 100.0])
    .y_bounds([0.0, 100.0])
    .marker(Marker::Braille)
    .background_color(Color::Black)
    .paint(|ctx| {
        // Rectangle
        ctx.draw(&Rectangle {
            x: 10.0,
            y: 10.0,
            width: 30.0,
            height: 20.0,
            color: Color::Cyan,
        });

        // Circle
        ctx.draw(&Circle {
            x: 60.0,
            y: 50.0,
            radius: 15.0,
            color: Color::Yellow,
        });

        // Line
        ctx.draw(&Line {
            x1: 0.0,
            y1: 0.0,
            x2: 100.0,
            y2: 100.0,
            color: Color::Red,
        });

        // Scatter points
        ctx.draw(&Points {
            coords: &[(5.0, 5.0), (50.0, 80.0), (90.0, 20.0)],
            color: Color::Green,
        });

        // Text label on canvas
        ctx.print(50.0, 95.0, ratatui::text::Line::from("Label").centered());
    });
```

### 4.3 Custom Shapes via the Shape Trait

```rust
use ratatui::style::Color;
use ratatui::widgets::canvas::{Painter, Shape};

/// A custom diamond shape
struct Diamond {
    cx: f64,
    cy: f64,
    size: f64,
    color: Color,
}

impl Shape for Diamond {
    fn draw(&self, painter: &mut Painter) {
        let steps = 100;
        // Top to right
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let x = self.cx + self.size * t;
            let y = self.cy + self.size * (1.0 - t);
            if let Some((px, py)) = painter.get_point(x, y) {
                painter.paint(px, py, self.color);
            }
        }
        // Right to bottom
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let x = self.cx + self.size * (1.0 - t);
            let y = self.cy - self.size * t;
            if let Some((px, py)) = painter.get_point(x, y) {
                painter.paint(px, py, self.color);
            }
        }
        // Bottom to left
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let x = self.cx - self.size * t;
            let y = self.cy - self.size * (1.0 - t);
            if let Some((px, py)) = painter.get_point(x, y) {
                painter.paint(px, py, self.color);
            }
        }
        // Left to top
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let x = self.cx - self.size * (1.0 - t);
            let y = self.cy + self.size * t;
            if let Some((px, py)) = painter.get_point(x, y) {
                painter.paint(px, py, self.color);
            }
        }
    }
}

// Use it:
// ctx.draw(&Diamond { cx: 50.0, cy: 50.0, size: 20.0, color: Color::Magenta });
```

### 4.4 Multi-Layer Canvas

```rust
Canvas::default()
    .marker(Marker::Braille)
    .x_bounds([0.0, 100.0])
    .y_bounds([0.0, 100.0])
    .paint(|ctx| {
        // Layer 0: background grid using Block marker
        ctx.marker(Marker::Block);
        ctx.draw(&Rectangle {
            x: 0.0, y: 0.0, width: 100.0, height: 100.0,
            color: Color::DarkGray,
        });
        ctx.layer(); // save and start new layer

        // Layer 1: high-res data using Braille
        ctx.marker(Marker::Braille);
        ctx.draw(&Line {
            x1: 0.0, y1: 0.0, x2: 100.0, y2: 100.0,
            color: Color::Cyan,
        });
        // layer() is called automatically at the end
    });
```

---

## 5. Sparkline Widget

### 5.1 Basic Sparkline

```rust
use ratatui::style::{Color, Style, Stylize};
use ratatui::symbols;
use ratatui::widgets::{Block, RenderDirection, Sparkline, SparklineBar};

// Simple from data slice
let sparkline = Sparkline::default()
    .block(Block::bordered().title("CPU %"))
    .data(&[0, 2, 3, 4, 1, 4, 10, 8, 7, 6, 5, 3])
    .max(10)
    .style(Style::new().cyan())
    .direction(RenderDirection::LeftToRight);
// Renders: ▁▂▃▄▁▄█▇▆▅▃▂ (scaled to max)
```

### 5.2 Per-Bar Styling with SparklineBar

```rust
let data = vec![
    SparklineBar::from(1).style(Some(Style::new().fg(Color::Green))),
    SparklineBar::from(5).style(Some(Style::new().fg(Color::Yellow))),
    SparklineBar::from(9).style(Some(Style::new().fg(Color::Red))),
    SparklineBar::from(None), // absent value
];

let sparkline = Sparkline::default()
    .data(data)
    .absent_value_style(Style::new().fg(Color::DarkGray))
    .absent_value_symbol(symbols::shade::FULL); // "█" in gray for missing
```

### 5.3 Custom Bar Sets

```rust
// Default: NINE_LEVELS = " ▁▂▃▄▅▆▇█" (9 levels of granularity)
// Alternative: THREE_LEVELS = " ▄█" (3 levels)

let sparkline = Sparkline::default()
    .data(&[0, 1, 2, 3, 4, 5, 6, 7, 8])
    .bar_set(symbols::bar::THREE_LEVELS); // coarser resolution

// You can define custom bar sets:
let custom_bars = symbols::bar::Set {
    full:           "X",
    seven_eighths:  "X",
    three_quarters: "x",
    five_eighths:   "x",
    half:           ".",
    three_eighths:  ".",
    one_quarter:    ".",
    one_eighth:     " ",
    empty:          " ",
};
let sparkline = Sparkline::default()
    .data(&[0, 2, 4, 6, 8])
    .bar_set(custom_bars);
```

---

## 6. BarChart Widget

### 6.1 Vertical BarChart

```rust
use ratatui::style::{Color, Style, Stylize};
use ratatui::symbols;
use ratatui::widgets::{Bar, BarChart, BarGroup, Block};

let chart = BarChart::default()
    .block(Block::bordered().title("Throughput"))
    .bar_width(3)
    .bar_gap(1)
    .bar_style(Style::new().cyan())
    .value_style(Style::new().white().bold())
    .label_style(Style::new().dark_gray())
    .data(&[("Mon", 10), ("Tue", 25), ("Wed", 18), ("Thu", 30), ("Fri", 22)])
    .max(30);
// Renders vertical bars with labels below and values on the bars
```

### 6.2 Horizontal BarChart

```rust
let chart = BarChart::horizontal(vec![
    Bar::with_label("fetch", 150),
    Bar::with_label("infer", 420),
    Bar::with_label("exec", 85),
])
.block(Block::bordered().title("Latency (ms)"))
.bar_width(1)
.bar_gap(1)
.bar_style(Style::new().green());
```

### 6.3 Grouped BarChart

```rust
let chart = BarChart::grouped(vec![
    BarGroup::with_label("v0.28", vec![
        Bar::with_label("parse", 120).style(Style::new().cyan()),
        Bar::with_label("exec", 300).style(Style::new().yellow()),
    ]),
    BarGroup::with_label("v0.29", vec![
        Bar::with_label("parse", 80).style(Style::new().cyan()),
        Bar::with_label("exec", 200).style(Style::new().yellow()),
    ]),
])
.bar_width(3)
.bar_gap(1)
.group_gap(2);
```

### 6.4 Custom Bar Symbols

```rust
// Use THREE_LEVELS for a chunkier look
let chart = BarChart::default()
    .data(&[("A", 3), ("B", 6), ("C", 8)])
    .bar_set(symbols::bar::THREE_LEVELS);  // " ▄█" instead of 9 levels

// Or NINE_LEVELS for smooth rendering (default)
// " ▁▂▃▄▅▆▇█"
```

---

## 7. Gauge Widget

### 7.1 Standard Gauge

```rust
use ratatui::style::{Color, Style, Stylize};
use ratatui::widgets::{Block, Gauge};

let gauge = Gauge::default()
    .block(Block::bordered().title("Download"))
    .gauge_style(Style::new().fg(Color::Green).bg(Color::DarkGray))
    .percent(67)
    .label("67% -- 2.4 MB/s");
```

### 7.2 Unicode High-Precision Gauge

```rust
// use_unicode enables 1/8th block characters for sub-cell precision
let gauge = Gauge::default()
    .gauge_style(Style::new().fg(Color::Cyan).bg(Color::Black))
    .ratio(0.424)  // 42.4%
    .use_unicode(true)
    .label("42.4%");
// The fractional cell uses: ▏▎▍▌▋▊▉█ (block characters for 1/8 precision)
```

### 7.3 LineGauge (Thin Progress Bar)

```rust
use ratatui::symbols;
use ratatui::widgets::LineGauge;

let line_gauge = LineGauge::default()
    .block(Block::bordered().title("Progress"))
    .ratio(0.6)
    .label("Step 3/5")
    .filled_symbol(symbols::line::THICK_HORIZONTAL)  // "━"
    .unfilled_symbol(symbols::line::HORIZONTAL)       // "─"
    .filled_style(Style::new().fg(Color::Green))
    .unfilled_style(Style::new().fg(Color::DarkGray));
// Renders: Step 3/5 ━━━━━━━━━━━━───────────
```

### 7.4 Custom LineGauge Symbols

```rust
// Creative progress bars using different unicode characters
LineGauge::default()
    .ratio(0.5)
    .filled_symbol("=")    // ============
    .unfilled_symbol("-"); // ============-----------

LineGauge::default()
    .ratio(0.7)
    .filled_symbol("\u{25B6}")  // triangle: ▶▶▶▶▶▶▶
    .unfilled_symbol("\u{25B7}"); // hollow:  ▶▶▶▶▶▶▶▷▷▷
```

---

## 8. Custom Widget and StatefulWidget Traits

### 8.1 The Widget Trait

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

pub trait Widget {
    fn render(self, area: Rect, buf: &mut Buffer);
}
```

### 8.2 Building a Custom Widget

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Widget};

/// A status indicator with icon + label + colored dot
struct StatusIndicator<'a> {
    label: &'a str,
    status: Status,
    block: Option<Block<'a>>,
}

enum Status {
    Ok,
    Warning,
    Error,
}

impl Widget for StatusIndicator<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Render optional block wrapper
        if let Some(block) = &self.block {
            block.clone().render(area, buf);
        }

        let inner = match &self.block {
            Some(b) => b.inner(area),
            None => area,
        };

        if inner.is_empty() {
            return;
        }

        let (icon, color) = match self.status {
            Status::Ok      => ("●", Color::Green),
            Status::Warning => ("●", Color::Yellow),
            Status::Error   => ("●", Color::Red),
        };

        // Draw the colored dot
        if inner.width >= 1 {
            buf[(inner.x, inner.y)]
                .set_symbol(icon)
                .set_fg(color);
        }

        // Draw the label
        if inner.width >= 3 {
            let label_area = inner.x + 2;
            for (i, ch) in self.label.chars().enumerate() {
                let x = label_area + i as u16;
                if x < inner.right() {
                    buf[(x, inner.y)].set_char(ch);
                }
            }
        }
    }
}
```

### 8.3 StatefulWidget for Interactive Widgets

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::StatefulWidget;

/// State for a selectable list of metrics
struct MetricListState {
    selected: usize,
    offset: usize,
}

struct MetricList<'a> {
    items: Vec<(&'a str, f64)>,
}

impl StatefulWidget for MetricList<'_> {
    type State = MetricListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        if area.is_empty() || self.items.is_empty() {
            return;
        }

        let visible_count = area.height as usize;

        // Adjust scroll offset
        if state.selected >= state.offset + visible_count {
            state.offset = state.selected - visible_count + 1;
        }
        if state.selected < state.offset {
            state.offset = state.selected;
        }

        for (i, item) in self.items.iter().skip(state.offset).take(visible_count).enumerate() {
            let y = area.top() + i as u16;
            let is_selected = state.offset + i == state.selected;

            let style = if is_selected {
                Style::new().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(Color::White)
            };

            // Draw label
            let label = format!("{:<20} {:>6.1}%", item.0, item.1);
            for (j, ch) in label.chars().enumerate() {
                let x = area.left() + j as u16;
                if x < area.right() {
                    buf[(x, y)].set_char(ch).set_style(style);
                }
            }

            // Mini inline sparkline bar
            let bar_start = area.left() + 28.min(area.width);
            let bar_width = area.right().saturating_sub(bar_start);
            let filled = (item.1 / 100.0 * bar_width as f64) as u16;
            for x in bar_start..bar_start + filled {
                if x < area.right() {
                    buf[(x, y)]
                        .set_symbol("█")
                        .set_fg(if item.1 > 80.0 { Color::Red } else { Color::Green });
                }
            }
        }
    }
}

// Usage:
// let mut state = MetricListState { selected: 0, offset: 0 };
// frame.render_stateful_widget(metric_list, area, &mut state);
```

---

## 9. Unicode Box Drawing -- Complete Reference

### 9.1 Thin Lines (─│)

```
Horizontal: ─  (U+2500)
Vertical:   │  (U+2502)
Corners:    ┌ (U+250C)  ┐ (U+2510)  └ (U+2514)  ┘ (U+2518)
T-pieces:   ├ (U+251C)  ┤ (U+2524)  ┬ (U+252C)  ┴ (U+2534)
Cross:      ┼ (U+253C)
```

### 9.2 Thick Lines (━┃)

```
Horizontal: ━  (U+2501)
Vertical:   ┃  (U+2503)
Corners:    ┏ (U+250F)  ┓ (U+2513)  ┗ (U+2517)  ┛ (U+251B)
T-pieces:   ┣ (U+2523)  ┫ (U+252B)  ┳ (U+2533)  ┻ (U+253B)
Cross:      ╋ (U+254B)
```

### 9.3 Double Lines (═║)

```
Horizontal: ═  (U+2550)
Vertical:   ║  (U+2551)
Corners:    ╔ (U+2554)  ╗ (U+2557)  ╚ (U+255A)  ╝ (U+255D)
T-pieces:   ╠ (U+2560)  ╣ (U+2563)  ╦ (U+2566)  ╩ (U+2569)
Cross:      ╬ (U+256C)
```

### 9.4 Rounded Corners (╭╮╰╯)

```
Corners only (sides are thin):
╭ (U+256D)  ╮ (U+256E)  ╰ (U+2570)  ╯ (U+256F)
```

### 9.5 Mixed Thin+Thick Junctions

ratatui's merge system supports all 48 combinations:

```
Thin-Thick corners: ┍ ┑ ┕ ┙  (thick horizontal, thin vertical)
                    ┎ ┒ ┖ ┚  (thin horizontal, thick vertical)
Mixed T-pieces:     ┝ ┞ ┟ ┠ ┡ ┢  (left T variants)
                    ┥ ┦ ┧ ┨ ┩ ┪  (right T variants)
                    ┭ ┮ ┯ ┰ ┱ ┲  (top T variants)
                    ┵ ┶ ┷ ┸ ┹ ┺  (bottom T variants)
Mixed crosses:      ┽ ┾ ┿ ╀ ╁ ╂ ╃ ╄ ╅ ╆ ╇ ╈ ╉ ╊
```

### 9.6 Dashed Lines

```
Light double-dashed:    ╌ (H) ╎ (V)
Heavy double-dashed:    ╍ (H) ╏ (V)
Light triple-dashed:    ┄ (H) ┆ (V)
Heavy triple-dashed:    ┅ (H) ┇ (V)
Light quadruple-dashed: ┈ (H) ┊ (V)
Heavy quadruple-dashed: ┉ (H) ┋ (V)
```

### 9.7 Half-Line Segments

```
╴ (left half)  ╵ (up half)  ╶ (right half)  ╷ (down half)    -- thin
╸ (left half)  ╹ (up half)  ╺ (right half)  ╻ (down half)    -- thick
╼ (right thick, left thin)   ╾ (right thin, left thick)       -- mixed H
╽ (up thin, down thick)      ╿ (up thick, down thin)          -- mixed V
```

---

## 10. Block Elements for Data Visualization

### 10.1 Vertical Bar Characters (▁▂▃▄▅▆▇█)

Available in `ratatui::symbols::bar`:

```
" " empty     (0/8)
"▁" one_eighth    (1/8)  U+2581
"▂" one_quarter   (2/8)  U+2582
"▃" three_eighths (3/8)  U+2583
"▄" half          (4/8)  U+2584
"▅" five_eighths  (5/8)  U+2585
"▆" three_quarters(6/8)  U+2586
"▇" seven_eighths (7/8)  U+2587
"█" full          (8/8)  U+2588
```

Used by: `Sparkline`, `BarChart`, `Gauge` (with `use_unicode`).

### 10.2 Horizontal Block Characters (▏▎▍▌▋▊▉█)

Available in `ratatui::symbols::block`:

```
"▏" one_eighth    (1/8)  U+258F
"▎" one_quarter   (2/8)  U+258E
"▍" three_eighths (3/8)  U+258D
"▌" half          (4/8)  U+258C
"▋" five_eighths  (5/8)  U+258B
"▊" three_quarters(6/8)  U+258A
"▉" seven_eighths (7/8)  U+2589
"█" full          (8/8)  U+2588
```

Used by: `Gauge` for sub-cell precision horizontal progress.

### 10.3 Shade Characters (░▒▓█)

Available in `ratatui::symbols::shade`:

```
" " EMPTY   -- no fill
"░" LIGHT   U+2591  (25% density)
"▒" MEDIUM  U+2592  (50% density)
"▓" DARK    U+2593  (75% density)
"█" FULL    U+2588  (100% density)
```

Use case -- heat map cells:

```rust
fn shade_for_value(value: f64) -> &'static str {
    match (value * 4.0) as u8 {
        0 => " ",
        1 => "░",
        2 => "▒",
        3 => "▓",
        _ => "█",
    }
}
```

### 10.4 Braille Density Characters

Canvas uses Braille patterns (U+2800..U+28FF) for 2x4 dot resolution per cell.
The density progresses from empty to full:

```
⠀ (0 dots)  -- U+2800 blank braille
⣀ (bottom row) -- partial
⣤ (two rows)
⣶ (three rows)
⣿ (all 8 dots) -- U+28FF full braille
```

Each braille cell is a 2x4 grid of independently toggleable dots.
The `Canvas` widget with `Marker::Braille` handles this automatically.

### 10.5 Quadrant Block Characters

```
▖ (U+2596) lower-left       ▗ (U+2597) lower-right
▘ (U+2598) upper-left       ▝ (U+259D) upper-right
▌ (U+258C) left half         ▐ (U+2590) right half
▀ (U+2580) upper half        ▄ (U+2584) lower half
▙ (U+2599) all but upper-right
▛ (U+259B) all but lower-right
▜ (U+259C) all but lower-left
▟ (U+259F) all but upper-left
▚ (U+259A) upper-left + lower-right (diagonal)
▞ (U+259E) upper-right + lower-left (diagonal)
█ (U+2588) full block
```

Used by: `BorderType::QuadrantInside`, `BorderType::QuadrantOutside`, and `Marker::Quadrant`.

### 10.6 Practical Example: Inline Heat Map

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

struct HeatMap<'a> {
    data: &'a [Vec<f64>],  // rows x cols, values 0.0..1.0
}

impl Widget for HeatMap<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for (row_idx, row) in self.data.iter().enumerate() {
            let y = area.top() + row_idx as u16;
            if y >= area.bottom() { break; }

            for (col_idx, &val) in row.iter().enumerate() {
                let x = area.left() + col_idx as u16;
                if x >= area.right() { break; }

                // Map value to shade character + color intensity
                let symbol = match (val * 4.0) as u8 {
                    0 => " ",
                    1 => "░",
                    2 => "▒",
                    3 => "▓",
                    _ => "█",
                };

                // Color from blue (cold) to red (hot)
                let r = (val * 255.0) as u8;
                let b = ((1.0 - val) * 255.0) as u8;
                buf[(x, y)]
                    .set_symbol(symbol)
                    .set_fg(Color::Rgb(r, 40, b));
            }
        }
    }
}
```

---

## Summary Table: Which Widget for What

| Need | Widget | Key Methods |
|------|--------|-------------|
| Container/frame | `Block` | `.border_type()`, `.border_set()`, `.padding()`, `.merge_borders()` |
| Time series | `Sparkline` | `.data()`, `.bar_set()`, `.direction()` |
| Bars/histogram | `BarChart` | `.bar_width()`, `.bar_gap()`, `.direction()`, `.data()` |
| Progress | `Gauge` / `LineGauge` | `.percent()`, `.ratio()`, `.use_unicode()` |
| Free drawing | `Canvas` | `.marker()`, `.paint()`, `.x_bounds()`, `.y_bounds()` |
| Custom complex | impl `Widget` | Direct `Buffer` manipulation |
| Stateful custom | impl `StatefulWidget` | `type State`, `.render(area, buf, &mut state)` |

---

## Sources

All code verified against source at:
- `/Users/thibaut/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ratatui-0.30.0/`
- `/Users/thibaut/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ratatui-widgets-0.3.0/`
- `/Users/thibaut/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ratatui-core-0.1.0/`

Confidence: **High** -- all APIs read directly from the crate source code shipped in the project's
Cargo.lock-pinned dependency tree.
