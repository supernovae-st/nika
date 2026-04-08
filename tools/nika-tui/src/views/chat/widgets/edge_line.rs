// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Chat Edge Line Widget
//!
//! Renders connections between ChatNodeBox widgets in the Chat DAG.
//! Supports flow animation and binding labels.
//!
//! Chat-as-DAG architecture

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

use crate::tokens::compat;

// ═══════════════════════════════════════════════════════════════════════════
// POSITION
// ═══════════════════════════════════════════════════════════════════════════

/// Position in the terminal grid
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChatPosition {
    pub x: u16,
    pub y: u16,
}

impl ChatPosition {
    /// Create a new position
    pub fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// EDGE LINE
// ═══════════════════════════════════════════════════════════════════════════

/// A line connecting two nodes in the Chat DAG
#[derive(Debug, Clone)]
pub struct ChatEdgeLine {
    /// Source position
    from: ChatPosition,
    /// Target position
    to: ChatPosition,
    /// Binding label (e.g., "with.ctx")
    label: Option<String>,
    /// Whether data is actively flowing
    active: bool,
    /// Animation tick for flow effect
    animation_tick: u8,
    /// Edge color
    color: Color,
}

impl ChatEdgeLine {
    /// Create a new edge line between two positions
    pub fn new(from: ChatPosition, to: ChatPosition) -> Self {
        Self {
            from,
            to,
            label: None,
            active: false,
            animation_tick: 0,
            color: compat::SLATE_600,
        }
    }

    /// Set the binding label
    pub fn with_label(mut self, label: &str) -> Self {
        self.label = Some(label.to_string());
        self
    }

    /// Set whether data is actively flowing
    pub fn with_active(mut self, active: bool) -> Self {
        self.active = active;
        if active {
            self.color = compat::CYAN_500;
        } else {
            self.color = compat::SLATE_600;
        }
        self
    }

    /// Check if edge is vertical
    pub fn is_vertical(&self) -> bool {
        self.from.x == self.to.x
    }

    /// Check if edge is horizontal
    pub fn is_horizontal(&self) -> bool {
        self.from.y == self.to.y
    }

    /// Get the length of the edge
    pub fn length(&self) -> u16 {
        if self.is_vertical() {
            self.to.y.abs_diff(self.from.y)
        } else if self.is_horizontal() {
            self.to.x.abs_diff(self.from.x)
        } else {
            // Diagonal - use manhattan distance
            self.to.y.abs_diff(self.from.y) + self.to.x.abs_diff(self.from.x)
        }
    }

