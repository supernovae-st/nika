//! The SVG writer · CHT §3bis law 5 + §12bis byte-golf.
//!
//! INTEGER COORDINATE SPACE (SVGO-class · «the decimal point contains no
//! information»): the viewBox is 2× the logical canvas and every emitted
//! coordinate is `px(v) = round(v·2)` — an i64. Half-pixel precision, ZERO
//! float formatting in the artifact, byte-stability by construction.
//! Rect/cell edges are rounded independently (x0=px(x) · x1=px(x+w) ·
//! w=x1−x0) so adjacent cells share an edge — seam-free without overlap.
//!
//! THEME SEAM (one colour seam): chrome constants map to CSS classes backed
//! by an embedded `<style>` with a `prefers-color-scheme: dark` block —
//! light is the base (the documented degradation for @media-blind rasterers
//! like resvg per §12bis) · SAME bytes render light and dark. Series colors
//! stay inline (Okabe-Ito is CVD-safe on both).
//!
//! All emission goes through `write!` into one buffer (no intermediate
//! `format!` allocations · perf law) · attributes in fixed hand-written
//! order · LF only · zero timestamps · zero ids.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)] // half-pixel integer grid: rounding casts ARE the contract
#![allow(clippy::write_with_newline)] // deliberate: LF is part of the byte contract

use std::fmt::Write as _;

use crate::palette;

/// Logical→artifact scale (half-pixel grid).
const S: f64 = 2.0;

/// Scale a logical coordinate to the integer artifact grid.
fn px(v: f64) -> i64 {
    (v * S).round() as i64
}

/// Fixed-2dp for 0..1 ratios (opacity) · integer math · trimmed.
fn ratio(o: f64) -> String {
    let scaled = (o * 100.0).round() as i64;
    let int = scaled / 100;
    let frac = scaled % 100;
    if frac == 0 {
        int.to_string()
    } else if frac % 10 == 0 {
        format!("{int}.{}", frac / 10)
    } else if frac < 10 {
        format!("{int}.0{frac}")
    } else {
        format!("{int}.{frac}")
    }
}

/// Light values mirror `palette` chrome constants · dark values are fixed
/// constants of the artifact contract.
const THEME_STYLE: &str = "<style>.bg{fill:#ffffff}.ink{fill:#1a1a1a}.ink2{fill:#555555}\
.grid{stroke:#e6e6e6}.axis{stroke:#9a9a9a}.nodata{fill:#eceef0}\
@media(prefers-color-scheme:dark){.bg{fill:#0f1318}.ink{fill:#e6e6e6}.ink2{fill:#9aa4ad}\
.grid{stroke:#262c33}.axis{stroke:#57616b}.nodata{fill:#1c2127}}</style>\n";

fn fill_class(color: &str) -> Option<&'static str> {
    match color {
        palette::BG => Some("bg"),
        palette::INK => Some("ink"),
        palette::INK_SOFT => Some("ink2"),
        _ => None,
    }
}

fn stroke_class(color: &str) -> Option<&'static str> {
    match color {
        palette::GRID => Some("grid"),
        palette::AXIS => Some("axis"),
        _ => None,
    }
}

#[must_use]
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Text anchor (closed set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

impl Anchor {
    fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Middle => "middle",
            Self::End => "end",
        }
    }
}

/// Buffer-backed writer · fixed attribute order · integer grid.
pub struct Svg {
    buf: String,
}

impl Svg {
    #[must_use]
    pub fn open(w: u32, h: u32, title: &str, metadata_json: Option<&str>) -> Self {
        let mut buf = String::with_capacity(8192);
        let _ = write!(
            buf,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" \
             viewBox=\"0 0 {} {}\" role=\"img\" aria-label=\"{}\">\n<title>{}</title>\n",
            u64::from(w) * 2,
            u64::from(h) * 2,
            escape(title),
            escape(title)
        );
        if let Some(m) = metadata_json {
            let _ = write!(buf, "<metadata>{}</metadata>\n", escape(m));
        }
        buf.push_str(THEME_STYLE);
        Self { buf }
    }

