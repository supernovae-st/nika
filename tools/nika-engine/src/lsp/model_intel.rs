// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Model Intelligence for the Nika LSP.
//!
//! Provides a unified model catalog with pricing, capabilities, context windows,
//! deprecation status, and provider compatibility checking. This module is the
//! single source of truth for all model metadata used by hover, code actions,
//! and diagnostics.
//!
//! ## Design: Static Catalog
//!
//! Model metadata is compiled into the binary as a static slice. This is
//! intentional: an LSP must be instant and offline-capable. Model catalogs
//! change slowly (quarterly). The maintenance cost is identical to the
//! existing pricing tables in `provider::cost` which already require manual
//! updates.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use nika::lsp::model_intel::{ModelCatalog, CapabilityFlag};
//!
//! // Hover: get model info
//! let info = ModelCatalog::lookup("claude-sonnet-4-6").unwrap();
//! let markdown = info.hover_markdown();
//!
//! // Code action: find alternatives
//! let alternatives = ModelCatalog::alternatives_for("claude-opus-4");
//!
//! // Diagnostics: check compatibility
//! let issues = ModelCatalog::check_compatibility("gpt-4o-mini", &task_config);
//! ```
//!
//! ## Relationship to existing modules
//!
//! - `provider::cost` -- Pricing data. `model_intel` wraps and extends it with
//!   capabilities, context windows, and deprecation. Does NOT duplicate pricing.
//! - `core::models` -- Native/local GGUF model catalog. Orthogonal: those are
//!   local models with quantization. This module covers cloud API models.
//! - `ast::analyzer` -- Consumes `CompatibilityChecker` to emit NIKA-032/033
//!   diagnostics during Phase 2 analysis.

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::provider::cost::{self, ModelPricing, ProviderKind};

// ═══════════════════════════════════════════════════════════════════════════
// CAPABILITY FLAGS
// ═══════════════════════════════════════════════════════════════════════════

/// Capability flags for a model.
///
/// Implemented as a manual bitfield to avoid adding a `bitflags` dependency.
/// Each flag represents a feature the model supports. Used for compatibility
/// checking: if a workflow requests a capability the model does not have,
/// the analyzer emits a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CapabilityFlags(u32);

impl CapabilityFlags {
    /// Supports function/tool calling
    pub const TOOL_CALLING: Self = Self(1 << 0);
    /// Supports JSON mode / structured output
    pub const JSON_MODE: Self = Self(1 << 1);
    /// Supports extended thinking (Claude-specific)
    pub const EXTENDED_THINKING: Self = Self(1 << 2);
    /// Supports vision (image inputs)
    pub const VISION: Self = Self(1 << 3);
    /// Supports system prompts
    pub const SYSTEM_PROMPT: Self = Self(1 << 4);
    /// Supports streaming
    pub const STREAMING: Self = Self(1 << 5);
    /// Supports tool_choice: required
    pub const TOOL_CHOICE: Self = Self(1 << 6);
    /// Supports reasoning/chain-of-thought
    pub const REASONING: Self = Self(1 << 7);
    /// Supports code execution / code interpreter
    pub const CODE_EXECUTION: Self = Self(1 << 8);
    /// Supports PDF/document input
    pub const DOCUMENT_INPUT: Self = Self(1 << 9);

    /// Empty flags (no capabilities).
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Combine two flag sets (union).
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Flags in self that are NOT in other (difference).
    pub const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// Check if self contains all flags in other.
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Check if the flag set is empty.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// MODEL TIER
// ═══════════════════════════════════════════════════════════════════════════

/// Qualitative capability tier for cost/quality tradeoff messaging.
///
/// This is intentionally coarse. We do not pretend to have a universal
/// quality score -- tiers are for the "~5% quality reduction" messaging
/// in the cost optimizer, not for benchmark ranking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ModelTier {
    /// Budget: fast, cheap, good for simple tasks (gpt-4o-mini, haiku, flash)
    Budget,
    /// Standard: balanced cost/quality (sonnet, gpt-4o, mistral-large)
    Standard,
    /// Premium: highest quality, expensive (opus, o1, gemini-1.5-pro)
    Premium,
    /// Reasoning: specialized for complex reasoning (o1, o3, deepseek-reasoner)
    Reasoning,
}

impl ModelTier {
    /// Human-readable label for the tier.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Budget => "Budget",
            Self::Standard => "Standard",
            Self::Premium => "Premium",
            Self::Reasoning => "Reasoning",
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// DEPRECATION STATUS
// ═══════════════════════════════════════════════════════════════════════════

/// Model lifecycle status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleStatus {
    /// Model is active and fully supported.
    Active,
    /// Model is deprecated; a replacement is recommended.
    Deprecated {
        /// Sunset date as "YYYY-MM-DD" (None if unknown).
        sunset_date: Option<&'static str>,
        /// Recommended replacement model ID.
        replacement: &'static str,
    },
    /// Model is in preview (may change without notice).
    Preview,
}

// ═══════════════════════════════════════════════════════════════════════════
// MODEL INFO
// ═══════════════════════════════════════════════════════════════════════════

/// Complete metadata for a cloud API model.
///
/// This struct unifies pricing (from `cost.rs`), capabilities, context window,
/// and lifecycle information into a single lookup target.
#[derive(Debug, Clone)]
pub struct ModelInfo {
    /// Canonical model ID (e.g., "claude-sonnet-4-6")
    pub id: &'static str,

    /// Human-readable display name (e.g., "Claude Sonnet 4")
    pub display_name: &'static str,

    /// Provider
    pub provider: ProviderKind,

    /// Qualitative capability tier
    pub tier: ModelTier,

    /// Context window in tokens
    pub context_window: u32,

    /// Maximum output tokens (0 = provider default)
    pub max_output_tokens: u32,

    /// Capability flags
    pub capabilities: CapabilityFlags,

    /// Lifecycle status
    pub lifecycle: LifecycleStatus,

    /// Known aliases (e.g., "claude-sonnet-4-20250514" for "claude-sonnet-4-6")
    pub aliases: &'static [&'static str],

    /// Short description for hover
    pub description: &'static str,
}

