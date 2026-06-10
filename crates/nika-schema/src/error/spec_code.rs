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
//! - `PARSE-WHEN` · the `when:` static boolean-shape gate
//!   (`NIKA-PARSE-WHEN-001` per spec `03-dag.md` §when).
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

/// `NIKA-PARSE-WHEN-001` — the 4-segment sub-namespace (spec
/// `03-dag.md` · « The engine rejects non-boolean `when:` expressions
/// at parse time (`NIKA-PARSE-WHEN-001`) »).
const PARSE_WHEN_001: SpecCode = SpecCode {
    namespace: "PARSE-WHEN",
    num: 1,
    category: SpecCategory::ValidationError,
    transient: false,
};

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
            Self::JqBindingContainsTemplate { .. } => parse(16, ValidationError),
            Self::DuplicateKey { .. } => parse(17, ValidationError),
            Self::MissingField { .. } => parse(18, ValidationError),
            Self::Validation { .. } => parse(19, ValidationError),

            // ── NIKA-PARSE-WHEN · the when: boolean gate ────────────
            Self::WhenNotBoolean { .. } => PARSE_WHEN_001,

            // ── NIKA-DAG · topology ─────────────────────────────────
            Self::Cycle { .. } => dag(1),
            Self::UnknownDependency { .. } => dag(2),
            Self::MissingDependsOnEdge { .. } => dag(3),

            // ── NIKA-VAR · the ${{ }} surface ───────────────────────
            // NIKA-VAR-001 · reference resolution (fixtures 001-007 ·
            // variable_error) · loop-locals + unknown task fields are
            // the same unresolved class per the fixture notes.
            Self::UnresolvedNamespaceRef { .. }
            | Self::LoopLocalOutsideForEach { .. }
            | Self::UnknownTaskField { .. } => var(1, VariableError),
            // Fixture variables/011 · malformed substitution syntax is
            // NIKA-VAR + validation_error (« the YAML itself parses
            // fine ») — NOT NIKA-PARSE.
            Self::TemplateSyntax { .. } => var(2, ValidationError),
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
        assert_eq!(PARSE_WHEN_001.to_string(), "NIKA-PARSE-WHEN-001");
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
    fn numbers_unique_within_namespace() {
        use std::collections::BTreeSet;
        let mut seen = BTreeSet::new();
        for err in &all_error_variants() {
            let code = err.spec_code();
            // VAR-001 is deliberately shared by the 3 resolution-class
            // variants (the spec defines ONE code for the class).
            seen.insert((code.namespace, code.num, code.category.as_str()));
        }
        // 27 variants · 3 share VAR-001 → 25 distinct codes.
        assert_eq!(seen.len(), 25, "{seen:?}");
    }
}
