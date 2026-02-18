//! Graph Traversal Panel
//!
//! Displays MCP/NovaNet graph traversal state.

use ratatui::{
    layout::Rect,
    widgets::{Block, Borders},
    Frame,
};

/// Panel showing graph traversal
pub struct GraphPanel;

impl GraphPanel {
    /// Render the graph panel
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(" Graph Traversal ")
            .borders(Borders::ALL);
        frame.render_widget(block, area);
    }
}
