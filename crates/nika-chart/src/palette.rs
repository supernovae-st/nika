//! Embedded palettes (zero-dep · operator lock « pas de dépendances »).
//!
//! · Categorical: Okabe-Ito CVD-safe hues, PER-MODE steps — dark mode is a
//!   selected palette (its own steps validated against the dark surface),
//!   never an automatic flip. Both sets pass the six computable checks
//!   (lightness band L 0.43–0.77 light / 0.48–0.67 dark · chroma ≥ 0.1 ·
//!   adjacent CVD ΔE ≥ 12 [worst pair 20.7] · contrast vs surface) — run,
//!   never eyeballed. Light differs from canonical Okabe-Ito in ONE slot:
//!   #F0E442 (yellow · 1.29:1 on white, invisible as a thin mark) re-steps
//!   to #A08C00 within the same hue.
//! · Sequential: viridis 10-anchor LUT (`SciPy` 2015 talk · no paper) · sRGB
//!   lerp between anchors (Lab-space lerp = v1.5 refinement).
//! · Diverging: Moreland cool-warm 3-anchor (ISVC 2009) for `delta` — the
//!   midpoint must read as « nothing », so it is per-mode too (light gray on
//!   light · near-surface graphite on dark; a WHITE midpoint on a dark
//!   surface makes « no change » the loudest thing on the chart).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::many_single_char_names
)] // color ramp lerp (r/g/b/t idiom)
/// Okabe-Ito hues stepped for the LIGHT surface (#ffffff) — black reserved
/// for text · the gold slot last (weakest identity work).
pub const CATEGORICAL_LIGHT: [&str; 7] = [
    "#0072B2", // blue
    "#E69F00", // orange
    "#009E73", // bluish green
    "#D55E00", // vermilion
    "#CC79A7", // reddish purple
    "#56B4E9", // sky blue
    "#A08C00", // gold (Okabe-Ito yellow re-stepped into the lightness band)
];

/// The SAME seven hues stepped for the DARK surface (#0f1318) — every slot
/// ≥ 3:1 against it (strict, no relief needed in dark).
pub const CATEGORICAL_DARK: [&str; 7] = [
    "#1E88CF", // blue
    "#BC8100", // orange
    "#009E73", // bluish green
    "#D55E00", // vermilion
    "#C05E93", // reddish purple
    "#3E9BD6", // sky blue
    "#A39500", // gold
];

/// Categorical color for series index (cycles past 7 — the lint upstream
/// warns at >8 categories per CHT §3ter). LIGHT step — the single-document
/// SVG swaps to [`CATEGORICAL_DARK`] via its `<style>` classes; PNG and
/// Vega-Lite (single-mode surfaces) render this set.
#[must_use]
pub fn categorical(i: usize) -> &'static str {
    let idx = i % CATEGORICAL_LIGHT.len();
    CATEGORICAL_LIGHT.get(idx).copied().unwrap_or("#0072B2")
}

/// Dark-mode step for the same series index (parity with [`categorical`]).
#[must_use]
pub fn categorical_dark(i: usize) -> &'static str {
    let idx = i % CATEGORICAL_DARK.len();
    CATEGORICAL_DARK.get(idx).copied().unwrap_or("#1E88CF")
}

/// Viridis anchors (matplotlib canonical 10-stop).
const VIRIDIS: [(u8, u8, u8); 10] = [
    (0x44, 0x01, 0x54),
    (0x48, 0x28, 0x78),
    (0x3E, 0x49, 0x89),
    (0x31, 0x68, 0x8E),
    (0x26, 0x82, 0x8E),
    (0x1F, 0x9E, 0x89),
    (0x35, 0xB7, 0x79),
    (0x6D, 0xCD, 0x59),
    (0xB4, 0xDE, 0x2C),
    (0xFD, 0xE7, 0x25),
];

/// Moreland cool-warm anchors (RGB 59,76,192 · 221,221,221 · 180,4,38).
const COOLWARM: [(u8, u8, u8); 3] = [(59, 76, 192), (221, 221, 221), (180, 4, 38)];

