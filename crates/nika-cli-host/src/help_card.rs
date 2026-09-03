// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The default `nika --help` postcard plus the `--help --all` classifier
//! (RAMS-13) and the HOME-isolation warning (B08). Split from `main.rs`
//! under the 1500-line file law.

use std::ffi::OsStr;
use std::path::Path;

/// Clap `--help --all` footer · issue 1274: the only non-mock path.
pub const AFTER_HELP: &str = "a REAL answer needs exactly one of { an API key · a signed-in harness seat (`--access`) · the Gear One pull (`nika model pull`) } — everything else is a mock rehearsal (the run says so out loud) · nika --help --all  the rest of the surface";

/// `nika check --help` footer (#1404): the base exit contract, so CI
/// can branch on the code alone. The house taxonomy (spec §4 · LOCKED):
/// 0 the report holds · 1 a workflow ran and failed (`run` only) · 2 the
/// FILE · 3 the ENVIRONMENT · 4 paused on a human gate (`run` only).
pub const CHECK_EXITS: &str = "exit codes · 0 the report holds (clean) · 2 the FILE: a grammar refusal or findings (`--json` carries `kind` to tell them apart) · or a mistyped flag (the parser's own usage error) · 3 the ENVIRONMENT: the file is missing or unreadable, a registry is unreachable · never 1 or 4 (those are `run`'s: a failed workflow · a paused gate) · `--profile operational` folds risk ≥ High and an unready access lane into 2 · the layers line: VALID (grammar · DAG · permits · types · the resolver knows every model) · ACCESS READY (a path on this machine · presence, never a dial) · CAPACITY FIT (the seat's limits vs the declaration) · RUN READY (the three, and no known blocker) · `--json` gate keys: `clean` · `verdicts.{valid,access_ready,capacity_fit,run_ready,blockers}` · `model_findings[]` · `access_plan[]` · `risk_grade` · `judged.{composition,skills}`";

/// `nika run --help` footer (One Door · wave 2b · the W1 gauntlet found
/// `run`'s codes documented only inside `check`'s help): the run's
/// exit ladder, so CI can branch on the code alone.
pub const RUN_EXITS: &str = "exit codes · 0 the run settled (or `--dry-run` previewed) · 1 the WORKFLOW: a task failed and the run settled failed · 2 the FILE: findings (the same audit `check` prints) or a cost floor above `--max-cost-usd` (NIKA-1709) · 3 the ENVIRONMENT: no ready access path (NIKA-1800), an unsatisfied `--access` (NIKA-1801 · NIKA-1803), a resume that would switch access (NIKA-1807), an unreadable file · 4 PAUSED on a human gate (resume with `--resume <trace> --answer <task>=<value>`)";

/// `nika test --help` footer (#1404): the golden test's exit ladder.
pub const TEST_EXITS: &str = "exit codes · 0 the golden matches · 1 the mock run failed or the outputs drifted from the golden · 2 the FILE has findings (`check` dirty) · 3 no golden yet (`--update` writes one), the file is missing, or `--var` without `--case` · a rule table: `--case <name>` pins `<file>.<name>.golden.json`, `--var KEY=VALUE` binds the case";

/// Human default help · B67 postcard, now naming the two first-run doors
/// (`try` · `new`), glossing `permits`, and documenting isolation.
#[must_use]
pub fn human_help() -> &'static str {
    "nika             a plan from a file\n\
     nika try         rehearsal · to own the file: nika new <slug>\n\
     nika new hello   one file that runs on this machine\n\
     nika run         run a file\n\
     nika check       audit · in the file, permits = what this file is allowed to touch\n\
     nika doctor      PATH, model, sandbox · isolate with env -i HOME=$scratch PATH=\"$PATH\" nika …\n"
}

/// Teaching line when someone types `nika permits` as if it were a verb.
#[must_use]
pub fn permits_teaching() -> &'static str {
    "nika: `permits` is not a command — it lives in the file. \
     Try `nika explain FILE` or `nika check --infer-permits FILE`.\n"
}

/// Teaching line when `--fix` is typed on the root (`nika --fix`) instead of
/// the check door.
#[must_use]
pub fn misplaced_fix_teaching() -> &'static str {
    "nika: `--fix` is not a root flag — rewrite a file with `nika check --fix FILE`.\n"
}

/// How a help-only argv should render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpKind {
    /// The postcard (`nika --help` · `nika --help --plain`).
    Short,
    /// The whole clap tree (`nika --help --all` · plus globals).
    All,
}

/// Classify a top-level help invocation BEFORE clap parses.
///
/// `--all` is not a clap flag on the root; combining it with a global
/// (`--plain`) used to drop the `--all` branch and clap-fail. Globals
/// that never name a verb stay compatible so `--help --all --plain`
/// keeps `--all` (B07 · I02).
#[must_use]
pub fn classify_help(argv: &[impl AsRef<OsStr>]) -> Option<HelpKind> {
    if argv.is_empty() {
        return None;
    }
    let mut skip_value = false;
    let mut saw_help = false;
    let mut saw_all = false;
    for arg in argv {
        if skip_value {
            skip_value = false;
            continue;
        }
        let s = arg.as_ref();
        if s == "--color" || s == "--hyperlink" {
            skip_value = true;
            continue;
        }
        let text = s.to_str()?;
        if text.starts_with("--color=") || text.starts_with("--hyperlink=") {
            continue;
        }
        if text == "--help" || text == "-h" || text == "help" {
            saw_help = true;
            continue;
        }
        if text == "--all" {
            saw_all = true;
            continue;
        }
        if text == "--plain" || text == "--ascii" {
            continue;
        }
        return None;
    }
    if !saw_help {
        return None;
    }
    Some(if saw_all {
        HelpKind::All
    } else {
        HelpKind::Short
    })
}

