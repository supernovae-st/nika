//! DAG Edge Widget
//!
//! Renders edges between DAG nodes with binding labels and data previews.
//! Supports active/inactive states with animated flow visualization.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
};

// ===============================================================================
// CONSTANTS
// ===============================================================================

/// Active edge color (Amber)
const ACTIVE_COLOR: Color = Color::Rgb(245, 158, 11);

/// Inactive edge color
const INACTIVE_COLOR: Color = Color::DarkGray;

/// Preview text color (muted gray)
const PREVIEW_COLOR: Color = Color::Rgb(107, 114, 128);

/// Binding label color (violet)
const BINDING_COLOR: Color = Color::Rgb(139, 92, 246);

// ===============================================================================
// DAG EDGE
// ===============================================================================

/// Edge between two DAG nodes
#[derive(Debug, Clone)]
pub struct DagEdge {
    /// Source node position (x, y of bottom center)
    pub from: (u16, u16),
    /// Target node position (x, y of top center)
    pub to: (u16, u16),
    /// Binding label (e.g., "{{use.data}}")
    pub binding: Option<String>,
    /// Data preview (shown in grey)
    pub preview: Option<String>,
    /// Is this edge active (data flowing)
    pub active: bool,
}

impl DagEdge {
    /// Create a new edge between two positions
    pub fn new(from: (u16, u16), to: (u16, u16)) -> Self {
        Self {
            from,
            to,
            binding: None,
            preview: None,
            active: false,
        }
    }

    /// Add a binding label to the edge
    pub fn with_binding(mut self, binding: impl Into<String>) -> Self {
        self.binding = Some(binding.into());
        self
    }

    /// Add a data preview to the edge
    pub fn with_preview(mut self, preview: impl Into<String>) -> Self {
        self.preview = Some(preview.into());
        self
    }

    /// Set the active state of the edge
    pub fn with_active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Render the edge to a buffer
    pub fn render(&self, buf: &mut Buffer, area: Rect) {
        let edge_color = if self.active {
            ACTIVE_COLOR
        } else {
            INACTIVE_COLOR
        };
        let edge_style = Style::default().fg(edge_color);

        // Calculate edge positions relative to area
        let from_x = self.from.0;
        let from_y = self.from.1;
        let to_x = self.to.0;
        let to_y = self.to.1;

        // Check bounds
        if !self.is_in_bounds(from_x, from_y, &area) && !self.is_in_bounds(to_x, to_y, &area) {
            return;
        }

        // Determine edge direction
        if from_x == to_x {
            // Pure vertical edge
            self.render_vertical_edge(buf, area, from_x, from_y, to_y, edge_style);
        } else if from_y == to_y {
            // Pure horizontal edge
            self.render_horizontal_edge(buf, area, from_x, to_x, from_y, edge_style);
        } else {
            // L-shaped edge (vertical then horizontal, or horizontal then vertical)
            self.render_l_edge(buf, area, edge_style);
        }
    }

    /// Check if a point is within the area bounds
    fn is_in_bounds(&self, x: u16, y: u16, area: &Rect) -> bool {
        x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height
    }

    /// Render a vertical edge segment
    fn render_vertical_edge(
        &self,
        buf: &mut Buffer,
        area: Rect,
        x: u16,
        from_y: u16,
        to_y: u16,
        style: Style,
    ) {
        let (start_y, end_y) = if from_y < to_y {
            (from_y, to_y)
        } else {
            (to_y, from_y)
        };

        let line_char = if self.active { "┃" } else { "│" };

        // Calculate midpoint for label placement
        let mid_y = start_y + (end_y - start_y) / 2;

        // Draw vertical line
        for y in start_y..=end_y {
            if self.is_in_bounds(x, y, &area) {
                // Skip the midpoint area if we have labels
                let has_label = self.binding.is_some() || self.preview.is_some();
                let is_label_area = has_label && (y >= mid_y.saturating_sub(1) && y <= mid_y + 1);

                if !is_label_area {
                    buf.set_string(x, y, line_char, style);
                }
            }
        }

        // Draw arrow at the bottom
        if self.is_in_bounds(x, end_y, &area) {
            buf.set_string(x, end_y, "▼", style);
        }

        // Render binding label at midpoint
        self.render_labels(buf, area, x, mid_y);
    }

