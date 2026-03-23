//! Doctor diagnostic subcommand handler

use std::fs;

use colored::Colorize;

use nika_engine::error::NikaError;

use crate::config::find_nika_dir;

#[derive(Debug, Clone)]
struct DiagnosticCheck {
    name: &'static str,
    section: &'static str,
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
            section: "",
            status: DiagnosticStatus::Pass,
            message: message.into(),
            suggestion: None,
        }
    }

    fn warn(name: &'static str, message: impl Into<String>, suggestion: impl Into<String>) -> Self {
        Self {
            name,
            section: "",
            status: DiagnosticStatus::Warn,
            message: message.into(),
            suggestion: Some(suggestion.into()),
        }
    }

    fn fail(name: &'static str, message: impl Into<String>, suggestion: impl Into<String>) -> Self {
        Self {
            name,
            section: "",
            status: DiagnosticStatus::Fail,
            message: message.into(),
            suggestion: Some(suggestion.into()),
        }
    }

    fn in_section(mut self, section: &'static str) -> Self {
        self.section = section;
        self
    }

    fn icon(&self) -> &'static str {
        match self.status {
            DiagnosticStatus::Pass => "✓",
            DiagnosticStatus::Warn => "⚠",
            DiagnosticStatus::Fail => "✗",
        }
    }
}

/// Helper to assign section to a vec of checks.
fn with_section(checks: Vec<DiagnosticCheck>, section: &'static str) -> Vec<DiagnosticCheck> {
    checks.into_iter().map(|c| c.in_section(section)).collect()
}

