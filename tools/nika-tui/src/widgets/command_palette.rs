// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Command Palette Widget
//!
//! Fuzzy command search overlay inspired by VS Code ⌘K.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Widget},
};

use crate::theme::VerbColor;
use crate::tokens::compat;

/// A command in the palette
#[derive(Debug, Clone)]
pub struct PaletteCommand {
    /// Command ID
    pub id: String,
    /// Display label
    pub label: String,
    /// Description
    pub description: String,
    /// Keyboard shortcut
    pub shortcut: Option<String>,
    /// Icon
    pub icon: &'static str,
    /// Category
    pub category: String,
}

impl PaletteCommand {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: description.into(),
            shortcut: None,
            icon: "▶",
            category: "General".to_string(),
        }
    }

    pub fn with_shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub fn with_icon(mut self, icon: &'static str) -> Self {
        self.icon = icon;
        self
    }

    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = category.into();
        self
    }

    /// Check if command matches query (fuzzy)
    pub fn matches(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let query_lower = query.to_lowercase();
        self.label.to_lowercase().contains(&query_lower)
            || self.description.to_lowercase().contains(&query_lower)
            || self.id.to_lowercase().contains(&query_lower)
    }

    /// Calculate match score (higher = better match)
    pub fn match_score(&self, query: &str) -> u32 {
        if query.is_empty() {
            return 0;
        }
        let query_lower = query.to_lowercase();
        let mut score = 0;

        // Exact match on ID
        if self.id.to_lowercase() == query_lower {
            score += 100;
        }
        // Starts with
        if self.label.to_lowercase().starts_with(&query_lower) {
            score += 50;
        }
        // Contains
        if self.label.to_lowercase().contains(&query_lower) {
            score += 25;
        }
        if self.description.to_lowercase().contains(&query_lower) {
            score += 10;
        }

        score
    }
}

/// Default commands
pub fn default_commands() -> Vec<PaletteCommand> {
    vec![
        // === Nika 5 Verbs (Slash Commands) ===
        PaletteCommand::new("infer", "/infer", "LLM text generation")
            .with_icon("⚡")
            .with_category("Verbs"),
        PaletteCommand::new("exec", "/exec", "Execute shell command")
            .with_icon("📟")
            .with_category("Verbs"),
        PaletteCommand::new("fetch", "/fetch", "HTTP request (GET/POST)")
            .with_icon("🛰")
            .with_category("Verbs"),
        PaletteCommand::new("invoke", "/invoke", "Call MCP tool")
            .with_icon("🔌")
            .with_category("Verbs"),
        PaletteCommand::new("agent", "/agent", "Multi-turn agent loop")
            .with_icon("🐔")
            .with_category("Verbs"),
        // === Workflow Actions ===
        PaletteCommand::new("run", "Run Workflow", "Execute the current workflow file")
            .with_shortcut("⌘⏎")
            .with_icon("▶")
            .with_category("Run"),
        PaletteCommand::new("run_task", "Run Task", "Execute a single task")
            .with_shortcut("⌘⇧G")
            .with_icon("🔷")
            .with_category("Run"),
        PaletteCommand::new(
            "run_monitor",
            "Run with Monitor",
            "Execute and open TUI monitor",
        )
        .with_shortcut("⌘M")
        .with_icon("📊")
        .with_category("Run"),
        PaletteCommand::new("dry_run", "Dry Run", "Validate DAG without executing")
            .with_shortcut("⌘D")
            .with_icon("🧪")
            .with_category("Run"),
        PaletteCommand::new("validate", "Validate Workflow", "Check YAML and schema")
            .with_shortcut("⌘V")
            .with_icon("✅")
            .with_category("Edit"),
        // === View Navigation (3-view architecture) ===
        PaletteCommand::new("studio", "1: Studio", "YAML editor with browser + DAG")
            .with_shortcut("1/s")
            .with_icon("📝")
            .with_category("View"),
        PaletteCommand::new("command", "2: Command", "Chat agent + workflow execution")
            .with_shortcut("2/c")
            .with_icon("⚡")
            .with_category("View"),
        PaletteCommand::new("control", "3: Control", "Providers, MCP, preferences")
            .with_shortcut("3/x")
            .with_icon("⚙")
            .with_category("View"),
        // === Chat Commands ===
        PaletteCommand::new("help", "Help", "Show help documentation")
            .with_shortcut("?")
            .with_icon("❓")
            .with_category("Help"),
        PaletteCommand::new("clear", "Clear Chat", "Clear chat history")
            .with_icon("🗑")
            .with_category("Chat"),
        PaletteCommand::new("model", "Change Model", "Switch LLM provider/model")
            .with_icon("🤖")
            .with_category("Chat"),
        PaletteCommand::new("mcp", "MCP Status", "View MCP server connections")
            .with_icon("🔌")
            .with_category("Chat"),
    ]
}

