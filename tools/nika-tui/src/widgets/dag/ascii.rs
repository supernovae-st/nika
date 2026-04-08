// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! DAG ASCII Widget
//!
//! Complete DAG visualization composing layout, nodes, and edges.
//! Renders a directed acyclic graph of workflow tasks with bindings and data previews.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};
use rustc_hash::FxHashMap;

use super::{
    edge::{render_merge, DagEdge},
    layout::{DagLayout, LayoutConfig, LayoutNode},
    node_box::NodeBox,
    node_data::{NodeBoxData, NodeBoxMode},
};
use crate::theme::Theme;

// ===============================================================================
// CONSTANTS
// ===============================================================================

/// Footer text color (fallback when no theme)
const DEFAULT_FOOTER_COLOR: Color = Color::Rgb(107, 114, 128); // gray-500

// ===============================================================================
// DAG ASCII WIDGET
// ===============================================================================

/// Complete DAG ASCII visualization widget
///
/// Composes DagLayout, NodeBox, and DagEdge to render a complete
/// DAG visualization with nodes, edges, bindings, and data previews.
pub struct DagAscii<'a> {
    /// Nodes to render
    nodes: &'a [NodeBoxData],
    /// Dependencies per node (node_id -> [dep_ids])
    dependencies: FxHashMap<String, Vec<String>>,
    /// Binding labels per edge (from_id -> [(to_id, binding)])
    bindings: FxHashMap<String, Vec<(String, String)>>,
    /// Data previews per binding (binding -> preview)
    previews: FxHashMap<String, String>,
    /// View mode
    mode: NodeBoxMode,
    /// Animation frame (for future animation support)
    frame: u8,
    /// Scroll offset (x, y)
    scroll: (u16, u16),
    /// Optional theme for colors
    theme: Option<&'a Theme>,
}

impl<'a> DagAscii<'a> {
    /// Create a new DagAscii widget with the given nodes
    pub fn new(nodes: &'a [NodeBoxData]) -> Self {
        Self {
            nodes,
            dependencies: FxHashMap::default(),
            bindings: FxHashMap::default(),
            previews: FxHashMap::default(),
            mode: NodeBoxMode::default(),
            frame: 0,
            scroll: (0, 0),
            theme: None,
        }
    }

    /// Set the theme for colors
    pub fn with_theme(mut self, theme: &'a Theme) -> Self {
        self.theme = Some(theme);
        self
    }

    /// Set dependencies map (node_id -> `[dep_ids]`)
    ///
    /// Dependencies indicate which tasks a node depends on (its predecessors).
    pub fn with_dependencies(mut self, deps: FxHashMap<String, Vec<String>>) -> Self {
        self.dependencies = deps;
        self
    }

    /// Set binding labels for edges
    ///
    /// Format: from_id -> [(to_id, binding_label)]
    /// The binding label (e.g., "{{with.data}}") is shown on the edge.
    pub fn with_bindings(mut self, bindings: FxHashMap<String, Vec<(String, String)>>) -> Self {
        self.bindings = bindings;
        self
    }

    /// Set data previews for bindings
    ///
    /// Format: binding_label -> preview_text
    /// Preview text is shown in grey below the binding label.
    pub fn with_previews(mut self, previews: FxHashMap<String, String>) -> Self {
        self.previews = previews;
        self
    }

    /// Set the display mode (Minimal or Expanded)
    pub fn mode(mut self, mode: NodeBoxMode) -> Self {
        self.mode = mode;
        self
    }

    /// Set the animation frame
    pub fn frame(mut self, frame: u8) -> Self {
        self.frame = frame;
        self
    }

    /// Set the scroll offset
    pub fn scroll(mut self, x: u16, y: u16) -> Self {
        self.scroll = (x, y);
        self
    }

