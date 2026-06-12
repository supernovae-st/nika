// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-runtime` errors — the NIKA-1700 range (`Category::Runtime`).
//!
//! These are RUN-ABORT classes only (contract breaches between the
//! checker and the runtime · template/gate static failures). A verb
//! failing inside a task is NOT a runtime error — it becomes a
//! `TaskFailed` event carrying the verb's own `nika_code()` and the run
//! continues per the cascade semantics (spec §3).

use nika_error::traits::NikaErrorCode;
use nika_kernel::prelude::codes;

/// Run-abort errors · `NIKA-1700..1703` (spec §4).
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum RuntimeError {
    /// NIKA-1700 · a dirty [`nika_schema::check::CheckReport`] reached
    /// `run` (audit-before-run violated · a dirty workflow never executes).
    #[error("NIKA-1700 · audit-before-run violated · the CheckReport is dirty")]
    #[diagnostic(code(nika::runtime::dirty_report))]
    DirtyReport,

    /// NIKA-1701 · a wave referenced a task index outside the workflow
    /// (the checker/runtime schedule contract was breached).
    #[error("NIKA-1701 · wave index {index} out of bounds (workflow has {task_count} tasks)")]
    #[diagnostic(code(nika::runtime::wave_out_of_bounds))]
    WaveOutOfBounds {
        /// The offending task index.
        index: usize,
        /// The workflow's task count.
        task_count: usize,
    },

    /// NIKA-1702 · a `${{ }}` reference did not resolve (unknown task id
    /// or var key · the silent-literal guard).
    #[error("NIKA-1702 · unresolved template reference `{reference}`")]
    #[diagnostic(code(nika::runtime::unresolved_template))]
    UnresolvedTemplate {
        /// The reference inside the island (e.g. `tasks.ghost.output`).
        reference: String,
    },

    /// NIKA-1703 · a `when:` expression is outside the v0 gate subset.
    #[error("NIKA-1703 · `when:` expression outside the v0 subset · `{expr}`")]
    #[diagnostic(code(nika::runtime::when_unsupported))]
    WhenUnsupported {
        /// The raw expression body.
        expr: String,
    },
}

impl NikaErrorCode for RuntimeError {
    fn nika_code(&self) -> codes::NikaCode {
        match self {
            Self::DirtyReport => codes::NIKA_1700,
            Self::WaveOutOfBounds { .. } => codes::NIKA_1701,
            Self::UnresolvedTemplate { .. } => codes::NIKA_1702,
            Self::WhenUnsupported { .. } => codes::NIKA_1703,
        }
    }

    fn is_transient(&self) -> bool {
        // Contract breaches + static expression classes · retry never helps.
        false
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn all() -> [RuntimeError; 4] {
        [
            RuntimeError::DirtyReport,
            RuntimeError::WaveOutOfBounds {
                index: 9,
                task_count: 3,
            },
            RuntimeError::UnresolvedTemplate {
                reference: "tasks.ghost.output".into(),
            },
            RuntimeError::WhenUnsupported {
                expr: "vars.a ~= 1".into(),
            },
        ]
    }

    #[test]
    fn codes_unique_and_in_range() {
        let mut nums: Vec<u16> = all().iter().map(|e| e.nika_code().num).collect();
        nums.sort_unstable();
        nums.dedup();
        assert_eq!(nums.len(), 4, "duplicate code in the 1700 range");
        assert!(nums.iter().all(|n| (1700..1800).contains(n)));
    }

    #[test]
    fn display_carries_wire_code() {
        for err in all() {
            let wire = err.nika_code().to_string();
            assert!(
                err.to_string().starts_with(&wire),
                "code-first Display violated · {err}"
            );
        }
    }

    #[test]
    fn registry_lookup_resolves_every_code() {
        for err in all() {
            let wire = err.nika_code().to_string();
            assert!(
                nika_error::codes::lookup(&wire).is_some(),
                "code {wire} missing from the ALL registry"
            );
        }
    }

    #[test]
    fn never_transient() {
        for err in all() {
            assert!(!err.is_transient(), "{err} must not be retry-eligible");
        }
    }
}
