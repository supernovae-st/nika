// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The onboarding surface — the founding wizard (`nika init`) and the
//! stateless Compile authoring core.
//!
//! Descended from the CLI authoring/bootstrap surface at the 15k
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
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod banner;
mod bootstrap;
pub mod briefs;
/// The Compile unit at the paths every caller reads: the deterministic core (`nika-compile`)
/// and the seats' doors (`nika-compile-cognition`), one unit in several members (ADR-137 ·
/// ADR-138 · ADR-140).
pub mod compile {
    pub use nika_compile::{
        AuthoringCognition, AuthoringKnowledge, AuthoringPolicy, AuthoringReceipt,
        COMPILE_WIRE_VERSION, ChoiceOffer, CompileDiagnostic, CompileError, CompileOutcome,
        CompilePreview, CompileProvenance, CompileQuestion, CompileRequest, CompileStatus,
        DiagnosticKind, Hit, HitKind, HotPolicy, KnowledgeReference, MaterializeError, NativeMode,
        PreviewScope, QuestionType, RepresentationError, Strategy, TriggerKind, TriggerRequirement,
        TriggerStatus, compile, fold, intent_sha256, materialize_ready, outcome_document, retrieve,
        retrieve_by_ops, revise_intent, stated_destinations, stated_sources, text,
    };
    pub use nika_compile_cognition::{
        Cognition, NoProvider, compile_with_cognition, compile_with_provider, decide,
    };
}
pub mod fixtures;
pub mod founding;
mod gitignore;
mod intent;
pub mod knowledge;
pub mod project_file;
pub mod recipes;
pub mod rehearsal;
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
