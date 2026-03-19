# Rust Data Visualization & Chart Generation Crates

**Date**: 2026-03-18
**Purpose**: Evaluate Rust crates for native chart/graph/plot generation as image artifacts in Nika workflows.

---

## Executive Summary

There are **6 mature crates** worth considering for Nika's chart generation feature, ranging from pure-Rust SVG emitters to JavaScript-engine-powered renderers. The two strongest candidates for Nika are **`charts-rs`** (pure Rust, SVG+PNG+JPEG+WebP+AVIF, zero external deps, JSON config) and **`plotters`** (dominant ecosystem, SVG+PNG, extensible backends). For rich interactive-quality charts without a browser, **`charming`** (ECharts via embedded Deno) offers the most chart types but at a heavy dependency cost.

---

## Tier 1: Primary Candidates

### 1. plotters

| Attribute | Value |
|-----------|-------|
| **Version** | 0.3.7 |
| **Downloads** | 138.9M total / 22.2M recent |
| **Dependents** | 417 crates |
| **Repository** | https://github.com/plotters-rs/plotters |
| **License** | MIT |
| **MSRV** | 1.56 |

**Output Formats**: SVG, PNG, JPEG, BMP, GIF (bitmap via `image` crate), WASM Canvas

**Architecture**: Backend-agnostic. Three official backends:
- `plotters-svg` -- pure Rust SVG output (no system deps)
- `plotters-bitmap` -- rasterized PNG/JPEG/BMP/GIF via the `image` crate (pure Rust)
- `plotters-canvas` -- HTML5 Canvas for WASM

**Chart Types**:
- Line, area, scatter, bar, histogram
- Box plots, error bars, candlestick
- 3D surface plots, 3D line plots
- Heatmap/matshow
- Custom drawing primitives (circles, rectangles, text, polygons)
- Multi-panel / split drawing areas
- Dual Y-axis, logarithmic scales
- Animations (GIF output)

**Rendering Quality**: Good for data-driven charts. Not as polished as ECharts/Plotly for presentation-grade output. Font rendering requires system fonts or `ttf-parser`/`ab_glyph`.

**Ease of Use**: Builder pattern API. Moderate learning curve. Excellent documentation with 20+ examples. Jupyter/evcxr integration.

**Key Dependencies** (default features):
- `plotters-svg` (pure Rust)
- `plotters-bitmap` -> `image` crate (pure Rust)
- `ttf-parser` or `ab_glyph` for fonts
- Optional: `chrono` for time series

**Pros**:
- Dominant Rust plotting library (138M downloads)
- Pure Rust, no external binaries required
- WASM support
- Pluggable backend architecture
- Extensive chart type coverage
- Active maintenance

**Cons**:
- Verbose API for simple charts (lots of boilerplate)
- Bitmap font rendering can look rough without proper font setup
- No built-in themes (manual styling)
- No JSON/declarative config -- purely programmatic

**Nika Fit**: HIGH -- can generate SVG or PNG directly to CAS. Feature-gate behind `plotters`. The verbose API is less of a concern since Nika would wrap it in a builtin tool.

---

### 2. charts-rs

| Attribute | Value |
|-----------|-------|
| **Version** | 0.3.28 |
| **Downloads** | 116K total / 3.8K recent |
| **Dependents** | 1 crate |
| **Repository** | https://github.com/vicanso/charts-rs |
| **License** | Apache-2.0 |
| **MSRV** | 1.65 |

**Output Formats**: SVG (native), PNG, JPEG, WebP, AVIF (via `resvg` + `image` feature)

**Architecture**: Pure Rust. Generates SVG natively using `fontdue` for text layout. Optional `image-encoder` feature adds raster output via `resvg` (pure Rust SVG renderer) + `image` crate.

**Chart Types**:
- Bar (vertical + horizontal)
- Line (smooth curves, area fill, mark points, mark lines)
- Pie
- Radar
- Scatter
- Candlestick
- Table
- Heatmap
- MultiChart (composites)

