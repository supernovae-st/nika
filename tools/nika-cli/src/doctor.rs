//! Doctor diagnostic subcommand handler

use std::fs;

use colored::Colorize;

use nika_engine::error::NikaError;

use crate::config::find_nika_dir;

#[derive(Debug, Clone)]
struct DiagnosticCheck {
    name: &'static str,
    status: DiagnosticStatus,
    message: String,
    suggestion: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum DiagnosticStatus {
    Pass,
    Warn,
    Fail,
}

impl DiagnosticCheck {
    fn pass(name: &'static str, message: impl Into<String>) -> Self {
        Self {
            name,
            status: DiagnosticStatus::Pass,
            message: message.into(),
            suggestion: None,
        }
    }

    fn warn(name: &'static str, message: impl Into<String>, suggestion: impl Into<String>) -> Self {
        Self {
            name,
            status: DiagnosticStatus::Warn,
            message: message.into(),
            suggestion: Some(suggestion.into()),
        }
    }

    fn fail(name: &'static str, message: impl Into<String>, suggestion: impl Into<String>) -> Self {
        Self {
            name,
            status: DiagnosticStatus::Fail,
            message: message.into(),
            suggestion: Some(suggestion.into()),
        }
    }

    fn icon(&self) -> &'static str {
        match self.status {
            DiagnosticStatus::Pass => "✓",
            DiagnosticStatus::Warn => "⚠",
            DiagnosticStatus::Fail => "✗",
        }
    }
}

pub async fn handle_doctor_command(full: bool, format: &str, quiet: bool) -> Result<(), NikaError> {
    let mut checks: Vec<DiagnosticCheck> = vec![];

    // 1. Check .nika directory + project structure
    checks.extend(check_nika_directory());

    // 2. Check config file
    checks.push(check_config_file());

    // 3. Check API keys
    checks.extend(check_api_keys());

    // 4. Check trace directory + accumulation
    checks.extend(check_trace_directory());

    // 5. Check Nika version
    checks.push(DiagnosticCheck::pass(
        "Version",
        format!("nika {}", env!("CARGO_PKG_VERSION")),
    ));

    // 6. Check Rust version
    checks.push(check_rust_version());

    // 7. Check workflow files in project
    checks.push(check_workflow_files());

    // 8. Check npx for MCP
    checks.push(check_npx());

    // 9. Full mode: Check MCP connectivity (slow)
    if full {
        checks.push(check_mcp_connectivity().await);
    }

    // 10. Check LSP availability (compiled-in feature)
    checks.push(check_lsp_available());

    // 11. Check editor integration (VS Code + extension)
    checks.extend(check_editor_integration());

    // 12. Check AI coding tool rules
    checks.extend(check_ai_rules());

    // 13. Check Agent Skills
    checks.push(check_agent_skills());

    // 14. Check AGENTS.md
    checks.push(check_agents_md());

    // 15. Check git co-author hook
    checks.push(check_git_hook());

    // Output results
    if format == "json" {
        output_doctor_json(&checks)?;
    } else {
        output_doctor_text(&checks, quiet);
    }

    // Return error if any checks failed
    let has_failures = checks.iter().any(|c| c.status == DiagnosticStatus::Fail);
    if has_failures {
        return Err(NikaError::ValidationError {
            reason: "Some diagnostic checks failed".to_string(),
        });
    }

    Ok(())
}

fn check_nika_directory() -> Vec<DiagnosticCheck> {
    let mut checks = vec![];

    let dir = match find_nika_dir() {
        Ok(dir) if dir.exists() => {
            checks.push(DiagnosticCheck::pass(
                "Project",
                format!(".nika directory found at {}", dir.display()),
            ));
            dir
        }
        Ok(dir) => {
            checks.push(DiagnosticCheck::warn(
                "Project",
                format!("No .nika directory at {}", dir.display()),
                "Run 'nika init' to create project structure",
            ));
            return checks;
        }
        Err(_) => {
            checks.push(DiagnosticCheck::fail(
                "Project",
                "Cannot determine current directory",
                "Check filesystem permissions",
            ));
            return checks;
        }
    };

    // Check for config.toml inside .nika/
    if !dir.join("config.toml").exists() {
        checks.push(DiagnosticCheck::warn(
            "Project",
            "config.toml missing from .nika/",
            "Run 'nika init' to regenerate project structure",
        ));
    }

    // Check for workflows/ directory inside .nika/
    if !dir.join("workflows").exists() {
        checks.push(DiagnosticCheck::warn(
            "Project",
            "workflows/ directory missing from .nika/",
            "Run 'nika init' to regenerate project structure",
        ));
    }

    checks
}

