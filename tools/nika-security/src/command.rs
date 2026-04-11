// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Command validation — control character checks and full exec validation.

use crate::SecurityError;

/// Validate command string for control characters
///
/// Rejects control characters (0x00-0x1F) except:
/// - `\n` (0x0A) - newline, allowed for multi-line commands
/// - `\t` (0x09) - tab, allowed for indentation
///
/// # Errors
///
/// Returns `BlockedCommand` if a control character is found.
pub fn validate_command_string(cmd: &str) -> Result<(), SecurityError> {
    for (i, c) in cmd.chars().enumerate() {
        let code = c as u32;
        // Reject 0x00-0x1F except \n (0x0A) and \t (0x09)
        if code < 0x20 && code != 0x0A && code != 0x09 {
            return Err(SecurityError::BlockedCommand {
                command: nika_core::util::redact_secrets(cmd),
                reason: format!("Control character 0x{code:02X} at position {i}"),
            });
        }
    }
    Ok(())
}

/// Full security validation for exec commands with explicit shell mode flag.
///
/// Combines control character validation and blocklist checking.
/// When `shell_mode` is true, also checks for shell-specific bypass
/// patterns like command substitution (`$()`, backticks).
///
/// # Errors
///
/// Returns `BlockedCommand` if any security check fails.
pub fn validate_exec_command_with_shell(cmd: &str, shell_mode: bool) -> Result<(), SecurityError> {
    validate_exec_command_full(cmd, shell_mode, None)
}

/// Full security validation with optional raw template for developer intent detection.
///
/// When `raw_template` is provided, interpreter patterns (`python3 -c`, etc.)
/// written by the developer in YAML are allowed through (BUG-032).
pub fn validate_exec_command_full(
    cmd: &str,
    shell_mode: bool,
    raw_template: Option<&str>,
) -> Result<(), SecurityError> {
    validate_command_string(cmd)?;
    crate::check_blocklist_with_intent(cmd, raw_template)?;
    if shell_mode {
        crate::check_shell_mode_blocklist(cmd)?;
    }
    Ok(())
}
