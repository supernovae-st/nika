//! ModelProvider enum and implementation
//!
//! Represents the available LLM providers in the TUI chat interface.
//! Each provider maps to a rig-core backend (cloud or native).

/// Available LLM providers via rig-core
#[derive(Debug, Clone, PartialEq)]
pub enum ModelProvider {
    /// OpenAI (gpt-4o, gpt-4, etc.)
    OpenAI,
    /// Anthropic Claude (claude-sonnet-4, etc.)
    Claude,
    /// Mistral AI (mistral-large-latest, etc.)
    Mistral,
    /// Groq (llama-3.3-70b-versatile, etc.)
    Groq,
    /// DeepSeek (deepseek-chat, etc.)
    DeepSeek,
    /// Native inference via mistral.rs (local GGUF models)
    Native,
    /// List available providers
    List,
}

impl ModelProvider {
    /// Get the display name for the provider
    pub fn name(&self) -> &'static str {
        match self {
            ModelProvider::OpenAI => "OpenAI (gpt-4o)",
            ModelProvider::Claude => "Anthropic Claude (claude-sonnet-4)",
            ModelProvider::Mistral => "Mistral AI (mistral-large)",
            ModelProvider::Groq => "Groq (llama-3.3-70b)",
            ModelProvider::DeepSeek => "DeepSeek (deepseek-chat)",
            ModelProvider::Native => "Native (mistral.rs)",
            ModelProvider::List => "list",
        }
    }

    /// Get the short command name for the provider (used with `/model <name>`)
    pub fn command_name(&self) -> &'static str {
        match self {
            ModelProvider::OpenAI => "openai",
            ModelProvider::Claude => "claude",
            ModelProvider::Mistral => "mistral",
            ModelProvider::Groq => "groq",
            ModelProvider::DeepSeek => "deepseek",
            ModelProvider::Native => "native",
            ModelProvider::List => "list",
        }
    }

    /// Get the environment variable name required for this provider.
    ///
    /// Delegates to `core::KNOWN_PROVIDERS` as the single source of truth.
    pub fn env_var(&self) -> &'static str {
        nika_engine::core::find_provider(self.command_name())
            .map(|p| p.env_var)
            .unwrap_or("")
    }

    /// Check if the provider is available (env var is set and non-empty).
    pub fn is_available(&self) -> bool {
        match self {
            ModelProvider::List => true,
            _ => nika_engine::core::find_provider(self.command_name())
                .map(|p| !p.requires_key || nika_engine::secrets::has_provider_key(p))
                .unwrap_or(false),
        }
    }

    /// Convert provider name string to ModelProvider enum.
    ///
    /// Uses `core::find_provider()` for alias resolution, then maps to the enum variant.
    pub fn from_name(name: &str) -> Option<Self> {
        let provider = nika_engine::core::find_provider(name)?;
        match provider.id {
            "anthropic" => Some(ModelProvider::Claude),
            "openai" => Some(ModelProvider::OpenAI),
            "mistral" => Some(ModelProvider::Mistral),
            "groq" => Some(ModelProvider::Groq),
            "deepseek" => Some(ModelProvider::DeepSeek),
            "native" => Some(ModelProvider::Native),
            _ => None,
        }
    }
}
