// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika explain NIKA-XXXX` — teach one error code (spec §2).
//!
//! Reads the canonical registry (`nika-error::codes`) — cause category,
//! severity, slug and the fix-form help text. Never invents: an unknown
//! code is a finding (`exit 2`), not a guess.

use nika_error::codes::{code_help, lookup};

use crate::verbs::VerbOutput;

/// The `nika explain <code>` verb. Accepts `NIKA-440` or bare `440`.
#[must_use]
pub fn run(wire: &str) -> VerbOutput {
    let normalized = if wire.starts_with("NIKA-") {
        wire.to_owned()
    } else {
        format!("NIKA-{wire}")
    };
    let Some(code) = lookup(&normalized) else {
        return VerbOutput::file(format!(
            "unknown code `{wire}` — the registry knows NIKA-001..NIKA-9999 \
             (allocated ranges only); see docs.nika.sh/errors"
        ));
    };
    // The category/severity labels are the OWNING crate's canonical
    // kebab-case (`Category::as_str`), not a `Debug` derive an enum rename
    // could silently change — one source of truth, compile-forced complete.
    let text = format!(
        "{code} · {category} · {severity} · {slug}\n\n  {help}\n",
        category = code.category.as_str(),
        severity = code.severity.as_str(),
        slug = code.slug,
        help = code_help(code),
    );
    VerbOutput::ok(text)
}
