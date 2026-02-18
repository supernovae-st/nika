//! Agent Reasoning Panel
//!
//! Displays agent reasoning and decision-making.

use ratatui::{
    layout::Rect,
    widgets::{Block, Borders},
    Frame,
};

/// Panel showing agent reasoning
#[allow(dead_code)]
pub struct ReasoningPanel;

impl ReasoningPanel {
    /// Render the reasoning panel
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(" Agent Reasoning ")
            .borders(Borders::ALL);
        frame.render_widget(block, area);
    }
}
