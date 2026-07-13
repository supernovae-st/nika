// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Spec-facing error codes — `NIKA-<NAMESPACE>-<NNN>` (spec
//! `05-errors.md` §code format · regex
//! `^NIKA-[A-Z]{2,9}(-[A-Z]{2,9})?-[0-9]{3}$`).
//!
//! ORTHOGONAL to the engine-internal [`NikaCode`](nika_error::codes::NikaCode)
//! surface (gate-12 mechanism · `Category::Schema` 280-329) · the spec
//! codes are the AUTHOR-facing contract the conformance suite matches.
//!
//! Namespace allocation (per the Core conformance fixtures) ·
//! - `PARSE` · structural/shape errors (YAML · envelope · task shape ·
//!   verbs · the `workflow.schema.json`-checkable layer).
//! - (the former `PARSE-WHEN` sub-namespace is RETIRED — the spec
//!   folded the static `when:` shape gate into `NIKA-VAR-005`).
//! - `DAG` · topology (cycle · unresolved dep · missing edge).
//! - `VAR` · the `${{ }}` substitution surface — both malformed
//!   substitution syntax (fixture `variables/011` · « the YAML itself
//!   parses fine — the defect is in the variable-substitution
//!   grammar » · `validation_error`) and reference RESOLUTION
//!   failures (`NIKA-VAR-001` · `variable_error`).

use std::fmt;

use super::SchemaError;

/// The closed error-category enum (spec `05-errors.md` §categories ·
/// `snake_case` on the wire).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SpecCategory {
    /// Malformed YAML / unparseable input.
    ParseError,
    /// A spec-rule violation in well-formed input.
    ValidationError,
    /// Reference resolution / substitution-surface defects.
    VariableError,
    /// LLM provider errors (runtime · reserved here).
    ProviderError,
    /// Network errors (runtime · reserved here).
    NetworkError,
    /// Tool invocation errors (runtime · reserved here).
    ToolError,
    /// Security violations (runtime · reserved here).
    SecurityError,
    /// Timeouts (runtime · reserved here).
    TimeoutError,
    /// Cancellation (runtime · reserved here).
    Cancelled,
    /// Engine bugs (runtime · reserved here).
    InternalError,
}

impl SpecCategory {
    /// The wire form (`snake_case` · matches `expected.json` `category`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ParseError => "parse_error",
            Self::ValidationError => "validation_error",
            Self::VariableError => "variable_error",
            Self::ProviderError => "provider_error",
            Self::NetworkError => "network_error",
            Self::ToolError => "tool_error",
            Self::SecurityError => "security_error",
            Self::TimeoutError => "timeout_error",
            Self::Cancelled => "cancelled",
            Self::InternalError => "internal_error",
        }
    }
}

/// A spec-facing error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct SpecCode {
    /// The namespace segment(s) (`PARSE` · `PARSE-WHEN` · `DAG` · `VAR`).
    pub namespace: &'static str,
    /// The 3-digit number within the namespace.
    pub num: u16,
    /// The closed category.
    pub category: SpecCategory,
    /// Whether retry may help (statically-detected errors never are).
    pub transient: bool,
}

impl SpecCode {
    /// Create a spec code.
    #[must_use]
    pub fn new(namespace: &'static str, num: u16, category: SpecCategory) -> Self {
        Self {
            namespace,
            num,
            category,
            transient: false,
        }
    }
}

impl fmt::Display for SpecCode {
    /// Renders the canonical `NIKA-<NAMESPACE>-<NNN>` form.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NIKA-{}-{:03}", self.namespace, self.num)
    }
}

// ── The canonical allocations ───────────────────────────────────────

const fn parse(num: u16, category: SpecCategory) -> SpecCode {
    SpecCode {
        namespace: "PARSE",
        num,
        category,
        transient: false,
    }
}

const fn dag(num: u16) -> SpecCode {
    SpecCode {
        namespace: "DAG",
        num,
        category: SpecCategory::ValidationError,
        transient: false,
    }
}

