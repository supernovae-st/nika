// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The onboarding surface — the founding wizard (`nika init`) and the
//! guided first workflow (`nika new`).
//!
//! Descended from `nika-cli/src/verbs/{new.rs, init/}` at the 15k
//! prod-LOC wall (2026-07-12 · the `nika-display`/`nika-dap`/`nika-tmpl`
//! precedents) — per D-2026-07-09-N1 this is the cli UNIT in a second
//! member, named by parentage in `docs/crate-specs/nika-onboard.md`.
//!
//! Two effects stay INJECTED (the kernel-trait stance, applied at the
//! surface layer): the audit ladder ([`Audit`] — `nika check` at the
//! composition root) and the MCP wiring ([`Wire`] — `nika wire`). This
//! crate converses, scaffolds and reports; the composition root owns
//! what proving and wiring actually mean. Conversations run over any
//! `BufRead`/`Write` pair (tests inject a cursor · the binary hands the
//! terminal), so nothing here touches a TTY directly.

// Test code speaks expect/unwrap freely (the nika-cli stance, inherited
// by the descent).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod banner;
pub mod briefs;
pub mod fixtures;
pub mod founding;
mod gitignore;
pub mod guided;
mod intent;
pub mod project_file;
pub mod recipes;
pub mod routing;
pub mod wizard;

/// A finished verb's text + exit code — the shape the composition root
/// re-wraps into its own `VerbOutput` (kept local so the descent adds
/// zero reverse dependency).
#[derive(Debug)]
pub struct Outcome {
    /// The human-facing (or machine-stable) text.
    pub text: String,
    /// The spec §4 exit code.
    pub code: u8,
}

impl Outcome {
    /// Success (`exit 0`).
    #[must_use]
    pub fn ok(text: String) -> Self {
        Self {
            text,
            code: codes::OK,
        }
    }

    /// Environment error (`exit 3`).
    #[must_use]
    pub fn env(text: String) -> Self {
        Self {
            text,
            code: codes::ENV,
        }
    }
}

/// The spec §4 exit vocabulary this surface speaks (mirrors the
/// composition root's `verbs::exit` — stable by contract, so the
/// duplication cannot drift).
pub mod codes {
    /// Success.
    pub const OK: u8 = 0;
    /// File findings (audit-before-run).
    pub const FILE: u8 = 2;
    /// Environment error (unwritable target · cancelled conversation).
    pub const ENV: u8 = 3;
}

/// The injected audit ladder — `path` in, the check report + exit code
/// out (`nika check <path>` at the composition root; tests inject a
/// stub). Infallible by contract: refusals are an [`Outcome`], never a
/// panic.
pub type Audit<'a> = dyn Fn(&str) -> Outcome + 'a;

/// The injected MCP wiring — `(client, dir)` in, the wire receipt out
/// (`nika wire <client>` at the composition root).
pub type Wire<'a> = dyn Fn(&str, &str) -> Outcome + 'a;