    /// Prepare LayoutNode data from NodeBoxData
    fn prepare_layout_nodes(&self) -> Vec<LayoutNode<'_>> {
        self.nodes
            .iter()
            .map(|node| {
                let deps: Vec<&str> = self
                    .dependencies
                    .get(&node.id)
                    .map(|d| d.iter().map(|s| s.as_str()).collect())
                    .unwrap_or_default();

                LayoutNode::new(&node.id).with_dependencies(deps)
            })
            .collect()
    }

    /// Compute node widths for layout
    fn compute_node_widths(&self) -> FxHashMap<String, u16> {
        let mut widths: FxHashMap<String, u16> = FxHashMap::default();

        for node_data in self.nodes {
            let widget = NodeBox::new(node_data).mode(self.mode);
            widths.insert(node_data.id.clone(), widget.required_width());
        }

        widths
    }

    /// Find binding label for an edge from source to target
    fn find_binding(&self, from_id: &str, to_id: &str) -> Option<&String> {
        self.bindings.get(from_id).and_then(|targets| {
            targets
                .iter()
                .find(|(t, _)| t == to_id)
                .map(|(_, binding)| binding)
        })
    }

    /// Get preview for a binding label
    fn get_preview(&self, binding: &str) -> Option<&String> {
        self.previews.get(binding)
    }

    /// Render edges between nodes
    fn render_edges(&self, buf: &mut Buffer, area: Rect, layout: &DagLayout) {
        // Group dependencies by target node to detect merge points
        let mut target_sources: FxHashMap<&str, Vec<&str>> = FxHashMap::default();

        for node_data in self.nodes {
            if let Some(deps) = self.dependencies.get(&node_data.id) {
                for dep in deps {
                    target_sources
                        .entry(node_data.id.as_str())
                        .or_default()
                        .push(dep.as_str());
                }
            }
        }

        // Render edges
        for (target_id, sources) in &target_sources {
            let Some(target_pos) = layout.get(target_id) else {
                continue;
            };

            // Calculate target connection point (top center) with area offset
            let target_x =
                area.x + target_pos.x.saturating_sub(self.scroll.0) + target_pos.width / 2;
            let target_y = area.y + target_pos.y.saturating_sub(self.scroll.1);

            if sources.len() > 1 {
                // Multiple dependencies: use merge rendering
                let source_positions: Vec<(u16, u16)> = sources
                    .iter()
                    .filter_map(|src_id| {
                        layout.get(src_id).map(|pos| {
                            let src_x =
                                area.x + pos.x.saturating_sub(self.scroll.0) + pos.width / 2;
                            let src_y = area.y + pos.y.saturating_sub(self.scroll.1) + pos.height;
                            (src_x, src_y)
                        })
                    })
                    .collect();

                // Determine if any source is active (for animation)
                let active = self.frame > 0;

                render_merge(
                    &source_positions,
                    (target_x, target_y),
                    buf,
                    area,
                    active,
                    self.theme,
                );
            } else if let Some(source_id) = sources.first() {
                // Single dependency: simple edge
                if let Some(source_pos) = layout.get(source_id) {
                    let source_x =
                        area.x + source_pos.x.saturating_sub(self.scroll.0) + source_pos.width / 2;
                    let source_y =
                        area.y + source_pos.y.saturating_sub(self.scroll.1) + source_pos.height;

                    let mut edge = DagEdge::new((source_x, source_y), (target_x, target_y));

                    // Add binding label if present
                    if let Some(binding) = self.find_binding(source_id, target_id) {
                        edge = edge.with_binding(binding.clone());

                        // Add preview if present
                        if let Some(preview) = self.get_preview(binding) {
                            edge = edge.with_preview(preview.clone());
                        }
                    }

                    // Set active state based on frame
                    edge = edge.with_active(self.frame > 0);

                    edge.render(buf, area);
                }
            }
        }
    }

    /// Render all nodes
    fn render_nodes(&self, buf: &mut Buffer, area: Rect, layout: &DagLayout) {
        for node_data in self.nodes {
            if let Some(pos) = layout.get(&node_data.id) {
                // Apply scroll offset and area offset
                let x = area.x + pos.x.saturating_sub(self.scroll.0);
                let y = area.y + pos.y.saturating_sub(self.scroll.1);

                // Clip to area bounds
                if x >= area.x + area.width || y >= area.y + area.height {
                    continue;
                }

                // Clamp width/height to fit within area
                let max_width = (area.x + area.width).saturating_sub(x);
                let max_height = (area.y + area.height).saturating_sub(y);
                let width = pos.width.min(max_width);
                let height = pos.height.min(max_height);

                if width < 5 || height < 3 {
                    continue; // Too small to render (NodeBox needs min 3 height)
                }

                let node_rect = Rect::new(x, y, width, height);

                let widget = NodeBox::new(node_data).mode(self.mode);
                widget.render(node_rect, buf);
            }
        }
    }

    /// Render footer with stats
    fn render_footer(&self, buf: &mut Buffer, area: Rect, layer_count: usize) {
        let task_count = self.nodes.len();
        let footer_text = format!(" {} tasks {} layers", task_count, layer_count);

        // Position at bottom of area
        let footer_y = area.y + area.height.saturating_sub(1);
        let footer_x = area.x;

        // Use theme color with fallback
        let footer_color = self
            .theme
            .map(|t| t.text_muted)
            .unwrap_or(DEFAULT_FOOTER_COLOR);

        // SAFETY: Clamp footer_text length to u16::MAX before cast to prevent truncation
        let footer_len = footer_text.len().min(u16::MAX as usize) as u16;
        if footer_y >= area.y
            && footer_x.saturating_add(footer_len) <= area.x.saturating_add(area.width)
        {
            buf.set_string(
                footer_x,
                footer_y,
                &footer_text,
                Style::default().fg(footer_color),
            );
        }
    }
}

