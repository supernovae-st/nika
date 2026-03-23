//! Machine-level auto-setup for Nika.
//!
//! Detects installed editors/AI tools and configures them automatically.
//! Tracks setup state via `~/.nika/machine.toml` marker file.
//! Called by `nika init` before the project wizard (Phase 1).

use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use colored::Colorize;

/// Result of a single setup action.
#[derive(Debug)]
pub struct SetupResult {
    pub name: String,
    pub success: bool,
    #[allow(dead_code)]
    pub message: String,
}

/// Distinguishes "never set up" from "version mismatch" from "ready".
#[derive(Debug, PartialEq)]
pub enum MachineStatus {
    /// Never set up (no marker file)
    NeverSetup,
    /// Set up but version doesn't match (user upgraded)
    NeedsUpdate,
    /// Fully set up and current
    Ready,
}

/// Return the machine setup status: NeverSetup, NeedsUpdate, or Ready.
pub fn machine_setup_status() -> MachineStatus {
    let marker = machine_toml_path();
    let content = match std::fs::read_to_string(&marker) {
        Ok(c) => c,
        Err(_) => return MachineStatus::NeverSetup,
    };
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("version") {
            let rest = rest.trim().strip_prefix('=').unwrap_or("").trim();
            let version = rest.trim_matches('"');
            if version == env!("CARGO_PKG_VERSION") {
                return MachineStatus::Ready;
            } else {
                return MachineStatus::NeedsUpdate;
            }
        }
    }
    MachineStatus::NeedsUpdate
}

/// Check if machine setup has been done (marker file exists + version current).
pub fn is_machine_setup() -> bool {
    machine_setup_status() == MachineStatus::Ready
}

/// Path to the machine marker file.
fn machine_toml_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".nika")
        .join("machine.toml")
}

/// Run the full machine auto-setup (Phase 1).
///
/// Detects editors and AI tools, installs extensions/rules/completions.
/// Prints progress as it goes. Writes marker file on success.
/// This is SILENT by design — no questions asked, just detect and install.
pub fn run_machine_setup() -> Vec<SetupResult> {
    let mut results = Vec::new();

    println!();
    println!("  {}", "Machine setup".bold().underline());

    // 1. Editors: detect + install extension
    results.extend(setup_editors());

    // 2. AI tools: detect + install rules
    results.extend(setup_ai_rules());

    // 3. Shell completions
    results.push(setup_completions());

    // Write marker file
    write_marker(&results);

    // Summary line
    let ok = results.iter().filter(|r| r.success).count();
    let total = results.len();
    println!();
    println!(
        "  {} Machine ready ({}/{} configured)",
        "\u{2713}".green(),
        ok,
        total
    );

    results
}

