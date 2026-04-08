// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Galaxy Star Field
//!
//! Twinkling star particles for empty TUI backgrounds.
//! Ported from NovaNet's animation system — same visual identity across tools.
//!
//! ## Design
//!
//! - Pure function: `galaxy_star(x, y, tick)` → `Option<(&str, Color)>`
//! - ~5% density with FNV-1a hash (no diagonal artifacts)
//! - 7-phase twinkle cycle (~3.3s per star at 10fps)
//! - Grayscale with slight blue tint for cosmic feel
//!
//! ## Usage
//!
//! Call `render_star_field()` after views render to fill empty cells:
//! ```rust,ignore
//! render_star_field(frame, content_area, tick);
//! ```

use ratatui::layout::{Position, Rect};
use ratatui::style::Color;
use ratatui::Frame;

// ═══════════════════════════════════════════════════════════════════════════════
// GALAXY STAR FIELD
// ═══════════════════════════════════════════════════════════════════════════════

/// Sparse twinkling star field for empty panel backgrounds.
///
/// Uses bit-mixed hash to avoid visible diagonal patterns.
/// Returns `Some((char, color))` for ~5% of positions.
///
/// Ported from NovaNet `tui::animations::galaxy_star`.
pub fn galaxy_star(x: u16, y: u16, tick: u16) -> Option<(&'static str, Color)> {
    // FNV-1a inspired hash — eliminates diagonal pattern artifacts
    let mut h = 2166136261u32;
    h ^= x as u32;
    h = h.wrapping_mul(16777619);
    h ^= y as u32;
    h = h.wrapping_mul(16777619);
    let hash = (h >> 16) ^ (h & 0xFFFF);

    // ~5% density
    if hash % 20 != 0 {
        return None;
    }

    // Twinkle: star visibility cycles based on position + tick
    let twinkle_phase = ((hash as u16).wrapping_add(tick / 3)) % 10;
    if twinkle_phase > 6 {
        return None; // Off 30% of time
    }

    // Brightness ramp: dim → bright → dim
    let brightness = match twinkle_phase {
        0 | 6 => 30,
        1 | 5 => 45,
        2 | 4 => 65,
        3 => 85,
        _ => 40,
    };

    let star = match (hash >> 2) % 4 {
        0 => "·",
        1 => "∙",
        2 => "⋅",
        _ => "˙",
    };

    // Blue tint for cosmic/hacking feel
    Some((star, Color::Rgb(brightness, brightness, brightness + 20)))
}

// ═══════════════════════════════════════════════════════════════════════════════
// STAR FIELD (Pre-computed positions for O(stars) rendering)
// ═══════════════════════════════════════════════════════════════════════════════

/// Pre-computed star field for O(stars) rendering instead of O(width*height).
///
/// Stars are deterministic for a given (x, y) position — only the twinkle
/// animation depends on tick. This struct pre-computes which cells ARE stars
/// (~5% density) on resize, then iterates only those positions each frame.
///
/// Typical savings: 10,000 iterations → ~500 iterations per frame (20x).
#[derive(Debug, Clone)]
pub struct StarField {
    /// Cached terminal content area dimensions. Rebuild on change.
    width: u16,
    height: u16,
    /// Pre-computed star positions: (x, y, hash).
    /// hash is the FNV-1a output used for twinkle phase, brightness, glyph.
    stars: Vec<(u16, u16, u32)>,
}

impl Default for StarField {
    fn default() -> Self {
        Self::new()
    }
}

impl StarField {
    /// Create an empty star field. Call `ensure_size()` before first render.
    pub fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            stars: Vec::new(),
        }
    }

    /// Rebuild star positions if terminal size changed. No-op if same size.
    pub fn ensure_size(&mut self, width: u16, height: u16) {
        if self.width == width && self.height == height {
            return;
        }
        self.width = width;
        self.height = height;

        // Pre-allocate: ~5% density
        let estimated = (width as usize * height as usize) / 20;
        self.stars = Vec::with_capacity(estimated);

        for y in 0..height {
            for x in 0..width {
                let mut h = 2166136261u32;
                h ^= x as u32;
                h = h.wrapping_mul(16777619);
                h ^= y as u32;
                h = h.wrapping_mul(16777619);
                let hash = (h >> 16) ^ (h & 0xFFFF);

                if hash % 20 == 0 {
                    self.stars.push((x, y, hash));
                }
            }
        }
    }

    /// Render twinkling stars in empty cells. Only iterates pre-computed
    /// star positions (~500) instead of all cells (~10,000). 20x faster.
    pub fn render(&self, frame: &mut Frame, area: Rect, tick: u16) {
        let buf = frame.buffer_mut();
        for &(x, y, hash) in &self.stars {
            let px = area.x + x;
            let py = area.y + y;

            // Bounds check (area may be smaller than pre-computed size)
            if px >= area.x + area.width || py >= area.y + area.height {
                continue;
            }

            let pos = Position::new(px, py);
            if let Some(cell) = buf.cell_mut(pos) {
                if cell.symbol() != " " {
                    continue; // Cell has widget content
                }

                // Twinkle phase (tick-dependent)
                let twinkle_phase = ((hash as u16).wrapping_add(tick / 3)) % 10;
                if twinkle_phase > 6 {
                    continue; // Star is off this tick
                }

                let brightness = match twinkle_phase {
                    0 | 6 => 30,
                    1 | 5 => 45,
                    2 | 4 => 65,
                    3 => 85,
                    _ => 40,
                };

                let star = match (hash >> 2) % 4 {
                    0 => "·",
                    1 => "∙",
                    2 => "⋅",
                    _ => "˙",
                };

                cell.set_symbol(star);
                cell.set_fg(Color::Rgb(brightness, brightness, brightness + 20));
            }
        }
    }
}

