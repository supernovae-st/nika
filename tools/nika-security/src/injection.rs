// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shell injection detection — structural blocklist and data injection checks.

use crate::normalize::{contains_unquoted, normalize_for_blocklist};
use crate::SecurityError;

/// Shell-mode patterns that are ALWAYS blocked (structural commands).
///
/// These patterns are dangerous regardless of origin — a workflow author
/// should never need alias/function definitions inside an exec command.
const SHELL_MODE_BLOCKLIST: &[&str] = &[
    "alias ",
    "function ",
    "declare -f",
];

/// Shell metacharacter patterns that are only dangerous when INJECTED via data.
///
/// When the dev writes `$()` in their YAML template, it's intentional.
/// When `$()` appears because runtime data (via `{{with.xxx}}`) contains it,
/// it's a potential injection attack. These patterns are checked by
/// `check_shell_data_injection()` which compares raw template vs resolved command.
const SHELL_INJECTION_PATTERNS: &[&str] = &[
    "$(", "`", "<(", "<<<",
    "&&", "||", ";", ">>", ">", "|",
];

/// Check command against shell-mode-specific blocklist.
///
/// These patterns (command substitution, backticks) are only dangerous
/// when shell mode is active (`shell: true`). In shell-free mode,
/// they are harmless literal characters.
///
/// The backtick pattern uses quote-aware matching so that backticks
/// inside quoted strings (e.g. `echo "file\`name.txt"`) are allowed,
/// while unquoted backtick substitution (e.g. `echo \`whoami\``) is blocked.
///
/// # Errors
///
/// Returns `BlockedCommand` if a shell-mode blocklisted pattern is found.
pub fn check_shell_mode_blocklist(cmd: &str) -> Result<(), SecurityError> {
    let normalized = normalize_for_blocklist(cmd);
    let lower = normalized.to_lowercase();

    for pattern in SHELL_MODE_BLOCKLIST {
        let matched = if *pattern == "`" {
            contains_unquoted(&lower, pattern)
        } else {
            lower.contains(pattern)
        };

        if matched {
            tracing::warn!(
                command = %nika_core::util::redact_secrets(cmd),
                pattern = %pattern,
                "NIKA-053: Blocked dangerous shell-mode pattern"
            );
            return Err(SecurityError::BlockedCommand {
                command: nika_core::util::redact_secrets(cmd),
                reason: format!("Shell-mode blocklisted pattern: {pattern}"),
            });
        }
    }
    Ok(())
}

/// Check for shell metacharacter injection via template data.
///
/// Compares the raw YAML template against the resolved command to detect
/// shell metacharacters (`$()`, backticks, `<(`, `<<<`) that were INJECTED
/// by runtime data — not written by the workflow author.
///
/// If a pattern exists in both raw template and resolved command, it's
/// intentional (dev wrote it). If it appears ONLY in the resolved command,
/// it came from task output data and is a potential injection.
///
/// # Errors
///
/// Returns `BlockedCommand` if an injected shell metacharacter is detected.
pub fn check_shell_data_injection(raw_template: &str, resolved_cmd: &str) -> Result<(), SecurityError> {
    let resolved_normalized = normalize_for_blocklist(resolved_cmd);
    let resolved_lower = resolved_normalized.to_lowercase();
    let raw_normalized = normalize_for_blocklist(raw_template);
    let raw_lower = raw_normalized.to_lowercase();

    for pattern in SHELL_INJECTION_PATTERNS {
        let in_resolved = if *pattern == "`" {
            contains_unquoted(&resolved_lower, pattern)
        } else {
            resolved_lower.contains(pattern)
        };

        if !in_resolved {
            continue;
        }

        let in_raw = if *pattern == "`" {
            contains_unquoted(&raw_lower, pattern)
        } else {
            raw_lower.contains(pattern)
        };

        if !in_raw {
            tracing::warn!(
                resolved = %nika_core::util::redact_secrets(resolved_cmd),
                pattern = %pattern,
                "NIKA-053: Blocked injected shell metacharacter from template data"
            );
            return Err(SecurityError::BlockedCommand {
                command: nika_core::util::redact_secrets(resolved_cmd),
                reason: format!(
                    "Shell metacharacter '{}' injected via template data — \
                     use |shell transform to escape dynamic values",
                    pattern
                ),
            });
        }
    }
    Ok(())
}