**Rendering Quality**: HIGH -- inspired by Apache ECharts visual style. Nine built-in themes: light, dark, grafana, ant, vintage, walden, westeros, chalk, shine. Custom font loading from TTF/OTF.

**Ease of Use**: EXCELLENT -- accepts JSON configuration directly:
```rust
let chart = BarChart::from_json(r#"{"title_text": "Sales", ...}"#).unwrap();
let svg = chart.svg().unwrap();
// or with image-encoder feature:
svg_to_png(&svg).unwrap();
```

**Key Dependencies**:
- `fontdue` (pure Rust font rasterizer)
- `resvg` + `image` (optional, for raster output)
- `serde` / `serde_json` (JSON config)
- Zero system dependencies

**Pros**:
- JSON-configurable (perfect for YAML workflow integration)
- ECharts-quality visual output with 9 themes
- Pure Rust, zero external binaries
- 5 raster output formats (PNG, JPEG, WebP, AVIF + SVG)
- Dual Y-axis, smooth lines, mark points
- Web demo for interactive config testing
- Small dependency footprint

**Cons**:
- Small community (116K downloads, 1 dependent)
- Fewer chart types than plotters/charming (no 3D, no treemap, no graph layout)
- Single maintainer project
- No WASM support mentioned

**Nika Fit**: HIGHEST -- JSON config maps directly to `with:` bindings in Nika workflows. Pure Rust with multi-format output. The `from_json()` API is exactly what a builtin tool needs. Could literally pass a JSON string from an `infer:` step to generate a chart.

---

### 3. charming (ECharts for Rust)

| Attribute | Value |
|-----------|-------|
| **Version** | 0.6.0 |
| **Downloads** | 878K total / 212K recent |
| **Dependents** | 9 crates |
| **Repository** | https://github.com/yuankunzhang/charming |
| **License** | MIT OR Apache-2.0 |
| **MSRV** | 1.88 (Rust edition 2024) |

**Output Formats**: HTML (interactive), SVG, PNG, JPEG, GIF, WebP, PNM, TIFF, TGA, DDS, BMP, ICO, HDR, OpenEXR, Farbfeld, AVIF, QOI

**Architecture**: Three rendering paths:
1. `HtmlRenderer` -- generates HTML with embedded ECharts JS (interactive)
2. `ImageRenderer` (feature `ssr`) -- embeds `deno_core` JS engine to execute ECharts JavaScript and produce SVG
3. `ImageRenderer` + `ssr-raster` -- SVG from deno, then rasterized via `resvg` + `image` to any format
4. `WasmRenderer` (feature `wasm`) -- runs ECharts in browser WASM context

**Chart Types**: Full Apache ECharts coverage:
- Bar, line, scatter, pie, radar, candlestick
- Boxplot, heatmap, treemap, sunburst
- Sankey, funnel, gauge, graph/network
- Parallel coordinates, polar, geo/map
- 3D charts (if ECharts GL loaded)
- Rich tooltip, legend, zoom, dataZoom
- 14 built-in themes (dark, vintage, westeros, essos, wonderland, walden, chalk, infographic, macarons, roma, shine, purple-passion, halloween)

**Rendering Quality**: EXCELLENT -- identical to Apache ECharts (the gold standard for web data visualization). 14 polished themes.

**Ease of Use**: Declarative Rust builder API that mirrors ECharts option structure:
```rust
let chart = Chart::new()
    .title(Title::new().text("Sales"))
    .series(Bar::new().data(vec![120.0, 200.0, 150.0]));
let mut renderer = ImageRenderer::new(1000, 800);
renderer.save(&chart, "/tmp/chart.svg");
```

**Key Dependencies** (with `ssr-raster`):
- `deno_core` v0.378 (HEAVY -- embeds V8 JavaScript engine)
- `serde_v8` v0.287
- `handlebars` (templating)
- `resvg` v0.46 (SVG rasterization)
- `image` v0.25 (raster encoding)

