// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Mention System Widget - @ mention autocomplete for Chat
//!
//! Supports referencing context items in chat input:
//! - `@file.rs` - Local files
//! - `@entity:qr-code` - NovaNet entities
//! - `@locale:fr-FR` - Locale context
//! - `@project:qrcode-ai` - Project context
//! - `@term:keyword` - Knowledge terms
//!
//! Features:
//! - Autocomplete popup with fuzzy matching
//! - Loading states for async resolution
//! - Integration with Mission Control panel

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Widget},
};

use crate::tokens::compat;

const COLOR_FILE: Color = compat::BLUE_500;
const COLOR_ENTITY: Color = compat::VIOLET_500;
const COLOR_LOCALE: Color = compat::CYAN_500;
const COLOR_PROJECT: Color = compat::GREEN_500;
const COLOR_TERM: Color = compat::AMBER_500;
const COLOR_MUTED: Color = compat::SLATE_500;
const COLOR_SELECTED: Color = compat::AMBER_400;
const COLOR_BORDER: Color = compat::SLATE_700;

/// Mention type categories
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MentionType {
    File,    // @file.rs
    Entity,  // @entity:qr-code
    Locale,  // @locale:fr-FR
    Project, // @project:qrcode-ai
    Term,    // @term:keyword
}

impl MentionType {
    pub fn icon(&self) -> &'static str {
        match self {
            MentionType::File => "📄",
            MentionType::Entity => "🔷",
            MentionType::Locale => "🌍",
            MentionType::Project => "📁",
            MentionType::Term => "📝",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            MentionType::File => COLOR_FILE,
            MentionType::Entity => COLOR_ENTITY,
            MentionType::Locale => COLOR_LOCALE,
            MentionType::Project => COLOR_PROJECT,
            MentionType::Term => COLOR_TERM,
        }
    }

    pub fn prefix(&self) -> &'static str {
        match self {
            MentionType::File => "@",
            MentionType::Entity => "@entity:",
            MentionType::Locale => "@locale:",
            MentionType::Project => "@project:",
            MentionType::Term => "@term:",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            MentionType::File => "file",
            MentionType::Entity => "entity",
            MentionType::Locale => "locale",
            MentionType::Project => "project",
            MentionType::Term => "term",
        }
    }
}

/// A parsed mention from input
#[derive(Debug, Clone)]
pub struct Mention {
    /// The type of mention
    pub mention_type: MentionType,
    /// The value after the prefix (e.g., "qr-code" for @entity:qr-code)
    pub value: String,
    /// The full mention string (e.g., "@entity:qr-code")
    pub full: String,
    /// Start position in input
    pub start: usize,
    /// End position in input
    pub end: usize,
}

impl Mention {
    /// Parse a mention from a string at a given position
    pub fn parse(input: &str, start: usize) -> Option<Mention> {
        let substring = &input[start..];

        if !substring.starts_with('@') {
            return None;
        }

        // Find the end of the mention (space or end of string)
        let end_offset = substring
            .char_indices()
            .skip(1) // Skip the @
            .find(|(_, c)| c.is_whitespace())
            .map(|(i, _)| i)
            .unwrap_or(substring.len());

        let mention_str = &substring[..end_offset];

        // Determine type and value
        let (mention_type, value) = if let Some(rest) = mention_str.strip_prefix("@entity:") {
            (MentionType::Entity, rest.to_string())
        } else if let Some(rest) = mention_str.strip_prefix("@locale:") {
            (MentionType::Locale, rest.to_string())
        } else if let Some(rest) = mention_str.strip_prefix("@project:") {
            (MentionType::Project, rest.to_string())
        } else if let Some(rest) = mention_str.strip_prefix("@term:") {
            (MentionType::Term, rest.to_string())
        } else if let Some(rest) = mention_str.strip_prefix('@') {
            // Assume file if no prefix
            (MentionType::File, rest.to_string())
        } else {
            return None;
        };

        Some(Mention {
            mention_type,
            value,
            full: mention_str.to_string(),
            start,
            end: start + end_offset,
        })
    }