fn setup_editors() -> Vec<SetupResult> {
    let mut results = Vec::new();

    let editors: &[(&str, &str, &str)] = &[
        ("VS Code", "code", "supernovae-studio.nika-lang"),
        ("Cursor", "cursor", "supernovae-studio.nika-lang"),
        ("Windsurf", "windsurf", "supernovae-studio.nika-lang"),
    ];

    for (name, binary, ext_id) in editors {
        // Check binary in PATH or macOS .app
        let has_binary = which::which(binary).is_ok() || check_macos_app(name);
        if !has_binary {
            continue;
        }

        // Check if extension already installed
        let has_ext = Command::new(binary)
            .args(["--list-extensions"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|list| list.lines().any(|l| l.eq_ignore_ascii_case(ext_id)))
            .unwrap_or(false);

        if has_ext {
            println!("  {} {} + nika-lang extension", "\u{2713}".green(), name);
            results.push(SetupResult {
                name: name.to_string(),
                success: true,
                message: "already installed".into(),
            });
            continue;
        }

        // Install extension
        print!("  {} {} — installing nika-lang...", "\u{25c7}".cyan(), name);
        let install = Command::new(binary)
            .args(["--install-extension", ext_id])
            .output();

        match install {
            Ok(output) if output.status.success() => {
                println!(
                    "\r  {} {} — nika-lang installed       ",
                    "\u{2713}".green(),
                    name
                );
                results.push(SetupResult {
                    name: name.to_string(),
                    success: true,
                    message: "installed".into(),
                });
            }
            _ => {
                println!(
                    "\r  {} {} — install failed          ",
                    "\u{2717}".red(),
                    name
                );
                results.push(SetupResult {
                    name: name.to_string(),
                    success: false,
                    message: format!("run: {} --install-extension {}", binary, ext_id),
                });
            }
        }
    }

    if results.is_empty() {
        println!(
            "  {} No editors detected (VS Code, Cursor, Windsurf)",
            "\u{25cb}".dimmed()
        );
    }

    results
}

#[cfg(target_os = "macos")]
fn check_macos_app(name: &str) -> bool {
    let app_names: &[&str] = match name {
        "VS Code" => &["Visual Studio Code"],
        "Cursor" => &["Cursor"],
        "Windsurf" => &["Windsurf"],
        _ => return false,
    };
    app_names.iter().any(|app| {
        std::path::Path::new(&format!("/Applications/{}.app", app)).exists()
            || dirs::home_dir()
                .map(|h| h.join(format!("Applications/{}.app", app)).exists())
                .unwrap_or(false)
    })
}

#[cfg(not(target_os = "macos"))]
fn check_macos_app(_name: &str) -> bool {
    false
}

/// Comprehensive Nika rules for Claude Code (~/.claude/rules/nika.md).
const CLAUDE_RULES_CONTENT: &str = r#"# Nika Workflow Rules

Schema: `nika/workflow@0.12` | Extension: `.nika.yaml`

## 5 Verbs

| Verb | Purpose | Example |
|------|---------|---------|
| `infer:` | LLM generation | `infer: "Summarize this"` |
| `exec:` | Shell command | `exec: "echo hello"` |
| `fetch:` | HTTP request | `fetch: "https://api.example.com"` |
| `invoke:` | MCP tool call | `invoke:` block with `tool:` + `params:` |
| `agent:` | Multi-turn loop | `agent:` block with `tools:` + `max_turns:` |

## Complete Workflow Example

```yaml
schema: "@0.12"
workflow: research-and-summarize
description: "Research a topic and create a summary"
provider: anthropic
model: claude-sonnet-4-20250514

inputs:
  topic: "AI workflow engines"

tasks:
  - id: research
    infer:
      prompt: |
        Research the following topic thoroughly: {{inputs.topic}}
        Provide key findings, trends, and notable projects.
      temperature: 0.7

  - id: summarize
    depends_on: [research]
    with:
      data: $research
    infer:
      prompt: |
        Create a concise executive summary from this research:
        {{with.data}}
      max_tokens: 500
```

## Workflow Header Fields

```yaml
schema: "@0.12"               # Required. Always "@0.12"
workflow: my-workflow          # Optional. Defaults to filename
description: "What it does"   # Optional
provider: anthropic            # Default LLM provider for all tasks
model: claude-sonnet-4-20250514  # Default model for all tasks

inputs:                        # Workflow parameters
  topic: "default value"

context:                       # File context bindings
  files:
    readme: ./README.md

skills:                        # Prompt augmentation files
  writing: ./skills/writing.md

artifacts:                     # Persist outputs to files
  dir: ./output
  format: markdown
```

## Data Flow

- **Bindings**: `with: { alias: $task_id }` then `{{with.alias}}`
- **Path access**: `with: { temp: $weather.data.temperature }`
- **Defaults**: `with: { val: $task.path ?? "fallback" }`
- **Env vars**: `with: { key: $env.API_KEY }`
- **Transforms**: `{{with.data | uppercase | trim}}`
- **Dependencies**: `depends_on: [task_id]` for ordering without data
- **Inputs**: `{{inputs.param}}` for workflow parameters
- **Context files**: `{{context.readme}}` for loaded file content

## Pipe Transforms (31 available)

**String**: `upper`, `lower`, `trim`, `trim_start`, `trim_end`, `length`, `to_string`
**Array**: `first`, `last`, `flatten`, `reverse`, `sort`, `unique`, `compact`, `keys`, `values`
**Numeric**: `to_number`, `round`, `abs`, `ceil`, `floor`
**Type**: `to_bool`, `to_json`, `parse_json`, `type_of`
**Parametric**: `join(", ")`, `split(",")`, `default("fallback")`
**System**: `shell` (execute as shell command)

Usage: `{{with.items | flatten | unique | join(", ")}}`

## Providers (7 Cloud + 1 Local)

| Provider | Env Var | Models |
|----------|---------|--------|
| `anthropic` | `ANTHROPIC_API_KEY` | claude-opus-4-20250514, claude-sonnet-4-20250514, claude-haiku-3.5 |
| `openai` | `OPENAI_API_KEY` | gpt-4o, gpt-4.1, o3, o4-mini |
| `mistral` | `MISTRAL_API_KEY` | mistral-large-latest, mistral-small-latest |
| `groq` | `GROQ_API_KEY` | llama-4-maverick, mixtral-8x7b |
| `deepseek` | `DEEPSEEK_API_KEY` | deepseek-chat, deepseek-reasoner |
| `gemini` | `GEMINI_API_KEY` | gemini-2.5-pro, gemini-2.5-flash |
| `xai` | `XAI_API_KEY` | grok-3 |
| `native` | (none) | Local GGUF via mistral.rs |

## For Each (Parallel Loop)

```yaml
- id: process
  for_each:
    items: "{{with.data}}"
    as: item
    concurrency: 3
  infer: "Process: {{with.item}}"
```

Access loop variable via `with:` prefix: `{{with.item}}` (same as all bindings).

## 24 Builtin Tools (nika:*)

**Always-on**: `nika:import`, `nika:dimensions`, `nika:thumbhash`, `nika:dominant_color`, `nika:pipeline`
**Media core**: `nika:thumbnail`, `nika:convert`, `nika:strip`, `nika:metadata`, `nika:optimize`, `nika:svg_render`
**Opt-in**: `nika:phash`, `nika:compare`, `nika:pdf_extract`, `nika:chart`, `nika:provenance`, `nika:verify`, `nika:qr_validate`, `nika:quality`, `nika:html_to_md`, `nika:css_select`, `nika:extract_metadata`, `nika:extract_links`, `nika:readability`

## Fetch Extract Modes (9)

| Mode | Description |
|------|-------------|
| `markdown` | Clean Markdown from HTML |
| `article` | Main article content (Readability) |
| `text` | Visible text, optionally filtered by `selector:` |
| `selector` | Raw HTML of matching elements (requires `selector:`) |
| `metadata` | OG, Twitter Cards, JSON-LD, SEO tags |
| `links` | Link classification (internal/external) |
| `jsonpath` | JSONPath query on JSON responses (requires `selector:` for path) |
| `feed` | RSS/Atom/JSON Feed parsing |
| `llm_txt` | AI content discovery (/llms.txt) |

## Common Mistakes

| Wrong | Right |
|-------|-------|
| `timeout: 30000` (ms) | `timeout: 30` (always seconds) |
| `use: { data: step1 }` | `with: { data: $step1 }` ($ prefix required) |
| `{{data}}` | `{{with.data}}` (always with. prefix) |
| `{{item}}` in for_each | `{{with.item}}` (loop var uses with. prefix) |
| `retry: 3` | `retry: { max_attempts: 3, delay: 2 }` |
| `.yaml` extension | `.nika.yaml` extension |
| Direct Cypher/SQL | Use `invoke:` with MCP tools |
| `shell: bash` | `shell: true` (boolean, not shell name) |
| Missing `schema:` line | Always start with `schema: "@0.12"` |
| `depends_on: task_id` | `depends_on: [task_id]` (always array) |

## Validation

```bash
nika check workflow.nika.yaml          # Validate syntax + DAG
nika check workflow.nika.yaml --strict # + test MCP connections
nika run workflow.nika.yaml            # Execute workflow
nika run workflow.nika.yaml --dry-run  # Validate without executing
nika ui                                # TUI
nika provider list                     # API key status
```
"#;

fn setup_ai_rules() -> Vec<SetupResult> {
    let home = dirs::home_dir();
    if home.is_none() {
        return vec![SetupResult {
            name: "AI Rules".into(),
            success: false,
            message: "cannot determine home directory".into(),
        }];
    }
    let home = home.unwrap();
    let mut results = Vec::new();

    // Claude Code: install user-level rules
    if which::which("claude").is_ok() || home.join(".claude").exists() {
        let rules_dir = home.join(".claude/rules");
        let rules_file = rules_dir.join("nika.md");
        if rules_file.exists() {
            println!("  {} Claude Code + Nika rules", "\u{2713}".green());
            results.push(SetupResult {
                name: "Claude Code".into(),
                success: true,
                message: "rules present".into(),
            });
        } else {
            // Create rules
            std::fs::create_dir_all(&rules_dir).ok();
            if std::fs::write(&rules_file, CLAUDE_RULES_CONTENT).is_ok() {
                println!(
                    "  {} Claude Code — Nika rules installed",
                    "\u{2713}".green()
                );
                results.push(SetupResult {
                    name: "Claude Code".into(),
                    success: true,
                    message: "installed".into(),
                });
            } else {
                results.push(SetupResult {
                    name: "Claude Code".into(),
                    success: false,
                    message: "could not write rules".into(),
                });
            }
        }
    }

    // Agent Skills: install to ~/.agents/skills/
    let skills_dir = home.join(".agents/skills");
    let has_skills = skills_dir.join("nika-workflow-syntax").exists();
    if !has_skills {
        // Try to install from embedded content
        let skill_dir = skills_dir.join("nika-workflow-syntax");
        std::fs::create_dir_all(&skill_dir).ok();
        let skill_content = concat!(
            "# Nika Workflow Syntax\n\n",
            "Refer to AGENTS.md in any Nika project ",
            "for the complete workflow syntax reference.\n"
        );
        if std::fs::write(skill_dir.join("SKILL.md"), skill_content).is_ok() {
            println!(
                "  {} Agent Skills installed [~/.agents/skills/]",
                "\u{2713}".green()
            );
            results.push(SetupResult {
                name: "Agent Skills".into(),
                success: true,
                message: "installed".into(),
            });
        }
    } else {
        println!("  {} Agent Skills [~/.agents/skills/]", "\u{2713}".green());
        results.push(SetupResult {
            name: "Agent Skills".into(),
            success: true,
            message: "present".into(),
        });
    }

    results
}

fn setup_completions() -> SetupResult {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => {
            return SetupResult {
                name: "Completions".into(),
                success: false,
                message: "cannot determine home directory".into(),
            };
        }
    };
    let shell = std::env::var("SHELL").unwrap_or_default();
    let shell_name = if shell.contains("zsh") {
        "zsh"
    } else if shell.contains("bash") {
        "bash"
    } else if shell.contains("fish") {
        "fish"
    } else {
        return SetupResult {
            name: "Completions".into(),
            success: false,
            message: "unknown shell".into(),
        };
    };

    // Check if nika completion command exists
    let output = Command::new("nika")
        .args(["completion", shell_name])
        .output();

    match output {
        Ok(o) if o.status.success() && !o.stdout.is_empty() => {
            // Write completions to appropriate location
            let target = match shell_name {
                "zsh" => {
                    let zfunc = home.join(".zfunc");
                    std::fs::create_dir_all(&zfunc).ok();
                    Some(zfunc.join("_nika"))
                }
                "bash" => {
                    let dir = home.join(".local/share/bash-completion/completions");
                    std::fs::create_dir_all(&dir).ok();
                    Some(dir.join("nika"))
                }
                "fish" => {
                    let dir = dirs::config_dir()
                        .unwrap_or_default()
                        .join("fish/completions");
                    std::fs::create_dir_all(&dir).ok();
                    Some(dir.join("nika.fish"))
                }
                _ => None,
            };

            if let Some(target) = target {
                if std::fs::write(&target, &o.stdout).is_ok() {
                    println!(
                        "  {} {} completions installed",
                        "\u{2713}".green(),
                        shell_name
                    );
                    return SetupResult {
                        name: "Completions".into(),
                        success: true,
                        message: format!("{} completions at {}", shell_name, target.display()),
                    };
                }
            }

            SetupResult {
                name: "Completions".into(),
                success: false,
                message: "could not write completions".into(),
            }
        }
        _ => {
            println!(
                "  {} {} completions (nika completion not available)",
                "\u{25cb}".dimmed(),
                shell_name
            );
            SetupResult {
                name: "Completions".into(),
                success: false,
                message: "nika completion command not available".into(),
            }
        }
    }
}

fn write_marker(results: &[SetupResult]) {
    let marker_path = machine_toml_path();
    let Some(dir) = marker_path.parent() else { return; };
    std::fs::create_dir_all(dir).ok();

    let editors: Vec<&str> = results
        .iter()
        .filter(|r| {
            r.success && !["Agent Skills", "Claude Code", "Completions"].contains(&r.name.as_str())
        })
        .map(|r| r.name.as_str())
        .collect();
    let ai_tools: Vec<&str> = results
        .iter()
        .filter(|r| r.success && ["Claude Code", "Agent Skills"].contains(&r.name.as_str()))
        .map(|r| r.name.as_str())
        .collect();

    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let timestamp = format!("{}", secs);

    let content = format!(
        "[machine]\nsetup_at = \"{}\"\nversion = \"{}\"\neditors = {:?}\nai_tools = {:?}\n",
        timestamp,
        env!("CARGO_PKG_VERSION"),
        editors,
        ai_tools,
    );

    // Don't fail init if marker write fails
    std::fs::write(&marker_path, content).ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_toml_path_is_in_home() {
        let path = machine_toml_path();
        assert!(path.to_string_lossy().contains(".nika"));
        assert!(path.to_string_lossy().ends_with("machine.toml"));
    }
}