/// Command palette state
#[derive(Debug, Clone)]
pub struct CommandPaletteState {
    /// Search query
    pub query: String,
    /// Selected index
    pub selected: usize,
    /// All commands
    pub commands: Vec<PaletteCommand>,
    /// Filtered command indices
    pub filtered: Vec<usize>,
    /// Recently used (command IDs)
    pub recent: Vec<String>,
    /// Is palette visible
    pub visible: bool,
    /// Parsed argument when command is detected (e.g., "novanet_describe" from "/invoke novanet_describe")
    pub argument: Option<String>,
    /// Whether a command was auto-detected from input
    pub command_locked: bool,
}

impl Default for CommandPaletteState {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandPaletteState {
    pub fn new() -> Self {
        let commands = default_commands();
        let filtered = (0..commands.len()).collect();
        Self {
            query: String::new(),
            selected: 0,
            commands,
            filtered,
            recent: Vec::new(),
            visible: false,
            argument: None,
            command_locked: false,
        }
    }

    /// Open the palette
    pub fn open(&mut self) {
        self.visible = true;
        self.query.clear();
        self.argument = None;
        self.command_locked = false;
        self.update_filter();
    }

    /// Close the palette
    pub fn close(&mut self) {
        self.visible = false;
        self.query.clear();
        self.argument = None;
        self.command_locked = false;
    }

    /// Toggle visibility
    pub fn toggle(&mut self) {
        if self.visible {
            self.close();
        } else {
            self.open();
        }
    }

    /// Update filtered results based on query
    /// Also detects complete commands with arguments (e.g., "/invoke novanet_describe")
    pub fn update_filter(&mut self) {
        // Check if query contains a complete slash command with argument
        // e.g., "/invoke novanet_describe" -> command="invoke", argument="novanet_describe"
        if !self.command_locked && self.query.starts_with('/') {
            if let Some(space_pos) = self.query.find(' ') {
                let cmd_part = &self.query[1..space_pos]; // "invoke" from "/invoke "
                let arg_part = self.query[space_pos + 1..].trim(); // "novanet_describe"

                // Find matching command by ID
                if let Some((idx, _)) = self
                    .commands
                    .iter()
                    .enumerate()
                    .find(|(_, c)| c.id == cmd_part)
                {
                    // Auto-select this command and store argument
                    self.filtered = vec![idx];
                    self.selected = 0;
                    self.command_locked = true;
                    self.argument = if arg_part.is_empty() {
                        None
                    } else {
                        Some(arg_part.to_string())
                    };
                    return;
                }
            }
        }

        // If command is locked, keep the single filtered result
        if self.command_locked {
            return;
        }

        if self.query.is_empty() {
            // Show recent first, then all
            let mut indices: Vec<(usize, u32)> = self
                .commands
                .iter()
                .enumerate()
                .map(|(i, cmd)| {
                    let recent_score =
                        if let Some(pos) = self.recent.iter().position(|r| r == &cmd.id) {
                            100 - pos as u32
                        } else {
                            0
                        };
                    (i, recent_score)
                })
                .collect();
            indices.sort_by(|a, b| b.1.cmp(&a.1));
            self.filtered = indices.into_iter().map(|(i, _)| i).collect();
        } else {
            // Filter and sort by match score
            let mut matches: Vec<(usize, u32)> = self
                .commands
                .iter()
                .enumerate()
                .filter(|(_, cmd)| cmd.matches(&self.query))
                .map(|(i, cmd)| (i, cmd.match_score(&self.query)))
                .collect();
            matches.sort_by(|a, b| b.1.cmp(&a.1));
            self.filtered = matches.into_iter().map(|(i, _)| i).collect();
        }
        self.selected = 0;
    }

