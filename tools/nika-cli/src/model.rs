// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Model management subcommand handler

use clap::Subcommand;

use nika_engine::display::{separator, StatusIcon};
use nika_engine::error::NikaError;

/// Model management actions
///
/// Model management for local GGUF models.
#[derive(Subcommand)]
pub enum ModelAction {
    /// List downloaded models in ~/.nika/models/
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Download a model from HuggingFace
    ///
    /// Supports curated models from KNOWN_MODELS or custom HuggingFace repos.
    /// Example: nika model pull qwen3:8b
    /// Example: nika model pull --repo TheBloke/Mistral-7B-v0.1-GGUF --file mistral-7b-v0.1.Q4_K_M.gguf
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

    /// Show information about a model
    ///
    /// Shows curated model info from KNOWN_MODELS or local file info.
    Info {
        /// Model name or path
        name: String,
    },

    /// Show status of loaded models
    Status,

    /// Delete a downloaded model
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
    /// Example: nika model vision google/gemma-3-4b-it --isq Q8_0
    Vision {
        /// HuggingFace model ID (e.g., Qwen/Qwen2.5-VL-7B-Instruct)
        model_id: String,

        /// ISQ quantization level (e.g., Q4K, Q8_0, Q6K)
        /// Reduces memory usage by quantizing weights after loading.
        #[arg(long)]
        isq: Option<String>,

        /// Context size (token window, default: 4096)
        #[arg(long, default_value = "4096")]
        context_size: u32,
    },
}

/// Find filename for a given quantization string in a known model.
/// Returns None if quant string is invalid or not available for this model.
fn find_filename_for_quant(
    model: &nika_engine::core::KnownModel,
    quant_str: &str,
) -> Option<&'static str> {
    use nika_engine::core::Quantization;
    let target = match quant_str.to_uppercase().as_str() {
        "Q4_K_S" => Quantization::Q4_K_S,
        "Q4_K_M" => Quantization::Q4_K_M,
        "Q5_K_S" => Quantization::Q5_K_S,
        "Q5_K_M" => Quantization::Q5_K_M,
        "Q6_K" => Quantization::Q6_K,
        "Q8_0" => Quantization::Q8_0,
        "F16" => Quantization::F16,
        _ => return None,
    };
    model
        .quantizations
        .iter()
        .find(|(q, _)| *q == target)
        .map(|(_, f)| *f)
}