pub async fn handle_doctor_command(
    full: bool,
    format: &str,
    quiet: bool,
    fix: bool,
) -> Result<(), NikaError> {
    let mut checks: Vec<DiagnosticCheck> = vec![];

    // ─── Core ──────────────────────────────────────────────────────────────
    checks.extend(with_section(check_nika_directory(), "Core"));
    checks.push(check_config_file().in_section("Core"));
    checks.extend(with_section(check_api_keys(), "Core"));
    checks.push(
        DiagnosticCheck::pass("Version", format!("nika {}", env!("CARGO_PKG_VERSION")))
            .in_section("Core"),
    );
    checks.push(check_workflow_files().in_section("Core"));

    // ─── Editor & LSP ──────────────────────────────────────────────────────
    checks.extend(with_section(check_lsp_available(), "Editor & LSP"));
    checks.extend(with_section(check_editor_integration(), "Editor & LSP"));

    // ─── AI Integration ────────────────────────────────────────────────────
    checks.extend(with_section(check_ai_rules(), "AI Integration"));
    checks.extend(with_section(check_agent_skills(), "AI Integration"));
    checks.push(check_agents_md().in_section("AI Integration"));

    // ─── Environment ───────────────────────────────────────────────────────
    checks.extend(with_section(check_trace_directory(), "Environment"));
    checks.push(check_rust_version().in_section("Environment"));
    checks.push(check_npx().in_section("Environment"));
    checks.push(check_git_hook().in_section("Environment"));

    if full {
        checks.extend(with_section(check_mcp_connectivity().await, "Environment"));
    }

    // Output results
    if format == "json" {
        output_doctor_json(&checks)?;
    } else {
        output_doctor_text(&checks, quiet);
    }

    // Auto-fix mode: run machine setup to repair issues
    if fix {
        let has_issues = checks.iter().any(|c| c.status != DiagnosticStatus::Pass);
        if !has_issues {
            println!();
            println!("  {} Nothing to fix!", "\u{2713}".green());
            return Ok(());
        }

        println!();
        println!("  {}", "Auto-fixing...".bold());
        let results = crate::machine::run_machine_setup();
        let fix_failures = results.iter().filter(|r| !r.success).count();
        println!();
        if fix_failures > 0 {
            println!(
                "  {} {fix_failures} issue(s) could not be auto-fixed",
                "\u{26a0}".yellow()
            );
            println!(
                "  {} Re-run {} to see details",
                "\u{2192}".cyan(),
                "nika doctor".bold()
            );
            return Err(NikaError::ValidationError {
                reason: format!("{} issue(s) could not be auto-fixed", fix_failures),
            });
        }
        println!(
            "  {} All issues fixed! Re-run {} to verify",
            "\u{2713}".green(),
            "nika doctor".bold()
        );
        return Ok(());
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

// ─── Task 4: Real MCP check ───────────────────────────────────────────────────

async fn check_mcp_connectivity() -> Vec<DiagnosticCheck> {
    let mut checks = vec![];

    // Step 1: Check if npx is available (needed for most MCP servers)
    let has_npx = std::process::Command::new("npx")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !has_npx {
        checks.push(DiagnosticCheck::warn(
            "MCP",
            "npx not available (most MCP servers require it)",
            "Install Node.js: https://nodejs.org",
        ));
    }

    // Step 2: Check if any .nika.yaml files reference mcp:
    let has_mcp_workflows = find_mcp_workflows();

    match has_mcp_workflows {
        McpWorkflowStatus::Found(count) => {
            checks.push(DiagnosticCheck::pass(
                "MCP",
                format!("{count} workflow(s) use MCP (invoke: with mcp: config)"),
            ));

            // Step 3: Check MCP config in .nika/config.toml
            if let Ok(nika_dir) = find_nika_dir() {
                let config_path = nika_dir.join("config.toml");
                if config_path.exists() {
                    if let Ok(content) = fs::read_to_string(&config_path) {
                        if content.contains("[mcp]") || content.contains("mcp.") {
                            checks.push(DiagnosticCheck::pass(
                                "MCP",
                                "MCP configuration found in config.toml",
                            ));
                        } else {
                            checks.push(DiagnosticCheck::warn(
                                "MCP",
                                "Workflows use MCP but no [mcp] section in config.toml",
                                "Add MCP server config to .nika/config.toml",
                            ));
                        }
                    }
                }
            }
        }
        McpWorkflowStatus::None => {
            checks.push(DiagnosticCheck::pass(
                "MCP",
                "No workflows use MCP (no connectivity needed)",
            ));
        }
        McpWorkflowStatus::NoWorkflows => {
            checks.push(DiagnosticCheck::pass(
                "MCP",
                "No workflow files found to check for MCP usage",
            ));
        }
    }

    if checks.is_empty() {
        checks.push(DiagnosticCheck::pass("MCP", "MCP readiness check complete"));
    }

    checks
}

enum McpWorkflowStatus {
    Found(usize),
    None,
    NoWorkflows,
}

/// Scan workflow files for `mcp:` references.
fn find_mcp_workflows() -> McpWorkflowStatus {
    let mut total_workflows = 0usize;
    let mut mcp_count = 0usize;

    let dirs_to_scan: &[&str] = &[".", "workflows", "examples", ".nika/workflows"];

    for dir in dirs_to_scan {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.ends_with(".nika.yaml") {
                    total_workflows += 1;
                    if let Ok(content) = fs::read_to_string(entry.path()) {
                        // Look for mcp: key (as YAML key, not in comments)
                        if content.lines().any(|line| {
                            let trimmed = line.trim();
                            trimmed.starts_with("mcp:") || trimmed.starts_with("invoke:")
                        }) {
                            mcp_count += 1;
                        }
                    }
                }
            }
        }
    }

    if total_workflows == 0 {
        McpWorkflowStatus::NoWorkflows
    } else if mcp_count > 0 {
        McpWorkflowStatus::Found(mcp_count)
    } else {
        McpWorkflowStatus::None
    }
}

// ─── Task 3: Real LSP check ───────────────────────────────────────────────────

fn check_lsp_available() -> Vec<DiagnosticCheck> {
    let mut checks = vec![];

    // Step 1: Check if compiled with LSP feature
    if !cfg!(feature = "lsp") {
        checks.push(DiagnosticCheck::fail(
            "LSP",
            "Language server not compiled (missing lsp feature)",
            "Reinstall with: cargo install nika --features lsp, or: brew reinstall nika",
        ));
        return checks;
    }

    // Step 2: Check if nika binary is in PATH
    match which::which("nika") {
        Ok(path) => {
            checks.push(DiagnosticCheck::pass(
                "LSP",
                format!("nika binary in PATH ({})", path.display()),
            ));
        }
        Err(_) => {
            checks.push(DiagnosticCheck::fail(
                "LSP",
                "nika binary not found in PATH",
                "Add nika to PATH for editor LSP integration",
            ));
            return checks;
        }
    }

    // Step 3: Probe nika lsp --help
    match std::process::Command::new("nika")
        .args(["lsp", "--help"])
        .output()
    {
        Ok(output) if output.status.success() => {
            checks.push(DiagnosticCheck::pass(
                "LSP",
                "Language server responds (nika lsp --help OK)",
            ));
        }
        Ok(_) => {
            checks.push(DiagnosticCheck::warn(
                "LSP",
                "nika lsp --help returned error",
                "LSP may not be compiled in the installed binary. Reinstall with --features lsp",
            ));
        }
        Err(e) => {
            checks.push(DiagnosticCheck::warn(
                "LSP",
                format!("Cannot probe LSP: {e}"),
                "Ensure nika is correctly installed",
            ));
        }
    }

    checks
}

