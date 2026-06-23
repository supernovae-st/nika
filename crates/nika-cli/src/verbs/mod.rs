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
pub mod doctor;
pub mod explain;
pub mod graph;
pub mod init;
pub mod inspect;
pub mod new;
pub mod pack_surface;
pub mod run;
pub mod wire;

use nika_schema::check::CheckReport;
use nika_schema::raw::RawWorkflow;
use nika_schema::{FileId, ParseMode};

/// Exit-code contract (spec §4 · LOCKED · additive-only forever).
pub mod exit {
    /// Success (run completed · check clean · verb done).
    pub const OK: u8 = 0;
    /// A workflow RAN and FAILED (a task failed unrecovered · `nika run`
    /// only · distinct from a static FILE finding · spec §4).
    pub const WORKFLOW: u8 = 1;
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
pub(crate) fn load_checked(path: &str) -> Result<(RawWorkflow, CheckReport), VerbOutput> {
    let yaml = std::fs::read_to_string(path)
        .map_err(|e| VerbOutput::env(format!("cannot read {path}: {e}")))?;
    let wf = nika_schema::parse(&yaml, FileId::new(0), ParseMode::Strict)
        .map_err(|e| VerbOutput::file(format!("PARSE ✗  [{}] {e}", e.spec_code())))?;
    let report = nika_schema::check(&wf);
    Ok((wf, report))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression · a parse-stage rejection must surface its spec wire code,
    /// exactly like the CONFORM stage. The multiple-verbs short-circuit used
    /// to render `PARSE ✗ <msg>` with no `[NIKA-PARSE-009]`, so an operator
    /// could not `nika explain` the failure (every other finding shows its
    /// code). `load_checked` now formats `e.spec_code()`.
    #[test]
    fn parse_error_carries_its_spec_code() {
        let path =
            std::env::temp_dir().join(format!("nika-parsecode-{}.nika.yaml", std::process::id(),));
        std::fs::write(
            &path,
            "nika: v1\nworkflow: two-verbs\nmodel: mock/echo\ntasks:\n  - id: a\n    infer: { prompt: \"x\" }\n    exec: { run: \"echo hi\" }\n",
        )
        .expect("fixture written");
        let err = load_checked(path.to_str().expect("utf-8 tmp path"))
            .expect_err("a task with two verbs must fail to parse");
        std::fs::remove_file(&path).ok();
        assert_eq!(err.code, exit::FILE, "{}", err.text);
        assert!(
            err.text.contains("NIKA-PARSE-009"),
            "parse error must carry its spec code · got: {}",
            err.text
        );
    }
}