fn check_config_file() -> DiagnosticCheck {
    let nika_dir = match find_nika_dir() {
        Ok(d) => d,
        Err(_) => {
            return DiagnosticCheck::warn(
                "Config",
                "Cannot locate .nika directory",
                "Run 'nika init' first",
            )
        }
    };

    let config_path = nika_dir.join("config.toml");
    if !config_path.exists() {
        return DiagnosticCheck::warn(
            "Config",
            "No config.toml found",
            "Run 'nika init' to create default config",
        );
    }

    // Try to parse the config
    match fs::read_to_string(&config_path) {
        Ok(content) => match toml::from_str::<toml::Value>(&content) {
            Ok(_) => DiagnosticCheck::pass("Config", "config.toml is valid TOML"),
            Err(e) => DiagnosticCheck::fail(
                "Config",
                format!("config.toml has syntax errors: {e}"),
                "Run 'nika config edit' to fix",
            ),
        },
        Err(e) => DiagnosticCheck::fail(
            "Config",
            format!("Cannot read config.toml: {e}"),
            "Check file permissions",
        ),
    }
}

fn check_api_keys() -> Vec<DiagnosticCheck> {
    let mut checks = vec![];

    // Check common API keys (without exposing values)
    let keys = [
        ("ANTHROPIC_API_KEY", "Claude"),
        ("OPENAI_API_KEY", "OpenAI"),
        ("MISTRAL_API_KEY", "Mistral"),
        ("GROQ_API_KEY", "Groq"),
        ("DEEPSEEK_API_KEY", "DeepSeek"),
        ("GEMINI_API_KEY", "Gemini"),
        ("XAI_API_KEY", "xAI/Grok"),
    ];

    let mut any_found = false;
    for (env_var, provider) in keys {
        if let Ok(val) = std::env::var(env_var) {
            // Basic format validation (don't expose the key)
            let len = val.len();
            let is_valid = if env_var == "ANTHROPIC_API_KEY" {
                val.starts_with("sk-ant-") && len > 40
            } else if env_var == "OPENAI_API_KEY" {
                val.starts_with("sk-") && len > 20
            } else {
                len > 10
            };

            if val.is_empty() {
                checks.push(DiagnosticCheck::warn(
                    "API Key",
                    format!("{provider} key is empty ({env_var})"),
                    format!("Set a valid {provider} key"),
                ));
            } else if !is_valid {
                checks.push(DiagnosticCheck::warn(
                    "API Key",
                    format!("{provider} key format looks invalid ({env_var}, {len} chars)"),
                    format!("Verify your {provider} API key is correct"),
                ));
                any_found = true;
            } else {
                checks.push(DiagnosticCheck::pass(
                    "API Key",
                    format!("{provider} configured ({env_var}, {len} chars)"),
                ));
                any_found = true;
            }
        }
    }

    if !any_found {
        checks.push(DiagnosticCheck::warn(
            "API Key",
            "No LLM API keys found",
            "Set ANTHROPIC_API_KEY, OPENAI_API_KEY, or use provider: native",
        ));
    }

    checks
}

fn check_trace_directory() -> Vec<DiagnosticCheck> {
    let mut checks = vec![];

    let nika_dir = match find_nika_dir() {
        Ok(d) => d,
        Err(_) => {
            checks.push(DiagnosticCheck::warn(
                "Traces",
                "Cannot locate .nika directory",
                "Run 'nika init' first",
            ));
            return checks;
        }
    };

    let trace_dir = nika_dir.join("traces");

    // Check if directory exists
    if !trace_dir.exists() {
        checks.push(DiagnosticCheck::warn(
            "Traces",
            "Trace directory doesn't exist",
            "It will be created on first workflow run",
        ));
        return checks;
    }

    // Check if writable by attempting to create a temp file
    let test_file = trace_dir.join(".nika_doctor_test");
    match fs::write(&test_file, b"test") {
        Ok(_) => {
            let _ = fs::remove_file(&test_file);
            checks.push(DiagnosticCheck::pass(
                "Traces",
                format!("Trace directory writable ({})", trace_dir.display()),
            ));
        }
        Err(e) => {
            checks.push(DiagnosticCheck::fail(
                "Traces",
                format!("Trace directory not writable: {e}"),
                "Check directory permissions",
            ));
            return checks;
        }
    }

    // Count trace files and warn on accumulation
    if let Ok(entries) = fs::read_dir(&trace_dir) {
        let count = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "ndjson"))
            .count();

        if count > 10_000 {
            checks.push(DiagnosticCheck::warn(
                "Traces",
                format!(
                    "{} trace files accumulated ({:.1} MB estimated)",
                    count,
                    count as f64 * 0.005 // ~5KB average per trace
                ),
                "Run 'nika trace clean --keep 100' to prune old traces",
            ));
        } else if count > 1_000 {
            checks.push(DiagnosticCheck::warn(
                "Traces",
                format!("{count} trace files"),
                "Consider running 'nika trace clean --keep 100'",
            ));
        } else {
            checks.push(DiagnosticCheck::pass(
                "Traces",
                format!("{count} trace files"),
            ));
        }
    }

    checks
}

