// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-runtime` errors — the NIKA-1700 range (`Category::Runtime`).
//!
//! These are RUN-ABORT classes only (contract breaches between the
//! checker and the runtime · template/gate static failures · launch
//! refusals like the admission preflight's NIKA-1708). A verb
//! failing inside a task is NOT a runtime error — it becomes a
//! `TaskFailed` event carrying the verb's own `nika_code()` and the run
//! continues per the cascade semantics (spec §3).

use nika_error::traits::NikaErrorCode;
use nika_kernel::prelude::codes;

/// Run-abort errors · the `NIKA-170x` range (spec §4).
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum RuntimeError {
    /// NIKA-1700 · a dirty [`nika_check::CheckReport`] reached
    /// `run` (audit-before-run violated · a dirty workflow never executes).
    #[error("NIKA-1700 · audit-before-run violated · the CheckReport is dirty")]
    #[diagnostic(code(nika::runtime::dirty_report))]
    DirtyReport,

    /// NIKA-1707 · the report's boundary lanes do not match the workflow
    /// bytes — the run-start re-derivation (the PURE permits-fit +
    /// trifecta lanes, re-run over the bytes) found something a clean
    /// report was credited with not having. The fail-closed backstop for
    /// LIBRARY embedders (the CLI re-checks right before run and never
    /// needs it): a clean report over DIFFERENT bytes is not clean.
    #[error(
        "NIKA-1707 · audit-before-run violated · the CheckReport does not match the workflow \
         bytes — re-check the file ({detail})"
    )]
    #[diagnostic(code(nika::runtime::report_mismatch))]
    ReportMismatch {
        /// What the re-derived boundary names (bounded — the full lanes
        /// are one `nika check` away).
        detail: String,
    },

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

    /// NIKA-1708 · a `required: true` input reached `run` with neither a
    /// declared `default:` nor a `--var` override — the ADMISSION
    /// preflight (issue #603) refuses the launch BEFORE the prologue
    /// (zero events · zero spend); the mid-DAG NIKA-VAR-001 at the first
    /// `${{ inputs.x }}` read was the bug. The launch-refusal family
    /// with [`Self::DirtyReport`]/[`Self::ReportMismatch`]: it aborts
    /// before the task pipeline, so the engine-internal code IS the
    /// wire form (never a `tasks.X.error` record).
    #[error("NIKA-1708 · {}", missing_required_message(.missing, .declared))]
    #[diagnostic(code(nika::runtime::missing_required_input))]
    MissingRequiredInputs {
        /// Each unsatisfied `required: true` input (declaration order).
        missing: Vec<String>,
        /// The full declared `inputs:` set (the satisfaction vocabulary).
        declared: Vec<String>,
    },

    /// NIKA-1709 · the run's unavoidable cost floor already exceeds the
    /// budget it was launched under (`--max-cost-usd`, or an inherited
    /// `min(parent remaining, child declared)` — spec 14 law 6). The
    /// ADMISSION form of the budget law: refuse BEFORE the prologue
    /// (zero events · zero spend). The CLI's preflight speaks this same
    /// constructor for the standalone surface; this gate is the
    /// fail-closed word for every OTHER embedder — a composed child
    /// included, which was the 2026-07-29 bypass (a child refused
    /// standalone RAN through a parent).
    #[error("NIKA-1709 · {message}")]
    #[diagnostic(code(nika::runtime::budget_floor))]
    BudgetFloor {
        /// The floor-vs-budget verdict (`floor_refusal`'s own text).
        message: String,
    },

    /// NIKA-1800 · no path survives admission under the `--access` pin
    /// — the pinned candidate itself failed a dimension, witnesses
    /// attached (D-2026-08-04-N1 · A-8). Launch-refusal family: before
    /// the prologue, zero events, zero spend; never fires without a pin.
    #[error("NIKA-1800 · {message}")]
    #[diagnostic(code(nika::runtime::access_no_path))]
    AccessNoPath {
        /// The witness lines (`<access> · <dimension> (<layer>) · <fix>`).
        message: String,
    },

    /// NIKA-1801 · the `--access` pin names a path this install does
    /// not offer — a pin is a pin: refusal, never a substitute (A-4).
    #[error("NIKA-1801 · {message}")]
    #[diagnostic(code(nika::runtime::access_pin_unsatisfied))]
    AccessPinUnsatisfied {
        /// Which model the pin cannot serve, and what it CAN use.
        message: String,
    },

    /// NIKA-1802 · the `--access` token is neither an access class nor
    /// an id this machine offers — refused before any resolution, so a
    /// typo never reads as an empty-candidate refusal.
    #[error("NIKA-1802 · {message}")]
    #[diagnostic(code(nika::runtime::access_unknown_token))]
    AccessUnknownToken {
        /// The rejected token and the vocabulary that exists here.
        message: String,
    },

    /// NIKA-1803 · a known agentic CLI token cannot run here — binary
    /// absent, ACP speaker missing, or this nika was built without
    /// adapters. Never an unknown-token 1802.
    #[error("NIKA-1803 · {message}")]
    #[diagnostic(code(nika::runtime::access_unavailable))]
    AccessUnavailable {
        /// Dummy-readable install / rebuild line.
        message: String,
    },

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

    /// The exec `decode:` pipeline failed to turn the captured bytes
    /// into a value (spec 09 §decode) — strict-UTF-8 text violation ·
    /// unparseable JSON/JSONL. Task-stage (inside `on_error:` scope ·
    /// « settles the task `failure`, honestly »); the engine-internal
    /// [`codes::NIKA_1705`] is the wire form until the spec registers a
    /// dedicated `NIKA-EXEC` row.
    #[error("NIKA-1705 · {message}")]
    #[diagnostic(code(nika::runtime::decode_failure))]
    Decode {
        /// What failed to decode and why (mode + cause + the fix).
        message: String,
    },

    /// A run-time contract violation (spec 09 · `NIKA-TYPE-101`) — the
    /// decoded value does not fit the task's `returns:` type. Task-stage
    /// (`exec:`/`invoke:` lane · `infer:`/`agent:` violations stay
    /// `NIKA-INFER-002`-class, one voice with the structured-output
    /// lane). Engine-internal identity [`codes::NIKA_1706`].
    #[error("NIKA-TYPE-101 · {message}")]
    #[diagnostic(code(nika::runtime::contract_violation))]
    ContractViolation {
        /// Which contract the value broke (task + type + value class).
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

/// The teaching text of [`RuntimeError::MissingRequiredInputs`] — names
/// EACH unsatisfied input, the `--var` satisfaction (the operator fix,
/// no workflow edit), the `default:` alternative, and the declared set
/// (the same vocabulary the CLI's unknown-`--var`-key refusal lists).
fn missing_required_message(missing: &[String], declared: &[String]) -> String {
    let names = missing
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(" · ");
    let fix = match missing.first() {
        Some(first) => format!("`--var {first}=<value>` satisfies it"),
        None => "`--var <name>=<value>` satisfies it".to_owned(),
    };
    let vocabulary = if declared.is_empty() {
        String::new()
    } else {
        format!(" · the workflow declares: {}", declared.join(" · "))
    };
    format!(
        "missing required inputs: {names} — a `required: true` input with no `default:` \
         must be supplied at launch ({fix}){vocabulary}"
    )
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
            // A contract violation is the SPEC-PLANE NIKA-TYPE-101 (spec
            // 09 §errors · registered in the canon table) — the 1706 is
            // its engine-internal identity only.
            Self::ContractViolation { .. } => "NIKA-TYPE-101".to_owned(),
            // An unresolved `${{ }}` reference (unknown ns · out-of-range
            // index · missing map key · unprovided secret) is the spec-plane
            // NIKA-VAR-001 (variable_error) — NEVER the engine-internal
            // NIKA-1702 (spec 05 §142: internal codes MUST NOT leak into
            // tasks.X.error · run reports · conformance output).
            Self::UnresolvedTemplate { .. } => "NIKA-VAR-001".to_owned(),
            // An out-of-subset expression reaching the runtime is NIKA-VAR-005
            // (validation_error · the checker is the primary site).
            Self::WhenUnsupported { .. } => "NIKA-VAR-005".to_owned(),
            // DirtyReport · ReportMismatch · WaveOutOfBounds ·
            // MissingRequiredInputs are launch refusals / invariant
            // breaches that abort the run before the task pipeline
            // (never a workflow-visible record) · their engine-internal
            // code is the only wire form.
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
            Self::ReportMismatch { .. } => codes::NIKA_1707,
            Self::WaveOutOfBounds { .. } => codes::NIKA_1701,
            Self::MissingRequiredInputs { .. } => codes::NIKA_1708,
            Self::BudgetFloor { .. } => codes::NIKA_1709,
            Self::AccessNoPath { .. } => codes::NIKA_1800,
            Self::AccessPinUnsatisfied { .. } => codes::NIKA_1801,
            Self::AccessUnknownToken { .. } => codes::NIKA_1802,
            Self::AccessUnavailable { .. } => codes::NIKA_1803,
            Self::UnresolvedTemplate { .. } => codes::NIKA_1702,
            // CelEval + OutputBinding are spec-plane evaluation classes ·
            // at the engine-internal layer they share the "expression
            // couldn't be honored" family with WhenUnsupported (NIKA-1703
            // · all resolve in the nika_error registry). The user-facing
            // wire code is `spec_code()` (NIKA-VAR-00x), not this.
            Self::WhenUnsupported { .. } | Self::CelEval { .. } | Self::OutputBinding { .. } => {
                codes::NIKA_1703
            }
            Self::Decode { .. } => codes::NIKA_1705,
            Self::ContractViolation { .. } => codes::NIKA_1706,
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

    fn all() -> [RuntimeError; 9] {
        [
            RuntimeError::DirtyReport,
            RuntimeError::ReportMismatch {
                detail: "capability escape · task `danger`".into(),
            },
            RuntimeError::WaveOutOfBounds {
                index: 9,
                task_count: 3,
            },
            RuntimeError::MissingRequiredInputs {
                missing: vec!["needle".into()],
                declared: vec!["needle".into(), "limit".into()],
            },
            RuntimeError::BudgetFloor {
                message: "refusing to start: floor $0.6 exceeds --max-cost-usd $0.000001".into(),
            },
            RuntimeError::UnresolvedTemplate {
                reference: "tasks.ghost.output".into(),
            },
            RuntimeError::WhenUnsupported {
                expr: "vars.a ~= 1".into(),
            },
            RuntimeError::Decode {
                message: "decode: json · expected value at line 1".into(),
            },
            RuntimeError::ContractViolation {
                message: "tasks.stats · the decoded value does not fit `returns:`".into(),
            },
        ]
    }

    #[test]
    fn codes_unique_and_in_range() {
        let mut nums: Vec<u16> = all().iter().map(|e| e.nika_code().num).collect();
        nums.sort_unstable();
        nums.dedup();
        assert_eq!(nums.len(), 9, "duplicate code in the 1700 range");
        assert!(nums.iter().all(|n| (1700..1800).contains(n)));
    }

    #[test]
    fn report_mismatch_is_the_dirty_reports_twin_class() {
        // The audit-before-run family: same wire shape as DirtyReport
        // (engine-internal code-first Display · never transient), the
        // message TEACHING the repair (re-check the file) and naming
        // what the re-derived boundary found.
        let err = RuntimeError::ReportMismatch {
            detail: "trifecta · task `leak`".into(),
        };
        assert_eq!(err.spec_code(), "NIKA-1707");
        assert_eq!(err.nika_code(), codes::NIKA_1707);
        let msg = err.to_string();
        assert!(msg.starts_with("NIKA-1707 · "), "{msg}");
        assert!(msg.contains("audit-before-run violated"), "{msg}");
        assert!(msg.contains("re-check the file"), "{msg}");
        assert!(msg.contains("task `leak`"), "the finding is named: {msg}");
        assert!(!err.is_transient(), "a forged report never retries");
    }

    #[test]
    fn missing_required_inputs_is_the_launch_refusal_class() {
        // Issue #603: the admission preflight's refusal TEACHES — every
        // unsatisfied input named, the `--var` satisfaction (the operator
        // fix, no workflow edit), the declared vocabulary. The wire form
        // is the engine-internal code (a launch refusal, never a
        // `tasks.X.error` record — the 1700/1707 posture).
        let err = RuntimeError::MissingRequiredInputs {
            missing: vec!["needle".into(), "region".into()],
            declared: vec!["needle".into(), "region".into(), "limit".into()],
        };
        assert_eq!(err.spec_code(), "NIKA-1708");
        assert_eq!(err.nika_code(), codes::NIKA_1708);
        let msg = err.to_string();
        assert!(msg.starts_with("NIKA-1708 · "), "{msg}");
        assert!(
            msg.contains("`needle`") && msg.contains("`region`"),
            "{msg}"
        );
        assert!(
            msg.contains("--var needle=<value>"),
            "the fix is named: {msg}"
        );
        assert!(msg.contains("default:"), "the author alternative: {msg}");
        assert!(
            msg.contains("the workflow declares: needle · region · limit"),
            "the declared set rides: {msg}"
        );
        assert!(!err.is_transient(), "an unsatisfied input never retries");
    }

    #[test]
    fn contract_violation_wires_the_spec_plane_type_101() {
        // The W3 runtime contract (spec 09 §errors): the WIRE code an
        // author filters on (`on_codes:` · `tasks.X.error.code`) is the
        // spec-plane NIKA-TYPE-101 — the 1706 stays engine-internal.
        let err = RuntimeError::ContractViolation {
            message: "x".into(),
        };
        assert_eq!(err.spec_code(), "NIKA-TYPE-101");
        assert_eq!(err.nika_code().num, 1706);
        // Decode failures keep the engine-internal wire form (no spec
        // row yet · the honest numeric — the InvalidParam precedent).
        let err = RuntimeError::Decode {
            message: "x".into(),
        };
        assert_eq!(err.spec_code(), "NIKA-1705");
        assert_eq!(err.nika_code().num, 1705);
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
    fn inputs_reference_carries_the_cli_fix_hint() {
        // F4: an unbound `inputs.*` reference must TEACH the fix the user
        // can apply without editing the workflow (`--var key=value`).
        let err = RuntimeError::UnresolvedTemplate {
            reference: "inputs.topic".into(),
        };
        let msg = err.to_string();
        assert!(msg.starts_with("NIKA-VAR-001"), "{msg}");
        assert!(msg.contains("--var"), "the CLI fix is named: {msg}");
        // Non-inputs references get no suffix — their fixes are different
        // classes (a ghost task id is a workflow bug, not a CLI miss).
        let task = RuntimeError::UnresolvedTemplate {
            reference: "tasks.ghost.output".into(),
        };
        assert!(!task.to_string().contains("--var"), "{task}");
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
