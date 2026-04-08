// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Curated model definitions for native inference via mistral.rs.
//!
//! This module defines the known models that nika supports for local inference:
//! - **Text models**: Qwen3, Llama4, Mistral, Phi-4, etc.
//! - **Vision models**: LLaVA, Qwen2-VL, etc.
//! - **Embedding models**: BGE, Nomic, etc.
//!
//! ## Model Selection
//!
//! Use `find_model()` to look up a model by ID, or `models_by_type()` to
//! filter by capability.
//!
//! ## Note
//!
//! Functions that require runtime dependencies (`resolve_model`,
//! `detect_available_ram_gb`) live in `nika::core::models` and re-export
//! everything from this module.

use std::fmt;
use std::path::PathBuf;
use thiserror::Error;

/// Error type for model resolution operations.
///
#[derive(Error, Debug, Clone)]
pub enum ModelResolveError {
    /// Unknown model ID.
    #[error("Unknown model: {id}")]
    UnknownModel {
        /// The model ID that was not found.
        id: String,
    },

    /// Requested quantization is not available for this model.
    #[error("Quantization {quantization} not available for {model_id}")]
    QuantizationNotAvailable {
        /// The quantization level requested.
        quantization: Quantization,
        /// The model ID.
        model_id: String,
    },

    /// Cannot determine home directory.
    #[error("Cannot determine home directory")]
    HomeDirectoryNotFound,

    /// Model is not downloaded locally.
    #[error("Model not downloaded. Run: nika model pull {model_id}")]
    ModelNotDownloaded {
        /// The model ID.
        model_id: String,
    },

    /// Cannot read snapshots directory.
    ///
    #[error("Cannot read snapshots directory {path}: {message}")]
    SnapshotsDirReadError {
        /// The path that could not be read.
        path: PathBuf,
        /// The error message (stringified from std::io::Error).
        message: String,
    },

    /// No snapshots found for model.
    #[error("No snapshots found for {model_id}")]
    NoSnapshotsFound {
        /// The model ID.
        model_id: String,
    },

    /// Model file not found at expected path.
    #[error("Model file not found: {path}. Run: nika model pull {model_id}")]
    ModelFileNotFound {
        /// The expected path.
        path: PathBuf,
        /// The model ID.
        model_id: String,
    },
}

/// Type of model capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelType {
    /// Text generation (LLM)
    Text,
    /// Vision/multimodal (VLM)
    Vision,
    /// Text embeddings
    Embedding,
    /// Audio/speech
    Audio,
    /// Image generation (diffusion)
    Diffusion,
}

impl fmt::Display for ModelType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text => write!(f, "Text"),
            Self::Vision => write!(f, "Vision"),
            Self::Embedding => write!(f, "Embedding"),
            Self::Audio => write!(f, "Audio"),
            Self::Diffusion => write!(f, "Diffusion"),
        }
    }
}

/// Model architecture supported by mistral.rs.
///
/// These correspond to the architectures that mistral.rs can load and run.
/// See: <https://github.com/EricLBuehler/mistral.rs>
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(non_camel_case_types)]
pub enum ModelArchitecture {
    // Text models
    Mistral,
    Mixtral,
    Llama,
    Llama4,
    Phi2,
    Phi3,
    Phi4,
    Qwen2,
    Qwen3,
    Gemma,
    Gemma2,
    Starcoder2,
    DeepSeek,
    DeepSeekV2,
    DeepSeekV3,
    Cohere,
    Dbrx,
    Yi,
    Falcon,
    Bloom,
    Mpt,
    Internlm2,
    // Vision models
    LLaVA,
    LLaVANext,
    Qwen2VL,
    Phi3V,
    Idefics2,
    Pixtral,
    // Embedding models
    Bert,
    Nomic,
    Jina,
    // Other
    Unknown,
}

