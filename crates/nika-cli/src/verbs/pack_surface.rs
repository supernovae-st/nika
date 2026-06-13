// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The embedded self-contained surface (spec §2) — `nika spec` ·
//! `nika schema` · `nika examples list|show|run`.
//!
//! Everything reads `nika-pack` (the spec snapshot baked at build):
//! offline forever, version-pinned, zero network. `examples run`
//! executes the embedded example through the shipped L3 runtime (see
//! `verbs::run::example`).

use crate::verbs::{VerbOutput, exit};

/// `nika spec` — the embedded spec identity (+ `--canon` = the SSOT).
#[must_use]
pub fn spec(canon: bool) -> VerbOutput {
    if canon {
        return VerbOutput::ok(nika_pack::canon().to_owned());
    }
    VerbOutput::ok(format!(
        "nika language pack {} (embedded · self-contained)\n\n{}",
        nika_pack::pack_version(),
        nika_pack::manifest()
    ))
}

/// `nika schema` — the JSON Schema for `*.nika.yaml` (machine surface).
#[must_use]
pub fn schema() -> VerbOutput {
    VerbOutput::ok(nika_pack::schema_json().to_owned())
}

/// `nika examples list` — every embedded example slug.
#[must_use]
pub fn examples_list() -> VerbOutput {
    VerbOutput::ok(nika_pack::example_slugs().join("\n"))
}

/// `nika examples show <slug>` — print one embedded example.
#[must_use]
pub fn examples_show(slug: &str) -> VerbOutput {
    match nika_pack::example(slug) {
        Some(yaml) => VerbOutput::ok(yaml.to_owned()),
        None => VerbOutput {
            text: format!("unknown example `{slug}` — `nika examples list` names the embedded set"),
            code: exit::FILE,
        },
    }
}

// `nika examples run <slug>` now EXECUTES — `verbs::run::example` (the
// L3 runtime shipped · the refusal stub retired).
