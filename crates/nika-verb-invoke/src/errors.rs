// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `VerbInvokeError` — the `invoke` verb error surface (NIKA-450..452).
//!
//! Constants are registry-owned in `nika_error::codes`. Spec anchoring:
//! NIKA-450 maps to spec `NIKA-INVOKE-001` (unresolvable tool id). NIKA-451
//! (the tool ran but returned `is_error: true`) has **no spec counterpart**
//! — the spec assigns INVOKE-001 to unknown-tool and INVOKE-002 to
//! args-schema, neither of which is a tool-runtime error; 451 is an
//! engine-internal code. NIKA-452 wraps the kernel `ToolExecError`
//! (`tool_error` class). NIKA-INVOKE-002 (args-schema) is reserved (§2) —
//! the tool owns its schema today.

use nika_error::codes::{self, NikaCode};
use nika_error::traits::NikaErrorCode;
use nika_kernel::tool_executor::ToolExecError;

/// How much of a tool's error content the error carries.
const CONTENT_TAIL_CAP: usize = 1024;

/// Errors from the `invoke` verb executor.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum VerbInvokeError {
    /// The tool id did not resolve (bad namespace · `mcp:` missing the
    /// slash · unknown builtin or server reaching the dispatcher).
    #[error("tool `{tool}` did not resolve: {detail}")]
    #[diagnostic(code(nika::verb::invoke_unresolvable_tool))]
    UnresolvableTool {
        /// The tool id as received.
        tool: String,
        /// Why it did not resolve.
        detail: String,
    },

    /// The tool ran but reported an error (`ToolResult.is_error == true`).
    #[error("tool `{tool}` reported an error: {content_tail}")]
    #[diagnostic(code(nika::verb::invoke_tool_reported_error))]
    ToolReportedError {
        /// The tool id.
        tool: String,
        /// Tail of the tool's error content (capped).
        content_tail: String,
    },

    /// Tool dispatch failed (timeout · execution · unavailable).
    #[error("tool dispatch failed: {source}")]
    #[diagnostic(code(nika::verb::invoke_dispatch_failure))]
    Dispatch {
        /// The underlying kernel tool-exec error.
        #[source]
        source: ToolExecError,
    },
}

impl VerbInvokeError {
    /// Build the tool-reported error with the content tail capped.
    pub(crate) fn tool_reported(tool: impl Into<String>, content: &str) -> Self {
        Self::ToolReportedError {
            tool: tool.into(),
            content_tail: cap_tail(content),
        }
    }
}

/// Keep the last `CONTENT_TAIL_CAP` bytes, walking to a char boundary.
fn cap_tail(s: &str) -> String {
    if s.len() <= CONTENT_TAIL_CAP {
        return s.to_owned();
    }
    let mut start = s.len() - CONTENT_TAIL_CAP;
    while !s.is_char_boundary(start) {
        start += 1;
    }
    format!("…{}", &s[start..])
}

impl NikaErrorCode for VerbInvokeError {
    fn nika_code(&self) -> NikaCode {
        match self {
            Self::UnresolvableTool { .. } => codes::NIKA_450,
            Self::ToolReportedError { .. } => codes::NIKA_451,
            Self::Dispatch { .. } => codes::NIKA_452,
        }
    }

    fn is_transient(&self) -> bool {
        match self {
            Self::UnresolvableTool { .. } | Self::ToolReportedError { .. } => false,
            // Inherit the dispatcher's classification (a timeout MAY be
            // transient once the kernel marks it so · terminal today).
            Self::Dispatch { source } => source.is_transient(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_match_the_registry() {
        let cases: Vec<(VerbInvokeError, NikaCode)> = vec![
            (
                VerbInvokeError::UnresolvableTool {
                    tool: "nika:ghost".to_owned(),
                    detail: "unknown".to_owned(),
                },
                codes::NIKA_450,
            ),
            (
                VerbInvokeError::tool_reported("mcp:db/query", "boom"),
                codes::NIKA_451,
            ),
            (
                VerbInvokeError::Dispatch {
                    source: ToolExecError::NotAvailable {
                        reason: "down".to_owned(),
                    },
                },
                codes::NIKA_452,
            ),
        ];
        for (err, expected) in cases {
            assert_eq!(err.nika_code(), expected, "{err}");
            assert_eq!(codes::lookup(&expected.to_string()), Some(expected));
        }
    }

    #[test]
    fn content_tail_boundary_walk_lands_on_the_next_char() {
        // 2-byte `é` second byte at len - CAP → walk steps past it.
        let run = "T".repeat(CONTENT_TAIL_CAP - 1);
        let err = VerbInvokeError::tool_reported("nika:x", &format!("xxé{run}"));
        match err {
            VerbInvokeError::ToolReportedError { content_tail, .. } => {
                assert_eq!(content_tail, format!("…{run}"));
                assert!(!content_tail.contains('é'));
                assert!(!content_tail.contains('x'));
            }
            other => panic!("expected ToolReportedError, got {other:?}"),
        }
    }

    #[test]
    fn short_content_passes_through() {
        let err = VerbInvokeError::tool_reported("nika:x", "brief");
        match err {
            VerbInvokeError::ToolReportedError { content_tail, .. } => {
                assert_eq!(content_tail, "brief");
            }
            other => panic!("expected ToolReportedError, got {other:?}"),
        }
    }

    #[test]
    fn transience_is_terminal_today_with_forward_compat_delegation() {
        assert!(
            !VerbInvokeError::UnresolvableTool {
                tool: "x".to_owned(),
                detail: String::new(),
            }
            .is_transient()
        );
        assert!(!VerbInvokeError::tool_reported("x", "").is_transient());
        // Equivalent-mutant note: every ToolExecError variant is terminal
        // today, so the Dispatch branch is honestly always-false until the
        // kernel marks a tool error transient.
        let timeout = || ToolExecError::Timeout {
            name: "t".to_owned(),
            duration_ms: 5,
        };
        assert_eq!(
            VerbInvokeError::Dispatch { source: timeout() }.is_transient(),
            timeout().is_transient()
        );
    }
}
