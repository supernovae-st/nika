//! Ollama local models tab
//!
//! Displays installed models with pull/delete actions.
//! Shows download progress when pulling new models.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

// =============================================================================
// SEMANTIC COLOR CONSTANTS (v0.9.1 - aligned with Theme)
// =============================================================================

/// Header text - high contrast
const COLOR_TEXT_PRIMARY: Color = Color::Rgb(229, 231, 235); // Gray-200

/// Secondary text - hints, instructions
const COLOR_TEXT_SECONDARY: Color = Color::Rgb(156, 163, 175); // Gray-400

/// Muted text - shortcuts
const COLOR_TEXT_MUTED: Color = Color::Rgb(107, 114, 128); // Gray-500

/// Selected row background
const COLOR_BG_SELECTED: Color = Color::Rgb(30, 41, 59); // Slate-800

/// Status: success
const COLOR_STATUS_SUCCESS: Color = Color::Rgb(34, 197, 94); // Emerald-500

/// Status: error
const COLOR_STATUS_ERROR: Color = Color::Rgb(239, 68, 68); // Red-500

use super::super::components::DownloadGauge;
use super::super::ollama_client::OllamaModelInfo;
use super::super::state::DownloadState;

/// Ollama models tab
pub struct OllamaTab<'a> {
    models: &'a [OllamaModelInfo],
    selected_idx: usize,
    download_state: &'a DownloadState,
    is_available: bool,
}

impl<'a> OllamaTab<'a> {
    pub fn new(
        models: &'a [OllamaModelInfo],
        selected_idx: usize,
        download_state: &'a DownloadState,
    ) -> Self {
        Self {
            models,
            selected_idx,
            download_state,
            is_available: true,
        }
    }

    /// Set availability status (whether Ollama is running)
    pub fn available(mut self, available: bool) -> Self {
        self.is_available = available;
        self
    }

    /// Get model count
    pub fn model_count(&self) -> usize {
        self.models.len()
    }

    /// Get selected model name
    pub fn selected_model(&self) -> Option<&str> {
        self.models.get(self.selected_idx).map(|m| m.name.as_str())
    }
}

impl Widget for OllamaTab<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 {
            return;
        }

        // If Ollama is not available, show message
        if !self.is_available {
            let msg = "🦙 Ollama is not running";
            let hint = "Start with: ollama serve";
            buf.set_string(
                area.x + 2,
                area.y + 1,
                msg,
                Style::default()
                    .fg(COLOR_STATUS_ERROR)
                    .add_modifier(Modifier::BOLD),
            );
            buf.set_string(
                area.x + 2,
                area.y + 2,
                hint,
                Style::default().fg(COLOR_TEXT_MUTED),
            );
            return;
        }

        // Header
        let header = format!("Installed Models ({})", self.models.len());
        buf.set_string(
            area.x + 1,
            area.y,
            &header,
            Style::default()
                .fg(COLOR_TEXT_PRIMARY)
                .add_modifier(Modifier::BOLD),
        );

        // Shortcuts hint
        let shortcuts = "[p] Pull  [d] Delete";
        let shortcuts_x = area.right().saturating_sub(shortcuts.len() as u16 + 1);
        buf.set_string(
            shortcuts_x,
            area.y,
            shortcuts,
            Style::default().fg(COLOR_TEXT_MUTED),
        );

        // Calculate list area (leave space for download gauge at bottom)
        let download_active = self.download_state.is_active();
        let list_height = if download_active {
            area.height.saturating_sub(5) // Leave room for gauge
        } else {
            area.height.saturating_sub(2) // Just header
        };

        // Render model list
        for (i, model) in self.models.iter().enumerate() {
            let y = area.y + 2 + i as u16;
            if y >= area.y + 2 + list_height {
                break;
            }

            let is_selected = i == self.selected_idx;
            let style = if is_selected {
                Style::default()
                    .fg(COLOR_TEXT_PRIMARY)
                    .bg(COLOR_BG_SELECTED)
            } else {
                Style::default().fg(COLOR_TEXT_SECONDARY)
            };

            // Selection indicator + Model name
            let indicator = if is_selected { "▸" } else { " " };
            let line = format!(
                "{} 🦙 {:20} {:>10} {:8}",
                indicator,
                truncate_model_name(&model.name, 20),
                model.size_display(),
                &model.details.parameter_size
            );

            buf.set_string(area.x + 1, y, &line, style);
        }

        // Show "No models installed" if empty
        if self.models.is_empty() {
            buf.set_string(
                area.x + 2,
                area.y + 2,
                "No models installed",
                Style::default().fg(COLOR_TEXT_MUTED),
            );
            buf.set_string(
                area.x + 2,
                area.y + 3,
                "Press [p] to pull a model (e.g., llama3.2)",
                Style::default().fg(COLOR_TEXT_SECONDARY),
            );
        }

        // Download progress gauge
        if let DownloadState::Downloading {
            model,
            progress,
            downloaded,
            total,
        } = self.download_state
        {
            let gauge_area = Rect {
                x: area.x + 1,
                y: area.bottom().saturating_sub(4),
                width: area.width.saturating_sub(2),
                height: 3,
            };

            DownloadGauge::new(model, *progress, *downloaded, *total).render(gauge_area, buf);
        }

        // Show complete/failed status
        match self.download_state {
            DownloadState::Complete { model } => {
                let msg = format!("✓ {} installed successfully", model);
                buf.set_string(
                    area.x + 2,
                    area.bottom().saturating_sub(2),
                    &msg,
                    Style::default().fg(COLOR_STATUS_SUCCESS),
                );
            }
            DownloadState::Failed { model, error } => {
                let msg = format!("✗ Failed to pull {}: {}", model, error);
                let truncated = if msg.len() > area.width as usize - 4 {
                    format!("{}...", &msg[..area.width as usize - 7])
                } else {
                    msg
                };
                buf.set_string(
                    area.x + 2,
                    area.bottom().saturating_sub(2),
                    &truncated,
                    Style::default().fg(COLOR_STATUS_ERROR),
                );
            }
            _ => {}
        }
    }
}