impl Widget for DagAscii<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Use theme color with fallback for messages
        let muted_color = self
            .theme
            .map(|t| t.text_muted)
            .unwrap_or(DEFAULT_FOOTER_COLOR);

        // Handle empty/small area
        if area.height < 3 || area.width < 10 {
            let msg = "(no tasks)";
            if area.width >= msg.len() as u16 {
                buf.set_string(area.x, area.y, msg, Style::default().fg(muted_color));
            }
            return;
        }

        // Handle empty nodes
        if self.nodes.is_empty() {
            let msg = "(no tasks)";
            let x = area.x + (area.width.saturating_sub(msg.len() as u16)) / 2;
            let y = area.y + area.height / 2;
            buf.set_string(x, y, msg, Style::default().fg(muted_color));
            return;
        }

        // Step 1: Prepare layout nodes
        let layout_nodes = self.prepare_layout_nodes();

        // Step 2: Compute node widths
        let node_widths = self.compute_node_widths();

        // Step 3: Create layout config based on mode
        let config = LayoutConfig {
            h_spacing: 4,
            v_spacing: if self.mode == NodeBoxMode::Expanded {
                4
            } else {
                2
            },
            max_node_width: 50,
            expanded: self.mode == NodeBoxMode::Expanded,
        };

        // Step 4: Compute layout
        let layout = DagLayout::compute(&layout_nodes, &config, Some(&node_widths));

        // Reserve space for footer
        let content_area = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1));

        // Step 5: Render edges first (below nodes)
        self.render_edges(buf, content_area, &layout);

        // Step 6: Render nodes (on top of edges)
        self.render_nodes(buf, content_area, &layout);

        // Step 7: Render footer
        self.render_footer(buf, area, layout.layer_count());
    }
}