impl fmt::Display for ModelArchitecture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Mistral => "Mistral",
            Self::Mixtral => "Mixtral",
            Self::Llama => "Llama",
            Self::Llama4 => "Llama 4",
            Self::Phi2 => "Phi-2",
            Self::Phi3 => "Phi-3",
            Self::Phi4 => "Phi-4",
            Self::Qwen2 => "Qwen2",
            Self::Qwen3 => "Qwen3",
            Self::Gemma => "Gemma",
            Self::Gemma2 => "Gemma 2",
            Self::Starcoder2 => "Starcoder 2",
            Self::DeepSeek => "DeepSeek",
            Self::DeepSeekV2 => "DeepSeek V2",
            Self::DeepSeekV3 => "DeepSeek V3",
            Self::Cohere => "Cohere",
            Self::Dbrx => "DBRX",
            Self::Yi => "Yi",
            Self::Falcon => "Falcon",
            Self::Bloom => "Bloom",
            Self::Mpt => "MPT",
            Self::Internlm2 => "InternLM 2",
            Self::LLaVA => "LLaVA",
            Self::LLaVANext => "LLaVA-Next",
            Self::Qwen2VL => "Qwen2-VL",
            Self::Phi3V => "Phi-3 Vision",
            Self::Idefics2 => "Idefics 2",
            Self::Pixtral => "Pixtral",
            Self::Bert => "BERT",
            Self::Nomic => "Nomic",
            Self::Jina => "Jina",
            Self::Unknown => "Unknown",
        };
        write!(f, "{}", name)
    }
}

/// Quantization level for GGUF models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(non_camel_case_types)]
pub enum Quantization {
    /// Float 16 (full precision, largest)
    F16,
    /// 8-bit quantization
    Q8_0,
    /// 6-bit K-quant medium
    Q6_K,
    /// 5-bit K-quant medium (good balance)
    Q5_K_M,
    /// 5-bit K-quant small
    Q5_K_S,
    /// 4-bit K-quant medium (recommended)
    Q4_K_M,
    /// 4-bit K-quant small
    Q4_K_S,
    /// 4-bit
    Q4_0,
    /// 3-bit K-quant small
    Q3_K_S,
    /// 3-bit K-quant medium
    Q3_K_M,
    /// 3-bit K-quant large
    Q3_K_L,
    /// 2-bit K-quant (smallest, lowest quality)
    Q2_K,
    /// IQ2 - intelligent 2-bit
    IQ2_XS,
    /// IQ3 - intelligent 3-bit
    IQ3_XS,
    /// IQ4 - intelligent 4-bit
    IQ4_NL,
}

impl Quantization {
    /// Memory multiplier relative to model size.
    /// Lower = less memory needed.
    pub fn memory_multiplier(&self) -> f32 {
        match self {
            Self::F16 => 2.0,
            Self::Q8_0 => 1.0,
            Self::Q6_K => 0.75,
            Self::Q5_K_M | Self::Q5_K_S => 0.625,
            Self::Q4_K_M | Self::Q4_K_S | Self::Q4_0 => 0.5,
            Self::Q3_K_S | Self::Q3_K_M | Self::Q3_K_L => 0.375,
            Self::Q2_K | Self::IQ2_XS => 0.25,
            Self::IQ3_XS => 0.375,
            Self::IQ4_NL => 0.5,
        }
    }

    /// Quality rating (1-5, higher = better).
    pub fn quality_rating(&self) -> u8 {
        match self {
            Self::F16 => 5,
            Self::Q8_0 => 5,
            Self::Q6_K => 4,
            Self::Q5_K_M | Self::Q5_K_S => 4,
            Self::Q4_K_M | Self::Q4_K_S | Self::Q4_0 => 3,
            Self::Q3_K_S | Self::Q3_K_M | Self::Q3_K_L => 2,
            Self::Q2_K | Self::IQ2_XS => 1,
            Self::IQ3_XS => 2,
            Self::IQ4_NL => 3,
        }
    }
}

impl fmt::Display for Quantization {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::F16 => "F16",
            Self::Q8_0 => "Q8_0",
            Self::Q6_K => "Q6_K",
            Self::Q5_K_M => "Q5_K_M",
            Self::Q5_K_S => "Q5_K_S",
            Self::Q4_K_M => "Q4_K_M",
            Self::Q4_K_S => "Q4_K_S",
            Self::Q4_0 => "Q4_0",
            Self::Q3_K_S => "Q3_K_S",
            Self::Q3_K_M => "Q3_K_M",
            Self::Q3_K_L => "Q3_K_L",
            Self::Q2_K => "Q2_K",
            Self::IQ2_XS => "IQ2_XS",
            Self::IQ3_XS => "IQ3_XS",
            Self::IQ4_NL => "IQ4_NL",
        };
        write!(f, "{}", name)
    }
}

