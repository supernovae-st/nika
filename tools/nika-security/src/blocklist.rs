// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Command blocklist — dangerous pattern detection with NFKC normalization.

use crate::normalize::{normalize_first_token_basename, normalize_for_blocklist};
use crate::SecurityError;

/// Blocklist of dangerous command patterns (case-insensitive)
///
/// These patterns are checked against the lowercase command string.
/// Any match results in a BlockedCommand error.
pub(crate) const BLOCKLIST: &[&str] = &[
    // Destructive file operations
    "rm -rf /",
    "rm -rf /*",
    "rm -rf ~",
    // Remote code execution (piping downloads to shell)
    "| bash",
    "|bash",
    "| sh",
    "|sh",
    // Shell injection via dynamic execution
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
    // Unix shell -c (arbitrary command execution)
    "bash -c",
    "sh -c",
    "zsh -c",
    "dash -c",
    "ksh -c",
    "csh -c",
    "tcsh -c",
    // Python/interpreter -c (arbitrary code execution — generic, not import-specific)
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
    // Destructive rm long-flag variants
    "rm --recursive",
    "rm --force",
    // Interpreter bypass (arbitrary code execution via scripting runtimes)
    "perl -e",
    "ruby -e",
    "node -e",
    // Piping to interpreter stdin (bypasses -e flag checks)
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
    // Command wrapper bypass (env can prefix any blocked command)
    "env ",
    // Shell prefix commands — bypass functions/aliases or wrap arbitrary commands
    "command ",
    "builtin ",
    "nohup ",
    "nice ",
    "timeout ",
    "strace ",
    // Shell builtins for script/coprocess execution
    "source ", // Executes script in current shell context
    "coproc ", // Background coprocess with bidirectional pipes
    // Bash virtual files for reverse shells (no nc needed)
    "/dev/tcp/",
    "/dev/udp/",
    // Privilege escalation (su; sudo already covered above)
    "su ",
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
];

/// Interpreter patterns that are safe when written by the developer in YAML,
/// but dangerous when injected via template data.
///
/// These are checked with the raw-vs-resolved pattern: if the pattern appears
/// in the raw YAML template, it's developer intent and allowed. If it only
/// appears after template resolution, it was injected and is blocked.
const INTENT_PATTERNS: &[&str] = &[
    "python -c",
    "python2 -c",
    "python3 -c",
    "perl -e",
    "ruby -e",
    "node -e",
];

/// Check command against blocklist
///
/// Performs case-insensitive matching against the blocklist.
/// Applies NFKC normalization to both the command and patterns
/// to prevent Unicode confusable bypass attacks.
///
/// # Security
///
/// NFKC normalization ensures that:
/// - `ｒｍ -rf /` (fullwidth) is blocked like `rm -rf /`
/// - `𝘀𝘂𝗱𝗼 rm` (math bold) is blocked like `sudo rm`
/// - Commands with combining characters are properly detected
///
/// # Errors
///
/// Returns `BlockedCommand` if a blocklisted pattern is found.
pub fn check_blocklist(cmd: &str) -> Result<(), SecurityError> {
    // SEC-1: Scan the FULL command, not just first 4KB.
    let normalized = normalize_for_blocklist(cmd);
    let lower = normalized.to_lowercase();

    // SEC-3: Strip shell quoting characters before pattern matching.
    let dequoted: String = lower
        .chars()
        .filter(|c| !matches!(c, '"' | '\'' | '\\'))
        .collect();

    // SEC-2: Also check with the first token basename-resolved.
    let basename_normalized = normalize_first_token_basename(&lower);
    let basename_dequoted = normalize_first_token_basename(&dequoted);

    for pattern in BLOCKLIST {
        let normalized_pattern = pattern.to_lowercase();
        if lower.contains(&normalized_pattern)
            || basename_normalized.contains(normalized_pattern.as_str())
            || dequoted.contains(&normalized_pattern)
            || basename_dequoted.contains(normalized_pattern.as_str())
        {
            tracing::warn!(
                command = %nika_core::util::redact_secrets(cmd),
                pattern = %pattern,
                "NIKA-053: Blocked dangerous command"
            );
            return Err(SecurityError::BlockedCommand {
                command: nika_core::util::redact_secrets(cmd),
                reason: format!("Blocklisted pattern: {pattern}"),
            });
        }
    }
    Ok(())
}

/// Check blocklist with optional developer intent awareness (BUG-032).
///
/// When `raw_template` is provided, interpreter patterns (`python3 -c`, `node -e`, etc.)
/// that appear in the raw YAML template are treated as developer intent and allowed.
/// Destructive patterns (`rm -rf /`, `sudo`, etc.) are ALWAYS blocked.
///
/// When `raw_template` is `None`, behaves identically to `check_blocklist` (all blocked).
pub fn check_blocklist_with_intent(cmd: &str, raw_template: Option<&str>) -> Result<(), SecurityError> {
    let normalized = normalize_for_blocklist(cmd);
    let lower = normalized.to_lowercase();
    let dequoted: String = lower
        .chars()
        .filter(|c| !matches!(c, '"' | '\'' | '\\'))
        .collect();
    let basename_normalized = normalize_first_token_basename(&lower);
    let basename_dequoted = normalize_first_token_basename(&dequoted);

    let candidates: [&str; 4] = [&lower, &basename_normalized, &dequoted, &basename_dequoted];

    for pattern in BLOCKLIST {
        let normalized_pattern = pattern.to_lowercase();
        let matched = candidates.iter().any(|c| c.contains(normalized_pattern.as_str()));
        if !matched {
            continue;
        }

        // Check if this is an intent pattern that may be developer-authored
        let is_intent_pattern = INTENT_PATTERNS
            .iter()
            .any(|ip| normalized_pattern.starts_with(ip));

        if is_intent_pattern {
            if let Some(raw) = raw_template {
                let raw_normalized = normalize_for_blocklist(raw);
                let raw_lower = raw_normalized.to_lowercase();
                let raw_dequoted: String = raw_lower
                    .chars()
                    .filter(|c| !matches!(c, '"' | '\'' | '\\'))
                    .collect();
                let in_raw = raw_lower.contains(&normalized_pattern)
                    || raw_dequoted.contains(&normalized_pattern);
                if in_raw {
                    tracing::debug!(
                        pattern = %pattern,
                        "NIKA-053: Allowing developer-intent pattern found in raw template"
                    );
                    continue;
                }
            }
        }

        tracing::warn!(
            command = %nika_core::util::redact_secrets(cmd),
            pattern = %pattern,
            "NIKA-053: Blocked dangerous command"
        );
        return Err(SecurityError::BlockedCommand {
            command: nika_core::util::redact_secrets(cmd),
            reason: format!("Blocklisted pattern: {pattern}"),
        });
    }
    Ok(())
}
