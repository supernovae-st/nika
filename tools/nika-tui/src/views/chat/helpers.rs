// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Chat View Helpers
//!
//! Utility functions and helpers for the chat view.

// ═══════════════════════════════════════════════════════════════════════════════
// Error Categorization
// ═══════════════════════════════════════════════════════════════════════════════

/// Categorize error and provide recovery suggestion.
///
/// Returns (category_name, recovery_suggestion) for user-friendly error display.
/// Used to provide actionable feedback when API/MCP/network errors occur.
pub fn categorize_error(error: &str) -> (&'static str, &'static str) {
    let error_lower = error.to_lowercase();

    if error_lower.contains("api key")
        || error_lower.contains("authentication")
        || error_lower.contains("unauthorized")
    {
        (
            "Auth",
            "Check your API key. Set ANTHROPIC_API_KEY or OPENAI_API_KEY.",
        )
    } else if error_lower.contains("timeout")
        || error_lower.contains("timed out")
        || error_lower.contains("deadline")
    {
        (
            "Timeout",
            "Request timed out. Try a shorter prompt or check your connection.",
        )
    } else if error_lower.contains("rate limit")
        || error_lower.contains("too many requests")
        || error_lower.contains("quota")
        || error_lower.contains("status: 429")
        || error_lower.contains("429 too many")
    {
        (
            "Rate Limit",
            "API rate limit reached. Wait a moment and try again.",
        )
    } else if error_lower.contains("status: 401") || error_lower.contains("status: 403") {
        (
            "Auth",
            "Authentication failed (HTTP 401/403). Check your API key.",
        )
    } else if error_lower.contains("status: 500")
        || error_lower.contains("status: 502")
        || error_lower.contains("status: 503")
    {
        (
            "Server",
            "LLM provider server error (5xx). Try again in a moment.",
        )
    } else if error_lower.contains("connection")
        || error_lower.contains("network")
        || error_lower.contains("dns")
        || error_lower.contains("resolve")
        || error_lower.contains("not responding")
    {
        (
            "Network",
            "Connection failed. Check your internet connection.",
        )
    } else if error_lower.contains("mcp")
        || error_lower.contains("mcp server")
        || error_lower.contains("tool not found")
        || error_lower.contains("tool call failed")
    {
        (
            "MCP",
            "MCP server issue. Use /mcp list to check available servers.",
        )
    } else if error_lower.contains("parse")
        || error_lower.contains("json")
        || error_lower.contains("invalid")
    {
        ("Parse", "Invalid input format. Check your command syntax.")
    } else {
        ("Unexpected", "Please try again or use /clear to restart.")
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_categorize_auth_errors() {
        let (cat, _) = categorize_error("Invalid API key");
        assert_eq!(cat, "Auth");

        let (cat, _) = categorize_error("authentication failed");
        assert_eq!(cat, "Auth");

        let (cat, _) = categorize_error("Unauthorized access");
        assert_eq!(cat, "Auth");
    }

    #[test]
    fn test_categorize_timeout_errors() {
        let (cat, _) = categorize_error("Request timeout");
        assert_eq!(cat, "Timeout");

        let (cat, _) = categorize_error("The operation timed out");
        assert_eq!(cat, "Timeout");

        let (cat, _) = categorize_error("deadline exceeded");
        assert_eq!(cat, "Timeout");
    }

    #[test]
    fn test_categorize_rate_limit_errors() {
        let (cat, _) = categorize_error("Rate limit exceeded");
        assert_eq!(cat, "Rate Limit");

        let (cat, _) = categorize_error("Too many requests");
        assert_eq!(cat, "Rate Limit");

        let (cat, _) = categorize_error("API quota exceeded");
        assert_eq!(cat, "Rate Limit");
    }

    #[test]
    fn test_categorize_network_errors() {
        let (cat, _) = categorize_error("Connection refused");
        assert_eq!(cat, "Network");

        let (cat, _) = categorize_error("Network unreachable");
        assert_eq!(cat, "Network");

        let (cat, _) = categorize_error("DNS resolution failed");
        assert_eq!(cat, "Network");

        let (cat, _) = categorize_error("Could not resolve host");
        assert_eq!(cat, "Network");
    }

    #[test]
    fn test_categorize_mcp_errors() {
        let (cat, _) = categorize_error("MCP server error");
        assert_eq!(cat, "MCP");

        // "Server not responding" is a network issue, not MCP-specific
        let (cat, _) = categorize_error("Server not responding");
        assert_eq!(cat, "Network");

        let (cat, _) = categorize_error("Tool not found");
        assert_eq!(cat, "MCP");
    }

    #[test]
    fn test_categorize_parse_errors() {
        let (cat, _) = categorize_error("Parse error in config");
        assert_eq!(cat, "Parse");

        let (cat, _) = categorize_error("Invalid JSON format");
        assert_eq!(cat, "Parse");

        let (cat, _) = categorize_error("Invalid parameter");
        assert_eq!(cat, "Parse");
    }

    #[test]
    fn test_categorize_unknown_errors() {
        let (cat, _) = categorize_error("Something went wrong");
        assert_eq!(cat, "Unexpected");

        let (cat, _) = categorize_error("");
        assert_eq!(cat, "Unexpected");
    }

    #[test]
    fn test_error_suggestions_not_empty() {
        // All categories should provide a non-empty suggestion
        let test_errors = [
            "api key invalid",
            "timeout",
            "rate limit",
            "connection error",
            "mcp error",
            "parse error",
            "unknown",
        ];

        for error in test_errors {
            let (_, suggestion) = categorize_error(error);
            assert!(
                !suggestion.is_empty(),
                "Suggestion for '{}' should not be empty",
                error
            );
        }
    }
}
