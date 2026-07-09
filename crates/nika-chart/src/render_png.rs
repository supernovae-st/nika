//! PNG chart renderer · the 4th surface (SVG · Vega-Lite · TTY · PNG).
//! v0 slice = bar (the per-step receipt) — same recipe, raster projection.
//! Where SVG theming is flaky (Safari `<img>` · raster pipelines), the PNG
//! is a single fixed-light artifact — explicit, not adaptive.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::many_single_char_names
)] // pixel math domain (x/y/w/h idiom · deliberate quantization)
use crate::data::{self, Row};
use crate::error::ChartError;
use crate::fmt;
use crate::raster::{Raster, Rgb};
use crate::spec::ChartSpec;
use crate::ticks::{extended_wilkinson, extended_wilkinson_opts};

/// font8 is ASCII-only: map the house separators · fold the rest to '?'.
fn ascii_title(s: &str, max_chars: usize) -> String {
    let mapped: String = s
        .chars()
        .map(|c| match c {
            '\u{b7}' | '\u{2013}' | '\u{2014}' => '-',
            '\u{d7}' => 'x',
            c if c.is_ascii() => c,
            _ => '?',
        })
        .collect();
    if mapped.chars().count() <= max_chars {
        mapped
    } else {
        let cut: String = mapped.chars().take(max_chars.saturating_sub(2)).collect();
        format!("{cut}..")
    }
}

const BG: Rgb = (255, 255, 255);
const INK: Rgb = (26, 26, 26);
const INK_SOFT: Rgb = (85, 85, 85);
const GRID: Rgb = (230, 230, 230);
const AXIS: Rgb = (154, 154, 154);
const BLUE: Rgb = (0x00, 0x72, 0xB2); // Okabe-Ito series 0

/// Bar chart → PNG bytes. Zero-anchored (length encoding law).
/// # Errors
/// Typed [`crate::error::ChartError`] (`NIKA-BUILTIN-CHART-*`) on invalid spec or data.
pub fn bar(spec: &ChartSpec, rows: &[Row], data8: &str) -> Result<Vec<u8>, ChartError> {
    let cats = data::string_column(rows, &spec.x.field)?;
    let vals = data::numeric_column(rows, &spec.y.field)?;
    let (_, hi_raw) = data::extent(&vals).ok_or(ChartError::EmptyData)?;
    let hi = if hi_raw > 0.0 { hi_raw * 1.05 } else { 1.0 };
    let t = extended_wilkinson(0.0, hi, 5.0).ok_or(ChartError::DomainCollapsed {
        field: spec.y.field.clone(),
    })?;
    let labels: Vec<String> = t
        .values()
        .iter()
        .map(|v| fmt::label(*v, spec.y.semantic))
        .collect();
    let label_w = labels
        .iter()
        .map(|l| Raster::text_width(l, 1))
        .max()
        .unwrap_or(8);

    let w = spec.width as i32;
    let h = spec.height as i32;
    let left = label_w + 14;
    let top = 34;
    let bottom = 38; // category labels + the fingerprint strip below them
    let right = 10;
    let pw = (w - left - right).max(40);
    let ph = (h - top - bottom).max(30);

    let mut r = Raster::new(spec.width, spec.height, BG);
    let title = ascii_title(&spec.title, (((w - left - right) / 16).max(4)) as usize);
    r.text(left, 10, &title, 2, INK);

    // Gridlines + y labels (top of plot = t.lmax).
    let lmax = t.lmax();
    let y_of = |v: f64| top + ((1.0 - v / lmax) * f64::from(ph)).round() as i32;
    for (v, label) in t.values().iter().zip(&labels) {
        let y = y_of(*v);
        r.hline(left, left + pw, y, GRID);
        r.text(
            left - 6 - Raster::text_width(label, 1),
            y - 4,
            label,
            1,
            INK_SOFT,
        );
    }
    r.vline(left, top, top + ph, AXIS);
    r.hline(left, left + pw, top + ph, AXIS);

    // Bars + category labels + value labels.
    let n = cats.len().max(1) as i32;
    let step = pw / n;
    let pad = step / 7;
    for (i, (cat, v)) in cats.iter().zip(&vals).enumerate() {
        let x = left + step * (i as i32) + pad;
        let bw = step - 2 * pad;
        let y = y_of(v.max(0.0));
        r.fill_rect(x, y, bw, (top + ph - y).max(0), BLUE);
        let vl = fmt::label(*v, spec.y.semantic);
        if Raster::text_width(&vl, 1) <= step {
            r.text(
                x + bw / 2 - Raster::text_width(&vl, 1) / 2,
                y - 10,
                &vl,
                1,
                INK_SOFT,
            );
        }
        let cl_w = Raster::text_width(cat, 1);
        if cl_w <= step {
            r.text(x + bw / 2 - cl_w / 2, top + ph + 8, cat, 1, INK_SOFT);
        }
    }

    // Visible fingerprint (philosophy law 5).
    let fp = format!("nika data {data8}");
    r.text(w - Raster::text_width(&fp, 1) - 4, h - 12, &fp, 1, INK_SOFT);
    Ok(r.to_png())
}