    /// Find all mentions in input text
    pub fn find_all(input: &str) -> Vec<Mention> {
        let mut mentions = Vec::new();
        let mut pos = 0;

        for (i, c) in input.char_indices() {
            if c == '@' && i >= pos {
                if let Some(mention) = Mention::parse(input, i) {
                    pos = mention.end;
                    mentions.push(mention);
                }
            }
        }

        mentions
    }

    /// Check if cursor is inside or right after a mention trigger
    pub fn at_trigger_position(input: &str, cursor: usize) -> Option<MentionTrigger> {
        if cursor == 0 {
            return None;
        }

        // Look backwards for @
        let before_cursor = &input[..cursor];
        let last_at = before_cursor.rfind('@')?;

        // Check if there's a space between @ and cursor
        let between = &input[last_at..cursor];
        if between.contains(' ') {
            return None;
        }

        // Parse the partial mention
        let partial = between.to_string();

        // Determine what type we're completing
        let (trigger_type, query) = if partial.starts_with("@entity:") {
            (
                Some(MentionType::Entity),
                partial.strip_prefix("@entity:").unwrap_or("").to_string(),
            )
        } else if partial.starts_with("@locale:") {
            (
                Some(MentionType::Locale),
                partial.strip_prefix("@locale:").unwrap_or("").to_string(),
            )
        } else if partial.starts_with("@project:") {
            (
                Some(MentionType::Project),
                partial.strip_prefix("@project:").unwrap_or("").to_string(),
            )
        } else if partial.starts_with("@term:") {
            (
                Some(MentionType::Term),
                partial.strip_prefix("@term:").unwrap_or("").to_string(),
            )
        } else {
            // Could be file or type selection
            (None, partial.strip_prefix('@').unwrap_or("").to_string())
        };

        Some(MentionTrigger {
            start: last_at,
            partial,
            trigger_type,
            query,
        })
    }
}

/// Active mention trigger (user is typing a mention)
#[derive(Debug, Clone)]
pub struct MentionTrigger {
    /// Start position of the @ in input
    pub start: usize,
    /// The partial mention text (including @)
    pub partial: String,
    /// The type if determined (e.g., @entity: prefix typed)
    pub trigger_type: Option<MentionType>,
    /// The query part for autocomplete
    pub query: String,
}

/// Autocomplete suggestion
#[derive(Debug, Clone)]
pub struct MentionSuggestion {
    /// Display text
    pub display: String,
    /// Text to insert
    pub insert: String,
    /// Type of mention
    pub mention_type: MentionType,
    /// Description/preview
    pub description: Option<String>,
}

impl MentionSuggestion {
    pub fn file(path: &str) -> Self {
        Self {
            display: path.to_string(),
            insert: format!("@{}", path),
            mention_type: MentionType::File,
            description: None,
        }
    }

    pub fn entity(key: &str, display_name: Option<&str>) -> Self {
        Self {
            display: display_name.unwrap_or(key).to_string(),
            insert: format!("@entity:{}", key),
            mention_type: MentionType::Entity,
            description: Some(format!("Entity: {}", key)),
        }
    }

    pub fn locale(code: &str, name: Option<&str>) -> Self {
        Self {
            display: name.unwrap_or(code).to_string(),
            insert: format!("@locale:{}", code),
            mention_type: MentionType::Locale,
            description: Some(format!("Locale: {}", code)),
        }
    }

    pub fn project(key: &str, name: Option<&str>) -> Self {
        Self {
            display: name.unwrap_or(key).to_string(),
            insert: format!("@project:{}", key),
            mention_type: MentionType::Project,
            description: Some(format!("Project: {}", key)),
        }
    }

    pub fn term(term: &str) -> Self {
        Self {
            display: term.to_string(),
            insert: format!("@term:{}", term),
            mention_type: MentionType::Term,
            description: None,
        }
    }
}

/// State for the autocomplete popup
#[derive(Debug, Clone, Default)]
pub struct MentionAutocompleteState {
    /// Current suggestions
    pub suggestions: Vec<MentionSuggestion>,
    /// Selected index
    pub selected: usize,
    /// Is the popup visible
    pub visible: bool,
    /// The trigger that opened autocomplete
    pub trigger: Option<MentionTrigger>,
}

