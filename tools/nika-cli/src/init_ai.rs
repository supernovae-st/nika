//! AI coding assistant file generation for `nika init`
//!
//! Generates project-level AI context files (AGENTS.md, VS Code settings,
//! git hooks). Editor-specific rules are installed at user scope by
//! `nika setup` / `machine.rs`.

use colored::Colorize;
use std::fs;
use std::path::Path;

/// Generate project-level AI integration files.
///
/// Editor-specific rules (Cursor, Copilot, Windsurf, Roo Code) are now
/// installed at user scope (`~/`) by `nika setup` via `machine.rs`.
/// This function only writes project-local files.
pub fn generate_ai_files(project_dir: &Path) -> Result<(), nika_engine::NikaError> {
    println!("\n  {}", "AI Integration".bold().underline());

    let mut count = 0;

    // AGENTS.md (lightweight project context)
    count += write_if_absent(
        &project_dir.join("AGENTS.md"),
        AGENTS_MD_CONTENT,
        "AGENTS.md",
    );

    // CLAUDE.md symlink
    let claude_md = project_dir.join("CLAUDE.md");
    if !claude_md.exists() {
        #[cfg(unix)]
        {
            match std::os::unix::fs::symlink("AGENTS.md", &claude_md) {
                Ok(()) => {
                    println!("  {} CLAUDE.md -> AGENTS.md (symlink)", "✓".green());
                    count += 1;
                }
                Err(e) => {
                    println!("  {} CLAUDE.md symlink failed: {}", "⚠".yellow(), e);
                }
            }
        }
    }

    // VS Code extensions.json
    count += write_if_absent_with_dir(
        project_dir,
        ".vscode/extensions.json",
        VSCODE_EXTENSIONS,
        "VS Code recommendations",
    );

    // VS Code settings (language association)
    count += write_if_absent_with_dir(
        project_dir,
        ".vscode/settings.json",
        VSCODE_SETTINGS,
        "VS Code settings",
    );

    // Git co-author hook
    let git_dir = project_dir.join(".git");
    if git_dir.exists() {
        let hook_path = git_dir.join("hooks/prepare-commit-msg");
        if !hook_path.exists() {
            fs::create_dir_all(git_dir.join("hooks")).ok();
            match fs::write(&hook_path, GIT_HOOK) {
                Ok(()) => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o755)).ok();
                    }
                    println!("  {} Git co-author hook", "✓".green());
                    count += 1;
                }
                Err(e) => {
                    println!("  {} Git co-author hook failed: {}", "⚠".yellow(), e);
                }
            }
        }
    }

    println!(
        "\n  {} {} project file(s) created",
        "✓".green(),
        count
    );

    Ok(())
}

fn write_if_absent(path: &Path, content: &str, label: &str) -> usize {
    if path.exists() {
        return 0;
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    match fs::write(path, content) {
        Ok(()) => {
            println!("  {} {}", "✓".green(), label);
            1
        }
        Err(e) => {
            println!("  {} {} — {}", "✗".red(), label, e);
            0
        }
    }
}

fn write_if_absent_with_dir(base: &Path, rel_path: &str, content: &str, label: &str) -> usize {
    let path = base.join(rel_path);
    write_if_absent(&path, content, label)
}

// ─── Embedded Content ─────────────────────────────────────────────────────────
//
// Editor-specific rules (Cursor, Copilot, Windsurf, Roo Code) have been moved
// to machine.rs and are installed at user scope (~/) by `nika setup`.
// Only project-local content remains here.

const VSCODE_EXTENSIONS: &str = r#"{
  "recommendations": [
    "supernovae-studio.nika-lang",
    "redhat.vscode-yaml"
  ]
}
"#;

const VSCODE_SETTINGS: &str = r#"{
  "files.associations": {
    "*.nika.yaml": "nika"
  },
  "[nika]": {
    "editor.tabSize": 2,
    "editor.insertSpaces": true
  }
}
"#;