    /// Get current position of flow animation dot
    pub fn flow_position(&self) -> Option<ChatPosition> {
        if !self.active {
            return None;
        }

        let len = self.length();
        if len == 0 {
            return None;
        }

        // Cycle through edge length (ping-pong effect)
        let cycle_len = len.saturating_mul(2);
        if cycle_len == 0 {
            return None;
        }

        let offset = self.animation_tick as u16 % cycle_len;
        let offset = if offset > len {
            cycle_len - offset
        } else {
            offset
        };

        if self.is_vertical() {
            let y = if self.from.y < self.to.y {
                self.from.y.saturating_add(offset)
            } else {
                self.from.y.saturating_sub(offset)
            };
            Some(ChatPosition::new(self.from.x, y))
        } else if self.is_horizontal() {
            let x = if self.from.x < self.to.x {
                self.from.x.saturating_add(offset)
            } else {
                self.from.x.saturating_sub(offset)
            };
            Some(ChatPosition::new(x, self.from.y))
        } else {
            // Diagonal - just show at midpoint
            let mid_x = self.from.x / 2 + self.to.x / 2;
            let mid_y = self.from.y / 2 + self.to.y / 2;
            Some(ChatPosition::new(mid_x, mid_y))
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// WIDGET IMPLEMENTATION
// ═══════════════════════════════════════════════════════════════════════════

impl Widget for ChatEdgeLine {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let style = Style::default().fg(self.color);

        // Check if positions are within area bounds
        let in_bounds = |p: ChatPosition| {
            p.x >= area.x
                && p.x < area.x + area.width
                && p.y >= area.y
                && p.y < area.y + area.height
        };

        if self.is_vertical() {
            let x = self.from.x;
            let (start_y, end_y) = if self.from.y < self.to.y {
                (self.from.y, self.to.y)
            } else {
                (self.to.y, self.from.y)
            };

            // Draw vertical line
            for y in start_y..end_y {
                if in_bounds(ChatPosition::new(x, y)) {
                    buf[(x, y)].set_symbol("│").set_style(style);
                }
            }

            // Draw arrow at end
            let arrow_pos = ChatPosition::new(x, end_y);
            if in_bounds(arrow_pos) {
                let arrow = if self.from.y < self.to.y {
                    "▼"
                } else {
                    "▲"
                };
                buf[(x, end_y)].set_symbol(arrow).set_style(style);
            }
        } else if self.is_horizontal() {
            let y = self.from.y;
            let (start_x, end_x) = if self.from.x < self.to.x {
                (self.from.x, self.to.x)
            } else {
                (self.to.x, self.from.x)
            };

            // Draw horizontal line
            for x in start_x..end_x {
                if in_bounds(ChatPosition::new(x, y)) {
                    buf[(x, y)].set_symbol("─").set_style(style);
                }
            }

            // Draw arrow at end
            let arrow_pos = ChatPosition::new(end_x, y);
            if in_bounds(arrow_pos) {
                let arrow = if self.from.x < self.to.x {
                    "▶"
                } else {
                    "◀"
                };
                buf[(end_x, y)].set_symbol(arrow).set_style(style);
            }
        } else {
            // Diagonal - draw L-shape (vertical then horizontal)
            let mid_y = self.to.y;

            // Vertical segment
            let (v_start, v_end) = if self.from.y < mid_y {
                (self.from.y, mid_y)
            } else {
                (mid_y, self.from.y)
            };
            for y in v_start..=v_end {
                if in_bounds(ChatPosition::new(self.from.x, y)) {
                    buf[(self.from.x, y)].set_symbol("│").set_style(style);
                }
            }

            // Corner
            if in_bounds(ChatPosition::new(self.from.x, mid_y)) {
                let corner = if self.from.x < self.to.x && self.from.y < mid_y {
                    "└"
                } else if self.from.x < self.to.x && self.from.y > mid_y {
                    "┌"
                } else if self.from.x > self.to.x && self.from.y < mid_y {
                    "┘"
                } else {
                    "┐"
                };
                buf[(self.from.x, mid_y)]
                    .set_symbol(corner)
                    .set_style(style);
            }

            // Horizontal segment
            let (h_start, h_end) = if self.from.x < self.to.x {
                (self.from.x, self.to.x)
            } else {
                (self.to.x, self.from.x)
            };
            for x in h_start..h_end {
                if x != self.from.x && in_bounds(ChatPosition::new(x, mid_y)) {
                    buf[(x, mid_y)].set_symbol("─").set_style(style);
                }
            }

            // Arrow at end
            if in_bounds(ChatPosition::new(self.to.x, mid_y)) {
                let arrow = if self.from.x < self.to.x {
                    "▶"
                } else {
                    "◀"
                };
                buf[(self.to.x, mid_y)].set_symbol(arrow).set_style(style);
            }
        }

        // Draw label if present
        if let Some(label) = &self.label {
            let mid = self.length() / 2;
            let (lx, ly) = if self.is_vertical() {
                let y = if self.from.y < self.to.y {
                    self.from.y.saturating_add(mid)
                } else {
                    self.from.y.saturating_sub(mid)
                };
                (self.from.x.saturating_add(1), y)
            } else {
                let x = if self.from.x < self.to.x {
                    self.from.x.saturating_add(mid)
                } else {
                    self.from.x.saturating_sub(mid)
                };
                (x, self.from.y.saturating_sub(1))
            };

            if in_bounds(ChatPosition::new(lx, ly)) {
                let label_style = Style::default().fg(compat::AMBER_500);
                buf.set_string(lx, ly, label, label_style);
            }
        }

        // Draw flow dot if active
        if let Some(pos) = self.flow_position() {
            if in_bounds(pos) {
                let flow_style = Style::default()
                    .fg(compat::CYAN_500)
                    .add_modifier(Modifier::BOLD);
                buf[(pos.x, pos.y)].set_symbol("●").set_style(flow_style);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // --- Creation tests ---

    #[test]
    fn test_chat_edge_line_creation() {
        let edge = ChatEdgeLine::new(ChatPosition::new(10, 5), ChatPosition::new(10, 10));

        assert_eq!(edge.from.x, 10);
        assert_eq!(edge.to.y, 10);
        assert!(edge.is_vertical());
    }

    #[test]
    fn test_chat_edge_line_horizontal() {
        let edge = ChatEdgeLine::new(ChatPosition::new(5, 10), ChatPosition::new(15, 10));

        assert!(edge.is_horizontal());
        assert!(!edge.is_vertical());
    }

    #[test]
    fn test_chat_edge_line_with_label() {
        let edge = ChatEdgeLine::new(ChatPosition::new(10, 5), ChatPosition::new(10, 10))
            .with_label("with.ctx");

        assert_eq!(edge.label.as_deref(), Some("with.ctx"));
    }

    // --- Properties tests ---

    #[test]
    fn test_chat_edge_line_length_vertical() {
        let edge = ChatEdgeLine::new(ChatPosition::new(10, 0), ChatPosition::new(10, 10));

        assert_eq!(edge.length(), 10);
    }

    #[test]
    fn test_chat_edge_line_length_horizontal() {
        let edge = ChatEdgeLine::new(ChatPosition::new(0, 5), ChatPosition::new(20, 5));

        assert_eq!(edge.length(), 20);
    }

    #[test]
    fn test_chat_edge_line_diagonal() {
        let edge = ChatEdgeLine::new(ChatPosition::new(0, 0), ChatPosition::new(5, 5));

        assert!(!edge.is_vertical());
        assert!(!edge.is_horizontal());
        // Manhattan distance
        assert_eq!(edge.length(), 10);
    }

    // --- Animation tests ---

    #[test]
    fn test_chat_edge_line_flow_position() {
        let mut edge = ChatEdgeLine::new(ChatPosition::new(10, 0), ChatPosition::new(10, 10))
            .with_active(true);

        // Initial position at start
        let pos0 = edge.flow_position().unwrap();
        assert!(pos0.y <= 10);

        // After some ticks, position should still be valid
        for _ in 0..5 {
            edge.animation_tick = edge.animation_tick.wrapping_add(1);
        }
        let pos1 = edge.flow_position().unwrap();
        assert!(pos1.y <= 10);
    }

    #[test]
    fn test_chat_edge_line_no_flow_when_inactive() {
        let edge = ChatEdgeLine::new(ChatPosition::new(10, 0), ChatPosition::new(10, 10));

        // Inactive edges have no flow position
        assert!(edge.flow_position().is_none());
    }

    #[test]
    fn test_chat_edge_line_animation_tick_wraps() {
        let mut edge = ChatEdgeLine::new(ChatPosition::new(10, 0), ChatPosition::new(10, 10))
            .with_active(true);

        let initial = edge.animation_tick;
        edge.animation_tick = edge.animation_tick.wrapping_add(1);
        assert_eq!(edge.animation_tick, initial.wrapping_add(1));
    }

    // --- Render tests ---

    #[test]
    fn test_chat_edge_line_render_vertical() {
        let edge = ChatEdgeLine::new(ChatPosition::new(5, 1), ChatPosition::new(5, 5));

        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 10));
        edge.render(buf.area, &mut buf);

        // Should have vertical line characters
        assert_eq!(buf.cell((5, 2)).unwrap().symbol(), "│");
        assert_eq!(buf.cell((5, 3)).unwrap().symbol(), "│");
        assert_eq!(buf.cell((5, 4)).unwrap().symbol(), "│");
        // Arrow at end
        assert_eq!(buf.cell((5, 5)).unwrap().symbol(), "▼");
    }

    #[test]
    fn test_chat_edge_line_render_horizontal() {
        let edge = ChatEdgeLine::new(ChatPosition::new(1, 5), ChatPosition::new(8, 5));

        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 10));
        edge.render(buf.area, &mut buf);

        // Should have horizontal line characters
        assert_eq!(buf.cell((2, 5)).unwrap().symbol(), "─");
        assert_eq!(buf.cell((3, 5)).unwrap().symbol(), "─");
        // Arrow at end
        assert_eq!(buf.cell((8, 5)).unwrap().symbol(), "▶");
    }

    #[test]
    fn test_chat_edge_line_render_with_label() {
        let edge =
            ChatEdgeLine::new(ChatPosition::new(5, 1), ChatPosition::new(5, 10)).with_label("ctx");

        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 15));
        edge.render(buf.area, &mut buf);

        let content = buffer_to_string(&buf);
        assert!(
            content.contains("ctx"),
            "Label 'ctx' should appear in output"
        );
    }

    #[test]
    fn test_chat_edge_line_render_active() {
        let edge =
            ChatEdgeLine::new(ChatPosition::new(5, 1), ChatPosition::new(5, 5)).with_active(true);

        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 10));
        edge.render(buf.area, &mut buf);

        // Should have the flow dot somewhere
        let content = buffer_to_string(&buf);
        assert!(content.contains("●"), "Flow dot should appear when active");
    }

    // --- Integration tests ---

    #[test]
    fn test_chat_edge_line_exported() {
        let _ = ChatEdgeLine::new(ChatPosition::new(0, 0), ChatPosition::new(0, 5));
    }

    #[test]
    fn test_chat_position_default() {
        let pos = ChatPosition::default();
        assert_eq!(pos.x, 0);
        assert_eq!(pos.y, 0);
    }

    /// Helper to convert buffer to string for assertions
    fn buffer_to_string(buf: &Buffer) -> String {
        buf.content.iter().map(|c| c.symbol()).collect()
    }
}
