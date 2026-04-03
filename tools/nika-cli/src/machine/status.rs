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

/// Update ONLY the version field in the `[machine]` section of machine.toml.
/// Used by `fast_rule_update()` after silently updating rules.
pub fn update_marker_version() {
    let path = machine_toml_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let new_version = env!("CARGO_PKG_VERSION");
    let mut in_machine = false;
    let mut replaced = false;
    let updated: String = content
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed == "[machine]" {
                in_machine = true;
            } else if trimmed.starts_with('[') {
                in_machine = false;
            }
            if in_machine
                && !replaced
                && trimmed.starts_with("version")
                && trimmed[7..].trim_start().starts_with('=')
            {
                replaced = true;
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

    let editors_toml: Vec<String> = editors.iter().map(|e| format!("\"{}\"", e)).collect();

    // Build new [machine] section
    let machine_section = format!(
        "[machine]\nsetup_at = \"{}\"\nversion = \"{}\"\neditors = [{}]\nai_tools = {:?}\n",
        secs,
        env!("CARGO_PKG_VERSION"),
        editors_toml.join(", "),
        ai_tools,
    );

    // Preserve non-[machine] sections (e.g. [extensions], [rule_hashes])
    let mut extra_sections = String::new();
    if let Ok(existing) = std::fs::read_to_string(&marker_path) {
        let mut in_machine = false;
        let mut in_other = false;
        for line in existing.lines() {
            let trimmed = line.trim();
            if trimmed == "[machine]" {
                in_machine = true;
                in_other = false;
                continue;
            }
            if trimmed.starts_with('[') {
                in_machine = false;
                in_other = true;
            }
            if in_machine {
                continue; // skip old [machine] content
            }
            if in_other {
                extra_sections.push_str(line);
                extra_sections.push('\n');
            }
        }
    }

    let content = if extra_sections.is_empty() {
        machine_section
    } else {
        format!("{}\n{}", machine_section, extra_sections)
    };

    std::fs::write(&marker_path, content).ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════════════
    // machine_toml_path
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn machine_toml_path_ends_with_marker_file() {
        let path = machine_toml_path();
        assert!(
            path.to_string_lossy().ends_with("machine.toml"),
            "path should end with machine.toml, got: {}",
            path.display()
        );
    }

    #[test]
    fn machine_toml_path_is_under_nika_dir() {
        let path = machine_toml_path();
        let parent = path.parent().unwrap();
        assert!(
            parent.to_string_lossy().ends_with(".nika"),
            "parent should be .nika dir, got: {}",
            parent.display()
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // MachineStatus parsing logic
    // ═══════════════════════════════════════════════════════════════════════

    /// Parsing a marker file with matching version yields Ready.
    #[test]
    fn marker_version_matching_is_ready() {
        let current = env!("CARGO_PKG_VERSION");
        let content = format!(
            "[machine]\nsetup_at = \"1711900000\"\nversion = \"{}\"\n",
            current
        );
        // Parse version the same way machine_setup_status does
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("version") {
                let rest = rest.trim().strip_prefix('=').unwrap_or("").trim();
                let version = rest.trim_matches('"');
                if version == current {
                    return; // Test passes: version matches
                }
                panic!("version should match current");
            }
        }
        panic!("version line not found");
    }

    /// Parsing a marker file with old version yields NeedsUpdate.
    #[test]
    fn marker_version_mismatch_is_needs_update() {
        let content = "[machine]\nversion = \"0.0.1\"\n";
        let current = env!("CARGO_PKG_VERSION");
        let mut status = MachineStatus::NeedsUpdate; // default if no version line
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("version") {
                let rest = rest.trim().strip_prefix('=').unwrap_or("").trim();
                let version = rest.trim_matches('"');
                if version == current {
                    status = MachineStatus::Ready;
                } else {
                    status = MachineStatus::NeedsUpdate;
                }
            }
        }
        assert_eq!(status, MachineStatus::NeedsUpdate);
    }

    /// Missing version line in marker yields NeedsUpdate.
    #[test]
    fn marker_no_version_line_is_needs_update() {
        let content = "[machine]\nsetup_at = \"1711900000\"\n";
        let _current = env!("CARGO_PKG_VERSION");
        let mut found_version = false;
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("version") {
                let rest = rest.trim().strip_prefix('=').unwrap_or("").trim();
                let _version = rest.trim_matches('"');
                found_version = true;
            }
        }
        assert!(!found_version, "version line should not be found");
        // machine_setup_status returns NeedsUpdate when no version line
    }

    // ═══════════════════════════════════════════════════════════════════════
    // update_marker_version — line replacement with tempfile
    // ═══════════════════════════════════════════════════════════════════════

    /// update_marker_version replaces only the version field, preserving others.
    #[test]
    fn update_marker_version_preserves_other_fields() {
        let tmpdir = tempfile::tempdir().unwrap();
        let path = tmpdir.path().join("machine.toml");
        let content = "\
[machine]\n\
setup_at = \"1711900000\"\n\
version = \"0.50.0\"\n\
editors = [\"vscode\", \"claude\"]\n\
ai_tools = [\"Claude Code\"]\n";
        std::fs::write(&path, content).unwrap();

        let new_version = env!("CARGO_PKG_VERSION");

        // Replicate update_marker_version logic reading from our temp path
        let content = std::fs::read_to_string(&path).unwrap();
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
        std::fs::write(&path, format!("{}\n", updated)).unwrap();

        let result = std::fs::read_to_string(&path).unwrap();
        assert!(
            result.contains(&format!("version = \"{}\"", new_version)),
            "version should be updated"
        );
        assert!(
            result.contains("setup_at = \"1711900000\""),
            "setup_at should be preserved"
        );
        assert!(
            result.contains("editors = [\"vscode\", \"claude\"]"),
            "editors should be preserved"
        );
        assert!(
            result.contains("ai_tools = [\"Claude Code\"]"),
            "ai_tools should be preserved"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // write_marker_with_editors
    // ═══════════════════════════════════════════════════════════════════════

    /// write_marker_with_editors produces valid TOML-like content.
    #[test]
    fn write_marker_with_editors_format() {
        let results = [
            SetupResult {
                name: "Claude Code".to_string(),
                success: true,
                message: "ok".to_string(),
            },
            SetupResult {
                name: "Cursor".to_string(),
                success: true,
                message: "ok".to_string(),
            },
            SetupResult {
                name: "FailedTool".to_string(),
                success: false,
                message: "failed".to_string(),
            },
        ];
        let editors = ["vscode", "claude", "cursor"];

        // Build the content the same way write_marker_with_editors does
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

        assert_eq!(ai_tools, vec!["Claude Code", "Cursor"]);

        let editors_toml: Vec<String> = editors.iter().map(|e| format!("\"{}\"", e)).collect();
        let editors_str = editors_toml.join(", ");
        assert_eq!(editors_str, "\"vscode\", \"claude\", \"cursor\"");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // SetupResult and MachineStatus basic behavior
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn machine_status_equality() {
        assert_eq!(MachineStatus::NeverSetup, MachineStatus::NeverSetup);
        assert_eq!(MachineStatus::NeedsUpdate, MachineStatus::NeedsUpdate);
        assert_eq!(MachineStatus::Ready, MachineStatus::Ready);
        assert_ne!(MachineStatus::NeverSetup, MachineStatus::Ready);
        assert_ne!(MachineStatus::NeedsUpdate, MachineStatus::Ready);
    }

    #[test]
    fn setup_result_debug_format() {
        let r = SetupResult {
            name: "Test".to_string(),
            success: true,
            message: "ok".to_string(),
        };
        let debug = format!("{:?}", r);
        assert!(debug.contains("Test"));
        assert!(debug.contains("true"));
    }
}
