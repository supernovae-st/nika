// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Tokenizer family enum for context-window estimation.
//!
//! Introduced in Session 2b as an optional capability of
//! [`crate::types::ModelCapabilities`]. `None` = provider does not disclose
//! its tokenizer family — fall back to conservative character-based
//! estimation.
//!
//! Session 3 adds `Qwen` variant when the Qwen provider lands.

/// Known tokenizer families.
///
/// **Kebab-case serde note** — automatic `rename_all = "kebab-case"`
/// deserialises `ClaudeV3` as `"claude-v3"` (hyphen inserted at
/// lowercase→uppercase transition). However `DeepSeek` becomes `"deep-seek"`
/// which is NOT the TOML value we want (`"deepseek"`). The `build.rs`
/// companion enum uses `#[serde(rename = "deepseek")]` on that variant to
/// bridge the gap (P0 fix, Session 2b review round 5).
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TokenizerFamily {
    /// tiktoken `cl100k_base` — GPT-4 legacy, GPT-3.5-turbo.
    Cl100k,
    /// tiktoken `o200k_base` — GPT-4o, GPT-4.1, o-series, xAI compat routes.
    O200k,
    /// Anthropic proprietary tokenizer used on Claude v3 / v4 generations.
    ClaudeV3,
    /// Google `SentencePiece` variant used on Gemini 1.x and 2.x.
    Gemini,
    /// Meta `LLaMA` 3.x / Llama 4 tokenizer (`TikToken`-based, ~128k vocab).
    LlamaV3,
    /// Mistral `SentencePiece` v3 — shared across Mistral Large 3,
    /// Small 3.x/4.x, Medium, Pixtral, Magistral.
    MistralV3,
    /// `DeepSeek` tokenizer — `cl100k`-family variant; distinct variant kept
    /// for accurate per-provider token accounting.
    ///
    /// Explicit `#[serde(rename = "deepseek")]` override — the enclosing
    /// `rename_all = "kebab-case"` would otherwise produce `"deep-seek"`
    /// (hyphen at the lowercase→uppercase transition) and break
    /// round-tripping with the canonical TOML value, the `as_str()`
    /// output, and the `FromStr` impl (all of which are `"deepseek"`).
    /// P0 fix — Session 2b review swarm finding.
    #[cfg_attr(feature = "serde", serde(rename = "deepseek"))]
    DeepSeek,
    /// Alibaba Qwen tokenizer (Qwen-family BPE) — shared across Qwen-Max,
    /// Qwen-Turbo, Qwen-Plus, Qwen-VL series, and Qwen-Coder. Distinct
    /// from `Cl100k`/`O200k` because vocab and special-token handling
    /// differ materially. Session 3 addition.
    Qwen,
}

impl TokenizerFamily {
    /// Canonical kebab-case string used in `data/model-capabilities.toml`
    /// and in serde output.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cl100k => "cl100k",
            Self::O200k => "o200k",
            Self::ClaudeV3 => "claude-v3",
            Self::Gemini => "gemini",
            Self::LlamaV3 => "llama-v3",
            Self::MistralV3 => "mistral-v3",
            Self::DeepSeek => "deepseek",
            Self::Qwen => "qwen",
        }
    }
}

impl core::fmt::Display for TokenizerFamily {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Failure when parsing a [`TokenizerFamily`] from its canonical string.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ParseTokenizerFamilyError {
    /// The raw input that failed to match.
    pub input: String,
}

impl ParseTokenizerFamilyError {
    /// Explicit constructor — required because the struct is
    /// `#[non_exhaustive]` (invariant #19).
    #[must_use]
    pub const fn new(input: String) -> Self {
        Self { input }
    }
}

impl core::fmt::Display for ParseTokenizerFamilyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "unknown tokenizer family {:?} — expected one of: \
             cl100k, o200k, claude-v3, gemini, llama-v3, mistral-v3, deepseek, qwen",
            self.input
        )
    }
}

impl std::error::Error for ParseTokenizerFamilyError {}

