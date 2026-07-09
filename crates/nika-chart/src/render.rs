//! Renderers · v0 slice = bar, line, `area_band` (scatter/heatmap = W1.2).
//! Encoding honesty law: PREDICTIONS are dashed · OBSERVATIONS are solid.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)] // plot-space mapping casts (deliberate)
use crate::data::{self, Row};
use crate::error::ChartError;
use crate::fmt;
use crate::layout::{self, Frame, LABEL_PX, TITLE_PX};
use crate::metrics;
use crate::palette;
use crate::scale::{Band, Linear};
use crate::spec::{ChartSpec, Semantic};
use crate::svg::{Anchor, Svg};
use crate::ticks::{TickSpec, extended_wilkinson, extended_wilkinson_opts};

/// Y domain policy. Bars ALWAYS anchor zero (length encoding · Cleveland-
/// `McGill`). Lines/bands encode position: anchor zero only when the data
/// already lives near it (lo < 0.4·hi) · else zoom with 8% padding.
fn y_domain(series: &[&[f64]], field: &str, anchor_zero: bool) -> Result<(f64, f64), ChartError> {
    let mut lo = f64::MAX;
    let mut hi = f64::MIN;
    for s in series {
        if let Some((a, b)) = data::extent(s) {
            lo = lo.min(a);
            hi = hi.max(b);
        }
    }
    if lo > hi {
        return Err(ChartError::EmptyData);
    }
    let anchored = anchor_zero || lo < 0.0 || (hi > 0.0 && lo < 0.4 * hi);
    let (lo, hi) = if anchored {
        (
            if lo < 0.0 { lo * 1.05 } else { 0.0 },
            if hi > 0.0 { hi * 1.05 } else { 0.0 },
        )
    } else {
        let pad = (hi - lo) * 0.08;
        (lo - pad, hi + pad)
    };
    if hi - lo <= 0.0 {
        return Err(ChartError::DomainCollapsed {
            field: field.to_owned(),
        });
    }
    Ok((lo, hi))
}

fn y_ticks(
    lo: f64,
    hi: f64,
    sem: Semantic,
    field: &str,
) -> Result<(TickSpec, Vec<String>), ChartError> {
    let t = extended_wilkinson(lo, hi, 5.0).ok_or(ChartError::DomainCollapsed {
        field: field.to_owned(),
    })?;
    let labels = t.values().iter().map(|v| fmt::label(*v, sem)).collect();
    Ok((t, labels))
}

/// Background + title + the visible data fingerprint (bottom-right · the
/// receipt ON the chart · philosophy law 5). `data8` = `data_sha256` prefix.
fn chrome(svg: &mut Svg, frame: &Frame, title: &str, data8: &str) {
    svg.rect(
        0.0,
        0.0,
        f64::from(frame.canvas_w),
        f64::from(frame.canvas_h),
        palette::BG,
        None,
    );
    svg.text(
        frame.x0,
        14.0 + TITLE_PX * 0.5,
        title,
        TITLE_PX,
        palette::INK,
        Anchor::Start,
        Some(600),
    );
    svg.text(
        f64::from(frame.canvas_w) - 6.0,
        f64::from(frame.canvas_h) - 5.0,
        &format!("nika \u{b7} data {data8}"),
        8.0,
        palette::INK_SOFT,
        Anchor::End,
        None,
    );
}

/// Horizontal gridlines + y tick labels + left axis line.
fn y_grid(svg: &mut Svg, frame: &Frame, t: &TickSpec, labels: &[String], ys: &Linear) {
    let segs: Vec<(f64, f64, f64)> = t
        .values()
        .iter()
        .map(|v| (frame.x0, frame.x0 + frame.pw, ys.map(*v)))
        .collect();
    svg.h_segments(&segs, palette::GRID, 1.0);
    for (v, label) in t.values().iter().zip(labels) {
        svg.text(
            frame.x0 - 8.0,
            ys.map(*v) + LABEL_PX * 0.34,
            label,
            LABEL_PX,
            palette::INK_SOFT,
            Anchor::End,
            None,
        );
    }
    svg.v_segments(
        &[(frame.x0, frame.y0, frame.y0 + frame.ph)],
        palette::AXIS,
        1.0,
    );
    svg.h_segments(
        &[(frame.x0, frame.x0 + frame.pw, frame.y0 + frame.ph)],
        palette::AXIS,
        1.0,
    );
}

