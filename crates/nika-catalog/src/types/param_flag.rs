// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! API-level parameter capability flags.
//!
//! Session 2b introduces [`ParamFlag`] so a [`crate::types::ModelCapabilities`]
//! can declare which optional knobs a provider/model actually accepts —
//! e.g. `parallel_tool_calls`, `reasoning_effort`, `web_search`.
//!
//! Note: `StructuredOutputNative` was removed in Session 4a and replaced
//! by the [`crate::types::JsonMode`] enum on [`crate::types::ModelCapabilities`],
//! which provides 3-level granularity (unavailable / object / schema).

/// API-level parameter capability flag.
///
/// **Declaration order = `Ord` order = emitted slice order** (enforced by
/// `build.rs`, same invariant as [`crate::types::Modality`]). Slices stored
/// in `ModelCapabilities::supported_parameters` are sorted by declaration
/// order so a caller can `binary_search` with the derived `Ord`.
///
/// `#[non_exhaustive]` — new flags (e.g. `VoiceMode`, `ResponsesApi`) will
/// be added as providers expose new knobs. Downstream `match` statements
/// must include a `_ =>` arm.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ParamFlag {
    /// Model accepts `parallel_tool_calls` (request multiple tool invocations
    /// in one turn). `OpenAI`, `Anthropic`, `Mistral` — most modern frontier
    /// chat models.
    ParallelToolCalls,
    /// Model accepts `reasoning_effort` (e.g. `"low" | "medium" | "high"`).
    /// `OpenAI` o-series, xAI `grok-3-mini`, `Gemini` 2.5 Flash. **NOT** grok-4 /
    /// grok-4-1-fast (those reason automatically and reject the param).
    ReasoningEffort,
    /// `Anthropic`-style `thinking.budget_tokens`. Claude 3.7+ and Claude 4.
    ThinkingBudget,
    /// `Anthropic` `cache_control` on message parts / system prompts.
    /// Gemini implicit caching exposed via explicit cache API. Grok beta.
    PromptCaching,
    /// `OpenAI` `file_search` tool — built-in retrieval augmentation.
    FileSearch,
    /// Provider-native web search tool (`web_search_preview`, Grok
    /// `web_search`, Perplexity search-augmented responses).
    WebSearch,
    /// Streaming of the `thinking` channel in real time (Anthropic stream
    /// `thinking_delta`, `OpenAI` Responses API `reasoning.delta`).
    StreamingThinking,
}

impl ParamFlag {
    /// Canonical kebab-case string used in `data/model-capabilities.toml`
    /// and in serde output.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ParallelToolCalls => "parallel-tool-calls",
            Self::ReasoningEffort => "reasoning-effort",
            Self::ThinkingBudget => "thinking-budget",
            Self::PromptCaching => "prompt-caching",
            Self::FileSearch => "file-search",
            Self::WebSearch => "web-search",
            Self::StreamingThinking => "streaming-thinking",
        }
    }
}

impl core::fmt::Display for ParamFlag {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Failure when parsing a [`ParamFlag`] from its canonical string.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ParseParamFlagError {
    /// The raw input that failed to match.
    pub input: String,
}

impl ParseParamFlagError {
    /// Explicit constructor — required because the struct is
    /// `#[non_exhaustive]` (invariant #19).
    #[must_use]
    pub const fn new(input: String) -> Self {
        Self { input }
    }
}

impl core::fmt::Display for ParseParamFlagError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "unknown param flag {:?} — expected one of: \
             parallel-tool-calls, reasoning-effort, thinking-budget, \
             prompt-caching, file-search, web-search, streaming-thinking",
            self.input
        )
    }
}

impl std::error::Error for ParseParamFlagError {}

impl core::str::FromStr for ParamFlag {
    type Err = ParseParamFlagError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "parallel-tool-calls" => Ok(Self::ParallelToolCalls),
            "reasoning-effort" => Ok(Self::ReasoningEffort),
            "thinking-budget" => Ok(Self::ThinkingBudget),
            "prompt-caching" => Ok(Self::PromptCaching),
            "file-search" => Ok(Self::FileSearch),
            "web-search" => Ok(Self::WebSearch),
            "streaming-thinking" => Ok(Self::StreamingThinking),
            _ => Err(ParseParamFlagError::new(s.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ParamFlag, ParseParamFlagError};
    use core::str::FromStr;

    const ALL: &[ParamFlag] = &[
        ParamFlag::ParallelToolCalls,
        ParamFlag::ReasoningEffort,
        ParamFlag::ThinkingBudget,
        ParamFlag::PromptCaching,
        ParamFlag::FileSearch,
        ParamFlag::WebSearch,
        ParamFlag::StreamingThinking,
    ];

    #[test]
    fn as_str_roundtrip_for_every_variant() {
        for f in ALL {
            let s = f.as_str();
            let parsed = ParamFlag::from_str(s).expect("round-trip must succeed");
            assert_eq!(parsed, *f, "as_str({f:?}) → {s:?} did not round-trip");
        }
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(format!("{}", ParamFlag::WebSearch), "web-search");
        assert_eq!(format!("{}", ParamFlag::FileSearch), "file-search");
    }

    #[test]
    fn ord_matches_declaration_order() {
        // Pairwise check: consecutive variants must compare strictly less.
        assert!(ParamFlag::ParallelToolCalls < ParamFlag::ReasoningEffort);
        assert!(ParamFlag::ReasoningEffort < ParamFlag::ThinkingBudget);
        assert!(ParamFlag::ThinkingBudget < ParamFlag::PromptCaching);
        assert!(ParamFlag::PromptCaching < ParamFlag::FileSearch);
        assert!(ParamFlag::FileSearch < ParamFlag::WebSearch);
        assert!(ParamFlag::WebSearch < ParamFlag::StreamingThinking);
    }

    #[test]
    fn sorted_slice_supports_binary_search() {
        let mut all = ALL.to_vec();
        all.sort();
        // Derived Ord + declaration-order invariant means a pre-sorted
        // slice is searchable.
        assert!(all.binary_search(&ParamFlag::WebSearch).is_ok());
        assert!(all.binary_search(&ParamFlag::FileSearch).is_ok());
    }

    #[test]
    fn from_str_unknown_returns_error_with_input() {
        let err = ParamFlag::from_str("quantum-thinking").unwrap_err();
        assert_eq!(
            err,
            ParseParamFlagError::new("quantum-thinking".to_string())
        );
        let msg = format!("{err}");
        assert!(msg.contains("quantum-thinking"));
        assert!(msg.contains("file-search"));
    }

    #[test]
    fn from_str_is_case_sensitive() {
        assert!(ParamFlag::from_str("Parallel-Tool-Calls").is_err());
        assert!(ParamFlag::from_str("WEB-SEARCH").is_err());
    }

    #[test]
    fn copy_and_hash_are_derived() {
        fn assert_copy<T: Copy>() {}
        fn assert_hash<T: core::hash::Hash>() {}
        assert_copy::<ParamFlag>();
        assert_hash::<ParamFlag>();
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip_kebab_case() {
        for f in ALL {
            let json = serde_json::to_string(f).unwrap();
            // Serialised value must match as_str wrapped in quotes.
            assert_eq!(json, format!("\"{}\"", f.as_str()));
            let back: ParamFlag = serde_json::from_str(&json).unwrap();
            assert_eq!(back, *f);
        }
    }
}