impl core::str::FromStr for TokenizerFamily {
    type Err = ParseTokenizerFamilyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "cl100k" => Ok(Self::Cl100k),
            "o200k" => Ok(Self::O200k),
            "claude-v3" => Ok(Self::ClaudeV3),
            "gemini" => Ok(Self::Gemini),
            "llama-v3" => Ok(Self::LlamaV3),
            "mistral-v3" => Ok(Self::MistralV3),
            "deepseek" => Ok(Self::DeepSeek),
            "qwen" => Ok(Self::Qwen),
            _ => Err(ParseTokenizerFamilyError::new(s.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ParseTokenizerFamilyError, TokenizerFamily};
    use core::str::FromStr;

    #[test]
    fn as_str_roundtrip_for_every_variant() {
        for t in [
            TokenizerFamily::Cl100k,
            TokenizerFamily::O200k,
            TokenizerFamily::ClaudeV3,
            TokenizerFamily::Gemini,
            TokenizerFamily::LlamaV3,
            TokenizerFamily::MistralV3,
            TokenizerFamily::DeepSeek,
            TokenizerFamily::Qwen,
        ] {
            let s = t.as_str();
            let parsed = TokenizerFamily::from_str(s).expect("round-trip must succeed");
            assert_eq!(parsed, t, "as_str({t:?}) → {s:?} did not round-trip");
        }
    }

    #[test]
    fn qwen_variant_roundtrips_with_canonical_string() {
        // Session 3 addition — dedicated test for the new variant so the
        // P0 regression (reintroducing a hyphen, or serde producing the
        // wrong string) fails loud instead of being absorbed by the generic
        // round-trip loop.
        assert_eq!(TokenizerFamily::Qwen.as_str(), "qwen");
        assert_eq!(
            TokenizerFamily::from_str("qwen").expect("qwen must parse"),
            TokenizerFamily::Qwen
        );
        assert_eq!(format!("{}", TokenizerFamily::Qwen), "qwen");
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(format!("{}", TokenizerFamily::Cl100k), "cl100k");
        assert_eq!(format!("{}", TokenizerFamily::ClaudeV3), "claude-v3");
        assert_eq!(format!("{}", TokenizerFamily::DeepSeek), "deepseek");
    }

    #[test]
    fn deepseek_canonical_is_compact_not_hyphenated() {
        // ⚠ P0: TOML value is "deepseek" (compact), NOT "deep-seek". The
        // matching build.rs enum uses #[serde(rename = "deepseek")] to
        // bridge the serde kebab-case → TOML mismatch.
        assert_eq!(TokenizerFamily::DeepSeek.as_str(), "deepseek");
        assert!(TokenizerFamily::from_str("deep-seek").is_err());
    }

    #[test]
    fn from_str_unknown_returns_error_with_input() {
        // NB: "qwen" became a valid variant in Session 3 — use a nonsense
        // token that is NOT a tokenizer family to keep the test meaningful.
        let err = TokenizerFamily::from_str("sentencepiece-alien").unwrap_err();
        assert_eq!(
            err,
            ParseTokenizerFamilyError::new("sentencepiece-alien".to_string())
        );
        let msg = format!("{err}");
        assert!(msg.contains("sentencepiece-alien"), "error must echo input: {msg}");
        assert!(msg.contains("cl100k"), "error must list expected values: {msg}");
        assert!(msg.contains("qwen"), "error must list Session 3 additions: {msg}");
    }

    #[test]
    fn from_str_is_case_sensitive() {
        assert!(TokenizerFamily::from_str("CL100K").is_err());
        assert!(TokenizerFamily::from_str("Claude-V3").is_err());
    }

    #[test]
    fn copy_and_hash_are_derived() {
        fn assert_copy<T: Copy>() {}
        fn assert_hash<T: core::hash::Hash>() {}
        assert_copy::<TokenizerFamily>();
        assert_hash::<TokenizerFamily>();
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip_kebab_case_produces_expected_strings() {
        // Every variant MUST serialize to its canonical `as_str()` output.
        // The P0 was that `DeepSeek` serialised as `"deep-seek"` under the
        // outer `rename_all = "kebab-case"` — fixed by the per-variant
        // `#[serde(rename = "deepseek")]`.
        for t in [
            TokenizerFamily::Cl100k,
            TokenizerFamily::O200k,
            TokenizerFamily::ClaudeV3,
            TokenizerFamily::Gemini,
            TokenizerFamily::LlamaV3,
            TokenizerFamily::MistralV3,
            TokenizerFamily::DeepSeek,
            TokenizerFamily::Qwen,
        ] {
            let actual = serde_json::to_string(&t).unwrap();
            let expected = format!("\"{}\"", t.as_str());
            assert_eq!(actual, expected, "serde mismatch for {t:?}");
            let back: TokenizerFamily = serde_json::from_str(&actual).unwrap();
            assert_eq!(back, t);
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_deepseek_emits_compact_not_kebab() {
        // Regression guard for the P0 — the DeepSeek serde rename. If the
        // `#[serde(rename = "deepseek")]` attribute disappears from the
        // variant, this test fails instead of the bug shipping silently.
        let json = serde_json::to_string(&TokenizerFamily::DeepSeek).unwrap();
        assert_eq!(json, "\"deepseek\"", "DeepSeek must serde as compact");
        assert_ne!(json, "\"deep-seek\"", "must NOT be kebab-hyphenated");
        let back: TokenizerFamily = serde_json::from_str("\"deepseek\"").unwrap();
        assert_eq!(back, TokenizerFamily::DeepSeek);
        // The hyphenated form must fail to deserialize — not an alias.
        assert!(serde_json::from_str::<TokenizerFamily>("\"deep-seek\"").is_err());
    }
}