/// Linear x ticks under the plot.
fn x_axis_linear(svg: &mut Svg, frame: &Frame, t: &TickSpec, sem: Semantic, xs: &Linear) {
    let mut ticks: Vec<(f64, f64, f64)> = Vec::new();
    for v in t.values() {
        let x = xs.map(v);
        if x < frame.x0 - 0.5 || x > frame.x0 + frame.pw + 0.5 {
            continue;
        }
        ticks.push((x, frame.y0 + frame.ph, frame.y0 + frame.ph + 4.0));
        svg.text(
            x,
            frame.y0 + frame.ph + 6.0 + LABEL_PX,
            &fmt::label(v, sem),
            LABEL_PX,
            palette::INK_SOFT,
            Anchor::Middle,
            None,
        );
    }
    svg.v_segments(&ticks, palette::AXIS, 1.0);
}

/// Bar chart · categories × quantity (per-node duration).
/// # Errors
/// Typed [`crate::error::ChartError`] (`NIKA-BUILTIN-CHART-*`) on invalid spec or data.
pub fn bar(
    spec: &ChartSpec,
    rows: &[Row],
    manifest: &str,
    data8: &str,
) -> Result<String, ChartError> {
    let cats = data::string_column(rows, &spec.x.field)?;
    let vals = data::numeric_column(rows, &spec.y.field)?;
    let (lo, hi) = y_domain(&[&vals], &spec.y.field, true)?;
    let (t, labels) = y_ticks(lo, hi, spec.y.semantic, &spec.y.field)?;
    let frame = layout::frame(spec.width, spec.height, &labels, false);
    let ys = Linear::new(t.lmin, t.lmax(), frame.y0 + frame.ph, frame.y0);
    let band = Band::new(cats.len(), frame.x0, frame.x0 + frame.pw);

    let mut svg = Svg::open(spec.width, spec.height, &spec.title, Some(manifest));
    chrome(&mut svg, &frame, &spec.title, data8);
    y_grid(&mut svg, &frame, &t, &labels, &ys);

    let zero_y = ys.map(0.0);
    for (i, v) in vals.iter().enumerate() {
        let (x, w) = band.slot(i);
        let yv = ys.map(*v);
        let (top, h) = if yv <= zero_y {
            (yv, zero_y - yv)
        } else {
            (zero_y, yv - zero_y)
        };
        svg.rect(x, top, w, h.max(0.0), palette::categorical(0), None);
        // Value label above the bar when it fits.
        let vl = fmt::label(*v, spec.y.semantic);
        if metrics::measure(&vl, 10.0) <= band.step_width() {
            svg.text(
                x + w / 2.0,
                top - 4.0,
                &vl,
                10.0,
                palette::INK_SOFT,
                Anchor::Middle,
                None,
            );
        }
    }
    for (i, cat) in cats.iter().enumerate() {
        let label = metrics::truncate(cat, LABEL_PX, band.step_width() - 4.0);
        svg.text(
            band.center(i),
            frame.y0 + frame.ph + 6.0 + LABEL_PX,
            &label,
            LABEL_PX,
            palette::INK_SOFT,
            Anchor::Middle,
            None,
        );
    }
    Ok(svg.close())
}

