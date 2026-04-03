//! Doctor diagnostic subcommand handler

use std::fs;

use colored::Colorize;

use nika_engine::display::{hint, section_header, status_line, StatusIcon};
use nika_engine::error::NikaError;

use crate::config::{find_nika_dir, find_project_root_from, ProjectRootSource};
use crate::machine::install::{
    is_version_outdated, query_extension_version, resolve_editor_cli, VSCODE_EDITORS,
};

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
    checks.push(
        DiagnosticCheck::pass("Version", format!("nika {}", env!("CARGO_PKG_VERSION")))
            .in_section("Core"),
    );
    checks.push(check_vault_health().in_section("Core"));
    checks.extend(with_section(check_api_keys(), "Core"));

    // ─── Project ───────────────────────────────────────────────────────────
    {
        let cwd = std::env::current_dir().unwrap_or_default();
        checks.extend(with_section(check_project_structure(&cwd), "Project"));
    }

    // ─── Editor & LSP ──────────────────────────────────────────────────────
    checks.extend(with_section(check_lsp_available(), "Editor & LSP"));
    checks.extend(with_section(check_editor_integration(), "Editor & LSP"));

    // ─── AI Integration ────────────────────────────────────────────────────
    checks.extend(with_section(check_ai_rules(), "AI Integration"));
    checks.extend(with_section(check_agent_skills(), "AI Integration"));
    checks.push(check_agents_md().in_section("AI Integration"));

    // ─── Daemon ──────────────────────────────────────────────────────────────
    #[cfg(unix)]
    checks.extend(with_section(check_daemon().await, "Daemon"));

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