/// A curated model with download and runtime metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct KnownModel {
    /// Short ID for CLI (e.g., "qwen3:8b")
    pub id: &'static str,
    /// Human-readable name (e.g., "Qwen3 8B")
    pub name: &'static str,
    /// Model capability type
    pub model_type: ModelType,
    /// Architecture for mistral.rs loader
    pub architecture: ModelArchitecture,
    /// HuggingFace repository (e.g., "Qwen/Qwen3-8B-GGUF")
    pub hf_repo: &'static str,
    /// Default GGUF filename
    pub default_file: &'static str,
    /// Available quantizations with filenames
    pub quantizations: &'static [(Quantization, &'static str)],
    /// Model size in billions of parameters
    pub param_billions: f32,
    /// Minimum RAM in GB for inference
    pub min_ram_gb: u32,
    /// Short description
    pub description: &'static str,
}

/// Curated models for native inference (15 models).
///
/// These are tested and verified to work with mistral.rs.
pub static KNOWN_MODELS: &[KnownModel] = &[
    // =============================================================================
    // TEXT MODELS - General Purpose
    // =============================================================================
    KnownModel {
        id: "qwen3:8b",
        name: "Qwen3 8B",
        model_type: ModelType::Text,
        architecture: ModelArchitecture::Qwen3,
        hf_repo: "Qwen/Qwen3-8B-GGUF",
        default_file: "Qwen3-8B-Q4_K_M.gguf",
        quantizations: &[
            (Quantization::Q4_K_M, "Qwen3-8B-Q4_K_M.gguf"),
            (Quantization::Q5_K_M, "Qwen3-8B-Q5_K_M.gguf"),
            (Quantization::Q8_0, "Qwen3-8B-Q8_0.gguf"),
        ],
        param_billions: 8.0,
        min_ram_gb: 8,
        description: "Best balance of speed and quality for most tasks",
    },
    KnownModel {
        id: "qwen3:1.7b",
        name: "Qwen3 1.7B",
        model_type: ModelType::Text,
        architecture: ModelArchitecture::Qwen3,
        hf_repo: "Qwen/Qwen3-1.7B-GGUF",
        default_file: "Qwen3-1.7B-Q8_0.gguf",
        quantizations: &[
            // Note: Qwen3-1.7B only has Q8_0 on HuggingFace
            (Quantization::Q8_0, "Qwen3-1.7B-Q8_0.gguf"),
        ],
        param_billions: 1.7,
        min_ram_gb: 4,
        description: "Fast and lightweight for simple tasks",
    },
    KnownModel {
        id: "qwen3:32b",
        name: "Qwen3 32B",
        model_type: ModelType::Text,
        architecture: ModelArchitecture::Qwen3,
        hf_repo: "Qwen/Qwen3-32B-GGUF",
        default_file: "qwen3-32b-q4_k_m.gguf",
        quantizations: &[
            (Quantization::Q4_K_M, "qwen3-32b-q4_k_m.gguf"),
            (Quantization::Q5_K_M, "qwen3-32b-q5_k_m.gguf"),
        ],
        param_billions: 32.0,
        min_ram_gb: 24,
        description: "High quality for complex reasoning",
    },
    KnownModel {
        id: "llama3.2:3b",
        name: "Llama 3.2 3B",
        model_type: ModelType::Text,
        architecture: ModelArchitecture::Llama,
        hf_repo: "meta-llama/Llama-3.2-3B-Instruct-GGUF",
        default_file: "Llama-3.2-3B-Instruct-Q4_K_M.gguf",
        quantizations: &[
            (Quantization::Q4_K_M, "Llama-3.2-3B-Instruct-Q4_K_M.gguf"),
            (Quantization::Q8_0, "Llama-3.2-3B-Instruct-Q8_0.gguf"),
        ],
        param_billions: 3.0,
        min_ram_gb: 6,
        description: "Meta's efficient small model",
    },
    KnownModel {
        id: "llama3.2:1b",
        name: "Llama 3.2 1B",
        model_type: ModelType::Text,
        architecture: ModelArchitecture::Llama,
        hf_repo: "meta-llama/Llama-3.2-1B-Instruct-GGUF",
        default_file: "Llama-3.2-1B-Instruct-Q4_K_M.gguf",
        quantizations: &[
            (Quantization::Q4_K_M, "Llama-3.2-1B-Instruct-Q4_K_M.gguf"),
            (Quantization::Q8_0, "Llama-3.2-1B-Instruct-Q8_0.gguf"),
        ],
        param_billions: 1.0,
        min_ram_gb: 4,
        description: "Smallest Llama for edge devices",
    },
    KnownModel {
        id: "llama3.1:8b",
        name: "Llama 3.1 8B",
        model_type: ModelType::Text,
        architecture: ModelArchitecture::Llama,
        hf_repo: "meta-llama/Llama-3.1-8B-Instruct-GGUF",
        default_file: "Llama-3.1-8B-Instruct-Q4_K_M.gguf",
        quantizations: &[
            (Quantization::Q4_K_M, "Llama-3.1-8B-Instruct-Q4_K_M.gguf"),
            (Quantization::Q5_K_M, "Llama-3.1-8B-Instruct-Q5_K_M.gguf"),
            (Quantization::Q8_0, "Llama-3.1-8B-Instruct-Q8_0.gguf"),
        ],
        param_billions: 8.0,
        min_ram_gb: 8,
        description: "Versatile 8B model with 128K context",
    },
    KnownModel {
        id: "phi4:14b",
        name: "Phi-4 14B",
        model_type: ModelType::Text,
        architecture: ModelArchitecture::Phi4,
        hf_repo: "microsoft/phi-4-gguf",
        default_file: "phi-4-q4_k_m.gguf",
        quantizations: &[
            (Quantization::Q4_K_M, "phi-4-q4_k_m.gguf"),
            (Quantization::Q8_0, "phi-4-q8_0.gguf"),
        ],
        param_billions: 14.0,
        min_ram_gb: 12,
        description: "Microsoft's reasoning-focused model",
    },
    KnownModel {
        id: "mistral:7b",
        name: "Mistral 7B",
        model_type: ModelType::Text,
        architecture: ModelArchitecture::Mistral,
        hf_repo: "TheBloke/Mistral-7B-Instruct-v0.2-GGUF",
        default_file: "mistral-7b-instruct-v0.2.Q4_K_M.gguf",
        quantizations: &[
            (Quantization::Q4_K_M, "mistral-7b-instruct-v0.2.Q4_K_M.gguf"),
            (Quantization::Q5_K_M, "mistral-7b-instruct-v0.2.Q5_K_M.gguf"),
            (Quantization::Q8_0, "mistral-7b-instruct-v0.2.Q8_0.gguf"),
        ],
        param_billions: 7.0,
        min_ram_gb: 8,
        description: "Classic efficient 7B model",
    },
    KnownModel {
        id: "gemma2:9b",
        name: "Gemma 2 9B",
        model_type: ModelType::Text,
        architecture: ModelArchitecture::Gemma2,
        hf_repo: "google/gemma-2-9b-it-GGUF",
        default_file: "gemma-2-9b-it-Q4_K_M.gguf",
        quantizations: &[
            (Quantization::Q4_K_M, "gemma-2-9b-it-Q4_K_M.gguf"),
            (Quantization::Q8_0, "gemma-2-9b-it-Q8_0.gguf"),
        ],
        param_billions: 9.0,
        min_ram_gb: 10,
        description: "Google's efficient instruction-tuned model",
    },
    KnownModel {
        id: "gemma2:2b",
        name: "Gemma 2 2B",
        model_type: ModelType::Text,
        architecture: ModelArchitecture::Gemma2,
        hf_repo: "google/gemma-2-2b-it-GGUF",
        default_file: "gemma-2-2b-it-Q4_K_M.gguf",
        quantizations: &[
            (Quantization::Q4_K_M, "gemma-2-2b-it-Q4_K_M.gguf"),
            (Quantization::Q8_0, "gemma-2-2b-it-Q8_0.gguf"),
        ],
        param_billions: 2.0,
        min_ram_gb: 4,
        description: "Compact Google model for constrained environments",
    },
    // =============================================================================
    // TEXT MODELS - Code Specialized
    // =============================================================================
    KnownModel {
        id: "deepseek-coder:6.7b",
        name: "DeepSeek Coder 6.7B",
        model_type: ModelType::Text,
        architecture: ModelArchitecture::DeepSeek,
        hf_repo: "TheBloke/deepseek-coder-6.7B-instruct-GGUF",
        default_file: "deepseek-coder-6.7b-instruct.Q4_K_M.gguf",
        quantizations: &[
            (
                Quantization::Q4_K_M,
                "deepseek-coder-6.7b-instruct.Q4_K_M.gguf",
            ),
            (Quantization::Q8_0, "deepseek-coder-6.7b-instruct.Q8_0.gguf"),
        ],
        param_billions: 6.7,
        min_ram_gb: 8,
        description: "Code generation specialist",
    },
    KnownModel {
        id: "starcoder2:7b",
        name: "StarCoder2 7B",
        model_type: ModelType::Text,
        architecture: ModelArchitecture::Starcoder2,
        hf_repo: "bigcode/starcoder2-7b-GGUF",
        default_file: "starcoder2-7b-Q4_K_M.gguf",
        quantizations: &[
            (Quantization::Q4_K_M, "starcoder2-7b-Q4_K_M.gguf"),
            (Quantization::Q8_0, "starcoder2-7b-Q8_0.gguf"),
        ],
        param_billions: 7.0,
        min_ram_gb: 8,
        description: "Code completion from BigCode",
    },
    // =============================================================================
    // VISION MODELS
    // =============================================================================
    KnownModel {
        id: "llava:7b",
        name: "LLaVA 1.6 7B",
        model_type: ModelType::Vision,
        architecture: ModelArchitecture::LLaVA,
        hf_repo: "mys/ggml_llava-v1.6-mistral-7b",
        default_file: "ggml-model-q4_k.gguf",
        quantizations: &[(Quantization::Q4_K_M, "ggml-model-q4_k.gguf")],
        param_billions: 7.0,
        min_ram_gb: 10,
        description: "Vision-language understanding",
    },
    // =============================================================================
    // EMBEDDING MODELS
    // =============================================================================
    KnownModel {
        id: "nomic-embed:1.5",
        name: "Nomic Embed 1.5",
        model_type: ModelType::Embedding,
        architecture: ModelArchitecture::Nomic,
        hf_repo: "nomic-ai/nomic-embed-text-v1.5-GGUF",
        default_file: "nomic-embed-text-v1.5.Q8_0.gguf",
        quantizations: &[(Quantization::Q8_0, "nomic-embed-text-v1.5.Q8_0.gguf")],
        param_billions: 0.137,
        min_ram_gb: 2,
        description: "Fast text embeddings with 8K context",
    },
    KnownModel {
        id: "bge:large",
        name: "BGE Large",
        model_type: ModelType::Embedding,
        architecture: ModelArchitecture::Bert,
        hf_repo: "BAAI/bge-large-en-v1.5-gguf",
        default_file: "bge-large-en-v1.5-q8_0.gguf",
        quantizations: &[(Quantization::Q8_0, "bge-large-en-v1.5-q8_0.gguf")],
        param_billions: 0.335,
        min_ram_gb: 2,
        description: "High-quality retrieval embeddings",
    },
];