    /// Themed or inline-filled rectangle · edge-rounded (seam-free).
    #[allow(clippy::many_single_char_names)] // geometry signature (x y w h)
    pub fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, fill: &str, opacity: Option<f64>) {
        let (x0, y0) = (px(x), px(y));
        let (rw, rh) = (px(x + w) - x0, px(y + h) - y0);
        let _ = write!(
            self.buf,
            "<rect x=\"{x0}\" y=\"{y0}\" width=\"{rw}\" height=\"{rh}\""
        );
        match fill_class(fill) {
            Some(c) => {
                let _ = write!(self.buf, " class=\"{c}\"");
            }
            None => {
                let _ = write!(self.buf, " fill=\"{fill}\"");
            }
        }
        if let Some(o) = opacity {
            let _ = write!(self.buf, " fill-opacity=\"{}\"", ratio(o));
        }
        self.buf.push_str("/>\n");
    }

    /// Explicit no-data backdrop (philosophy law 6 · themed).
    pub fn nodata(&mut self, x: f64, y: f64, w: f64, h: f64) {
        let (x0, y0) = (px(x), px(y));
        let _ = write!(
            self.buf,
            "<rect x=\"{x0}\" y=\"{y0}\" width=\"{}\" height=\"{}\" class=\"nodata\"/>\n",
            px(x + w) - x0,
            px(y + h) - y0
        );
    }

    /// Heatmap cell: inline ramp fill + crispEdges + shared edges.
    pub fn cell(&mut self, x: f64, y: f64, w: f64, h: f64, fill: &str) {
        let (x0, y0) = (px(x), px(y));
        let _ = write!(
            self.buf,
            "<rect x=\"{x0}\" y=\"{y0}\" width=\"{}\" height=\"{}\" fill=\"{fill}\" \
             shape-rendering=\"crispEdges\"/>\n",
            px(x + w) - x0,
            px(y + h) - y0
        );
    }

    /// One batched path of horizontal segments (gridlines) · §12bis golf:
    /// N lines → one element · `h` relative commands.
    pub fn h_segments(&mut self, segs: &[(f64, f64, f64)], stroke: &str, width: f64) {
        if segs.is_empty() {
            return;
        }
        self.buf.push_str("<path d=\"");
        for (x1, x2, y) in segs {
            let x0 = px(*x1);
            let _ = write!(self.buf, "M{x0} {}h{}", px(*y), px(*x2) - x0);
        }
        self.stroke_close(stroke, width);
    }

    /// One batched path of vertical segments (tick marks · axes).
    pub fn v_segments(&mut self, segs: &[(f64, f64, f64)], stroke: &str, width: f64) {
        if segs.is_empty() {
            return;
        }
        self.buf.push_str("<path d=\"");
        for (x, y1, y2) in segs {
            let y0 = px(*y1);
            let _ = write!(self.buf, "M{} {y0}v{}", px(*x), px(*y2) - y0);
        }
        self.stroke_close(stroke, width);
    }

    fn stroke_close(&mut self, stroke: &str, width: f64) {
        self.buf.push_str("\" fill=\"none\"");
        match stroke_class(stroke) {
            Some(c) => {
                let _ = write!(self.buf, " class=\"{c}\"");
            }
            None => {
                let _ = write!(self.buf, " stroke=\"{stroke}\"");
            }
        }
        let _ = write!(
            self.buf,
            " stroke-width=\"{}\" shape-rendering=\"crispEdges\"/>\n",
            px(width)
        );
    }

    /// Polyline · optional LOGICAL dash pattern (scaled internally).
    pub fn polyline(
        &mut self,
        pts: &[(f64, f64)],
        stroke: &str,
        width: f64,
        dash: Option<(u32, u32)>,
    ) {
        self.buf.push_str("<polyline points=\"");
        for (i, (x, y)) in pts.iter().enumerate() {
            if i > 0 {
                self.buf.push(' ');
            }
            let _ = write!(self.buf, "{},{}", px(*x), px(*y));
        }
        let _ = write!(
            self.buf,
            "\" fill=\"none\" stroke=\"{stroke}\" stroke-width=\"{}\" \
             stroke-linejoin=\"round\" stroke-linecap=\"round\"",
            px(width)
        );
        if let Some((a, b)) = dash {
            let _ = write!(self.buf, " stroke-dasharray=\"{} {}\"", a * 2, b * 2);
        }
        self.buf.push_str("/>\n");
    }

    /// Closed band polygon: upper polyline forward + lower reversed + Z.
    pub fn band_path(&mut self, up: &[(f64, f64)], lo: &[(f64, f64)], fill: &str, opacity: f64) {
        self.buf.push_str("<path d=\"");
        for (i, (x, y)) in up.iter().enumerate() {
            let _ = write!(
                self.buf,
                "{}{} {}",
                if i == 0 { "M" } else { "L" },
                px(*x),
                px(*y)
            );
        }
        for (x, y) in lo.iter().rev() {
            let _ = write!(self.buf, "L{} {}", px(*x), px(*y));
        }
        let _ = write!(
            self.buf,
            "Z\" fill=\"{fill}\" fill-opacity=\"{}\"/>\n",
            ratio(opacity)
        );
    }

    /// Circle with optional fill-opacity.
    pub fn dot(&mut self, cx: f64, cy: f64, r: f64, fill: &str, opacity: Option<f64>) {
        let _ = write!(
            self.buf,
            "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{fill}\"",
            px(cx),
            px(cy),
            px(r)
        );
        if let Some(o) = opacity {
            let _ = write!(self.buf, " fill-opacity=\"{}\"", ratio(o));
        }
        self.buf.push_str("/>\n");
    }

    #[allow(clippy::too_many_arguments)]
    pub fn text(
        &mut self,
        x: f64,
        y: f64,
        content: &str,
        size: f64,
        fill: &str,
        anchor: Anchor,
        weight: Option<u32>,
    ) {
        let _ = write!(
            self.buf,
            "<text x=\"{}\" y=\"{}\" font-family=\"'Helvetica Neue', Arial, sans-serif\" \
             font-size=\"{}\"",
            px(x),
            px(y),
            px(size)
        );
        match fill_class(fill) {
            Some(c) => {
                let _ = write!(self.buf, " class=\"{c}\"");
            }
            None => {
                let _ = write!(self.buf, " fill=\"{fill}\"");
            }
        }
        let _ = write!(self.buf, " text-anchor=\"{}\"", anchor.as_str());
        if let Some(v) = weight {
            let _ = write!(self.buf, " font-weight=\"{v}\"");
        }
        let _ = write!(self.buf, ">{}</text>\n", escape(content));
    }

    #[must_use]
    pub fn close(mut self) -> String {
        self.buf.push_str("</svg>\n");
        self.buf
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::unreadable_literal
)]
mod tests {
    use super::*;

