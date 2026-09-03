// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The lazy-hands resolver — bare `nika run` / `nika check` resolve
//! the workspace's only workflow (split from `main.rs` under the
//! 1500-line file law · the `init_args` precedent).

use std::path::Path;

use nika_cli::Theme;
use nika_cli::display::format::LinkChoice;
use nika_cli::verbs;

use crate::{ColorWhenArg, RunArgs, registry_args};
use nika_cli::verbs::check::CheckFlags;

/// The check-side lazy door — an explicit list passes through; an
/// empty one resolves the workspace's only workflow (or refuses with
/// the routing the resolver prints).
pub(crate) fn lazy_files(files: Vec<String>) -> Result<Vec<String>, u8> {
    if files.is_empty() {
        resolve_lazy_target(None, "check").map(|one| vec![one])
    } else {
        Ok(files)
    }
}

/// The lazy-hands resolver — `nika run` / `nika check` with NO file.
/// Exactly one workflow in this workspace → use it and SAY so (stderr —
/// the announce never touches a stdout contract); zero → route to the
/// founding trio; several → name them all, copy-paste ready. Nobody
/// should have to remember a filename the tool already knows.
pub(crate) fn resolve_lazy_target(given: Option<String>, verb: &str) -> Result<String, u8> {
    if let Some(file) = given {
        return Ok(file);
    }
    let cwd = Path::new(".");
    let mut budget = 4000usize;
    let mut paths = Vec::new();
    verbs::probe::collect_workflow_paths(cwd, cwd, 4, &mut budget, &mut paths);
    paths.sort();
    match paths.len() {
        1 => {
            let one = paths[0].to_string_lossy().into_owned();
            let one = one.strip_prefix("./").unwrap_or(&one).to_owned();
            eprintln!("→ {one} (the only workflow here)");
            Ok(one)
        }
        0 => {
            eprintln!("nika {verb}: no workflow here yet\n  nika new hello");
            Err(verbs::exit::ENV)
        }
        n => {
            eprintln!("nika {verb}: {n} workflows here — pass one:");
            for p in paths.iter().take(10) {
                let p = p.to_string_lossy();
                eprintln!("  nika {verb} {}", p.strip_prefix("./").unwrap_or(&p));
            }
            if n > 10 {
                eprintln!("  … and {} more", n - 10);
            }
            Err(verbs::exit::ENV)
        }
    }
}

/// The check verb behind the lazy door — resolve the (possibly empty)
/// list, then hand it to the registry-gated check.
pub(crate) fn check_lazy(
    files: Vec<String>,
    flags: &CheckFlags,
    fix: bool,
    overrides: (Option<&str>, Option<&str>),
    theme: Theme,
) -> u8 {
    match lazy_files(files) {
        Ok(files) => registry_args::check_verb(&files, flags, fix, overrides, theme),
        Err(code) => code,
    }
}

/// The run verb behind the lazy door — resolve the missing target,
/// then take the registry-then-run road.
pub(crate) fn run_lazy(
    mut args: RunArgs,
    color: ColorWhenArg,
    link_when: LinkChoice,
    plain: bool,
    ascii: bool,
) -> u8 {
    match resolve_lazy_target(args.file.take(), "run") {
        Ok(file) => {
            args.file = Some(file);
            registry_args::registry_then_run(args, color, link_when, plain, ascii)
        }
        Err(code) => code,
    }
}