fn check_vault_health() -> DiagnosticCheck {
    let vault = crate::provider::get_vault();
    if !vault.exists() {
        return DiagnosticCheck::pass(
            "Vault",
            "No vault (keys stored via env vars or not yet set)",
        );
    }
    match vault.health_check() {
        Ok(true) => DiagnosticCheck::pass("Vault", "Encrypted vault is readable"),
        Ok(false) => DiagnosticCheck::pass(
            "Vault",
            "No vault (keys stored via env vars or not yet set)",
        ),
        Err(e) => DiagnosticCheck::fail(
            "Vault",
            format!("Vault cannot be decrypted: {e}"),
            "Run 'nika provider vault-reset' to start fresh, or set NIKA_VAULT_PASSPHRASE",
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

// ─── Project structure checks ────────────────────────────────────────────────

/// Check project structure from a given root directory.
///
/// Accepts a `Path` so that tests can pass a temp directory.
fn check_project_structure(start: &std::path::Path) -> Vec<DiagnosticCheck> {
    let mut checks = vec![];

    // 1. Check nika.toml exists → show project root path, pass/fail
    let project = match find_project_root_from(start) {
        Ok(p) => p,
        Err(_) => {
            checks.push(DiagnosticCheck::fail(
                "nika.toml",
                "Cannot determine project root",
                "Check filesystem permissions",
            ));
            return checks;
        }
    };

    match project.source {
        ProjectRootSource::NikaToml => {
            // Validate nika.toml is valid TOML
            let toml_path = project.root.join("nika.toml");
            match fs::read_to_string(&toml_path) {
                Ok(content) => match toml::from_str::<toml::Value>(&content) {
                    Ok(_) => {
                        checks.push(DiagnosticCheck::pass(
                            "nika.toml",
                            format!("Project root: {}", project.root.display()),
                        ));
                    }
                    Err(e) => {
                        checks.push(DiagnosticCheck::fail(
                            "nika.toml",
                            format!("nika.toml has syntax errors: {e}"),
                            "Run 'nika config edit' to fix",
                        ));
                    }
                },
                Err(e) => {
                    checks.push(DiagnosticCheck::fail(
                        "nika.toml",
                        format!("Cannot read nika.toml: {e}"),
                        "Check file permissions",
                    ));
                }
            }
        }
        ProjectRootSource::DotNika => {
            checks.push(DiagnosticCheck::warn(
                "nika.toml",
                format!(
                    "No nika.toml found (using legacy .nika/ at {})",
                    project.root.display()
                ),
                "Run 'nika init' to create nika.toml",
            ));
        }
        ProjectRootSource::Fallback => {
            checks.push(DiagnosticCheck::warn(
                "nika.toml",
                "No nika.toml or .nika/ found",
                "Run 'nika init' to initialize a Nika project",
            ));
        }
    }

    let root = &project.root;

    // 2. Check .gitignore includes `.nika/`
    let gitignore_path = root.join(".gitignore");
    if gitignore_path.exists() {
        let content = fs::read_to_string(&gitignore_path).unwrap_or_default();
        let has_nika_ignore = content.lines().any(|line| {
            let trimmed = line.trim();
            trimmed == ".nika/" || trimmed == ".nika" || trimmed == "/.nika/" || trimmed == "/.nika"
        });
        if has_nika_ignore {
            checks.push(DiagnosticCheck::pass(".gitignore", ".nika/ is gitignored"));
        } else {
            checks.push(DiagnosticCheck::warn(
                ".gitignore",
                ".nika/ is not in .gitignore (runtime data may be committed)",
                "Add '.nika/' to .gitignore",
            ));
        }
    } else {
        checks.push(DiagnosticCheck::warn(
            ".gitignore",
            "No .gitignore found",
            "Create .gitignore with '.nika/' and 'artifacts/' entries",
        ));
    }

    // 3. Check .gitignore includes artifacts dir
    let artifacts_dir = read_artifacts_dir(root);
    if gitignore_path.exists() {
        let content = fs::read_to_string(&gitignore_path).unwrap_or_default();
        let has_artifacts_ignore = content.lines().any(|line| {
            let trimmed = line.trim();
            // Match the dir with or without leading / and trailing /
            let dir_name = artifacts_dir.strip_prefix("./").unwrap_or(&artifacts_dir);
            trimmed == dir_name
                || trimmed == format!("{}/", dir_name)
                || trimmed == format!("/{}", dir_name)
                || trimmed == format!("/{}/", dir_name)
        });
        if has_artifacts_ignore {
            checks.push(DiagnosticCheck::pass(
                ".gitignore",
                format!("{artifacts_dir} is gitignored"),
            ));
        } else {
            checks.push(DiagnosticCheck::warn(
                ".gitignore",
                format!("{artifacts_dir} is not in .gitignore (outputs may be committed)"),
                format!("Add '{artifacts_dir}' to .gitignore"),
            ));
        }
    }

    // 4. Detect legacy `.nika/config.toml`
    let legacy_config = root.join(".nika").join("config.toml");
    if legacy_config.exists() {
        checks.push(DiagnosticCheck::warn(
            "Legacy config",
            format!(
                "Found legacy .nika/config.toml at {}",
                legacy_config.display()
            ),
            "Migrate to nika.toml with 'nika init' (new project root marker)",
        ));
    }

    // 5. Count `*.nika.yaml` files recursively
    let workflow_count = count_workflows_recursive(root);
    if workflow_count == 0 {
        checks.push(DiagnosticCheck::warn(
            "Workflows",
            "No *.nika.yaml files found",
            "Create one with 'nika new my-flow --verb infer'",
        ));
    } else {
        checks.push(DiagnosticCheck::pass(
            "Workflows",
            format!("{workflow_count} workflow(s) found (recursive scan)"),
        ));
    }

    checks
}

/// Read the artifacts directory from nika.toml, defaulting to "artifacts".
fn read_artifacts_dir(root: &std::path::Path) -> String {
    let toml_path = root.join("nika.toml");
    if toml_path.exists() {
        if let Ok(content) = fs::read_to_string(&toml_path) {
            if let Ok(value) = toml::from_str::<toml::Value>(&content) {
                if let Some(dir) = value
                    .get("artifacts")
                    .and_then(|a| a.get("dir"))
                    .and_then(|d| d.as_str())
                {
                    return dir.to_string();
                }
            }
        }
    }
    "artifacts".to_string()
}

/// Recursively count *.nika.yaml files under `root`, skipping hidden dirs and
/// common non-project directories.
fn count_workflows_recursive(root: &std::path::Path) -> usize {
    let mut count = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if path.is_dir() {
                    // Skip hidden dirs, node_modules, target, .nika
                    if !name.starts_with('.') && name != "node_modules" && name != "target" {
                        stack.push(path);
                    }
                } else if name.ends_with(".nika.yaml") {
                    count += 1;
                }
            }
        }
    }
    count
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

            // Step 3: Check MCP config — .mcp.json (preferred) or .nika/mcp.yaml (legacy)
            {
                let current = std::env::current_dir().unwrap_or_default();
                let project = crate::config::find_project_root_from(&current).unwrap_or(
                    crate::config::ProjectRoot {
                        root: current,
                        source: crate::config::ProjectRootSource::Fallback,
                    },
                );

                let mcp_json_path = project.root.join(".mcp.json");
                let legacy_yaml = project.root.join(".nika").join("mcp.yaml");

                if mcp_json_path.exists() {
                    checks.push(DiagnosticCheck::pass(
                        "MCP",
                        "MCP configuration found in .mcp.json",
                    ));
                } else if legacy_yaml.exists() {
                    checks.push(DiagnosticCheck::pass(
                        "MCP",
                        "MCP configuration found in .nika/mcp.yaml (consider migrating to .mcp.json)",
                    ));
                } else {
                    checks.push(DiagnosticCheck::warn(
                        "MCP",
                        "Workflows use MCP but no .mcp.json found",
                        "Create .mcp.json at project root (Claude Code convention)",
                    ));
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
                "Add nika to PATH: export PATH=\"$HOME/.cargo/bin:$PATH\" (cargo) or verify brew install",
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

/// Infer the marketplace source from the editor binary name.
fn extension_source_label(binary: &str) -> &'static str {
    match binary {
        "code" => "Marketplace",
        "cursor" | "windsurf" => "Open VSX",
        _ => "marketplace",
    }
}

fn check_editor_integration() -> Vec<DiagnosticCheck> {
    let mut checks = vec![];
    let mut found_any = false;

    for def in VSCODE_EDITORS {
        let Some(cli) = resolve_editor_cli(def.binary) else {
            continue;
        };

        let Ok(output) = std::process::Command::new(&cli).arg("--version").output() else {
            continue;
        };
        if !output.status.success() {
            continue;
        }

        let version = String::from_utf8_lossy(&output.stdout);
        let first_line = version.lines().next().unwrap_or("unknown").trim();
        checks.push(DiagnosticCheck::pass(
            "Editor",
            format!("{} {} detected", def.name, first_line),
        ));
        found_any = true;

        // Check nika-lang extension for this editor
        let cli_ver = env!("CARGO_PKG_VERSION");
        match query_extension_version(&cli, def.ext_id) {
            Some(ext_ver) => {
                if is_version_outdated(&ext_ver, cli_ver) {
                    checks.push(DiagnosticCheck::warn(
                        "Extension",
                        format!(
                            "{}: nika-lang v{ext_ver} outdated (CLI v{cli_ver})",
                            def.name
                        ),
                        format!(
                            "Update: {} --install-extension {} --force",
                            def.binary, def.ext_id
                        ),
                    ));
                } else {
                    let source = extension_source_label(def.binary);
                    checks.push(DiagnosticCheck::pass(
                        "Extension",
                        format!("{}: nika-lang v{ext_ver} ({source})", def.name),
                    ));
                }
            }
            None => {
                checks.push(DiagnosticCheck::warn(
                    "Extension",
                    format!("{}: nika-lang not installed", def.name),
                    format!("Install: {} --install-extension {}", def.binary, def.ext_id),
                ));
            }
        }
    }

    if !found_any {
        checks.push(DiagnosticCheck::warn(
            "Editor",
            "No supported editor detected (VS Code, Cursor, Windsurf)",
            "Install VS Code and add 'code' to PATH for LSP integration",
        ));
    }

    checks
}

// ─── Task 2: AI Integration Checks with scope labels ──────────────────────────

fn check_ai_rules() -> Vec<DiagnosticCheck> {
    let mut checks = vec![];
    let home = dirs::home_dir().unwrap_or_default();

    // ─── User-level rules [user] — installed by `nika setup` ─────────────
    let user_rules: &[(&str, &str)] = &[
        ("Claude Code", ".claude/rules/nika.md"),
        ("Cursor", ".cursor/rules/nika.mdc"),
        ("Copilot", ".github/copilot/nika.instructions.md"),
        ("Windsurf", ".windsurf/rules/nika.md"),
        ("Roo Code", ".roo/rules/nika.md"),
    ];

    let has_claude_binary = which::which("claude").is_ok();
    let mut has_user_rules = false;

    for (tool, rel_path) in user_rules {
        let path = home.join(rel_path);
        if path.exists() {
            checks.push(DiagnosticCheck::pass(
                "AI Rules",
                format!("[user] {tool} rules present ({})", path.display()),
            ));
            has_user_rules = true;
        } else if *tool == "Claude Code" && has_claude_binary {
            checks.push(DiagnosticCheck::warn(
                "AI Rules",
                format!(
                    "[user] Claude Code detected but no rules at {}",
                    path.display()
                ),
                "Run: nika init (auto-installs editor rules on first run)",
            ));
        }
    }

    // ─── Project-level rules [project] — manually placed or committed ─────
    let project_rules: &[(&str, &str)] = &[
        ("Cursor", ".cursor/rules/nika.mdc"),
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
    }

    if checks.is_empty() && !has_user_rules && !has_project_rules {
        checks.push(DiagnosticCheck::warn(
            "AI Rules",
            "No AI coding tool rules found (user or project)",
            "Run: nika init (auto-installs editor rules on first run)",
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
            "Run: nika doctor --fix to install user-level skills for all projects",
        ));
    }

    if !has_user && !has_project {
        checks.push(DiagnosticCheck::warn(
            "Agent Skills",
            "No Nika Agent Skills installed (user or project)",
            "Run: nika doctor --fix (global) or: npx skills add supernovae-st/nika-skills",
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
            "Create with: nika init (generates AGENTS.md + CLAUDE.md symlink)",
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
                "Run: nika doctor --fix (will backup existing hook)",
            )
        }
    } else {
        DiagnosticCheck::warn(
            "Git Hook",
            "No co-author hook installed",
            "Run: nika doctor --fix",
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
            println!("{}", section_header(current_section));
        }

        let icon = match check.status {
            DiagnosticStatus::Pass => StatusIcon::Ok,
            DiagnosticStatus::Warn => StatusIcon::Warn,
            DiagnosticStatus::Fail => StatusIcon::Fail,
        };

        println!(
            "{}",
            status_line(icon, &format!("{} {}", check.name.bold(), check.message))
        );

        if let Some(ref suggestion) = check.suggestion {
            println!("{}", hint(suggestion));
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
            println!("{}", section_header("Next steps"));
            if fail_count > 0 {
                println!(
                    "{}",
                    hint(&format!(
                        "Fix {} failure(s) above before running workflows",
                        fail_count
                    ))
                );
            }
            if warn_count > 0 {
                println!(
                    "{}",
                    hint(&format!(
                        "Address {} warning(s) for optimal experience",
                        warn_count
                    ))
                );
            }
            println!(
                "{}",
                hint("nika doctor --full  Full diagnostics (includes MCP connectivity)")
            );
        }
    }
}

#[cfg(unix)]
async fn check_daemon() -> Vec<DiagnosticCheck> {
    use nika_daemon::{daemon_pid_path, daemon_socket_path, DaemonClient};
    use std::time::Duration;

    let mut checks = vec![];
    let socket_path = daemon_socket_path();
    let pid_path = daemon_pid_path();

    // Check socket exists
    if !socket_path.exists() {
        checks.push(DiagnosticCheck::warn(
            "Daemon socket",
            "daemon not running",
            "start with: nika daemon start",
        ));
        return checks;
    }
    checks.push(DiagnosticCheck::pass(
        "Daemon socket",
        format!("{}", socket_path.display()),
    ));

    // Check PID file
    match nika_daemon::lifecycle::check_pid_file(&pid_path) {
        Ok(Some(pid)) => {
            checks.push(DiagnosticCheck::pass("Daemon PID", format!("pid {pid}")));
        }
        Ok(None) => {
            checks.push(DiagnosticCheck::warn(
                "Daemon PID",
                "socket exists but no valid PID file",
                "try: nika daemon restart",
            ));
        }
        Err(e) => {
            checks.push(DiagnosticCheck::fail(
                "Daemon PID",
                format!("PID check failed: {e}"),
                "try: nika daemon restart",
            ));
        }
    }

    // Ping the daemon
    let client = DaemonClient::new(&socket_path).with_timeout(Duration::from_secs(2));
    match client.ping().await {
        Ok((version, uptime_secs)) => {
            checks.push(DiagnosticCheck::pass(
                "Daemon ping",
                format!("v{version}, uptime {uptime_secs}s"),
            ));

            // Version mismatch check
            let cli_version = env!("CARGO_PKG_VERSION");
            if version != cli_version {
                checks.push(DiagnosticCheck::warn(
                    "Daemon version",
                    format!("daemon v{version} != CLI v{cli_version}"),
                    "restart daemon: nika daemon restart",
                ));
            }
        }
        Err(e) => {
            checks.push(DiagnosticCheck::fail(
                "Daemon ping",
                format!("ping failed: {e}"),
                "try: nika daemon restart",
            ));
        }
    }

    checks
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_outdated_detects_minor_gap() {
        assert!(is_version_outdated("0.42.0", "0.58.0"));
        assert!(is_version_outdated("0.51.0", "0.58.0"));
        assert!(is_version_outdated("0.57.0", "0.58.0"));
    }

    #[test]
    fn version_outdated_same_minor_is_ok() {
        assert!(!is_version_outdated("0.58.0", "0.58.0"));
        assert!(!is_version_outdated("0.58.1", "0.58.0"));
        assert!(!is_version_outdated("0.58.3", "0.58.0"));
    }

    #[test]
    fn version_outdated_major_gap() {
        assert!(is_version_outdated("0.99.0", "1.0.0"));
        assert!(!is_version_outdated("1.0.0", "0.99.0"));
    }

    #[test]
    fn diagnostic_pass_creates_pass_status() {
        let check = DiagnosticCheck::pass("test", "ok");
        assert_eq!(check.status, DiagnosticStatus::Pass);
        assert!(check.suggestion.is_none());
    }

    #[test]
    fn diagnostic_fail_creates_fail_with_suggestion() {
        let check = DiagnosticCheck::fail("test", "bad", "fix it");
        assert_eq!(check.status, DiagnosticStatus::Fail);
        assert_eq!(check.suggestion.as_deref(), Some("fix it"));
    }

    #[test]
    fn diagnostic_warn_creates_warn_with_suggestion() {
        let check = DiagnosticCheck::warn("test", "meh", "consider this");
        assert_eq!(check.status, DiagnosticStatus::Warn);
        assert_eq!(check.suggestion.as_deref(), Some("consider this"));
    }

    #[test]
    fn in_section_assigns_section() {
        let check = DiagnosticCheck::pass("test", "ok").in_section("System");
        assert_eq!(check.section, "System");
    }

    #[test]
    fn with_section_assigns_all_checks() {
        let checks = vec![
            DiagnosticCheck::pass("a", "ok"),
            DiagnosticCheck::fail("b", "bad", "fix"),
        ];
        let grouped = with_section(checks, "Group");
        assert!(grouped.iter().all(|c| c.section == "Group"));
    }

    #[test]
    fn json_output_is_valid_json() {
        let checks = [
            DiagnosticCheck::pass("test1", "ok").in_section("System"),
            DiagnosticCheck::fail("test2", "bad", "fix").in_section("System"),
            DiagnosticCheck::warn("test3", "meh", "hint").in_section("Config"),
        ];
        // Simulate JSON output construction
        let results: Vec<serde_json::Value> = checks
            .iter()
            .map(|c| {
                let mut obj = serde_json::json!({
                    "name": c.name,
                    "status": format!("{:?}", c.status),
                    "message": c.message,
                });
                if let Some(ref s) = c.suggestion {
                    obj["suggestion"] = serde_json::json!(s);
                }
                obj
            })
            .collect();
        let output = serde_json::json!({"checks": results});
        assert!(serde_json::to_string(&output).is_ok());
    }

    #[test]
    fn check_api_keys_returns_at_least_one_check() {
        let checks = check_api_keys();
        // Always returns at least one check (either found keys or "no keys" warning)
        assert!(
            !checks.is_empty(),
            "Should return at least one check result"
        );
    }

    #[test]
    fn check_project_structure_returns_checks() {
        let temp = tempfile::tempdir().unwrap();
        let checks = check_project_structure(temp.path());
        assert!(
            !checks.is_empty(),
            "Should always return at least one check"
        );
    }

    #[test]
    fn doctor_summary_counts_correct() {
        let checks = [
            DiagnosticCheck::pass("a", "ok"),
            DiagnosticCheck::pass("b", "ok"),
            DiagnosticCheck::warn("c", "meh", "hint"),
            DiagnosticCheck::fail("d", "bad", "fix"),
        ];
        let pass = checks
            .iter()
            .filter(|c| c.status == DiagnosticStatus::Pass)
            .count();
        let warn = checks
            .iter()
            .filter(|c| c.status == DiagnosticStatus::Warn)
            .count();
        let fail = checks
            .iter()
            .filter(|c| c.status == DiagnosticStatus::Fail)
            .count();
        assert_eq!(pass, 2);
        assert_eq!(warn, 1);
        assert_eq!(fail, 1);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Phase 5: Doctor project structure checks (TDD)
    // ═══════════════════════════════════════════════════════════════════════

    // Test 35: doctor detects nika.toml and reports project root
    #[test]
    fn doctor_detects_nika_toml() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("nika.toml"),
            "[project]\nname = \"test\"\n",
        )
        .unwrap();
        // Also create .gitignore with .nika/ and artifacts/ to avoid extra warnings
        std::fs::write(temp.path().join(".gitignore"), ".nika/\nartifacts/\n").unwrap();

        let checks = check_project_structure(temp.path());

        // First check should be nika.toml pass with project root
        let toml_check = checks.iter().find(|c| c.name == "nika.toml").unwrap();
        assert_eq!(toml_check.status, DiagnosticStatus::Pass);
        assert!(
            toml_check.message.contains("Project root:"),
            "Expected 'Project root:' in message, got: {}",
            toml_check.message
        );
        assert!(
            toml_check
                .message
                .contains(&temp.path().display().to_string()),
            "Expected temp path in message, got: {}",
            toml_check.message
        );
    }

    // Test 36: doctor warns when .gitignore is missing .nika/
    #[test]
    fn doctor_warns_gitignore_missing_nika() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("nika.toml"),
            "[project]\nname = \"test\"\n",
        )
        .unwrap();
        // .gitignore exists but does NOT contain .nika/
        std::fs::write(temp.path().join(".gitignore"), "node_modules/\n").unwrap();

        let checks = check_project_structure(temp.path());

        let gitignore_checks: Vec<_> = checks.iter().filter(|c| c.name == ".gitignore").collect();
        // Should have a warning about .nika/ missing
        let nika_warn = gitignore_checks
            .iter()
            .find(|c| c.message.contains(".nika/"))
            .expect("Expected a .gitignore check mentioning .nika/");
        assert_eq!(nika_warn.status, DiagnosticStatus::Warn);
        assert!(
            nika_warn.suggestion.as_deref().unwrap().contains(".nika/"),
            "Suggestion should mention .nika/"
        );
    }

    // Test 37: doctor warns when .gitignore is missing artifacts/
    #[test]
    fn doctor_warns_gitignore_missing_artifacts() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("nika.toml"),
            "[project]\nname = \"test\"\n",
        )
        .unwrap();
        // .gitignore has .nika/ but NOT artifacts/
        std::fs::write(temp.path().join(".gitignore"), ".nika/\n").unwrap();

        let checks = check_project_structure(temp.path());

        let gitignore_checks: Vec<_> = checks.iter().filter(|c| c.name == ".gitignore").collect();
        // Should have a warning about artifacts/ missing
        let artifacts_warn = gitignore_checks
            .iter()
            .find(|c| c.message.contains("artifacts"))
            .expect("Expected a .gitignore check mentioning artifacts");
        assert_eq!(artifacts_warn.status, DiagnosticStatus::Warn);
    }

    // Test 38: doctor detects legacy .nika/config.toml
    #[test]
    fn doctor_detects_legacy_config() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("nika.toml"),
            "[project]\nname = \"test\"\n",
        )
        .unwrap();
        std::fs::write(temp.path().join(".gitignore"), ".nika/\nartifacts/\n").unwrap();
        // Create legacy .nika/config.toml
        let nika_dir = temp.path().join(".nika");
        std::fs::create_dir_all(&nika_dir).unwrap();
        std::fs::write(nika_dir.join("config.toml"), "[editor]\ntheme = \"dark\"\n").unwrap();

        let checks = check_project_structure(temp.path());

        let legacy_check = checks
            .iter()
            .find(|c| c.name == "Legacy config")
            .expect("Expected a 'Legacy config' check");
        assert_eq!(legacy_check.status, DiagnosticStatus::Warn);
        assert!(
            legacy_check.message.contains(".nika/config.toml"),
            "Should mention .nika/config.toml, got: {}",
            legacy_check.message
        );
        assert!(
            legacy_check
                .suggestion
                .as_deref()
                .unwrap()
                .contains("nika init"),
            "Should suggest nika init"
        );
    }

    // Test 39: doctor counts *.nika.yaml files recursively
    #[test]
    fn doctor_counts_workflows() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("nika.toml"),
            "[project]\nname = \"test\"\n",
        )
        .unwrap();
        std::fs::write(temp.path().join(".gitignore"), ".nika/\nartifacts/\n").unwrap();

        // Create workflows at various depths
        std::fs::write(
            temp.path().join("root.nika.yaml"),
            "schema: 'nika/workflow@0.12'\n",
        )
        .unwrap();
        let sub = temp.path().join("flows");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("a.nika.yaml"), "schema: 'nika/workflow@0.12'\n").unwrap();
        std::fs::write(sub.join("b.nika.yaml"), "schema: 'nika/workflow@0.12'\n").unwrap();
        let deep = temp.path().join("flows").join("nested");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("c.nika.yaml"), "schema: 'nika/workflow@0.12'\n").unwrap();

        // Should NOT count files in hidden dirs
        let hidden = temp.path().join(".hidden");
        std::fs::create_dir_all(&hidden).unwrap();
        std::fs::write(hidden.join("skip.nika.yaml"), "").unwrap();

        let checks = check_project_structure(temp.path());

        let wf_check = checks
            .iter()
            .find(|c| c.name == "Workflows" && c.message.contains("recursive"))
            .expect("Expected a recursive Workflows check");
        assert_eq!(wf_check.status, DiagnosticStatus::Pass);
        assert!(
            wf_check.message.contains("4 workflow(s)"),
            "Expected 4 workflows (skipping hidden dir), got: {}",
            wf_check.message
        );
    }
}
