// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-dataflow` errors — the EVALUATION family.
//!
//! These four classes were `RuntimeError` variants until the descent at the
//! 15k wall. They travel with the code that raises them: every one is a
//! `${{ }}` island, a `cel-subset/0.1` expression or an `output:` jq binding
//! failing to resolve. The engine-internal identities stay
//! [`codes::NIKA_1702`]/[`codes::NIKA_1703`] (the `NIKA-170x` runtime range) —
//! moving the code did NOT move the range, because the range names the
//! *class*, not the crate. `nika-runtime` wraps this enum in its own
//! `RuntimeError::Dataflow` and delegates both code accessors, so the wire
//! form a consumer sees is byte-identical to before the descent.

use nika_error::codes;
use nika_error::traits::NikaErrorCode;

/// Expression + template + binding evaluation failures (spec 05 · spec 04
/// §binding rules).
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum DataflowError {
    /// A `${{ }}` reference did not resolve (unknown task id / var key ·
    /// out-of-range index · missing map key · the silent-literal guard).
    /// Wire code `NIKA-VAR-001` (`variable_error`, the unresolved-reference
    /// class · spec 05) · engine-internal [`codes::NIKA_1702`] via
    /// [`NikaErrorCode::nika_code`] for diagnostics. The wire code is what
    /// `tasks.X.error.code` exposes — never the 1702 (spec 05 §142).
    /// A `vars.*` reference carries the CLI fix (`--var key=value` · F4)
    /// — the first thing a user with an unbound required var needs.
    /// Post-C2: the hint follows the overridable authority (`inputs.*`).
    #[error(
        "NIKA-VAR-001 · unresolved template reference `{reference}`{}",
        var_cli_hint(.reference)
    )]
    #[diagnostic(code(nika::runtime::unresolved_template))]
    UnresolvedTemplate {
        /// The reference inside the island (e.g. `tasks.ghost.output`).
        reference: String,
    },

    /// A `when:` (or island) expression reached the runtime outside the v0
    /// gate subset. Wire code `NIKA-VAR-005` (`validation_error` · the
    /// checker is the primary site, the runtime is the defensive backstop) ·
    /// engine-internal [`codes::NIKA_1703`] for diagnostics.
    #[error("NIKA-VAR-005 · `when:` expression outside the v0 subset · `{expr}`")]
    #[diagnostic(code(nika::runtime::when_unsupported))]
    WhenUnsupported {
        /// The raw expression body.
        expr: String,
    },

    /// A `cel-subset/0.1` EVALUATION-time failure surfaced from
    /// [`nika_cel`] — a TYPE error (cross-type compare · non-boolean
    /// `when:` value · `for_each` over a non-array · `size()` of a
    /// scalar). The wire code is the SPEC-PLANE `NIKA-VAR-006` (the
    /// canon is the spec 05 table · resolvable via
    /// `nika_pack::error_codes()`), same plane as `NIKA-TIMEOUT-001`
    /// — NOT a `nika_error` registry range. Unknown references
    /// (`NIKA-VAR-001`) map to [`Self::UnresolvedTemplate`] (1702) and
    /// static-grammar violations (`NIKA-VAR-005`) to
    /// [`Self::WhenUnsupported`] (1703) instead — those are the two
    /// engine-internal classes; this carries the spec-plane type class.
    #[error("{code} · {message}")]
    #[diagnostic(code(nika::runtime::cel_eval))]
    CelEval {
        /// The spec wire code (`NIKA-VAR-006`).
        code: &'static str,
        /// The expression-relative message from `nika-cel`.
        message: String,
    },

    /// An `output:` named-binding evaluation failure (spec 04 §binding
    /// rules · the jq runs over the task's RAW output). The wire code is
    /// SPEC-PLANE (resolvable via `nika_pack::error_codes()`, same plane
    /// as [`Self::CelEval`]) ·
    /// - `NIKA-VAR-002` · the jq program emitted zero or MORE than one
    ///   value (a binding is single-valued · collect a stream with
    ///   `[ … ]` or take one with an index / `first(…)`).
    /// - `NIKA-VAR-004` · the jq program itself errored at runtime.
    ///
    /// A binding failure FAILS the task (it is evaluated before the
    /// terminal frame · a `TaskCompleted` becomes `TaskFailed`) · it
    /// never aborts the run.
    #[error("{code} · {message}")]
    #[diagnostic(code(nika::runtime::output_binding))]
    OutputBinding {
        /// The spec wire code (`NIKA-VAR-002` · `NIKA-VAR-004`).
        code: &'static str,
        /// The binding-relative message (which `<name>` · the jq cause).
        message: String,
    },
}

/// The actionable suffix for an unresolved `inputs.*` reference — the CLI
/// is the fix a user can apply WITHOUT editing the workflow (`--var` ·
/// F4). Non-`inputs` references (tasks · secrets · config · const) get no
/// suffix: their fixes are different classes.
fn var_cli_hint(reference: &str) -> &'static str {
    if reference.trim_start().starts_with("inputs.") {
        " — supply it with `nika run <file> --var <key>=<value>` or declare a `default:`"
    } else {
        ""
    }
}

impl DataflowError {
    /// The WIRE code a consumer filters on (`on_codes:` · the user-visible
    /// code). Every variant here is spec-plane: the engine-internal
    /// `nika_code()` (1702/1703) is diagnostics only and MUST NOT leak into
    /// `tasks.X.error` · run reports · conformance output (spec 05 §142).
    #[must_use]
    pub fn spec_code(&self) -> String {
        match self {
            Self::CelEval { code, .. } | Self::OutputBinding { code, .. } => (*code).to_owned(),
            // An unresolved `${{ }}` reference (unknown ns · out-of-range
            // index · missing map key · unprovided secret) is the spec-plane
            // NIKA-VAR-001 (variable_error) — NEVER the engine-internal
            // NIKA-1702.
            Self::UnresolvedTemplate { .. } => "NIKA-VAR-001".to_owned(),
            // An out-of-subset expression reaching the runtime is NIKA-VAR-005
            // (validation_error · the checker is the primary site).
            Self::WhenUnsupported { .. } => "NIKA-VAR-005".to_owned(),
        }
    }

