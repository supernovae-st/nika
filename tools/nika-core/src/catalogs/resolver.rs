//! ModelResolver — centralized model resolution for all execution paths.
//!
//! Eliminates 12 model routing issues by providing a single source of truth
//! for provider default models, model-provider compatibility validation,
//! and fallback chain model substitution.

use crate::catalogs::providers::find_provider;

/// Single source of truth for provider default models.
///
/// Every consumer (RigProvider::default_model, TUI routing, cost tracking)
/// must use this table instead of hardcoding model names.
pub static PROVIDER_DEFAULTS: &[(&str, &str)] = &[
    // rig-core providers
    ("anthropic", "claude-sonnet-4-6"),
    ("openai", "gpt-4o"),
    ("mistral", "mistral-large-latest"),
    ("groq", "llama-3.3-70b-versatile"),
    ("deepseek", "deepseek-chat"),
    ("gemini", "gemini-2.0-flash"),
    ("xai", "grok-3-fast"),
    // OpenAI-compat providers
    ("openrouter", "anthropic/claude-sonnet-4-6"),
    ("together", "meta-llama/Llama-3.3-70B-Instruct-Turbo"),
    (
        "fireworks",
        "accounts/fireworks/models/llama-v3p3-70b-instruct",
    ),
    ("cerebras", "llama-3.3-70b"),
    ("sambanova", "Meta-Llama-3.3-70B-Instruct"),
    ("cohere", "command-r-plus"),
    ("ai21", "jamba-1.5-large"),
    // Local
    ("native", "native-model"),
    ("mock", "mock-model"),
];

/// Get the default model for a provider (by canonical ID or alias).
///
/// Returns `None` only for unknown providers.
///
/// # Examples
///
/// ```
/// use nika_core::catalogs::resolver::default_model_for_provider;
/// assert_eq!(default_model_for_provider("anthropic"), Some("claude-sonnet-4-6"));
/// assert_eq!(default_model_for_provider("claude"), Some("claude-sonnet-4-6")); // alias
/// ```
pub fn default_model_for_provider(provider: &str) -> Option<&'static str> {
    let canonical = find_provider(provider).map(|p| p.id).unwrap_or(provider);
    PROVIDER_DEFAULTS
        .iter()
        .find(|(id, _)| *id == canonical)
        .map(|(_, model)| *model)
}

/// Cheapest available model per provider (for compression, repair, etc.).
///
/// These are small/fast models that minimize cost for auxiliary tasks.
/// Falls back to [`default_model_for_provider`] when no cheap alternative exists.
pub static PROVIDER_CHEAP_MODELS: &[(&str, &str)] = &[
    ("anthropic", "claude-haiku-4-5"),
    ("openai", "gpt-4.1-mini"),
    ("gemini", "gemini-2.0-flash"),
    ("groq", "llama-3.3-70b-versatile"),
    ("deepseek", "deepseek-chat"),
    ("mistral", "mistral-small-latest"),
];

/// Get the cheapest model for a provider (for compression/repair tasks).
///
/// Returns `None` for unknown providers or providers without a cheap alternative.
pub fn cheap_model_for_provider(provider: &str) -> Option<&'static str> {
    let canonical = find_provider(provider).map(|p| p.id).unwrap_or(provider);
    PROVIDER_CHEAP_MODELS
        .iter()
        .find(|(id, _)| *id == canonical)
        .map(|(_, model)| *model)
}

/// The result of resolving a model — carries provenance so cost tracking
/// and events always know exactly which model is in use and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModel {
    /// The actual model ID to send to the provider API (e.g., "claude-sonnet-4-6")
    pub model_id: String,
    /// The canonical provider ID (e.g., "anthropic", not "claude")
    pub provider_id: String,
    /// How this model was determined — for debugging and events
    pub source: ModelSource,
}