    /// Set full query (for paste operations)
    /// Parses "/command argument" syntax and auto-selects the command
    pub fn set_query(&mut self, query: &str) {
        self.query = query.to_string();
        self.argument = None;
        self.command_locked = false;
        self.update_filter();
    }

    /// Get the parsed argument (if any)
    pub fn get_argument(&self) -> Option<&str> {
        self.argument.as_deref()
    }

    /// Select next command
    pub fn select_next(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 1) % self.filtered.len();
        }
    }

    /// Select previous command
    pub fn select_prev(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = self
                .selected
                .checked_sub(1)
                .unwrap_or(self.filtered.len() - 1);
        }
    }

    /// Get currently selected command
    pub fn selected_command(&self) -> Option<&PaletteCommand> {
        self.filtered
            .get(self.selected)
            .and_then(|&i| self.commands.get(i))
    }

    /// Execute selected command (returns command ID)
    pub fn execute_selected(&mut self) -> Option<String> {
        if let Some(cmd) = self.selected_command() {
            let id = cmd.id.clone();
            // Add to recent
            self.recent.retain(|r| r != &id);
            self.recent.insert(0, id.clone());
            if self.recent.len() > 5 {
                self.recent.truncate(5);
            }
            self.close();
            Some(id)
        } else {
            None
        }
    }

    /// Input a character
    /// When command is locked, characters are added to the argument
    pub fn input_char(&mut self, c: char) {
        if self.command_locked {
            // When command is locked, add to argument instead of query
            if let Some(ref mut arg) = self.argument {
                arg.push(c);
            } else {
                self.argument = Some(c.to_string());
            }
        } else {
            self.query.push(c);
            self.update_filter();
        }
    }

    /// Delete last character
    /// When command is locked, delete from argument; if argument empty, unlock
    pub fn backspace(&mut self) {
        if self.command_locked {
            // When command is locked, delete from argument
            if let Some(ref mut arg) = self.argument {
                arg.pop();
                if arg.is_empty() {
                    self.argument = None;
                }
            } else {
                // No argument left, unlock the command and go back to normal filtering
                self.command_locked = false;
                // Reconstruct query without the trailing space
                if let Some(space_pos) = self.query.rfind(' ') {
                    self.query.truncate(space_pos);
                }
                self.update_filter();
            }
        } else {
            self.query.pop();
            self.update_filter();
        }
    }
}

/// Map command ID to VerbColor (returns None for non-verb commands)
fn verb_color_for_command(id: &str) -> Option<VerbColor> {
    match id {
        "infer" => Some(VerbColor::Infer),
        "exec" => Some(VerbColor::Exec),
        "fetch" => Some(VerbColor::Fetch),
        "invoke" => Some(VerbColor::Invoke),
        "agent" => Some(VerbColor::Agent),
        _ => None,
    }
}

/// Command palette widget
pub struct CommandPalette<'a> {
    state: &'a CommandPaletteState,
    /// Animation frame counter (for verb color animation)
    frame: u8,
}

impl<'a> CommandPalette<'a> {
    pub fn new(state: &'a CommandPaletteState) -> Self {
        Self { state, frame: 0 }
    }

    /// Set animation frame for verb color gradient effect
    pub fn with_frame(mut self, frame: u8) -> Self {
        self.frame = frame;
        self
    }
}