    #[test]
    fn escapes_all_specials() {
        assert_eq!(escape("a<b>&\"c'"), "a&lt;b&gt;&amp;&quot;c&apos;");
    }

    #[test]
    fn integer_grid_no_dots_in_coords() {
        let mut s = Svg::open(100, 50, "t", None);
        s.rect(1.23, 4.56, 10.0, 10.0, "#123456", None);
        s.polyline(&[(0.4, 0.6), (10.2, 5.7)], "#000000", 1.8, None);
        let out = s.close();
        // Coordinates are integers on the 2× grid: no decimal points in
        // x/y/width/height/points attributes (ratio() only serves opacity).
        assert!(out.contains("x=\"2\""));
        assert!(out.contains("points=\"1,1 20,11\""));
        assert!(!out.contains("fill-opacity"), "no opacity requested");
    }

    #[test]
    fn seam_free_adjacent_cells() {
        let mut s = Svg::open(100, 50, "t", None);
        // Two cells sharing the fractional edge 10.25 → same rounded edge.
        s.cell(0.0, 0.0, 10.25, 5.0, "#111111");
        s.cell(10.25, 0.0, 10.25, 5.0, "#222222");
        let out = s.close();
        assert!(out.contains("x=\"0\" y=\"0\" width=\"21\""));
        assert!(out.contains("x=\"21\" y=\"0\" width=\"20\""));
    }

    #[test]
    fn batched_grid_is_one_path() {
        let mut s = Svg::open(100, 50, "t", None);
        s.h_segments(
            &[(0.0, 90.0, 10.0), (0.0, 90.0, 20.0), (0.0, 90.0, 30.0)],
            palette::GRID,
            1.0,
        );
        let out = s.close();
        assert_eq!(out.matches("<path").count(), 1);
        assert!(out.contains("M0 20h180M0 40h180M0 60h180"));
        assert!(out.contains("class=\"grid\""));
    }

    #[test]
    fn theme_seam_and_no_ids() {
        let mut s = Svg::open(100, 50, "t", None);
        s.rect(0.0, 0.0, 10.0, 10.0, palette::BG, None);
        let out = s.close();
        assert!(out.contains("class=\"bg\""));
        assert!(out.contains("prefers-color-scheme:dark"));
        assert!(!out.contains("id="));
    }
}
