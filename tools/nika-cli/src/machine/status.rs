//! Machine status checks and marker file management.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

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

/// Returns true in CI/CD or automated environments where machine setup must not run.
pub fn is_ci() -> bool {
    use std::env;
    // Explicit opt-out
    if env::var("NIKA_NO_SETUP").map(|v| v == "1").unwrap_or(false) {
        return true;
    }
    // GitHub Codespaces is a dev environment, not CI — allow setup there.
    // Codespaces sets both CODESPACES=true and CI=true, so check first.
    if env::var("CODESPACES").map(|v| v == "true").unwrap_or(false) {
        return false;
    }
    if env::var("CI").is_ok() {
        return true;
    }
    let ci_vars = [
        "GITHUB_ACTIONS",
        "GITLAB_CI",
        "CIRCLECI",
        "JENKINS_URL",
        "BUILDKITE",
        "TRAVIS",
        "CODEBUILD_BUILD_ID",
        "TF_BUILD",
    ];
    if ci_vars.iter().any(|v| env::var(v).is_ok()) {
        return true;
    }
    if dirs::home_dir().is_none() {
        return true;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let dumb = env::var("TERM").map(|t| t == "dumb").unwrap_or(false);
        let no_display = env::var("DISPLAY").is_err() && env::var("WAYLAND_DISPLAY").is_err();
        if dumb && no_display {
            return true;
        }
    }
    false
}

/// Path to the machine marker file.
pub(crate) fn machine_toml_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".nika")
        .join("machine.toml")
}

/// Update ONLY the version field in machine.toml, preserving everything else.
/// Used by `fast_rule_update()` after silently updating rules.
pub fn update_marker_version() {
    let path = machine_toml_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let new_version = env!("CARGO_PKG_VERSION");
    let updated: String = content
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("version") && trimmed[7..].trim_start().starts_with('=') {
                format!("version = \"{}\"", new_version)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&path, updated + "\n").ok();
}

/// Write the machine.toml marker file after setup completes.
pub(super) fn write_marker(results: &[SetupResult]) {
    let editors = super::install::detect_editors();
    write_marker_with_editors(results, &editors);
}

fn write_marker_with_editors(results: &[SetupResult], editors: &[&str]) {
    let marker_path = machine_toml_path();
    let Some(dir) = marker_path.parent() else {
        return;
    };
    std::fs::create_dir_all(dir).ok();

    let ai_tools: Vec<&str> = results
        .iter()
        .filter(|r| {
            r.success
                && [
                    "Claude Code",
                    "Cursor",
                    "Copilot",
                    "Windsurf",
                    "Roo Code",
                    "Agent Skills",
                ]
                .contains(&r.name.as_str())
        })
        .map(|r| r.name.as_str())
        .collect();

    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Format editors as TOML array: editors = ["vscode", "claude", "cursor"]
    let editors_toml: Vec<String> = editors.iter().map(|e| format!("\"{}\"", e)).collect();

    let content = format!(
        "[machine]\nsetup_at = \"{}\"\nversion = \"{}\"\neditors = [{}]\nai_tools = {:?}\n",
        secs,
        env!("CARGO_PKG_VERSION"),
        editors_toml.join(", "),
        ai_tools,
    );

    // Don't fail init if marker write fails
    std::fs::write(&marker_path, content).ok();
}