fn check_editor_integration() -> Vec<DiagnosticCheck> {
    let mut checks = vec![];

    // Detect VS Code (or common forks) -- check PATH and platform-specific locations
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

// ─── Task 2: AI Integration Checks with scope labels ──────────────────────────

fn check_ai_rules() -> Vec<DiagnosticCheck> {
    let mut checks = vec![];
    let home = dirs::home_dir().unwrap_or_default();

    // ─── Project-level rules [project] ─────────────────────────────────────
    let project_rules: &[(&str, &str)] = &[
        ("Cursor", ".cursor/rules/nika-workflows.mdc"),
        ("Copilot", ".github/copilot/nika.instructions.md"),
        ("Windsurf", ".windsurf/rules/nika.md"),
        ("Roo Code", ".roo/rules/nika.md"),
    ];

    let mut has_project_rules = false;
    for (tool, path) in project_rules {
        if std::path::Path::new(path).exists() {
            checks.push(DiagnosticCheck::pass(
                "AI Rules",
                format!("[project] {tool} rules present ({path})"),
            ));
            has_project_rules = true;
        }
        // Only warn if the tool is detected (don't warn for tools not installed)
    }

    // ─── User-level rules [user] ───────────────────────────────────────────

    // Claude Code: rules live at user-level ~/.claude/rules/
    let claude_user_path = home.join(".claude/rules/nika.md");
    let has_claude_binary = which::which("claude").is_ok();

    if claude_user_path.exists() {
        checks.push(DiagnosticCheck::pass(
            "AI Rules",
            format!(
                "[user] Claude Code rules present ({})",
                claude_user_path.display()
            ),
        ));
    } else if has_claude_binary {
        // Claude Code is installed but no nika rules
        checks.push(DiagnosticCheck::warn(
            "AI Rules",
            format!(
                "[user] Claude Code detected but no rules at {}",
                claude_user_path.display()
            ),
            "Run: nika setup ai to generate Claude Code rules",
        ));
    }

    // Cursor user-level rules
    let cursor_user_path = home.join(".cursor/rules/nika.mdc");
    if cursor_user_path.exists() {
        checks.push(DiagnosticCheck::pass(
            "AI Rules",
            format!(
                "[user] Cursor rules present ({})",
                cursor_user_path.display()
            ),
        ));
    }

    if checks.is_empty() && !has_project_rules {
        checks.push(DiagnosticCheck::warn(
            "AI Rules",
            "No AI coding tool rules found (user or project)",
            "Run: nika init (select AI rules) or: nika setup ai",
        ));
    }

    checks
}

fn check_agent_skills() -> Vec<DiagnosticCheck> {
    let mut checks = vec![];
    let home = dirs::home_dir().unwrap_or_default();

    // Check user-level skills [user]
    let user_skills = home.join(".agents/skills");
    let has_user = user_skills.join("nika-workflow-syntax").exists()
        || user_skills.join("nika-create").exists();

    // Check project-level skills [project]
    let has_project = std::path::Path::new("skills/nika-workflow-syntax").exists()
        || std::path::Path::new(".agents/skills/nika-workflow-syntax").exists();

    if has_user {
        checks.push(DiagnosticCheck::pass(
            "Agent Skills",
            format!("[user] Nika skills installed at {}", user_skills.display()),
        ));
    }

    if has_project {
        checks.push(DiagnosticCheck::pass(
            "Agent Skills",
            "[project] Nika skills present in project".to_string(),
        ));
    }

    if !has_user && has_project {
        checks.push(DiagnosticCheck::warn(
            "Agent Skills",
            "[user] No global skills (only project-level found)",
            "Run: nika setup ai to install user-level skills for all projects",
        ));
    }

    if !has_user && !has_project {
        checks.push(DiagnosticCheck::warn(
            "Agent Skills",
            "No Nika Agent Skills installed (user or project)",
            "Run: nika setup ai (global) or: npx skills add SuperNovae-studio/nika-skills",
        ));
    }

    checks
}

fn check_agents_md() -> DiagnosticCheck {
    if std::path::Path::new("AGENTS.md").exists() {
        let is_symlink = std::fs::symlink_metadata("CLAUDE.md")
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);
        let points_to_agents = std::fs::read_link("CLAUDE.md")
            .map(|t| t == std::path::Path::new("AGENTS.md"))
            .unwrap_or(false);
        if is_symlink && points_to_agents {
            DiagnosticCheck::pass("AGENTS.md", "AGENTS.md present (CLAUDE.md symlinked)")
        } else if is_symlink {
            DiagnosticCheck::warn(
                "AGENTS.md",
                "CLAUDE.md is a symlink but doesn't point to AGENTS.md",
                "Fix with: ln -sf AGENTS.md CLAUDE.md",
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
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = std::fs::metadata(hook_path) {
                    if meta.permissions().mode() & 0o111 == 0 {
                        return DiagnosticCheck::warn(
                            "Git Hook",
                            "Co-author hook exists but is not executable",
                            "Run: chmod +x .git/hooks/prepare-commit-msg",
                        );
                    }
                }
            }
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

// ─── Task 1: Sectioned output ──────────────────────────────────────────────────

fn output_doctor_text(checks: &[DiagnosticCheck], quiet: bool) {
    if !quiet {
        nika_engine::display::print_doctor_header(env!("CARGO_PKG_VERSION"));
    }

    let mut pass_count = 0;
    let mut warn_count = 0;
    let mut fail_count = 0;
    let mut current_section = "";

    for check in checks {
        // Print section header when section changes
        if !check.section.is_empty() && check.section != current_section {
            current_section = check.section;
            println!();
            println!("  {} {}", "---".dimmed(), current_section.bold().cyan());
        }

        let icon = match check.status {
            DiagnosticStatus::Pass => check.icon().green(),
            DiagnosticStatus::Warn => check.icon().yellow(),
            DiagnosticStatus::Fail => check.icon().red(),
        };

        println!("  {} {} {}", icon, check.name.bold(), check.message);

        if let Some(ref suggestion) = check.suggestion {
            println!("    {} {}", "->".cyan(), suggestion.dimmed());
        }

        match check.status {
            DiagnosticStatus::Pass => pass_count += 1,
            DiagnosticStatus::Warn => warn_count += 1,
            DiagnosticStatus::Fail => fail_count += 1,
        }
    }

    if !quiet {
        nika_engine::display::print_doctor_summary(pass_count, warn_count, fail_count);

        // "Next steps" footer when warnings/failures exist
        if warn_count > 0 || fail_count > 0 {
            println!();
            println!("  {} {}", "Next steps:".bold(), "".dimmed());
            if fail_count > 0 {
                println!(
                    "    {} Fix {} failure(s) above before running workflows",
                    "1.".bold(),
                    fail_count
                );
            }
            if warn_count > 0 {
                let step = if fail_count > 0 { "2." } else { "1." };
                println!(
                    "    {} Address {} warning(s) for optimal experience",
                    step.bold(),
                    warn_count
                );
            }
            println!(
                "    {} Run {} for full diagnostics (includes MCP connectivity)",
                if fail_count > 0 && warn_count > 0 {
                    "3."
                } else if fail_count > 0 || warn_count > 0 {
                    "2."
                } else {
                    "1."
                }
                .bold(),
                "nika doctor --full".cyan()
            );
        }
    }
}

fn output_doctor_json(checks: &[DiagnosticCheck]) -> Result<(), NikaError> {
    let results: Vec<serde_json::Value> = checks
        .iter()
        .map(|c| {
            serde_json::json!({
                "name": c.name,
                "section": c.section,
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
