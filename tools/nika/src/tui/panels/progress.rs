//! Workflow Progress Panel
//!
//! Displays overall workflow execution progress.

use ratatui::{
    layout::Rect,
    widgets::{Block, Borders},
    Frame,
};

/// Panel showing workflow progress
pub struct ProgressPanel;

impl ProgressPanel {
    /// Render the progress panel
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(" Workflow Progress ")
            .borders(Borders::ALL);
        frame.render_widget(block, area);
    }
}