impl MentionAutocompleteState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Show autocomplete with suggestions
    pub fn show(&mut self, trigger: MentionTrigger, suggestions: Vec<MentionSuggestion>) {
        self.trigger = Some(trigger);
        self.suggestions = suggestions;
        self.selected = 0;
        self.visible = !self.suggestions.is_empty();
    }

    /// Hide autocomplete
    pub fn hide(&mut self) {
        self.visible = false;
        self.trigger = None;
        self.suggestions.clear();
        self.selected = 0;
    }

    /// Select next suggestion
    pub fn next(&mut self) {
        if !self.suggestions.is_empty() {
            self.selected = (self.selected + 1) % self.suggestions.len();
        }
    }

    /// Select previous suggestion
    pub fn prev(&mut self) {
        if !self.suggestions.is_empty() {
            self.selected = self
                .selected
                .checked_sub(1)
                .unwrap_or(self.suggestions.len().saturating_sub(1));
        }
    }

    /// Get the currently selected suggestion
    pub fn current(&self) -> Option<&MentionSuggestion> {
        self.suggestions.get(self.selected)
    }
}

/// Autocomplete popup widget
pub struct MentionAutocomplete<'a> {
    state: &'a MentionAutocompleteState,
    max_items: usize,
}

impl<'a> MentionAutocomplete<'a> {
    pub fn new(state: &'a MentionAutocompleteState) -> Self {
        Self {
            state,
            max_items: 8,
        }
    }

    pub fn max_items(mut self, max: usize) -> Self {
        self.max_items = max;
        self
    }
}

impl Widget for MentionAutocomplete<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.state.visible || self.state.suggestions.is_empty() {
            return;
        }

        let visible_count = self.state.suggestions.len().min(self.max_items);
        let popup_height = (visible_count + 2) as u16; // +2 for borders
        let popup_width = area.width.min(50);

        // Position popup above input (if space) or below
        let popup_y = if area.y >= popup_height {
            area.y.saturating_sub(popup_height)
        } else {
            area.y + area.height
        };

        let popup_area = Rect {
            x: area.x,
            y: popup_y,
            width: popup_width,
            height: popup_height,
        };

        // Clear background
        Clear.render(popup_area, buf);

        // Build list items
        let items: Vec<ListItem> = self
            .state
            .suggestions
            .iter()
            .take(self.max_items)
            .enumerate()
            .map(|(i, suggestion)| {
                let is_selected = i == self.state.selected;
                let style = if is_selected {
                    Style::default().fg(Color::Black).bg(COLOR_SELECTED)
                } else {
                    Style::default().fg(Color::White)
                };

                let line = Line::from(vec![
                    Span::raw(suggestion.mention_type.icon()),
                    Span::raw(" "),
                    Span::styled(&suggestion.display, style),
                    if let Some(desc) = &suggestion.description {
                        Span::styled(format!(" - {}", desc), Style::default().fg(COLOR_MUTED))
                    } else {
                        Span::raw("")
                    },
                ]);

                ListItem::new(line).style(style)
            })
            .collect();

        let block = Block::default()
            .title(" @ Mentions ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(COLOR_BORDER));

        let list = List::new(items).block(block);
        list.render(popup_area, buf);
    }
}

