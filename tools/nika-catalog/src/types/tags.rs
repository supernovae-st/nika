// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Tag vocabulary for catalog entries.
//!
//! `Tag` is a **closed, typed enum** for capabilities, deployment properties,
//! and MCP-server permissioning. `#[non_exhaustive]` allows new variants to be
//! added in future minor versions without breaking downstream crates.
//!
//! # Wire format
//!
//! TOML files store tags as kebab-case strings (e.g. `"prompt-caching"`),
//! parsed at build time via [`FromStr`] with a hard compile error on unknown
//! tags. Runtime callers use [`Tag::as_str`] for display / serialisation.
//!
//! # Escape hatch
//!
//! For community-specific or experimental vocabulary not in this enum, types
//! also carry `extra_tags: &'static [&'static str]`. No validation — pure
//! passthrough.
//!
//! # Assignment rules (convention, not compiler-enforced)
//!
//! - Max ~8 tags per entry (more = noise)
//! - `Vision`/`Audio`/`Multimodal` — only ONE applies (most-specific wins)
//! - `Budget`/`Frontier` — mutually exclusive per model
//! - `ReadOnly`/`Destructive` — every MCP server MUST carry one
//! - `Official`/`Verified` — apply to MCP servers only
//! - `Embedding`/`Reranker` — apply to embedding entries only

/// Typed tag vocabulary for catalog entries.
///
/// 42 variants covering model I/O modalities, reasoning/generation behaviour,
/// economics, deployment/sovereignty, specialisation, domain, and MCP-server
/// permissioning.
///
/// Locked as an enum (not `&'static str`) so community pck authors get a
/// compile error on typos — migrating strings → enum at v0.92 would be a
/// breaking change for pck TOML authors.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Tag {
    // ─── Model I/O modalities ─────────────────────────────────────────────
    /// Image input (incl. OCR, video frames). Most-specific of the
    /// Vision/Audio/Multimodal group — assign this when a model is
    /// image-capable but NOT audio-capable.
    Vision,
    /// Audio input AND/OR output.
    Audio,
    /// Live voice API (`OpenAI` Realtime, `Gemini` Live). WebSocket protocol,
    /// distinct pricing model — more specific than `Audio`.
    Realtime,
    /// Generic image + audio + text input (covers multiple modalities).
    Multimodal,
    /// Can generate images (DALL-E, Imagen, fal.ai, etc.).
    ImageGen,
    /// Embedding model marker — for entries in the embeddings catalog.
    Embedding,
    /// Reranking model marker — for entries in the embeddings catalog.
    Reranker,

    // ─── Reasoning / generation behaviour ────────────────────────────────
    /// Chain-of-thought / thinking visible or controllable.
    Reasoning,
    /// Claude-specific `thinking_budget` parameter support.
    ExtendedThinking,
    /// Tool / function calling supported.
    FunctionCalling,
    /// Multiple tools per turn (most 2025+ models). Distinct from
    /// `FunctionCalling` — older models support tools but not in parallel.
    ParallelTools,
    /// Native JSON mode / `response_format` support.
    StructuredOutput,
    /// JSON schema constraint (stricter than plain JSON mode).
    ResponseSchema,
    /// SSE / chunked streaming output.
    Streaming,
    /// Context window ≥ 200 k tokens.
    LongContext,
    /// Anthropic / `OpenAI` / `Gemini` prompt-cache support (50-90 % cost
    /// savings on repeated prefix — big impact on Cortex projections).
    PromptCaching,
    /// Built-in web grounding (Perplexity Sonar, Gemini grounding).
    /// Distinct from `Rag` which is user-provided context.
    WebSearch,
    /// Built-in code interpreter / sandbox (Claude code execution,
    /// `OpenAI` code interpreter, `Gemini` code tool).
    CodeExecution,
    /// Truncatable / Matryoshka embedding (voyage-3, Gemini Embedding 001).
    /// Critical for vector store compat when stored dims < max dims.
    Matryoshka,

    // ─── Economics / performance tiers ───────────────────────────────────
    /// Under $1 / 1M input tokens.
    Budget,
    /// Top-tier quality. Mutually exclusive with `Budget`.
    Frontier,
    /// ≥ 500 tok/sec throughput (Groq, Cerebras, Hyperbolic).
    Fast,

    // ─── Deployment / sovereignty ─────────────────────────────────────────
    /// Runnable without API key (Ollama, native GGUF).
    Local,
    /// Pay-per-call, no warmup required (Modal, Replicate).
    Serverless,
    /// Open weights or open client library.
    OpenSource,
    /// SLA / SOC 2 / HIPAA / BAA available.
    Enterprise,
    /// EU data residency (Mistral, Nebius, Infomaniak, Scaleway).
    European,
    /// Mainland-China provider (Moonshot, Qwen, Z.ai, `DeepSeek`, 01.AI).
    Chinese,
    /// Optimised for Japanese language and market.
    Japanese,
    /// First-class support for ≥ 20 languages.
    Multilingual,

    // ─── Specialisation ───────────────────────────────────────────────────
    /// Code-optimised (Codestral, DeepSeek-Coder, GLM-4.6).
    Code,
    /// Math-specialised (DeepSeek-Math).
    Math,
    /// Retrieval-augmented-generation-optimised (user-provided context).
    Rag,
    /// Agentic workflow-optimised (multi-turn, tool-calling focus).
    Agent,

    // ─── Domain (embeddings / specialty models) ───────────────────────────
    /// Legal domain (voyage-law-2).
    Legal,
    /// Finance domain (voyage-finance-2).
    Finance,
    /// Medical / biomedical domain.
    Medical,

    // ─── MCP server permissioning (security filter) ───────────────────────
    /// MCP exposes only read-tools (safe for untrusted workflows). Every
    /// MCP server entry MUST carry `ReadOnly` XOR `Destructive`.
    ReadOnly,
    /// MCP can mutate external state (file writes, payments, DB writes).
    /// Every MCP server entry MUST carry `ReadOnly` XOR `Destructive`.
    Destructive,
    /// MCP runs in a sandboxed environment (Playwright, Browserbase).
    Sandbox,
    /// Published by the vendor themselves (Stripe-official, HubSpot-official).
    Official,
    /// Verified by the MCP registry (registry.modelcontextprotocol.io).
    Verified,
}