impl ModelInfo {
    /// Get pricing from the existing cost module (no duplication).
    pub fn pricing(&self) -> ModelPricing {
        cost::get_model_pricing(self.provider, self.id)
    }

    /// Generate hover markdown for this model.
    ///
    /// Format:
    /// ```text
    /// ## claude-sonnet-4-6
    /// **Claude Sonnet 4** | Anthropic | Standard
    ///
    /// | Property | Value |
    /// |----------|-------|
    /// | Input    | $3.00 / 1M tokens |
    /// | Output   | $15.00 / 1M tokens |
    /// | Context  | 200K tokens |
    /// | Max out  | 8,192 tokens |
    ///
    /// **Capabilities:** tool calling, JSON mode, extended thinking, vision, streaming
    ///
    /// Balanced cost and quality for most Nika workflows.
    /// ```
    pub fn hover_markdown(&self) -> String {
        let pricing = self.pricing();
        let mut md = String::with_capacity(512);

        // Header
        md.push_str(&format!("## `{}`\n\n", self.id));
        md.push_str(&format!(
            "**{}** | {} | {}\n\n",
            self.display_name,
            self.provider.name(),
            self.tier.label(),
        ));

        // Pricing table
        md.push_str("| Property | Value |\n|----------|-------|\n");
        md.push_str(&format!(
            "| Input | ${:.2} / 1M tokens |\n",
            pricing.input_per_million
        ));
        md.push_str(&format!(
            "| Output | ${:.2} / 1M tokens |\n",
            pricing.output_per_million
        ));
        md.push_str(&format!(
            "| Context | {}K tokens |\n",
            self.context_window / 1_000
        ));
        if self.max_output_tokens > 0 {
            md.push_str(&format!(
                "| Max output | {} tokens |\n",
                format_number(self.max_output_tokens)
            ));
        }

        // Capabilities
        let caps = self.capability_labels();
        if !caps.is_empty() {
            md.push_str(&format!("\n**Capabilities:** {}\n", caps.join(", ")));
        }

        // Lifecycle warnings
        match &self.lifecycle {
            LifecycleStatus::Deprecated {
                sunset_date,
                replacement,
            } => {
                md.push_str("\n> **Warning:** This model is deprecated.");
                if let Some(date) = sunset_date {
                    md.push_str(&format!(" Sunset date: {}.", date));
                }
                md.push_str(&format!(" Consider `{}`.\n", replacement));
            }
            LifecycleStatus::Preview => {
                md.push_str("\n> **Note:** This model is in preview and may change.\n");
            }
            LifecycleStatus::Active => {}
        }

        // Description
        if !self.description.is_empty() {
            md.push_str(&format!("\n{}\n", self.description));
        }

        md
    }

    /// Get human-readable capability labels.
    pub fn capability_labels(&self) -> Vec<&'static str> {
        capability_labels_for(self.capabilities)
    }

    /// Check if this model is a valid replacement for another model.
    ///
    /// A model is compatible if it has at least the same capabilities.
    pub fn is_compatible_replacement_for(&self, other: &ModelInfo) -> bool {
        other.capabilities.difference(self.capabilities).is_empty()
    }
}

/// Get human-readable labels for a set of capability flags.
fn capability_labels_for(flags: CapabilityFlags) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(CapabilityFlags::TOOL_CALLING) {
        labels.push("tool calling");
    }
    if flags.contains(CapabilityFlags::JSON_MODE) {
        labels.push("JSON mode");
    }
    if flags.contains(CapabilityFlags::EXTENDED_THINKING) {
        labels.push("extended thinking");
    }
    if flags.contains(CapabilityFlags::VISION) {
        labels.push("vision");
    }
    if flags.contains(CapabilityFlags::SYSTEM_PROMPT) {
        labels.push("system prompt");
    }
    if flags.contains(CapabilityFlags::STREAMING) {
        labels.push("streaming");
    }
    if flags.contains(CapabilityFlags::TOOL_CHOICE) {
        labels.push("tool_choice");
    }
    if flags.contains(CapabilityFlags::REASONING) {
        labels.push("reasoning");
    }
    if flags.contains(CapabilityFlags::CODE_EXECUTION) {
        labels.push("code execution");
    }
    if flags.contains(CapabilityFlags::DOCUMENT_INPUT) {
        labels.push("document input");
    }
    labels
}

/// Format a number with comma separators.
fn format_number(n: u32) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

// ═══════════════════════════════════════════════════════════════════════════
// MODEL CATALOG (STATIC DATA)
// ═══════════════════════════════════════════════════════════════════════════

/// Standard capabilities shared by most modern models.
const STANDARD_CAPS: CapabilityFlags = CapabilityFlags::TOOL_CALLING
    .union(CapabilityFlags::JSON_MODE)
    .union(CapabilityFlags::SYSTEM_PROMPT)
    .union(CapabilityFlags::STREAMING);

/// Claude standard capabilities (STANDARD + extended thinking + vision + tool_choice).
const CLAUDE_CAPS: CapabilityFlags = STANDARD_CAPS
    .union(CapabilityFlags::EXTENDED_THINKING)
    .union(CapabilityFlags::VISION)
    .union(CapabilityFlags::TOOL_CHOICE)
    .union(CapabilityFlags::DOCUMENT_INPUT);

/// OpenAI GPT-4o capabilities (STANDARD + vision + tool_choice).
const GPT4O_CAPS: CapabilityFlags = STANDARD_CAPS
    .union(CapabilityFlags::VISION)
    .union(CapabilityFlags::TOOL_CHOICE);

/// OpenAI reasoning model capabilities (STANDARD + reasoning).
const O_SERIES_CAPS: CapabilityFlags = STANDARD_CAPS
    .union(CapabilityFlags::REASONING)
    .union(CapabilityFlags::VISION);

