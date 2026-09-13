// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `init` clap surface + its verb unpacker — split from `main.rs`
//! under the 1500-line file law (the `registry_args` precedent). The
//! founding wizard itself lives in `nika-onboard`; the REAL effects are
//! injected by `verbs::init` (the composition-root adapter).

use clap::Args;

use crate::{Theme, VerbOutput, verbs};

/// The `init` clap surface — the founding wizard's scriptable twin.
#[derive(Args)]
pub(crate) struct InitArgs {
    /// Target directory (default · the current directory).
    #[arg(default_value = ".")]
    dir: String,
    /// Overwrite existing files.
    #[arg(long)]
    force: bool,
    /// Accept every default — never prompt (pipes and CI are
    /// implicitly `--yes`; prompts only ever appear on a terminal).
    #[arg(long, short = 'y')]
    yes: bool,
    /// Scaffold a workflow set — the wizard's recipe step, scriptable
    /// (`agentic` = the 4-pattern curriculum).
    #[arg(long, value_parser = clap::builder::PossibleValuesParser::new(verbs::init::RECIPE_NAMES))]
    recipe: Option<String>,
    /// Found the project from ONE embedded example (verbatim — any slug
    /// from bare `nika try`). One founding source: conflicts with
    /// `--recipe`.
    #[arg(long, value_name = "SLUG", conflicts_with = "recipe")]
    example: Option<String>,
    /// Stamp the VS Code DAG canvas skin (`nika.dag.theme`) into the
    /// created `.vscode/settings.json` (`nika` the brand skin · `editor`
    /// adaptive · `phosphor` terminal green · `auto` lets the extension decide).
    #[arg(long, value_parser = clap::builder::PossibleValuesParser::new(verbs::init::CANVAS_THEMES))]
    theme: Option<String>,
    /// Wire agent clients to the MCP oracle after the scaffold
    /// (comma-separated · the same targets as `nika wire`).
    #[arg(long, value_enum, value_delimiter = ',')]
    wire: Vec<verbs::wire::WireTarget>,
    /// Lay the starter `nika.yaml` — every scripted init does since #1283
    /// (existing skipped · `--force` overrides); kept so older scripts work.
    #[arg(long, hide = true)]
    project_file: bool,
}

/// Unpack the `init` clap surface into the library verb call.
pub(crate) fn init_verb(args: &InitArgs, plain_theme: Theme) -> VerbOutput {
    verbs::init::run(
        &args.dir,
        args.force,
        args.yes,
        args.recipe.as_deref(),
        args.example.as_deref(),
        args.theme.as_deref(),
        &args.wire,
        args.project_file,
        plain_theme,
    )
}
