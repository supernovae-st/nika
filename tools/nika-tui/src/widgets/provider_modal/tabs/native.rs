// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Native inference tab - Local model management via mistral.rs
//!

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget},
};

use super::super::state::{DownloadState, NativeModelInfo};

// =============================================================================
// SEMANTIC COLOR CONSTANTS
// =============================================================================

/// Dark background
const COLOR_BG_DARK: Color = Color::Rgb(17, 24, 39); // Gray-900

/// Header text (light blue)
const COLOR_HEADER: Color = Color::Rgb(147, 197, 253); // Blue-300

/// Selected item (amber)
const COLOR_SELECTED: Color = Color::Rgb(251, 191, 36); // Amber-400

/// Normal text
const COLOR_TEXT: Color = Color::Rgb(209, 213, 219); // Gray-300

/// Muted text
const COLOR_MUTED: Color = Color::Rgb(107, 114, 128); // Gray-500

/// Success color
const COLOR_SUCCESS: Color = Color::Rgb(74, 222, 128); // Green-400

/// Warning color
const COLOR_WARNING: Color = Color::Rgb(251, 191, 36); // Amber-400

/// Error color
const COLOR_ERROR: Color = Color::Rgb(248, 113, 113); // Red-400

/// Native tab widget for local model management
pub struct NativeTab<'a> {
    models: &'a [NativeModelInfo],
    selected_idx: usize,
    download_state: &'a DownloadState,
    available: bool,
}

impl<'a> NativeTab<'a> {
    /// Create new native tab
    pub fn new(
        models: &'a [NativeModelInfo],
        selected_idx: usize,
        download_state: &'a DownloadState,
    ) -> Self {
        Self {
            models,
            selected_idx,
            download_state,
            available: false,
        }
    }

    /// Set whether native inference is available
    pub fn available(mut self, available: bool) -> Self {
        self.available = available;
        self
    }

    /// Render the status header
    fn render_status_header(&self, area: Rect, buf: &mut Buffer) {
        let status_text = if self.available {
            Span::styled(
                "● Native inference available (mistral.rs)",
                Style::default().fg(COLOR_SUCCESS),
            )
        } else {
            Span::styled(
                "○ Native inference not available (compile with --features native-inference)",
                Style::default().fg(COLOR_MUTED),
            )
        };

        let header = Paragraph::new(Line::from(vec![
            Span::styled("🦙 ", Style::default()),
            status_text,
        ]));

        header.render(area, buf);
    }

    /// Render the model list
    fn render_model_list(&self, area: Rect, buf: &mut Buffer) {
        if self.models.is_empty() {
            // Show empty state
            let empty_msg = if self.available {
                vec![
                    Line::from(Span::styled(
                        "No models loaded",
                        Style::default().fg(COLOR_MUTED),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        "Download models with: nika model pull llama3.2:1b",
                        Style::default().fg(COLOR_TEXT),
                    )),
                ]
            } else {
                vec![
                    Line::from(Span::styled(
                        "Native inference requires --features native-inference",
                        Style::default().fg(COLOR_MUTED),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        "Rebuild with: cargo build --features native-inference",
                        Style::default().fg(COLOR_TEXT),
                    )),
                ]
            };

            let paragraph = Paragraph::new(empty_msg).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Models ")
                    .style(Style::default().bg(COLOR_BG_DARK)),
            );
            paragraph.render(area, buf);
            return;
        }