const ORANGE: Rgb = (0xD5, 0x5E, 0x00); // Okabe-Ito vermilion (actual overlay)
const BAND: Rgb = (0xC2, 0xDA, 0xEC); // light blue band fill (blue @ ~18% on white)

/// Shared numeric-x frame: y ticks + labels + grid + axes → mapping closures.
struct NumFrame {
    left: i32,
    top: i32,
    pw: i32,
    ph: i32,
    xlo: f64,
    xhi: f64,
    ylo: f64,
    yhi: f64,
}

impl NumFrame {
    fn x_of(&self, v: f64) -> i32 {
        self.left + (((v - self.xlo) / (self.xhi - self.xlo)) * f64::from(self.pw)).round() as i32
    }

    fn y_of(&self, v: f64) -> i32 {
        self.top
            + ((1.0 - (v - self.ylo) / (self.yhi - self.ylo)) * f64::from(self.ph)).round() as i32
    }
}

#[allow(clippy::too_many_arguments)]
fn num_frame(
    r: &mut Raster,
    spec: &ChartSpec,
    title_data8: (&str, &str),
    xlo: f64,
    xhi: f64,
    ylo: f64,
    yhi: f64,
    y_sem: crate::spec::Semantic,
    x_sem: crate::spec::Semantic,
) -> Result<NumFrame, ChartError> {
    let t = extended_wilkinson(ylo, yhi, 5.0).ok_or(ChartError::DomainCollapsed {
        field: "y".to_owned(),
    })?;
    let labels: Vec<String> = t.values().iter().map(|v| fmt::label(*v, y_sem)).collect();
    let label_w = labels
        .iter()
        .map(|l| Raster::text_width(l, 1))
        .max()
        .unwrap_or(8);
    let w = spec.width as i32;
    let h = spec.height as i32;
    let f = NumFrame {
        left: label_w + 14,
        top: 34,
        pw: (w - label_w - 14 - 10).max(40),
        ph: (h - 34 - 38).max(30),
        xlo,
        xhi,
        ylo: t.lmin,
        yhi: t.lmax(),
    };
    let (title, data8) = title_data8;
    let fitted = ascii_title(title, (((w - f.left - 10) / 16).max(4)) as usize);
    r.text(f.left, 10, &fitted, 2, INK);
    for (v, label) in t.values().iter().zip(&labels) {
        let y = f.y_of(*v);
        r.hline(f.left, f.left + f.pw, y, GRID);
        r.text(
            f.left - 6 - Raster::text_width(label, 1),
            y - 4,
            label,
            1,
            INK_SOFT,
        );
    }
    r.vline(f.left, f.top, f.top + f.ph, AXIS);
    r.hline(f.left, f.left + f.pw, f.top + f.ph, AXIS);
    // X ticks (integer search for Count · run numbers).
    if let Some(xt) = extended_wilkinson_opts(xlo, xhi, 6.0, x_sem == crate::spec::Semantic::Count)
    {
        for v in xt.values() {
            if v < xlo - 1e-9 || v > xhi + 1e-9 {
                continue;
            }
            let x = f.x_of(v);
            r.vline(x, f.top + f.ph, f.top + f.ph + 3, AXIS);
            let label = fmt::label(v, x_sem);
            r.text(
                x - Raster::text_width(&label, 1) / 2,
                f.top + f.ph + 7,
                &label,
                1,
                INK_SOFT,
            );
        }
    }
    let fp = format!("nika data {data8}");
    r.text(w - Raster::text_width(&fp, 1) - 4, h - 12, &fp, 1, INK_SOFT);
    Ok(f)
}

