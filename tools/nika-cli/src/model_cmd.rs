//! Unified model command — cloud + local models under `nika model`.
//!
//! Merges the old `nika models` (cloud) and `nika model` (local/native)
//! into a single command. Cloud subcommands are always available;
//! local subcommands require the `native-inference` feature.

use clap::Subcommand;

use nika_engine::error::NikaError;

/// Unified model management actions.
///
/// `nika model` (no subcommand) defaults to listing cloud models.
#[derive(Subcommand)]
pub enum ModelAction {
    /// List cloud LLM models with pricing and capabilities
    ///
    /// Shows all models grouped by provider with input/output pricing,
    /// capability tags, and context window sizes.
    #[command(visible_alias = "ls")]
    List {
        /// Filter by provider name (anthropic, openai, mistral, groq, deepseek, gemini, xai)
        #[arg(short, long)]
        provider: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show details for a specific model (cloud or local)
    ///
    /// Searches cloud models first, then local models if native-inference is enabled.
    Info {
        /// Model name (e.g., claude-sonnet-4-6, gpt-4o, qwen3:8b)
        name: String,
    },

    /// Smart model recommendation based on your configured API keys
    Recommend,

    // ── Local model management (native-inference only) ──────────────
    /// List downloaded local GGUF models in ~/.nika/models/
    #[cfg(feature = "native-inference")]
    Local {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Download a model from HuggingFace
    ///
    /// Supports curated models from KNOWN_MODELS or custom HuggingFace repos.
    /// Example: nika model pull qwen3:8b
    /// Example: nika model pull --repo TheBloke/Mistral-7B-v0.1-GGUF --file mistral-7b-v0.1.Q4_K_M.gguf
    #[cfg(feature = "native-inference")]
    Pull {
        /// Model name (from KNOWN_MODELS, e.g., qwen3:8b, llama3:8b)
        name: Option<String>,

        /// HuggingFace repository (e.g., TheBloke/Mistral-7B-v0.1-GGUF)
        #[arg(long)]
        repo: Option<String>,

        /// Specific GGUF filename to download
        #[arg(long)]
        file: Option<String>,

        /// Quantization level (e.g., Q4_K_M, Q8_0, F16)
        #[arg(short = 'Q', long)]
        quant: Option<String>,

        /// Force re-download even if model exists
        #[arg(short, long)]
        force: bool,
    },

    /// Show status of loaded local models
    #[cfg(feature = "native-inference")]
    Status,

    /// Delete a downloaded local model
    #[cfg(feature = "native-inference")]
    Delete {
        /// Model name or path (relative to ~/.nika/models/)
        name: String,

        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },

    /// Load a HuggingFace vision model with ISQ quantization
    ///
    /// Vision models use VisionModelBuilder + ISQ (in-situ quantization)
    /// from HuggingFace safetensors. GGUF models are text-only.
    ///
    /// Example: nika model vision Qwen/Qwen2.5-VL-7B-Instruct --isq Q4K
    #[cfg(feature = "native-inference")]
    Vision {
        /// HuggingFace model ID (e.g., Qwen/Qwen2.5-VL-7B-Instruct)
        model_id: String,

        /// ISQ quantization level (e.g., Q4K, Q8_0, Q6K)
        #[arg(long)]
        isq: Option<String>,

        /// Context size (token window, default: 4096)
        #[arg(long, default_value = "4096")]
        context_size: u32,
    },
}

/// Handle the unified model command.
///
/// Cloud subcommands call into `model_cloud.rs` directly.
/// Native-inference subcommands delegate to the existing `model.rs` handler.
pub async fn handle_model_command(
    action: Option<ModelAction>,
    quiet: bool,
) -> Result<(), NikaError> {
    // No subcommand → default to cloud model listing
    let action = match action {
        Some(a) => a,
        None => return super::model_cloud::print_cloud_models(None, false),
    };

    match action {
        // ── Cloud (always available) ─────────────────────────────────
        ModelAction::List { provider, json } => {
            super::model_cloud::print_cloud_models(provider.as_deref(), json)
        }
        ModelAction::Info { name } => handle_model_info(&name, quiet),
        ModelAction::Recommend => super::model_cloud::print_model_recommend(),

        // ── Local (native-inference only) ────────────────────────────
        #[cfg(feature = "native-inference")]
        local_action => {
            let old_action = to_old_model_action(local_action);
            super::model::handle_model_command(old_action, quiet).await
        }
    }
}

/// Unified model info: tries cloud first, then local.
fn handle_model_info(name: &str, _quiet: bool) -> Result<(), NikaError> {
    // Try cloud model first
    if super::model_cloud::print_model_info(name).is_ok() {
        return Ok(());
    }

    // Try local model (native-inference only)
    #[cfg(feature = "native-inference")]
    {
        let old_action = super::model::ModelAction::Info {
            name: name.to_string(),
        };
        // Can't call async from sync, but model info is sync in practice.
        // Use a mini tokio block_on.
        let rt = tokio::runtime::Handle::current();
        rt.block_on(super::model::handle_model_command(old_action, _quiet))
    }

    #[cfg(not(feature = "native-inference"))]
    Err(NikaError::ValidationError {
        reason: format!("Model '{}' not found. Run: nika model list", name),
    })
}

/// Convert unified action to old ModelAction for delegation.
#[cfg(feature = "native-inference")]
fn to_old_model_action(action: ModelAction) -> super::model::ModelAction {
    match action {
        ModelAction::Local { json } => super::model::ModelAction::List { json },
        ModelAction::Pull {
            name,
            repo,
            file,
            quant,
            force,
        } => super::model::ModelAction::Pull {
            name,
            repo,
            file,
            quant,
            force,
        },
        ModelAction::Status => super::model::ModelAction::Status,
        ModelAction::Delete { name, force } => super::model::ModelAction::Delete { name, force },
        ModelAction::Vision {
            model_id,
            isq,
            context_size,
        } => super::model::ModelAction::Vision {
            model_id,
            isq,
            context_size,
        },
        // Cloud actions should never reach here
        _ => unreachable!("cloud actions handled before delegation"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_action_list_variant() {
        let _ = ModelAction::List {
            provider: None,
            json: false,
        };
    }

    #[test]
    fn model_action_info_variant() {
        let _ = ModelAction::Info {
            name: "claude-sonnet-4-6".to_string(),
        };
    }

    #[test]
    fn model_action_recommend_variant() {
        let _ = ModelAction::Recommend;
    }

    #[cfg(feature = "native-inference")]
    #[test]
    fn model_action_local_variant() {
        let _ = ModelAction::Local { json: false };
    }

    #[cfg(feature = "native-inference")]
    #[test]
    fn model_action_pull_variant() {
        let _ = ModelAction::Pull {
            name: None,
            repo: None,
            file: None,
            quant: None,
            force: false,
        };
    }
}
