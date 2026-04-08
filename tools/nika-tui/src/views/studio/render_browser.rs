// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Browser panel rendering for the Studio view.
//!
//! Contains `render_browser()` and `render_quick_access()` which draw the
//! left-hand file tree panel with Quick Access section and TreeWidget.

use std::time::{Duration, Instant};

use camino::Utf8Path;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::StudioView;
use crate::theme::Theme;
use crate::views::studio::types::StudioFocus;
use crate::widgets::tree::{build_git_status_cache, TreeColors, TreeNode, TreeWidget};

impl StudioView {
    /// Render the browser panel with TreeWidget
    pub(super) fn render_browser(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let style = self.border_style(StudioFocus::Browser, theme);

        // UX: Add focus indicator to show which panel is active
        let focus_indicator = if self.focus == StudioFocus::Browser {
            "●"
        } else {
            " "
        };

        // Show filter badge in title so users know which filter is active
        let title = if self.filter_config.filter.badge().is_empty() {
            format!(" {} Browser ", focus_indicator)
        } else {
            format!(
                " {} Browser [{}] ",
                focus_indicator,
                self.filter_config.filter.badge()
            )
        };

        let block = Block::default()
            .title(Span::styled(title, style))
            .borders(Borders::ALL)
            .border_style(style);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Split into Quick Access (top) + Tree (bottom)
        let quick_access_height = if self.quick_access.is_empty() {
            0
        } else {
            // Header (1) + files (n) + separator (1)
            (self.quick_access.len() + 2) as u16
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(quick_access_height), Constraint::Min(3)])
            .split(inner);

        // Render Quick Access section
        if !self.quick_access.is_empty() {
            self.render_quick_access(frame, chunks[0], theme);
        }

        let tree_area = chunks[1];

        // Refresh git cache periodically (every 5 seconds)
        const GIT_CACHE_TTL: Duration = Duration::from_secs(5);
        let root_path = Utf8Path::from_path(&self.root_dir).unwrap_or(Utf8Path::new("."));

        if self.git_cache_time.elapsed() > GIT_CACHE_TTL {
            self.git_cache = build_git_status_cache(root_path);
            self.git_cache_time = Instant::now();
            // Invalidate tree cache when git status changes
            self.cached_tree = None;
        }

        // PERF: Build tree with caching (rebuild only when cache is empty).
        // Borrow from cache instead of cloning — eliminates deep tree clone per frame.
        let tree_rebuilt = self.cached_tree.is_none();
        if self.cached_tree.is_none() {
            let tree = TreeNode::build_tree(root_path, Some(&self.git_cache), None);
            self.cached_tree = Some(tree);
        }
        let root_node = self.cached_tree.as_ref().unwrap();

        // Only update visible_nodes when tree structure changes
        // This was being called EVERY frame causing severe performance issues
        if tree_rebuilt {
            self.tree_state.update_visible_nodes(root_node);
            self.tree_state.select_first_if_none();
            // Ensure root is expanded on rebuild
            if !self.tree_state.is_expanded(root_node.id) {
                self.tree_state.expand(root_node.id);
                self.tree_state.update_visible_nodes(root_node);
            }
        }

        // Only rebuild tree colors when theme actually changes (avoids 16 Color copies per frame)
        if self.tree_colors.bg != theme.background {
            self.tree_colors = TreeColors::from_theme(theme);
        }

        // Render TreeWidget with state (borrows from cache, zero clone)
        // FilterConfig is Copy — no heap allocation on clone
        let tree_widget = TreeWidget::new(root_node)
            .colors(self.tree_colors.clone())
            .ticker(&self.animation_ticker)
            .filter(self.filter_config);

        frame.render_stateful_widget(tree_widget, tree_area, &mut self.tree_state);
    }

    /// Render Quick Access section
    fn render_quick_access(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let mut lines: Vec<Line> = Vec::new();

        // Header with emoji
        lines.push(Line::from(vec![
            Span::styled("⚡ ", Style::default().fg(theme.status_running)),
            Span::styled(
                "QUICK ACCESS",
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        // Quick access files
        for path in &self.quick_access {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("🦋 ", Style::default()),
                Span::styled(name, Style::default().fg(theme.border_focused)),
            ]));
        }

        // Separator
        let sep_width = area.width.saturating_sub(2) as usize;
        lines.push(Line::from(Span::styled(
            "─".repeat(sep_width),
            Style::default().fg(theme.border_normal),
        )));

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }
}