/// Line chart · ordered quantity (run-over-run cost) + point markers.
/// # Errors
/// Typed [`crate::error::ChartError`] (`NIKA-BUILTIN-CHART-*`) on invalid spec or data.
pub fn line(
    spec: &ChartSpec,
    rows: &[Row],
    manifest: &str,
    data8: &str,
) -> Result<String, ChartError> {
    let xv = data::numeric_column(rows, &spec.x.field)?;
    let yv = data::numeric_column(rows, &spec.y.field)?;
    let (xlo, xhi) = data::extent(&xv).ok_or(ChartError::EmptyData)?;
    let xt = extended_wilkinson_opts(xlo, xhi, 6.0, spec.x.semantic == Semantic::Count).ok_or(
        ChartError::DomainCollapsed {
            field: spec.x.field.clone(),
        },
    )?;
    // Optional series split (color channel · one line per category ·
    // first-appearance order · Okabe-Ito cycle).
    let series: Option<SeriesSplit> = match &spec.color {
        Some(ch) => {
            let cats = data::string_column(rows, &ch.field)?;
            let names = uniq_ordered(&cats);
            let groups = names
                .iter()
                .map(|name| {
                    let mut sx = Vec::new();
                    let mut sy = Vec::new();
                    for ((x, y), c) in xv.iter().zip(&yv).zip(&cats) {
                        if c == name {
                            sx.push(*x);
                            sy.push(*y);
                        }
                    }
                    (sx, sy)
                })
                .collect();
            Some((names, groups))
        }
        None => None,
    };

    let (lo, hi) = y_domain(&[&yv], &spec.y.field, false)?;
    let (t, labels) = y_ticks(lo, hi, spec.y.semantic, &spec.y.field)?;
    let frame = layout::frame(spec.width, spec.height, &labels, series.is_some());
    let ys = Linear::new(t.lmin, t.lmax(), frame.y0 + frame.ph, frame.y0);
    let xs = Linear::new(xt.lmin, xt.lmax(), frame.x0, frame.x0 + frame.pw);

    let mut svg = Svg::open(spec.width, spec.height, &spec.title, Some(manifest));
    chrome(&mut svg, &frame, &spec.title, data8);
    y_grid(&mut svg, &frame, &t, &labels, &ys);
    x_axis_linear(&mut svg, &frame, &xt, spec.x.semantic, &xs);

    // Above ~2 points/px the polyline is overdrawn noise: LTTB to the pixel
    // budget (deterministic · Steinarsson 2013 · keeps extremes) · markers
    // only when sparse enough to read individually.
    let budget = (frame.pw as usize).max(64) * 2;
    let draw_series = |svg: &mut Svg, sxv: &[f64], syv: &[f64], color: &'static str| {
        let raw: Vec<(f64, f64)> = sxv
            .iter()
            .zip(syv)
            .map(|(x, y)| (xs.map(*x), ys.map(*y)))
            .collect();
        let pts = crate::decimate::lttb(&raw, budget);
        svg.polyline(&pts, color, 1.8, None);
        if pts.len() <= 160 {
            for (x, y) in &pts {
                svg.dot(*x, *y, 2.4, color, None);
            }
        }
    };
    match &series {
        Some((names, groups)) => {
            for (i, (sxv, syv)) in groups.iter().enumerate() {
                draw_series(&mut svg, sxv, syv, palette::categorical(i));
            }
            let entries: Vec<(Glyph, &str)> = names
                .iter()
                .enumerate()
                .map(|(i, n)| (Glyph::SolidLine(palette::categorical(i)), n.as_str()))
                .collect();
            legend(&mut svg, &frame, &entries);
        }
        None => draw_series(&mut svg, &xv, &yv, palette::categorical(0)),
    }
    Ok(svg.close())
}

/// Per-series split: names (first-appearance order) + (x, y) column pairs.
type SeriesSplit = (Vec<String>, Vec<(Vec<f64>, Vec<f64>)>);

