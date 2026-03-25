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

/// Render twinkling stars in empty cells of the given area.
///
/// Scans the buffer after views have rendered and fills cells whose symbol
/// is `" "` (untouched by widgets) with star characters. This creates a
/// background star field that shows through gaps in the UI.
///
/// # Performance
///
/// Direct buffer writes — zero widget allocation. Skips cells that already
/// have content, so cost is proportional to empty space only.
pub fn render_star_field(frame: &mut Frame, area: Rect, tick: u16) {
    let buf = frame.buffer_mut();
    for py in area.y..area.y + area.height {
        for px in area.x..area.x + area.width {
            let pos = Position::new(px, py);
            if let Some(cell) = buf.cell_mut(pos) {
                // Only fill truly empty cells (space character = no widget content)
                if cell.symbol() == " " {
                    if let Some((star_char, star_color)) = galaxy_star(px, py, tick) {
                        cell.set_symbol(star_char);
                        cell.set_fg(star_color);
                    }
                }
            }
        }
    }
}

/// Get a stable tick value for star animations.
///
/// Derives from system time so stars twinkle consistently regardless of
/// frame rate. Increments every 100ms (~10fps star animation).
pub fn star_tick() -> u16 {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    (millis / 100) as u16
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
    fn star_tick_increments() {
        let t1 = star_tick();
        // star_tick is time-based, just verify it returns something reasonable
        assert!(t1 > 0, "star_tick should be positive");
    }
}