    /// Render a horizontal edge segment
    fn render_horizontal_edge(
        &self,
        buf: &mut Buffer,
        area: Rect,
        from_x: u16,
        to_x: u16,
        y: u16,
        style: Style,
    ) {
        let (start_x, end_x) = if from_x < to_x {
            (from_x, to_x)
        } else {
            (to_x, from_x)
        };

        let line_char = if self.active { "━" } else { "─" };

        for x in start_x..=end_x {
            if self.is_in_bounds(x, y, &area) {
                buf.set_string(x, y, line_char, style);
            }
        }
    }

    /// Render an L-shaped edge
    fn render_l_edge(&self, buf: &mut Buffer, area: Rect, style: Style) {
        let from_x = self.from.0;
        let from_y = self.from.1;
        let to_x = self.to.0;
        let to_y = self.to.1;

        let line_v = if self.active { "┃" } else { "│" };
        let line_h = if self.active { "━" } else { "─" };

        // Determine edge direction
        let going_down = to_y > from_y;
        let going_right = to_x > from_x;

        // Calculate corner position (go vertical first, then horizontal)
        let corner_y = to_y;
        let corner_x = from_x;

        // Draw vertical segment (from source down to corner)
        let (v_start, v_end) = if going_down {
            (from_y, corner_y)
        } else {
            (corner_y, from_y)
        };

        // Calculate midpoint for label placement on vertical segment
        let mid_y = v_start + (v_end - v_start) / 2;

        for y in v_start..v_end {
            if self.is_in_bounds(corner_x, y, &area) {
                // Skip label area
                let has_label = self.binding.is_some() || self.preview.is_some();
                let is_label_area = has_label && (y >= mid_y.saturating_sub(1) && y <= mid_y + 1);

                if !is_label_area {
                    buf.set_string(corner_x, y, line_v, style);
                }
            }
        }

        // Draw corner
        let corner_char = match (going_down, going_right) {
            (true, true) => "└",   // Down then right
            (true, false) => "┘",  // Down then left
            (false, true) => "┌",  // Up then right
            (false, false) => "┐", // Up then left
        };

        if self.is_in_bounds(corner_x, corner_y, &area) {
            buf.set_string(corner_x, corner_y, corner_char, style);
        }

        // Draw horizontal segment
        let (h_start, h_end) = if going_right {
            (corner_x + 1, to_x)
        } else {
            (to_x, corner_x.saturating_sub(1))
        };

        for x in h_start..=h_end {
            if self.is_in_bounds(x, corner_y, &area) {
                if x == to_x {
                    // Arrow at the end
                    let arrow = if going_right { "▶" } else { "◀" };
                    buf.set_string(x, corner_y, arrow, style);
                } else {
                    buf.set_string(x, corner_y, line_h, style);
                }
            }
        }

        // Render binding label at midpoint of vertical segment
        self.render_labels(buf, area, corner_x, mid_y);
    }