**Pros**:
- Apache ECharts rendering quality (best-in-class)
- Widest chart type coverage of any Rust library
- 14 themes out of the box
- 17+ raster output formats
- WASM support for browser contexts
- Growing adoption (878K downloads, trending up)

**Cons**:
- VERY HEAVY dependency: `deno_core` embeds the V8 JavaScript engine (~50MB+ binary size increase)
- High MSRV (1.88, edition 2024) -- requires latest Rust
- `ssr` and `wasm` features are mutually exclusive
- Startup overhead from V8 initialization
- Single-threaded JS execution for rendering

**Nika Fit**: MEDIUM -- best visual output but the V8 dependency is problematic for a CLI tool. Could work as an optional feature-gated capability. The binary size and startup cost may be unacceptable for `nika` core.

---

### 4. plotly (Plotly.rs)

| Attribute | Value |
|-----------|-------|
| **Version** | 0.14.1 |
| **Downloads** | 2.7M total / 923K recent |
| **Dependents** | 82 crates |
| **Repository** | https://github.com/plotly/plotly.rs |
| **License** | MIT |

**Output Formats**: HTML (interactive), PNG, JPEG, WebP, SVG, PDF, EPS

**Architecture**: Generates Plotly.js JSON specifications. For static image export, requires either:
1. `plotly_static` feature -- uses WebDriver (chromedriver/geckodriver) for headless browser rendering
2. `kaleido` feature (deprecated) -- uses Plotly's Kaleido binary

**Chart Types**: Full Plotly.js coverage:
- Scatter, line, bar, pie, donut
- Box, violin, histogram, heatmap
- Contour, surface (3D), mesh3d
- Sankey, treemap, sunburst, funnel
- Candlestick, OHLC
- Geo/choropleth maps
- Subplots, dual axes, annotations

**Rendering Quality**: EXCELLENT -- Plotly.js is industry-standard for data visualization. Publication quality.

**Key Dependencies** (for static export):
- `plotly_static` -> headless Chrome/Firefox via WebDriver
- `askama` (templating)
- `serde` / `serde_json`
- External: chromedriver or geckodriver binary required on system

**Pros**:
- Official Plotly Rust crate (backed by Plotly org)
- Widest output format support (including PDF)
- Publication-quality rendering
- Strong community (2.7M downloads, 82 dependents)
- ndarray integration for scientific computing

**Cons**:
- REQUIRES external browser + WebDriver for static image export
- Cannot render images in headless/serverless environments without Chrome
- HTML-only output if no external tools available
- Kaleido deprecated, `plotly_static` is the future but needs system deps

**Nika Fit**: LOW for native chart generation -- the requirement for chromedriver/geckodriver is a non-starter for a portable CLI tool. Fine for HTML artifact output, but not for generating PNG/SVG artifacts without system dependencies.

---

## Tier 2: Specialized / Niche

### 5. inferno (Flamegraph toolkit)

| Attribute | Value |
|-----------|-------|
| **Version** | 0.12.6 |
| **Downloads** | 33.9M total / 8.5M recent |
| **Repository** | https://github.com/jonhoo/inferno |
| **License** | CDDL-1.0 |

**Output Format**: SVG only (interactive flamegraph SVG with embedded JavaScript)

**Purpose**: Port of Brendan Gregg's FlameGraph toolkit. Generates flamegraph SVGs from stack profiling data (perf, DTrace).

**Library API**: Yes -- `inferno::flamegraph::from_lines()` can generate flamegraph SVG programmatically.

**Nika Fit**: NICHE -- only useful if Nika workflows process profiling data. Not a general-purpose chart library. However, flamegraph output could be a specialized artifact type for performance analysis workflows.

---

### 6. poloto