/// Line chart → PNG (single series · run-over-run).
/// # Errors
/// Typed [`crate::error::ChartError`] (`NIKA-BUILTIN-CHART-*`) on invalid spec or data.
pub fn line(spec: &ChartSpec, rows: &[Row], data8: &str) -> Result<Vec<u8>, ChartError> {
    let xv = data::numeric_column(rows, &spec.x.field)?;
    let yv = data::numeric_column(rows, &spec.y.field)?;
    let (xlo, xhi) = data::extent(&xv).ok_or(ChartError::EmptyData)?;
    let (dlo, dhi) = data::extent(&yv).ok_or(ChartError::EmptyData)?;
    if xhi - xlo <= 0.0 {
        return Err(ChartError::DomainCollapsed {
            field: spec.x.field.clone(),
        });
    }
    let anchored = dlo >= 0.0 && dlo < 0.4 * dhi;
    let (ylo, yhi) = if anchored {
        (0.0, dhi * 1.05)
    } else {
        (dlo - (dhi - dlo) * 0.08, dhi + (dhi - dlo) * 0.08)
    };
    let mut r = Raster::new(spec.width, spec.height, BG);
    let f = num_frame(
        &mut r,
        spec,
        (&spec.title, data8),
        xlo,
        xhi,
        ylo,
        yhi,
        spec.y.semantic,
        spec.x.semantic,
    )?;
    let pts: Vec<(f64, f64)> = xv.iter().copied().zip(yv.iter().copied()).collect();
    let pts = crate::decimate::lttb(&pts, (f.pw as usize).max(64) * 2);
    for w2 in pts.windows(2) {
        let &[a, b] = w2 else { continue };
        r.line(f.x_of(a.0), f.y_of(a.1), f.x_of(b.0), f.y_of(b.1), BLUE);
    }
    if pts.len() <= 160 {
        for (x, y) in &pts {
            r.disc(f.x_of(*x), f.y_of(*y), 2, BLUE);
        }
    }
    Ok(r.to_png())
}

/// Forecast band → PNG (p50/p90 fill · dashed p50 · solid actual).
/// # Errors
/// Typed [`crate::error::ChartError`] (`NIKA-BUILTIN-CHART-*`) on invalid spec or data.
pub fn area_band(spec: &ChartSpec, rows: &[Row], data8: &str) -> Result<Vec<u8>, ChartError> {
    let (Some(lo_ch), Some(hi_ch)) = (&spec.y_lo, &spec.y_hi) else {
        return Err(ChartError::InvalidSpec {
            reason: "area_band requires y_lo and y_hi".to_owned(),
        });
    };
    let xv = data::numeric_column(rows, &spec.x.field)?;
    let lows = data::numeric_column(rows, &lo_ch.field)?;
    let highs = data::numeric_column(rows, &hi_ch.field)?;
    let actual = match &spec.y2 {
        Some(ch) => Some(data::numeric_column(rows, &ch.field)?),
        None => None,
    };
    let (xlo, xhi) = data::extent(&xv).ok_or(ChartError::EmptyData)?;
    let mut all = lows.clone();
    all.extend_from_slice(&highs);
    if let Some(a) = &actual {
        all.extend_from_slice(a);
    }
    let (dlo, dhi) = data::extent(&all).ok_or(ChartError::EmptyData)?;
    let pad = (dhi - dlo).max(1.0) * 0.08;
    let mut r = Raster::new(spec.width, spec.height, BG);
    let f = num_frame(
        &mut r,
        spec,
        (&spec.title, data8),
        xlo,
        xhi,
        dlo - pad,
        dhi + pad,
        spec.y.semantic,
        spec.x.semantic,
    )?;
    // Band fill: per-x-column vertical spans between interpolated lo/hi.
    for px in f.x_of(xlo)..=f.x_of(xhi) {
        let xval = f.xlo + (f64::from(px - f.left) / f64::from(f.pw)) * (f.xhi - f.xlo);
        let interp = |ys: &[f64]| -> Option<f64> {
            let pos = xv.iter().position(|x| *x >= xval)?;
            if pos == 0 {
                return ys.first().copied();
            }
            let (x0, x1) = (*xv.get(pos - 1)?, *xv.get(pos)?);
            let (y0, y1) = (*ys.get(pos - 1)?, *ys.get(pos)?);
            let t = if x1 > x0 {
                (xval - x0) / (x1 - x0)
            } else {
                0.0
            };
            Some(y0 + t * (y1 - y0))
        };
        if let (Some(lo), Some(hi)) = (interp(&lows), interp(&highs)) {
            r.vline(px, f.y_of(hi), f.y_of(lo), BAND);
        }
    }
    // Dashed p50: alternate 4-on/3-off segments along x pixels.
    let mid_pts: Vec<(i32, i32)> = xv
        .iter()
        .zip(&lows)
        .map(|(x, y)| (f.x_of(*x), f.y_of(*y)))
        .collect();
    for w2 in mid_pts.windows(2) {
        let &[a, b] = w2 else { continue };
        let seg = ((b.0 - a.0).abs()).max(1);
        for s in (0..seg).step_by(7) {
            let t0 = f64::from(s) / f64::from(seg);
            let t1 = (f64::from(s) + 4.0).min(f64::from(seg)) / f64::from(seg);
            let x0 = a.0 + ((f64::from(b.0 - a.0)) * t0).round() as i32;
            let y0 = a.1 + ((f64::from(b.1 - a.1)) * t0).round() as i32;
            let x1 = a.0 + ((f64::from(b.0 - a.0)) * t1).round() as i32;
            let y1 = a.1 + ((f64::from(b.1 - a.1)) * t1).round() as i32;
            r.line(x0, y0, x1, y1, BLUE);
        }
    }
    if let Some(av) = &actual {
        let a_pts: Vec<(i32, i32)> = xv
            .iter()
            .zip(av)
            .map(|(x, y)| (f.x_of(*x), f.y_of(*y)))
            .collect();
        for w2 in a_pts.windows(2) {
            let &[a, b] = w2 else { continue };
            r.line(a.0, a.1, b.0, b.1, ORANGE);
        }
        for (x, y) in &a_pts {
            r.disc(*x, *y, 2, ORANGE);
        }
    }
    Ok(r.to_png())
}

