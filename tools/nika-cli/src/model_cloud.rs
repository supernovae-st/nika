// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Cloud model listing with capability tags and context windows.

use colored::Colorize;
use nika_engine::display::{hint, tree_connector, StatusIcon};
use nika_engine::error::NikaError;
use nika_engine::provider::cost::{
    format_context_window, get_model_meta, list_provider_models, ModelPricing, ModelTag,
    ProviderKind,
};

struct ProviderDisplay {
    name: &'static str,
    kind: ProviderKind,
    env_var: &'static str,
    best_for: &'static str,
}

const CLOUD_PROVIDERS: &[ProviderDisplay] = &[
    ProviderDisplay {
        name: "ANTHROPIC",
        kind: ProviderKind::Claude,
        env_var: "ANTHROPIC_API_KEY",
        best_for: "reasoning, code",
    },
    ProviderDisplay {
        name: "OPENAI",
        kind: ProviderKind::OpenAI,
        env_var: "OPENAI_API_KEY",
        best_for: "versatile, ecosystem",
    },
    ProviderDisplay {
        name: "MISTRAL",
        kind: ProviderKind::Mistral,
        env_var: "MISTRAL_API_KEY",
        best_for: "multilingual, EU",
    },
    ProviderDisplay {
        name: "GROQ",
        kind: ProviderKind::Groq,
        env_var: "GROQ_API_KEY",
        best_for: "ultra-fast inference",
    },
    ProviderDisplay {
        name: "DEEPSEEK",
        kind: ProviderKind::DeepSeek,
        env_var: "DEEPSEEK_API_KEY",
        best_for: "budget reasoning",
    },
    ProviderDisplay {
        name: "GEMINI",
        kind: ProviderKind::Gemini,
        env_var: "GEMINI_API_KEY",
        best_for: "large context, multimodal",
    },
    ProviderDisplay {
        name: "XAI",
        kind: ProviderKind::XAi,
        env_var: "XAI_API_KEY",
        best_for: "real-time knowledge",
    },
];

fn format_tags(model_name: &str) -> String {
    match get_model_meta(model_name) {
        Some(meta) => {
            let tags: Vec<String> = meta
                .tags
                .iter()
                .map(|t| {
                    let label = t.label();
                    match t {
                        ModelTag::Reasoning => format!("[{}]", label.magenta()),
                        ModelTag::Code => format!("[{}]", label.blue()),
                        ModelTag::Balanced => format!("[{}]", label.green()),
                        ModelTag::Fast => format!("[{}]", label.yellow()),
                        ModelTag::Vision => format!("[{}]", label.cyan()),
                    }
                })
                .collect();
            let ctx = format_context_window(meta.context_window);
            format!("{} [{}]", tags.join(" "), ctx.dimmed())
        }
        None => String::new(),
    }
}

pub fn print_cloud_models(filter_provider: Option<&str>, json: bool) -> Result<(), NikaError> {
    if json {
        return print_cloud_models_json(filter_provider);
    }
    println!();
    println!(
        "  {}{}",
        "Cloud Models".bold(),
        "                              input / output per M tokens".dimmed()
    );
    println!("  {}", "═".repeat(72));
    let mut any_shown = false;
    for p in CLOUD_PROVIDERS {
        if let Some(filter) = filter_provider {
            if !p.name.eq_ignore_ascii_case(filter) {
                continue;
            }
        }
        let has_key = std::env::var(p.env_var).is_ok_and(|v| !v.trim().is_empty());
        let status = if has_key {
            format!("{}", StatusIcon::Ok)
        } else {
            format!("{}", StatusIcon::Fail)
        };
        let models = list_provider_models(p.kind);
        if models.is_empty() {
            continue;
        }
        println!();
        println!(
            "  {} {}  {}",
            status,
            p.name.bold(),
            format!("Best for: {}", p.best_for).dimmed()
        );
        if !has_key {
            println!(
                "  {} {}",
                StatusIcon::Hint,
                format!("nika keys set {}", p.name.to_lowercase()).dimmed()
            );
        } else {
            for (i, (name, pricing)) in models.iter().enumerate() {
                let is_last = i == models.len() - 1;
                let connector = tree_connector(is_last).dimmed();
                let tags = format_tags(name);
                println!(
                    "  {} {:<32} ${:>6.2} / ${:>6.2}  {}",
                    connector,
                    name.cyan(),
                    pricing.input_per_million,
                    pricing.output_per_million,
                    tags
                );
            }
        }
        any_shown = true;
    }
    if !any_shown {
        println!("  No providers matched filter.");
    }
    println!();
    println!(
        "{}",
        hint("nika infer \"...\" -m <model>             Run inference")
    );
    println!(
        "{}",
        hint("nika config set default_model <model>   Set default")
    );
    println!();
    Ok(())
}