        // Render model list
        let items: Vec<ListItem> = self
            .models
            .iter()
            .enumerate()
            .map(|(idx, model)| {
                let is_selected = idx == self.selected_idx;
                let style = if is_selected {
                    Style::default()
                        .fg(COLOR_SELECTED)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(COLOR_TEXT)
                };

                let size_str = format_bytes(model.size);
                let family = model.details.family.as_deref().unwrap_or("unknown");
                let params = &model.details.parameter_size;
                let quant = &model.details.quantization_level;

                let line = Line::from(vec![
                    Span::styled(if is_selected { "► " } else { "  " }, style),
                    Span::styled(&model.name, style),
                    Span::styled(format!(" ({}) ", family), Style::default().fg(COLOR_MUTED)),
                    Span::styled(
                        format!("{} {} ", params, quant),
                        Style::default().fg(COLOR_MUTED),
                    ),
                    Span::styled(size_str, Style::default().fg(COLOR_MUTED)),
                ]);

                ListItem::new(line)
            })
            .collect();

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Models ({}) ", self.models.len()))
                .style(Style::default().bg(COLOR_BG_DARK)),
        );

        list.render(area, buf);
    }

    /// Render download progress if active
    fn render_download_progress(&self, area: Rect, buf: &mut Buffer) {
        if let DownloadState::Downloading {
            model,
            progress,
            downloaded,
            total,
        } = self.download_state
        {
            let percentage = (progress * 100.0) as u16;
            let progress_bar_width = (area.width.saturating_sub(30) as f64 * progress) as u16;

            let line = Line::from(vec![
                Span::styled("⬇ Downloading: ", Style::default().fg(COLOR_WARNING)),
                Span::styled(model, Style::default().fg(COLOR_TEXT)),
                Span::styled(
                    format!(" {}% ", percentage),
                    Style::default().fg(COLOR_HEADER),
                ),
                Span::styled("[", Style::default().fg(COLOR_MUTED)),
                Span::styled(
                    "█".repeat(progress_bar_width as usize),
                    Style::default().fg(COLOR_SUCCESS),
                ),
                Span::styled(
                    "░".repeat(
                        (area
                            .width
                            .saturating_sub(30)
                            .saturating_sub(progress_bar_width)) as usize,
                    ),
                    Style::default().fg(COLOR_MUTED),
                ),
                Span::styled("]", Style::default().fg(COLOR_MUTED)),
                Span::styled(
                    format!(" {}/{}", format_bytes(*downloaded), format_bytes(*total)),
                    Style::default().fg(COLOR_MUTED),
                ),
            ]);

            Paragraph::new(line).render(area, buf);
        } else if let DownloadState::Complete { model } = self.download_state {
            let line = Line::from(vec![
                Span::styled("✓ Downloaded: ", Style::default().fg(COLOR_SUCCESS)),
                Span::styled(model, Style::default().fg(COLOR_TEXT)),
            ]);
            Paragraph::new(line).render(area, buf);
        } else if let DownloadState::Failed { model, error } = self.download_state {
            let line = Line::from(vec![
                Span::styled("✗ Failed: ", Style::default().fg(COLOR_ERROR)),
                Span::styled(model, Style::default().fg(COLOR_TEXT)),
                Span::styled(format!(" - {}", error), Style::default().fg(COLOR_MUTED)),
            ]);
            Paragraph::new(line).render(area, buf);
        }
    }
}

impl Widget for NativeTab<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Layout: header (1) | models (fill) | download progress (1 if active)
        let has_download = !matches!(self.download_state, DownloadState::Idle);
        let constraints = if has_download {
            vec![
                Constraint::Length(1),
                Constraint::Min(3),
                Constraint::Length(1),
            ]
        } else {
            vec![Constraint::Length(1), Constraint::Min(3)]
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(area);

        self.render_status_header(chunks[0], buf);
        self.render_model_list(chunks[1], buf);

        if has_download && chunks.len() > 2 {
            self.render_download_progress(chunks[2], buf);
        }
    }
}

/// Format bytes as human-readable string
fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.1}GB", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.1}MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.1}KB", bytes as f64 / 1_000.0)
    } else {
        format!("{}B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::provider_modal::state::NativeModelDetails;

    #[test]
    fn test_native_tab_new() {
        let models = vec![];
        let download_state = DownloadState::Idle;
        let tab = NativeTab::new(&models, 0, &download_state);
        assert!(!tab.available);
        assert_eq!(tab.selected_idx, 0);
    }

    #[test]
    fn test_native_tab_available() {
        let models = vec![];
        let download_state = DownloadState::Idle;
        let tab = NativeTab::new(&models, 0, &download_state).available(true);
        assert!(tab.available);
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500B");
        assert_eq!(format_bytes(1500), "1.5KB");
        assert_eq!(format_bytes(1_500_000), "1.5MB");
        assert_eq!(format_bytes(1_500_000_000), "1.5GB");
    }

    #[test]
    fn test_native_tab_renders_without_panic() {
        let models = vec![NativeModelInfo {
            name: "llama3.2:1b".to_string(),
            size: 1_000_000_000,
            digest: "sha256:abc".to_string(),
            modified_at: "2026-01-01".to_string(),
            details: NativeModelDetails {
                parameter_size: "1B".to_string(),
                quantization_level: "Q4_0".to_string(),
                family: Some("llama".to_string()),
            },
        }];
        let download_state = DownloadState::Idle;
        let tab = NativeTab::new(&models, 0, &download_state).available(true);

        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 20));
        tab.render(Rect::new(0, 0, 80, 20), &mut buf);
        // Should not panic
    }

    #[test]
    fn test_native_tab_with_download_progress() {
        let models = vec![];
        let download_state = DownloadState::Downloading {
            model: "llama3.2".to_string(),
            progress: 0.5,
            downloaded: 500_000_000,
            total: 1_000_000_000,
        };
        let tab = NativeTab::new(&models, 0, &download_state);

        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 20));
        tab.render(Rect::new(0, 0, 80, 20), &mut buf);
        // Should not panic
    }
}