// ===============================================================================
// TESTS
// ===============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{TaskStatus, VerbColor};

    // ═══════════════════════════════════════════════════════════════════════════
    // EMPTY NODES TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_dag_ascii_empty_nodes() {
        let nodes: Vec<NodeBoxData> = vec![];
        let widget = DagAscii::new(&nodes);

        let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 10));
        widget.render(Rect::new(0, 0, 40, 10), &mut buffer);

        // Should show "(no tasks)" message centered
        let content: String = (0..40)
            .map(|x| buffer.cell((x, 5)).unwrap().symbol().to_string())
            .collect();
        assert!(
            content.contains("(no tasks)"),
            "Expected '(no tasks)' in '{}'",
            content
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // SINGLE NODE TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_dag_ascii_single_node() {
        let nodes = vec![NodeBoxData::new("task1", VerbColor::Infer)
            .with_status(TaskStatus::Pending)
            .with_estimate("~1s")];

        let widget = DagAscii::new(&nodes);

        let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 10));
        widget.render(Rect::new(0, 0, 40, 10), &mut buffer);

        // Footer should show "1 tasks 1 layers"
        let footer: String = (0..40)
            .map(|x| buffer.cell((x, 9)).unwrap().symbol().to_string())
            .collect();
        assert!(
            footer.contains("1 tasks") && footer.contains("1 layers"),
            "Expected footer with stats in '{}'",
            footer
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // LINEAR CHAIN TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_dag_ascii_linear_chain() {
        let nodes = vec![
            NodeBoxData::new("a", VerbColor::Infer),
            NodeBoxData::new("b", VerbColor::Exec),
            NodeBoxData::new("c", VerbColor::Fetch),
        ];

        let mut deps = FxHashMap::default();
        deps.insert("b".to_string(), vec!["a".to_string()]);
        deps.insert("c".to_string(), vec!["b".to_string()]);

        let widget = DagAscii::new(&nodes).with_dependencies(deps);

        let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 15));
        widget.render(Rect::new(0, 0, 40, 15), &mut buffer);

        // Should have 3 tasks and 3 layers in footer
        let footer: String = (0..40)
            .map(|x| buffer.cell((x, 14)).unwrap().symbol().to_string())
            .collect();
        assert!(
            footer.contains("3 tasks") && footer.contains("3 layers"),
            "Expected '3 tasks 3 layers' in '{}'",
            footer
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // DEPENDENCY TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_dag_ascii_with_dependencies() {
        // Diamond: start -> {a, b} -> end
        let nodes = vec![
            NodeBoxData::new("start", VerbColor::Infer),
            NodeBoxData::new("a", VerbColor::Exec),
            NodeBoxData::new("b", VerbColor::Fetch),
            NodeBoxData::new("end", VerbColor::Agent),
        ];

        let mut deps = FxHashMap::default();
        deps.insert("a".to_string(), vec!["start".to_string()]);
        deps.insert("b".to_string(), vec!["start".to_string()]);
        deps.insert("end".to_string(), vec!["a".to_string(), "b".to_string()]);

        let widget = DagAscii::new(&nodes).with_dependencies(deps);

        let mut buffer = Buffer::empty(Rect::new(0, 0, 60, 20));
        widget.render(Rect::new(0, 0, 60, 20), &mut buffer);

        // Should render without panic and show correct stats
        let footer: String = (0..60)
            .map(|x| buffer.cell((x, 19)).unwrap().symbol().to_string())
            .collect();
        assert!(
            footer.contains("4 tasks") && footer.contains("3 layers"),
            "Expected '4 tasks 3 layers' in '{}'",
            footer
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // BINDING TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_dag_ascii_with_bindings() {
        let nodes = vec![
            NodeBoxData::new("generate", VerbColor::Infer),
            NodeBoxData::new("process", VerbColor::Exec),
        ];

        let mut deps = FxHashMap::default();
        deps.insert("process".to_string(), vec!["generate".to_string()]);

        let mut bindings = FxHashMap::default();
        bindings.insert(
            "generate".to_string(),
            vec![("process".to_string(), "{{with.data}}".to_string())],
        );

        let mut previews = FxHashMap::default();
        previews.insert("{{with.data}}".to_string(), "Hello world...".to_string());

        let widget = DagAscii::new(&nodes)
            .with_dependencies(deps)
            .with_bindings(bindings)
            .with_previews(previews);

        let mut buffer = Buffer::empty(Rect::new(0, 0, 50, 15));
        widget.render(Rect::new(0, 0, 50, 15), &mut buffer);

        // Should render without panic
        // Verify footer is present
        let footer: String = (0..50)
            .map(|x| buffer.cell((x, 14)).unwrap().symbol().to_string())
            .collect();
        assert!(
            footer.contains("2 tasks"),
            "Expected '2 tasks' in '{}'",
            footer
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // MODE TOGGLE TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_dag_ascii_mode_toggle() {
        let nodes = vec![NodeBoxData::new("task", VerbColor::Infer)
            .with_prompt_preview("Test prompt")
            .with_model("claude-sonnet")];

        // Render in minimal mode
        let widget_minimal = DagAscii::new(&nodes).mode(NodeBoxMode::Minimal);
        let mut buffer_minimal = Buffer::empty(Rect::new(0, 0, 40, 15));
        widget_minimal.render(Rect::new(0, 0, 40, 15), &mut buffer_minimal);

        // Render in expanded mode
        let widget_expanded = DagAscii::new(&nodes).mode(NodeBoxMode::Expanded);
        let mut buffer_expanded = Buffer::empty(Rect::new(0, 0, 40, 15));
        widget_expanded.render(Rect::new(0, 0, 40, 15), &mut buffer_expanded);

        // Both should render without panic
        // Both should show the same task count
        let footer_min: String = (0..40)
            .map(|x| buffer_minimal.cell((x, 14)).unwrap().symbol().to_string())
            .collect();
        let footer_exp: String = (0..40)
            .map(|x| buffer_expanded.cell((x, 14)).unwrap().symbol().to_string())
            .collect();

        assert!(
            footer_min.contains("1 tasks"),
            "Minimal footer: {}",
            footer_min
        );
        assert!(
            footer_exp.contains("1 tasks"),
            "Expanded footer: {}",
            footer_exp
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // BUILDER METHOD TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_dag_ascii_builder_methods() {
        let nodes = vec![NodeBoxData::new("task", VerbColor::Infer)];

        let mut deps = FxHashMap::default();
        deps.insert("task".to_string(), vec![]);

        let widget = DagAscii::new(&nodes)
            .with_dependencies(deps)
            .with_bindings(FxHashMap::default())
            .with_previews(FxHashMap::default())
            .mode(NodeBoxMode::Expanded)
            .frame(5)
            .scroll(10, 20);

        // Verify builder chain works (no assertions on internals, just no panic)
        assert_eq!(widget.frame, 5);
        assert_eq!(widget.scroll, (10, 20));
        assert_eq!(widget.mode, NodeBoxMode::Expanded);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // EDGE CASE TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_dag_ascii_small_area() {
        let nodes = vec![NodeBoxData::new("task", VerbColor::Infer)];
        let widget = DagAscii::new(&nodes);

        // Very small area
        let mut buffer = Buffer::empty(Rect::new(0, 0, 5, 2));
        widget.render(Rect::new(0, 0, 5, 2), &mut buffer);

        // Should handle gracefully without panic
    }

    #[test]
    fn test_dag_ascii_with_scroll_offset() {
        let nodes = vec![
            NodeBoxData::new("a", VerbColor::Infer),
            NodeBoxData::new("b", VerbColor::Exec),
        ];

        let mut deps = FxHashMap::default();
        deps.insert("b".to_string(), vec!["a".to_string()]);

        let widget = DagAscii::new(&nodes).with_dependencies(deps).scroll(5, 3);

        let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 15));
        widget.render(Rect::new(0, 0, 40, 15), &mut buffer);

        // Should render without panic even with scroll offset
    }

    #[test]
    fn test_dag_ascii_frame_affects_active_state() {
        let nodes = vec![
            NodeBoxData::new("a", VerbColor::Infer),
            NodeBoxData::new("b", VerbColor::Exec),
        ];

        let mut deps = FxHashMap::default();
        deps.insert("b".to_string(), vec!["a".to_string()]);

        // Frame 0 = inactive edges
        let widget_inactive = DagAscii::new(&nodes)
            .with_dependencies(deps.clone())
            .frame(0);

        let mut buffer_inactive = Buffer::empty(Rect::new(0, 0, 40, 15));
        widget_inactive.render(Rect::new(0, 0, 40, 15), &mut buffer_inactive);

        // Frame > 0 = active edges
        let widget_active = DagAscii::new(&nodes).with_dependencies(deps).frame(1);

        let mut buffer_active = Buffer::empty(Rect::new(0, 0, 40, 15));
        widget_active.render(Rect::new(0, 0, 40, 15), &mut buffer_active);

        // Both should render without panic
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // LAYOUT COMPUTATION TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_prepare_layout_nodes() {
        let nodes = vec![
            NodeBoxData::new("a", VerbColor::Infer),
            NodeBoxData::new("b", VerbColor::Exec),
        ];

        let mut deps = FxHashMap::default();
        deps.insert("b".to_string(), vec!["a".to_string()]);

        let widget = DagAscii::new(&nodes).with_dependencies(deps);
        let layout_nodes = widget.prepare_layout_nodes();

        assert_eq!(layout_nodes.len(), 2);
        assert_eq!(layout_nodes[0].id, "a");
        assert_eq!(layout_nodes[1].id, "b");
        assert!(layout_nodes[0].dependencies.is_empty());
        assert_eq!(layout_nodes[1].dependencies, vec!["a"]);
    }

    #[test]
    fn test_compute_node_widths() {
        let nodes = vec![
            NodeBoxData::new("short", VerbColor::Infer),
            NodeBoxData::new("very_long_task_name", VerbColor::Exec),
        ];

        let widget = DagAscii::new(&nodes);
        let widths = widget.compute_node_widths();

        assert!(widths.contains_key("short"));
        assert!(widths.contains_key("very_long_task_name"));

        // Longer name should have larger width
        assert!(
            widths.get("very_long_task_name").unwrap() >= widths.get("short").unwrap(),
            "Long name width {} should be >= short name width {}",
            widths.get("very_long_task_name").unwrap(),
            widths.get("short").unwrap()
        );
    }

    #[test]
    fn test_find_binding() {
        let nodes = vec![
            NodeBoxData::new("a", VerbColor::Infer),
            NodeBoxData::new("b", VerbColor::Exec),
        ];

        let mut bindings = FxHashMap::default();
        bindings.insert(
            "a".to_string(),
            vec![("b".to_string(), "{{with.data}}".to_string())],
        );

        let widget = DagAscii::new(&nodes).with_bindings(bindings);

        assert_eq!(
            widget.find_binding("a", "b"),
            Some(&"{{with.data}}".to_string())
        );
        assert_eq!(widget.find_binding("a", "c"), None);
        assert_eq!(widget.find_binding("x", "y"), None);
    }

    #[test]
    fn test_get_preview() {
        let nodes = vec![NodeBoxData::new("a", VerbColor::Infer)];

        let mut previews = FxHashMap::default();
        previews.insert("{{with.data}}".to_string(), "preview text".to_string());

        let widget = DagAscii::new(&nodes).with_previews(previews);

        assert_eq!(
            widget.get_preview("{{with.data}}"),
            Some(&"preview text".to_string())
        );
        assert_eq!(widget.get_preview("{{with.other}}"), None);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // AREA OFFSET TESTS (verifies nodes render in correct panel location)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_dag_ascii_renders_with_area_offset() {
        // Simulate a panel at (20, 5) with width 40, height 15
        // This is the typical case in Home view where DAG preview is on the right
        let nodes =
            vec![NodeBoxData::new("task1", VerbColor::Infer).with_status(TaskStatus::Pending)];

        let widget = DagAscii::new(&nodes);

        // Create buffer larger than area to detect misaligned rendering
        let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 30));
        let area = Rect::new(20, 5, 40, 15);
        widget.render(area, &mut buffer);

        // The node should be rendered WITHIN the area (20..60, 5..20)
        // NOT at (0, 0) which would be wrong

        // Check that nothing is rendered at (0, 0) - the old bug location
        let cell_origin = buffer.cell((0, 0)).unwrap();
        assert_eq!(
            cell_origin.symbol(),
            " ",
            "Nothing should render at buffer origin (0,0)"
        );

        // Check that footer appears at correct position (within area)
        // Footer is at y = area.y + area.height - 1 = 5 + 15 - 1 = 19
        let footer_y = 19;
        let footer: String = (20..60)
            .map(|x| buffer.cell((x, footer_y)).unwrap().symbol().to_string())
            .collect();
        assert!(
            footer.contains("1 tasks") && footer.contains("1 layers"),
            "Footer should be at y={} within area, got: '{}'",
            footer_y,
            footer
        );

        // Verify node box renders within area (not at origin)
        // The node should start somewhere after area.x (20) and area.y (5)
        let node_area_start_y = 5;
        let found_node_content = (20..60).any(|x| {
            let cell = buffer.cell((x, node_area_start_y)).unwrap();
            cell.symbol() != " "
        });
        assert!(
            found_node_content,
            "Node content should be rendered within area starting at y={}",
            node_area_start_y
        );
    }

    #[test]
    fn test_dag_ascii_clips_nodes_to_area() {
        // Test that nodes too wide for the area are clipped
        let nodes = vec![NodeBoxData::new(
            "very_long_task_name_that_exceeds_width",
            VerbColor::Infer,
        )];

        let widget = DagAscii::new(&nodes);

        // Small area
        let mut buffer = Buffer::empty(Rect::new(0, 0, 50, 20));
        let area = Rect::new(5, 5, 20, 10);
        widget.render(area, &mut buffer);

        // Should render without panic and clip to area
        // Check that nothing renders outside the area bounds
        for x in 0..5 {
            let cell = buffer.cell((x, 5)).unwrap();
            assert_eq!(
                cell.symbol(),
                " ",
                "Nothing should render before area.x at ({}, 5)",
                x
            );
        }
    }
}
