// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The exec-floor mirror (P0-13 check-side · audit UX 2026-07-30) —
//! **advisory**: an `exec-floor` HINT when an argv-form `exec:` command
//! WILL be refused at run time by the interpreter inline-eval floor.
//!
//! The asymmetry the finding names: `nika check` stayed green on
//! `exec: { command: ["node", "-p", "1+1"] }` and the run then refused
//! it at the exec floor — the author learned the law AFTER the green.
//! The check now predicts the refusal: same per-interpreter eval table,
//! same positional argv parse, advisory voice (a hint names the law
//! without changing a verdict — the `retry-effects` precedent).
//!
//! SCOPE (the honest mirror, never a lie in either direction):
//!
//! - **argv form only** — the eval floor judges the argv form
//!   structurally (`check_argv`); the shell form rides the joined-string
//!   blocklist, which does NOT refuse `node -p` today, so a shell-form
//!   hint would predict a refusal the runtime does not apply. Silence.
//! - **literal argv only** — a `${{ }}` island in the program or any
//!   argument makes the positional parse unreliable: no static claim.
//! - **plain lowercase basename** — the runtime also NFKC-folds
//!   confusable program names (fullwidth `ｎｏｄｅ`); the check does not
//!   (no normalization dep in this crate). That direction UNDER-claims
//!   (a hint missed, never a hint lied) — the deliberate side.
//!
//! keep in sync with nika-exec-runner blocklist.rs — the `EvalSpec`
//! table + the positional scan below are a MINIMAL duplication of the
//! runtime floor (nika-check is L0 pure/sync and cannot depend on the
//! L1 tokio effect crate; the layering forbids the reuse). The
//! coherence test at the bottom ratchets the two tables against each
//! other.

use nika_schema::raw::{RawAction, RawCommand, RawWorkflow};

use super::hints::Hint;

/// The hint kind (agents route on it · `hints.rs` kind registry).
const KIND: &str = "exec-floor";

/// One interpreter family's inline-eval signature — the MINIMAL mirror
/// of `nika-exec-runner/src/blocklist.rs::EvalSpec` (P0-13).
/// keep in sync with nika-exec-runner blocklist.rs.
#[derive(Clone, Copy)]
struct EvalSpec {
    /// Short-option LETTERS: any single-dash bundle containing one
    /// re-parses code (`perl -pe` · `node -pe` · `sh -ce`).
    short_letters: &'static [char],
    /// Long eval flags, EXACT — only where the interpreter really has
    /// them.
    long_flags: &'static [&'static str],
    /// Eval SUBCOMMANDS (`deno eval <code>`) — code execution with no
    /// flag.
    eval_subcommands: &'static [&'static str],
    /// Flags that CONSUME the next argv element as their operand —
    /// skipped, so the scan resumes after them.
    value_flags: &'static [&'static str],
    /// The module handoff (python `-m`): everything after `-m <module>`
    /// is the MODULE's argv — the interpreter's scan stops there.
    module_flag: Option<&'static str>,
}

/// The per-interpreter eval table — `None` for a program with no
/// inline-eval form.
/// keep in sync with nika-exec-runner `blocklist.rs::eval_spec`.
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

