// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The default `nika --help` postcard plus the `--help --all` classifier
//! (RAMS-13) and the HOME-isolation warning (B08). Split from `main.rs`
//! under the 1500-line file law.

use std::ffi::OsStr;
use std::path::Path;

/// Human default help · B67 postcard, now naming the two first-run doors
/// (`try` · `new`), glossing `permits`, and documenting isolation.
#[must_use]
pub fn human_help() -> &'static str {
    "nika             a plan from a file\n\
     nika try         rehearsal · to own the file: nika new <slug>\n\
     nika new hello   one file that runs on this machine\n\
     nika run         run a file\n\
     nika check       audit a file before it runs\n\
     nika doctor      PATH, model, sandbox\n\
     in the file · permits    what this file is allowed to touch\n\
     to isolate · env -i HOME=$scratch PATH=\"$PATH\" nika …\n"
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
            help.lines().filter(|l| !l.is_empty()).count() <= 8,
            "postcard stays a card, got:\n{help}"
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
            help.contains("to isolate"),
            "isolation is a note, not a verb: {help}"
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
