//! Context Assembled Panel
//!
//! Displays the context being assembled for LLM calls.

use ratatui::{
    layout::Rect,
    widgets::{Block, Borders},
    Frame,
};

/// Panel showing assembled context
pub struct ContextPanel;

impl ContextPanel {
    /// Render the context panel
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(" Context Assembled ")
            .borders(Borders::ALL);
        frame.render_widget(block, area);
    }
}
