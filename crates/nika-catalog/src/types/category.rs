// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Category enum for MCP servers and providers.
//!
//! Closed, typed enum. `#[non_exhaustive]` so new categories are additive,
//! not breaking. `FromStr` / `as_str` roundtrip uses kebab-case — the exact
//! wire format found in `data/*.toml` and in the MCP Registry.

/// Category used to group MCP servers and providers in CLI displays.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Category {
    /// Anthropic's reference MCP servers.
    Anthropic,
    /// SQL / `NoSQL` / graph databases.
    Databases,
    /// Web / code search engines.
    Search,
    /// Developer tooling (GitHub, GitLab, CI, code quality).
    Developer,
    /// Productivity apps (Notion, Linear, Asana).
    Productivity,
    /// LLM / AI-adjacent tools.
    Ai,
    /// Image generation, editing, analysis.
    Image,
    /// Audio / TTS / speech.
    Audio,
    /// Messaging / email / communication.
    Communication,
    /// Vector databases (Qdrant, Chroma, Weaviate, Milvus).
    Vectordb,
    /// Analytics / observability (Grafana, `PostHog`).
    Analytics,
    /// Commerce platforms (Stripe, Shopify).
    Ecommerce,
    /// Content management systems (Ghost, `WordPress`).
    Cms,
    /// Infrastructure / devops (AWS, Cloudflare, Fly.io, Buildkite).
    Devops,
    /// Social networks (Twitter, `LinkedIn`, Reddit).
    Social,
    /// Lifestyle (recipes, weather).
    Lifestyle,
    /// Marketing (Mailchimp, Google Ads, `SEMrush`).
    Marketing,
    /// Mapping and geocoding.
    Maps,
}

impl Category {
    /// Kebab-case wire identifier, matching TOML and registry schemas.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::Databases => "databases",
            Self::Search => "search",
            Self::Developer => "developer",
            Self::Productivity => "productivity",
            Self::Ai => "ai",
            Self::Image => "image",
            Self::Audio => "audio",
            Self::Communication => "communication",
            Self::Vectordb => "vectordb",
            Self::Analytics => "analytics",
            Self::Ecommerce => "ecommerce",
            Self::Cms => "cms",
            Self::Devops => "devops",
            Self::Social => "social",
            Self::Lifestyle => "lifestyle",
            Self::Marketing => "marketing",
            Self::Maps => "maps",
        }
    }

    /// Parse a kebab-case category identifier. Returns `None` for unknown
    /// strings — callers (notably `build.rs`) decide whether "unknown" is
    /// fatal. Thin `Option` wrapper over the [`std::str::FromStr`] impl
    /// below; use whichever fits the call site.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        <Self as core::str::FromStr>::from_str(s).ok()
    }
}

/// Error returned when parsing a [`Category`] from a string fails.
///
/// Carries the offending input as an owned `String` so callers can quote
/// it back in diagnostics (including `?`-propagated error chains from
/// external tooling that iterates TOML files).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ParseCategoryError {
    /// The input that failed to parse.
    pub input: String,
}

impl ParseCategoryError {
    /// Explicit constructor — required because [`ParseCategoryError`] is
    /// `#[non_exhaustive]` (invariant #19).
    #[must_use]
    pub const fn new(input: String) -> Self {
        Self { input }
    }
}

impl core::fmt::Display for ParseCategoryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "unknown category {:?}", self.input)
    }
}

impl std::error::Error for ParseCategoryError {}

impl core::str::FromStr for Category {
    type Err = ParseCategoryError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "anthropic" => Self::Anthropic,
            "databases" => Self::Databases,
            "search" => Self::Search,
            "developer" => Self::Developer,
            "productivity" => Self::Productivity,
            "ai" => Self::Ai,
            "image" => Self::Image,
            "audio" => Self::Audio,
            "communication" => Self::Communication,
            "vectordb" => Self::Vectordb,
            "analytics" => Self::Analytics,
            "ecommerce" => Self::Ecommerce,
            "cms" => Self::Cms,
            "devops" => Self::Devops,
            "social" => Self::Social,
            "lifestyle" => Self::Lifestyle,
            "marketing" => Self::Marketing,
            "maps" => Self::Maps,
            _ => {
                return Err(ParseCategoryError {
                    input: s.to_owned(),
                });
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_CATEGORIES: &[Category] = &[
        Category::Anthropic,
        Category::Databases,
        Category::Search,
        Category::Developer,
        Category::Productivity,
        Category::Ai,
        Category::Image,
        Category::Audio,
        Category::Communication,
        Category::Vectordb,
        Category::Analytics,
        Category::Ecommerce,
        Category::Cms,
        Category::Devops,
        Category::Social,
        Category::Lifestyle,
        Category::Marketing,
        Category::Maps,
    ];

    #[test]
    fn roundtrip_all_categories() {
        for cat in ALL_CATEGORIES {
            assert_eq!(Category::parse(cat.as_str()), Some(*cat));
        }
    }

    #[test]
    fn unknown_category_returns_none() {
        assert_eq!(Category::parse("cheese"), None);
        assert_eq!(Category::parse(""), None);
        assert_eq!(Category::parse("AI"), None); // case-sensitive
    }

    #[test]
    fn as_str_is_kebab_case_lowercase() {
        for cat in ALL_CATEGORIES {
            let s = cat.as_str();
            assert!(!s.is_empty());
            assert!(
                s.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "category {s:?} must be kebab-case lowercase",
            );
        }
    }

    #[test]
    fn covers_all_in_use_categories() {
        // Every category currently used by an entry in `data/mcp-servers.toml`
        // must round-trip. Add a row here when the catalog gains a new one.
        let in_use = [
            "ai",
            "analytics",
            "anthropic",
            "cms",
            "communication",
            "databases",
            "developer",
            "devops",
            "ecommerce",
            "image",
            "lifestyle",
            "maps",
            "marketing",
            "productivity",
            "search",
            "social",
            "vectordb",
        ];
        for s in in_use {
            assert!(Category::parse(s).is_some(), "missing category: {s}");
        }
    }
}
