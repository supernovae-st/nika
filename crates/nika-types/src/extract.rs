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

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// One extraction mode for `nika:fetch` (the `mode:` argument).
///
/// `#[non_exhaustive]`: stdlib v0.x MAY add modes (forward-compat
/// additive per the spec) — consumers match with a wildcard arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
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

impl UnknownExtractMode {
    /// The canonical mode this input is a TYPO of, when one is close
    /// enough to assert (`markdwon` → `markdown`) — the shared
    /// [`crate::suggest::did_you_mean`] metric over [`ExtractMode::ALL`],
    /// so this surface suggests with the same threshold semantics as the
    /// parser/checker. Distinct from [`Self::hint`]: a typo names the
    /// right mode wrong; a hint answers the WRONG-CONCEPT miss (`json`).
    #[must_use]
    pub fn suggestion(&self) -> Option<&'static str> {
        crate::suggest::did_you_mean(&self.input, ExtractMode::ALL.map(ExtractMode::as_str))
    }

    /// A route hint for the empirically-common wrong guesses — the
    /// new-user battery (2026-07-10) hit `json` twice: the set is closed
    /// by the one-data-language law (jq replaced `JSONPath`), so the error
    /// must TEACH the canonical route instead of only naming the enum
    /// (the arithmetic-diagnostic precedent). Evidence-based rows only ·
    /// `None` for inputs with no obvious intent.
    #[must_use]
    pub fn hint(&self) -> Option<&'static str> {
        match self.input.as_str() {
            "json" => Some(
                "for a parsed JSON response use `mode: jq` with `jq: \".\"` \
                 (jq is the one data language)",
            ),
            "html" => Some(
                "for the raw page use `mode: raw` · for one element use \
                 `mode: selector`",
            ),
            "xml" | "rss" | "atom" => Some(
                "for RSS/Atom/JSON-Feed use `mode: feed` · for arbitrary XML \
                 use `mode: raw`",
            ),
            _ => None,
        }
    }
}

impl fmt::Display for UnknownExtractMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown extract mode `{}` — the stdlib v0.1 set is closed: {EXTRACT_MODE_NAMES} \
             (extract-modes-v0.1.md)",
            self.input
        )?;
        // A typo of a real mode outranks the wrong-concept table — the
        // rename IS the fix; the hint would answer a question not asked.
        if let Some(mode) = self.suggestion() {
            write!(f, " · did you mean `{mode}`?")?;
        } else if let Some(hint) = self.hint() {
            write!(f, " · {hint}")?;
        }
        Ok(())
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
#[cfg(test)]
mod hint_tests {
    use super::*;

    /// The did-you-mean table teaches the canonical route for the
    /// measured wrong guesses (battery 2026-07-10 · `json` twice) and
    /// stays silent on inputs with no obvious intent — the closed-set
    /// framing must never speculate.
    #[test]
    fn hint_teaches_measured_guesses_only() {
        let e = |s: &str| UnknownExtractMode {
            input: s.to_owned(),
        };
        let json = e("json").to_string();
        assert!(
            json.contains("mode: jq") && json.contains("one data language"),
            "{json}"
        );
        assert!(e("html").to_string().contains("mode: selector"));
        assert!(e("rss").to_string().contains("mode: feed"));
        // no speculation · the base closed-set message stands alone
        let plain = e("banana").to_string();
        assert!(plain.contains("the stdlib v0.1 set is closed"));
        assert!(!plain.contains(" · for"), "{plain}");
        assert!(e("banana").hint().is_none());
    }

    /// The typo rung (sweep 2026-07-11 · `markdwon` answered mutely):
    /// a near-miss of a real mode gets the rename, and it OUTRANKS the
    /// wrong-concept hint (the rename is the fix — a route hint would
    /// answer a question not asked). The two rungs never collide:
    /// every hint-table input (`json` · `html` · `rss`…) is past the
    /// metric threshold from every canonical mode name.
    #[test]
    fn typo_of_a_real_mode_gets_the_rename() {
        let e = |s: &str| UnknownExtractMode {
            input: s.to_owned(),
        };
        assert_eq!(e("markdwon").suggestion(), Some("markdown"));
        let msg = e("markdwon").to_string();
        assert!(msg.contains("did you mean `markdown`?"), "{msg}");
        assert_eq!(e("selectr").suggestion(), Some("selector"));
        // the hint-table inputs stay on their semantic rung
        for concept in ["json", "html", "rss", "atom", "xml"] {
            assert_eq!(e(concept).suggestion(), None, "{concept} is not a typo");
            assert!(e(concept).hint().is_some(), "{concept} keeps its route");
        }
        // far from everything → base message only
        assert_eq!(e("banana").suggestion(), None);
    }
}