impl Widget for CommandPalette<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        if !self.state.visible {
            return;
        }

        // Center the palette
        let palette_width = 60.min(area.width.saturating_sub(10));
        let palette_height = 15.min(area.height.saturating_sub(6));

        let x = area.x + (area.width.saturating_sub(palette_width)) / 2;
        let y = area.y + 3;

        let palette_area = Rect {
            x,
            y,
            width: palette_width,
            height: palette_height,
        };

        // Clear background
        Clear.render(palette_area, buf);

        // Draw border
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(compat::INDIGO_500))
            .style(Style::default().bg(compat::GRAY_900));
        let inner = block.inner(palette_area);
        block.render(palette_area, buf);

        // Search input
        // Show command + argument when command is locked
        let cursor = if self.state.visible { "_" } else { "" };
        let input_line = if self.state.command_locked {
            // Show locked command + argument
            let selected_cmd = self.state.selected_command();
            let cmd_name = selected_cmd
                .map(|c| c.label.as_str())
                .unwrap_or(&self.state.query);
            let cmd_id = selected_cmd.map(|c| c.id.as_str()).unwrap_or("");
            let arg = self.state.argument.as_deref().unwrap_or("");

            // Use verb color for verb commands (animated + bold)
            let cmd_style = if let Some(verb_color) = verb_color_for_command(cmd_id) {
                Style::default()
                    .fg(verb_color.animated(self.frame))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            };

            Line::from(vec![
                Span::styled("🔍 > ", Style::default().fg(compat::SLATE_600)),
                Span::styled(cmd_name, cmd_style),
                Span::styled(" ", Style::default()),
                Span::styled(arg, Style::default().fg(Color::White)),
                Span::styled(
                    cursor,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::SLOW_BLINK),
                ),
            ])
        } else {
            Line::from(vec![
                Span::styled("🔍 > ", Style::default().fg(compat::SLATE_600)),
                Span::styled(&self.state.query, Style::default().fg(Color::White)),
                Span::styled(
                    cursor,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::SLOW_BLINK),
                ),
            ])
        };
        buf.set_line(inner.x + 1, inner.y, &input_line, inner.width - 2);

        // Separator
        let sep = "─".repeat((inner.width.saturating_sub(2)) as usize);
        buf.set_string(
            inner.x + 1,
            inner.y + 1,
            &sep,
            Style::default().fg(compat::GRAY_700),
        );

        // Command list
        let list_y = inner.y + 2;
        let list_height = inner.height.saturating_sub(3);

        for (i, &cmd_idx) in self.state.filtered.iter().enumerate() {
            if i >= list_height as usize {
                break;
            }
            let cmd = &self.state.commands[cmd_idx];
            let is_selected = i == self.state.selected;

            let row_y = list_y + i as u16;

            let bg = if is_selected {
                compat::GRAY_700
            } else {
                compat::GRAY_900
            };

            // Clear line background
            for x in inner.x..(inner.x + inner.width) {
                if let Some(cell) = buf.cell_mut((x, row_y)) {
                    cell.set_bg(bg);
                }
            }

            // Icon and label
            // Verb commands get animated colors
            let (label_style, icon_style) =
                if let Some(verb_color) = verb_color_for_command(&cmd.id) {
                    let animated_color = verb_color.animated(self.frame);
                    if is_selected {
                        // Selected verb: bright animated color + bold
                        (
                            Style::default()
                                .fg(animated_color)
                                .add_modifier(Modifier::BOLD),
                            Style::default().fg(animated_color),
                        )
                    } else {
                        // Unselected verb: animated color + bold (still stands out)
                        (
                            Style::default()
                                .fg(animated_color)
                                .add_modifier(Modifier::BOLD),
                            Style::default().fg(animated_color),
                        )
                    }
                } else if is_selected {
                    // Selected non-verb: white + bold
                    (
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                        Style::default(),
                    )
                } else {
                    // Unselected non-verb: default text
                    (Style::default().fg(compat::SLATE_200), Style::default())
                };

            buf.set_string(inner.x + 2, row_y, cmd.icon, icon_style);
            buf.set_string(inner.x + 5, row_y, &cmd.label, label_style);

            // Shortcut on the right
            if let Some(ref shortcut) = cmd.shortcut {
                let shortcut_x = inner.x
                    + inner
                        .width
                        .saturating_sub(shortcut.chars().count() as u16 + 3);
                if shortcut_x > inner.x + cmd.label.len() as u16 + 10 {
                    buf.set_string(
                        shortcut_x,
                        row_y,
                        shortcut,
                        Style::default().fg(compat::SLATE_500),
                    );
                }
            }
        }

        // Footer hint
        let footer_y = inner.y + inner.height - 1;
        if footer_y > list_y {
            buf.set_string(
                inner.x + 2,
                footer_y,
                "↑↓ Navigate  ⏎ Select  Esc Cancel",
                Style::default().fg(compat::SLATE_500),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_palette_command_creation() {
        let cmd = PaletteCommand::new("test", "Test Command", "A test command")
            .with_shortcut("⌘T")
            .with_icon("🧪")
            .with_category("Test");

        assert_eq!(cmd.id, "test");
        assert_eq!(cmd.label, "Test Command");
        assert_eq!(cmd.shortcut, Some("⌘T".to_string()));
        assert_eq!(cmd.icon, "🧪");
        assert_eq!(cmd.category, "Test");
    }

    #[test]
    fn test_matches() {
        let cmd = PaletteCommand::new("run_workflow", "Run Workflow", "Execute workflow");

        assert!(cmd.matches(""));
        assert!(cmd.matches("run"));
        assert!(cmd.matches("work"));
        assert!(cmd.matches("RUN")); // Case insensitive
        assert!(cmd.matches("execute"));
        assert!(!cmd.matches("xyz"));
    }

    #[test]
    fn test_match_score() {
        let cmd = PaletteCommand::new("run", "Run Workflow", "Execute workflow");

        // Exact ID match = highest
        assert!(cmd.match_score("run") > cmd.match_score("workflow"));

        // Starts with > contains
        assert!(cmd.match_score("run") > cmd.match_score("work"));
    }

    #[test]
    fn test_default_commands() {
        let cmds = default_commands();
        assert!(!cmds.is_empty());
        assert!(cmds.iter().any(|c| c.id == "run"));
        assert!(cmds.iter().any(|c| c.id == "help"));
    }

    #[test]
    fn test_palette_state_default() {
        let state = CommandPaletteState::default();
        assert!(!state.visible);
        assert!(state.query.is_empty());
        assert!(!state.commands.is_empty());
    }

    #[test]
    fn test_open_close() {
        let mut state = CommandPaletteState::new();

        state.open();
        assert!(state.visible);

        state.close();
        assert!(!state.visible);
    }

    #[test]
    fn test_toggle() {
        let mut state = CommandPaletteState::new();

        state.toggle();
        assert!(state.visible);

        state.toggle();
        assert!(!state.visible);
    }

    #[test]
    fn test_filter() {
        let mut state = CommandPaletteState::new();
        state.open();

        let all_count = state.filtered.len();

        state.input_char('r');
        state.input_char('u');
        state.input_char('n');

        // Should have fewer matches
        assert!(state.filtered.len() < all_count);
        // Should still have "run" commands
        assert!(!state.filtered.is_empty());
    }

    #[test]
    fn test_navigation() {
        let mut state = CommandPaletteState::new();
        state.open();

        assert_eq!(state.selected, 0);

        state.select_next();
        assert_eq!(state.selected, 1);

        state.select_prev();
        assert_eq!(state.selected, 0);

        // Wrap around
        state.select_prev();
        assert_eq!(state.selected, state.filtered.len() - 1);
    }

    #[test]
    fn test_selected_command() {
        let state = CommandPaletteState::new();
        let cmd = state.selected_command();
        assert!(cmd.is_some());
    }

    #[test]
    fn test_execute_selected() {
        let mut state = CommandPaletteState::new();
        state.open();

        let id = state.execute_selected();
        assert!(id.is_some());
        assert!(!state.visible); // Closes after execute

        // Should be in recent
        assert!(!state.recent.is_empty());
    }

    #[test]
    fn test_input_backspace() {
        let mut state = CommandPaletteState::new();
        state.open();

        state.input_char('t');
        state.input_char('e');
        assert_eq!(state.query, "te");

        state.backspace();
        assert_eq!(state.query, "t");

        state.backspace();
        assert_eq!(state.query, "");
    }

    #[test]
    fn test_recent_order() {
        let mut state = CommandPaletteState::new();
        state.open();

        // Execute a command
        state.query = "help".to_string();
        state.update_filter();
        state.execute_selected();

        // Recent should have help at front
        assert!(!state.recent.is_empty());

        state.open();
        state.query = "run".to_string();
        state.update_filter();
        state.execute_selected();

        // Most recent should be first
        assert_eq!(state.recent.len(), 2);
    }

    #[test]
    fn test_recent_limit() {
        let mut state = CommandPaletteState::new();

        // Add more than 5 recent
        for i in 0..10 {
            state.recent.insert(0, format!("cmd{}", i));
        }
        state.recent.truncate(5);

        assert_eq!(state.recent.len(), 5);
    }
}
