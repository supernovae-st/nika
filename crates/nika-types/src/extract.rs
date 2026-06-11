// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Extract modes for the `nika:fetch` builtin (stdlib v0.1).
//!
//! The single source of truth for the CLOSED mode set, shared by the
//! static checker (`nika-schema` arg-shape rules · conformance fixtures
//! `stdlib/extract-modes/*`), the extraction pipeline (`nika-extract`)
//! and the builtin (`nika-builtin` `nika:fetch`). The spec's 9 canonical
//! modes (`stdlib/extract-modes-v0.1.md`) plus the implicit `raw`.
//! `llm-txt` is RESERVED — rejected at v0.1, the set is closed.

use alloc::string::String;
use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

/// One extraction mode for `nika:fetch` (the `mode:` argument).
///
/// `#[non_exhaustive]`: stdlib v0.x MAY add modes (forward-compat
/// additive per the spec) — consumers match with a wildcard arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ExtractMode {
    /// HTML → cleaned Markdown (the default for content scraping).
    #[default]
    Markdown,
    /// Readability article body → Markdown (news/blogs · cleaner).
    Article,
    /// HTML tags stripped · plain text with line breaks.
    Text,
    /// Raw HTML of the element(s) matching a CSS `selector:`.
    Selector,
    /// Response parsed as JSON · a `jq:` expression applied (the one
    /// data language — composed at the builtin layer over its jq
    /// engine, never re-implemented downstream).
    Jq,
    /// `<meta>` tags · `OpenGraph` · Twitter cards · canonical · lang.
    Metadata,
    /// Every `<a href>` resolved to an absolute URL.
    Links,
    /// RSS · Atom · JSON Feed → normalized object.
    Feed,
    /// sitemap.xml / sitemap index → URL entries.
    Sitemap,
    /// The decoded body verbatim (UTF-8 text only · the implicit mode).
    Raw,
}

/// The spec spelling of every mode, in canon order (9 + `raw`) — kept
/// next to [`ExtractMode::ALL`] so error messages and checkers share
/// one list.
pub const EXTRACT_MODE_NAMES: &str =
    "markdown, article, text, selector, jq, metadata, links, feed, sitemap, raw";

impl ExtractMode {
    /// The closed stdlib v0.1 set — the spec's 9 canonical modes plus
    /// the implicit `raw`.
    pub const ALL: [Self; 10] = [
        Self::Markdown,
        Self::Article,
        Self::Text,
        Self::Selector,
        Self::Jq,
        Self::Metadata,
        Self::Links,
        Self::Feed,
        Self::Sitemap,
        Self::Raw,
    ];

    /// The spec spelling (the `mode:` argument value).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Article => "article",
            Self::Text => "text",
            Self::Selector => "selector",
            Self::Jq => "jq",
            Self::Metadata => "metadata",
            Self::Links => "links",
            Self::Feed => "feed",
            Self::Sitemap => "sitemap",
            Self::Raw => "raw",
        }
    }
}

impl fmt::Display for ExtractMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Unknown `mode:` value — the set is CLOSED at stdlib v0.1.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct UnknownExtractMode {
    /// The rejected input (as received · not normalized).
    pub input: String,
}

impl fmt::Display for UnknownExtractMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown extract mode `{}` — the stdlib v0.1 set is closed: {EXTRACT_MODE_NAMES} \
             (extract-modes-v0.1.md)",
            self.input
        )
    }
}

impl core::error::Error for UnknownExtractMode {}

impl FromStr for ExtractMode {
    type Err = UnknownExtractMode;

    /// Exact spec spelling only (the YAML surface is lowercase —
    /// liberal parsing would invite drift the conformance oracle
    /// exists to prevent).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|m| m.as_str() == s)
            .ok_or_else(|| UnknownExtractMode {
                input: String::from(s),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn the_set_is_the_canonical_ten() {
        assert_eq!(ExtractMode::ALL.len(), 10, "9 canonical + implicit raw");
        // The shared names list stays in lockstep with ALL.
        for mode in ExtractMode::ALL {
            assert!(
                EXTRACT_MODE_NAMES.contains(mode.as_str()),
                "{mode} missing from EXTRACT_MODE_NAMES"
            );
        }
        assert_eq!(
            EXTRACT_MODE_NAMES.split(", ").count(),
            ExtractMode::ALL.len(),
            "names list and ALL must have the same cardinality"
        );
    }

    #[test]
    fn from_str_round_trips_every_mode() {
        for mode in ExtractMode::ALL {
            assert_eq!(mode.as_str().parse::<ExtractMode>(), Ok(mode));
            assert_eq!(mode.to_string(), mode.as_str());
        }
    }

    #[test]
    fn unknown_and_reserved_modes_are_rejected() {
        for bad in [
            "html",
            "llm-txt",
            "jsonpath",
            "metadata-links",
            "MARKDOWN",
            "",
        ] {
            let err = bad.parse::<ExtractMode>().expect_err(bad);
            assert_eq!(err.input, bad);
            let msg = err.to_string();
            assert!(msg.contains("closed"), "{msg}");
            assert!(msg.contains("markdown, article"), "{msg}");
        }
    }

    #[test]
    fn default_is_markdown_per_spec() {
        assert_eq!(ExtractMode::default(), ExtractMode::Markdown);
    }

    #[test]
    fn serde_uses_the_spec_spelling() {
        let json = serde_json::to_string(&ExtractMode::Sitemap).expect("serialize");
        assert_eq!(json, "\"sitemap\"");
        let back: ExtractMode = serde_json::from_str("\"article\"").expect("deserialize");
        assert_eq!(back, ExtractMode::Article);
        assert!(serde_json::from_str::<ExtractMode>("\"llm-txt\"").is_err());
    }
}