/// Whether an interpreter's argv requests INLINE code evaluation —
/// POSITIONAL over the discrete argv elements, per the interpreter's
/// [`EvalSpec`].
/// keep in sync with nika-exec-runner `blocklist.rs::interpreter_eval_requested`.
fn interpreter_eval_requested(base: &str, args: &[&str]) -> bool {
    let Some(spec) = eval_spec(base) else {
        return false;
    };
    let mut i = 0;
    while i < args.len() {
        let arg = args[i];
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

/// Scan every argv-form `exec:` task (and `on_finally` cleanups) for a
/// command the runtime's eval floor WILL refuse — one hint per task.
pub(super) fn scan(wf: &RawWorkflow) -> Vec<Hint> {
    let mut hints = Vec::new();
    for task in &wf.tasks {
        let id = task.value.id.value.as_str();
        push_exec_floor(&task.value.action, id, &mut hints);
        for cleanup in &task.value.on_finally {
            push_exec_floor(&cleanup.value.action, id, &mut hints);
        }
    }
    hints
}

fn push_exec_floor(action: &RawAction, id: &str, hints: &mut Vec<Hint>) {
    let RawAction::Exec(exec) = action else {
        return;
    };
    let RawCommand::Argv(parts) = &exec.command else {
        return; // the shell form rides the joined-string blocklist — no eval-floor claim
    };
    let mut argv = parts.iter().map(|p| p.value.as_str());
    let Some(program) = argv.next() else {
        return;
    };
    // A `${{ }}` island anywhere makes the positional parse unreliable —
    // no static claim (the native-first precedent: a templated head
    // makes no claim).
    if program.contains("${{") || argv.clone().any(|a| a.contains("${{")) {
        return;
    }
    let base = program
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(program)
        .to_lowercase();
    let arg_refs: Vec<&str> = argv.collect();
    if interpreter_eval_requested(&base, &arg_refs) {
        hints.push(Hint {
            kind: KIND,
            task: id.to_owned(),
            // N-6 (G-24 · Lucie): the old advice also taught « route the
            // genuine need via `pre_validated` » — a field NO author can
            // write. It is the kernel's internal wire flag (nika-verb-exec:
            // « `pre_validated` is NEVER set — the runner's blocklist stays
            // the floor »), absent from the workflow schema; a user who
            // greps for it finds nothing. A hint may only teach a route
            // that exists: the script file IS the route.
            advice: format!(
                "`{base}` with an inline-eval flag or subcommand is REFUSED at run time by \
                 the exec floor (the runtime parses the argv positionally, per interpreter — \
                 this command never starts): run a script file instead (P0-13 · the check \
                 predicts the refusal the run would apply)"
            ),
        });
    }
}

#[cfg(test)]
mod tests {
    use crate::check;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn report(yaml: &str) -> crate::CheckReport {
        check(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses"))
    }

    fn exec_wf(command_yaml: &str) -> String {
        format!(
            "nika: v1\nworkflow:\n  id: w\npermits:\n  exec: true\ntasks:\n  t:\n    exec: {{ command: {command_yaml} }}\n"
        )
    }

    fn floor_hints(yaml: &str) -> Vec<crate::hints::Hint> {
        report(yaml)
            .hints
            .into_iter()
            .filter(|h| h.kind == "exec-floor")
            .collect()
    }

    /// THE finding: `["node", "-p", "1+1"]` checked green, then the run
    /// refused it at the eval floor. The check now predicts the refusal.
    #[test]
    fn node_p_argv_earns_the_exec_floor_hint() {
        let hints = floor_hints(&exec_wf("[\"node\", \"-p\", \"1+1\"]"));
        assert_eq!(hints.len(), 1, "the refusal is predicted: {hints:?}");
        assert_eq!(hints[0].task, "t");
        assert!(hints[0].advice.contains("node"), "{}", hints[0].advice);
        assert!(
            hints[0].advice.contains("script file"),
            "the real route is taught: {}",
            hints[0].advice
        );
        // N-6 · the phantom-route pin: `pre_validated` is the kernel's
        // internal wire flag, never authorable — a hint that names it
        // sends the user grepping for a field that does not exist.
        assert!(
            !hints[0].advice.contains("pre_validated"),
            "a hint may only teach a field an author can write: {}",
            hints[0].advice
        );
    }

    /// The finding's false-positive fixture stays silent: after
    /// `-m <module>` the flags are the MODULE's — `unittest -p` is a
    /// pattern flag, not a print-eval.
    #[test]
    fn python_m_unittest_pattern_stays_silent() {
        let hints = floor_hints(&exec_wf(
            "[\"python3\", \"-m\", \"unittest\", \"discover\", \"tests\", \"-p\", \"test_*.py\"]",
        ));
        assert!(
            hints.is_empty(),
            "the module handoff cuts the scan: {hints:?}"
        );
    }

    /// The per-interpreter map: node `-c` is a syntax CHECK (allowed),
    /// deno `eval` is a SUBCOMMAND the flag scan never sees (refused).
    #[test]
    fn the_per_interpreter_table_is_mirrored() {
        assert!(
            floor_hints(&exec_wf("[\"node\", \"-c\", \"server.js\"]")).is_empty(),
            "node -c checks syntax, no code runs"
        );
        assert!(floor_hints(&exec_wf("[\"node\", \"--check\", \"server.js\"]")).is_empty());
        assert_eq!(
            floor_hints(&exec_wf("[\"deno\", \"eval\", \"Deno.exit(1)\"]")).len(),
            1,
            "the eval subcommand refuses"
        );
        assert_eq!(
            floor_hints(&exec_wf("[\"node\", \"--print\", \"1+1\"]")).len(),
            1
        );
        assert_eq!(
            floor_hints(&exec_wf("[\"node\", \"-pe\", \"x\"]")).len(),
            1,
            "a bundled short flag containing an eval letter refuses"
        );
        assert_eq!(
            floor_hints(&exec_wf("[\"php\", \"-r\", \"system();\"]")).len(),
            1
        );
        assert!(floor_hints(&exec_wf("[\"ruby\", \"-c\", \"app.rb\"]")).is_empty());
    }

    /// Interpreters on SCRIPT FILES stay silent — the eval floor only
    /// judges the interpreter's own argv, before the script positional.
    #[test]
    fn script_files_and_value_flags_stay_silent() {
        for command in [
            "[\"python3\", \"app.py\", \"-p\", \"8080\"]",
            "[\"node\", \"server.js\"]",
            "[\"python3\", \"-m\", \"pytest\"]",
            "[\"bash\", \"deploy.sh\", \"prod\"]",
            "[\"make\", \"build\"]",
        ] {
            let hints = floor_hints(&exec_wf(command));
            assert!(hints.is_empty(), "{command} must stay silent: {hints:?}");
        }
        // …and a value-flag operand does not hide the eval flag after it.
        assert_eq!(
            floor_hints(&exec_wf(
                "[\"python3\", \"-X\", \"faulthandler\", \"-c\", \"x\"]"
            ))
            .len(),
            1
        );
    }

    /// A templated program or argument makes NO claim — the runtime
    /// judges the interpolated argv, the check cannot.
    #[test]
    fn templated_argv_makes_no_claim() {
        for command in [
            "[\"${{ vars.tool }}\", \"-p\", \"1+1\"]",
            "[\"node\", \"-p\", \"${{ vars.code }}\"]",
            "[\"node\", \"${{ vars.flag }}\", \"1+1\"]",
        ] {
            let hints = floor_hints(&exec_wf(command));
            assert!(
                hints.is_empty(),
                "{command} makes no static claim: {hints:?}"
            );
        }
    }

    /// The SHELL form stays silent: the runtime's shell-mode floor (the
    /// joined-string blocklist) does NOT refuse `node -p` today, so a
    /// shell-form hint would predict a refusal the run does not apply.
    #[test]
    fn the_shell_form_makes_no_eval_floor_claim() {
        let r = report(
            "nika: v1\nworkflow:\n  id: w\npermits:\n  exec: true\ntasks:\n  t:\n    exec: { shell: \"node -p 1+1\" }\n",
        );
        assert!(
            !r.hints.iter().any(|h| h.kind == "exec-floor"),
            "no lie in the refusal direction: {:?}",
            r.hints
        );
    }

    /// The program's PATH prefix does not hide the interpreter (the
    /// runtime basenames `command[0]` — the check predicts the same).
    #[test]
    fn a_pathed_interpreter_is_still_judged() {
        let hints = floor_hints(&exec_wf("[\"/usr/bin/python3\", \"-c\", \"import os\"]"));
        assert_eq!(hints.len(), 1, "the basename is judged: {hints:?}");
    }

    /// `on_finally` cleanups are scanned too (the native-first
    /// precedent — a cleanup command is the same floor).
    #[test]
    fn on_finally_cleanups_are_scanned_too() {
        let r = report(
            "nika: v1\nworkflow:\n  id: w\npermits:\n  exec: true\ntasks:\n  t:\n    exec: { command: [\"make\", \"build\"] }\n    on_finally:\n      - exec: { command: [\"node\", \"-e\", \"process.exit(0)\"] }\n",
        );
        assert!(
            r.hints
                .iter()
                .any(|h| h.kind == "exec-floor" && h.task == "t"),
            "the cleanup's refusal is predicted: {:?}",
            r.hints
        );
    }

    /// COHERENCE RATCHET — every family + flag-table line this file's
    /// classification uses must exist BYTE-IDENTICAL in the runtime
    /// floor source (same workspace · the `CARGO_MANIFEST_DIR` precedent
    /// of nika-schema/skill.rs); skips when the sibling crate is not
    /// checked out (a packaged source tree). Textual and deterministic:
    /// a drift in blocklist.rs's `match base` arms or `EvalSpec` tables
    /// fails here until the two are re-synced. (Only the runtime side
    /// is asserted — asserting against THIS file's own source would be
    /// self-referential, the test text contains the same strings.)
    #[test]
    fn the_eval_table_matches_the_runtime_floor() {
        let Ok(runtime_src) = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../nika-exec-runner/src/blocklist.rs"),
        ) else {
            return; // sibling crate not checked out — no claim
        };
        for family in [
            "\"sh\" | \"bash\" | \"zsh\" | \"dash\" | \"ksh\" | \"csh\" | \"tcsh\"",
            "\"python\" | \"python2\" | \"python3\"",
            "\"perl\" | \"ruby\"",
            "\"node\" => Some(NODE)",
            "\"deno\" | \"bun\"",
            "\"php\" => Some(PHP)",
        ] {
            assert!(
                runtime_src.contains(family),
                "the runtime floor dropped the family {family} — re-sync exec_floor.rs"
            );
        }
        // The flag tables ride the same ratchet: each interpreter's
        // short-eval letters and long flags are byte-identical lines.
        for table_line in [
            "short_letters: &['c'],",
            "short_letters: &['e', 'E'],",
            "short_letters: &['e', 'p'],",
            "short_letters: &['r'],",
            "long_flags: &[\"--eval\", \"--print\"],",
            "eval_subcommands: &[\"eval\"],",
            "module_flag: Some(\"-m\"),",
        ] {
            assert!(
                runtime_src.contains(table_line),
                "the table line {table_line:?} drifted — re-sync exec_floor.rs with blocklist.rs"
            );
        }
    }
}