/// `HOME=$scratch` without `env -i` still walks the operator's real
/// home on macOS (`dirs` / Directory Services ignore `$HOME`). Warn
/// when HOME looks like a scratch isolation attempt.
#[must_use]
pub fn isolation_warning() -> Option<String> {
    let home = env_home()?;
    home_looks_like_scratch(&home).then(|| isolation_warning_text(&home))
}

/// Whether `$HOME` looks like a scratch isolation directory.
#[must_use]
pub fn home_looks_like_scratch(home: &str) -> bool {
    let path = Path::new(home);
    path.starts_with(std::env::temp_dir())
        || home.contains("/tmp/")
        || home.contains("/var/folders/")
        || home.contains("/scratch")
        || home.contains("scratch-")
}

/// The isolation warning for a scratch `$HOME` that still inherited the env.
#[must_use]
pub fn isolation_warning_text(home: &str) -> String {
    format!(
        "nika: HOME={home} looks like a scratch, but this process still inherited \
         the rest of the environment. Isolation needs `env -i HOME={home} \
         PATH=\"$PATH\" nika …` — HOME alone does not move ~/.nika on macOS."
    )
}

#[allow(clippy::disallowed_methods)] // presentation HOME, not a secret
fn env_home() -> Option<String> {
    std::env::var("HOME").ok().filter(|h| !h.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn after_help_names_the_one_free_path() {
        assert!(
            AFTER_HELP.contains("REAL answer") && AFTER_HELP.contains("mock rehearsal"),
            "{AFTER_HELP}"
        );
    }

    #[test]
    fn the_postcard_names_try_new_permits_and_isolation() {
        let help = human_help();
        assert!(help.contains("try"), "C11: {help}");
        assert!(help.contains("new"), "C11: {help}");
        assert!(
            help.contains("what this file is allowed to touch"),
            "UX-2: {help}"
        );
        assert!(
            help.contains("env -i") && help.contains("HOME=$scratch"),
            "B08: {help}"
        );
        assert!(
            help.lines().filter(|l| !l.is_empty()).count() <= 6,
            "postcard stays a six-line card, got:\n{help}"
        );
    }

    #[test]
    fn default_help_does_not_invite_nika_permits_as_a_command() {
        let help = human_help();
        assert!(
            !help.lines().any(|l| l.starts_with("     permits")),
            "permits must not sit in the verb column: {help}"
        );
        assert!(
            !help
                .lines()
                .any(|l| l.trim_start().starts_with("nika permits")),
            "must not teach `nika permits` as a command: {help}"
        );
        assert!(
            help.contains("in the file") && help.contains("permits"),
            "permits is a field in the file: {help}"
        );
        assert!(
            help.contains("isolate") && help.contains("env -i"),
            "isolation is a note on doctor, not a verb: {help}"
        );
    }

    #[test]
    fn permits_teaching_names_the_file_and_the_doors() {
        let text = permits_teaching();
        assert!(text.contains("not a command"), "{text}");
        assert!(text.contains("lives in the file"), "{text}");
        assert!(text.contains("nika explain FILE"), "{text}");
        assert!(text.contains("nika check --infer-permits FILE"), "{text}");
        assert!(!text.contains("nika permits"), "{text}");
    }

    #[test]
    fn misplaced_fix_teaching_names_check_fix() {
        let text = misplaced_fix_teaching();
        assert!(text.contains("nika check --fix"), "{text}");
        assert!(!text.contains("nika --fix FILE"), "{text}");
    }

    #[test]
    fn help_all_plain_keeps_all() {
        assert_eq!(
            classify_help(&["--help", "--all", "--plain"]),
            Some(HelpKind::All),
            "B07+I02: --plain must not drop --all"
        );
        assert_eq!(
            classify_help(&["--plain", "--help", "--all"]),
            Some(HelpKind::All)
        );
        assert_eq!(
            classify_help(&["--help", "--all", "--ascii"]),
            Some(HelpKind::All)
        );
        assert_eq!(classify_help(&["--help", "--plain"]), Some(HelpKind::Short));
        assert_eq!(classify_help(&["--help"]), Some(HelpKind::Short));
        assert_eq!(
            classify_help(&["--all"]),
            None,
            "bare --all is not a help invocation"
        );
        assert_eq!(
            classify_help(&["try", "--all", "--help"]),
            None,
            "a named verb keeps its own help (RAMS-13)"
        );
    }

    #[test]
    fn scratch_home_is_warned() {
        assert!(home_looks_like_scratch("/tmp/nika-scratch-home"));
        assert!(home_looks_like_scratch("/var/folders/xx/scratch"));
        assert!(!home_looks_like_scratch("/Users/thibaut"));
        assert!(!home_looks_like_scratch("/home/nika"));
        let text = isolation_warning_text("/tmp/scratch");
        assert!(text.contains("env -i"), "{text}");
        assert!(text.contains("HOME=/tmp/scratch"), "{text}");
    }
}
