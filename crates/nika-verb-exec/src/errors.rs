// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `VerbExecError` — the `exec` verb error surface (NIKA-440..442).
//!
//! Constants are registry-owned in `nika_error::codes`; the spec-level
//! rows are `NIKA-EXEC-001/002` (`nika-spec spec/05-errors.md`) — 440
//! maps to 001 (non-zero exit), 441 maps to 002 (spawn/shell failure).
//! NIKA-442 (`InvalidParam`) has **no spec counterpart** — it is an
//! engine-side guard refusing corrupting input BEFORE the shell call.

use nika_error::codes::{self, NikaCode};
use nika_error::traits::NikaErrorCode;
use nika_kernel::process::ShellError;

/// How much trailing stderr the non-zero-exit error carries.
const STDERR_TAIL_CAP: usize = 1024;

/// Errors from the `exec` verb executor.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum VerbExecError {
    /// The command exited non-zero in a default capture mode
    /// (`capture: structured` returns the exit code as data instead).
    #[error("command exited with status {status}: {stderr_tail}")]
    #[diagnostic(code(nika::verb::exec_non_zero_exit))]
    NonZeroExit {
        /// Process exit code.
        status: i32,
        /// Tail of captured stderr (capped · diagnostic context).
        stderr_tail: String,
    },

    /// Shell execution failed (spawn · blocklist · timeout · cancel).
    #[error("shell execution failed: {source}")]
    #[diagnostic(code(nika::verb::exec_shell_failure))]
    Shell {
        /// The underlying kernel shell error.
        #[source]
        source: ShellError,
    },

    /// An `exec` parameter is invalid (empty command).
    #[error("invalid `exec` parameter `{param}`: {detail}")]
    #[diagnostic(code(nika::verb::exec_invalid_param))]
    InvalidParam {
        /// Which parameter failed validation.
        param: &'static str,
        /// Why it failed.
        detail: String,
    },
}

impl VerbExecError {
    /// Build the non-zero-exit error with the stderr tail capped.
    pub(crate) fn non_zero(status: i32, stderr: &str) -> Self {
        let tail = if stderr.len() > STDERR_TAIL_CAP {
            let mut start = stderr.len() - STDERR_TAIL_CAP;
            while !stderr.is_char_boundary(start) {
                start += 1;
            }
            format!("…{}", &stderr[start..])
        } else {
            stderr.to_owned()
        };
        Self::NonZeroExit {
            status,
            stderr_tail: tail,
        }
    }
}

impl NikaErrorCode for VerbExecError {
    fn nika_code(&self) -> NikaCode {
        match self {
            Self::NonZeroExit { .. } => codes::NIKA_440,
            Self::Shell { .. } => codes::NIKA_441,
            Self::InvalidParam { .. } => codes::NIKA_442,
        }
    }

    /// The user-facing SPEC code (`spec/05-errors.md` · what `on_codes:`
    /// filters on). `NIKA-440` → `NIKA-EXEC-001` (non-zero exit) · `NIKA-441`
    /// → `NIKA-EXEC-002` (spawn/shell failure). `InvalidParam` has NO spec
    /// row (an engine-side guard before the shell call) — it keeps its
    /// numeric wire form via the trait default.
    fn spec_code(&self) -> String {
        match self {
            Self::NonZeroExit { .. } => "NIKA-EXEC-001".to_owned(),
            // A blocklist hit is a SECURITY stop — spec `NIKA-SEC-001`
            // (`security_error`), NOT a spawn/process failure. Routing it
            // distinctly is what makes `on_error.on_codes: [NIKA-SEC-001]`
            // and audit-by-category work (the kernel already labels the
            // message "security blocklist"). The other `Shell` sources
            // (spawn · timeout · cancel) stay `NIKA-EXEC-002`.
            Self::Shell {
                source: ShellError::Blocked { .. },
            } => "NIKA-SEC-001".to_owned(),
            Self::Shell { .. } => "NIKA-EXEC-002".to_owned(),
            Self::InvalidParam { .. } => self.nika_code().to_string(),
        }
    }