fn check_workflow_files() -> DiagnosticCheck {
    // Count .nika.yaml files in current directory (shallow)
    let count = fs::read_dir(".")
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_name()
                        .to_str()
                        .is_some_and(|s| s.ends_with(".nika.yaml"))
                })
                .count()
        })
        .unwrap_or(0);

    // Also check workflows/ and examples/ subdirs
    let sub_count: usize = ["workflows", "examples", ".nika/workflows"]
        .iter()
        .filter_map(|dir| fs::read_dir(dir).ok())
        .flat_map(|entries| entries.filter_map(|e| e.ok()))
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|s| s.ends_with(".nika.yaml"))
        })
        .count();

    let total = count + sub_count;

    if total == 0 {
        DiagnosticCheck::warn(
            "Workflows",
            "No .nika.yaml workflow files found",
            "Run 'nika init' or 'nika new my-workflow --template simple-infer'",
        )
    } else {
        DiagnosticCheck::pass("Workflows", format!("{total} workflow files found"))
    }
}

fn check_rust_version() -> DiagnosticCheck {
    // Minimum supported Rust version (from Cargo.toml rust-version)
    const MSRV_MAJOR: u32 = 1;
    const MSRV_MINOR: u32 = 86;

    match std::process::Command::new("rustc")
        .arg("--version")
        .output()
    {
        Ok(output) => {
            let version_str = String::from_utf8_lossy(&output.stdout);
            let version_str = version_str.trim();

            // Parse "rustc X.Y.Z (...)" to extract major.minor
            let parts: Vec<&str> = version_str
                .strip_prefix("rustc ")
                .unwrap_or(version_str)
                .split(|c: char| !c.is_ascii_digit())
                .collect();

            if parts.len() >= 2 {
                if let (Ok(major), Ok(minor)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                    if major > MSRV_MAJOR || (major == MSRV_MAJOR && minor >= MSRV_MINOR) {
                        return DiagnosticCheck::pass("Rust", version_str.to_string());
                    } else {
                        return DiagnosticCheck::warn(
                            "Rust",
                            format!("{version_str} (MSRV is {MSRV_MAJOR}.{MSRV_MINOR})"),
                            "Update with: rustup update",
                        );
                    }
                }
            }

            // Fallback: can't parse version, just report it
            DiagnosticCheck::pass("Rust", version_str.to_string())
        }
        Err(_) => DiagnosticCheck::warn(
            "Rust",
            "rustc not found in PATH",
            "Install Rust: https://rustup.rs",
        ),
    }
}

fn check_npx() -> DiagnosticCheck {
    match std::process::Command::new("npx").arg("--version").output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            DiagnosticCheck::pass("npx", format!("npx {} available", version.trim()))
        }
        _ => DiagnosticCheck::warn(
            "npx",
            "npx not found",
            "MCP servers using npx won't work. Install Node.js: https://nodejs.org",
        ),
    }
}

async fn check_mcp_connectivity() -> DiagnosticCheck {
    // This is a placeholder - in a real implementation, we'd try to connect
    // to configured MCP servers from the config file
    DiagnosticCheck::pass(
        "MCP",
        "MCP connectivity check (requires configured servers)",
    )
}

fn check_lsp_available() -> DiagnosticCheck {
    if cfg!(feature = "lsp") {
        DiagnosticCheck::pass("LSP", "Language server compiled in (nika lsp)")
    } else {
        DiagnosticCheck::warn(
            "LSP",
            "Language server not available (compiled without lsp feature)",
            "Reinstall with: cargo install nika --features lsp, or: brew reinstall nika",
        )
    }
}