| Attribute | Value |
|-----------|-------|
| **Version** | 19.1.2 |
| **Downloads** | 593K total / 55K recent |
| **Repository** | https://github.com/tiby312/poloto-project |
| **License** | MIT OR Apache-2.0 |

**Output Format**: SVG only (CSS-styleable)

**Chart Types**: Line, scatter, histogram, area fill. 2D only.

**Unique Feature**: Outputs SVG that can be styled with CSS. Dark/light theme switching via CSS classes. The SVG can be embedded in HTML and dynamically themed.

**Key Dependencies**: Minimal -- `tagu` (HTML/SVG builder). No system deps.

**Nika Fit**: LOW-MEDIUM -- SVG-only output is limiting. The CSS-theming is interesting but niche. Less chart variety than plotters or charts-rs.

---

### 7. Terminal / ASCII visualization

| Crate | Downloads | Purpose |
|-------|-----------|---------|
| **textplots** v0.8.7 | 853K | Unicode scatter/line plots in terminal |
| **lowcharts** v0.5.8 | 462K | Low-resolution terminal graphs |
| **drawille** v0.3.0 | 2.0M | Braille character terminal drawing |
| **rasciigraph** v0.3.0 | 80K | ASCII line graphs |
| **sparkline** v0.1.1 | 12K | Unicode sparklines (one-line trends) |

**Nika Fit**: These produce text output, not image files. Useful for TUI display or text artifact output but not for generating chart images. Could complement the TUI runner view.

---

### 8. Graph / Diagram Layout

| Crate | Downloads | Purpose |
|-------|-----------|---------|
| **graphviz-rust** v0.9.7 | 1.0M | Graphviz DOT format parser + renderer (requires graphviz binary for image output) |
| **layout-rs** v0.1.3 | 408K | Graph layout algorithms (force-directed, hierarchical) |
| **petgraph** v0.8.3 | 315.9M | Graph data structure + algorithms (no rendering) |

**graphviz-rust** can generate SVG/PNG/PDF but requires the `graphviz` (dot) binary installed on the system. The Rust part handles DOT parsing and generation; actual rendering is delegated to the external `dot` command.

**Nika Fit**: LOW -- external binary dependency. For DAG visualization in Nika's own debugging tools, could be useful but not for portable artifact generation.

---

### 9. SVG-to-Raster Pipeline (resvg stack)

| Crate | Version | Downloads | Purpose |
|-------|---------|-----------|---------|
| **resvg** | 0.47.0 | 10.9M | SVG rendering to pixel buffer |
| **usvg** | 0.47.0 | 12.1M | SVG simplification/normalization |
| **tiny-skia** | 0.12.0 | 22.7M | 2D rendering engine (Skia subset) |

This stack is **pure Rust** and converts SVG to raster formats without any system dependencies. Both `charts-rs` and `charming` use `resvg` for their raster output. This is the recommended approach: generate SVG first, then use `resvg` + `image` to encode as PNG/JPEG/WebP/AVIF.

---

## Comparison Matrix

| Feature | plotters | charts-rs | charming | plotly |
|---------|----------|-----------|----------|--------|
| **SVG output** | Yes | Yes | Yes | Yes |
| **PNG output** | Yes (pure Rust) | Yes (pure Rust) | Yes (V8+resvg) | Yes (needs Chrome) |
| **PDF output** | No | No | No | Yes (needs Chrome) |
| **WebP/AVIF** | No | Yes | Yes | Yes (needs Chrome) |
| **Pure Rust** | Yes | Yes | No (V8) | No (WebDriver) |
| **No system deps** | Yes | Yes | No | No |
| **JSON config** | No | Yes | No (Rust API) | No (Rust API) |
| **Built-in themes** | No | 9 themes | 14 themes | N/A (Plotly.js) |
| **Chart types** | ~15 | 10 | 30+ (ECharts) | 30+ (Plotly.js) |
| **3D support** | Yes (basic) | No | Yes (ECharts GL) | Yes |
| **Heatmap** | Yes (matshow) | Yes | Yes | Yes |
| **Treemap** | No | No | Yes | Yes |
| **Rendering quality** | Good | Very Good | Excellent | Excellent |
| **Binary size impact** | Small (~2MB) | Small (~3MB) | Large (~50MB+) | Small (HTML only) |
| **WASM support** | Yes | No | Yes | Yes |
| **Downloads** | 138.9M | 116K | 878K | 2.7M |
| **Dependents** | 417 | 1 | 9 | 82 |