/// Provenance of the resolved model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelSource {
    /// Explicitly set on the task (highest priority)
    Task,
    /// Inherited from the workflow header
    Workflow,
    /// Provider default (used when neither task nor workflow specify a model)
    ProviderDefault,
    /// Substituted during fallback chain (original model was incompatible)
    FallbackSubstituted {
        /// The original model that was requested
        original_model: String,
        /// Index in the fallback chain (0 = primary, 1 = first fallback, ...)
        chain_position: usize,
    },
}

/// Compatibility verdict for a (provider, model) pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelCompatibility {
    /// Model is known to work with this provider
    Compatible,
    /// Model belongs to a different provider — will fail at API level
    Incompatible {
        model: String,
        provider: String,
        reason: String,
    },
    /// Model is not in any catalog — might work (custom/new model), proceed with warning
    Unknown,
}

/// Centralized model resolution.
///
/// Resolution priority: task_model > workflow_model > provider_default.
///
/// When a fallback chain substitutes the provider, the resolver detects if the
/// current model is incompatible with the new provider and substitutes the new
/// provider's default model, recording the swap in `ModelSource::FallbackSubstituted`.
pub struct ModelResolver;

impl ModelResolver {
    /// Resolve the effective model for a task execution.
    pub fn resolve(
        task_model: Option<&str>,
        workflow_model: Option<&str>,
        provider: &str,
        fallback_position: usize,
        original_model: Option<&str>,
    ) -> ResolvedModel {
        let canonical = Self::canonical_provider(provider);

        // Step 1: Determine the candidate model and its source
        let (candidate, source) = if let Some(m) = task_model {
            (m.to_string(), ModelSource::Task)
        } else if let Some(m) = workflow_model {
            (m.to_string(), ModelSource::Workflow)
        } else {
            let default = default_model_for_provider(provider).unwrap_or("claude-sonnet-4-6");
            (default.to_string(), ModelSource::ProviderDefault)
        };

        // Step 2: If this is a fallback provider, check compatibility
        if fallback_position > 0 {
            if let ModelCompatibility::Incompatible { .. } = Self::validate(canonical, &candidate) {
                let substitute =
                    default_model_for_provider(provider).unwrap_or("claude-sonnet-4-6");
                return ResolvedModel {
                    model_id: substitute.to_string(),
                    provider_id: canonical.to_string(),
                    source: ModelSource::FallbackSubstituted {
                        original_model: original_model.unwrap_or(&candidate).to_string(),
                        chain_position: fallback_position,
                    },
                };
            }
        }

        ResolvedModel {
            model_id: candidate,
            provider_id: canonical.to_string(),
            source,
        }
    }

    /// Validate whether a model is compatible with a provider.
    ///
    /// Uses model name prefix heuristics (not an exhaustive API call).
    pub fn validate(provider: &str, model: &str) -> ModelCompatibility {
        let canonical = Self::canonical_provider(provider);
        let model_lower = model.to_lowercase();

        let detected_provider = if model_lower.starts_with("claude") {
            Some("anthropic")
        } else if model_lower.starts_with("gpt-")
            || model_lower.starts_with("o1")
            || model_lower.starts_with("o3")
            || model_lower.starts_with("o4")
        {
            Some("openai")
        } else if model_lower.starts_with("mistral")
            || model_lower.starts_with("codestral")
            || model_lower.starts_with("pixtral")
            || model_lower.starts_with("ministral")
        {
            Some("mistral")
        } else if model_lower.starts_with("llama")
            || model_lower.starts_with("mixtral")
            || model_lower.starts_with("gemma")
        {
            Some("groq")
        } else if model_lower.starts_with("deepseek") {
            Some("deepseek")
        } else if model_lower.starts_with("gemini") {
            Some("gemini")
        } else if model_lower.starts_with("grok") {
            Some("xai")
        } else {
            None
        };

        match detected_provider {
            Some(expected) if expected != canonical => ModelCompatibility::Incompatible {
                model: model.to_string(),
                provider: canonical.to_string(),
                reason: format!(
                    "model '{}' belongs to provider '{}', not '{}'",
                    model, expected, canonical
                ),
            },
            Some(_) => ModelCompatibility::Compatible,
            None => ModelCompatibility::Unknown,
        }
    }

