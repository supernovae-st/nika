// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Tree widget renderer with icons, colors, and animations.
//!
//! Provides a full tree view widget that renders TreeNodes with:
//! - Proper indentation and tree lines
//! - NodeKind-based icons and colors
//! - Selection highlighting
//! - Glow animation for ecosystem files
//! - Git status indicators

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, StatefulWidget, Widget},
};

use super::{
    animation::AnimationTicker, colors::TreeColors, filter::FilterConfig, NodeKind, TreeNode,
    TreeState,
};

/// Tree branch characters (VS Code-style box-drawing)
pub mod chars {
    /// Vertical line (indent guide)
    pub const VERTICAL: &str = "│";
    /// Branch (tee) - middle child
    pub const BRANCH: &str = "├";
    /// Last branch (corner) - last child
    pub const LAST: &str = "└";
    /// Horizontal line (connector)
    pub const HORIZONTAL: &str = "─";
    /// Collapsed folder arrow (Nerd Font)
    pub const COLLAPSED: &str = "";
    /// Expanded folder arrow (Nerd Font)
    pub const EXPANDED: &str = "";
    /// Collapsed folder arrow (fallback)
    pub const COLLAPSED_FALLBACK: &str = "▶";
    /// Expanded folder arrow (fallback)
    pub const EXPANDED_FALLBACK: &str = "▼";
}

/// Tree widget for rendering file trees
pub struct TreeWidget<'a> {
    /// Root node to render
    root: &'a TreeNode,
    /// Block wrapper
    block: Option<Block<'a>>,
    /// Color palette
    colors: TreeColors,
    /// Animation ticker for glow effects
    ticker: Option<&'a AnimationTicker>,
    /// Filter configuration
    filter: FilterConfig,
    /// Highlight style for selected items
    highlight_style: Style,
    /// Whether to use NerdFont icons
    use_nerd_icons: bool,
}