const GIT_HOOK: &str = r#"#!/bin/sh
# Nika co-author hook
COMMIT_MSG_FILE=$1
COMMIT_SOURCE=$2
case "$COMMIT_SOURCE" in merge|squash) exit 0 ;; esac
if grep -q "Co-Authored-By:" "$COMMIT_MSG_FILE" 2>/dev/null; then exit 0; fi
if git diff --cached --name-only | grep -q '\.nika\.yaml$'; then
    printf '\n\nCo-Authored-By: Nika 🦋 <nika@supernovae.studio>\n' >> "$COMMIT_MSG_FILE"
fi
"#;

const AGENTS_MD_CONTENT: &str = r#"# Nika Workflows

This project uses [Nika](https://github.com/supernovae-st/nika) — a semantic YAML workflow engine for AI tasks.

**Schema:** `nika/workflow@0.12` | **Extension:** `.nika.yaml`

## Workflows

| Directory | Contents |
|-----------|----------|
| `workflows/minimal/` | 5 starter workflows (1 per verb) |
| `workflows/showcase*/` | 60 showcase examples |

## Quick Start

```bash
# No API key needed
nika run workflows/minimal/01-exec.nika.yaml

# Validate syntax
nika check <workflow.nika.yaml>

# Check provider status
nika provider list

# Start the interactive course
nika course next
```

## Nika Quick Reference

**5 Verbs:** `infer:` (LLM) · `exec:` (shell) · `fetch:` (HTTP) · `invoke:` (MCP) · `agent:` (loop)

**Data Flow:**
```yaml
- id: source
  exec: "echo hello"
- id: consumer
  depends_on: [source]
  with:
    data: $source
  exec: "echo {{with.data}}"
```

**Transforms:** `{{with.data | upper | trim | length}}`

**For full syntax reference**, run `nika --help` or visit https://github.com/supernovae-st/nika
"#;

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    /// AGENTS.md must reference current schema version.
    #[test]
    fn agents_md_references_current_schema() {
        assert!(
            AGENTS_MD_CONTENT.contains("@0.12"),
            "AGENTS_MD missing @0.12"
        );
    }

    /// AGENTS.md must mention all 5 verbs.
    #[test]
    fn agents_md_mentions_all_verbs() {
        for verb in &["infer:", "exec:", "fetch:", "invoke:", "agent:"] {
            assert!(
                AGENTS_MD_CONTENT.contains(verb),
                "AGENTS_MD missing verb {}",
                verb
            );
        }
    }

    /// AGENTS.md must use {{with.data}} not bare {{data}}.
    #[test]
    fn agents_md_uses_with_prefix() {
        assert!(
            AGENTS_MD_CONTENT.contains("{{with.data}}"),
            "AGENTS_MD should demonstrate with. prefix"
        );
        // No bare {{data}} outside of template syntax explanations
        for line in AGENTS_MD_CONTENT.lines() {
            let trimmed = line.trim();
            if trimmed.contains("{{data}}")
                && !trimmed.starts_with('|')
                && !trimmed.contains("Wrong")
            {
                panic!("AGENTS_MD has bare {{{{data}}}} in: {}", trimmed);
            }
        }
    }

    /// AGENTS.md must mention nika --help for full reference.
    #[test]
    fn agents_md_mentions_help() {
        assert!(
            AGENTS_MD_CONTENT.contains("nika --help"),
            "AGENTS_MD should point users to nika --help for full syntax"
        );
    }

    /// Project-local constants must be non-empty.
    #[test]
    fn constants_are_non_empty() {
        assert!(!VSCODE_EXTENSIONS.is_empty());
        assert!(!VSCODE_SETTINGS.is_empty());
        assert!(!GIT_HOOK.is_empty());
        assert!(!AGENTS_MD_CONTENT.is_empty());
    }
}