impl Tag {
    /// Kebab-case wire identifier, matching TOML and pck manifest schemas.
    ///
    /// This is the canonical string form used for serialisation, display, and
    /// TOML round-trips. The `FromStr` impl accepts exactly these strings.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vision => "vision",
            Self::Audio => "audio",
            Self::Realtime => "realtime",
            Self::Multimodal => "multimodal",
            Self::ImageGen => "image-gen",
            Self::Embedding => "embedding",
            Self::Reranker => "reranker",
            Self::Reasoning => "reasoning",
            Self::ExtendedThinking => "extended-thinking",
            Self::FunctionCalling => "function-calling",
            Self::ParallelTools => "parallel-tools",
            Self::StructuredOutput => "structured-output",
            Self::ResponseSchema => "response-schema",
            Self::Streaming => "streaming",
            Self::LongContext => "long-context",
            Self::PromptCaching => "prompt-caching",
            Self::WebSearch => "web-search",
            Self::CodeExecution => "code-execution",
            Self::Matryoshka => "matryoshka",
            Self::Budget => "budget",
            Self::Frontier => "frontier",
            Self::Fast => "fast",
            Self::Local => "local",
            Self::Serverless => "serverless",
            Self::OpenSource => "open-source",
            Self::Enterprise => "enterprise",
            Self::European => "european",
            Self::Chinese => "chinese",
            Self::Japanese => "japanese",
            Self::Multilingual => "multilingual",
            Self::Code => "code",
            Self::Math => "math",
            Self::Rag => "rag",
            Self::Agent => "agent",
            Self::Legal => "legal",
            Self::Finance => "finance",
            Self::Medical => "medical",
            Self::ReadOnly => "read-only",
            Self::Destructive => "destructive",
            Self::Sandbox => "sandbox",
            Self::Official => "official",
            Self::Verified => "verified",
        }
    }

    /// Parse a kebab-case tag string. Returns `None` for unknown strings.
    ///
    /// Thin `Option` wrapper over [`FromStr`]; use whichever fits the
    /// call-site. `build.rs` uses `FromStr` directly to get the error message
    /// with the offending input.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        <Self as core::str::FromStr>::from_str(s).ok()
    }
}

