// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Showcase subcommand handler — list and extract showcase workflows
//!
//! Source: init showcases — patterns (15), advanced (15), infra (15), fetch (15) — `WorkflowTemplate`

use std::path::{Path, PathBuf};

use clap::Subcommand;
use colored::Colorize;

use nika_engine::error::NikaError;
use nika_init::WorkflowTemplate;

/// Showcase subcommand actions
#[derive(Subcommand)]
pub enum ShowcaseAction {
    /// List all showcase workflows (default)
    List {
        /// Filter by category (e.g., llm, builtin, exec, content, system, core, file, media)
        #[arg(short, long)]
        category: Option<String>,
    },
    /// Extract a showcase workflow to the current directory
    Extract {
        /// Workflow name (e.g., "blog-post-generator") or --all
        name: Option<String>,

        /// Extract all showcase workflows to ./nika-showcase/
        #[arg(long)]
        all: bool,

        /// Output directory (default: current directory, or ./nika-showcase/ with --all)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

/// Entry point for `nika showcase <action>`
pub fn handle_showcase_command(action: ShowcaseAction, quiet: bool) -> Result<(), NikaError> {
    match action {
        ShowcaseAction::List { category } => cmd_list(category.as_deref(), quiet),
        ShowcaseAction::Extract { name, all, output } => {
            if all {
                let dir = output.unwrap_or_else(|| PathBuf::from("nika-showcase"));
                cmd_extract_all(&dir, quiet)
            } else if let Some(name) = name {
                let dir = output.unwrap_or_else(|| PathBuf::from("."));
                cmd_extract(&name, &dir, quiet)
            } else {
                Err(NikaError::ValidationError {
                    reason: "Provide a workflow name or use --all. Try: nika showcase list"
                        .to_string(),
                })
            }
        }
    }
}

// ── Unified entry ───────────────────────────────────────────────────────────

/// A unified view over both showcase types for display and extraction.
struct ShowcaseEntry {
    name: &'static str,
    description: &'static str,
    category: &'static str,
    content: &'static str,
    requires_llm: bool,
    /// Source group for display (e.g., "course/builtin", "init/patterns")
    source: &'static str,
}

/// Collect all showcase workflows from every source.
fn all_showcases() -> Vec<ShowcaseEntry> {
    let mut entries = Vec::with_capacity(60);

    let init_workflows = nika_init::get_all_workflows();
    for w in &init_workflows {
        // Skip minimal starters — they are not really showcases
        if w.tier_dir == "minimal" {
            continue;
        }
        entries.push(from_template(w));
    }

    entries
}

fn from_template(w: &WorkflowTemplate) -> ShowcaseEntry {
    // Derive name from filename: "01-exec.nika.yaml" -> "01-exec"
    let name_str = w.filename.trim_end_matches(".nika.yaml");

    // Detect LLM requirement from content
    let requires_llm = w.content.contains("infer:") || w.content.contains("agent:");

    ShowcaseEntry {
        name: leak_str(name_str),
        description: leak_str(&format!("{} workflow", w.tier_dir)),
        category: leak_str(w.tier_dir),
        content: w.content,
        requires_llm,
        source: leak_str(&format!("init/{}", w.tier_dir)),
    }
}

/// Leak a string for the static lifetime required by ShowcaseEntry.
/// This is fine — we only call it once at list time, not in a loop.
fn leak_str(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

// ── List ────────────────────────────────────────────────────────────────────

fn cmd_list(category: Option<&str>, _quiet: bool) -> Result<(), NikaError> {
    let entries = all_showcases();

    let filtered: Vec<&ShowcaseEntry> = if let Some(cat) = category {
        let cat_lower = cat.to_lowercase();
        entries
            .iter()
            .filter(|e| {
                e.category.to_lowercase().contains(&cat_lower)
                    || e.source.to_lowercase().contains(&cat_lower)
            })
            .collect()
    } else {
        entries.iter().collect()
    };

    if filtered.is_empty() {
        println!(
            "\n  {} No showcase workflows found{}.\n",
            "!".yellow(),
            category
                .map(|c| format!(" for category '{}'", c))
                .unwrap_or_default()
        );
        return Ok(());
    }

    println!();
    println!(
        "  {} ({} workflows)",
        "Nika Showcase Workflows".cyan().bold(),
        filtered.len()
    );
    println!();

    // Group by source for clean display
    let mut current_source = "";
    for entry in &filtered {
        if entry.source != current_source {
            current_source = entry.source;
            println!("  {}", format!("-- {} --", current_source).dimmed());
        }

        let llm_badge = if entry.requires_llm {
            " [LLM]".yellow().to_string()
        } else {
            String::new()
        };

        println!(
            "    {:<32} {}{}",
            entry.name.bold(),
            entry.description.dimmed(),
            llm_badge
        );
    }

    println!();
    println!("  {}", "Extract: nika showcase extract <name>".dimmed());
    println!("  {}", "Extract all: nika showcase extract --all".dimmed());
    println!();

    Ok(())
}

// ── Extract ─────────────────────────────────────────────────────────────────

/// Substitute `{{PROVIDER}}` and `{{MODEL}}` placeholders with auto-detected values.
///
/// Provider detection uses env vars in priority order; model falls back to a
/// sensible default per provider.
fn substitute_placeholders(content: &str) -> String {
    let (provider, model) = detect_provider_and_model();
    content
        .replace("{{PROVIDER}}", provider)
        .replace("{{MODEL}}", model)
}

/// Map an API-key-backed provider to a default model.
fn detect_provider_and_model() -> (&'static str, &'static str) {
    if std::env::var("ANTHROPIC_API_KEY").is_ok_and(|v| !v.trim().is_empty()) {
        ("anthropic", "claude-sonnet-4-20250514")
    } else if std::env::var("OPENAI_API_KEY").is_ok_and(|v| !v.trim().is_empty()) {
        ("openai", "gpt-4o-mini")
    } else if std::env::var("GROQ_API_KEY").is_ok_and(|v| !v.trim().is_empty()) {
        ("groq", "llama-3.3-70b-versatile")
    } else if std::env::var("MISTRAL_API_KEY").is_ok_and(|v| !v.trim().is_empty()) {
        ("mistral", "mistral-large-latest")
    } else if std::env::var("GEMINI_API_KEY").is_ok_and(|v| !v.trim().is_empty()) {
        ("gemini", "gemini-2.5-flash")
    } else if std::env::var("DEEPSEEK_API_KEY").is_ok_and(|v| !v.trim().is_empty()) {
        ("deepseek", "deepseek-chat")
    } else if std::env::var("XAI_API_KEY").is_ok_and(|v| !v.trim().is_empty()) {
        ("xai", "grok-3")
    } else {
        ("anthropic", "claude-sonnet-4-20250514")
    }
}

fn cmd_extract(name: &str, output_dir: &Path, quiet: bool) -> Result<(), NikaError> {
    let entries = all_showcases();

    let entry = entries.iter().find(|e| e.name == name).ok_or_else(|| {
        // Suggest close matches
        let suggestions: Vec<&str> = entries
            .iter()
            .filter(|e| e.name.contains(name) || name.contains(e.name))
            .map(|e| e.name)
            .take(5)
            .collect();

        let hint = if suggestions.is_empty() {
            "Try: nika showcase list".to_string()
        } else {
            format!("Did you mean: {}?", suggestions.join(", "))
        };

        NikaError::ValidationError {
            reason: format!("Showcase workflow '{}' not found. {}", name, hint),
        }
    })?;

    let filename = format!("{}.nika.yaml", entry.name);
    let dest = output_dir.join(&filename);

    // Ensure parent directory exists
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(NikaError::IoError)?;
    }

    let content = substitute_placeholders(entry.content);
    std::fs::write(&dest, content).map_err(NikaError::IoError)?;

    if !quiet {
        println!(
            "\n  {} Extracted: {}\n",
            "OK".green().bold(),
            dest.display()
        );
    }

    Ok(())
}

fn cmd_extract_all(output_dir: &Path, quiet: bool) -> Result<(), NikaError> {
    let entries = all_showcases();

    std::fs::create_dir_all(output_dir).map_err(NikaError::IoError)?;

    let mut count = 0;
    let mut by_category: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();

    for entry in &entries {
        // Organize into subdirectories by category
        let cat_dir = output_dir.join(entry.category);
        std::fs::create_dir_all(&cat_dir).map_err(NikaError::IoError)?;

        let filename = format!("{}.nika.yaml", entry.name);
        let dest = cat_dir.join(&filename);

        let content = substitute_placeholders(entry.content);
        std::fs::write(&dest, content).map_err(NikaError::IoError)?;

        count += 1;
        *by_category.entry(entry.category).or_insert(0) += 1;
    }

    if !quiet {
        println!();
        println!(
            "  {} Extracted {} workflows to {}",
            "OK".green().bold(),
            count,
            output_dir.display()
        );
        let mut cats: Vec<_> = by_category.iter().collect();
        cats.sort_by_key(|(name, _)| *name);
        for (cat, n) in cats {
            println!("    {}: {} workflows", cat, n);
        }
        println!();
    }

    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_showcases_not_empty() {
        let entries = all_showcases();
        assert!(
            !entries.is_empty(),
            "Should have at least one showcase workflow, got {}",
            entries.len()
        );
    }

    #[test]
    fn test_all_showcases_have_content() {
        for entry in all_showcases() {
            assert!(
                !entry.content.is_empty(),
                "Showcase '{}' should have content",
                entry.name
            );
        }
    }

    #[test]
    fn test_all_showcases_have_name() {
        for entry in all_showcases() {
            assert!(!entry.name.is_empty(), "Every showcase should have a name");
        }
    }

    #[test]
    fn test_extract_to_tempdir() {
        let dir = tempfile::tempdir().unwrap();
        let entries = all_showcases();
        let first = entries.first().expect("at least one showcase entry");
        let name = first.name;
        let result = cmd_extract(name, dir.path(), true);
        assert!(result.is_ok());
        assert!(dir.path().join(format!("{}.nika.yaml", name)).exists());
    }

    #[test]
    fn test_extract_unknown_name() {
        let dir = tempfile::tempdir().unwrap();
        let result = cmd_extract("nonexistent-workflow-xyz", dir.path(), true);
        assert!(result.is_err());
    }

    #[test]
    fn test_substitute_placeholders() {
        let content = "provider: {{PROVIDER}}\nmodel: {{MODEL}}\n";
        let result = substitute_placeholders(content);
        assert!(
            !result.contains("{{PROVIDER}}"),
            "{{{{PROVIDER}}}} should be substituted"
        );
        assert!(
            !result.contains("{{MODEL}}"),
            "{{{{MODEL}}}} should be substituted"
        );
        // Should contain actual provider/model values
        assert!(result.contains("provider:"));
        assert!(result.contains("model:"));
    }

    #[test]
    fn test_extract_substitutes_placeholders() {
        let dir = tempfile::tempdir().unwrap();
        let entries = all_showcases();
        let first = entries.first().expect("at least one showcase entry");
        let name = first.name;
        let result = cmd_extract(name, dir.path(), true);
        assert!(result.is_ok());
        let content = std::fs::read_to_string(dir.path().join(format!("{}.nika.yaml", name)))
            .unwrap();
        // Extracted content should not contain raw placeholders
        assert!(
            !content.contains("{{PROVIDER}}"),
            "Extracted file should not contain raw {{{{PROVIDER}}}} placeholder"
        );
        assert!(
            !content.contains("{{MODEL}}"),
            "Extracted file should not contain raw {{{{MODEL}}}} placeholder"
        );
    }
}