    /// Render binding and preview labels at a position
    fn render_labels(&self, buf: &mut Buffer, area: Rect, x: u16, y: u16) {
        // Render binding label
        if let Some(binding) = &self.binding {
            let label = binding.as_str();
            let label_width = label.len() as u16;

            // Position label to the right of the edge
            let label_x = x.saturating_add(2);
            let label_y = y;

            if self.is_in_bounds(label_x, label_y, &area) {
                // Truncate if needed
                let available_width = area.x + area.width - label_x;
                let display_label = if label_width > available_width {
                    let truncate_at = available_width.saturating_sub(3) as usize;
                    if truncate_at > 0 {
                        format!("{}...", &label[..truncate_at.min(label.len())])
                    } else {
                        String::new()
                    }
                } else {
                    label.to_string()
                };

                if !display_label.is_empty() {
                    buf.set_string(
                        label_x,
                        label_y,
                        &display_label,
                        Style::default().fg(BINDING_COLOR),
                    );
                }
            }
        }

        // Render preview below binding
        if let Some(preview) = &self.preview {
            let preview_y = y.saturating_add(1);
            let preview_x = x.saturating_add(2);

            if self.is_in_bounds(preview_x, preview_y, &area) {
                // Format preview with decorators
                let formatted = format!("{} {}", char::from_u32(0x2591).unwrap_or(' '), preview);
                let available_width = (area.x + area.width).saturating_sub(preview_x) as usize;

                let display_preview = if formatted.len() > available_width {
                    let truncate_at = available_width.saturating_sub(4);
                    if truncate_at > 0 {
                        format!("{}...", &formatted[..truncate_at.min(formatted.len())])
                    } else {
                        String::new()
                    }
                } else {
                    formatted
                };

                if !display_preview.is_empty() {
                    buf.set_string(
                        preview_x,
                        preview_y,
                        &display_preview,
                        Style::default().fg(PREVIEW_COLOR),
                    );
                }
            }
        }
    }
}

// ===============================================================================
// MERGE POINT RENDERING
// ===============================================================================

/// Render a merge point where multiple edges converge to a single target
///
/// Draws horizontal lines from multiple sources converging to a single target:
/// - `└` or `┘` corners for edge connections
/// - `─` horizontal lines
/// - `┬` merge point
pub fn render_merge(
    sources: &[(u16, u16)],
    target: (u16, u16),
    buf: &mut Buffer,
    area: Rect,
    active: bool,
) {
    if sources.is_empty() {
        return;
    }

    let edge_color = if active { ACTIVE_COLOR } else { INACTIVE_COLOR };
    let style = Style::default().fg(edge_color);

    let line_h = if active { "━" } else { "─" };
    let line_v = if active { "┃" } else { "│" };

    let target_x = target.0;
    let target_y = target.1;

    // Sort sources by x position
    let mut sorted_sources = sources.to_vec();
    sorted_sources.sort_by_key(|(x, _)| *x);

    // Find the merge line y position (one row above target)
    let merge_y = target_y.saturating_sub(1);

    // Draw vertical line from merge point to target
    if merge_y < target_y {
        for y in merge_y..target_y {
            if is_in_bounds(target_x, y, &area) {
                buf.set_string(target_x, y, line_v, style);
            }
        }
    }

    // Draw arrow at target
    if is_in_bounds(target_x, target_y, &area) {
        buf.set_string(target_x, target_y, "▼", style);
    }

    // Draw merge point
    if is_in_bounds(target_x, merge_y, &area) {
        let merge_char = if sources.len() > 1 { "┬" } else { "│" };
        buf.set_string(target_x, merge_y, merge_char, style);
    }

    // Draw horizontal lines and corners for each source
    for (src_x, src_y) in sorted_sources.iter() {
        let src_x = *src_x;
        let src_y = *src_y;

        if src_x == target_x {
            // Source directly above - just draw vertical line
            for y in src_y..merge_y {
                if is_in_bounds(src_x, y, &area) {
                    buf.set_string(src_x, y, line_v, style);
                }
            }
        } else {
            // Draw vertical line from source to merge_y
            for y in src_y..merge_y {
                if is_in_bounds(src_x, y, &area) {
                    buf.set_string(src_x, y, line_v, style);
                }
            }

            // Draw corner at merge_y
            let corner = if src_x < target_x { "└" } else { "┘" };
            if is_in_bounds(src_x, merge_y, &area) {
                buf.set_string(src_x, merge_y, corner, style);
            }

            // Draw horizontal line to merge point
            let (h_start, h_end) = if src_x < target_x {
                (src_x + 1, target_x)
            } else {
                (target_x + 1, src_x)
            };

            for x in h_start..h_end {
                if is_in_bounds(x, merge_y, &area) {
                    buf.set_string(x, merge_y, line_h, style);
                }
            }
        }
    }
}

