// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The static verb suite — everything auditable BEFORE a run (spec §2).
//!
//! Every verb here is a pure function `(args, file) → (text, exit code)`
//! over the shipped engine layers (`nika-schema` ladder · `nika-error`
//! registry · `nika-pack` embedded surface). No network, no effects, no
//! runtime — the `run` verb arrives with L3 and is refused honestly until
//! then. The bin (`main.rs`) stays a thin dispatcher so every surface is
//! testable as a library call.

pub mod check;
pub mod explain;
pub mod graph;
pub mod inspect;
pub mod new;
pub mod pack_surface;

use nika_schema::check::CheckReport;
use nika_schema::raw::RawWorkflow;
use nika_schema::{FileId, ParseMode};

/// Exit-code contract (spec §4 · LOCKED · additive-only forever).
pub mod exit {
    /// Success (run completed · check clean · verb done).
    pub const OK: u8 = 0;
    /// Validation findings — the FILE has errors (CI gates on this).
    pub const FILE: u8 = 2;
    /// Environment error — config · I/O · missing resource.
    pub const ENV: u8 = 3;
}

/// One verb invocation's outcome: the text to print + the exit code.
#[derive(Debug)]
pub struct VerbOutput {
    /// Human or machine text (the caller owns the stream choice).
    pub text: String,
    /// Spec §4 exit code.
    pub code: u8,
}

impl VerbOutput {
    fn ok(text: String) -> Self {
        Self {
            text,
            code: exit::OK,
        }
    }

    fn file(text: String) -> Self {
        Self {
            text,
            code: exit::FILE,
        }
    }

    fn env(text: String) -> Self {
        Self {
            text,
            code: exit::ENV,
        }
    }
}

/// Read + strict-parse + ladder-check one workflow file.
///
/// Failure mapping per spec §4: unreadable = environment (`3`) · parse
/// error = a finding in the FILE (`2`).
fn load_checked(path: &str) -> Result<(RawWorkflow, CheckReport), VerbOutput> {
    let yaml = std::fs::read_to_string(path)
        .map_err(|e| VerbOutput::env(format!("cannot read {path}: {e}")))?;
    let wf = nika_schema::parse(&yaml, FileId::new(0), ParseMode::Strict)
        .map_err(|e| VerbOutput::file(format!("PARSE ✗  {e}")))?;
    let report = nika_schema::check(&wf);
    Ok((wf, report))
}