/// The complete model catalog.
///
/// Entries use the canonical model ID (the one users type in `model:` fields).
/// Aliases are listed in the `aliases` field for lookup resolution.
pub static MODEL_CATALOG: &[ModelInfo] = &[
    // ─────────────────────────────────────────────────────────────────────
    // ANTHROPIC
    // ─────────────────────────────────────────────────────────────────────
    ModelInfo {
        id: "claude-opus-4",
        display_name: "Claude Opus 4",
        provider: ProviderKind::Claude,
        tier: ModelTier::Premium,
        context_window: 200_000,
        max_output_tokens: 32_768,
        capabilities: CLAUDE_CAPS,
        lifecycle: LifecycleStatus::Active,
        aliases: &["claude-opus-4-20250514"],
        description: "Most capable Claude model. Best for complex analysis and creative tasks.",
    },
    ModelInfo {
        id: "claude-sonnet-4-6",
        display_name: "Claude Sonnet 4",
        provider: ProviderKind::Claude,
        tier: ModelTier::Standard,
        context_window: 200_000,
        max_output_tokens: 16_384,
        capabilities: CLAUDE_CAPS,
        lifecycle: LifecycleStatus::Active,
        aliases: &["claude-sonnet-4", "claude-sonnet-4-20250514"],
        description: "Balanced cost and quality for most Nika workflows.",
    },
    ModelInfo {
        id: "claude-3-5-sonnet-latest",
        display_name: "Claude 3.5 Sonnet",
        provider: ProviderKind::Claude,
        tier: ModelTier::Standard,
        context_window: 200_000,
        max_output_tokens: 8_192,
        capabilities: CLAUDE_CAPS,
        lifecycle: LifecycleStatus::Active,
        aliases: &["claude-3-5-sonnet-20241022"],
        description: "Previous generation Sonnet. Still excellent for most tasks.",
    },
    ModelInfo {
        id: "claude-3-5-haiku-latest",
        display_name: "Claude 3.5 Haiku",
        provider: ProviderKind::Claude,
        tier: ModelTier::Budget,
        context_window: 200_000,
        max_output_tokens: 8_192,
        capabilities: CLAUDE_CAPS,
        lifecycle: LifecycleStatus::Active,
        aliases: &["claude-3-5-haiku-20241022"],
        description:
            "Fast and affordable. Great for classification, extraction, and simple generation.",
    },
    ModelInfo {
        id: "claude-3-opus-latest",
        display_name: "Claude 3 Opus",
        provider: ProviderKind::Claude,
        tier: ModelTier::Premium,
        context_window: 200_000,
        max_output_tokens: 4_096,
        capabilities: CLAUDE_CAPS.difference(CapabilityFlags::EXTENDED_THINKING),
        lifecycle: LifecycleStatus::Deprecated {
            sunset_date: None,
            replacement: "claude-opus-4",
        },
        aliases: &["claude-3-opus-20240229"],
        description: "Previous generation Opus. Consider upgrading to Claude Opus 4.",
    },
    ModelInfo {
        id: "claude-3-haiku-20240307",
        display_name: "Claude 3 Haiku",
        provider: ProviderKind::Claude,
        tier: ModelTier::Budget,
        context_window: 200_000,
        max_output_tokens: 4_096,
        capabilities: STANDARD_CAPS.union(CapabilityFlags::VISION),
        lifecycle: LifecycleStatus::Deprecated {
            sunset_date: None,
            replacement: "claude-3-5-haiku-latest",
        },
        aliases: &[],
        description: "Legacy Haiku. Consider claude-3-5-haiku-latest.",
    },
    // ─────────────────────────────────────────────────────────────────────
    // OPENAI
    // ─────────────────────────────────────────────────────────────────────
    ModelInfo {
        id: "gpt-4o",
        display_name: "GPT-4o",
        provider: ProviderKind::OpenAI,
        tier: ModelTier::Standard,
        context_window: 128_000,
        max_output_tokens: 16_384,
        capabilities: GPT4O_CAPS,
        lifecycle: LifecycleStatus::Active,
        aliases: &["gpt-4o-2024-11-20"],
        description: "OpenAI's flagship multimodal model. Fast and capable.",
    },
    ModelInfo {
        id: "gpt-4o-mini",
        display_name: "GPT-4o Mini",
        provider: ProviderKind::OpenAI,
        tier: ModelTier::Budget,
        context_window: 128_000,
        max_output_tokens: 16_384,
        capabilities: GPT4O_CAPS,
        lifecycle: LifecycleStatus::Active,
        aliases: &["gpt-4o-mini-2024-07-18"],
        description: "Most cost-efficient OpenAI model. Great for high-volume tasks.",
    },
    ModelInfo {
        id: "gpt-4.1",
        display_name: "GPT-4.1",
        provider: ProviderKind::OpenAI,
        tier: ModelTier::Standard,
        context_window: 1_000_000,
        max_output_tokens: 32_768,
        capabilities: GPT4O_CAPS,
        lifecycle: LifecycleStatus::Active,
        aliases: &[],
        description: "OpenAI's latest flagship. 1M context, improved instruction following.",
    },
    ModelInfo {
        id: "gpt-4.1-mini",
        display_name: "GPT-4.1 Mini",
        provider: ProviderKind::OpenAI,
        tier: ModelTier::Budget,
        context_window: 1_000_000,
        max_output_tokens: 32_768,
        capabilities: GPT4O_CAPS,
        lifecycle: LifecycleStatus::Active,
        aliases: &[],
        description: "Cost-efficient GPT-4.1 variant. Best price/performance.",
    },
    ModelInfo {
        id: "gpt-4.1-nano",
        display_name: "GPT-4.1 Nano",
        provider: ProviderKind::OpenAI,
        tier: ModelTier::Budget,
        context_window: 1_000_000,
        max_output_tokens: 32_768,
        capabilities: GPT4O_CAPS,
        lifecycle: LifecycleStatus::Active,
        aliases: &[],
        description: "Smallest GPT-4.1. Ultra-fast, ultra-cheap.",
    },
    ModelInfo {
        id: "gpt-4-turbo",
        display_name: "GPT-4 Turbo",
        provider: ProviderKind::OpenAI,
        tier: ModelTier::Standard,
        context_window: 128_000,
        max_output_tokens: 4_096,
        capabilities: GPT4O_CAPS,
        lifecycle: LifecycleStatus::Deprecated {
            sunset_date: Some("2025-06-01"),
            replacement: "gpt-4o",
        },
        aliases: &["gpt-4-turbo-2024-04-09", "gpt-4-turbo-preview"],
        description: "Legacy GPT-4. Consider gpt-4o for better performance and lower cost.",
    },
    ModelInfo {
        id: "o1",
        display_name: "o1",
        provider: ProviderKind::OpenAI,
        tier: ModelTier::Reasoning,
        context_window: 200_000,
        max_output_tokens: 100_000,
        capabilities: O_SERIES_CAPS,
        lifecycle: LifecycleStatus::Active,
        aliases: &["o1-2024-12-17"],
        description: "OpenAI's reasoning model. Best for math, science, and complex analysis.",
    },
    ModelInfo {
        id: "o1-mini",
        display_name: "o1 Mini",
        provider: ProviderKind::OpenAI,
        tier: ModelTier::Reasoning,
        context_window: 128_000,
        max_output_tokens: 65_536,
        capabilities: O_SERIES_CAPS.difference(CapabilityFlags::VISION),
        lifecycle: LifecycleStatus::Active,
        aliases: &["o1-mini-2024-09-12"],
        description: "Smaller reasoning model. Good balance of reasoning and cost.",
    },
    ModelInfo {
        id: "o3-mini",
        display_name: "o3 Mini",
        provider: ProviderKind::OpenAI,
        tier: ModelTier::Reasoning,
        context_window: 200_000,
        max_output_tokens: 100_000,
        capabilities: O_SERIES_CAPS,
        lifecycle: LifecycleStatus::Active,
        aliases: &["o3-mini-2025-01-31"],
        description: "Latest compact reasoning model from OpenAI.",
    },
    ModelInfo {
        id: "gpt-3.5-turbo",
        display_name: "GPT-3.5 Turbo",
        provider: ProviderKind::OpenAI,
        tier: ModelTier::Budget,
        context_window: 16_385,
        max_output_tokens: 4_096,
        capabilities: STANDARD_CAPS,
        lifecycle: LifecycleStatus::Deprecated {
            sunset_date: None,
            replacement: "gpt-4o-mini",
        },
        aliases: &["gpt-3.5-turbo-0125"],
        description: "Legacy model. gpt-4o-mini is cheaper and more capable.",
    },
    // ─────────────────────────────────────────────────────────────────────
    // GOOGLE
    // ─────────────────────────────────────────────────────────────────────
    ModelInfo {
        id: "gemini-2.0-flash",
        display_name: "Gemini 2.0 Flash",
        provider: ProviderKind::Gemini,
        tier: ModelTier::Budget,
        context_window: 1_000_000,
        max_output_tokens: 8_192,
        capabilities: STANDARD_CAPS.union(CapabilityFlags::VISION),
        lifecycle: LifecycleStatus::Active,
        aliases: &[],
        description: "Google's fastest model. 1M context window at rock-bottom pricing.",
    },
    ModelInfo {
        id: "gemini-1.5-pro",
        display_name: "Gemini 1.5 Pro",
        provider: ProviderKind::Gemini,
        tier: ModelTier::Standard,
        context_window: 2_000_000,
        max_output_tokens: 8_192,
        capabilities: STANDARD_CAPS
            .union(CapabilityFlags::VISION)
            .union(CapabilityFlags::DOCUMENT_INPUT),
        lifecycle: LifecycleStatus::Active,
        aliases: &["gemini-1.5-pro-latest"],
        description: "Google's most capable model. 2M context for massive documents.",
    },
    ModelInfo {
        id: "gemini-1.5-flash",
        display_name: "Gemini 1.5 Flash",
        provider: ProviderKind::Gemini,
        tier: ModelTier::Budget,
        context_window: 1_000_000,
        max_output_tokens: 8_192,
        capabilities: STANDARD_CAPS.union(CapabilityFlags::VISION),
        lifecycle: LifecycleStatus::Active,
        aliases: &["gemini-1.5-flash-latest"],
        description: "Fast and affordable with 1M context window.",
    },
    // ─────────────────────────────────────────────────────────────────────
    // MISTRAL
    // ─────────────────────────────────────────────────────────────────────
    ModelInfo {
        id: "mistral-large-latest",
        display_name: "Mistral Large",
        provider: ProviderKind::Mistral,
        tier: ModelTier::Standard,
        context_window: 128_000,
        max_output_tokens: 8_192,
        capabilities: STANDARD_CAPS.union(CapabilityFlags::TOOL_CHOICE),
        lifecycle: LifecycleStatus::Active,
        aliases: &["mistral-large-2411"],
        description: "Mistral's most capable model. Strong multilingual support.",
    },
    ModelInfo {
        id: "mistral-small-latest",
        display_name: "Mistral Small",
        provider: ProviderKind::Mistral,
        tier: ModelTier::Budget,
        context_window: 128_000,
        max_output_tokens: 8_192,
        capabilities: STANDARD_CAPS,
        lifecycle: LifecycleStatus::Active,
        aliases: &["mistral-small-2409"],
        description: "Cost-effective for simple tasks. Good latency.",
    },
    ModelInfo {
        id: "codestral-latest",
        display_name: "Codestral",
        provider: ProviderKind::Mistral,
        tier: ModelTier::Standard,
        context_window: 256_000,
        max_output_tokens: 8_192,
        capabilities: STANDARD_CAPS.union(CapabilityFlags::CODE_EXECUTION),
        lifecycle: LifecycleStatus::Active,
        aliases: &["codestral-2501"],
        description: "Specialized for code generation. 256K context.",
    },
    ModelInfo {
        id: "ministral-8b-latest",
        display_name: "Ministral 8B",
        provider: ProviderKind::Mistral,
        tier: ModelTier::Budget,
        context_window: 128_000,
        max_output_tokens: 8_192,
        capabilities: STANDARD_CAPS,
        lifecycle: LifecycleStatus::Active,
        aliases: &[],
        description: "Ultra-low-cost Mistral model for high-volume simple tasks.",
    },
    // ─────────────────────────────────────────────────────────────────────
    // GROQ (hosted open-source, ultra-fast)
    // ─────────────────────────────────────────────────────────────────────
    ModelInfo {
        id: "llama-3.3-70b-versatile",
        display_name: "Llama 3.3 70B",
        provider: ProviderKind::Groq,
        tier: ModelTier::Standard,
        context_window: 128_000,
        max_output_tokens: 32_768,
        capabilities: STANDARD_CAPS.union(CapabilityFlags::TOOL_CHOICE),
        lifecycle: LifecycleStatus::Active,
        aliases: &[],
        description: "Meta's Llama 3.3 on Groq hardware. Extremely fast inference.",
    },
    ModelInfo {
        id: "llama-3.1-8b-instant",
        display_name: "Llama 3.1 8B",
        provider: ProviderKind::Groq,
        tier: ModelTier::Budget,
        context_window: 128_000,
        max_output_tokens: 8_192,
        capabilities: STANDARD_CAPS,
        lifecycle: LifecycleStatus::Active,
        aliases: &[],
        description: "Ultra-fast small model on Groq. Sub-100ms latency.",
    },
    ModelInfo {
        id: "mixtral-8x7b-32768",
        display_name: "Mixtral 8x7B",
        provider: ProviderKind::Groq,
        tier: ModelTier::Budget,
        context_window: 32_768,
        max_output_tokens: 8_192,
        capabilities: STANDARD_CAPS,
        lifecycle: LifecycleStatus::Active,
        aliases: &[],
        description: "Mixtral MoE on Groq. Good quality at low cost.",
    },
    // ─────────────────────────────────────────────────────────────────────
    // DEEPSEEK
    // ─────────────────────────────────────────────────────────────────────
    ModelInfo {
        id: "deepseek-chat",
        display_name: "DeepSeek Chat",
        provider: ProviderKind::DeepSeek,
        tier: ModelTier::Budget,
        context_window: 128_000,
        max_output_tokens: 8_192,
        capabilities: STANDARD_CAPS,
        lifecycle: LifecycleStatus::Active,
        aliases: &[],
        description: "DeepSeek's general chat model. Extremely affordable.",
    },
    ModelInfo {
        id: "deepseek-reasoner",
        display_name: "DeepSeek Reasoner",
        provider: ProviderKind::DeepSeek,
        tier: ModelTier::Reasoning,
        context_window: 128_000,
        max_output_tokens: 8_192,
        capabilities: STANDARD_CAPS.union(CapabilityFlags::REASONING),
        lifecycle: LifecycleStatus::Active,
        aliases: &[],
        description: "DeepSeek's reasoning model. Strong at math and logic.",
    },
    // ─────────────────────────────────────────────────────────────────────
    // XAI (GROK)
    // ─────────────────────────────────────────────────────────────────────
    ModelInfo {
        id: "grok-3",
        display_name: "Grok 3",
        provider: ProviderKind::XAi,
        tier: ModelTier::Premium,
        context_window: 131_072,
        max_output_tokens: 32_768,
        capabilities: GPT4O_CAPS,
        lifecycle: LifecycleStatus::Active,
        aliases: &[],
        description: "xAI's flagship model. Competitive with GPT-4o and Claude Sonnet.",
    },
    ModelInfo {
        id: "grok-3-fast",
        display_name: "Grok 3 Fast",
        provider: ProviderKind::XAi,
        tier: ModelTier::Standard,
        context_window: 131_072,
        max_output_tokens: 32_768,
        capabilities: GPT4O_CAPS,
        lifecycle: LifecycleStatus::Active,
        aliases: &[],
        description: "Faster Grok 3 variant. Lower latency, slightly reduced quality.",
    },
    ModelInfo {
        id: "grok-3-mini",
        display_name: "Grok 3 Mini",
        provider: ProviderKind::XAi,
        tier: ModelTier::Budget,
        context_window: 131_072,
        max_output_tokens: 32_768,
        capabilities: STANDARD_CAPS,
        lifecycle: LifecycleStatus::Active,
        aliases: &[],
        description: "Small, efficient Grok variant. Good for high-volume tasks.",
    },
    ModelInfo {
        id: "grok-3-mini-fast",
        display_name: "Grok 3 Mini Fast",
        provider: ProviderKind::XAi,
        tier: ModelTier::Budget,
        context_window: 131_072,
        max_output_tokens: 32_768,
        capabilities: STANDARD_CAPS,
        lifecycle: LifecycleStatus::Active,
        aliases: &[],
        description: "Fastest Grok variant. Ultra-cheap for simple tasks.",
    },
];