/// Helper to highlight mentions in text
pub fn highlight_mentions<'a>(text: &'a str) -> Vec<Span<'a>> {
    let mentions = Mention::find_all(text);
    if mentions.is_empty() {
        return vec![Span::raw(text)];
    }

    let mut spans = Vec::new();
    let mut last_end = 0;

    for mention in mentions {
        // Text before mention
        if mention.start > last_end {
            spans.push(Span::raw(&text[last_end..mention.start]));
        }

        // Highlighted mention
        spans.push(Span::styled(
            &text[mention.start..mention.end],
            Style::default()
                .fg(mention.mention_type.color())
                .add_modifier(Modifier::BOLD),
        ));

        last_end = mention.end;
    }

    // Text after last mention
    if last_end < text.len() {
        spans.push(Span::raw(&text[last_end..]));
    }

    spans
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mention_parse_file() {
        let mention = Mention::parse("@readme.md", 0).unwrap();
        assert_eq!(mention.mention_type, MentionType::File);
        assert_eq!(mention.value, "readme.md");
        assert_eq!(mention.full, "@readme.md");
    }

    #[test]
    fn test_mention_parse_entity() {
        let mention = Mention::parse("@entity:qr-code", 0).unwrap();
        assert_eq!(mention.mention_type, MentionType::Entity);
        assert_eq!(mention.value, "qr-code");
    }

    #[test]
    fn test_mention_parse_locale() {
        let mention = Mention::parse("@locale:fr-FR", 0).unwrap();
        assert_eq!(mention.mention_type, MentionType::Locale);
        assert_eq!(mention.value, "fr-FR");
    }

    #[test]
    fn test_mention_parse_project() {
        let mention = Mention::parse("@project:qrcode-ai", 0).unwrap();
        assert_eq!(mention.mention_type, MentionType::Project);
        assert_eq!(mention.value, "qrcode-ai");
    }

    #[test]
    fn test_mention_parse_term() {
        let mention = Mention::parse("@term:api", 0).unwrap();
        assert_eq!(mention.mention_type, MentionType::Term);
        assert_eq!(mention.value, "api");
    }

    #[test]
    fn test_find_all_mentions() {
        let mentions = Mention::find_all("Use @entity:qr-code and @locale:fr-FR for @file.rs");
        assert_eq!(mentions.len(), 3);
        assert_eq!(mentions[0].mention_type, MentionType::Entity);
        assert_eq!(mentions[1].mention_type, MentionType::Locale);
        assert_eq!(mentions[2].mention_type, MentionType::File);
    }

    #[test]
    fn test_trigger_position() {
        let trigger = Mention::at_trigger_position("Hello @ent", 10).unwrap();
        assert_eq!(trigger.start, 6);
        assert_eq!(trigger.partial, "@ent");
        assert_eq!(trigger.trigger_type, None); // Not complete prefix yet
    }

    #[test]
    fn test_trigger_position_with_type() {
        let trigger = Mention::at_trigger_position("Use @entity:qr", 14).unwrap();
        assert_eq!(trigger.trigger_type, Some(MentionType::Entity));
        assert_eq!(trigger.query, "qr");
    }

    #[test]
    fn test_autocomplete_state() {
        let mut state = MentionAutocompleteState::new();
        assert!(!state.visible);

        let trigger = MentionTrigger {
            start: 0,
            partial: "@e".to_string(),
            trigger_type: None,
            query: "e".to_string(),
        };
        let suggestions = vec![
            MentionSuggestion::entity("qr-code", Some("QR Code")),
            MentionSuggestion::entity("landing-page", Some("Landing Page")),
        ];

        state.show(trigger, suggestions);
        assert!(state.visible);
        assert_eq!(state.selected, 0);

        state.next();
        assert_eq!(state.selected, 1);

        state.next();
        assert_eq!(state.selected, 0); // Wrap around

        state.hide();
        assert!(!state.visible);
    }

    #[test]
    fn test_highlight_mentions() {
        let spans = highlight_mentions("Use @entity:qr-code");
        assert_eq!(spans.len(), 2);
    }

    #[test]
    fn test_suggestion_constructors() {
        let file = MentionSuggestion::file("readme.md");
        assert_eq!(file.insert, "@readme.md");

        let entity = MentionSuggestion::entity("qr-code", Some("QR Code"));
        assert_eq!(entity.insert, "@entity:qr-code");

        let locale = MentionSuggestion::locale("fr-FR", Some("French"));
        assert_eq!(locale.insert, "@locale:fr-FR");
    }

    #[test]
    fn test_mention_type_colors() {
        assert_eq!(MentionType::File.color(), COLOR_FILE);
        assert_eq!(MentionType::Entity.color(), COLOR_ENTITY);
        assert_eq!(MentionType::Locale.color(), COLOR_LOCALE);
        assert_eq!(MentionType::Project.color(), COLOR_PROJECT);
        assert_eq!(MentionType::Term.color(), COLOR_TERM);
    }

    #[test]
    fn test_mention_type_icons() {
        assert_eq!(MentionType::File.icon(), "📄");
        assert_eq!(MentionType::Entity.icon(), "🔷");
        assert_eq!(MentionType::Locale.icon(), "🌍");
        assert_eq!(MentionType::Project.icon(), "📁");
        assert_eq!(MentionType::Term.icon(), "📝");
    }
}
