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

    /// A `${{ }}` reference did not resolve (unknown task id / var key ·
    /// out-of-range index · missing map key · the silent-literal guard).
    /// Wire code `NIKA-VAR-001` (`variable_error`, the unresolved-reference
    /// class · spec 05) · engine-internal [`codes::NIKA_1702`] via
    /// [`NikaErrorCode::nika_code`] for diagnostics. The wire code is what
    /// `tasks.X.error.code` exposes — never the 1702 (spec 05 §142).
    #[error("NIKA-VAR-001 · unresolved template reference `{reference}`")]
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

impl RuntimeError {
    /// The WIRE code a consumer filters on (`on_codes:` · the user-
    /// visible code). Defaults to the engine-internal `nika_code()` for
    /// the NIKA-170x family · returns the carried SPEC-PLANE code for
    /// [`Self::CelEval`] (`NIKA-VAR-006`, which is not a `nika_error`
    /// registry constant · its canon is the spec table).
    #[must_use]
    pub fn spec_code(&self) -> String {
        match self {
            Self::CelEval { code, .. } | Self::OutputBinding { code, .. } => (*code).to_owned(),
            // An unresolved `${{ }}` reference (unknown ns · out-of-range
            // index · missing map key · unprovided secret) is the spec-plane
            // NIKA-VAR-001 (variable_error) — NEVER the engine-internal
            // NIKA-1702 (spec 05 §142: internal codes MUST NOT leak into
            // tasks.X.error · run reports · conformance output).
            Self::UnresolvedTemplate { .. } => "NIKA-VAR-001".to_owned(),
            // An out-of-subset expression reaching the runtime is NIKA-VAR-005
            // (validation_error · the checker is the primary site).
            Self::WhenUnsupported { .. } => "NIKA-VAR-005".to_owned(),
            // DirtyReport · WaveOutOfBounds are engine invariant breaches that
            // abort the run before the task pipeline (never a workflow-visible
            // record) · their engine-internal code is the only wire form.
            other => other.nika_code().to_string(),
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

    /// Map a [`nika_cel::CelError`] onto the runtime's error plane —
    /// the ONE place the CEL conformance classes meet the runtime codes
    /// (spec 05) ·
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

impl NikaErrorCode for RuntimeError {
    fn nika_code(&self) -> codes::NikaCode {
        match self {
            Self::DirtyReport => codes::NIKA_1700,
            Self::WaveOutOfBounds { .. } => codes::NIKA_1701,
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
        // Code-first Display leads with the WIRE code a consumer sees
        // (`spec_code()`) — NIKA-VAR-001/005 for the eval classes, the
        // engine-internal 1700/1701 for the never-surfacing invariant
        // breaches (where wire == internal). NEVER the 1702/1703 internal
        // form for the eval classes (spec 05 §142 · the leak this closed).
        for err in all() {
            let wire = err.spec_code();
            assert!(
                err.to_string().starts_with(&wire),
                "code-first Display violated · {err} (wire {wire})"
            );
        }
    }

    #[test]
    fn eval_classes_expose_the_canonical_wire_code_not_the_internal_one() {
        // The CEL-2 leak: an unresolved ref / out-of-subset form must
        // surface its spec-plane code, never NIKA-1702/1703.
        let unresolved = RuntimeError::UnresolvedTemplate {
            reference: "vars.list[99]".into(),
        };
        assert_eq!(unresolved.spec_code(), "NIKA-VAR-001");
        assert_eq!(unresolved.nika_code(), codes::NIKA_1702); // internal intact
        assert!(!unresolved.to_string().contains("1702"));

        let out_of_subset = RuntimeError::WhenUnsupported {
            expr: "a ~= 1".into(),
        };
        assert_eq!(out_of_subset.spec_code(), "NIKA-VAR-005");
        assert_eq!(out_of_subset.nika_code(), codes::NIKA_1703);
        assert!(!out_of_subset.to_string().contains("1703"));

        // Both wire codes resolve in the embedded spec canon.
        let canon = nika_pack::error_codes();
        for code in ["NIKA-VAR-001", "NIKA-VAR-005"] {
            assert!(
                canon.iter().any(|row| row.code == code),
                "{code} must resolve in the embedded spec table"
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
        // The spec-plane carrier is never retryable either (type class).
        assert!(
            !RuntimeError::CelEval {
                code: "NIKA-VAR-006",
                message: "x".into(),
            }
            .is_transient()
        );
    }

    /// [`RuntimeError::from_cel`] is the ONE CEL→runtime mapping:
    /// VAR-001 → 1702 (the island/gate text, not an injected value) ·
    /// VAR-005 → 1703 · VAR-006 → the spec-plane [`RuntimeError::CelEval`]
    /// carrier.
    #[test]
    fn from_cel_maps_each_class_to_its_runtime_code() {
        let unresolved = nika_cel::CelError::unresolved("unresolved reference `vars`", (0, 4));
        assert!(matches!(
            RuntimeError::from_cel(&unresolved, "vars.nope"),
            RuntimeError::UnresolvedTemplate { ref reference } if reference == "vars.nope"
        ));

        let static_err = nika_cel::CelError::static_err("chained relation", (0, 3));
        assert!(matches!(
            RuntimeError::from_cel(&static_err, "a < b < c"),
            RuntimeError::WhenUnsupported { ref expr } if expr == "a < b < c"
        ));

        let type_err = nika_cel::CelError::type_err("cross-type compare", (0, 3));
        let mapped = RuntimeError::from_cel(&type_err, "vars.n == 'x'");
        assert!(matches!(
            mapped,
            RuntimeError::CelEval { code, .. } if code == "NIKA-VAR-006"
        ));
        assert_eq!(mapped.spec_code(), "NIKA-VAR-006");
    }

    /// The [`RuntimeError::CelEval`] carrier is code-first on its
    /// SPEC-PLANE code (the user-visible wire form) while its
    /// engine-internal `nika_code()` is the 1703 family (resolves in
    /// the `nika_error` registry).
    #[test]
    fn cel_eval_wire_codes_are_spec_plane_yet_registry_resolvable() {
        let err = RuntimeError::CelEval {
            code: "NIKA-VAR-006",
            message: "for_each collection must be an array".into(),
        };
        assert_eq!(err.spec_code(), "NIKA-VAR-006");
        assert!(err.to_string().starts_with("NIKA-VAR-006 · "), "{err}");
        // Engine-internal classification still resolves in the registry.
        assert_eq!(err.nika_code(), codes::NIKA_1703);
        assert!(nika_error::codes::lookup(&err.nika_code().to_string()).is_some());
        // …and the SPEC-PLANE code resolves in the embedded spec canon.
        assert!(
            nika_pack::error_codes()
                .iter()
                .any(|row| row.code == err.spec_code()),
            "NIKA-VAR-006 must resolve in the embedded spec table"
        );
    }
}
