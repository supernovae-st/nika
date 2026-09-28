// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Binary-only options preserve the public, closed `ArmArgs` constructor.

#[derive(Debug, clap::Args)]
pub(crate) struct ArmOptions {
    #[command(flatten)]
    pub(crate) base: nika_cli::verbs::arm::args::ArmArgs,
    /// Report captured workflow and input-binding readiness as JSON; grants no authority.
    #[arg(long)]
    pub(crate) json: bool,
}

pub(crate) fn run(options: ArmOptions) -> nika_cli::verbs::VerbOutput {
    if options.json {
        nika_cli::verbs::arm::run_json(&options.base)
    } else {
        nika_cli::verbs::arm::run(options.base)
    }
}