    /// Normalize provider name to canonical ID.
    fn canonical_provider(name: &str) -> &str {
        find_provider(name).map(|p| p.id).unwrap_or(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_model_takes_priority() {
        let r = ModelResolver::resolve(
            Some("claude-opus-4-20250514"),
            Some("claude-sonnet-4-6"),
            "anthropic",
            0,
            None,
        );
        assert_eq!(r.model_id, "claude-opus-4-20250514");
        assert_eq!(r.source, ModelSource::Task);
    }

    #[test]
    fn workflow_model_when_no_task_model() {
        let r = ModelResolver::resolve(None, Some("gpt-4o"), "openai", 0, None);
        assert_eq!(r.model_id, "gpt-4o");
        assert_eq!(r.source, ModelSource::Workflow);
    }

    #[test]
    fn provider_default_when_nothing_specified() {
        let r = ModelResolver::resolve(None, None, "anthropic", 0, None);
        assert_eq!(r.model_id, "claude-sonnet-4-6");
        assert_eq!(r.source, ModelSource::ProviderDefault);
    }

    #[test]
    fn alias_resolves_to_canonical() {
        let r = ModelResolver::resolve(None, None, "claude", 0, None);
        assert_eq!(r.provider_id, "anthropic");
        assert_eq!(r.model_id, "claude-sonnet-4-6");
    }

    #[test]
    fn fallback_substitutes_incompatible_model() {
        let r = ModelResolver::resolve(
            Some("llama-3.3-70b-versatile"),
            None,
            "openai",
            1,
            Some("llama-3.3-70b-versatile"),
        );
        assert_eq!(r.model_id, "gpt-4o");
        assert!(matches!(r.source, ModelSource::FallbackSubstituted { .. }));
        if let ModelSource::FallbackSubstituted {
            original_model,
            chain_position,
        } = &r.source
        {
            assert_eq!(original_model, "llama-3.3-70b-versatile");
            assert_eq!(*chain_position, 1);
        }
    }

    #[test]
    fn fallback_keeps_compatible_model() {
        let r = ModelResolver::resolve(Some("gpt-4o"), None, "openai", 1, Some("gpt-4o"));
        assert_eq!(r.model_id, "gpt-4o");
        assert_eq!(r.source, ModelSource::Task);
    }

    #[test]
    fn validate_detects_cross_provider_model() {
        let v = ModelResolver::validate("openai", "claude-sonnet-4-6");
        assert!(matches!(v, ModelCompatibility::Incompatible { .. }));
    }

    #[test]
    fn validate_unknown_model_passes() {
        let v = ModelResolver::validate("openai", "my-custom-finetuned-model");
        assert_eq!(v, ModelCompatibility::Unknown);
    }

    #[test]
    fn every_provider_has_a_default() {
        for id in [
            "anthropic",
            "openai",
            "mistral",
            "groq",
            "deepseek",
            "gemini",
            "xai",
            "openrouter",
            "together",
            "fireworks",
            "cerebras",
            "sambanova",
            "cohere",
            "ai21",
            "native",
            "mock",
        ] {
            assert!(
                default_model_for_provider(id).is_some(),
                "Provider '{}' missing default model",
                id
            );
        }
    }

    #[test]
    fn aliases_resolve_to_same_defaults() {
        assert_eq!(
            default_model_for_provider("claude"),
            default_model_for_provider("anthropic")
        );
        assert_eq!(
            default_model_for_provider("gpt"),
            default_model_for_provider("openai")
        );
        assert_eq!(
            default_model_for_provider("grok"),
            default_model_for_provider("xai")
        );
    }

    #[test]
    fn mock_provider_has_default() {
        assert_eq!(default_model_for_provider("mock"), Some("mock-model"));
    }
}