// ═══════════════════════════════════════════════════════════════════════════
// MODEL CATALOG (LOOKUP)
// ═══════════════════════════════════════════════════════════════════════════

/// Index: model_id/alias -> index into MODEL_CATALOG.
static CATALOG_INDEX: LazyLock<HashMap<&'static str, usize>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    for (idx, info) in MODEL_CATALOG.iter().enumerate() {
        map.insert(info.id, idx);
        for alias in info.aliases {
            map.insert(alias, idx);
        }
    }
    map
});

/// The model catalog: lookup, search, and comparison.
pub struct ModelCatalog;

impl ModelCatalog {
    /// Look up a model by ID or alias.
    pub fn lookup(model_id: &str) -> Option<&'static ModelInfo> {
        CATALOG_INDEX.get(model_id).map(|&idx| &MODEL_CATALOG[idx])
    }

    /// Find all models from the same provider.
    pub fn models_for_provider(provider: ProviderKind) -> Vec<&'static ModelInfo> {
        MODEL_CATALOG
            .iter()
            .filter(|m| m.provider == provider)
            .collect()
    }

    /// Find alternative models for a given model ID.
    ///
    /// Returns models sorted by estimated cost (cheapest first), grouped by provider.
    /// Only includes active models (not deprecated).
    pub fn alternatives_for(model_id: &str) -> Vec<ModelAlternative> {
        let current = match Self::lookup(model_id) {
            Some(m) => m,
            None => return Vec::new(),
        };

        let current_pricing = current.pricing();
        // Reference cost for a "standard task": 1K input, 500 output tokens
        let current_cost = current_pricing.calculate(1_000, 500);

        let mut alternatives: Vec<ModelAlternative> = MODEL_CATALOG
            .iter()
            .filter(|m| {
                m.id != current.id && !matches!(m.lifecycle, LifecycleStatus::Deprecated { .. })
            })
            .map(|m| {
                let alt_pricing = m.pricing();
                let alt_cost = alt_pricing.calculate(1_000, 500);

                let cost_delta_pct = if current_cost > 0.0 {
                    ((alt_cost - current_cost) / current_cost) * 100.0
                } else {
                    0.0
                };

                let capability_warning = if m.is_compatible_replacement_for(current) {
                    None
                } else {
                    let missing = current.capabilities.difference(m.capabilities);
                    let labels = capability_labels_for(missing);
                    if labels.is_empty() {
                        None
                    } else {
                        Some(format!("Missing: {}", labels.join(", ")))
                    }
                };

                let tier_delta = if m.tier < current.tier {
                    Some("lower capability tier".to_string())
                } else if m.tier > current.tier {
                    Some("higher capability tier".to_string())
                } else {
                    None
                };

                ModelAlternative {
                    model: m,
                    cost_delta_pct,
                    capability_warning,
                    tier_delta,
                }
            })
            .collect();

        // Sort: same provider first, then by cost
        alternatives.sort_by(|a, b| {
            let same_provider_a = a.model.provider == current.provider;
            let same_provider_b = b.model.provider == current.provider;
            same_provider_b.cmp(&same_provider_a).then(
                a.cost_delta_pct
                    .partial_cmp(&b.cost_delta_pct)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
        });

        alternatives
    }

    /// Get all models (for completions).
    pub fn all_models() -> &'static [ModelInfo] {
        MODEL_CATALOG
    }

    /// Detect the provider for a model ID.
    pub fn detect_provider(model_id: &str) -> Option<ProviderKind> {
        Self::lookup(model_id).map(|m| m.provider)
    }
}

