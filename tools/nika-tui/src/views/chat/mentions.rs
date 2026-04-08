// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! @ Mention Autocomplete for Chat View
//!
//! Contains mention trigger detection, suggestion generation, and acceptance.

use tui_input::InputRequest;

use super::{ChatView, Mention, MentionSuggestion, MentionType};

// ═══════════════════════════════════════════════════════════════════════════════
// @ Mention Autocomplete
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    /// Check for @ trigger in current input and update autocomplete popup
    ///
    /// Called on every input change to detect when user types @ and show suggestions.
    pub fn check_mention_trigger(&mut self) {
        let input = self.input.value();
        let cursor_char = self.input.cursor();

        // Convert character index to byte index for safe string slicing
        // tui-input uses character indices, but at_trigger_position needs byte indices
        let cursor_byte = input
            .char_indices()
            .nth(cursor_char)
            .map(|(i, _)| i)
            .unwrap_or(input.len());

        // Check if we're in a @ trigger position
        if let Some(trigger) = Mention::at_trigger_position(input, cursor_byte) {
            // Generate suggestions based on trigger
            let suggestions = self.generate_mention_suggestions(&trigger);

            if !suggestions.is_empty() {
                self.mention_autocomplete.show(trigger, suggestions);
            } else {
                self.mention_autocomplete.hide();
            }
        } else {
            self.mention_autocomplete.hide();
        }
    }

    /// Generate suggestions for a mention trigger
    fn generate_mention_suggestions(
        &self,
        trigger: &crate::widgets::MentionTrigger,
    ) -> Vec<MentionSuggestion> {
        let mut suggestions = Vec::new();
        let query = trigger.query.to_lowercase();

        match trigger.trigger_type {
            Some(MentionType::File) | None => {
                // Suggest files from memory_files and working directory
                for file in &self.memory_files {
                    if file.name.to_lowercase().contains(&query) {
                        suggestions.push(MentionSuggestion::file(&file.name));
                    }
                }

                // If no type specified, also suggest type prefixes
                if trigger.trigger_type.is_none() && query.is_empty() {
                    // Show type hints when user just typed @
                    if suggestions.len() < 5 {
                        suggestions.push(MentionSuggestion {
                            display: "entity:".to_string(),
                            insert: "@entity:".to_string(),
                            mention_type: MentionType::Entity,
                            description: Some("Reference a NovaNet entity".to_string()),
                        });
                        suggestions.push(MentionSuggestion {
                            display: "locale:".to_string(),
                            insert: "@locale:".to_string(),
                            mention_type: MentionType::Locale,
                            description: Some("Reference a locale".to_string()),
                        });
                        suggestions.push(MentionSuggestion {
                            display: "project:".to_string(),
                            insert: "@project:".to_string(),
                            mention_type: MentionType::Project,
                            description: Some("Reference a project".to_string()),
                        });
                    }
                }
            }
            Some(MentionType::Entity) => {
                // Suggest entities (would come from NovaNet MCP)
                // For now, use context_items as source
                for item in &self.context_items {
                    if item.mention.to_lowercase().contains(&query) {
                        suggestions.push(MentionSuggestion::entity(
                            &item.mention,
                            Some(&item.mention),
                        ));
                    }
                }
                // Add some example entities if empty
                if suggestions.is_empty() && query.is_empty() {
                    suggestions.push(MentionSuggestion::entity("qr-code", Some("QR Code")));
                    suggestions.push(MentionSuggestion::entity(
                        "url-shortener",
                        Some("URL Shortener"),
                    ));
                }
            }
            Some(MentionType::Locale) => {
                // Suggest common locales
                let locales = [
                    ("en-US", "English (US)"),
                    ("fr-FR", "French"),
                    ("de-DE", "German"),
                    ("es-ES", "Spanish"),
                    ("it-IT", "Italian"),
                    ("ja-JP", "Japanese"),
                    ("zh-CN", "Chinese (Simplified)"),
                    ("pt-BR", "Portuguese (Brazil)"),
                ];
                for (code, name) in locales {
                    if code.to_lowercase().contains(&query) || name.to_lowercase().contains(&query)
                    {
                        suggestions.push(MentionSuggestion::locale(code, Some(name)));
                    }
                }
            }
            Some(MentionType::Project) => {
                // Suggest projects (would come from NovaNet MCP)
                if "qrcode-ai".contains(&query) {
                    suggestions.push(MentionSuggestion::project("qrcode-ai", Some("QR Code AI")));
                }
            }
            Some(MentionType::Term) => {
                // Suggest terms (would come from NovaNet MCP)
                // For now, empty - would be populated by MCP
            }
        }

        // Limit to 8 suggestions
        suggestions.truncate(8);
        suggestions
    }

    /// Accept the currently selected mention suggestion
    ///
    /// Replaces the partial @ trigger with the full mention text.
    pub fn accept_mention_suggestion(&mut self) {
        if let (Some(trigger), Some(suggestion)) = (
            self.mention_autocomplete.trigger.as_ref(),
            self.mention_autocomplete.current(),
        ) {
            let input = self.input.value().to_string();
            let before = &input[..trigger.start];
            let after = &input[trigger.start + trigger.partial.len()..];

            // Build new input with suggestion
            let new_input = format!("{}{} {}", before, suggestion.insert, after);

            // Update input (using tui-input API)
            // Input::new() places cursor at end, so we need to reposition
            self.input = tui_input::Input::new(new_input.clone());
            // This ensures cursor ends up after the inserted mention + space
            self.input.handle(InputRequest::GoToStart);
            let new_cursor = trigger.start + suggestion.insert.len() + 1;
            for _ in 0..new_cursor.min(new_input.len()) {
                self.input.handle(InputRequest::GoToNextChar);
            }
        }

        self.mention_autocomplete.hide();
    }
}