/// Scatter → PNG · translucent-free raster: grid-dedupe keeps ink honest.
/// # Errors
/// Typed [`crate::error::ChartError`] (`NIKA-BUILTIN-CHART-*`) on invalid spec or data.
pub fn scatter(spec: &ChartSpec, rows: &[Row], data8: &str) -> Result<Vec<u8>, ChartError> {
    let xv = data::numeric_column(rows, &spec.x.field)?;
    let yv = data::numeric_column(rows, &spec.y.field)?;
    let (xlo0, xhi0) = data::extent(&xv).ok_or(ChartError::EmptyData)?;
    let (dlo, dhi) = data::extent(&yv).ok_or(ChartError::EmptyData)?;
    let xpad = ((xhi0 - xlo0) * 0.06).max(f64::MIN_POSITIVE);
    let xlo = if xlo0 >= 0.0 {
        (xlo0 - xpad).max(0.0)
    } else {
        xlo0 - xpad
    };
    let xhi = xhi0 + xpad;
    if xhi - xlo <= 0.0 {
        return Err(ChartError::DomainCollapsed {
            field: spec.x.field.clone(),
        });
    }
    let anchored = dlo >= 0.0 && dlo < 0.4 * dhi;
    let (ylo, yhi) = if anchored {
        (0.0, dhi * 1.05)
    } else {
        (dlo - (dhi - dlo) * 0.08, dhi + (dhi - dlo) * 0.08)
    };
    let mut r = Raster::new(spec.width, spec.height, BG);
    let f = num_frame(
        &mut r,
        spec,
        (&spec.title, data8),
        xlo,
        xhi,
        ylo,
        yhi,
        spec.y.semantic,
        spec.x.semantic,
    )?;
    let mut seen = std::collections::BTreeSet::new();
    for (x, y) in xv.iter().zip(&yv) {
        let (px, py) = (f.x_of(*x), f.y_of(*y));
        if seen.insert((px, py)) {
            r.disc(px, py, 2, BLUE);
        }
    }
    Ok(r.to_png())
}