fn print_cloud_models_json(filter_provider: Option<&str>) -> Result<(), NikaError> {
    let mut result: Vec<serde_json::Value> = Vec::new();
    for p in CLOUD_PROVIDERS {
        if let Some(filter) = filter_provider {
            if !p.name.eq_ignore_ascii_case(filter) {
                continue;
            }
        }
        let models = list_provider_models(p.kind);
        for (name, pricing) in models {
            let meta = get_model_meta(name);
            let mut obj = serde_json::json!({
                "provider": p.name.to_lowercase(),
                "model": name,
                "input_per_million": pricing.input_per_million,
                "output_per_million": pricing.output_per_million,
            });
            if let Some(m) = meta {
                obj["context_window"] = serde_json::json!(m.context_window);
                obj["tags"] =
                    serde_json::json!(m.tags.iter().map(|t| t.label()).collect::<Vec<_>>());
            }
            result.push(obj);
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&result).unwrap_or_default()
    );
    Ok(())
}

pub fn print_model_info(model_name: &str) -> Result<(), NikaError> {
    for p in CLOUD_PROVIDERS {
        for (name, pricing) in list_provider_models(p.kind) {
            if name == model_name {
                let has_key = std::env::var(p.env_var).is_ok_and(|v| !v.trim().is_empty());
                let meta = get_model_meta(name);
                println!();
                println!("  {} ({})", name.bold().cyan(), p.name);
                println!("  {}", "─".repeat(50));
                println!("    {:<14} {}", "Provider:".dimmed(), p.name.to_lowercase());
                println!(
                    "    {:<14} ${:.2} input / ${:.2} output per M tokens",
                    "Pricing:".dimmed(),
                    pricing.input_per_million,
                    pricing.output_per_million
                );
                if let Some(m) = meta {
                    println!(
                        "    {:<14} {} tokens",
                        "Context:".dimmed(),
                        format_context_window(m.context_window)
                    );
                    let tags: Vec<&str> = m.tags.iter().map(|t| t.label()).collect();
                    println!("    {:<14} {}", "Tags:".dimmed(), tags.join(", "));
                }
                println!(
                    "    {:<14} {}",
                    "Status:".dimmed(),
                    if has_key {
                        format!("{} API key available", StatusIcon::Ok)
                    } else {
                        format!(
                            "{} Set key: nika keys set {}",
                            StatusIcon::Fail,
                            p.name.to_lowercase()
                        )
                    }
                );
                println!();
                return Ok(());
            }
        }
    }
    Err(NikaError::ValidationError {
        reason: format!("Model '{}' not found. Run: nika model list", model_name),
    })
}

pub fn print_model_recommend() -> Result<(), NikaError> {
    println!();
    println!("  {}", "Model Recommendation".bold());
    println!("  {}", "─".repeat(50));
    let mut available: Vec<(&str, &str, ModelPricing)> = Vec::new();
    for p in CLOUD_PROVIDERS {
        if !std::env::var(p.env_var).is_ok_and(|v| !v.trim().is_empty()) {
            continue;
        }
        for (name, pricing) in list_provider_models(p.kind) {
            available.push((name, p.name, pricing));
        }
    }
    if available.is_empty() {
        println!("  No API keys configured.");
        println!("{}", hint("nika keys set <provider>"));
        println!();
        return Ok(());
    }
    available.sort_by(|a, b| {
        a.2.output_per_million
            .partial_cmp(&b.2.output_per_million)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if let Some((name, provider, pricing)) = available.first() {
        println!(
            "  {} {:<30} ${:.2}/${:.2}  [{}]",
            "⚡ Budget:".yellow(),
            name.cyan(),
            pricing.input_per_million,
            pricing.output_per_million,
            provider.to_lowercase().dimmed()
        );
    }
    if let Some((name, provider, pricing)) = available.last() {
        println!(
            "  {} {:<30} ${:.2}/${:.2}  [{}]",
            "★  Quality:".bold(),
            name.cyan(),
            pricing.input_per_million,
            pricing.output_per_million,
            provider.to_lowercase().dimmed()
        );
    }
    let balanced = available
        .iter()
        .find(|(n, _, _)| n.contains("sonnet") || *n == "gpt-4o")
        .or_else(|| available.get(available.len() / 2));
    if let Some((name, provider, pricing)) = balanced {
        println!(
            "  {} {:<30} ${:.2}/${:.2}  [{}]",
            "◆  Balanced:".green(),
            name.cyan(),
            pricing.input_per_million,
            pricing.output_per_million,
            provider.to_lowercase().dimmed()
        );
    }
    println!();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_providers_has_seven() {
        assert_eq!(CLOUD_PROVIDERS.len(), 7);
    }

    #[test]
    fn cloud_providers_all_have_best_for() {
        for p in CLOUD_PROVIDERS {
            assert!(!p.best_for.is_empty(), "{} missing best_for", p.name);
        }
    }

    #[test]
    fn format_tags_known_model() {
        let tags = format_tags("claude-sonnet-4-6");
        assert!(tags.contains("balanced") || tags.contains("code"));
    }

    #[test]
    fn format_tags_unknown_model() {
        assert!(format_tags("nonexistent-model").is_empty());
    }

    #[test]
    fn print_cloud_models_doesnt_panic() {
        let _ = print_cloud_models(Some("nonexistent"), false);
    }

    #[test]
    fn print_cloud_models_json_doesnt_panic() {
        let _ = print_cloud_models(None, true);
    }

    #[test]
    fn print_model_info_not_found() {
        assert!(print_model_info("nonexistent-model-xyz").is_err());
    }
}