/// A model alternative with cost and capability comparison.
#[derive(Debug)]
pub struct ModelAlternative {
    /// The alternative model.
    pub model: &'static ModelInfo,
    /// Cost delta as percentage (negative = cheaper).
    pub cost_delta_pct: f64,
    /// Capability warning if capabilities are lost.
    pub capability_warning: Option<String>,
    /// Tier change description.
    pub tier_delta: Option<String>,
}

impl ModelAlternative {
    /// Generate a one-line summary for code action menu.
    ///
    /// Examples:
    /// - "Switch to gpt-4o-mini (OpenAI): save 94% | Budget tier"
    /// - "Switch to claude-3-5-haiku-latest (Claude): save 73% | Budget tier"
    /// - "Switch to gemini-2.0-flash (Gemini): save 97% | Missing: extended thinking"
    pub fn code_action_title(&self) -> String {
        let mut title = format!(
            "Switch to {} ({})",
            self.model.id,
            self.model.provider.name()
        );

        if self.cost_delta_pct < -1.0 {
            title.push_str(&format!(": save {:.0}%", -self.cost_delta_pct));
        } else if self.cost_delta_pct > 1.0 {
            title.push_str(&format!(": +{:.0}% cost", self.cost_delta_pct));
        }

        if let Some(ref warning) = self.capability_warning {
            title.push_str(&format!(" | {}", warning));
        } else if let Some(ref tier) = self.tier_delta {
            title.push_str(&format!(" | {}", tier));
        }

        title
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// COMPATIBILITY CHECKER
// ═══════════════════════════════════════════════════════════════════════════

/// A compatibility issue found during analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatibilityIssue {
    /// NIKA error code (e.g., "NIKA-032")
    pub code: &'static str,
    /// Severity: error or warning
    pub severity: IssueSeverity,
    /// Human-readable message
    pub message: String,
}

/// Issue severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueSeverity {
    Error,
    Warning,
}