/// Get a stable tick value for star animations.
///
/// Derives from monotonic clock so stars twinkle consistently regardless of
/// frame rate. Increments every 100ms (~10fps star animation).
/// Uses `Instant` instead of `SystemTime` to avoid syscall overhead.
pub fn star_tick() -> u16 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let start = START.get_or_init(std::time::Instant::now);
    (start.elapsed().as_millis() / 100) as u16
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn galaxy_star_sparse_density() {
        let count: usize = (0..100)
            .flat_map(|x| (0..100).map(move |y| galaxy_star(x, y, 0)))
            .filter(|s| s.is_some())
            .count();
        // ~5% of 10000 = ~500, allow wide margin
        assert!(count < 1000, "too many stars: {count}");
        assert!(count > 50, "too few stars: {count}");
    }

    #[test]
    fn galaxy_star_no_diagonal_patterns() {
        let mut row_counts = [0u32; 10];
        for y in 0..10u16 {
            for x in 0..100u16 {
                if galaxy_star(x, y, 0).is_some() {
                    row_counts[y as usize] += 1;
                }
            }
        }
        let avg = row_counts.iter().sum::<u32>() as f32 / 10.0;
        for (y, &count) in row_counts.iter().enumerate() {
            let ratio = count as f32 / avg.max(1.0);
            assert!(
                (0.3..=3.0).contains(&ratio),
                "row {y} has {count} stars vs avg {avg:.1} — pattern detected"
            );
        }
    }

    #[test]
    fn galaxy_star_twinkles_over_time() {
        // Count visible stars across a range of ticks — count should vary
        let counts: Vec<usize> = (0..30)
            .map(|tick| {
                (0..100u16)
                    .flat_map(|x| (0..100u16).map(move |y| galaxy_star(x, y, tick * 3)))
                    .filter(|s| s.is_some())
                    .count()
            })
            .collect();
        // Not all tick counts should be identical (stars twinkle on/off)
        let all_same = counts.windows(2).all(|w| w[0] == w[1]);
        assert!(!all_same, "star count should vary across ticks: {counts:?}");
    }

    #[test]
    fn galaxy_star_blue_tint() {
        // All star colors should have blue > red/green
        for x in 0..100u16 {
            for y in 0..100u16 {
                if let Some((_, Color::Rgb(r, _g, b))) = galaxy_star(x, y, 0) {
                    assert!(
                        b > r,
                        "star at ({x},{y}) should have blue tint: r={r} b={b}"
                    );
                }
            }
        }
    }

    #[test]
    fn star_tick_returns_u16() {
        // star_tick uses monotonic Instant, first call may be 0
        let t1 = star_tick();
        let t2 = star_tick();
        // Both should be valid u16 values (no panic)
        assert!(t2 >= t1, "star_tick should be monotonic");
    }

    // ═══ StarField tests ═══

    #[test]
    fn star_field_empty_on_new() {
        let sf = StarField::new();
        assert_eq!(sf.stars.len(), 0);
        assert_eq!(sf.width, 0);
    }

    #[test]
    fn star_field_populates_on_ensure_size() {
        let mut sf = StarField::new();
        sf.ensure_size(200, 50);
        assert_eq!(sf.width, 200);
        assert_eq!(sf.height, 50);
        assert!(sf.stars.len() > 200, "too few stars: {}", sf.stars.len());
        assert!(sf.stars.len() < 1000, "too many stars: {}", sf.stars.len());
    }

    #[test]
    fn star_field_no_rebuild_same_size() {
        let mut sf = StarField::new();
        sf.ensure_size(100, 40);
        let count = sf.stars.len();
        let ptr = sf.stars.as_ptr();
        sf.ensure_size(100, 40); // Same size — no-op
        assert_eq!(sf.stars.len(), count);
        assert_eq!(sf.stars.as_ptr(), ptr);
    }

    #[test]
    fn star_field_rebuilds_on_resize() {
        let mut sf = StarField::new();
        sf.ensure_size(100, 40);
        let count_small = sf.stars.len();
        sf.ensure_size(200, 50);
        let count_large = sf.stars.len();
        assert!(count_large > count_small);
    }

    #[test]
    fn star_field_all_positions_in_bounds() {
        let mut sf = StarField::new();
        sf.ensure_size(80, 24);
        for &(x, y, _) in &sf.stars {
            assert!(x < 80, "x={x} out of bounds");
            assert!(y < 24, "y={y} out of bounds");
        }
    }

    #[test]
    fn star_field_zero_size() {
        let mut sf = StarField::new();
        sf.ensure_size(0, 0);
        assert!(sf.stars.is_empty());
    }
}