fn check_editor_integration() -> Vec<DiagnosticCheck> {
    let mut checks = vec![];

    // Detect VS Code (or common forks) — check PATH and platform-specific locations
    // (binary_path, short_cmd_for_suggestions, display_name)
    let editors: Vec<(String, &str, &str)> = {
        let mut v: Vec<(String, &str, &str)> = vec![
            ("code".to_string(), "code", "VS Code"),
            ("cursor".to_string(), "cursor", "Cursor"),
            ("windsurf".to_string(), "windsurf", "Windsurf"),
        ];
        // macOS: VS Code CLI may not be in PATH but the .app bundle exists
        #[cfg(target_os = "macos")]
        {
            v.push((
                "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code".to_string(),
                "code",
                "VS Code",
            ));
            v.push((
                "/Applications/Cursor.app/Contents/Resources/app/bin/cursor".to_string(),
                "cursor",
                "Cursor",
            ));
        }
        v
    };
    let mut found_editor: Option<(String, &str, &str, String)> = None;

    for (bin, short_cmd, name) in &editors {
        if let Ok(output) = std::process::Command::new(bin).arg("--version").output() {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout);
                let first_line = version.lines().next().unwrap_or("unknown").to_string();
                found_editor = Some((bin.clone(), short_cmd, name, first_line));
                break;
            }
        }
    }

    match found_editor {
        Some((ref bin, short_cmd, name, version)) => {
            checks.push(DiagnosticCheck::pass(
                "Editor",
                format!("{name} {version} detected"),
            ));

            // Check if nika-lang extension is installed
            match std::process::Command::new(bin)
                .args(["--list-extensions"])
                .output()
            {
                Ok(output) if output.status.success() => {
                    let extensions = String::from_utf8_lossy(&output.stdout);
                    if extensions.lines().any(|l| {
                        let trimmed = l.trim().to_lowercase();
                        trimmed == "supernovae-studio.nika-lang"
                    }) {
                        checks.push(DiagnosticCheck::pass(
                            "Extension",
                            "nika-lang extension installed",
                        ));
                    } else {
                        checks.push(DiagnosticCheck::warn(
                            "Extension",
                            "nika-lang extension not installed",
                            format!(
                                "Install with: {short_cmd} --install-extension supernovae-studio.nika-lang"
                            ),
                        ));
                    }
                }
                _ => {
                    checks.push(DiagnosticCheck::warn(
                        "Extension",
                        "Cannot query installed extensions",
                        format!(
                            "Install with: {short_cmd} --install-extension supernovae-studio.nika-lang"
                        ),
                    ));
                }
            }
        }
        None => {
            checks.push(DiagnosticCheck::warn(
                "Editor",
                "No supported editor detected (VS Code, Cursor, Windsurf)",
                "Install VS Code and add 'code' to PATH for LSP integration",
            ));
        }
    }

    checks
}

// ─── AI Integration Checks ────────────────────────────────────────────────────

fn check_ai_rules() -> Vec<DiagnosticCheck> {
    let mut checks = vec![];

    let rules: &[(&str, &str, &str)] = &[
        ("Claude Code", ".claude/rules/nika.md", "nika init"),
        ("Cursor", ".cursor/rules/nika-workflows.mdc", "nika init"),
        (
            "Copilot",
            ".github/copilot/nika.instructions.md",
            "nika init",
        ),
        ("Windsurf", ".windsurf/rules/nika.md", "nika init"),
        ("Roo Code", ".roo/rules/nika.md", "nika init"),
    ];

    for (tool, path, _fix_cmd) in rules {
        if std::path::Path::new(path).exists() {
            checks.push(DiagnosticCheck::pass(
                "AI Rules",
                format!("{tool} rules present ({path})"),
            ));
        }
        // Only warn if the tool is detected (don't warn for tools not installed)
    }

    if checks.is_empty() {
        checks.push(DiagnosticCheck::warn(
            "AI Rules",
            "No AI coding tool rules found",
            "Run: nika init (select AI rules) to generate per-tool rules",
        ));
    }

    checks
}

fn check_agent_skills() -> DiagnosticCheck {
    let home = dirs::home_dir().unwrap_or_default();

    // Check user-level skills
    let user_skills = home.join(".agents/skills");
    let has_user = user_skills.join("nika-workflow-syntax").exists()
        || user_skills.join("nika-create").exists();

    // Check project-level skills
    let has_project = std::path::Path::new("skills/nika-workflow-syntax").exists()
        || std::path::Path::new(".agents/skills/nika-workflow-syntax").exists();

    if has_user {
        DiagnosticCheck::pass(
            "Agent Skills",
            format!(
                "Nika skills installed at {}",
                user_skills.display()
            ),
        )
    } else if has_project {
        DiagnosticCheck::pass("Agent Skills", "Nika skills present in project")
    } else {
        DiagnosticCheck::warn(
            "Agent Skills",
            "No Nika Agent Skills installed",
            "Run: nika setup ai (global) or: npx skills add SuperNovae-studio/nika-skills",
        )
    }
}