/// Task configuration extracted from the analyzed AST for compatibility checking.
///
/// The analyzer constructs this from `AnalyzedTask` + `AnalyzedInferAction` /
/// `AnalyzedAgentAction` fields. This keeps model_intel independent of the AST types.
#[derive(Debug, Default)]
pub struct TaskModelConfig {
    /// The model ID from the `model:` field
    pub model_id: Option<String>,
    /// The provider from `provider:` field
    pub provider: Option<String>,
    /// Whether `extended_thinking: true` is set
    pub extended_thinking: bool,
    /// Whether `output.format: json` is set
    pub json_output: bool,
    /// Whether `tool_choice: required` is set (for agent)
    pub tool_choice_required: bool,
}

/// Check a task's model configuration for compatibility issues.
///
/// Returns a list of issues (errors and warnings). Called by the analyzer
/// during Phase 2, and by the LSP for real-time feedback.
pub fn check_compatibility(config: &TaskModelConfig) -> Vec<CompatibilityIssue> {
    let mut issues = Vec::new();

    let model_id = match config.model_id.as_deref() {
        Some(id) => id,
        None => return issues, // No model specified, nothing to check
    };

    let model = match ModelCatalog::lookup(model_id) {
        Some(m) => m,
        None => return issues, // Unknown model, not our problem here
    };

    // NIKA-032: Extended thinking with non-Claude model
    if config.extended_thinking
        && !model
            .capabilities
            .contains(CapabilityFlags::EXTENDED_THINKING)
    {
        issues.push(CompatibilityIssue {
            code: "NIKA-032",
            severity: IssueSeverity::Error,
            message: format!(
                "`extended_thinking: true` is not supported by `{}`. Only Claude models support extended thinking.",
                model.id
            ),
        });
    }

    // NIKA-032: JSON mode with model that doesn't support it
    if config.json_output && !model.capabilities.contains(CapabilityFlags::JSON_MODE) {
        issues.push(CompatibilityIssue {
            code: "NIKA-032",
            severity: IssueSeverity::Warning,
            message: format!(
                "`output: json` may not be reliably supported by `{}`. Consider a model with native JSON mode.",
                model.id
            ),
        });
    }

    // NIKA-032: tool_choice: required with provider that doesn't support it
    if config.tool_choice_required && !model.capabilities.contains(CapabilityFlags::TOOL_CHOICE) {
        issues.push(CompatibilityIssue {
            code: "NIKA-032",
            severity: IssueSeverity::Warning,
            message: format!(
                "`tool_choice: required` is not supported by `{}`. The model may not always call tools.",
                model.id
            ),
        });
    }

    // NIKA-033: Deprecated model
    if let LifecycleStatus::Deprecated {
        sunset_date,
        replacement,
    } = &model.lifecycle
    {
        let mut msg = format!("`{}` is deprecated.", model.id);
        if let Some(date) = sunset_date {
            msg.push_str(&format!(" Sunset date: {}.", date));
        }
        msg.push_str(&format!(" Consider `{}`.", replacement));

        issues.push(CompatibilityIssue {
            code: "NIKA-033",
            severity: IssueSeverity::Warning,
            message: msg,
        });
    }

    issues
}

