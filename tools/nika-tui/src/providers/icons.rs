// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Provider icons for TUI display
//!
//! Consistent icon mapping for all providers (7 LLM + 1 Local + 11 MCP).

use ratatui::style::Color;

use crate::tokens::compat;

/// Get icon for a provider
pub fn provider_icon(provider: &str) -> &'static str {
    match provider {
        // LLM providers (7)
        "anthropic" => "🧠",
        "openai" => "🤖",
        "mistral" => "🌀",
        "groq" => "⚡",
        "deepseek" => "🔬",
        "gemini" => "💎",
        "xai" => "𝕏",
        // Local providers (1)
        "native" => "🦋",
        // MCP providers
        "neo4j" => "🔷",
        "github" => "🐙",
        "slack" => "💬",
        "perplexity" => "🔍",
        "firecrawl" => "🔥",
        "supadata" => "📊",
        // Unknown
        _ => "🔑",
    }
}

/// Get terminal-safe icon (no emoji) for a provider
pub fn provider_icon_ascii(provider: &str) -> &'static str {
    match provider {
        // LLM providers (7)
        "anthropic" => "[A]",
        "openai" => "[O]",
        "mistral" => "[M]",
        "groq" => "[G]",
        "deepseek" => "[D]",
        "gemini" => "[Gm]",
        "xai" => "[X]",
        // Local providers (1)
        "native" => "[N]",
        // MCP providers
        "neo4j" => "[N4]",
        "github" => "[GH]",
        "slack" => "[Sl]",
        "perplexity" => "[Px]",
        "firecrawl" => "[Fc]",
        "supadata" => "[Sd]",
        // Unknown
        _ => "[?]",
    }
}

/// Get color for a provider
pub fn provider_color(provider: &str) -> Color {
    match provider {
        // LLM providers - distinct colors (6)
        "anthropic" => compat::AMBER_500, // Amber for Claude
        "openai" => compat::EMERALD_500,  // Green for OpenAI
        "mistral" => compat::CYAN_500,    // Cyan for Mistral
        "groq" => compat::YELLOW_500,     // Yellow for Groq (fast)
        "deepseek" => compat::BLUE_500,   // Blue for DeepSeek
        "gemini" => compat::VIOLET_500,   // Violet for Gemini
        "xai" => compat::SLATE_400,       // Slate for xAI/Grok
        // Local providers (1)
        "native" => compat::LIME_500, // Lime for native (local)
        // MCP providers - muted colors
        "neo4j" => compat::SKY_500,
        "github" => compat::SLATE_400,
        "slack" => compat::FUCHSIA_500,
        "perplexity" => compat::TEAL_500,
        "firecrawl" => compat::ORANGE_500,
        "supadata" => compat::INDIGO_500,
        // Unknown
        _ => compat::ZINC_500,
    }
}

/// Get display name for a provider
pub fn provider_display_name(provider: &str) -> &'static str {
    match provider {
        "anthropic" => "Claude (Anthropic)",
        "openai" => "OpenAI",
        "mistral" => "Mistral AI",
        "groq" => "Groq",
        "deepseek" => "DeepSeek",
        "gemini" => "Google Gemini",
        "xai" => "xAI Grok",
        "native" => "Native (Local)",
        "neo4j" => "Neo4j",
        "github" => "GitHub",
        "slack" => "Slack",
        "perplexity" => "Perplexity",
        "firecrawl" => "Firecrawl",
        "supadata" => "Supadata",
        _ => "Unknown",
    }
}

/// Get category label for a provider
pub fn provider_category(provider: &str) -> &'static str {
    match provider {
        "anthropic" | "openai" | "mistral" | "groq" | "deepseek" | "gemini" | "xai" => "LLM",
        "native" => "Local",
        "neo4j" | "github" | "slack" | "perplexity" | "firecrawl" | "supadata" => "MCP",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_llm_providers_have_icons() {
        let providers = [
            "anthropic",
            "openai",
            "mistral",
            "groq",
            "deepseek",
            "gemini",
            "xai",
        ];
        for provider in providers {
            assert_ne!(
                provider_icon(provider),
                "🔑",
                "{} should have a specific icon",
                provider
            );
        }
    }

    #[test]
    fn test_native_provider_has_icon() {
        assert_ne!(
            provider_icon("native"),
            "🔑",
            "native should have a specific icon"
        );
        assert_eq!(provider_icon("native"), "🦋");
    }

    #[test]
    fn test_all_mcp_providers_have_icons() {
        let providers = [
            "neo4j",
            "github",
            "slack",
            "perplexity",
            "firecrawl",
            "supadata",
        ];
        for provider in providers {
            assert_ne!(
                provider_icon(provider),
                "🔑",
                "{} should have a specific icon",
                provider
            );
        }
    }

    #[test]
    fn test_gemini_icon() {
        assert_eq!(provider_icon("gemini"), "💎");
    }

    #[test]
    fn test_unknown_provider_icon() {
        assert_eq!(provider_icon("unknown"), "🔑");
    }

    #[test]
    fn test_provider_category() {
        assert_eq!(provider_category("anthropic"), "LLM");
        assert_eq!(provider_category("native"), "Local");
        assert_eq!(provider_category("neo4j"), "MCP");
    }
}
