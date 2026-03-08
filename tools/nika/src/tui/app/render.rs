//! App Frame Rendering
//!
//! Contains frame rendering logic for the unified TUI architecture.

use ratatui::widgets::{Block, Borders};

use crate::error::{NikaError, Result};

use super::super::views::TuiView;
use super::App;

impl App {
    /// Render the current frame based on active view
    ///
    /// Uses the View trait's render method for each view type.
    /// Each view handles its own layout and widget rendering.
    pub(crate) fn render_unified_frame(&mut self) -> Result<()> {
        if let Some(ref mut terminal) = self.terminal {
            // Clone theme for borrow checker
            let _theme = self.theme.clone();
            let current_view = self.current_view;

            terminal
                .draw(|f| {
                    let area = f.area();
                    // Render current view (simplified - just show a header)
                    // TODO: Delegate to view-specific render methods
                    let title = match current_view {
                        TuiView::Studio => "Studio",
                        TuiView::Runner => "Runner",
                        TuiView::Chat => "Chat",
                        TuiView::Settings => "Settings",
                    };
                    let block = Block::default().title(title).borders(Borders::ALL);
                    f.render_widget(block, area);
                })
                .map_err(|e| NikaError::TuiError {
                    reason: format!("Failed to render frame: {}", e),
                })?;
        }
        Ok(())
    }
}