/// Cool-warm re-anchored for the DARK surface: the arms lift toward
/// legibility on #0f1318 and the midpoint drops to near-surface graphite —
/// « no change » must recede, not glow.
const COOLWARM_DARK: [(u8, u8, u8); 3] = [(110, 139, 239), (58, 65, 73), (226, 91, 99)];

fn lerp_channel(a: u8, b: u8, t: f64) -> u8 {
    let v = (f64::from(a) + (f64::from(b) - f64::from(a)) * t).round();
    v.clamp(0.0, 255.0) as u8
}

fn ramp(anchors: &[(u8, u8, u8)], t: f64) -> String {
    let t = t.clamp(0.0, 1.0);
    let segments = anchors.len() - 1;
    let pos = t * segments as f64;
    let i = (pos.floor() as usize).min(segments - 1);
    let frac = pos - i as f64;
    let a = anchors.get(i).copied().unwrap_or((0, 0, 0));
    let b = anchors.get(i + 1).copied().unwrap_or(a);
    format!(
        "#{:02X}{:02X}{:02X}",
        lerp_channel(a.0, b.0, frac),
        lerp_channel(a.1, b.1, frac),
        lerp_channel(a.2, b.2, frac)
    )
}

/// Sequential ramp · t ∈ `[0,1]`.
#[must_use]
pub fn viridis(t: f64) -> String {
    ramp(&VIRIDIS, t)
}

/// Diverging ramp · t ∈ `[0,1]` with 0.5 = neutral midpoint.
#[must_use]
pub fn coolwarm(t: f64) -> String {
    ramp(&COOLWARM, t)
}

/// Dark-mode diverging ramp (near-surface midpoint · lifted arms).
#[must_use]
pub fn coolwarm_dark(t: f64) -> String {
    ramp(&COOLWARM_DARK, t)
}

/// Heatmap cells and their legend share ONE quantized scale — this many
/// discrete bins (the legend already swore « never a gradient »; the cells
/// keep the same promise, and the bin index doubles as the per-mode CSS
/// class so ONE document renders both themes).
pub const DIVERGING_BINS: usize = 8;

/// Quantize t ∈ `[0,1]` onto the shared diverging bins → bin index.
#[must_use]
pub fn diverging_bin(t: f64) -> usize {
    let t = t.clamp(0.0, 1.0);
    ((t * DIVERGING_BINS as f64) as usize).min(DIVERGING_BINS - 1)
}

/// The LIGHT fill for a diverging bin (bin midpoint sampled on the ramp).
#[must_use]
pub fn diverging_bin_light(bin: usize) -> String {
    coolwarm(bin_center(bin))
}

/// The DARK fill for a diverging bin.
#[must_use]
pub fn diverging_bin_dark(bin: usize) -> String {
    coolwarm_dark(bin_center(bin))
}

fn bin_center(bin: usize) -> f64 {
    (bin.min(DIVERGING_BINS - 1) as f64 + 0.5) / DIVERGING_BINS as f64
}

/// Chrome constants (one place · one voice).
pub const INK: &str = "#1a1a1a";
pub const INK_SOFT: &str = "#555555";
pub const GRID: &str = "#e6e6e6";
pub const AXIS: &str = "#9a9a9a";
pub const BG: &str = "#ffffff";

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
    fn viridis_endpoints() {
        assert_eq!(viridis(0.0), "#440154");
        assert_eq!(viridis(1.0), "#FDE725");
    }

    #[test]
    fn coolwarm_midpoint_neutral() {
        assert_eq!(coolwarm(0.5), "#DDDDDD");
    }

    #[test]
    fn categorical_cycles() {
        assert_eq!(categorical(0), "#0072B2");
        assert_eq!(categorical(7), "#0072B2");
    }

    #[test]
    fn ramp_is_deterministic() {
        assert_eq!(viridis(0.37), viridis(0.37));
    }
}