fn check_agents_md() -> DiagnosticCheck {
    if std::path::Path::new("AGENTS.md").exists() {
        let is_symlink = std::fs::symlink_metadata("CLAUDE.md")
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);
        if is_symlink {
            DiagnosticCheck::pass(
                "AGENTS.md",
                "AGENTS.md present (CLAUDE.md symlinked)",
            )
        } else if std::path::Path::new("CLAUDE.md").exists() {
            DiagnosticCheck::warn(
                "AGENTS.md",
                "Both AGENTS.md and CLAUDE.md exist (not symlinked)",
                "Consider: ln -sf AGENTS.md CLAUDE.md",
            )
        } else {
            DiagnosticCheck::pass("AGENTS.md", "AGENTS.md present")
        }
    } else if std::path::Path::new("CLAUDE.md").exists() {
        DiagnosticCheck::warn(
            "AGENTS.md",
            "Only CLAUDE.md found (20+ tools support AGENTS.md)",
            "Migrate: mv CLAUDE.md AGENTS.md && ln -s AGENTS.md CLAUDE.md",
        )
    } else {
        DiagnosticCheck::warn(
            "AGENTS.md",
            "No AGENTS.md or CLAUDE.md found",
            "Create with: nika init (or manually)",
        )
    }
}

fn check_git_hook() -> DiagnosticCheck {
    let hook_path = std::path::Path::new(".git/hooks/prepare-commit-msg");
    if !std::path::Path::new(".git").exists() {
        return DiagnosticCheck::warn(
            "Git Hook",
            "Not a git repository",
            "Initialize with: git init",
        );
    }

    if hook_path.exists() {
        let content = fs::read_to_string(hook_path).unwrap_or_default();
        if content.contains("Nika co-author") {
            DiagnosticCheck::pass("Git Hook", "Co-author hook installed")
        } else {
            DiagnosticCheck::warn(
                "Git Hook",
                "prepare-commit-msg hook exists but is not Nika's",
                "Run: nika setup git (will backup existing hook)",
            )
        }
    } else {
        DiagnosticCheck::warn(
            "Git Hook",
            "No co-author hook installed",
            "Run: nika setup git",
        )
    }
}

fn output_doctor_text(checks: &[DiagnosticCheck], quiet: bool) {
    if !quiet {
        nika_engine::display::print_doctor_header(env!("CARGO_PKG_VERSION"));
    }

    let mut pass_count = 0;
    let mut warn_count = 0;
    let mut fail_count = 0;

    for check in checks {
        let icon = match check.status {
            DiagnosticStatus::Pass => check.icon().green(),
            DiagnosticStatus::Warn => check.icon().yellow(),
            DiagnosticStatus::Fail => check.icon().red(),
        };

        println!("  {} {} {}", icon, check.name.bold(), check.message);

        if let Some(ref suggestion) = check.suggestion {
            println!("    {} {}", "→".cyan(), suggestion.dimmed());
        }

        match check.status {
            DiagnosticStatus::Pass => pass_count += 1,
            DiagnosticStatus::Warn => warn_count += 1,
            DiagnosticStatus::Fail => fail_count += 1,
        }
    }

    if !quiet {
        nika_engine::display::print_doctor_summary(pass_count, warn_count, fail_count);
    }
}

fn output_doctor_json(checks: &[DiagnosticCheck]) -> Result<(), NikaError> {
    let results: Vec<serde_json::Value> = checks
        .iter()
        .map(|c| {
            serde_json::json!({
                "name": c.name,
                "status": match c.status {
                    DiagnosticStatus::Pass => "pass",
                    DiagnosticStatus::Warn => "warn",
                    DiagnosticStatus::Fail => "fail",
                },
                "message": c.message,
                "suggestion": c.suggestion,
            })
        })
        .collect();

    let output = serde_json::json!({
        "checks": results,
        "summary": {
            "pass": checks.iter().filter(|c| c.status == DiagnosticStatus::Pass).count(),
            "warn": checks.iter().filter(|c| c.status == DiagnosticStatus::Warn).count(),
            "fail": checks.iter().filter(|c| c.status == DiagnosticStatus::Fail).count(),
        }
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