impl<'a> TreeWidget<'a> {
    /// Create a new tree widget
    pub fn new(root: &'a TreeNode) -> Self {
        Self {
            root,
            block: None,
            colors: TreeColors::default(),
            ticker: None,
            filter: FilterConfig::default(),
            highlight_style: Style::default()
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::REVERSED),
            use_nerd_icons: false,
        }
    }

    /// Set the block wrapper
    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    /// Set the color palette
    pub fn colors(mut self, colors: TreeColors) -> Self {
        self.colors = colors;
        self
    }

    /// Set the animation ticker for glow effects
    pub fn ticker(mut self, ticker: &'a AnimationTicker) -> Self {
        self.ticker = Some(ticker);
        self
    }

    /// Set the filter configuration
    pub fn filter(mut self, filter: FilterConfig) -> Self {
        self.filter = filter;
        self
    }

    /// Set the highlight style
    pub fn highlight_style(mut self, style: Style) -> Self {
        self.highlight_style = style;
        self
    }

    /// Enable NerdFont icons
    pub fn nerd_icons(mut self, enabled: bool) -> Self {
        self.use_nerd_icons = enabled;
        self
    }

    /// Flatten the tree into visible lines
    fn flatten_visible(&self, node: &TreeNode, state: &TreeState) -> Vec<FlatNode> {
        let mut result = Vec::new();
        self.flatten_recursive(node, state, &mut result, &[], true);
        result
    }

    fn flatten_recursive(
        &self,
        node: &TreeNode,
        state: &TreeState,
        result: &mut Vec<FlatNode>,
        ancestors: &[bool], // true = last child at each level
        is_visible: bool,
    ) {
        // Check filter
        let is_hidden = node.is_hidden();
        if !self.filter.is_visible(&node.kind, is_hidden) {
            return;
        }

        if is_visible {
            result.push(FlatNode {
                id: node.id,
                name: node.name.clone(),
                kind: node.kind,
                is_directory: node.is_directory(),
                is_expanded: state.is_expanded(node.id),
                ancestors: ancestors.to_vec(),
                git_status: node.git_status,
            });
        }

        // Recurse into children if expanded
        if node.is_directory() && state.is_expanded(node.id) {
            let child_count = node.children.len();
            for (i, child) in node.children.iter().enumerate() {
                let is_last_child = i == child_count - 1;
                let mut child_ancestors = ancestors.to_vec();
                child_ancestors.push(is_last_child);
                self.flatten_recursive(child, state, result, &child_ancestors, true);
            }
        }
    }

    /// Render a single node line
    fn render_line(&self, node: &FlatNode, is_selected: bool, width: u16) -> Line<'static> {
        let mut spans = Vec::new();

        // Build indent with tree lines
        for (i, &is_last_at_level) in node.ancestors.iter().enumerate() {
            if i == node.ancestors.len() - 1 {
                // Last level shows branch/corner
                if is_last_at_level {
                    spans.push(Span::styled(
                        chars::LAST,
                        Style::default().fg(self.colors.branch_lines),
                    ));
                } else {
                    spans.push(Span::styled(
                        chars::BRANCH,
                        Style::default().fg(self.colors.branch_lines),
                    ));
                }
                spans.push(Span::styled(
                    chars::HORIZONTAL,
                    Style::default().fg(self.colors.branch_lines),
                ));
            } else {
                // Inner levels show vertical lines or space
                if is_last_at_level {
                    spans.push(Span::raw("  "));
                } else {
                    spans.push(Span::styled(
                        chars::VERTICAL,
                        Style::default().fg(self.colors.indent_guide),
                    ));
                    spans.push(Span::raw(" "));
                }
            }
        }

        // Extra padding after tree lines for cleaner VS Code-like spacing
        spans.push(Span::raw(" "));

        // Folder expand/collapse indicator
        if node.is_directory {
            let indicator = if node.is_expanded {
                if self.use_nerd_icons {
                    chars::EXPANDED
                } else {
                    chars::EXPANDED_FALLBACK
                }
            } else if self.use_nerd_icons {
                chars::COLLAPSED
            } else {
                chars::COLLAPSED_FALLBACK
            };
            spans.push(Span::styled(
                indicator,
                Style::default().fg(self.colors.directory),
            ));
            spans.push(Span::raw(" "));
        } else {
            // File - add spacing to align with folder indicators (chevron width)
            spans.push(Span::raw("  "));
        }

        // Icon
        let icon = if self.use_nerd_icons {
            node.kind.nerd_icon()
        } else {
            node.kind.icon()
        };

        // Get base color
        let is_hidden = node.kind.is_hidden();
        let base_color = self.colors.node_color(node.kind, is_hidden);

        // Apply glow for ecosystem files on selection
        let icon_style = if node.kind.is_ecosystem() && is_selected {
            if let Some(ticker) = self.ticker {
                let glow = ticker.glow_factor();
                // Modulate color brightness
                let icon_color = self.colors.icon_color(node.kind);
                // Create glowing style
                Style::default()
                    .fg(interpolate_color(icon_color, Color::White, glow - 0.7))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(self.colors.icon_color(node.kind))
                    .add_modifier(Modifier::BOLD)
            }
        } else {
            Style::default().fg(self.colors.icon_color(node.kind))
        };

        spans.push(Span::styled(format!("{} ", icon), icon_style));

        // Name
        let name_style = if is_selected {
            self.highlight_style.fg(base_color)
        } else {
            Style::default().fg(base_color)
        };

        // Truncate name if needed (UTF-8 safe)
        let max_name_len = width
            .saturating_sub(spans.iter().map(|s| s.width()).sum::<usize>() as u16 + 2)
            as usize;
        let name = if node.name.chars().count() > max_name_len {
            // Use chars() for UTF-8 safe truncation
            let truncate_at = max_name_len.saturating_sub(3);
            let truncated: String = node.name.chars().take(truncate_at).collect();
            format!("{}...", truncated)
        } else {
            node.name.clone()
        };

        spans.push(Span::styled(name, name_style));

        // Git status badge
        if let Some(git_status) = node.git_status {
            let (badge, color) = match git_status {
                super::GitStatus::Modified => ("M", self.colors.git_modified),
                super::GitStatus::Added => ("A", self.colors.git_added),
                super::GitStatus::Deleted => ("D", self.colors.git_deleted),
                super::GitStatus::Untracked => ("?", self.colors.git_untracked),
                super::GitStatus::Conflict => ("!", Color::Red),
                super::GitStatus::Ignored => ("", self.colors.hidden),
                super::GitStatus::Clean => ("", self.colors.hidden),
            };
            if !badge.is_empty() {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(badge, Style::default().fg(color)));
            }
        }

        Line::from(spans)
    }
}

/// Flattened node for rendering
#[derive(Debug, Clone)]
struct FlatNode {
    id: u64,
    name: String,
    kind: NodeKind,
    is_directory: bool,
    is_expanded: bool,
    ancestors: Vec<bool>,
    git_status: Option<super::GitStatus>,
}

/// Interpolate between two colors
fn interpolate_color(from: Color, to: Color, factor: f32) -> Color {
    let factor = factor.clamp(0.0, 1.0);

    match (from, to) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let r = (r1 as f32 + (r2 as f32 - r1 as f32) * factor) as u8;
            let g = (g1 as f32 + (g2 as f32 - g1 as f32) * factor) as u8;
            let b = (b1 as f32 + (b2 as f32 - b1 as f32) * factor) as u8;
            Color::Rgb(r, g, b)
        }
        _ => from, // Fallback for non-RGB colors
    }
}