/// Handle model management commands
pub async fn handle_model_command(action: ModelAction, quiet: bool) -> Result<(), NikaError> {
    use colored::Colorize;
    use nika_engine::core::{find_model, KNOWN_MODELS};
    use nika_engine::provider::{
        default_model_dir, DownloadRequest, HuggingFaceStorage, PullProgress,
    };

    let storage =
        HuggingFaceStorage::new(default_model_dir()).map_err(|e| NikaError::ConfigError {
            reason: format!("Failed to initialize storage: {e}"),
        })?;

    match action {
        ModelAction::List { json } => {
            let models = storage.list_models().map_err(|e| NikaError::ConfigError {
                reason: format!("Failed to list models: {e}"),
            })?;

            if json {
                let output: Vec<serde_json::Value> = models
                    .iter()
                    .map(|m| {
                        serde_json::json!({
                            "name": m.name,
                            "size": m.size,
                            "quantization": m.quantization,
                            "parameters": m.parameters,
                            "digest": m.digest,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else if models.is_empty() {
                if !quiet {
                    println!("{} No models downloaded yet.", StatusIcon::Info);
                    println!(
                        "{}",
                        "Use 'nika model pull <name>' to download a model.".dimmed()
                    );
                    println!();
                    println!("{}", "Available models:".bold());
                    for model in KNOWN_MODELS.iter().take(5) {
                        println!("  • {} - {}", model.id.cyan(), model.description);
                    }
                    if KNOWN_MODELS.len() > 5 {
                        println!(
                            "  {} more available...",
                            (KNOWN_MODELS.len() - 5).to_string().dimmed()
                        );
                    }
                }
            } else {
                println!("{}", "Downloaded Models".bold());
                println!("{}", separator(70));
                for model in &models {
                    let size_mb = model.size / (1024 * 1024);
                    let quant = model.quantization.as_deref().unwrap_or("?");
                    println!(
                        "  {:30} {:>8} MB  {}",
                        model.name.cyan(),
                        size_mb,
                        quant.dimmed()
                    );
                }
                println!();
                println!(
                    "{} models, {} total",
                    models.len(),
                    format_size(models.iter().map(|m| m.size).sum())
                );
            }
            Ok(())
        }

        ModelAction::Pull {
            name,
            repo,
            file,
            quant,
            force,
        } => {
            // Determine download target - always use passthrough mode (hf_repo + filename)
            let (hf_repo, hf_file) = match (&name, &repo, &file) {
                // Named model from KNOWN_MODELS - extract repo/file
                (Some(model_name), None, None) => {
                    let model =
                        find_model(model_name).ok_or_else(|| NikaError::ValidationError {
                            reason: format!(
                            "Unknown model: '{model_name}'. Use 'nika model list' to see available models."
                        ),
                        })?;

                    // Determine filename based on quantization or default
                    let filename = if let Some(q) = &quant {
                        find_filename_for_quant(model, q).ok_or_else(|| {
                            NikaError::ValidationError {
                                reason: format!(
                                    "Invalid or unavailable quantization: {}. Available: {:?}",
                                    q,
                                    model
                                        .quantizations
                                        .iter()
                                        .map(|(q, _)| format!("{q:?}"))
                                        .collect::<Vec<_>>()
                                ),
                            }
                        })?
                    } else {
                        model.default_file
                    };

                    (model.hf_repo.to_string(), filename.to_string())
                }
                // Custom HF repo (passthrough)
                (None, Some(hf_repo), Some(hf_file)) => (hf_repo.clone(), hf_file.clone()),
                // Invalid combination
                _ => {
                    return Err(NikaError::ValidationError {
                        reason: "Specify either a model name OR --repo and --file".to_string(),
                    });
                }
            };

            // Check if already exists
            let model_path = default_model_dir().join(&hf_file);
            if model_path.exists() && !force {
                if !quiet {
                    println!(
                        "{} Model already exists: {}",
                        StatusIcon::Info,
                        model_path.display()
                    );
                    println!("{}", "Use --force to re-download.".dimmed());
                }
                return Ok(());
            }

            if !quiet {
                println!(
                    "{} Downloading {} from {}...",
                    StatusIcon::Download,
                    hf_file.bold(),
                    hf_repo
                );
            }

            // Create download request using passthrough mode
            let request = DownloadRequest {
                model: None, // Don't use model field - types are incompatible
                hf_repo: Some(hf_repo),
                filename: Some(hf_file.clone()),
                quantization: None, // Quantization already resolved to filename
                force,
            };

            // Download with indicatif progress bar
            let pb = if !quiet {
                use indicatif::{ProgressBar, ProgressStyle};
                let pb = ProgressBar::new(0);
                pb.set_style(
                    ProgressStyle::default_bar()
                        .template(
                            "  {spinner:.cyan} {bar:40.cyan/dim} {percent}% ({bytes}/{total_bytes}) {bytes_per_sec} ETA {eta}",
                        )
                        .unwrap()
                        .progress_chars("━╸─"),
                );
                Some(pb)
            } else {
                None
            };

            let pb_clone = pb.clone();
            storage
                .download(
                    &request,
                    Box::new(move |progress: PullProgress| {
                        if let Some(ref pb) = pb_clone {
                            if progress.total > 0 {
                                pb.set_length(progress.total);
                                pb.set_position(progress.completed);
                            }
                        }
                    }),
                )
                .await
                .map_err(|e| NikaError::ConfigError {
                    reason: format!("Download failed: {e}"),
                })?;

            if let Some(pb) = pb {
                pb.finish_and_clear();
            }

            if !quiet {
                println!(
                    "{} Model downloaded: {}",
                    StatusIcon::Ok,
                    model_path.display()
                );
            }
            Ok(())
        }

        ModelAction::Info { name } => {
            // Try KNOWN_MODELS first
            if let Some(model) = find_model(&name) {
                println!("{}", format!("Model: {}", model.name).bold());
                println!("{}", separator(50));
                println!("  ID:           {}", model.id);
                println!("  Description:  {}", model.description);
                println!("  Repository:   {}", model.hf_repo.cyan());
                println!("  Parameters:   {}B", model.param_billions);
                println!("  Min RAM:      {} GB", model.min_ram_gb);
                println!("  Default File: {}", model.default_file);
                println!("  Quantizations:");
                for (quant, filename) in model.quantizations {
                    let path = default_model_dir().join(filename);
                    let status = if path.exists() {
                        "✓ downloaded".green().to_string()
                    } else {
                        "not downloaded".dimmed().to_string()
                    };
                    println!("    • {quant:?}: {filename} ({status})");
                }
                if let Some(meta) = nika_engine::provider::cost::get_model_meta(&name) {
                    let tags: Vec<&str> = meta.tags.iter().map(|t| t.label()).collect();
                    println!("  Tags:         {}", tags.join(", "));
                    println!(
                        "  Context:      {} tokens",
                        nika_engine::provider::cost::format_context_window(meta.context_window)
                    );
                }
            } else {
                // Try as local file path
                let path = if name.contains('/') || name.contains('.') {
                    std::path::PathBuf::from(&name)
                } else {
                    default_model_dir().join(&name)
                };

                if path.exists() {
                    let metadata = std::fs::metadata(&path)?;
                    let size_mb = metadata.len() / (1024 * 1024);
                    let quant = path
                        .file_name()
                        .and_then(|f| f.to_str())
                        .and_then(nika_engine::provider::native::extract_quantization);

                    println!("{}", format!("Model: {}", path.display()).bold());
                    println!("{}", separator(50));
                    println!("  Size:         {size_mb} MB");
                    println!(
                        "  Quantization: {}",
                        quant.unwrap_or_else(|| "Unknown".to_string())
                    );
                } else {
                    return Err(NikaError::ValidationError {
                        reason: format!("Model not found: {name}"),
                    });
                }
            }
            Ok(())
        }

        ModelAction::Status => {
            // For now, just show if NativeRuntime would be available
            // In the future, this could show loaded models in a daemon
            println!("{}", "Model Status".bold());
            println!("{}", separator(50));

            let models = storage.list_models().map_err(|e| NikaError::ConfigError {
                reason: format!("Failed to list models: {e}"),
            })?;

            if models.is_empty() {
                println!("{} No models available for inference.", StatusIcon::Info);
            } else {
                println!(
                    "{} {} models available for inference:",
                    StatusIcon::Ok,
                    models.len()
                );
                for model in models.iter().take(5) {
                    println!("  • {}", model.name.cyan());
                }
                if models.len() > 5 {
                    println!("  ... and {} more", models.len() - 5);
                }
            }
            println!();
            println!(
                "{}",
                "Use 'provider: native' in workflows for local inference.".dimmed()
            );
            Ok(())
        }

        ModelAction::Delete { name, force } => {
            // Find the model path
            let path = if name.contains('/') || name.contains('.') {
                std::path::PathBuf::from(&name)
            } else {
                default_model_dir().join(&name)
            };

            if !path.exists() {
                return Err(NikaError::ValidationError {
                    reason: format!("Model not found: {}", path.display()),
                });
            }

            // Confirm deletion
            if !force && !quiet {
                println!("{} Delete model: {}?", StatusIcon::Warn, path.display());
                print!("  Type 'yes' to confirm: ");
                let _ = std::io::Write::flush(&mut std::io::stdout());

                let mut input = String::new();
                std::io::stdin()
                    .read_line(&mut input)
                    .map_err(|e| NikaError::ValidationError {
                        reason: format!("Failed to read from stdin: {e}"),
                    })?;
                if input.trim() != "yes" {
                    println!("{}", "Cancelled.".dimmed());
                    return Ok(());
                }
            }

            std::fs::remove_file(&path)?;

            if !quiet {
                println!("{} Model deleted: {}", StatusIcon::Ok, path.display());
            }
            Ok(())
        }

        ModelAction::Vision {
            model_id,
            isq,
            context_size,
        } => {
            use nika_engine::core::backend::{LoadConfig, NativeModelKind};
            use nika_engine::provider::native::InferenceBackend;

            if !quiet {
                println!(
                    "{} Loading vision model: {}",
                    StatusIcon::Download,
                    model_id.bold()
                );
                if let Some(ref isq_level) = isq {
                    println!("  ISQ quantization: {}", isq_level.cyan());
                }
                println!("  Context size: {context_size} tokens");
                println!();
                println!(
                    "{}",
                    "This will download from HuggingFace and quantize in-situ.".dimmed()
                );
                println!(
                    "{}",
                    "First run may take several minutes depending on model size.".dimmed()
                );
                println!();
            }

            let config = LoadConfig {
                model_kind: NativeModelKind::VisionHf {
                    model_id: model_id.clone(),
                    isq: isq.clone(),
                },
                context_size: Some(context_size),
                ..Default::default()
            };

            let mut runtime = nika_engine::provider::native::NativeRuntime::new();

            // Load the vision model (downloads from HuggingFace, applies ISQ)
            runtime
                .load(std::path::PathBuf::new(), config)
                .await
                .map_err(|e| NikaError::ConfigError {
                    reason: format!("Failed to load vision model: {e}"),
                })?;

            if !quiet {
                let vision_status = if runtime.supports_vision() {
                    "vision".green().to_string()
                } else {
                    "text-only (unexpected)".yellow().to_string()
                };

                println!("{} Vision model loaded successfully!", StatusIcon::Ok);
                println!("  Model:       {}", model_id.cyan());
                println!("  Capability:  {vision_status}");
                if let Some(ref isq_level) = isq {
                    println!("  ISQ:         {isq_level}");
                }
                println!();
                println!(
                    "{}",
                    "Use 'provider: native' with 'content:' in workflows for vision inference."
                        .dimmed()
                );
            }
            Ok(())
        }
    }
}

/// Format bytes as human-readable size
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_size_bytes() {
        assert_eq!(format_size(500), "500 B");
    }

    #[test]
    fn format_size_kilobytes() {
        assert_eq!(format_size(1536), "1.5 KB");
    }

    #[test]
    fn format_size_megabytes() {
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn format_size_gigabytes() {
        assert_eq!(format_size(4 * 1024 * 1024 * 1024), "4.0 GB");
    }

    #[test]
    fn model_action_variants_exist() {
        let _ = ModelAction::List { json: false };
        let _ = ModelAction::Pull {
            name: None,
            repo: None,
            file: None,
            quant: None,
            force: false,
        };
        let _ = ModelAction::Delete {
            name: "test".into(),
            force: false,
        };
    }
}
