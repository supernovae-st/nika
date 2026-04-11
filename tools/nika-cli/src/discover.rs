// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Workflow discovery, resolution, and download helpers.
//!
//! Handles the chain: user reference → resolved filesystem path.
//! Resolution order: URL download → package `@name` → simple name →
//! filesystem path.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use colored::Colorize;
use nika_engine::error::NikaError;

/// Check if a file has the `.nika.yaml` or `.nika.yml` extension.
pub fn is_nika_workflow(file: &Path) -> bool {
    let filename = file
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    filename.ends_with(".nika.yaml") || filename.ends_with(".nika.yml")
}

/// Download a remote `.nika.yaml` workflow to `.nika/remote/` and return its path.
pub async fn download_remote_workflow(url: &str) -> Result<PathBuf, NikaError> {
    if !url.ends_with(".nika.yaml") && !url.ends_with(".yaml") {
        return Err(NikaError::WorkflowNotFound {
            path: format!("Remote URL must end with .nika.yaml or .yaml: {url}"),
        });
    }

    let filename = url
        .rsplit('/')
        .next()
        .unwrap_or("remote-workflow.nika.yaml");

    let remote_dir = PathBuf::from(".nika").join("remote");
    tokio::fs::create_dir_all(&remote_dir).await.map_err(|e| {
        NikaError::IoError(std::io::Error::other(format!(
            "Cannot create .nika/remote/: {e}"
        )))
    })?;

    let dest = remote_dir.join(filename);

    eprintln!("  {} Downloading {}", "→".cyan(), url.dimmed());

    // `kill_on_drop(true)` mandatory per invariant #11 — if the caller
    // future is dropped mid-await (e.g. Ctrl-C from `nika run`), curl
    // must be killed rather than left running to its 30 s max-time
    // timeout as an orphan.
    let output = tokio::process::Command::new("curl")
        .args(["-fsSL", "--max-time", "30", "-o"])
        .arg(&dest)
        .arg(url)
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| {
            NikaError::IoError(std::io::Error::other(format!(
                "curl not found or failed: {e}"
            )))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(NikaError::WorkflowNotFound {
            path: format!("Failed to download {url}: {stderr}"),
        });
    }

    let content = tokio::fs::read_to_string(&dest).await.map_err(|e| {
        NikaError::IoError(std::io::Error::other(format!(
            "Cannot read downloaded file: {e}"
        )))
    })?;

    if !content.contains("schema:") && !content.contains("tasks:") {
        let _ = tokio::fs::remove_file(&dest).await;
        return Err(NikaError::WorkflowNotFound {
            path: format!("Downloaded file from {url} does not appear to be a Nika workflow"),
        });
    }

    eprintln!("  {} Downloaded to {}", "✓".green(), dest.display());
    Ok(dest)
}

/// Resolve a workflow reference to an actual file path.
///
/// Resolution order:
/// 1. URL (http/https) → Download to `.nika/remote/`
/// 2. Simple name (no slash, no extension) → `.nika/workflows/{name}.nika.yaml`
/// 3. Filesystem path → as-is
pub async fn resolve_workflow_path(reference: &str) -> Result<PathBuf, NikaError> {
    // 1. Remote URL — download to temp and execute
    if reference.starts_with("http://") || reference.starts_with("https://") {
        return download_remote_workflow(reference).await;
    }

    // 2. Simple name without path separator or .yaml extension → try .nika/workflows/
    if !reference.contains('/')
        && !reference.ends_with(".nika.yaml")
        && !reference.ends_with(".yaml")
    {
        let local_path = PathBuf::from(".nika")
            .join("workflows")
            .join(format!("{reference}.nika.yaml"));

        if local_path.exists() {
            return Ok(local_path);
        }

        if !PathBuf::from(reference).exists() {
            return Err(NikaError::WorkflowNotFound {
                path: format!(
                    "Workflow '{reference}' not found in .nika/workflows/ or current directory"
                ),
            });
        }
    }

    // 3. Direct filesystem path
    let path = PathBuf::from(reference);
    if !path.exists() {
        return Err(NikaError::WorkflowNotFound {
            path: format!("File not found: {reference}. Check the path."),
        });
    }

    Ok(path)
}

