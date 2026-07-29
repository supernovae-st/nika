// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `examples` verb's clap surface — action enum + dispatcher
//! (split from `main.rs` under the 1500-line file law · the
//! `init_args` precedent).

use clap::Subcommand;

use nika_cli::Theme;
use nika_cli::verbs;

use crate::emit;

#[derive(Subcommand)]
pub(crate) enum ExamplesAction {
    /// List the embedded example slugs.
    List,
    /// Which examples USE a construct — and with no argument, the
    /// coverage of the whole index, gaps included.
    Teaches {
        /// A construct key (`for_each:` · the trailing colon is optional).
        /// Omit it to see every construct and which are uncovered.
        construct: Option<String>,
    },
    /// Print one embedded example.
    Show {
        /// Example slug (from `list`).
        slug: String,
    },
    /// Copy an embedded example into YOUR workspace (make it yours).
    Copy {
        /// Example slug (from `list`).
        slug: String,
        /// Destination path (default: `<slug>.nika.yaml` beside you).
        dest: Option<String>,
        /// Overwrite an existing destination.
        #[arg(long)]
        force: bool,
    },
    /// Run an embedded example (audited first · live render).
    Run {
        /// Example slug (from `list`).
        slug: String,
        /// Override the example's `model:` (`<provider>/<name>`). Use
        /// `--model mock/echo` to preview offline (zero key · zero network).
        #[arg(long, value_name = "PROVIDER/NAME")]
        model: Option<String>,
        /// Set a workflow `inputs:` value (repeatable) — required examples name theirs in the header.
        #[arg(long, value_name = "KEY=VALUE")]
        var: Vec<String>,
        /// Verdict line only (suppress the storyboard).
        #[arg(long)]
        quiet: bool,
        /// One final storyboard frame (no live repaints) — pipes/CI
        /// get this automatically.
        #[arg(long)]
        no_progress: bool,
        /// Refuse to start if the static cost floor exceeds this (USD);
        /// metered spend aborts past it mid-run. Same guard, same
        /// parser as `nika run` — a NaN/inf here would silently disarm
        /// the budget (every comparison false), the exact class the
        /// parser exists to kill.
        #[arg(long, value_name = "USD", value_parser = crate::parse_budget_usd)]
        max_cost_usd: Option<f64>,
    },
}

/// Dispatch the `trace` verb family: the live renders (replay · show)
/// plus the static readers (outputs · peek · flow).
/// The `examples` sub-verbs — list · show · run-for-real (L3 shipped).
pub(crate) fn examples_verb(action: Option<ExamplesAction>, plain_theme: Theme) -> u8 {
    match action.unwrap_or(ExamplesAction::List) {
        ExamplesAction::List => emit(&verbs::examples::list(plain_theme)),
        ExamplesAction::Teaches { construct } => emit(&verbs::examples::teaches_verb(
            construct.as_deref(),
            plain_theme,
        )),
        ExamplesAction::Show { slug } => emit(&verbs::examples::show(&slug, plain_theme)),
        ExamplesAction::Copy { slug, dest, force } => emit(&verbs::examples::copy(
            &slug,
            dest.as_deref(),
            force,
            plain_theme,
        )),
        // The L3 run verb shipped — execute the embedded example for real.
        ExamplesAction::Run {
            slug,
            model,
            var,
            quiet,
            no_progress,
            max_cost_usd,
        } => verbs::run::example(
            &slug,
            model.as_deref(),
            &var,
            (quiet, no_progress),
            max_cost_usd,
            plain_theme,
        ),
    }
}