// ═══════════════════════════════════════════════════════════════════════════
// WORKFLOW COST OPTIMIZER
// ═══════════════════════════════════════════════════════════════════════════

/// A cost optimization suggestion for a workflow.
#[derive(Debug)]
pub struct CostOptimization {
    /// Task ID
    pub task_id: String,
    /// Current model
    pub current_model: &'static str,
    /// Suggested replacement
    pub suggested_model: &'static str,
    /// Estimated savings percentage
    pub savings_pct: f64,
    /// Quality impact description
    pub quality_note: String,
}

/// Analyze a workflow's model usage and suggest cost optimizations.
///
/// Takes a list of (task_id, model_id) pairs and returns suggestions.
pub fn optimize_workflow_costs(tasks: &[(&str, &str)]) -> Vec<CostOptimization> {
    let mut suggestions = Vec::new();

    for (task_id, model_id) in tasks {
        let current = match ModelCatalog::lookup(model_id) {
            Some(m) => m,
            None => continue,
        };

        // Only suggest downgrades for Premium/Reasoning tier
        if current.tier != ModelTier::Premium && current.tier != ModelTier::Reasoning {
            continue;
        }

        // Find the cheapest compatible model from the same provider
        let current_pricing = current.pricing();
        let current_cost = current_pricing.calculate(1_000, 500);

        let same_provider_cheaper: Option<&ModelInfo> = MODEL_CATALOG
            .iter()
            .filter(|m| {
                m.provider == current.provider
                    && m.id != current.id
                    && m.tier < current.tier
                    && !matches!(m.lifecycle, LifecycleStatus::Deprecated { .. })
                    && m.is_compatible_replacement_for(current)
            })
            .min_by(|a, b| {
                let ca = a.pricing().calculate(1_000, 500);
                let cb = b.pricing().calculate(1_000, 500);
                ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
            });

        if let Some(cheaper) = same_provider_cheaper {
            let cheaper_cost = cheaper.pricing().calculate(1_000, 500);
            let savings = if current_cost > 0.0 {
                ((current_cost - cheaper_cost) / current_cost) * 100.0
            } else {
                0.0
            };

            if savings > 10.0 {
                suggestions.push(CostOptimization {
                    task_id: task_id.to_string(),
                    current_model: current.id,
                    suggested_model: cheaper.id,
                    savings_pct: savings,
                    quality_note: format!(
                        "{} tier -> {} tier",
                        current.tier.label(),
                        cheaper.tier.label()
                    ),
                });
            }
        }
    }

    suggestions
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ───────────────────────────────────────────────────────────────────────
    // Catalog lookup
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn lookup_canonical_id() {
        let info = ModelCatalog::lookup("claude-sonnet-4-6").unwrap();
        assert_eq!(info.display_name, "Claude Sonnet 4");
        assert_eq!(info.provider, ProviderKind::Claude);
        assert_eq!(info.context_window, 200_000);
    }

    #[test]
    fn lookup_alias() {
        let info = ModelCatalog::lookup("claude-sonnet-4-20250514").unwrap();
        assert_eq!(info.id, "claude-sonnet-4-6");
    }

    #[test]
    fn lookup_unknown_returns_none() {
        assert!(ModelCatalog::lookup("nonexistent-model").is_none());
    }

    #[test]
    fn all_catalog_ids_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for model in MODEL_CATALOG {
            assert!(seen.insert(model.id), "Duplicate catalog ID: {}", model.id);
        }
    }

    #[test]
    fn all_aliases_resolve_to_correct_model() {
        for model in MODEL_CATALOG {
            for alias in model.aliases {
                let resolved = ModelCatalog::lookup(alias).unwrap();
                assert_eq!(
                    resolved.id, model.id,
                    "Alias '{}' resolved to '{}' instead of '{}'",
                    alias, resolved.id, model.id
                );
            }
        }
    }

    // ───────────────────────────────────────────────────────────────────────
    // Pricing integration
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn pricing_matches_cost_module() {
        let info = ModelCatalog::lookup("claude-sonnet-4-6").unwrap();
        let pricing = info.pricing();
        assert!((pricing.input_per_million - 3.0).abs() < 0.001);
        assert!((pricing.output_per_million - 15.0).abs() < 0.001);
    }

    #[test]
    fn pricing_gpt4o_mini() {
        let info = ModelCatalog::lookup("gpt-4o-mini").unwrap();
        let pricing = info.pricing();
        assert!((pricing.input_per_million - 0.15).abs() < 0.001);
    }

    // ───────────────────────────────────────────────────────────────────────
    // Hover markdown
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn hover_markdown_contains_essentials() {
        let info = ModelCatalog::lookup("claude-sonnet-4-6").unwrap();
        let md = info.hover_markdown();
        assert!(md.contains("Claude Sonnet 4"), "Missing display name");
        assert!(md.contains("$3.00"), "Missing input pricing");
        assert!(md.contains("$15.00"), "Missing output pricing");
        assert!(md.contains("200K"), "Missing context window");
        assert!(md.contains("extended thinking"), "Missing capability");
    }

    #[test]
    fn hover_markdown_deprecated_model_shows_warning() {
        let info = ModelCatalog::lookup("gpt-4-turbo").unwrap();
        let md = info.hover_markdown();
        assert!(md.contains("Warning"), "Missing deprecation warning");
        assert!(md.contains("gpt-4o"), "Missing replacement suggestion");
    }

    // ───────────────────────────────────────────────────────────────────────
    // Alternatives
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn alternatives_for_opus_includes_sonnet() {
        let alts = ModelCatalog::alternatives_for("claude-opus-4");
        assert!(
            alts.iter().any(|a| a.model.id == "claude-sonnet-4-6"),
            "Should suggest Sonnet as alternative to Opus"
        );
    }

    #[test]
    fn alternatives_exclude_deprecated() {
        let alts = ModelCatalog::alternatives_for("claude-sonnet-4-6");
        assert!(
            !alts
                .iter()
                .any(|a| matches!(a.model.lifecycle, LifecycleStatus::Deprecated { .. })),
            "Should not suggest deprecated models"
        );
    }

    #[test]
    fn alternatives_for_unknown_returns_empty() {
        let alts = ModelCatalog::alternatives_for("nonexistent");
        assert!(alts.is_empty());
    }

    #[test]
    fn alternative_code_action_title_shows_savings() {
        let alts = ModelCatalog::alternatives_for("claude-opus-4");
        let sonnet = alts
            .iter()
            .find(|a| a.model.id == "claude-sonnet-4-6")
            .expect("Sonnet should be an alternative");
        let title = sonnet.code_action_title();
        assert!(
            title.contains("save") || title.contains("Switch"),
            "Title should show savings: {}",
            title
        );
    }

    // ───────────────────────────────────────────────────────────────────────
    // Compatibility checking
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn extended_thinking_with_non_claude_is_error() {
        let config = TaskModelConfig {
            model_id: Some("gpt-4o".to_string()),
            extended_thinking: true,
            ..Default::default()
        };
        let issues = check_compatibility(&config);
        assert!(
            issues
                .iter()
                .any(|i| i.code == "NIKA-032" && i.severity == IssueSeverity::Error),
            "Should flag extended_thinking with non-Claude model"
        );
    }

    #[test]
    fn extended_thinking_with_claude_is_ok() {
        let config = TaskModelConfig {
            model_id: Some("claude-sonnet-4-6".to_string()),
            extended_thinking: true,
            ..Default::default()
        };
        let issues = check_compatibility(&config);
        assert!(
            !issues
                .iter()
                .any(|i| i.code == "NIKA-032" && i.severity == IssueSeverity::Error),
            "Should NOT flag extended_thinking with Claude"
        );
    }

    #[test]
    fn deprecated_model_emits_warning() {
        let config = TaskModelConfig {
            model_id: Some("gpt-4-turbo".to_string()),
            ..Default::default()
        };
        let issues = check_compatibility(&config);
        assert!(
            issues.iter().any(|i| i.code == "NIKA-033"),
            "Should warn about deprecated model"
        );
    }

    #[test]
    fn active_model_no_deprecation_warning() {
        let config = TaskModelConfig {
            model_id: Some("gpt-4o".to_string()),
            ..Default::default()
        };
        let issues = check_compatibility(&config);
        assert!(
            !issues.iter().any(|i| i.code == "NIKA-033"),
            "Should NOT warn about active model"
        );
    }

    #[test]
    fn no_model_no_issues() {
        let config = TaskModelConfig::default();
        let issues = check_compatibility(&config);
        assert!(issues.is_empty());
    }

    // ───────────────────────────────────────────────────────────────────────
    // Workflow cost optimizer
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn optimizer_suggests_downgrade_for_premium() {
        let tasks = vec![("research", "claude-opus-4")];
        let suggestions = optimize_workflow_costs(&tasks);
        assert!(!suggestions.is_empty(), "Should suggest downgrade for Opus");
        assert!(
            suggestions[0].savings_pct > 50.0,
            "Savings should be significant"
        );
    }

    #[test]
    fn optimizer_no_suggestion_for_budget() {
        let tasks = vec![("classify", "gpt-4o-mini")];
        let suggestions = optimize_workflow_costs(&tasks);
        assert!(
            suggestions.is_empty(),
            "Should not suggest downgrade for Budget tier"
        );
    }

    // ───────────────────────────────────────────────────────────────────────
    // Capability flags
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn claude_has_extended_thinking() {
        let info = ModelCatalog::lookup("claude-sonnet-4-6").unwrap();
        assert!(info
            .capabilities
            .contains(CapabilityFlags::EXTENDED_THINKING));
    }

    #[test]
    fn gpt4o_does_not_have_extended_thinking() {
        let info = ModelCatalog::lookup("gpt-4o").unwrap();
        assert!(!info
            .capabilities
            .contains(CapabilityFlags::EXTENDED_THINKING));
    }

    #[test]
    fn compatibility_check_supersets() {
        let opus = ModelCatalog::lookup("claude-opus-4").unwrap();
        let sonnet = ModelCatalog::lookup("claude-sonnet-4-6").unwrap();
        // Sonnet has the same caps as Opus (both CLAUDE_CAPS)
        assert!(sonnet.is_compatible_replacement_for(opus));
    }

    // ───────────────────────────────────────────────────────────────────────
    // Format helpers
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn format_number_small() {
        assert_eq!(format_number(100), "100");
    }

    #[test]
    fn format_number_thousands() {
        assert_eq!(format_number(8_192), "8,192");
    }

    #[test]
    fn format_number_large() {
        assert_eq!(format_number(200_000), "200,000");
    }

    // ───────────────────────────────────────────────────────────────────────
    // Provider detection
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn detect_provider_claude() {
        assert_eq!(
            ModelCatalog::detect_provider("claude-sonnet-4-6"),
            Some(ProviderKind::Claude)
        );
    }

    #[test]
    fn detect_provider_openai() {
        assert_eq!(
            ModelCatalog::detect_provider("gpt-4o"),
            Some(ProviderKind::OpenAI)
        );
    }

    #[test]
    fn detect_provider_unknown() {
        assert_eq!(ModelCatalog::detect_provider("unknown-model"), None);
    }

    // ───────────────────────────────────────────────────────────────────────
    // ModelTier ordering
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn tier_ordering() {
        assert!(ModelTier::Budget < ModelTier::Standard);
        assert!(ModelTier::Standard < ModelTier::Premium);
    }

    // ───────────────────────────────────────────────────────────────────────
    // Catalog completeness
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn all_providers_have_models() {
        for provider in [
            ProviderKind::Claude,
            ProviderKind::OpenAI,
            ProviderKind::Gemini,
            ProviderKind::Mistral,
            ProviderKind::Groq,
            ProviderKind::DeepSeek,
        ] {
            let models = ModelCatalog::models_for_provider(provider);
            assert!(
                !models.is_empty(),
                "Provider {:?} has no models in catalog",
                provider
            );
        }
    }
}