    fn is_transient(&self) -> bool {
        match self {
            // A non-zero exit is the command's verdict — rerunning the same
            // command is workflow policy (`retry:`), not error transience.
            Self::NonZeroExit { .. } | Self::InvalidParam { .. } => false,
            // Forward-compat delegation: today every `ShellError` variant is
            // terminal (the kernel default), so this is always `false` — a
            // mutation to a literal `false` is EQUIVALENT until the kernel
            // marks a variant (e.g. a future retryable Timeout) transient,
            // at which point this delegation starts to matter.
            Self::Shell { source } => source.is_transient(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_match_the_registry() {
        let cases: Vec<(VerbExecError, NikaCode)> = vec![
            (VerbExecError::non_zero(2, "boom"), codes::NIKA_440),
            (
                VerbExecError::Shell {
                    source: ShellError::Other {
                        reason: "x".to_owned(),
                    },
                },
                codes::NIKA_441,
            ),
            (
                VerbExecError::InvalidParam {
                    param: "command",
                    detail: "empty".to_owned(),
                },
                codes::NIKA_442,
            ),
        ];
        for (err, expected) in cases {
            assert_eq!(err.nika_code(), expected, "{err}");
            assert_eq!(codes::lookup(&expected.to_string()), Some(expected));
        }
    }

    #[test]
    fn blocklist_hit_speaks_nika_sec_001_not_exec_002() {
        // A kernel blocklist hit (`ShellError::Blocked`) is a SECURITY stop:
        // it MUST surface as NIKA-SEC-001 (security_error · spec 05) so
        // `on_error.on_codes: [NIKA-SEC-001]` and audit-by-category work —
        // NOT NIKA-EXEC-002 (process_error), which conflates it with a spawn
        // failure (the bug 3 coverage agents independently hit).
        let blocked = VerbExecError::Shell {
            source: ShellError::Blocked {
                reason: "command matches security blocklist".to_owned(),
            },
        };
        assert_eq!(blocked.spec_code(), "NIKA-SEC-001");
        // A non-blocklist Shell error stays NIKA-EXEC-002 (spawn/other).
        let other = VerbExecError::Shell {
            source: ShellError::Other {
                reason: "spawn failed".to_owned(),
            },
        };
        assert_eq!(other.spec_code(), "NIKA-EXEC-002");
        // A security verdict is never transient — a blind retry replays it.
        assert!(!blocked.is_transient());
    }

    fn tail_of(err: VerbExecError) -> String {
        match err {
            VerbExecError::NonZeroExit { stderr_tail, .. } => stderr_tail,
            other => panic!("expected NonZeroExit, got {other:?}"),
        }
    }

    #[test]
    fn stderr_tail_boundary_walk_lands_on_the_next_char() {
        // Position a 2-byte `é` so its SECOND byte sits exactly at
        // `len - CAP`: the raw cut is mid-char, the forward walk must step
        // to the char AFTER it. Layout: "xx" + "é" + "T"*(CAP-1).
        //   len = 2 + 2 + (CAP-1) = CAP + 3 · raw start = len - CAP = 3
        //   byte 3 = é's 2nd byte → not a boundary → walk to 4 (the T run).
        let tail_run = "T".repeat(STDERR_TAIL_CAP - 1);
        let input = format!("xxé{tail_run}");
        let tail = tail_of(VerbExecError::non_zero(1, &input));
        // The kept body is EXACTLY the T run — é dropped, no x's. A `-=`
        // mutant keeps é (start=2), a `*=` mutant overshoots — both differ.
        assert_eq!(tail, format!("…{tail_run}"));
        assert!(!tail.contains('é'), "the straddling char is dropped");
        assert!(!tail.contains('x'), "the head is dropped");
    }

    #[test]
    fn stderr_exactly_at_cap_is_not_truncated() {
        // Boundary: len == cap takes the else branch (`>` not `>=`).
        let at_cap = "y".repeat(STDERR_TAIL_CAP);
        let tail = tail_of(VerbExecError::non_zero(1, &at_cap));
        assert_eq!(tail, at_cap, "no ellipsis at exactly the cap");
        // One over the cap DOES truncate.
        let over = "y".repeat(STDERR_TAIL_CAP + 1);
        assert!(tail_of(VerbExecError::non_zero(1, &over)).starts_with('…'));
    }

    #[test]
    fn short_stderr_passes_through_untouched() {
        assert_eq!(
            tail_of(VerbExecError::non_zero(1, "just this")),
            "just this"
        );
    }

    #[test]
    fn spec_code_maps_to_the_user_facing_form() {
        // The on_codes:/tasks.X.error.code identifier (BUG-C) — the spec
        // namespace form the author is forced (by `nika check`) to write,
        // NOT the engine wire code.
        assert_eq!(
            VerbExecError::non_zero(2, "boom").spec_code(),
            "NIKA-EXEC-001"
        );
        assert_eq!(
            VerbExecError::Shell {
                source: ShellError::Other {
                    reason: "x".to_owned(),
                },
            }
            .spec_code(),
            "NIKA-EXEC-002"
        );
        // InvalidParam has NO spec row → numeric wire form (the default).
        assert_eq!(
            VerbExecError::InvalidParam {
                param: "command",
                detail: "empty".to_owned(),
            }
            .spec_code(),
            "NIKA-442"
        );
    }

    #[test]
    fn transience_classification() {
        assert!(!VerbExecError::non_zero(1, "").is_transient());
        assert!(
            !VerbExecError::InvalidParam {
                param: "command",
                detail: String::new(),
            }
            .is_transient()
        );
        // Shell transience is inherited from the kernel error, both branches.
        let timeout = || ShellError::Timeout { duration_ms: 5000 };
        let other = || ShellError::Other {
            reason: "spawn".to_owned(),
        };
        assert_eq!(
            VerbExecError::Shell { source: timeout() }.is_transient(),
            timeout().is_transient()
        );
        assert_eq!(
            VerbExecError::Shell { source: other() }.is_transient(),
            other().is_transient()
        );
    }
}
