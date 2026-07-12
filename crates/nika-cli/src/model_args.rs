// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `nika model` clap family — a bin-side sibling of `main.rs` (the
//! `registry_args.rs` convention): the dispatcher stays a thin verb
//! tree, and the model family grows HERE, not at main.rs' 1500-line
//! wall. Verb bodies live in the library (`verbs::model`) so every
//! surface stays testable as a library call.

use clap::Subcommand;

use crate::emit;
use nika_cli::verbs;

#[derive(Subcommand)]
pub(crate) enum ModelAction {
    /// Serve a GGUF model — an OpenAI-compatible foreground server on
    /// 127.0.0.1 (Ctrl-C stops it · the banner says how workflows reach it).
    Serve {
        /// The model: a `.gguf` path, or a pulled id — `owner/repo[:QUANT]`
        /// or a file stem, resolved against the models dir (`nika model list`).
        #[arg(long, value_name = "PATH|ID")]
        model: String,
        /// The tokenizer file (default: `tokenizer.json` beside the model).
        #[arg(long, value_name = "PATH")]
        tokenizer: Option<std::path::PathBuf>,
        /// Loopback port to listen on.
        #[arg(long, default_value_t = verbs::model::DEFAULT_PORT)]
        port: u16,
        /// The model id responses report (default: the model file's name).
        #[arg(long, value_name = "ID")]
        model_id: Option<String>,
    },
    /// Download a GGUF from the Hugging Face Hub into the ONE models dir
    /// (`~/.nika/models` — the same dir `serve --model <id>` resolves, by
    /// construction). Size prints BEFORE downloading; 2 GiB and over
    /// confirms (`--yes` for CI). An interrupted pull resumes from its
    /// `.part`. `HF_TOKEN` authenticates gated repos. This fetch is
    /// CLI-level, like `registry:` pulls — a workflow's `permits:` never
    /// govern it.
    Pull {
        /// The Hub model: `owner/repo[:QUANT]` — default quant `Q4_K_M`
        /// when the repo tags one.
        #[arg(value_name = "OWNER/REPO[:QUANT]")]
        model: String,
        /// Skip the size confirmation (CI · scripts).
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// What's on disk: id · size · file per GGUF — the ONE models dir
    /// printed once at top.
    List,
    /// Remove a pulled model: `owner/repo` removes every quant (and the
    /// tokenizer beside them) · `owner/repo:QUANT` one file. A no-match
    /// refuses, listing what IS there.
    Rm {
        /// The id to remove (`owner/repo[:QUANT]`, or a file stem from `list`).
        #[arg(value_name = "ID")]
        id: String,
    },
}

/// The `model` sub-verbs — a healthy `serve` never returns, so `emit` only prints refusals.
pub(crate) fn model_verb(action: ModelAction) -> u8 {
    match action {
        ModelAction::Serve {
            model,
            tokenizer,
            port,
            model_id,
        } => {
            // By-id resolution against the ONE models dir (`pull` writes
            // it, this reads it) — a real path passes straight through.
            match verbs::model::store::resolve_serve_model(&model) {
                Ok(path) => emit(&verbs::model::serve(
                    &path,
                    tokenizer.as_deref(),
                    port,
                    model_id.as_deref(),
                )),
                Err(refusal) => emit(&refusal),
            }
        }
        ModelAction::Pull { model, yes } => emit(&verbs::model::pull::run(&model, yes)),
        ModelAction::List => emit(&verbs::model::store::list()),
        ModelAction::Rm { id } => emit(&verbs::model::store::rm(&id)),
    }
}
