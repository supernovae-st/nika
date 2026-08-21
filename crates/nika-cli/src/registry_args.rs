// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `registry:` arg seam of the binary (issue #452) — swap any
//! `registry:owner/name[@version]` file argument for its verified local
//! cache file, then hand check/run the path exactly as if the user had
//! typed it. A bin-side sibling of `main.rs` (the dispatcher stays a
//! thin verb tree; this is the one arg transform it owes the registry
//! lane). Resolution is CLI-level and happens BEFORE any workflow is
//! parsed: a workflow's `permits:` govern its run, never this fetch.

use nika_cli::registry;
use nika_cli::verbs::{self, VerbOutput};

use crate::{ColorWhenArg, RunArgs, emit, run_verb};
use nika_cli::display::format::LinkChoice;
use nika_cli::verbs::check::{CheckFlags, CheckTarget, dispatch_targets as check_dispatch};

/// The `check` arm: registry refs resolve first (the `--fix` guard
/// rides the same seam), then the normal multi-file dispatch.
pub(crate) fn check_verb(
    files: &[String],
    flags: &CheckFlags,
    fix: bool,
    model: Option<&str>,
    theme: nika_cli::Theme,
) -> u8 {
    match resolve_registry_args(files, fix) {
        Ok(files) => emit(&check_dispatch(&files, flags, fix, model, theme)),
        Err(out) => emit(&out),
    }
}

/// The `run` arm: a registry ref resolves to its verified cache file,
/// then the run proceeds exactly as if given that path.
pub(crate) fn registry_then_run(
    mut args: RunArgs,
    color: ColorWhenArg,
    link_when: LinkChoice,
    plain: bool,
    ascii: bool,
) -> u8 {
    // The dispatcher's lazy resolver guaranteed the target before this
    // seam — an absent file here is a wiring bug, surfaced honestly.
    let Some(file) = args.file.as_deref() else {
        return emit(&crate::VerbOutput {
            text: "nika run: no workflow target resolved (internal wiring)".to_owned(),
            code: nika_cli::verbs::exit::ENV,
        });
    };
    let repair_target = registry::is_registry_ref(file)
        .then_some(nika_cli::display::check_render::RepairTarget::RegistryArtifact);
    match resolve_registry_arg(file) {
        Ok(file) => {
            args.file = Some(file);
            run_verb(&args, color, link_when, plain, ascii, repair_target)
        }
        Err(out) => emit(&out),
    }
}

/// Swap every registry ref among the file args for its verified cache
/// path — non-refs pass untouched. `--fix` refuses refs BEFORE any
/// network: a digest-pinned artifact stays read-only (rewriting the
/// cache would poison its record).
fn resolve_registry_args(files: &[String], fix: bool) -> Result<Vec<CheckTarget>, VerbOutput> {
    if fix && files.iter().any(|f| registry::is_registry_ref(f)) {
        return Err(VerbOutput {
            text: "--fix rewrites a file, and a registry artifact is pinned by its \
                   digest — editing the cache would poison it\n  fix: copy the cached \
                   file into your workspace, edit the copy, check that"
                .to_owned(),
            code: verbs::exit::ENV,
        });
    }
    files
        .iter()
        .map(|f| resolve_registry_check_arg(f))
        .collect()
}

/// Resolve a check target while retaining the coordinate's immutable
/// provenance separately from its cache path.
fn resolve_registry_check_arg(arg: &str) -> Result<CheckTarget, VerbOutput> {
    if !registry::is_registry_ref(arg) {
        return Ok(CheckTarget::workspace(arg));
    }
    resolve_registry_arg(arg).map(CheckTarget::registry_artifact)
}

/// One argument through the registry seam. The fetch note goes to
/// stderr — the machine surfaces on stdout stay pure.
fn resolve_registry_arg(arg: &str) -> Result<String, VerbOutput> {
    if !registry::is_registry_ref(arg) {
        return Ok(arg.to_owned());
    }
    match registry::resolve_blocking(arg) {
        Ok(resolved) => {
            eprintln!("{}", resolved.describe());
            Ok(resolved.path.to_string_lossy().into_owned())
        }
        Err(e) => Err(VerbOutput {
            text: e.to_string(),
            code: verbs::exit::ENV,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_fix_refuses_before_resolution_and_teaches_copy() {
        let mut refused = false;
        if let Err(out) = resolve_registry_args(&["registry:acme/report".to_owned()], true) {
            refused = true;
            assert_eq!(out.code, verbs::exit::ENV);
            assert!(
                out.text
                    .contains("copy the cached file into your workspace")
            );
            assert!(out.text.contains("edit the copy"));
        }
        assert!(refused, "registry artifacts are immutable repair inputs");
    }
}
