// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The per-verb arms folded out of `main.rs` at the 1500-line file wall:
//! the dispatch seam stays one line per verb, the plumbing lives here.

use clap::CommandFactory as _;
use nika_cli::Theme;
use nika_cli::verbs;

use crate::lazy::{check_lazy, resolve_lazy_target};
use crate::{Cli, emit, help_card, interactive_theme};

/// `nika --help` (#1249): the postcard, then EVERY verb the tree carries
/// (hidden ones too — a door the help never names is never opened), the
/// file-as-command gesture and the deeper doors. Derived from the tree.
pub(crate) fn help_page() -> String {
    use std::fmt::Write as _;
    let card = help_card::human_help();
    let mut page = format!(
        "{card}nika x.nika the file IS the command · the same as `nika run x.nika`\n\nalso ·\n"
    );
    let tree = Cli::command();
    let mut rows: Vec<&clap::Command> = tree
        .get_subcommands()
        .filter(|s| s.get_name() != "help" && !card.contains(&format!("nika {} ", s.get_name())))
        .collect();
    rows.sort_by_key(|s| (s.get_display_order(), s.get_name()));
    for s in rows {
        // The first clause only — `nika <verb> --help` keeps the paragraph.
        let about = s.get_about().map(ToString::to_string).unwrap_or_default();
        let mut short = about.as_str();
        for sep in [" · ", " — ", ". ", " ("] {
            short = short.split_once(sep).map_or(short, |(head, _)| head);
        }
        let _ = writeln!(page, "nika {:<12} {short}", s.get_name());
    }
    page += "\nnika <verb> --help   its flags · nika --help --all   the whole tree with flags\n";
    page
}

/// `nika notes.yaml` · `nika missing.nika` (#1249): a first word that
/// is no verb but looks like a file (on disk, or a program/project name)
/// gets the door named instead of clap's dead end; a typo'd verb keeps
/// clap's own.
pub(crate) fn file_near_miss(first: &std::ffi::OsStr) -> Option<String> {
    let s = first.to_str()?;
    let name = nika_source::path_file_name(s).unwrap_or(s);
    let kind = nika_source::classify_file_name(name);
    let ext = std::path::Path::new(s)
        .extension()
        .map(std::ffi::OsStr::to_ascii_lowercase);
    let yaml = ext.as_deref().is_some_and(|e| e == "yaml" || e == "yml");
    let looks_like_source = matches!(
        kind,
        nika_source::SourceNameKind::CanonicalProgram
            | nika_source::SourceNameKind::RetiredProgram
            | nika_source::SourceNameKind::ProjectFile
    ) || yaml;
    let file = std::path::Path::new(s).is_file();
    if s.starts_with('-')
        || (!looks_like_source && !file)
        || Cli::command().find_subcommand(s).is_some()
    {
        return None;
    }
    if kind == nika_source::SourceNameKind::RetiredProgram {
        let hint = nika_source::retired_rename_hint(name).unwrap_or_else(|| {
            format!("`{s}` uses a retired Nika program suffix; rename to `*.nika`")
        });
        return Some(format!("nika: `{s}` is not a command\n  {hint}"));
    }
    let why = match (file, kind) {
        (false, _) => "\n  no such file here — `nika list` names the workflows below",
        (true, nika_source::SourceNameKind::CanonicalProgram) => "",
        _ => "\n  `*.nika` is the workflow suffix — then the bare name runs it",
    };
    Some(format!(
        "nika: `{s}` is not a command\n  did you mean: nika run {s}{why}"
    ))
}

/// The `test` arm — resolve the lazy target, then run the goldens.
pub(crate) fn test_arm(
    file: Option<String>,
    update: bool,
    answer: &[String],
    (vars, case): (&[String], Option<&str>),
    plain_theme: Theme,
) -> u8 {
    match resolve_lazy_target(file, "test") {
        Ok(file) => verbs::test::run_case(&file, update, answer, (vars, case), plain_theme),
        Err(code) => code,
    }
}

/// The `inspect` arm — the one graph projector behind `--format`.
pub(crate) fn inspect_arm(
    file: &str,
    format: Option<verbs::graph::GraphFormatArg>,
    plain_theme: Theme,
) -> u8 {
    match format {
        Some(f) => emit(&verbs::graph::run(file, f.into(), plain_theme)),
        None => emit(&verbs::inspect::run(file, plain_theme)),
    }
}

/// The check arm's plumbing — folded out of the dispatch so the seam
/// stays one line per verb.
pub(crate) fn check_arm(args: verbs::check::CheckArgs, plain_theme: Theme) -> u8 {
    if args.sdk_snapshot {
        let output = match args.files.as_slice() {
            [file]
                if args.json
                    && !args.fix
                    && !args.infer_permits
                    && !args.native_strict
                    && args.profile == verbs::check::Profile::Advisory
                    && args.model.is_none()
                    && args.access.is_none() =>
            {
                verbs::check::run_snapshot_export(file, interactive_theme(plain_theme))
            }
            _ => verbs::VerbOutput::env(
                "check: --sdk-snapshot requires exactly one file and --json, with no other check overrides\n".to_owned(),
            ),
        };
        return emit(&output);
    }
    let flags = verbs::check::CheckFlags {
        json: args.json,
        infer_permits: args.infer_permits,
        native_strict: args.native_strict,
        profile: args.profile,
    };
    check_lazy(
        args.files,
        &flags,
        args.fix,
        (args.model.as_deref(), args.access.as_deref()),
        interactive_theme(plain_theme),
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;

    /// #1249 · the postcard leads, then every verb the tree carries (the
    /// hidden ones too), the file-as-command gesture and the deeper doors.
    #[test]
    fn the_help_page_names_every_verb_and_the_file_gesture() {
        let page = help_page();
        assert!(page.starts_with(help_card::human_help()), "{page}");
        for sub in Cli::command().get_subcommands() {
            let name = sub.get_name();
            assert!(
                name == "help" || page.contains(&format!("nika {name} ")),
                "`nika {name}` is missing from --help:\n{page}"
            );
        }
        assert!(page.contains("nika x.nika"), "{page}");
        assert!(page.contains("nika --help --all"), "{page}");
    }

    /// #1249 · a file-shaped first word names the run door; a verb, a
    /// typo'd verb, a flag and `help` keep clap's own answer.
    #[test]
    fn a_file_shaped_first_word_names_the_run_door_and_a_verb_does_not() {
        let dir = tempfile::tempdir().expect("tmp");
        let other = dir.path().join("notes.yaml");
        std::fs::write(&other, "nika: notes\n").expect("write");
        let text = file_near_miss(other.as_os_str()).expect("a .yaml on disk is a near-miss");
        assert!(
            text.contains(&format!("nika run {}", other.display())) && text.contains("*.nika"),
            "{text}"
        );
        let missing = file_near_miss(std::ffi::OsStr::new("missing.nika")).expect("missing");
        assert!(
            missing.contains("nika run missing.nika") && missing.contains("nika list"),
            "{missing}"
        );
        for word in ["check", "run", "chek", "--json", "help", "thread"] {
            assert!(
                file_near_miss(std::ffi::OsStr::new(word)).is_none(),
                "`{word}` keeps clap's answer"
            );
        }
    }
}
