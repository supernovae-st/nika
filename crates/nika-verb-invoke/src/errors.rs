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
use nika_error::traits::{AsAny, NikaErrorCode};
use nika_kernel::tool_executor::{ToolErrorDetails, ToolErrorMeta, ToolExecError};

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
        /// The tool's OWN user-facing spec code when it surfaced one
        /// (`NIKA-BUILTIN-FETCH-001` · the identifier an author writes in
        /// `on_codes`). `None` for a tool that reported only text — then the
        /// engine code (`NIKA-451`) is the user-facing code. Carried via the
        /// tool-result error metadata seam (BUG-D).
        spec_code: Option<String>,
        /// Whether the tool deemed its failure retryable (HTTP 503/429 · DNS ·
        /// connection reset for `nika:fetch`). `false` for a text-only tool
        /// (no surfaced metadata) — the prior non-retryable behavior (BUG-D).
        transient: bool,
    },

    /// Tool dispatch failed (timeout · execution · unavailable).
    #[error("tool dispatch failed: {source}")]
    #[diagnostic(code(nika::verb::invoke_dispatch_failure))]
    Dispatch {
        /// The underlying kernel tool-exec error.
        #[source]
        source: ToolExecError,
    },

    /// The tool ran, reported an error AND surfaced typed failure details
    /// (`nika:fetch`: the status it received, the set it accepted). The same
    /// report as [`Self::ToolReportedError`] (message, codes and retry class
    /// are identical) plus the details a task error keeps
    /// (`tasks.X.error.details`). A separate variant keeps that one's fields
    /// source-compatible; read the details with [`Self::details`], or with
    /// [`Self::details_of`] through the engine's `&dyn NikaErrorCode` seam.
    #[error("tool `{tool}` reported an error: {content_tail}")]
    #[diagnostic(code(nika::verb::invoke_tool_reported_error))]
    #[non_exhaustive]
    ToolReportedDetailed {
        /// The tool id.
        tool: String,
        /// Tail of the tool's error content (capped).
        content_tail: String,
        /// The tool's own failure metadata: its spec code, its retry class
        /// and its typed details (present on this variant by construction).
        meta: ToolErrorMeta,
    },
}

impl VerbInvokeError {
    /// Build the tool-reported error with the content tail capped and no
    /// surfaced metadata (a text-only tool — the engine `NIKA-451` is the
    /// user-facing code, non-transient · the prior behavior).
    pub(crate) fn tool_reported(tool: impl Into<String>, content: &str) -> Self {
        Self::ToolReportedError {
            tool: tool.into(),
            content_tail: cap_tail(content),
            spec_code: None,
            transient: false,
        }
    }

    /// Build the tool-reported error carrying the tool's OWN spec code +
    /// retry class (the tool-result error metadata · BUG-D). `spec_code` is
    /// the `NIKA-BUILTIN-…` identifier the author filters on in `on_codes:`;
    /// `transient` lets a genuinely-transient tool failure be retried.
    pub(crate) fn tool_reported_coded(
        tool: impl Into<String>,
        content: &str,
        spec_code: Option<String>,
        transient: bool,
    ) -> Self {
        Self::ToolReportedError {
            tool: tool.into(),
            content_tail: cap_tail(content),
            spec_code,
            transient,
        }
    }

    /// Build the tool-reported error from the tool's whole failure metadata:
    /// the detailed variant when the tool surfaced typed details, else the
    /// coded report exactly as before.
    pub(crate) fn tool_reported_meta(
        tool: impl Into<String>,
        content: &str,
        meta: ToolErrorMeta,
    ) -> Self {
        if meta.details.is_none() {
            return Self::tool_reported_coded(tool, content, meta.spec_code, meta.transient);
        }
        Self::ToolReportedDetailed {
            tool: tool.into(),
            content_tail: cap_tail(content),
            meta,
        }
    }

    /// The tool's typed failure details, when it surfaced any.
    #[must_use]
    pub fn details(&self) -> Option<&ToolErrorDetails> {
        match self {
            Self::ToolReportedDetailed { meta, .. } => meta.details.as_ref(),
            _ => None,
        }
    }

