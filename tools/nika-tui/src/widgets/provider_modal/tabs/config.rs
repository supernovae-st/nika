// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! ConfigTab - Configuration preferences for Provider Modal v2
//!
//! Displays editable settings like default provider, model, theme, etc.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

// =============================================================================
// SEMANTIC COLOR CONSTANTS
// =============================================================================

/// Label text
const COLOR_LABEL: Color = Color::Rgb(156, 163, 175); // Gray-400

/// Editable value
const COLOR_VALUE: Color = Color::Rgb(129, 140, 248); // Indigo-400

/// Read-only value
const COLOR_VALUE_READONLY: Color = Color::Rgb(107, 114, 128); // Gray-500

/// Selected row background
const COLOR_BG_SELECTED: Color = Color::Rgb(55, 65, 81); // Gray-700

/// Selected label text
const COLOR_LABEL_SELECTED: Color = Color::Rgb(229, 231, 235); // Gray-200

/// Description text
const COLOR_DESCRIPTION: Color = Color::Rgb(107, 114, 128); // Gray-500

/// Configuration entry for display
#[derive(Debug, Clone)]
pub struct ConfigEntry {
    /// Setting label
    pub label: &'static str,
    /// Current value
    pub value: String,
    /// Description/help text
    pub description: &'static str,
    /// Whether this setting is editable
    pub editable: bool,
}

impl ConfigEntry {
    /// Create a new config entry
    pub fn new(label: &'static str, value: impl Into<String>, description: &'static str) -> Self {
        Self {
            label,
            value: value.into(),
            description,
            editable: true,
        }
    }

    /// Create a read-only entry
    pub fn readonly(
        label: &'static str,
        value: impl Into<String>,
        description: &'static str,
    ) -> Self {
        Self {
            label,
            value: value.into(),
            description,
            editable: false,
        }
    }
}

/// Config tab widget
pub struct ConfigTab {
    entries: Vec<ConfigEntry>,
    selected_idx: usize,
}

impl ConfigTab {
    /// Create new ConfigTab with default settings
    pub fn new(selected_idx: usize) -> Self {
        let entries = vec![
            ConfigEntry::new(
                "Default Provider",
                "Claude",
                "Primary provider for infer: tasks",
            ),
            ConfigEntry::new("Default Model", "claude-sonnet-4-6", "Model for new tasks"),
            ConfigEntry::new("Theme", "Solarized Dark", "TUI color theme"),
            ConfigEntry::new(
                "Auto-save Sessions",
                "Enabled",
                "Persist editor state on exit",
            ),
            ConfigEntry::new("MCP Timeout", "30s", "Timeout for MCP operations"),
            ConfigEntry::readonly("Config Path", "nika.toml", "Configuration file location"),
        ];
        Self {
            entries,
            selected_idx,
        }
    }

    /// Create with custom entries
    pub fn with_entries(entries: Vec<ConfigEntry>, selected_idx: usize) -> Self {
        Self {
            entries,
            selected_idx,
        }
    }

    /// Get item count for navigation
    pub fn item_count(&self) -> usize {
        self.entries.len()
    }

    /// Get selected entry
    pub fn selected_entry(&self) -> Option<&ConfigEntry> {
        self.entries.get(self.selected_idx)
    }
}

impl Widget for ConfigTab {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 30 {
            return;
        }

        // Styles
        let label_style = Style::default().fg(COLOR_LABEL);
        let value_style = Style::default()
            .fg(COLOR_VALUE)
            .add_modifier(Modifier::BOLD);
        let readonly_value_style = Style::default().fg(COLOR_VALUE_READONLY);
        let selected_bg = Style::default().bg(COLOR_BG_SELECTED);
        let selected_label = Style::default()
            .fg(COLOR_LABEL_SELECTED)
            .bg(COLOR_BG_SELECTED);
        let desc_style = Style::default().fg(COLOR_DESCRIPTION);

        let mut y = area.y;
        let value_col = area.x + 22; // Column where values start