const fn var(num: u16, category: SpecCategory) -> SpecCode {
    SpecCode {
        namespace: "VAR",
        num,
        category,
        transient: false,
    }
}

impl SchemaError {
    /// Map to the spec-facing code (spec `05-errors.md`).
    ///
    /// Exhaustive — a new variant fails compilation until mapped.
    #[must_use]
    pub fn spec_code(&self) -> SpecCode {
        use SpecCategory::{ParseError, ValidationError, VariableError};
        match self {
            // ── NIKA-PARSE · structural / shape ─────────────────────
            Self::YamlSyntax { .. } => parse(1, ParseError),
            Self::MissingEnvelopeField { .. } => parse(2, ValidationError),
            // Fixture envelope/003 · bad `nika:` is parse_error (the
            // version marker gate is the file's entry contract).
            Self::BadNikaVersion { .. } => parse(3, ParseError),
            Self::BadWorkflowId { .. } => parse(4, ValidationError),
            Self::UnknownField { .. } => parse(5, ValidationError),
            Self::BadTaskId { .. } => parse(6, ValidationError),
            Self::DuplicateTaskId { .. } => parse(7, ValidationError),
            Self::MissingVerb { .. } => parse(8, ValidationError),
            Self::MultipleVerbs { .. } => parse(9, ValidationError),
            Self::BadTimeout { .. } => parse(10, ValidationError),
            Self::BadRetry { .. } => parse(11, ValidationError),
            Self::BadOnError { .. } => parse(12, ValidationError),
            // Fixture variables/009 · « NIKA-PARSE (not NIKA-VAR)
            // because the rule is schema-checkable structure ».
            Self::ReservedBindingName { .. } => parse(13, ValidationError),
            Self::BadSecretRef { .. } => parse(14, ValidationError),
            Self::BadTypedVar { .. } => parse(15, ValidationError),

            Self::DuplicateKey { .. } => parse(17, ValidationError),
            Self::MissingField { .. } => parse(18, ValidationError),
            Self::Validation { .. } => parse(19, ValidationError),
            // W1 « the map » migration teachings (dead forms · 0.104)
            Self::W1WorkflowScalar { .. } => parse(20, ValidationError),
            Self::W1TopLevelDescription { .. } => parse(21, ValidationError),
            Self::W1TasksSequence { .. } => parse(22, ValidationError),
            Self::W1TaskIdField { .. } => parse(23, ValidationError),

            // Spec 05 registry · NIKA-VAR-005 = « static expression
            // violation — outside cel-subset/0.1 · chained relation ·
            // unknown function · non-boolean when: root · jq compile
            // error ». The class spans the non-boolean `when:` shape gate
            // (the retired NIKA-PARSE-WHEN-001 folded here), `${{ }}` inside
            // an output binding (04 §binding rules · deep fixtures 003/007/008),
            // AND a CLOSED `${{ }}` island whose CEL is outside the subset
            // (chained relation · unknown function · arithmetic · stray token
            // — `ExpressionViolation`, distinct from the unclosed-opener
            // VAR-008 below).
            Self::WhenNotBoolean { .. }
            | Self::JqBindingContainsTemplate { .. }
            | Self::ExpressionViolation { .. } => var(5, ValidationError),

            // ── NIKA-DAG · topology ─────────────────────────────────
            Self::Cycle { .. } => dag(1),
            Self::UnknownDependency { .. } => dag(2),
            Self::MissingDependsOnEdge { .. } => dag(3),
            Self::RecoverAwaitDeadlock { .. } => dag(4),

            // ── NIKA-BUILTIN · arg-shape contracts ── nika:done carries
            // its REGISTERED exact code (deep fixture 010 matches on it) ·
            // the other shapes emit the generic builtin namespace.
            Self::BadBuiltinArgs { tool, .. } if tool == "nika:done" => SpecCode {
                namespace: "BUILTIN-DONE",
                num: 1,
                category: ValidationError,
                transient: false,
            },
            Self::BadBuiltinArgs { .. } => SpecCode {
                namespace: "BUILTIN",
                num: 1,
                category: ValidationError,
                transient: false,
            },

            // ── NIKA-VAR · the ${{ }} surface ───────────────────────
            // NIKA-VAR-001 · reference resolution (fixtures 001-007 ·
            // variable_error) · loop-locals + unknown task fields are
            // the same unresolved class per the fixture notes.
            Self::UnresolvedNamespaceRef { .. }
            | Self::LoopLocalOutsideForEach { .. }
            | Self::UnknownTaskField { .. } => var(1, VariableError),
            // Spec 05 registry · NIKA-VAR-008 = unclosed/malformed
            // `${{` opener (« the YAML itself parses fine » · fixture
            // variables/011 matches on namespace) — VAR-002 is the
            // RUNTIME binding-cardinality code · never static.
            Self::TemplateSyntax { .. } => var(8, ValidationError),
            // NIKA-VAR-003 · static binding validation (04 §Static
            // binding validation · fixture variables/012) · the
            // category table's « invalid path » class.
            Self::OutputPathProvablyInvalid { .. } => var(3, VariableError),
            // NIKA-VAR-020 · bare `tasks.X` is the envelope, not a value
            // (04 §namespaces · 0.103 · #75 D2 · spec fixture
            // variables/020 matches on namespace + validation_error).
            Self::BareTaskEnvelope { .. } => var(20, ValidationError),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::all_error_variants;

    #[test]
    fn display_renders_canonical_form() {
        assert_eq!(dag(1).to_string(), "NIKA-DAG-001");
        assert_eq!(
            var(1, SpecCategory::VariableError).to_string(),
            "NIKA-VAR-001"
        );
        assert_eq!(dag(4).to_string(), "NIKA-DAG-004");
        assert_eq!(
            parse(17, SpecCategory::ValidationError).to_string(),
            "NIKA-PARSE-017"
        );
    }

    #[test]
    fn every_code_matches_the_spec_regex() {
        // Spec 05 · ^NIKA-[A-Z]{2,9}(-[A-Z]{2,9})?-[0-9]{3}$ — verified
        // with the same hand-rolled validator retry on_codes uses.
        for err in &all_error_variants() {
            let code = err.spec_code().to_string();
            assert!(
                crate::types::is_valid_error_code(&code),
                "{code} violates the spec code regex"
            );
        }
    }

    #[test]
    fn statically_detected_errors_are_never_transient() {
        for err in &all_error_variants() {
            assert!(!err.spec_code().transient, "{err:?}");
        }
    }

    #[test]
    fn every_emittable_code_is_registered_in_the_canon() {
        // THE RATCHET (emitted ⊆ registered) — the gap class this kills:
        // the checker emitted the whole NIKA-PARSE namespace plus the
        // generic NIKA-BUILTIN-001 for weeks while the spec registry
        // (canon error_codes · spec/05-errors.md normative floor) did
        // not list them — `nika explain` had nothing to teach and a
        // second engine could not match parse-time behavior from the
        // spec alone. Both sides DERIVED: the variant enumerator on the
        // left, the typed registry accessor on the right (THE one
        // parser · its contract pinned in nika-pack's seam tests) — a
        // new error variant whose code lacks a registry row fails HERE,
        // at the crate that introduces it, before any release.
        let registered: std::collections::BTreeSet<&str> = nika_pack::error_codes()
            .into_iter()
            .map(|row| row.code)
            .collect();
        assert!(
            registered.len() >= 30,
            "canon registry parse broke — {} rows (the table is never this small)",
            registered.len()
        );
        for err in &all_error_variants() {
            let code = err.spec_code().to_string();
            assert!(
                registered.contains(code.as_str()),
                "{code} is emitted by the checker but NOT registered in the canon \
                 error_codes table — add the row to spec canon.yaml + \
                 spec/05-errors.md (the table is the SSOT · the engine derives), \
                 then re-run crates/nika-pack/scripts/sync-pack.sh"
            );
        }
    }

    #[test]
    fn fixture_critical_mappings() {
        use crate::error::SchemaError;
        // envelope/003 · bad nika version → NIKA-PARSE + parse_error.
        let code = SchemaError::BadNikaVersion {
            version: "v1.0".into(),
            span: None,
        }
        .spec_code();
        assert_eq!(code.namespace, "PARSE");
        assert_eq!(code.category.as_str(), "parse_error");

        // dag-topology/001 · cycle → exact NIKA-DAG-001.
        let code = SchemaError::Cycle { cycle: vec![] }.spec_code();
        assert_eq!(code.to_string(), "NIKA-DAG-001");
        assert_eq!(code.category.as_str(), "validation_error");

        // variables/003 · unresolved → NIKA-VAR-001 + variable_error.
        let code = SchemaError::UnresolvedNamespaceRef {
            reference: "vars.topik".into(),
            location: "task `go`".into(),
            suggestion: None,
            span: None,
        }
        .spec_code();
        assert_eq!(code.to_string(), "NIKA-VAR-001");
        assert_eq!(code.category.as_str(), "variable_error");

        // variables/009 · reserved binding name → NIKA-PARSE +
        // validation_error (schema-checkable structure).
        let code = SchemaError::ReservedBindingName {
            name: "status".into(),
            task: "api".into(),
            span: None,
        }
        .spec_code();
        assert_eq!(code.namespace, "PARSE");
        assert_eq!(code.category.as_str(), "validation_error");

        // variables/011 · unclosed `${{` → NIKA-VAR + validation_error
        // (the substitution grammar owns it · NOT NIKA-PARSE).
        let code = SchemaError::TemplateSyntax {
            reason: "unterminated".into(),
            span: None,
        }
        .spec_code();
        assert_eq!(code.namespace, "VAR");
        assert_eq!(code.category.as_str(), "validation_error");
    }

    #[test]
    fn bad_builtin_args_done_carries_its_registered_code() {
        // Deep fixture 010 matches `nika:done` on its REGISTERED exact code
        // (BUILTIN-DONE-001); every other tool emits the generic BUILTIN-001.
        // This pins both arms — killing the `tool == "nika:done"` guard
        // (forced true/false) and the `==`→`!=` flip.
        let done = SchemaError::BadBuiltinArgs {
            task: "finish".into(),
            tool: "nika:done".into(),
            reason: "needs a `status:` arg".into(),
            span: None,
        }
        .spec_code();
        assert_eq!(done.namespace, "BUILTIN-DONE");
        assert_eq!(done.num, 1);
        assert_eq!(done.to_string(), "NIKA-BUILTIN-DONE-001");

        // A DIFFERENT builtin must NOT borrow the nika:done code — it falls
        // to the generic namespace (this is the arm a guard→true would steal).
        let other = SchemaError::BadBuiltinArgs {
            task: "fetch".into(),
            tool: "nika:fetch".into(),
            reason: "needs a `url:` arg".into(),
            span: None,
        }
        .spec_code();
        assert_eq!(other.namespace, "BUILTIN");
        assert_eq!(other.num, 1);
        assert_eq!(other.to_string(), "NIKA-BUILTIN-001");
    }

    #[test]
    fn numbers_unique_within_namespace() {
        use std::collections::BTreeSet;
        let mut seen = BTreeSet::new();
        for err in &all_error_variants() {
            let code = err.spec_code();
            // VAR-001 is deliberately shared by the 3 resolution-class
            // variants (the spec defines ONE code for the class).
            seen.insert((code.namespace, code.num, code.category.as_str()));
        }
        // 31 variants · 3 share VAR-001 · 2 share VAR-005 → 28
        // distinct codes (DAG-004 + BadBuiltinArgs generic + the
        // registry remaps of 2026-06-11 · VAR-020 bare-envelope joins
        // 0.103 · the nika:done arm adds BUILTIN-DONE-001 only when the
        // tool matches — the enumerator carries the generic arm).
        assert_eq!(seen.len(), 28, "{seen:?}");
    }
}
