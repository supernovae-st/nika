//! Showcase subcommand handler — list and extract showcase workflows
//!
//! Combines all showcase workflow sources:
//! - Course showcases: builtin (15), llm (20), exec (20) — `ShowcaseWorkflow`
//! - Init showcases: patterns (15), advanced (15), infra (15), fetch (15) — `WorkflowTemplate`

use std::path::{Path, PathBuf};

use clap::Subcommand;
use colored::Colorize;

use nika_engine::error::NikaError;
use nika_init::course::showcase::ShowcaseWorkflow;
use nika_init::course::showcase_builtin::SHOWCASE_BUILTIN;
use nika_init::course::showcase_exec::SHOWCASE_EXEC;
use nika_init::course::showcase_llm::SHOWCASE_LLM;
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
    let mut entries = Vec::with_capacity(120);

    // Course showcase workflows (ShowcaseWorkflow type)
    for w in SHOWCASE_BUILTIN {
        entries.push(from_showcase(w, "course/builtin"));
    }
    for w in SHOWCASE_LLM {
        entries.push(from_showcase(w, "course/llm"));
    }
    for w in SHOWCASE_EXEC {
        entries.push(from_showcase(w, "course/exec"));
    }

    // Init showcase workflows (WorkflowTemplate type)
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

fn from_showcase(w: &'static ShowcaseWorkflow, source: &'static str) -> ShowcaseEntry {
    ShowcaseEntry {
        name: w.name,
        description: w.description,
        category: w.category,
        content: w.content,
        requires_llm: w.requires_llm,
        source,
    }
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
fn substitute_placeholders(content: &str) -> String {
    let (provider, model) = nika_init::course::generator::detect_provider();
    content
        .replace("{{PROVIDER}}", provider)
        .replace("{{MODEL}}", model)
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
            entries.len() > 100,
            "Should have 100+ showcase workflows, got {}",
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
    fn test_course_showcases_have_unique_names() {
        let course_entries: Vec<_> = all_showcases()
            .into_iter()
            .filter(|e| e.source.starts_with("course/"))
            .collect();
        let mut names: Vec<&str> = course_entries.iter().map(|e| e.name).collect();
        let n = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), n, "Course showcase names should be unique");
    }

    #[test]
    fn test_filter_by_category() {
        let entries = all_showcases();
        let content: Vec<_> = entries.iter().filter(|e| e.category == "content").collect();
        assert!(
            !content.is_empty(),
            "Should have content-category showcases"
        );
    }

    #[test]
    fn test_extract_to_tempdir() {
        let dir = tempfile::tempdir().unwrap();
        let result = cmd_extract("progress-tracker", dir.path(), true);
        assert!(result.is_ok());
        assert!(dir.path().join("progress-tracker.nika.yaml").exists());
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
        let result = cmd_extract("progress-tracker", dir.path(), true);
        assert!(result.is_ok());
        let content =
            std::fs::read_to_string(dir.path().join("progress-tracker.nika.yaml")).unwrap();
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