/// Heatmap → PNG · categorical grid + ramp cells + swatch legend.
/// # Errors
/// Typed [`crate::error::ChartError`] (`NIKA-BUILTIN-CHART-*`) on invalid spec or data.
pub fn heatmap(spec: &ChartSpec, rows: &[Row], data8: &str) -> Result<Vec<u8>, ChartError> {
    let val_ch = spec.color.as_ref().ok_or(ChartError::InvalidSpec {
        reason: "heatmap requires a color channel carrying the value field".to_owned(),
    })?;
    let xc = data::string_column(rows, &spec.x.field)?;
    let yc = data::string_column(rows, &spec.y.field)?;
    let vv = data::numeric_column(rows, &val_ch.field)?;
    let (vmin, vmax) = data::extent(&vv).ok_or(ChartError::EmptyData)?;
    let xcats = crate::render::uniq_ordered(&xc);
    let ycats = crate::render::uniq_ordered(&yc);

    let w = spec.width as i32;
    let h = spec.height as i32;
    let ylab_w = ycats
        .iter()
        .map(|c| Raster::text_width(c, 1))
        .max()
        .unwrap_or(8);
    let left = ylab_w + 14;
    let top = 44;
    let bottom = 38;
    let pw = (w - left - 10).max(40);
    let ph = (h - top - bottom).max(30);

    let mut r = Raster::new(spec.width, spec.height, BG);
    let title = ascii_title(&spec.title, (((w - left - 10) / 16).max(4)) as usize);
    r.text(left, 8, &title, 2, INK);

    let hex_rgb = |s: &str| -> Rgb {
        let p = |i: usize| u8::from_str_radix(s.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
        (p(1), p(3), p(5))
    };
    let color_of = |v: f64| -> Rgb {
        if val_ch.semantic == crate::spec::Semantic::Delta {
            let m = vmin.abs().max(vmax.abs());
            let t = if m <= 0.0 { 0.5 } else { 0.5 + v / (2.0 * m) };
            hex_rgb(&crate::palette::coolwarm(t))
        } else {
            let span = vmax - vmin;
            let t = if span <= 0.0 { 0.5 } else { (v - vmin) / span };
            hex_rgb(&crate::palette::viridis(t))
        }
    };

    // No-data backdrop, then cells (first-wins on duplicates).
    r.fill_rect(left, top, pw, ph, (0xEC, 0xEE, 0xF0));
    let nx = xcats.len().max(1) as i32;
    let ny = ycats.len().max(1) as i32;
    let mut filled = std::collections::BTreeSet::new();
    for ((x, y), v) in xc.iter().zip(&yc).zip(&vv) {
        let xi = xcats.iter().position(|c| c == x).unwrap_or(0) as i32;
        let yi = ycats.iter().position(|c| c == y).unwrap_or(0) as i32;
        if filled.insert((xi, yi)) {
            let x0 = left + pw * xi / nx;
            let x1 = left + pw * (xi + 1) / nx;
            let y0 = top + ph * yi / ny;
            let y1 = top + ph * (yi + 1) / ny;
            r.fill_rect(x0, y0, x1 - x0, y1 - y0, color_of(*v));
        }
    }
    for (i, cat) in xcats.iter().enumerate() {
        let cx = left + pw * (i as i32) / nx + pw / (2 * nx);
        r.text(
            cx - Raster::text_width(cat, 1) / 2,
            top + ph + 8,
            cat,
            1,
            INK_SOFT,
        );
    }
    for (i, cat) in ycats.iter().enumerate() {
        let cy = top + ph * (i as i32) / ny + ph / (2 * ny);
        r.text(
            left - 6 - Raster::text_width(cat, 1),
            cy - 4,
            cat,
            1,
            INK_SOFT,
        );
    }
    // Swatch legend strip (7 discrete · min/max labels).
    let min_l = fmt::label(vmin, val_ch.semantic);
    let max_l = fmt::label(vmax, val_ch.semantic);
    let mut lx = left;
    r.text(lx, top - 16, &min_l, 1, INK_SOFT);
    lx += Raster::text_width(&min_l, 1) + 6;
    for i in 0..7 {
        let v = vmin + (f64::from(i) / 6.0) * (vmax - vmin);
        r.fill_rect(lx, top - 18, 14, 10, color_of(v));
        lx += 14;
    }
    r.text(lx + 6, top - 16, &max_l, 1, INK_SOFT);

    let fp = format!("nika data {data8}");
    r.text(w - Raster::text_width(&fp, 1) - 4, h - 12, &fp, 1, INK_SOFT);
    Ok(r.to_png())
}
