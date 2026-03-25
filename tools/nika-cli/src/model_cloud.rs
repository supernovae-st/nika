//! Cloud model listing — always available (no native-inference feature required).
//!
//! Shows all cloud LLM models with pricing, grouped by provider.

use colored::Colorize;
use nika_engine::error::NikaError;
use nika_engine::provider::cost::{list_provider_models, ModelPricing, ProviderKind};

/// Provider display info.
struct ProviderDisplay {
    name: &'static str,
    kind: ProviderKind,
    env_var: &'static str,
}

const CLOUD_PROVIDERS: &[ProviderDisplay] = &[
    ProviderDisplay {
        name: "ANTHROPIC",
        kind: ProviderKind::Claude,
        env_var: "ANTHROPIC_API_KEY",
    },
    ProviderDisplay {
        name: "OPENAI",
        kind: ProviderKind::OpenAI,
        env_var: "OPENAI_API_KEY",
    },
    ProviderDisplay {
        name: "MISTRAL",
        kind: ProviderKind::Mistral,
        env_var: "MISTRAL_API_KEY",
    },
    ProviderDisplay {
        name: "GROQ",
        kind: ProviderKind::Groq,
        env_var: "GROQ_API_KEY",
    },
    ProviderDisplay {
        name: "DEEPSEEK",
        kind: ProviderKind::DeepSeek,
        env_var: "DEEPSEEK_API_KEY",
    },
    ProviderDisplay {
        name: "GEMINI",
        kind: ProviderKind::Gemini,
        env_var: "GEMINI_API_KEY",
    },
    ProviderDisplay {
        name: "XAI",
        kind: ProviderKind::XAi,
        env_var: "XAI_API_KEY",
    },
];

/// Display all cloud models with pricing, grouped by provider.
pub fn print_cloud_models(filter_provider: Option<&str>) -> Result<(), NikaError> {
    println!();
    println!(
        "  {}{}",
        "Available Models".bold(),
        "                          input / output per M tokens".dimmed()
    );
    println!("  {}", "─".repeat(62));

    let mut any_shown = false;

    for p in CLOUD_PROVIDERS {
        // Filter by provider if specified
        if let Some(filter) = filter_provider {
            if !p.name.eq_ignore_ascii_case(filter) {
                continue;
            }
        }

        let has_key = std::env::var(p.env_var).is_ok_and(|v| !v.trim().is_empty());
        let status = if has_key {
            format!("{} key set", "✓".green())
        } else {
            format!("{} no key", "✗".red())
        };

        let models = list_provider_models(p.kind);
        if models.is_empty() {
            continue;
        }

        println!();
        println!("  {} ({})", p.name.bold(), status);

        if !has_key {
            println!(
                "  {} nika provider set {}",
                "→".dimmed(),
                p.name.to_lowercase().dimmed()
            );
        } else {
            for (i, (name, pricing)) in models.iter().enumerate() {
                let is_last = i == models.len() - 1;
                let prefix = if is_last { "└──" } else { "├──" };
                println!(
                    "  {} {:<30} ${:.2} / ${:.2}",
                    prefix.dimmed(),
                    name.cyan(),
                    pricing.input_per_million,
                    pricing.output_per_million,
                );
            }
        }
        any_shown = true;
    }

    if !any_shown {
        println!("  No providers matched filter.");
    }

    println!();
    println!("  {} nika infer \"...\" -m <model>", "Use:".dimmed());
    println!(
        "  {} nika config set default_model <model>",
        "Default:".dimmed()
    );
    println!();

    Ok(())
}

/// Show detailed info for a specific cloud model.
pub fn print_model_info(model_name: &str) -> Result<(), NikaError> {
    // Search all providers for this model
    for p in CLOUD_PROVIDERS {
        let models = list_provider_models(p.kind);
        for (name, pricing) in &models {
            if *name == model_name {
                let has_key = std::env::var(p.env_var).is_ok_and(|v| !v.trim().is_empty());
                println!();
                println!("  {} ({})", name.bold().cyan(), p.name);
                println!("  {}", "─".repeat(40));
                println!("  Provider:  {}", p.name.to_lowercase());
                println!(
                    "  Pricing:   ${:.2} input / ${:.2} output per M tokens",
                    pricing.input_per_million, pricing.output_per_million
                );
                println!(
                    "  Status:    {}",
                    if has_key {
                        "✓ API key available".green().to_string()
                    } else {
                        format!("✗ Set key: nika provider set {}", p.name.to_lowercase())
                            .red()
                            .to_string()
                    }
                );
                println!();
                println!("  Use: nika infer \"...\" -m {}", name);
                println!();
                return Ok(());
            }
        }
    }

    Err(NikaError::ValidationError {
        reason: format!("Model '{}' not found. Run: nika model list", model_name),
    })
}

/// Smart model recommendation based on available API keys.
pub fn print_model_recommend() -> Result<(), NikaError> {
    println!();
    println!("  {}", "Model Recommendation".bold());
    println!("  {}", "─".repeat(40));

    let mut available: Vec<(&str, &str, ModelPricing)> = Vec::new();

    for p in CLOUD_PROVIDERS {
        let has_key = std::env::var(p.env_var).is_ok_and(|v| !v.trim().is_empty());
        if !has_key {
            continue;
        }
        let models = list_provider_models(p.kind);
        for (name, pricing) in models {
            available.push((name, p.name, pricing));
        }
    }

    if available.is_empty() {
        println!("  No API keys configured.");
        println!("  Run: nika provider set <provider>");
        println!();
        return Ok(());
    }

    // Sort by output cost (most representative for recommendation)
    available.sort_by(|(_, _, a), (_, _, b)| {
        a.output_per_million
            .partial_cmp(&b.output_per_million)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Budget: cheapest
    if let Some((name, provider, pricing)) = available.first() {
        println!(
            "  {} {:<28} ${:.2}/${:.2}  [{}]",
            "⚡ Budget:".yellow(),
            name.cyan(),
            pricing.input_per_million,
            pricing.output_per_million,
            provider.to_lowercase().dimmed()
        );
    }

    // Quality: most expensive
    if let Some((name, provider, pricing)) = available.last() {
        println!(
            "  {} {:<28} ${:.2}/${:.2}  [{}]",
            "🏆 Quality:".bold(),
            name.cyan(),
            pricing.input_per_million,
            pricing.output_per_million,
            provider.to_lowercase().dimmed()
        );
    }

    // Balanced: mid-range (prefer sonnet/gpt-4o)
    let balanced = available
        .iter()
        .find(|(name, _, _)| name.contains("sonnet") || *name == "gpt-4o")
        .or_else(|| available.get(available.len() / 2));
    if let Some((name, provider, pricing)) = balanced {
        println!(
            "  {} {:<28} ${:.2}/${:.2}  [{}]",
            "★ Balanced:".green(),
            name.cyan(),
            pricing.input_per_million,
            pricing.output_per_million,
            provider.to_lowercase().dimmed()
        );
    }

    println!();
    println!("  Set default: nika config set default_model <model>");
    println!();

    Ok(())
}