/// Error returned when parsing a [`Tag`] from a string fails.
///
/// Carries the offending input so callers can quote it in diagnostics.
/// `build.rs` uses this to emit a compile error with the exact unknown tag.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ParseTagError {
    /// The input string that failed to parse.
    pub input: String,
}

impl core::fmt::Display for ParseTagError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "unknown tag {:?} — not in the 42-variant Tag enum", self.input)
    }
}

impl std::error::Error for ParseTagError {}

impl core::str::FromStr for Tag {
    type Err = ParseTagError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "vision" => Self::Vision,
            "audio" => Self::Audio,
            "realtime" => Self::Realtime,
            "multimodal" => Self::Multimodal,
            "image-gen" => Self::ImageGen,
            "embedding" => Self::Embedding,
            "reranker" => Self::Reranker,
            "reasoning" => Self::Reasoning,
            "extended-thinking" => Self::ExtendedThinking,
            "function-calling" => Self::FunctionCalling,
            "parallel-tools" => Self::ParallelTools,
            "structured-output" => Self::StructuredOutput,
            "response-schema" => Self::ResponseSchema,
            "streaming" => Self::Streaming,
            "long-context" => Self::LongContext,
            "prompt-caching" => Self::PromptCaching,
            "web-search" => Self::WebSearch,
            "code-execution" => Self::CodeExecution,
            "matryoshka" => Self::Matryoshka,
            "budget" => Self::Budget,
            "frontier" => Self::Frontier,
            "fast" => Self::Fast,
            "local" => Self::Local,
            "serverless" => Self::Serverless,
            "open-source" => Self::OpenSource,
            "enterprise" => Self::Enterprise,
            "european" => Self::European,
            "chinese" => Self::Chinese,
            "japanese" => Self::Japanese,
            "multilingual" => Self::Multilingual,
            "code" => Self::Code,
            "math" => Self::Math,
            "rag" => Self::Rag,
            "agent" => Self::Agent,
            "legal" => Self::Legal,
            "finance" => Self::Finance,
            "medical" => Self::Medical,
            "read-only" => Self::ReadOnly,
            "destructive" => Self::Destructive,
            "sandbox" => Self::Sandbox,
            "official" => Self::Official,
            "verified" => Self::Verified,
            _ => {
                return Err(ParseTagError {
                    input: s.to_owned(),
                });
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exhaustive list of all 42 variants. Updated here when a new variant is
    /// added — CI fails if `as_str` / `FromStr` are out of sync.
    const ALL_TAGS: &[Tag] = &[
        // modalities
        Tag::Vision,
        Tag::Audio,
        Tag::Realtime,
        Tag::Multimodal,
        Tag::ImageGen,
        Tag::Embedding,
        Tag::Reranker,
        // reasoning / generation
        Tag::Reasoning,
        Tag::ExtendedThinking,
        Tag::FunctionCalling,
        Tag::ParallelTools,
        Tag::StructuredOutput,
        Tag::ResponseSchema,
        Tag::Streaming,
        Tag::LongContext,
        Tag::PromptCaching,
        Tag::WebSearch,
        Tag::CodeExecution,
        Tag::Matryoshka,
        // economics
        Tag::Budget,
        Tag::Frontier,
        Tag::Fast,
        // deployment / sovereignty
        Tag::Local,
        Tag::Serverless,
        Tag::OpenSource,
        Tag::Enterprise,
        Tag::European,
        Tag::Chinese,
        Tag::Japanese,
        Tag::Multilingual,
        // specialisation
        Tag::Code,
        Tag::Math,
        Tag::Rag,
        Tag::Agent,
        // domain
        Tag::Legal,
        Tag::Finance,
        Tag::Medical,
        // MCP permissioning
        Tag::ReadOnly,
        Tag::Destructive,
        Tag::Sandbox,
        Tag::Official,
        Tag::Verified,
    ];

    #[test]
    fn variant_count_is_42() {
        assert_eq!(ALL_TAGS.len(), 42, "Tag enum must have exactly 42 variants");
    }

    #[test]
    fn roundtrip_all_tags() {
        for tag in ALL_TAGS {
            let s = tag.as_str();
            let parsed = Tag::parse(s);
            assert_eq!(
                parsed,
                Some(*tag),
                "roundtrip failed: {tag:?}.as_str() = {s:?} did not parse back",
            );
        }
    }

    #[test]
    fn from_str_matches_parse() {
        for tag in ALL_TAGS {
            let s = tag.as_str();
            let via_from_str: Result<Tag, _> = s.parse();
            assert_eq!(
                via_from_str.ok(),
                Tag::parse(s),
                "FromStr and parse() disagree for {s:?}",
            );
        }
    }

    #[test]
    fn unknown_tag_returns_error_with_input() {
        let err = "yolo-mode".parse::<Tag>().unwrap_err();
        assert_eq!(err.input, "yolo-mode");
        assert!(err.to_string().contains("yolo-mode"));

        assert!("".parse::<Tag>().is_err());
        assert!("Vision".parse::<Tag>().is_err()); // case-sensitive: PascalCase rejected
        assert!("VISION".parse::<Tag>().is_err());
        assert!("open_source".parse::<Tag>().is_err()); // underscore, not hyphen
    }

    #[test]
    fn parse_is_case_sensitive_lowercase_only() {
        // Wire format is strictly lowercase kebab-case.
        assert!(Tag::parse("Vision").is_none());
        assert!(Tag::parse("REASONING").is_none());
        assert!(Tag::parse("Long-Context").is_none()); // mixed case
        assert!(Tag::parse("longcontext").is_none());  // missing hyphen
    }

    #[test]
    fn as_str_is_kebab_case_lowercase() {
        for tag in ALL_TAGS {
            let s = tag.as_str();
            assert!(!s.is_empty(), "{tag:?}.as_str() must not be empty");
            assert!(
                s.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "{tag:?}.as_str() = {s:?} must be kebab-case (lowercase + hyphens only)",
            );
        }
    }

    #[test]
    fn no_duplicate_wire_strings() {
        // Sort wire strings; duplicates will be adjacent.
        let mut wires: Vec<&'static str> = ALL_TAGS.iter().map(|t| t.as_str()).collect();
        wires.sort_unstable();
        for w in wires.windows(2) {
            assert_ne!(
                w[0],
                w[1],
                "duplicate wire string {:?} — two Tag variants map to the same kebab string",
                w[0],
            );
        }
    }

    #[test]
    fn specific_variants_spot_check() {
        assert_eq!(Tag::parse("prompt-caching"), Some(Tag::PromptCaching));
        assert_eq!(Tag::parse("extended-thinking"), Some(Tag::ExtendedThinking));
        assert_eq!(Tag::parse("open-source"), Some(Tag::OpenSource));
        assert_eq!(Tag::parse("read-only"), Some(Tag::ReadOnly));
        assert_eq!(Tag::parse("image-gen"), Some(Tag::ImageGen));
        assert_eq!(Tag::parse("code-execution"), Some(Tag::CodeExecution));

        assert_eq!(Tag::PromptCaching.as_str(), "prompt-caching");
        assert_eq!(Tag::OpenSource.as_str(), "open-source");
        assert_eq!(Tag::ReadOnly.as_str(), "read-only");
    }
}