    /// The human message WITHOUT the leading wire-code. `Display` is
    /// code-first (`"{code} · {text}"`) for engine logs · a `TaskErrorRecord`
    /// carries the code in its own field, so the record's `message` is the
    /// text alone — a consumer rendering `"{code} · {message}"` (the run
    /// report · the `TaskFailed` detail) then shows the code ONCE, not twice.
    #[must_use]
    pub fn wire_message(&self) -> String {
        let display = self.to_string();
        let prefix = format!("{} · ", self.spec_code());
        display.strip_prefix(&prefix).unwrap_or(&display).to_owned()
    }

    /// Map a [`nika_cel::CelError`] onto the evaluation plane — the ONE
    /// place the CEL conformance classes meet the runtime codes (spec 05) ·
    ///
    /// - `NIKA-VAR-001` (unresolved reference) → [`Self::UnresolvedTemplate`]
    ///   (NIKA-1702 · the silent-literal guard · the `reference` is the
    ///   author's island/expression text, never an injected value).
    /// - `NIKA-VAR-005` (static grammar violation) → [`Self::WhenUnsupported`]
    ///   (NIKA-1703 · genuinely outside `cel-subset/0.1` — the runtime is
    ///   the defensive backstop · the checker is the primary site).
    /// - `NIKA-VAR-006` (evaluation type error) → [`Self::CelEval`]
    ///   (the spec-plane `NIKA-VAR-006` wire code).
    ///
    /// `reference` is the AUTHOR's source text (the island body / gate
    /// body) — it is the text we parsed, NOT any runtime value, so the
    /// 1702 message can never leak an injected `${{ … }}` payload.
    #[must_use]
    pub fn from_cel(err: &nika_cel::CelError, reference: &str) -> Self {
        match err.kind() {
            nika_cel::CelErrorKind::Unresolved => Self::UnresolvedTemplate {
                reference: reference.to_owned(),
            },
            nika_cel::CelErrorKind::Type => Self::CelEval {
                code: err.spec_code(),
                message: err.message().to_owned(),
            },
            // `Static` (NIKA-VAR-005 · genuinely out of grammar) — and
            // any future #[non_exhaustive] CEL class — fail loudly as an
            // out-of-subset form (NIKA-1703) rather than mis-coding.
            _ => Self::WhenUnsupported {
                expr: reference.to_owned(),
            },
        }
    }
}

impl NikaErrorCode for DataflowError {
    fn nika_code(&self) -> codes::NikaCode {
        match self {
            Self::UnresolvedTemplate { .. } => codes::NIKA_1702,
            // CelEval + OutputBinding are spec-plane evaluation classes ·
            // at the engine-internal layer they share the "expression
            // couldn't be honored" family with WhenUnsupported (NIKA-1703
            // · all resolve in the nika_error registry). The user-facing
            // wire code is `spec_code()` (NIKA-VAR-00x), not this.
            Self::WhenUnsupported { .. } | Self::CelEval { .. } | Self::OutputBinding { .. } => {
                codes::NIKA_1703
            }
        }
    }

    fn is_transient(&self) -> bool {
        // Static expression classes · retry never helps.
        false
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn unresolved_input_display_keeps_the_actionable_cli_hint() {
        let input = DataflowError::UnresolvedTemplate {
            reference: "  inputs.api_base".to_owned(),
        };
        assert_eq!(
            input.to_string(),
            "NIKA-VAR-001 · unresolved template reference `  inputs.api_base` — supply it with `nika run <file> --var <key>=<value>` or declare a `default:`"
        );

        let task = DataflowError::UnresolvedTemplate {
            reference: "tasks.fetch.output".to_owned(),
        };
        assert_eq!(
            task.to_string(),
            "NIKA-VAR-001 · unresolved template reference `tasks.fetch.output`"
        );
    }

    #[test]
    fn wire_message_strips_exactly_one_code_prefix() {
        let cases = [
            (
                DataflowError::UnresolvedTemplate {
                    reference: "tasks.missing.output".to_owned(),
                },
                "unresolved template reference `tasks.missing.output`",
            ),
            (
                DataflowError::WhenUnsupported {
                    expr: "inputs.a &&".to_owned(),
                },
                "`when:` expression outside the v0 subset · `inputs.a &&`",
            ),
            (
                DataflowError::CelEval {
                    code: "NIKA-VAR-006",
                    message: "expected bool".to_owned(),
                },
                "expected bool",
            ),
            (
                DataflowError::OutputBinding {
                    code: "NIKA-VAR-004",
                    message: "output binding `answer` · boom".to_owned(),
                },
                "output binding `answer` · boom",
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(error.wire_message(), expected);
        }
    }

    #[test]
    fn evaluation_errors_are_never_transient() {
        let cases = [
            DataflowError::UnresolvedTemplate {
                reference: "x".to_owned(),
            },
            DataflowError::WhenUnsupported {
                expr: "x".to_owned(),
            },
            DataflowError::CelEval {
                code: "NIKA-VAR-006",
                message: "x".to_owned(),
            },
            DataflowError::OutputBinding {
                code: "NIKA-VAR-004",
                message: "x".to_owned(),
            },
        ];
        for error in cases {
            assert!(!error.is_transient(), "{error:?}");
        }
    }
}