impl StatefulWidget for TreeWidget<'_> {
    type State = TreeState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        // Render block if present
        let inner = if let Some(ref block) = self.block {
            let inner = block.inner(area);
            block.clone().render(area, buf);
            inner
        } else {
            area
        };

        // Update viewport height
        state.set_viewport_height(inner.height as usize);

        // Flatten visible nodes
        let flat_nodes = self.flatten_visible(self.root, state);

        // Get scroll offset
        let scroll_offset = state.scroll_offset();

        // Render visible lines
        for (i, node) in flat_nodes
            .iter()
            .skip(scroll_offset)
            .take(inner.height as usize)
            .enumerate()
        {
            let y = inner.y + i as u16;
            let is_selected = state.is_selected(node.id);
            let line = self.render_line(node, is_selected, inner.width);

            // Render line
            buf.set_line(inner.x, y, &line, inner.width);

            // Full row highlight for selected item
            if is_selected {
                for x in inner.x..(inner.x + inner.width) {
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_style(cell.style().add_modifier(Modifier::REVERSED));
                    }
                }
            }
        }

        // Show scroll indicator if there are more items
        let total = flat_nodes.len();
        let visible = inner.height as usize;
        if total > visible && scroll_offset > 0 {
            // Top scroll indicator
            if let Some(cell) = buf.cell_mut((inner.x + inner.width - 1, inner.y)) {
                cell.set_char('▲');
                cell.set_fg(self.colors.fg);
            }
        }
        if total > visible && scroll_offset + visible < total {
            // Bottom scroll indicator
            if let Some(cell) =
                buf.cell_mut((inner.x + inner.width - 1, inner.y + inner.height - 1))
            {
                cell.set_char('▼');
                cell.set_fg(self.colors.fg);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;

    fn create_test_tree() -> TreeNode {
        TreeNode {
            id: 1,
            name: "root".to_string(),
            path: Utf8PathBuf::from("."),
            kind: NodeKind::Directory,
            git_status: None,
            children: vec![
                TreeNode {
                    id: 2,
                    name: "workflow.nika.yaml".to_string(),
                    path: Utf8PathBuf::from("./workflow.nika.yaml"),
                    kind: NodeKind::NikaWorkflow,
                    git_status: Some(super::super::GitStatus::Modified),
                    children: vec![],
                    expanded: false,
                    depth: 1,
                },
                TreeNode {
                    id: 3,
                    name: "src".to_string(),
                    path: Utf8PathBuf::from("./src"),
                    kind: NodeKind::SrcFolder,
                    git_status: None,
                    children: vec![TreeNode {
                        id: 4,
                        name: "main.rs".to_string(),
                        path: Utf8PathBuf::from("./src/main.rs"),
                        kind: NodeKind::Rust,
                        git_status: None,
                        children: vec![],
                        expanded: false,
                        depth: 2,
                    }],
                    expanded: false,
                    depth: 1,
                },
            ],
            expanded: true,
            depth: 0,
        }
    }

    #[test]
    fn test_tree_widget_new() {
        let root = create_test_tree();
        let widget = TreeWidget::new(&root);
        assert!(!widget.use_nerd_icons);
    }

    #[test]
    fn test_tree_widget_with_options() {
        let root = create_test_tree();
        let colors = TreeColors::solarized_light();
        let widget = TreeWidget::new(&root).colors(colors).nerd_icons(true);
        assert!(widget.use_nerd_icons);
    }

    #[test]
    fn test_flatten_visible() {
        let root = create_test_tree();
        let mut state = TreeState::new();
        state.expand(1); // Expand root

        let widget = TreeWidget::new(&root);
        let flat = widget.flatten_visible(&root, &state);

        // root + workflow + src
        assert_eq!(flat.len(), 3);
        assert_eq!(flat[0].name, "root");
        assert_eq!(flat[1].name, "workflow.nika.yaml");
        assert_eq!(flat[2].name, "src");
    }

    #[test]
    fn test_flatten_with_nested_expand() {
        let root = create_test_tree();
        let mut state = TreeState::new();
        state.expand(1); // Expand root
        state.expand(3); // Expand src

        let widget = TreeWidget::new(&root);
        let flat = widget.flatten_visible(&root, &state);

        // root + workflow + src + main.rs
        assert_eq!(flat.len(), 4);
        assert_eq!(flat[3].name, "main.rs");
    }

    #[test]
    fn test_interpolate_color() {
        let white = Color::Rgb(255, 255, 255);
        let black = Color::Rgb(0, 0, 0);

        // Midpoint should be gray
        let mid = interpolate_color(black, white, 0.5);
        if let Color::Rgb(r, g, b) = mid {
            assert!(r > 100 && r < 150);
            assert!(g > 100 && g < 150);
            assert!(b > 100 && b < 150);
        }

        // Factor 0 should return from
        let zero = interpolate_color(black, white, 0.0);
        assert_eq!(zero, black);

        // Factor 1 should return to
        let one = interpolate_color(black, white, 1.0);
        assert_eq!(one, white);
    }

    #[test]
    fn test_filter_affects_visibility() {
        let root = create_test_tree();
        let mut state = TreeState::new();
        state.expand(1);

        // With workflow filter
        let mut filter = FilterConfig::new();
        filter.filter = super::super::filter::TreeFilter::WorkflowsOnly;

        let widget = TreeWidget::new(&root).filter(filter);
        let flat = widget.flatten_visible(&root, &state);

        // Only directories and workflow files pass
        // root (dir) + workflow.nika.yaml + src (dir)
        assert!(flat.iter().any(|n| n.name == "workflow.nika.yaml"));
    }
}