/// Helper function to check if a point is within bounds
fn is_in_bounds(x: u16, y: u16, area: &Rect) -> bool {
    x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height
}

// ===============================================================================
// TESTS
// ===============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edge_creation() {
        let edge = DagEdge::new((10, 5), (10, 15));

        assert_eq!(edge.from, (10, 5));
        assert_eq!(edge.to, (10, 15));
        assert_eq!(edge.binding, None);
        assert_eq!(edge.preview, None);
        assert!(!edge.active);
    }

    #[test]
    fn test_edge_with_binding() {
        let edge = DagEdge::new((5, 0), (5, 10)).with_binding("{{use.data}}");

        assert_eq!(edge.binding, Some("{{use.data}}".to_string()));
    }

    #[test]
    fn test_edge_with_preview() {
        let edge = DagEdge::new((5, 0), (5, 10)).with_preview("some data...");

        assert_eq!(edge.preview, Some("some data...".to_string()));
    }

    #[test]
    fn test_edge_active_state() {
        let inactive_edge = DagEdge::new((5, 0), (5, 10));
        assert!(!inactive_edge.active);

        let active_edge = DagEdge::new((5, 0), (5, 10)).with_active(true);
        assert!(active_edge.active);
    }

    #[test]
    fn test_edge_builder_chain() {
        let edge = DagEdge::new((0, 0), (10, 10))
            .with_binding("{{use.result}}")
            .with_preview("preview text")
            .with_active(true);

        assert_eq!(edge.from, (0, 0));
        assert_eq!(edge.to, (10, 10));
        assert_eq!(edge.binding, Some("{{use.result}}".to_string()));
        assert_eq!(edge.preview, Some("preview text".to_string()));
        assert!(edge.active);
    }

    #[test]
    fn test_edge_render_vertical_does_not_panic() {
        let edge = DagEdge::new((5, 2), (5, 8));

        let mut buffer = Buffer::empty(Rect::new(0, 0, 20, 15));
        edge.render(&mut buffer, Rect::new(0, 0, 20, 15));

        // Should have rendered the arrow
        let cell = buffer.cell((5, 8)).unwrap();
        assert_eq!(cell.symbol(), "▼");
    }

    #[test]
    fn test_edge_render_active_uses_amber_color() {
        let edge = DagEdge::new((5, 2), (5, 8)).with_active(true);

        let mut buffer = Buffer::empty(Rect::new(0, 0, 20, 15));
        edge.render(&mut buffer, Rect::new(0, 0, 20, 15));

        // Active edge uses bold line character
        let cell = buffer.cell((5, 3)).unwrap();
        assert_eq!(cell.symbol(), "┃");
    }

    #[test]
    fn test_edge_render_inactive_uses_thin_line() {
        let edge = DagEdge::new((5, 2), (5, 8));

        let mut buffer = Buffer::empty(Rect::new(0, 0, 20, 15));
        edge.render(&mut buffer, Rect::new(0, 0, 20, 15));

        // Inactive edge uses thin line character
        let cell = buffer.cell((5, 3)).unwrap();
        assert_eq!(cell.symbol(), "│");
    }

    #[test]
    fn test_edge_render_with_binding_label() {
        let edge = DagEdge::new((5, 2), (5, 10)).with_binding("{{use.ctx}}");

        let mut buffer = Buffer::empty(Rect::new(0, 0, 30, 15));
        edge.render(&mut buffer, Rect::new(0, 0, 30, 15));

        // Binding should be rendered to the right of the edge
        // Midpoint is at y=6, label starts at x=7
        let cell = buffer.cell((7, 6)).unwrap();
        assert_eq!(cell.symbol(), "{");
    }

    #[test]
    fn test_merge_render_does_not_panic() {
        let sources = vec![(5, 2), (10, 2), (15, 2)];
        let target = (10, 10);

        let mut buffer = Buffer::empty(Rect::new(0, 0, 25, 15));
        render_merge(
            &sources,
            target,
            &mut buffer,
            Rect::new(0, 0, 25, 15),
            false,
        );

        // Should have rendered the merge point
        let cell = buffer.cell((10, 9)).unwrap();
        assert_eq!(cell.symbol(), "┬");
    }

    #[test]
    fn test_merge_single_source() {
        let sources = vec![(10, 2)];
        let target = (10, 10);

        let mut buffer = Buffer::empty(Rect::new(0, 0, 25, 15));
        render_merge(
            &sources,
            target,
            &mut buffer,
            Rect::new(0, 0, 25, 15),
            false,
        );

        // Single source should use vertical line, not merge point
        let cell = buffer.cell((10, 9)).unwrap();
        assert_eq!(cell.symbol(), "│");
    }

    #[test]
    fn test_merge_empty_sources() {
        let sources: Vec<(u16, u16)> = vec![];
        let target = (10, 10);

        let mut buffer = Buffer::empty(Rect::new(0, 0, 25, 15));
        render_merge(
            &sources,
            target,
            &mut buffer,
            Rect::new(0, 0, 25, 15),
            false,
        );

        // Nothing should be rendered
        let cell = buffer.cell((10, 10)).unwrap();
        assert_eq!(cell.symbol(), " ");
    }

    #[test]
    fn test_merge_active_state() {
        let sources = vec![(5, 2), (15, 2)];
        let target = (10, 10);

        let mut buffer = Buffer::empty(Rect::new(0, 0, 25, 15));
        render_merge(&sources, target, &mut buffer, Rect::new(0, 0, 25, 15), true);

        // Active merge uses bold characters
        let cell = buffer.cell((10, 9)).unwrap();
        assert_eq!(cell.symbol(), "┬");

        // Check horizontal line uses bold character
        let h_cell = buffer.cell((8, 9)).unwrap();
        assert_eq!(h_cell.symbol(), "━");
    }

    #[test]
    fn test_is_in_bounds() {
        let area = Rect::new(5, 5, 10, 10);

        // Inside
        assert!(is_in_bounds(5, 5, &area));
        assert!(is_in_bounds(10, 10, &area));
        assert!(is_in_bounds(14, 14, &area));

        // Outside
        assert!(!is_in_bounds(4, 5, &area));
        assert!(!is_in_bounds(5, 4, &area));
        assert!(!is_in_bounds(15, 5, &area));
        assert!(!is_in_bounds(5, 15, &area));
    }

    #[test]
    fn test_edge_l_shaped_down_right() {
        let edge = DagEdge::new((5, 2), (15, 8));

        let mut buffer = Buffer::empty(Rect::new(0, 0, 25, 15));
        edge.render(&mut buffer, Rect::new(0, 0, 25, 15));

        // Corner should be at (5, 8) - going down then right
        let cell = buffer.cell((5, 8)).unwrap();
        assert_eq!(cell.symbol(), "└");
    }

    #[test]
    fn test_edge_l_shaped_down_left() {
        let edge = DagEdge::new((15, 2), (5, 8));

        let mut buffer = Buffer::empty(Rect::new(0, 0, 25, 15));
        edge.render(&mut buffer, Rect::new(0, 0, 25, 15));

        // Corner should be at (15, 8) - going down then left
        let cell = buffer.cell((15, 8)).unwrap();
        assert_eq!(cell.symbol(), "┘");
    }

    #[test]
    fn test_edge_out_of_bounds_does_not_panic() {
        let edge = DagEdge::new((100, 100), (200, 200));

        let mut buffer = Buffer::empty(Rect::new(0, 0, 20, 15));
        edge.render(&mut buffer, Rect::new(0, 0, 20, 15));

        // Should not panic, just skip rendering
        let cell = buffer.cell((0, 0)).unwrap();
        assert_eq!(cell.symbol(), " ");
    }
}