        for (idx, entry) in self.entries.iter().enumerate() {
            if y >= area.y + area.height - 1 {
                break;
            }

            let is_selected = idx == self.selected_idx;

            // Highlight entire row if selected
            if is_selected {
                for x in area.x..area.x + area.width {
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_style(selected_bg);
                    }
                }
            }

            // Selection indicator
            let indicator = if is_selected { "▸" } else { " " };
            buf.set_string(
                area.x + 1,
                y,
                indicator,
                if is_selected {
                    selected_label
                } else {
                    label_style
                },
            );

            // Label
            buf.set_string(
                area.x + 3,
                y,
                entry.label,
                if is_selected {
                    selected_label
                } else {
                    label_style
                },
            );

            // Value
            let val_style = if !entry.editable {
                readonly_value_style
            } else if is_selected {
                selected_label.add_modifier(Modifier::BOLD)
            } else {
                value_style
            };

            // Editable indicator
            let value_display = if entry.editable && is_selected {
                format!("[{}]", entry.value)
            } else {
                entry.value.clone()
            };
            buf.set_string(value_col, y, &value_display, val_style);

            y += 1;

            // Description on next line
            if y < area.y + area.height {
                let desc = format!("   └─ {}", entry.description);
                buf.set_string(area.x + 1, y, &desc, desc_style);
                y += 1;
            }

            // Spacing between entries
            if y < area.y + area.height {
                y += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_tab_new_has_default_entries() {
        let tab = ConfigTab::new(0);
        assert_eq!(tab.item_count(), 6);
    }

    #[test]
    fn test_config_tab_selected_entry() {
        let tab = ConfigTab::new(0);
        let entry = tab.selected_entry().unwrap();
        assert_eq!(entry.label, "Default Provider");
    }

    #[test]
    fn test_config_tab_selected_entry_at_index() {
        let tab = ConfigTab::new(2);
        let entry = tab.selected_entry().unwrap();
        assert_eq!(entry.label, "Theme");
    }

    #[test]
    fn test_config_tab_renders_settings() {
        let tab = ConfigTab::new(0);
        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 20));
        tab.render(Rect::new(0, 0, 60, 20), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Default Provider"));
        assert!(content.contains("Claude"));
    }

    #[test]
    fn test_config_tab_renders_description() {
        let tab = ConfigTab::new(0);
        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 20));
        tab.render(Rect::new(0, 0, 60, 20), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("infer:"));
    }

    #[test]
    fn test_config_tab_renders_theme_setting() {
        let tab = ConfigTab::new(2);
        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 20));
        tab.render(Rect::new(0, 0, 60, 20), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Theme"));
        assert!(content.contains("Solarized"));
    }

    #[test]
    fn test_config_entry_new() {
        let entry = ConfigEntry::new("Test", "Value", "Description");
        assert_eq!(entry.label, "Test");
        assert_eq!(entry.value, "Value");
        assert_eq!(entry.description, "Description");
        assert!(entry.editable);
    }

    #[test]
    fn test_config_entry_readonly() {
        let entry = ConfigEntry::readonly("Path", "/home", "Location");
        assert!(!entry.editable);
    }

    #[test]
    fn test_config_tab_with_entries() {
        let entries = vec![
            ConfigEntry::new("A", "1", "First"),
            ConfigEntry::new("B", "2", "Second"),
        ];
        let tab = ConfigTab::with_entries(entries, 1);
        assert_eq!(tab.item_count(), 2);
        assert_eq!(tab.selected_entry().unwrap().label, "B");
    }

    #[test]
    fn test_config_tab_handles_small_area() {
        let tab = ConfigTab::new(0);
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 2));
        // Should not panic with small area
        tab.render(Rect::new(0, 0, 10, 2), &mut buf);
    }

    #[test]
    fn test_config_tab_readonly_entry_in_list() {
        let tab = ConfigTab::new(5); // Config Path is index 5
        let entry = tab.selected_entry().unwrap();
        assert_eq!(entry.label, "Config Path");
        assert!(!entry.editable);
    }
}
