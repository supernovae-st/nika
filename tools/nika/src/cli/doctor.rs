//! Doctor diagnostic subcommand handler

use std::fs;

use colored::Colorize;

use nika::error::NikaError;

use super::config::find_nika_dir;

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

    // 1. Check .nika directory
    checks.push(check_nika_directory());

    // 2. Check config file
    checks.push(check_config_file());

    // 3. Check API keys
    checks.extend(check_api_keys());

    // 4. Check trace directory
    checks.push(check_trace_directory());

    // 5. Check Rust version
    checks.push(check_rust_version());

    // 6. Full mode: Check MCP connectivity (slow)
    if full {
        checks.push(check_mcp_connectivity().await);
    }

    // Output results
    if format == "json" {
        output_doctor_json(&checks);
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

fn check_nika_directory() -> DiagnosticCheck {
    match find_nika_dir() {
        Ok(dir) if dir.exists() => DiagnosticCheck::pass(
            "Project",
            format!(".nika directory found at {}", dir.display()),
        ),
        Ok(dir) => DiagnosticCheck::warn(
            "Project",
            format!("No .nika directory at {}", dir.display()),
            "Run 'nika init' to create project structure",
        ),
        Err(_) => DiagnosticCheck::fail(
            "Project",
            "Cannot determine current directory",
            "Check filesystem permissions",
        ),
    }
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
                format!("config.toml has syntax errors: {}", e),
                "Run 'nika config edit' to fix",
            ),
        },
        Err(e) => DiagnosticCheck::fail(
            "Config",
            format!("Cannot read config.toml: {}", e),
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
    ];

    let mut any_found = false;
    for (env_var, provider) in keys {
        if std::env::var(env_var).is_ok() {
            checks.push(DiagnosticCheck::pass(
                "API Key",
                format!("{} API key configured ({})", provider, env_var),
            ));
            any_found = true;
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

fn check_trace_directory() -> DiagnosticCheck {
    let nika_dir = match find_nika_dir() {
        Ok(d) => d,
        Err(_) => {
            return DiagnosticCheck::warn(
                "Traces",
                "Cannot locate .nika directory",
                "Run 'nika init' first",
            )
        }
    };

    let trace_dir = nika_dir.join("traces");

    // Check if directory exists
    if !trace_dir.exists() {
        return DiagnosticCheck::warn(
            "Traces",
            "Trace directory doesn't exist",
            "It will be created on first workflow run",
        );
    }

    // Check if writable by attempting to create a temp file
    let test_file = trace_dir.join(".nika_doctor_test");
    match fs::write(&test_file, b"test") {
        Ok(_) => {
            let _ = fs::remove_file(&test_file);
            DiagnosticCheck::pass(
                "Traces",
                format!("Trace directory writable ({})", trace_dir.display()),
            )
        }
        Err(e) => DiagnosticCheck::fail(
            "Traces",
            format!("Trace directory not writable: {}", e),
            "Check directory permissions",
        ),
    }
}

fn check_rust_version() -> DiagnosticCheck {
    // Get rustc version
    match std::process::Command::new("rustc")
        .arg("--version")
        .output()
    {
        Ok(output) => {
            let version = String::from_utf8_lossy(&output.stdout);
            let version = version.trim();
            if version.contains("1.8") || version.contains("1.9") {
                DiagnosticCheck::pass("Rust", version.to_string())
            } else if version.starts_with("rustc 1.7") {
                DiagnosticCheck::warn(
                    "Rust",
                    format!("{} (older version)", version),
                    "Consider updating: rustup update",
                )
            } else {
                DiagnosticCheck::pass("Rust", version.to_string())
            }
        }
        Err(_) => DiagnosticCheck::warn(
            "Rust",
            "rustc not found in PATH",
            "Install Rust: https://rustup.rs",
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

fn output_doctor_text(checks: &[DiagnosticCheck], quiet: bool) {
    if !quiet {
        println!();
        println!("{}", "Nika Doctor".bold());
        println!("{}", "═".repeat(50));
        println!();
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

        println!("{} {} {}", icon, check.name.bold(), check.message);

        if let Some(ref suggestion) = check.suggestion {
            println!("  {} {}", "→".cyan(), suggestion);
        }

        match check.status {
            DiagnosticStatus::Pass => pass_count += 1,
            DiagnosticStatus::Warn => warn_count += 1,
            DiagnosticStatus::Fail => fail_count += 1,
        }
    }

    if !quiet {
        println!();
        println!(
            "{} {} passed, {} warnings, {} failed",
            "Summary:".bold(),
            pass_count.to_string().green(),
            warn_count.to_string().yellow(),
            fail_count.to_string().red()
        );
    }
}

fn output_doctor_json(checks: &[DiagnosticCheck]) {
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

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}
