// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `try` clap surface + its two halves — split from `main.rs`
//! under the 1500-line file law (the `init_args` · `registry_args`
//! precedent). The showroom rendering itself lives in
//! `verbs::examples`; the rehearsal is `verbs::run::example`.

use std::path::PathBuf;

use clap::Args;

use crate::{Theme, VerbOutput, verbs};

/// The `try` clap surface — the showroom door and its rehearsal seat.
#[derive(Args)]
pub(crate) struct TryArgs {
    /// Example slug (bare `nika try` shows the storefront).
    pub(crate) slug: Option<String>,
    /// The whole shelf — the numbered path plus every job (bare
    /// `nika try` shows three familiar jobs first).
    #[arg(long)]
    all: bool,
    /// Run on a REAL seat instead of the default mock rehearsal
    /// (`<provider>/<name>` — the example's own `model:` via `self`).
    #[arg(long, value_name = "PROVIDER/NAME")]
    model: Option<String>,
    /// Pin the ACCESS path for the rehearsal (a class — `local` · `api`
    /// · `harness` · `oauth` · `mock` — or an access id `nika doctor`
    /// lists). Unsatisfied refuses with a witness, never substitutes.
    #[arg(long, value_name = "PATH")]
    access: Option<String>,
    /// Set a workflow `inputs:` value (repeatable).
    #[arg(long, value_name = "KEY=VALUE")]
    var: Vec<String>,
    /// Verdict line only (suppress the storyboard).
    #[arg(long)]
    quiet: bool,
    /// One final storyboard frame (no live repaints) — pipes/CI get
    /// this automatically.
    #[arg(long)]
    no_progress: bool,
    /// Refuse to start if the static cost floor exceeds this (USD);
    /// metered spend aborts past it mid-run. Same guard, same parser
    /// as `nika run`.
    #[arg(long, value_name = "USD", value_parser = crate::parse_budget_usd)]
    max_cost_usd: Option<f64>,
    /// Resume from a prior run's NDJSON trace — same flag, same parser
    /// as `nika run --resume`. Relative paths pin to the operator cwd
    /// (the rehearsal then enters a temp room).
    #[arg(long, value_name = "TRACE")]
    pub(crate) resume: Option<PathBuf>,
    /// Answer a `nika:prompt` gate (repeatable · same parser as
    /// `nika run --answer`). Without `--resume` the answer is
    /// PRE-SEEDED on the fresh rehearsal (the CI one-pass gate).
    #[arg(long = "answer", value_name = "TASK=VALUE")]
    pub(crate) answer: Vec<String>,
}

/// The listing half — `Some(output)` when no slug was named (the
/// composition root emits it), `None` when the caller must rehearse.
pub(crate) fn listing(args: &TryArgs, plain_theme: Theme) -> Option<VerbOutput> {
    args.slug
        .is_none()
        .then(|| verbs::examples::shelf_or_front(args.all, plain_theme))
}

/// The rehearsal half — the named example runs on its seat (offline
/// mock by default · `--model self` keeps the file's own).
pub(crate) fn rehearse(args: &TryArgs, plain_theme: Theme) -> u8 {
    let Some(slug) = args.slug.as_deref() else {
        return verbs::exit::OK;
    };
    verbs::run::example(
        slug,
        args.model.as_deref(),
        args.access.as_deref(),
        &args.var,
        (args.quiet, args.no_progress),
        args.max_cost_usd,
        (&args.answer, args.resume.as_deref()),
        plain_theme,
    )
}