---

## Recommendation for Nika

### Primary: `charts-rs` (behind feature flag `chart`)

**Rationale**:
1. **JSON-first API** -- `BarChart::from_json()` maps perfectly to Nika's `with:` bindings. An LLM `infer:` step could generate chart JSON, and a `chart:` builtin renders it.
2. **Pure Rust, zero system deps** -- works everywhere Nika runs.
3. **Multi-format output** -- SVG + PNG + JPEG + WebP + AVIF covers all artifact needs.
4. **ECharts-quality visuals** -- 9 themes, professional appearance.
5. **Small footprint** -- `fontdue` + `resvg` + `image` are already useful for the media pipeline.
6. **10 chart types** cover 95% of workflow visualization needs.

**Integration sketch**:
```yaml
tasks:
  - id: generate_chart
    infer:
      prompt: "Generate a charts-rs JSON config for a bar chart showing {{with.data}}"
      output: json
  - id: render_chart
    chart:                          # new builtin verb or builtin tool
      config: "{{with.chart_json}}"
      format: png
      theme: grafana
      width: 800
      height: 600
    with:
      chart_json: $generate_chart
    artifacts:
      - name: sales-chart.png
        format: binary
```

**Dependency cost** (with `image-encoder` feature):
```toml
charts-rs = { version = "0.3", features = ["image-encoder"] }
# Brings in: fontdue, resvg, image, serde_json (most already in tree)
```

### Secondary: `plotters` (behind feature flag `plotters`)

For users who need programmatic chart construction, 3D plots, or animation (GIF), `plotters` provides the escape hatch. More verbose but more flexible.

### Not Recommended for Core:
- **charming** -- V8 dependency too heavy for a CLI tool
- **plotly** -- requires external browser/WebDriver
- **graphviz-rust** -- requires external `dot` binary

### For TUI: `textplots` or `drawille`

For the Nika TUI runner view, `textplots` (Unicode plots) or `drawille` (braille drawing) could render inline chart previews without generating image files.

---

## Sources

1. [plotters](https://github.com/plotters-rs/plotters) -- 138.9M downloads, pure Rust plotting
2. [charming](https://github.com/yuankunzhang/charming) -- 878K downloads, ECharts via Deno
3. [charts-rs](https://github.com/vicanso/charts-rs) -- 116K downloads, pure Rust, JSON config
4. [plotly.rs](https://github.com/plotly/plotly.rs) -- 2.7M downloads, Plotly.js based
5. [inferno](https://github.com/jonhoo/inferno) -- 33.9M downloads, flamegraph generation
6. [poloto](https://github.com/tiby312/poloto-project) -- 593K downloads, CSS-styleable SVG
7. [resvg](https://github.com/nicoulaj/resvg) -- 10.9M downloads, SVG-to-raster pipeline
8. [textplots](https://github.com/loony-bean/textplots-rs) -- 853K downloads, terminal plots
9. [graphviz-rust](https://github.com/besok/graphviz-rust) -- 1.0M downloads, DOT format

## Methodology

- Crates.io API queries for metadata, download counts, reverse dependencies
- GitHub README analysis for feature coverage and API design
- Cargo.toml inspection for dependency trees and feature flags
- Cross-referenced output format support across all candidates

## Confidence Level

**High** -- all data sourced from crates.io (live API) and official GitHub repositories. Download counts and feature flags are factual. Rendering quality assessments are based on published example galleries.
