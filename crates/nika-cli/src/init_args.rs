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
    /// created `.vscode/settings.json`.
    #[arg(long, value_enum)]
    theme: Option<verbs::init::CanvasTheme>,
    /// Wire agent clients to the MCP oracle after the scaffold
    /// (comma-separated · the same targets as `nika wire`).
    #[arg(long, value_enum, value_delimiter = ',')]
    wire: Vec<verbs::wire::WireTarget>,
    /// Lay a starter `nika.yaml` (the project file — ceiling +
    /// retention examples, commented so the starter governs nothing
    /// until you edit it). The ONLY scripted door; the wizard lane
    /// asks instead. An existing file is skipped, `--force` overrides.
    #[arg(long)]
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
        args.theme,
        &args.wire,
        args.project_file,
        plain_theme,
    )
}