/// Count `*.nika.yaml` files recursively, skipping hidden dirs and common junk.
pub fn count_nika_workflows(dir: &Path) -> usize {
    fn walk(dir: &Path, count: &mut usize) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if path.is_dir() {
                // Skip hidden, node_modules, target
                if name_str.starts_with('.') || name_str == "node_modules" || name_str == "target" {
                    continue;
                }
                walk(&path, count);
            } else if name_str.ends_with(".nika.yaml") {
                *count += 1;
            }
        }
    }
    let mut count = 0;
    walk(dir, &mut count);
    count
}

/// Auto-discover a workflow when no file argument is provided.
pub async fn resolve_or_discover_workflow(quiet: bool) -> Result<String, NikaError> {
    let workflows = discover_workflows(".").await?;
    match workflows.len() {
        0 => Err(NikaError::ValidationError {
            reason: "No .nika.yaml files found in current directory. Try: nika init".to_string(),
        }),
        1 => {
            if !quiet {
                eprintln!("  {} {}", "Auto-discovered:".dimmed(), workflows[0]);
            }
            Ok(workflows.into_iter().next().expect("len checked == 1"))
        }
        _ => pick_workflow(&workflows),
    }
}

/// Discover `.nika.yaml` files in the given directory (non-recursive).
pub async fn discover_workflows(dir: &str) -> Result<Vec<String>, NikaError> {
    let mut entries = tokio::fs::read_dir(dir)
        .await
        .map_err(|e| NikaError::ParseError {
            details: format!("Failed to read directory '{}': {}", dir, e),
        })?;
    let mut found = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| NikaError::ParseError {
            details: format!("Failed to read directory entry: {}", e),
        })?
    {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".nika.yaml") {
            found.push(name);
        }
    }
    found.sort();
    Ok(found)
}

/// Interactive workflow picker when multiple `.nika.yaml` files found.
pub fn pick_workflow(workflows: &[String]) -> Result<String, NikaError> {
    if !std::io::stdin().is_terminal() {
        return Err(NikaError::ValidationError {
            reason: format!(
                "Multiple .nika.yaml files found ({}). Specify one: nika run <file>",
                workflows.join(", ")
            ),
        });
    }
    let items: Vec<(String, String, &str)> = workflows
        .iter()
        .map(|w| (w.clone(), w.clone(), ""))
        .collect();
    let selected: String = cliclack::select("Which workflow?")
        .items(&items)
        .interact()
        .map_err(|e| NikaError::ValidationError {
            reason: format!("Workflow selection cancelled: {}", e),
        })?;
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_nika_workflow_accepts_nika_yaml() {
        assert!(is_nika_workflow(Path::new("my-flow.nika.yaml")));
        assert!(is_nika_workflow(Path::new("/home/user/flow.nika.yaml")));
    }

    #[test]
    fn is_nika_workflow_accepts_nika_yml() {
        assert!(is_nika_workflow(Path::new("flow.nika.yml")));
    }

    #[test]
    fn is_nika_workflow_rejects_plain_yaml() {
        assert!(!is_nika_workflow(Path::new("config.yaml")));
        assert!(!is_nika_workflow(Path::new("values.yml")));
    }

    #[test]
    fn is_nika_workflow_rejects_non_yaml() {
        assert!(!is_nika_workflow(Path::new("readme.md")));
        assert!(!is_nika_workflow(Path::new("main.rs")));
    }

    #[test]
    fn count_nika_workflows_in_tempdir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.nika.yaml"), "schema: x").unwrap();
        std::fs::write(dir.path().join("b.nika.yaml"), "schema: x").unwrap();
        std::fs::write(dir.path().join("c.yaml"), "not nika").unwrap();
        assert_eq!(count_nika_workflows(dir.path()), 2);
    }

    #[test]
    fn count_nika_workflows_skips_hidden_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let hidden = dir.path().join(".hidden");
        std::fs::create_dir(&hidden).unwrap();
        std::fs::write(hidden.join("x.nika.yaml"), "schema: x").unwrap();
        assert_eq!(count_nika_workflows(dir.path()), 0);
    }

    #[test]
    fn pick_workflow_rejects_non_tty() {
        // CI is non-TTY — should fail with a helpful message
        let workflows = vec!["a.nika.yaml".to_string(), "b.nika.yaml".to_string()];
        let err = pick_workflow(&workflows).unwrap_err().to_string();
        assert!(err.contains("Multiple"), "Error: {err}");
    }
}
