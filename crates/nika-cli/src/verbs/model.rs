// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika model` — thin adapters over the descended `nika-models` unit
//! (serve · pull · list · rm · issue #146). The member owns the logic
//! (the ONE models dir · Hub acquisition · the sidecar launch glue) and
//! speaks plain strings; THIS file owns what the texts MEAN on the verb
//! tree — receipts exit `0`, refusals exit `3` (environment class).
//! Split 2026-07-12 per D-2026-07-09-N1 (the 15k prod-LOC crate cap —
//! the `nika-onboard`/`nika-display` precedents).

use std::path::Path;

use crate::verbs::{VerbOutput, exit};

/// The default loopback port (`nika mcp` owns 8123; this stays clear of
/// the 5 external local servers' defaults: 11434 · 1234 · 8080 · 8000).
pub const DEFAULT_PORT: u16 = nika_models::serve::DEFAULT_PORT;

/// Serve a GGUF in the foreground on loopback `port` — returns ONLY on
/// refusal (a healthy server runs until Ctrl-C); the whole surface is
/// feature-gated `local-infer` (the default body teaches the recipe).
#[must_use]
pub fn serve(
    gguf: &Path,
    tokenizer: Option<&Path>,
    port: u16,
    model_id: Option<&str>,
) -> VerbOutput {
    env(nika_models::serve::serve(gguf, tokenizer, port, model_id))
}

/// The models-dir store: by-id resolution · list · rm.
pub mod store {
    use std::path::PathBuf;

    use super::{VerbOutput, env, ok};

    /// Resolve `serve --model <arg>`: paths pass through (`serve` owns
    /// the missing-path verdict per build axis — the bin_smoke-pinned
    /// #482 contract); ids resolve against the ONE models dir.
    ///
    /// # Errors
    ///
    /// An exit-3 refusal on the id lane only — a miss lists what IS
    /// installed (the teaching surface), an ambiguous id the exact ids.
    pub fn resolve_serve_model(arg: &str) -> Result<PathBuf, VerbOutput> {
        nika_models::store::resolve_serve_model(arg).map_err(env)
    }

    /// `nika model list` — the dir once at top, then id · size · file.
    #[must_use]
    pub fn list() -> VerbOutput {
        ok(nika_models::store::list())
    }

    /// `nika model rm <id>` — reclaim a repo or one quant; a no-match
    /// refuses with the installed list.
    #[must_use]
    pub fn rm(id: &str) -> VerbOutput {
        nika_models::store::rm(id).map_or_else(env, ok)
    }
}

/// The Hub acquisition (`nika model pull`).
pub mod pull {
    use super::{VerbOutput, env, ok};

    /// `nika model pull <owner/repo[:QUANT]>` — receipts (and the
    /// operator's clean abort) exit `0`, refusals `3`.
    #[must_use]
    pub fn run(arg: &str, yes: bool) -> VerbOutput {
        nika_models::pull::run(arg, yes).map_or_else(env, ok)
    }
}

/// A receipt on the success stream (exit `0`).
fn ok(text: String) -> VerbOutput {
    VerbOutput {
        text,
        code: exit::OK,
    }
}

/// A teaching refusal on stderr (exit `3` · environment class).
fn env(text: String) -> VerbOutput {
    VerbOutput {
        text,
        code: exit::ENV,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The adapter's ONE job: receipts ride exit 0, refusals exit 3 —
    /// the member's strings keep their teaching text verbatim.
    #[test]
    fn the_adapter_maps_strings_onto_the_locked_exit_contract() {
        let refusal = pull::run("not-a-ref", true);
        assert_eq!(refusal.code, exit::ENV);
        assert!(refusal.text.contains("owner/repo"), "{}", refusal.text);

        let listed = store::list();
        assert_eq!(listed.code, exit::OK);
        assert!(listed.text.contains("models ·"), "{}", listed.text);

        let missing = store::rm("absent/never-pulled-model");
        assert_eq!(missing.code, exit::ENV);
    }
}