/// Truncate model name for display
fn truncate_model_name(name: &str, max_len: usize) -> String {
    if name.len() <= max_len {
        name.to_string()
    } else {
        format!("{}…", &name[..max_len.saturating_sub(1)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::widgets::provider_modal::ollama_client::OllamaModelDetails;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    fn make_test_model(name: &str, size: u64) -> OllamaModelInfo {
        OllamaModelInfo {
            name: name.to_string(),
            size,
            digest: "sha256:abc".to_string(),
            modified_at: "2026-02-24".to_string(),
            details: OllamaModelDetails {
                parameter_size: "8B".to_string(),
                quantization_level: "Q4_0".to_string(),
                family: Some("llama".to_string()),
            },
        }
    }

    #[test]
    fn test_ollama_tab_model_count() {
        let models = vec![make_test_model("llama3.2", 4_700_000_000)];
        let download_state = DownloadState::Idle;
        let tab = OllamaTab::new(&models, 0, &download_state);
        assert_eq!(tab.model_count(), 1);
    }

    #[test]
    fn test_ollama_tab_selected_model() {
        let models = vec![
            make_test_model("llama3.2", 4_700_000_000),
            make_test_model("mistral", 7_000_000_000),
        ];
        let download_state = DownloadState::Idle;
        let tab = OllamaTab::new(&models, 1, &download_state);
        assert_eq!(tab.selected_model(), Some("mistral"));
    }

    #[test]
    fn test_ollama_tab_renders_models() {
        let models = vec![
            make_test_model("llama3.2", 4_700_000_000),
            make_test_model("mistral", 7_000_000_000),
        ];
        let download_state = DownloadState::Idle;
        let tab = OllamaTab::new(&models, 0, &download_state);

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 10));
        tab.render(Rect::new(0, 0, 60, 10), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("llama3.2"));
        assert!(content.contains("mistral"));
        assert!(content.contains("4.7 GB"));
    }

    #[test]
    fn test_ollama_tab_renders_empty_state() {
        let models: Vec<OllamaModelInfo> = vec![];
        let download_state = DownloadState::Idle;
        let tab = OllamaTab::new(&models, 0, &download_state);

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 10));
        tab.render(Rect::new(0, 0, 60, 10), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("No models installed"));
    }

    #[test]
    fn test_ollama_tab_renders_not_available() {
        let models: Vec<OllamaModelInfo> = vec![];
        let download_state = DownloadState::Idle;
        let tab = OllamaTab::new(&models, 0, &download_state).available(false);

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 10));
        tab.render(Rect::new(0, 0, 60, 10), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("not running"));
    }

    #[test]
    fn test_ollama_tab_renders_download_progress() {
        let models: Vec<OllamaModelInfo> = vec![];
        let download_state = DownloadState::Downloading {
            model: "phi3".to_string(),
            progress: 0.45,
            downloaded: 2_100_000_000,
            total: 4_700_000_000,
        };
        let tab = OllamaTab::new(&models, 0, &download_state);

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 10));
        tab.render(Rect::new(0, 0, 60, 10), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("phi3"));
    }

    #[test]
    fn test_ollama_tab_renders_complete_status() {
        let models: Vec<OllamaModelInfo> = vec![];
        let download_state = DownloadState::Complete {
            model: "phi3".to_string(),
        };
        let tab = OllamaTab::new(&models, 0, &download_state);

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 10));
        tab.render(Rect::new(0, 0, 60, 10), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("successfully"));
    }

    #[test]
    fn test_ollama_tab_renders_failed_status() {
        let models: Vec<OllamaModelInfo> = vec![];
        let download_state = DownloadState::Failed {
            model: "invalid".to_string(),
            error: "Not found".to_string(),
        };
        let tab = OllamaTab::new(&models, 0, &download_state);

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 10));
        tab.render(Rect::new(0, 0, 60, 10), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Failed"));
    }

    #[test]
    fn test_truncate_model_name() {
        assert_eq!(truncate_model_name("llama3.2", 20), "llama3.2");
        assert_eq!(
            truncate_model_name("very-long-model-name-here", 10),
            "very-long…"
        );
    }

    #[test]
    fn test_ollama_tab_small_area_no_panic() {
        let models: Vec<OllamaModelInfo> = vec![];
        let download_state = DownloadState::Idle;
        let tab = OllamaTab::new(&models, 0, &download_state);

        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 2));
        // Should not panic
        tab.render(Rect::new(0, 0, 20, 2), &mut buf);
    }

    #[test]
    fn test_ollama_tab_selection_highlight() {
        let models = vec![
            make_test_model("llama3.2", 4_700_000_000),
            make_test_model("mistral", 7_000_000_000),
        ];
        let download_state = DownloadState::Idle;
        let tab = OllamaTab::new(&models, 1, &download_state); // Select second model

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 10));
        tab.render(Rect::new(0, 0, 60, 10), &mut buf);

        // Should render without panic with selection on second item
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("mistral"));
    }
}