/// Legend entry glyph kinds.
enum Glyph {
    BandSwatch(&'static str),
    DashLine(&'static str),
    SolidLine(&'static str),
}

fn legend(svg: &mut Svg, frame: &Frame, entries: &[(Glyph, &str)]) {
    let y = frame.y0 - 10.0;
    let mut x = frame.x0;
    for (glyph, label) in entries {
        match glyph {
            Glyph::BandSwatch(color) => svg.rect(x, y - 7.0, 16.0, 9.0, color, Some(0.22)),
            Glyph::DashLine(color) => svg.polyline(
                &[(x, y - 2.5), (x + 16.0, y - 2.5)],
                color,
                1.8,
                Some((4, 3)),
            ),
            Glyph::SolidLine(color) => {
                svg.polyline(&[(x, y - 2.5), (x + 16.0, y - 2.5)], color, 1.8, None);
            }
        }
        x += 20.0;
        svg.text(
            x,
            y + 1.0,
            label,
            10.0,
            palette::INK_SOFT,
            Anchor::Start,
            None,
        );
        x += metrics::measure(label, 10.0) + 16.0;
    }
}

/// Forecast band · `y_lo..y_hi` band + dashed forecast line (y) + optional
/// solid actual overlay (y2). Predictions dashed · observations solid.
/// # Errors
/// Typed [`crate::error::ChartError`] (`NIKA-BUILTIN-CHART-*`) on invalid spec or data.
pub fn area_band(
    spec: &ChartSpec,
    rows: &[Row],
    manifest: &str,
    data8: &str,
) -> Result<String, ChartError> {
    let (Some(lo_ch), Some(hi_ch)) = (&spec.y_lo, &spec.y_hi) else {
        return Err(ChartError::InvalidSpec {
            reason: "area_band requires y_lo and y_hi".to_owned(),
        });
    };
    let xv = data::numeric_column(rows, &spec.x.field)?;
    let lov = data::numeric_column(rows, &lo_ch.field)?;
    let hiv = data::numeric_column(rows, &hi_ch.field)?;
    let midv = data::numeric_column(rows, &spec.y.field)?;
    let actual = match &spec.y2 {
        Some(ch) => Some(data::numeric_column(rows, &ch.field)?),
        None => None,
    };

    let (xlo, xhi) = data::extent(&xv).ok_or(ChartError::EmptyData)?;
    let xt = extended_wilkinson_opts(xlo, xhi, 6.0, spec.x.semantic == Semantic::Count).ok_or(
        ChartError::DomainCollapsed {
            field: spec.x.field.clone(),
        },
    )?;
    let mut series: Vec<&[f64]> = vec![&lov, &hiv, &midv];
    if let Some(a) = &actual {
        series.push(a);
    }
    let (lo, hi) = y_domain(&series, &spec.y.field, false)?;
    let (t, labels) = y_ticks(lo, hi, spec.y.semantic, &spec.y.field)?;
    let frame = layout::frame(spec.width, spec.height, &labels, true);
    let ys = Linear::new(t.lmin, t.lmax(), frame.y0 + frame.ph, frame.y0);
    let xs = Linear::new(xt.lmin, xt.lmax(), frame.x0, frame.x0 + frame.pw);

    let mut svg = Svg::open(spec.width, spec.height, &spec.title, Some(manifest));
    chrome(&mut svg, &frame, &spec.title, data8);
    y_grid(&mut svg, &frame, &t, &labels, &ys);
    x_axis_linear(&mut svg, &frame, &xt, spec.x.semantic, &xs);

    // Band polygon: upper polyline forward + lower reversed + Z.
    let blue = palette::categorical(0);
    let up: Vec<(f64, f64)> = xv
        .iter()
        .zip(&hiv)
        .map(|(x, y)| (xs.map(*x), ys.map(*y)))
        .collect();
    let lo_pts: Vec<(f64, f64)> = xv
        .iter()
        .zip(&lov)
        .map(|(x, y)| (xs.map(*x), ys.map(*y)))
        .collect();
    svg.band_path(&up, &lo_pts, blue, 0.18);

    let mid_pts: Vec<(f64, f64)> = xv
        .iter()
        .zip(&midv)
        .map(|(x, y)| (xs.map(*x), ys.map(*y)))
        .collect();
    svg.polyline(&mid_pts, blue, 1.8, Some((4, 3)));
    if let Some(av) = &actual {
        let a_pts: Vec<(f64, f64)> = xv
            .iter()
            .zip(av)
            .map(|(x, y)| (xs.map(*x), ys.map(*y)))
            .collect();
        svg.polyline(&a_pts, "#D55E00", 1.8, None);
        for (x, y) in &a_pts {
            svg.dot(*x, *y, 2.2, "#D55E00", None);
        }
    }

    let lo_name = lo_ch.field.as_str();
    let hi_name = hi_ch.field.as_str();
    let band_label = format!("{lo_name}\u{2013}{hi_name}");
    let mut entries: Vec<(Glyph, &str)> = vec![
        (Glyph::BandSwatch(blue), band_label.as_str()),
        (Glyph::DashLine(blue), "forecast"),
    ];
    if actual.is_some() {
        entries.push((Glyph::SolidLine("#D55E00"), "actual"));
    }
    legend(&mut svg, &frame, &entries);
    Ok(svg.close())
}

/// Ordered-unique categories (first-appearance order · deterministic).
pub(crate) fn uniq_ordered(vals: &[String]) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for v in vals {
        if seen.insert(v.clone()) {
            out.push(v.clone());
        }
    }
    out
}

/// Scatter · two quantities (cost vs duration outliers) · exact-duplicate
/// dedupe on the 0.01px grid (stable first-wins) + translucent dots.
/// # Errors
/// Typed [`crate::error::ChartError`] (`NIKA-BUILTIN-CHART-*`) on invalid spec or data.
pub fn scatter(
    spec: &ChartSpec,
    rows: &[Row],
    manifest: &str,
    data8: &str,
) -> Result<String, ChartError> {
    let xv = data::numeric_column(rows, &spec.x.field)?;
    let yv = data::numeric_column(rows, &spec.y.field)?;
    let (xlo, xhi) = data::extent(&xv).ok_or(ChartError::EmptyData)?;
    let xpad = ((xhi - xlo) * 0.06).max(f64::MIN_POSITIVE);
    // Padding never invents a sign the data does not have.
    let padded_lo = if xlo >= 0.0 {
        (xlo - xpad).max(0.0)
    } else {
        xlo - xpad
    };
    let xt = extended_wilkinson_opts(
        padded_lo,
        xhi + xpad,
        6.0,
        spec.x.semantic == Semantic::Count,
    )
    .ok_or(ChartError::DomainCollapsed {
        field: spec.x.field.clone(),
    })?;
    let (lo, hi) = y_domain(&[&yv], &spec.y.field, false)?;
    let (t, labels) = y_ticks(lo, hi, spec.y.semantic, &spec.y.field)?;
    let frame = layout::frame(spec.width, spec.height, &labels, false);
    let ys = Linear::new(t.lmin, t.lmax(), frame.y0 + frame.ph, frame.y0);
    let xs = Linear::new(xt.lmin, xt.lmax(), frame.x0, frame.x0 + frame.pw);

    let mut svg = Svg::open(spec.width, spec.height, &spec.title, Some(manifest));
    chrome(&mut svg, &frame, &spec.title, data8);
    y_grid(&mut svg, &frame, &t, &labels, &ys);
    x_axis_linear(&mut svg, &frame, &xt, spec.x.semantic, &xs);

    let mut seen = std::collections::BTreeSet::new();
    for (x, y) in xv.iter().zip(&yv) {
        let px = xs.map(*x);
        let py = ys.map(*y);
        let key = ((px * 100.0).round() as i64, (py * 100.0).round() as i64);
        if seen.insert(key) {
            svg.dot(px, py, 3.0, palette::categorical(0), Some(0.72));
        }
    }
    Ok(svg.close())
}

/// Normalized ramp color for a heatmap cell value.
fn heat_color(v: f64, min: f64, max: f64, sem: Semantic) -> String {
    if sem == Semantic::Delta {
        let m = min.abs().max(max.abs());
        if m <= 0.0 {
            return palette::coolwarm(0.5);
        }
        palette::coolwarm(0.5 + v / (2.0 * m))
    } else {
        let span = max - min;
        if span <= 0.0 {
            return palette::viridis(0.5);
        }
        palette::viridis((v - min) / span)
    }
}

/// Heatmap · category × category → quantity color (step × run flakiness).
/// The value channel is `spec.color`. Missing cells get an EXPLICIT no-data
/// fill (philosophy law 6 · never silent-skip).
/// # Errors
/// Typed [`crate::error::ChartError`] (`NIKA-BUILTIN-CHART-*`) on invalid spec or data.
pub fn heatmap(
    spec: &ChartSpec,
    rows: &[Row],
    manifest: &str,
    data8: &str,
) -> Result<String, ChartError> {
    let val_ch = spec.color.as_ref().ok_or(ChartError::InvalidSpec {
        reason: "heatmap requires a color channel carrying the value field".to_owned(),
    })?;
    let xc = data::string_column(rows, &spec.x.field)?;
    let yc = data::string_column(rows, &spec.y.field)?;
    let vv = data::numeric_column(rows, &val_ch.field)?;
    let xcats = uniq_ordered(&xc);
    let ycats = uniq_ordered(&yc);
    let (vmin, vmax) = data::extent(&vv).ok_or(ChartError::EmptyData)?;

    let frame = layout::frame(spec.width, spec.height, &ycats, true);
    let mut svg = Svg::open(spec.width, spec.height, &spec.title, Some(manifest));
    chrome(&mut svg, &frame, &spec.title, data8);

    let nx = xcats.len().max(1) as f64;
    let ny = ycats.len().max(1) as f64;
    let cw = frame.pw / nx;
    let ch = frame.ph / ny;

    // No-data base: one explicit backdrop under the cells.
    svg.nodata(frame.x0, frame.y0, frame.pw, frame.ph);

    let mut filled = std::collections::BTreeSet::new();
    for ((x, y), v) in xc.iter().zip(&yc).zip(&vv) {
        let xi = xcats.iter().position(|c| c == x).unwrap_or(0);
        let yi = ycats.iter().position(|c| c == y).unwrap_or(0);
        if filled.insert((xi, yi)) {
            svg.cell(
                frame.x0 + cw * (xi as f64),
                frame.y0 + ch * (yi as f64),
                cw,
                ch,
                &heat_color(*v, vmin, vmax, val_ch.semantic),
            );
        }
    }

    for (i, cat) in xcats.iter().enumerate() {
        let label = metrics::truncate(cat, LABEL_PX, cw - 4.0);
        svg.text(
            frame.x0 + cw * (i as f64 + 0.5),
            frame.y0 + frame.ph + 6.0 + LABEL_PX,
            &label,
            LABEL_PX,
            palette::INK_SOFT,
            Anchor::Middle,
            None,
        );
    }
    for (i, cat) in ycats.iter().enumerate() {
        svg.text(
            frame.x0 - 8.0,
            frame.y0 + ch * (i as f64 + 0.5) + LABEL_PX * 0.34,
            cat,
            LABEL_PX,
            palette::INK_SOFT,
            Anchor::End,
            None,
        );
    }

    // Discrete swatch legend (never a gradient · §3bis) + min/max labels.
    let ly = frame.y0 - 10.0;
    let mut lx = frame.x0;
    let min_l = fmt::label(vmin, val_ch.semantic);
    svg.text(
        lx,
        ly + 1.0,
        &min_l,
        10.0,
        palette::INK_SOFT,
        Anchor::Start,
        None,
    );
    lx += metrics::measure(&min_l, 10.0) + 6.0;
    for i in 0..7 {
        let tt = f64::from(i) / 6.0;
        let v = vmin + tt * (vmax - vmin);
        svg.cell(
            lx,
            ly - 7.0,
            14.0,
            9.0,
            &heat_color(v, vmin, vmax, val_ch.semantic),
        );
        lx += 14.0;
    }
    lx += 6.0;
    svg.text(
        lx,
        ly + 1.0,
        &fmt::label(vmax, val_ch.semantic),
        10.0,
        palette::INK_SOFT,
        Anchor::Start,
        None,
    );
    Ok(svg.close())
}