/// Find a model by ID.
///
/// # Example
///
/// ```
/// use nika_core::catalogs::models::find_model;
///
/// let model = find_model("qwen3:8b").unwrap();
/// assert_eq!(model.param_billions, 8.0);
/// ```
#[must_use]
pub fn find_model(id: &str) -> Option<&'static KnownModel> {
    KNOWN_MODELS.iter().find(|m| m.id == id)
}

/// Get all models of a specific type.
///
/// # Example
///
/// ```
/// use nika_core::catalogs::models::{models_by_type, ModelType};
///
/// let text_models = models_by_type(ModelType::Text);
/// assert!(text_models.len() > 5);
/// ```
#[must_use]
pub fn models_by_type(model_type: ModelType) -> Vec<&'static KnownModel> {
    KNOWN_MODELS
        .iter()
        .filter(|m| m.model_type == model_type)
        .collect()
}

/// Auto-select the best quantization for available RAM.
///
/// # Example
///
/// ```
/// use nika_core::catalogs::models::{find_model, auto_select_quantization};
///
/// let model = find_model("qwen3:8b").unwrap();
/// let quant = auto_select_quantization(model, 16);
/// // With 16GB RAM, should select Q4_K_M or better
/// ```
#[must_use]
pub fn auto_select_quantization(model: &KnownModel, available_ram_gb: u32) -> Quantization {
    // Estimate memory needed for each quantization
    for (quant, _filename) in model.quantizations.iter() {
        let estimated_gb = (model.param_billions * quant.memory_multiplier()) as u32 + 2; // +2GB headroom
        if estimated_gb <= available_ram_gb {
            return *quant;
        }
    }

    // Fall back to smallest available
    model
        .quantizations
        .last()
        .map(|(q, _)| *q)
        .unwrap_or(Quantization::Q4_K_M)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_models_count() {
        // Should have at least 15 models
        assert!(KNOWN_MODELS.len() >= 15);
    }

    #[test]
    fn test_find_model() {
        let qwen = find_model("qwen3:8b").unwrap();
        assert_eq!(qwen.id, "qwen3:8b");
        assert_eq!(qwen.param_billions, 8.0);
        assert_eq!(qwen.architecture, ModelArchitecture::Qwen3);

        assert!(find_model("nonexistent").is_none());
    }

    #[test]
    fn test_models_by_type() {
        let text_models = models_by_type(ModelType::Text);
        assert!(text_models.len() >= 10);
        assert!(text_models.iter().all(|m| m.model_type == ModelType::Text));

        let vision_models = models_by_type(ModelType::Vision);
        assert!(!vision_models.is_empty());

        let embedding_models = models_by_type(ModelType::Embedding);
        assert!(embedding_models.len() >= 2);
    }

    #[test]
    fn test_auto_select_quantization() {
        let model = find_model("qwen3:8b").unwrap();

        // With lots of RAM, should pick highest quality available
        let quant = auto_select_quantization(model, 32);
        assert!(quant.quality_rating() >= 3);

        // With limited RAM, should pick smaller quantization
        let quant_low = auto_select_quantization(model, 6);
        assert!(quant_low.memory_multiplier() <= 0.5);
    }

    #[test]
    fn test_quantization_properties() {
        assert!(Quantization::F16.memory_multiplier() > Quantization::Q4_K_M.memory_multiplier());
        assert!(Quantization::Q8_0.quality_rating() > Quantization::Q2_K.quality_rating());
    }

    #[test]
    fn test_model_type_display() {
        assert_eq!(ModelType::Text.to_string(), "Text");
        assert_eq!(ModelType::Vision.to_string(), "Vision");
        assert_eq!(ModelType::Embedding.to_string(), "Embedding");
    }

    #[test]
    fn test_architecture_display() {
        assert_eq!(ModelArchitecture::Qwen3.to_string(), "Qwen3");
        assert_eq!(ModelArchitecture::Llama4.to_string(), "Llama 4");
        assert_eq!(ModelArchitecture::Phi4.to_string(), "Phi-4");
    }

    // =========================================================================
    // UNIQUE IDS
    // =========================================================================

    #[test]
    fn test_model_ids_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for model in KNOWN_MODELS {
            assert!(seen.insert(model.id), "Duplicate model ID: {}", model.id);
        }
    }

    // =========================================================================
    // FIND_MODEL EDGE CASES
    // =========================================================================

    #[test]
    fn test_find_model_case_sensitive() {
        assert!(find_model("Qwen3:8b").is_none());
        assert!(find_model("QWEN3:8B").is_none());
        assert!(find_model("qwen3:8b").is_some());
    }

    #[test]
    fn test_find_model_empty_string() {
        assert!(find_model("").is_none());
    }

    #[test]
    fn test_find_model_all_known_ids() {
        for model in KNOWN_MODELS {
            assert!(
                find_model(model.id).is_some(),
                "find_model should find: {}",
                model.id
            );
        }
    }

    // =========================================================================
    // MODELS_BY_TYPE EDGE CASES
    // =========================================================================

    #[test]
    fn test_models_by_type_audio_empty() {
        let audio = models_by_type(ModelType::Audio);
        assert!(audio.is_empty(), "No Audio models currently defined");
    }

    #[test]
    fn test_models_by_type_diffusion_empty() {
        let diffusion = models_by_type(ModelType::Diffusion);
        assert!(
            diffusion.is_empty(),
            "No Diffusion models currently defined"
        );
    }

    // =========================================================================
    // MODEL TYPE DISPLAY COMPLETENESS
    // =========================================================================

    #[test]
    fn test_model_type_display_all() {
        assert_eq!(ModelType::Audio.to_string(), "Audio");
        assert_eq!(ModelType::Diffusion.to_string(), "Diffusion");
    }

    // =========================================================================
    // QUANTIZATION EDGE CASES
    // =========================================================================

    #[test]
    fn test_quantization_display_all() {
        assert_eq!(Quantization::F16.to_string(), "F16");
        assert_eq!(Quantization::Q8_0.to_string(), "Q8_0");
        assert_eq!(Quantization::Q6_K.to_string(), "Q6_K");
        assert_eq!(Quantization::Q5_K_M.to_string(), "Q5_K_M");
        assert_eq!(Quantization::Q5_K_S.to_string(), "Q5_K_S");
        assert_eq!(Quantization::Q4_K_M.to_string(), "Q4_K_M");
        assert_eq!(Quantization::Q4_K_S.to_string(), "Q4_K_S");
        assert_eq!(Quantization::Q4_0.to_string(), "Q4_0");
        assert_eq!(Quantization::Q3_K_S.to_string(), "Q3_K_S");
        assert_eq!(Quantization::Q3_K_M.to_string(), "Q3_K_M");
        assert_eq!(Quantization::Q3_K_L.to_string(), "Q3_K_L");
        assert_eq!(Quantization::Q2_K.to_string(), "Q2_K");
        assert_eq!(Quantization::IQ2_XS.to_string(), "IQ2_XS");
        assert_eq!(Quantization::IQ3_XS.to_string(), "IQ3_XS");
        assert_eq!(Quantization::IQ4_NL.to_string(), "IQ4_NL");
    }

    #[test]
    fn test_quantization_memory_multiplier_ordering() {
        // F16 > Q8 > Q6 > Q5 > Q4 > Q3 > Q2
        assert!(Quantization::F16.memory_multiplier() > Quantization::Q8_0.memory_multiplier());
        assert!(Quantization::Q8_0.memory_multiplier() > Quantization::Q6_K.memory_multiplier());
        assert!(Quantization::Q6_K.memory_multiplier() > Quantization::Q5_K_M.memory_multiplier());
        assert!(
            Quantization::Q5_K_M.memory_multiplier() > Quantization::Q4_K_M.memory_multiplier()
        );
        assert!(
            Quantization::Q4_K_M.memory_multiplier() > Quantization::Q3_K_M.memory_multiplier()
        );
        assert!(Quantization::Q3_K_M.memory_multiplier() > Quantization::Q2_K.memory_multiplier());
    }

    #[test]
    fn test_quantization_quality_rating_bounds() {
        // All quality ratings should be 1-5
        let all_quants = [
            Quantization::F16,
            Quantization::Q8_0,
            Quantization::Q6_K,
            Quantization::Q5_K_M,
            Quantization::Q5_K_S,
            Quantization::Q4_K_M,
            Quantization::Q4_K_S,
            Quantization::Q4_0,
            Quantization::Q3_K_S,
            Quantization::Q3_K_M,
            Quantization::Q3_K_L,
            Quantization::Q2_K,
            Quantization::IQ2_XS,
            Quantization::IQ3_XS,
            Quantization::IQ4_NL,
        ];
        for q in &all_quants {
            let rating = q.quality_rating();
            assert!(
                (1..=5).contains(&rating),
                "{} has quality_rating {} outside 1-5",
                q,
                rating
            );
        }
    }

    #[test]
    fn test_quantization_memory_multiplier_positive() {
        let all_quants = [
            Quantization::F16,
            Quantization::Q8_0,
            Quantization::Q6_K,
            Quantization::Q5_K_M,
            Quantization::Q5_K_S,
            Quantization::Q4_K_M,
            Quantization::Q4_K_S,
            Quantization::Q4_0,
            Quantization::Q3_K_S,
            Quantization::Q3_K_M,
            Quantization::Q3_K_L,
            Quantization::Q2_K,
            Quantization::IQ2_XS,
            Quantization::IQ3_XS,
            Quantization::IQ4_NL,
        ];
        for q in &all_quants {
            assert!(
                q.memory_multiplier() > 0.0,
                "{} has non-positive memory_multiplier",
                q
            );
        }
    }

    // =========================================================================
    // ARCHITECTURE DISPLAY COMPLETENESS
    // =========================================================================

    #[test]
    fn test_architecture_display_all_variants() {
        // Ensure Display is implemented for all variants without panic
        let all_archs = [
            ModelArchitecture::Mistral,
            ModelArchitecture::Mixtral,
            ModelArchitecture::Llama,
            ModelArchitecture::Llama4,
            ModelArchitecture::Phi2,
            ModelArchitecture::Phi3,
            ModelArchitecture::Phi4,
            ModelArchitecture::Qwen2,
            ModelArchitecture::Qwen3,
            ModelArchitecture::Gemma,
            ModelArchitecture::Gemma2,
            ModelArchitecture::Starcoder2,
            ModelArchitecture::DeepSeek,
            ModelArchitecture::DeepSeekV2,
            ModelArchitecture::DeepSeekV3,
            ModelArchitecture::Cohere,
            ModelArchitecture::Dbrx,
            ModelArchitecture::Yi,
            ModelArchitecture::Falcon,
            ModelArchitecture::Bloom,
            ModelArchitecture::Mpt,
            ModelArchitecture::Internlm2,
            ModelArchitecture::LLaVA,
            ModelArchitecture::LLaVANext,
            ModelArchitecture::Qwen2VL,
            ModelArchitecture::Phi3V,
            ModelArchitecture::Idefics2,
            ModelArchitecture::Pixtral,
            ModelArchitecture::Bert,
            ModelArchitecture::Nomic,
            ModelArchitecture::Jina,
            ModelArchitecture::Unknown,
        ];
        for arch in &all_archs {
            let s = arch.to_string();
            assert!(!s.is_empty(), "Architecture {:?} has empty display", arch);
        }
    }

    // =========================================================================
    // AUTO_SELECT_QUANTIZATION EDGE CASES
    // =========================================================================

    #[test]
    fn test_auto_select_quantization_zero_ram() {
        let model = find_model("qwen3:8b").unwrap();
        // With 0 RAM, should fall back to last quantization
        let quant = auto_select_quantization(model, 0);
        // Should pick the last in the list (smallest)
        let last_quant = model.quantizations.last().unwrap().0;
        assert_eq!(quant, last_quant);
    }

    #[test]
    fn test_auto_select_quantization_single_quant_model() {
        // qwen3:1.7b has only Q8_0
        let model = find_model("qwen3:1.7b").unwrap();
        assert_eq!(model.quantizations.len(), 1);
        let quant = auto_select_quantization(model, 32);
        assert_eq!(quant, Quantization::Q8_0);
    }

    #[test]
    fn test_auto_select_quantization_huge_ram() {
        let model = find_model("qwen3:8b").unwrap();
        // With 256GB RAM, should pick first (highest quality) quantization
        let quant = auto_select_quantization(model, 256);
        let first_quant = model.quantizations.first().unwrap().0;
        assert_eq!(quant, first_quant);
    }

    // =========================================================================
    // ERROR DISPLAY
    // =========================================================================

    #[test]
    fn test_model_resolve_error_display() {
        let err = ModelResolveError::UnknownModel {
            id: "foo".to_string(),
        };
        assert_eq!(err.to_string(), "Unknown model: foo");

        let err = ModelResolveError::QuantizationNotAvailable {
            quantization: Quantization::F16,
            model_id: "bar".to_string(),
        };
        assert_eq!(err.to_string(), "Quantization F16 not available for bar");

        let err = ModelResolveError::HomeDirectoryNotFound;
        assert_eq!(err.to_string(), "Cannot determine home directory");

        let err = ModelResolveError::ModelNotDownloaded {
            model_id: "baz".to_string(),
        };
        assert!(err.to_string().contains("nika model pull baz"));

        let err = ModelResolveError::NoSnapshotsFound {
            model_id: "qux".to_string(),
        };
        assert_eq!(err.to_string(), "No snapshots found for qux");

        let err = ModelResolveError::ModelFileNotFound {
            path: PathBuf::from("/tmp/model.gguf"),
            model_id: "quux".to_string(),
        };
        assert!(err.to_string().contains("/tmp/model.gguf"));
        assert!(err.to_string().contains("nika model pull quux"));

        let err = ModelResolveError::SnapshotsDirReadError {
            path: PathBuf::from("/tmp/snapshots"),
            message: "permission denied".to_string(),
        };
        assert!(err.to_string().contains("/tmp/snapshots"));
        assert!(err.to_string().contains("permission denied"));
    }

    // =========================================================================
    // KNOWN_MODEL DATA INVARIANTS
    // =========================================================================

    #[test]
    fn test_all_models_have_valid_data() {
        for model in KNOWN_MODELS {
            assert!(!model.id.is_empty());
            assert!(!model.name.is_empty());
            assert!(!model.hf_repo.is_empty());
            assert!(!model.default_file.is_empty());
            assert!(!model.quantizations.is_empty());
            assert!(model.param_billions > 0.0);
            assert!(model.min_ram_gb > 0);
            assert!(!model.description.is_empty());

            // Default file should be in quantizations list
            let default_in_quants = model
                .quantizations
                .iter()
                .any(|(_, f)| *f == model.default_file);
            assert!(
                default_in_quants,
                "Model {} default_file not in quantizations",
                model.id
            );
        }
    }
}
