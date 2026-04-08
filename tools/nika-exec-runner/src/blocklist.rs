// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Command blocklist for shell execution security (NIKA-053 equivalent).
//!
//! Scans the FULL command string — no 4KB limit (SEC-1 fix from v0.66).

use nika_kernel::shell::ShellError;

/// Patterns that are always blocked.
const BLOCKED_PATTERNS: &[&str] = &[
    "rm -rf /",
    "rm -rf /*",
    "rm -rf ~",
    "rm -rf $home",
    ":(){ :|:& };:", // fork bomb
    "sudo rm",
    "sudo dd",
    "mkfs.",
    "dd if=/dev/zero",
    "dd if=/dev/random",
    "> /dev/sda",
    "> /dev/nvme",
    "chmod -R 777 /",
    "chown -R",
];

/// Programs that are always blocked.
const BLOCKED_PROGRAMS: &[&str] = &[
    "shutdown",
    "reboot",
    "halt",
    "poweroff",
    "init",
];

/// Check whether a command is on the blocklist.
///
/// Scans the entire command string (no truncation).
pub fn check_command(command: &str) -> Result<(), ShellError> {
    let normalized = command.to_lowercase();

    for pattern in BLOCKED_PATTERNS {
        if normalized.contains(pattern) {
            return Err(ShellError::Blocked {
                reason: "command matches security blocklist".to_string(),
            });
        }
    }

    // Check if the command starts with a blocked program
    let first_word = normalized.split_whitespace().next().unwrap_or("");
    // Also check after path prefix (e.g. /usr/sbin/shutdown)
    let base_name = first_word.rsplit('/').next().unwrap_or(first_word);

    for program in BLOCKED_PROGRAMS {
        if base_name == *program {
            return Err(ShellError::Blocked {
                reason: format!("blocked program: {}", program),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_rm_rf_root() {
        assert!(check_command("rm -rf /").is_err());
        assert!(check_command("rm -rf /*").is_err());
    }

    #[test]
    fn blocks_rm_rf_home() {
        assert!(check_command("rm -rf ~").is_err());
        assert!(check_command("rm -rf $HOME").is_err());
    }

    #[test]
    fn blocks_fork_bomb() {
        assert!(check_command(":(){ :|:& };:").is_err());
    }

    #[test]
    fn blocks_sudo_destructive() {
        assert!(check_command("sudo rm -rf /tmp").is_err());
        assert!(check_command("sudo dd if=/dev/zero of=/dev/sda").is_err());
    }

    #[test]
    fn blocks_shutdown_programs() {
        assert!(check_command("shutdown -h now").is_err());
        assert!(check_command("reboot").is_err());
        assert!(check_command("/usr/sbin/shutdown").is_err());
    }

    #[test]
    fn allows_safe_commands() {
        assert!(check_command("echo hello").is_ok());
        assert!(check_command("ls -la").is_ok());
        assert!(check_command("cat file.txt").is_ok());
        assert!(check_command("python3 script.py").is_ok());
        assert!(check_command("npm run build").is_ok());
        assert!(check_command("cargo test").is_ok());
    }

    #[test]
    fn allows_rm_with_specific_paths() {
        // rm on specific files should be allowed
        assert!(check_command("rm /tmp/test.txt").is_ok());
        assert!(check_command("rm -f output.log").is_ok());
    }

    #[test]
    fn case_insensitive_blocking() {
        assert!(check_command("RM -RF /").is_err());
        assert!(check_command("Sudo Rm").is_err());
    }

    #[test]
    fn scans_full_command() {
        // The command should be scanned entirely, not truncated
        let padding = "a".repeat(8000);
        let evil = format!("{} && rm -rf /", padding);
        assert!(check_command(&evil).is_err());
    }
}
