//! Embedded palettes (zero-dep · operator lock « pas de dépendances »).
//!
//! · Categorical: Okabe-Ito CVD-safe set (jfly.uni-koeln.de/color · web
//!   resource, no DOI exists) — series order starts at blue.
//! · Sequential: viridis 10-anchor LUT (`SciPy` 2015 talk · no paper) · sRGB
//!   lerp between anchors (Lab-space lerp = v1.5 refinement).
//! · Diverging: Moreland cool-warm 3-anchor (ISVC 2009) for `delta` (midpoint 0).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::many_single_char_names
)] // color ramp lerp (r/g/b/t idiom)
/// Okabe-Ito series colors (black reserved for text · yellow last — weak on white).
const OKABE_ITO: [&str; 7] = [
    "#0072B2", // blue
    "#E69F00", // orange
    "#009E73", // bluish green
    "#D55E00", // vermilion
    "#CC79A7", // reddish purple
    "#56B4E9", // sky blue
    "#F0E442", // yellow
];

/// Categorical color for series index (cycles past 7 — the lint upstream
/// warns at >8 categories per CHT §3ter).
#[must_use]
pub fn categorical(i: usize) -> &'static str {
    let idx = i % OKABE_ITO.len();
    OKABE_ITO.get(idx).copied().unwrap_or("#0072B2")
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
