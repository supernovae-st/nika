//! AI coding assistant file generation for `nika init`
//!
//! Generates per-tool AI rules, AGENTS.md, and git hooks
//! during project initialization.

use colored::Colorize;
use std::fs;
use std::path::Path;

/// Generate all AI integration files for the project
pub fn generate_ai_files(project_dir: &Path) -> Result<(), nika_engine::NikaError> {
    println!("\n  {}", "AI Integration".bold().underline());

    let mut count = 0;

    // AGENTS.md (universal, 20+ tools)
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
                    println!("  {} CLAUDE.md → AGENTS.md (symlink)", "✓".green());
                    count += 1;
                }
                Err(e) => {
                    println!(
                        "  {} CLAUDE.md symlink failed: {}",
                        "⚠".yellow(),
                        e
                    );
                }
            }
        }
    }

    // Cursor rule
    count += write_if_absent_with_dir(
        project_dir,
        ".cursor/rules/nika-workflows.mdc",
        CURSOR_RULE,
        "Cursor rule",
    );

    // Copilot instructions
    count += write_if_absent_with_dir(
        project_dir,
        ".github/copilot/nika.instructions.md",
        COPILOT_INSTRUCTIONS,
        "Copilot instructions",
    );

    // Windsurf rule
    count += write_if_absent_with_dir(
        project_dir,
        ".windsurf/rules/nika.md",
        WINDSURF_RULE,
        "Windsurf rule",
    );

    // Roo Code rule
    count += write_if_absent_with_dir(project_dir, ".roo/rules/nika.md", ROO_RULE, "Roo Code rule");

    // Roo Code .roomodes
    count += write_if_absent(&project_dir.join(".roomodes"), ROOMODES, ".roomodes (Roo Code mode)");

    // VS Code extensions.json
    count += write_if_absent_with_dir(
        project_dir,
        ".vscode/extensions.json",
        VSCODE_EXTENSIONS,
        "VS Code recommendations",
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
                    println!(
                        "  {} Git co-author hook failed: {}",
                        "⚠".yellow(),
                        e
                    );
                }
            }
        }
    }

    println!(
        "\n  {} {} AI integration file(s) created",
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

const CURSOR_RULE: &str = r#"---
description: "Nika workflow syntax for .nika.yaml files"
globs: "**/*.nika.yaml"
alwaysApply: false
---

# Nika Workflow Rules

Schema: `nika/workflow@0.12` | Extension: `.nika.yaml`

## 5 Verbs

| Verb | Purpose |
|------|---------|
| `infer:` | LLM generation (prompt, system, temperature, max_tokens) |
| `exec:` | Shell command (command, shell, cwd, env) |
| `fetch:` | HTTP request (url, method, headers, extract) |
| `invoke:` | MCP tool call (tool, mcp, params) |
| `agent:` | Multi-turn loop (prompt, tools, max_turns) |

## Key Syntax

- Bindings: `with: { alias: $task_id }` then `{{with.alias}}`
- Dependencies: `depends_on: [task_id]`
- Parallel: `for_each: [items]` + `as: var` + `concurrency: N`
- Timeout: in seconds (not ms)
- Structured: `structured: { schema: { type: object, ... } }`

## Validation

```bash
nika check workflow.nika.yaml
nika run workflow.nika.yaml
```
"#;

const COPILOT_INSTRUCTIONS: &str = r#"---
applyTo: "**/*.nika.yaml"
---

# Nika Workflow Conventions

- Extension: `.nika.yaml` | Schema: `nika/workflow@0.12`
- 5 verbs: infer (LLM), exec (shell), fetch (HTTP), invoke (MCP), agent (loop)
- Bindings: `with: { alias: $task_id }` → `{{with.alias}}`
- Dependencies: `depends_on: [task_id]`
- Timeout values are in seconds
- Validate: `nika check <file>`
"#;

const WINDSURF_RULE: &str = r#"---
trigger: glob
globs: "**/*.nika.yaml"
description: "Nika YAML workflow engine rules"
---

# Nika Workflow Rules

Schema: `nika/workflow@0.12` | Extension: `.nika.yaml`

## 5 Verbs
- `infer:` — LLM generation
- `exec:` — Shell command
- `fetch:` — HTTP request (9 extract modes)
- `invoke:` — MCP tool call (26 builtin nika:* tools)
- `agent:` — Multi-turn autonomous loop

## Syntax
- Bindings: `with: { alias: $task_id }` → `{{with.alias}}`
- DAG: `depends_on: [task_id]`
- Parallel: `for_each: [items]` + `concurrency: N`
- Validate: `nika check <file>`
"#;

const ROO_RULE: &str = r#"---
description: "Nika YAML workflow engine syntax and patterns"
globs: ["*.nika.yaml"]
---

# Nika Workflow Rules

Schema: `nika/workflow@0.12` | Extension: `.nika.yaml`

## 5 Verbs
- `infer:` — LLM generation
- `exec:` — Shell command
- `fetch:` — HTTP request
- `invoke:` — MCP tool call
- `agent:` — Multi-turn loop

## Key Rules
- Bindings: `with: { alias: $task_id }` → `{{with.alias}}`
- DAG ordering: `depends_on: [task_id]`
- Timeout: in seconds
- Zero Cypher: use invoke: for NovaNet, never raw Cypher
- Validate: `nika check <file>`
"#;

const VSCODE_EXTENSIONS: &str = r#"{
  "recommendations": [
    "supernovae-studio.nika-lang",
    "redhat.vscode-yaml"
  ]
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

const AGENTS_MD_CONTENT: &str = r#"# Nika

Semantic YAML workflow engine for AI tasks. Schema `nika/workflow@0.12` | [QR Code AI](https://qrcode-ai.com)

## 5 Verbs

| Verb | Purpose |
|------|---------|
| `infer:` | LLM generation |
| `exec:` | Shell command |
| `fetch:` | HTTP request |
| `invoke:` | MCP tool call |
| `agent:` | Multi-turn loop |

## Workflow Syntax

`with:` for bindings, `{{with.alias}}` for templates, `.nika.yaml` extension.

## Integration with NovaNet

Nika connects to NovaNet via MCP only (Zero Cypher rule). Use `invoke:` verb.

## TUI Views

`1/s` Studio | `2/c` Command | `3/x` Control

## Commands

```bash
nika check workflow.nika.yaml    # Validate
nika run workflow.nika.yaml      # Execute
nika ui                          # TUI
nika provider list               # API key status
nika init                        # Interactive project setup (wizard)
nika init --course               # Generate 12-level learning course (44 exercises)
nika init --minimal              # Minimal scaffold (5 workflows, 1 per verb)
nika course status               # Show constellation progress map
nika course next                 # Open next exercise
nika course check [level]        # Validate exercises
nika course hint [exercise]      # Progressive hints (3 tiers)
nika course run <exercise>       # Run a course exercise
nika course info [level]         # Show course/level details
nika course reset <level>        # Reset a level
nika course watch                # Auto-check on file save
nika showcase list               # Browse 200+ showcase workflows
nika showcase extract <name>     # Extract a showcase to current dir
```
"#;