    /// [`Self::details`] through the engine's verb-agnostic seam: the runtime
    /// settles every verb's error as a `&dyn NikaErrorCode`, and only an
    /// invoke error can carry a tool's details (`None` for any other type).
    #[must_use]
    pub fn details_of(err: &dyn NikaErrorCode) -> Option<&ToolErrorDetails> {
        AsAny::as_any(err)
            .downcast_ref::<Self>()
            .and_then(Self::details)
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
            Self::ToolReportedError { .. } | Self::ToolReportedDetailed { .. } => codes::NIKA_451,
            Self::Dispatch { .. } => codes::NIKA_452,
        }
    }

    /// The user-facing SPEC code (`spec/05-errors.md` · what `on_codes:`
    /// filters on). `NIKA-450` → `NIKA-INVOKE-001` (unknown tool). A
    /// tool-reported error (`NIKA-451`) carries the TOOL's own surfaced spec
    /// code when present (e.g. `NIKA-BUILTIN-FETCH-001` · the identifier an
    /// author writes in `on_codes` per the spec retry example), else its
    /// numeric wire form. `Dispatch` (`NIKA-452`) is the engine-internal
    /// `tool_error` class with no spec namespace row — numeric wire form.
    fn spec_code(&self) -> String {
        match self {
            Self::UnresolvableTool { .. } => "NIKA-INVOKE-001".to_owned(),
            // The tool's own spec code rides through when it surfaced one
            // (the BUILTIN sub-namespace the author filters on); else the
            // numeric wire form via the registry.
            Self::ToolReportedError {
                spec_code: Some(code),
                ..
            }
            | Self::ToolReportedDetailed {
                meta:
                    ToolErrorMeta {
                        spec_code: Some(code),
                        ..
                    },
                ..
            } => code.clone(),
            Self::ToolReportedError { .. }
            | Self::ToolReportedDetailed { .. }
            | Self::Dispatch { .. } => self.nika_code().to_string(),
        }
    }

    fn is_transient(&self) -> bool {
        match self {
            Self::UnresolvableTool { .. } => false,
            // A tool that ran and reported an error carries its OWN retry
            // class (BUG-D): `nika:fetch` marks HTTP 503/429 · DNS ·
            // connection failures transient so `retry:` works; 4xx-other ·
            // SSRF-block · bad-scheme stay false. A text-only tool surfaced
            // no metadata → `transient: false` (the prior behavior).
            Self::ToolReportedError { transient, .. } => *transient,
            Self::ToolReportedDetailed { meta, .. } => meta.transient,
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

    /// The detailed report says exactly what the coded one says (same text,
    /// codes and retry class) and carries the tool's typed details, readable
    /// directly and through the engine's `&dyn NikaErrorCode` seam.
    #[test]
    fn a_detailed_report_is_the_coded_report_plus_its_details() {
        let details = ToolErrorDetails::new()
            .with_status_code(503)
            .with_accepted(vec![200]);
        let meta = ToolErrorMeta::new(Some("NIKA-BUILTIN-FETCH-001".to_owned()), true);
        let content = "NIKA-BUILTIN-FETCH-001 · HTTP 503 from http://x.test/busy";
        let coded = VerbInvokeError::tool_reported_meta("nika:fetch", content, meta.clone());
        let detailed = VerbInvokeError::tool_reported_meta(
            "nika:fetch",
            content,
            meta.with_details(details.clone()),
        );
        assert!(
            matches!(coded, VerbInvokeError::ToolReportedError { .. }),
            "without details the report is the variant it always was"
        );
        assert!(matches!(
            detailed,
            VerbInvokeError::ToolReportedDetailed { .. }
        ));
        assert_eq!(detailed.to_string(), coded.to_string());
        assert_eq!(detailed.nika_code(), coded.nika_code());
        assert_eq!(detailed.spec_code(), "NIKA-BUILTIN-FETCH-001");
        assert_eq!(detailed.spec_code(), coded.spec_code());
        assert_eq!(detailed.is_transient(), coded.is_transient());
        assert_eq!(detailed.details(), Some(&details));
        assert_eq!(coded.details(), None);
        assert_eq!(VerbInvokeError::details_of(&detailed), Some(&details));
        assert_eq!(VerbInvokeError::details_of(&coded), None);
        // Without a tool code the engine code answers, exactly as for a coded report.
        let uncoded = VerbInvokeError::tool_reported_meta(
            "mcp:x/y",
            "boom",
            ToolErrorMeta::new(None, false)
                .with_details(ToolErrorDetails::new().with_status_code(500)),
        );
        assert_eq!(uncoded.spec_code(), "NIKA-451");
        assert!(!uncoded.is_transient());
    }

    /// Only an invoke error that carries details answers `details_of`: every
    /// other verb's error, and every other invoke report, is `None`.
    #[test]
    fn details_of_is_none_for_every_other_error() {
        #[derive(Debug, thiserror::Error, miette::Diagnostic)]
        #[error("a foreign verb error")]
        struct Foreign;
        impl NikaErrorCode for Foreign {
            fn nika_code(&self) -> NikaCode {
                codes::NIKA_450
            }
        }
        assert_eq!(VerbInvokeError::details_of(&Foreign), None);
        let text_only = VerbInvokeError::tool_reported("nika:x", "boom");
        assert_eq!(VerbInvokeError::details_of(&text_only), None);
        let dispatch = VerbInvokeError::Dispatch {
            source: ToolExecError::NotAvailable {
                reason: "down".to_owned(),
            },
        };
        assert_eq!(VerbInvokeError::details_of(&dispatch), None);
    }
}
