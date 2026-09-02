// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The per-verb arms folded out of `main.rs` at the 1500-line file wall:
//! the dispatch seam stays one line per verb, the plumbing lives here.

use nika_cli::Theme;
use nika_cli::verbs;

use crate::lazy::{check_lazy, resolve_lazy_target};
use crate::{emit, interactive_theme};

/// The check arm's plumbing — folded out of the dispatch so the seam
/// stays one line per verb.
#[allow(
    clippy::fn_params_excessive_bools,
    clippy::too_many_arguments,
    clippy::needless_pass_by_value
)]
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

pub(crate) fn check_arm(args: verbs::check::CheckArgs, plain_theme: Theme) -> u8 {
    if args.sdk_snapshot {
        let output = match args.files.as_slice() {
            [file]
                if args.json
                    && !args.fix
                    && !args.infer_permits
                    && !args.native_strict
                    && args.profile == verbs::check::Profile::Advisory
                    && args.model.is_none() =>
            {
                verbs::check::run_snapshot_export(file, interactive_theme(plain_theme))
            }
            _ => verbs::VerbOutput {
                text: "check: --sdk-snapshot requires exactly one file and --json, with no other check overrides\n"
                    .to_owned(),
                code: verbs::exit::ENV,
            },
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
        args.model.as_deref(),
        interactive_theme(plain_theme),
    )
}
