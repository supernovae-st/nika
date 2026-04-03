//! Extract and response mode enums for the `fetch:` verb.
//!
//! These enums replace stringly-typed `Option<String>` fields with compile-time
//! checked variants. Serde deserializes directly from snake_case YAML values.

use serde::{Deserialize, Serialize};

/// Post-processing extraction mode for the `fetch:` verb.
///
/// Each mode may require a Cargo feature flag at compile time.
/// Serde deserializes from snake_case YAML strings (e.g. `extract: markdown`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractMode {
    /// Clean Markdown from HTML (feature: fetch-markdown)
    Markdown,
    /// Main article content via Readability (feature: fetch-article)
    Article,
    /// Visible text, optionally filtered by CSS selector (feature: fetch-html)
    Text,
    /// Raw HTML of matching elements (feature: fetch-html)
    Selector,
    /// OG, Twitter Cards, JSON-LD, SEO metadata (feature: fetch-html)
    Metadata,
    /// Rich link classification (feature: fetch-html)
    Links,
    /// JSONPath query on JSON responses (zero deps)
    Jsonpath,
    /// RSS/Atom/JSON Feed parsing (feature: fetch-feed)
    Feed,
    /// AI content discovery /.well-known/llm.txt
    #[serde(rename = "llm_txt")]
    LlmTxt,
    /// XML sitemap parsing (urlset + sitemapindex) (feature: fetch-sitemap)
    Sitemap,
}

impl ExtractMode {
    /// All valid mode names, for error messages and LSP completions.
    pub const ALL_NAMES: &'static [&'static str] = &[
        "markdown", "article", "text", "selector", "metadata", "links", "jsonpath", "feed",
        "llm_txt", "sitemap",
    ];

    /// Parse a string into an `ExtractMode`.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "markdown" => Some(Self::Markdown),
            "article" => Some(Self::Article),
            "text" => Some(Self::Text),
            "selector" => Some(Self::Selector),
            "metadata" => Some(Self::Metadata),
            "links" => Some(Self::Links),
            "jsonpath" => Some(Self::Jsonpath),
            "feed" => Some(Self::Feed),
            "llm_txt" => Some(Self::LlmTxt),
            "sitemap" => Some(Self::Sitemap),
            _ => None,
        }
    }

    /// Canonical snake_case name for display and serialization.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Article => "article",
            Self::Text => "text",
            Self::Selector => "selector",
            Self::Metadata => "metadata",
            Self::Links => "links",
            Self::Jsonpath => "jsonpath",
            Self::Feed => "feed",
            Self::LlmTxt => "llm_txt",
            Self::Sitemap => "sitemap",
        }
    }

    /// Check if this mode requires a specific feature flag.
    pub fn required_feature(&self) -> Option<&'static str> {
        match self {
            Self::Markdown => Some("fetch-markdown"),
            Self::Article => Some("fetch-article"),
            Self::Text | Self::Selector | Self::Metadata | Self::Links => Some("fetch-html"),
            Self::Feed => Some("fetch-feed"),
            Self::Sitemap => Some("fetch-sitemap"),
            Self::Jsonpath | Self::LlmTxt => None,
        }
    }
}

impl std::fmt::Display for ExtractMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Response output mode for the `fetch:` verb.
///
/// Controls how the HTTP response is returned to downstream tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseMode {
    /// JSON with status, headers, body, final URL
    Full,
    /// Store raw bytes in CAS, return hash
    Binary,
    /// Metadata only: status + url + elapsed_ms + redirects (no body/headers)
    Slim,
}

impl ResponseMode {
    /// All valid mode names, for error messages and LSP completions.
    pub const ALL_NAMES: &'static [&'static str] = &["full", "binary", "slim"];

    /// Parse a string into a `ResponseMode`.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "full" => Some(Self::Full),
            "binary" => Some(Self::Binary),
            "slim" => Some(Self::Slim),
            _ => None,
        }
    }

    /// Canonical name for display.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Binary => "binary",
            Self::Slim => "slim",
        }
    }
}

impl std::fmt::Display for ResponseMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_mode_parse_all() {
        for name in ExtractMode::ALL_NAMES {
            assert!(
                ExtractMode::parse(name).is_some(),
                "failed to parse '{name}'"
            );
        }
        assert!(ExtractMode::parse("unknown").is_none());
        assert!(ExtractMode::parse("").is_none());
    }

    #[test]
    fn extract_mode_roundtrip_str() {
        for name in ExtractMode::ALL_NAMES {
            let mode = ExtractMode::parse(name).unwrap();
            assert_eq!(mode.as_str(), *name);
            assert_eq!(mode.to_string(), *name);
        }
    }

    #[test]
    fn extract_mode_serde_roundtrip() {
        for name in ExtractMode::ALL_NAMES {
            let yaml = format!("\"{}\"", name);
            let mode: ExtractMode = serde_json::from_str(&yaml).unwrap();
            assert_eq!(mode, ExtractMode::parse(name).unwrap());
            let back = serde_json::to_string(&mode).unwrap();
            assert_eq!(back, yaml);
        }
    }

    #[test]
    fn extract_mode_llm_txt_serde() {
        let mode: ExtractMode = serde_json::from_str("\"llm_txt\"").unwrap();
        assert_eq!(mode, ExtractMode::LlmTxt);
        assert_eq!(serde_json::to_string(&mode).unwrap(), "\"llm_txt\"");
    }

    #[test]
    fn response_mode_parse_all() {
        for name in ResponseMode::ALL_NAMES {
            assert!(
                ResponseMode::parse(name).is_some(),
                "failed to parse '{name}'"
            );
        }
        assert!(ResponseMode::parse("stream").is_none());
        assert!(ResponseMode::parse("").is_none());
    }

    #[test]
    fn response_mode_serde_roundtrip() {
        for name in ResponseMode::ALL_NAMES {
            let yaml = format!("\"{}\"", name);
            let mode: ResponseMode = serde_json::from_str(&yaml).unwrap();
            assert_eq!(mode, ResponseMode::parse(name).unwrap());
        }
    }

    #[test]
    fn extract_mode_required_features() {
        assert_eq!(
            ExtractMode::Markdown.required_feature(),
            Some("fetch-markdown")
        );
        assert_eq!(
            ExtractMode::Article.required_feature(),
            Some("fetch-article")
        );
        assert_eq!(ExtractMode::Text.required_feature(), Some("fetch-html"));
        assert_eq!(ExtractMode::Feed.required_feature(), Some("fetch-feed"));
        assert_eq!(ExtractMode::Jsonpath.required_feature(), None);
        assert_eq!(ExtractMode::LlmTxt.required_feature(), None);
    }
}
