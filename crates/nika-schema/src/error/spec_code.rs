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

/// The base URL of the per-code error pages (`<base>/<CODE>`) — the
/// human twin of the machine registry served at
/// `https://nika.sh/errors/catalog.json`. Findings stamp their own
/// docs URL from it so consumers never hardcode the scheme. Defined
/// HERE (beside [`SpecCode`], below every consumer) because both the
/// parser side (`skill.rs`) and the check ladder (`nika-check`, which
/// re-exports it unchanged) stamp findings with it.
pub const ERROR_DOCS_BASE: &str = "https://nika.sh/language/errors";

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

const fn typ(num: u16) -> SpecCode {
    SpecCode {
        namespace: "TYPE",
        num,
        category: SpecCategory::ValidationError,
        transient: false,
    }
}

/// The BUILTIN arg-shape codes — `nika:done` carries its REGISTERED
/// exact code (deep fixture 010 matches on it) · the other shapes emit
/// the generic builtin namespace.
const fn builtin_spec_code(tool: &str) -> SpecCode {
    // const-friendly comparison: namespace choice is the only fork
    if matches!(tool.as_bytes(), b"nika:done") {
        SpecCode {
            namespace: "BUILTIN-DONE",
            num: 1,
            category: SpecCategory::ValidationError,
            transient: false,
        }
    } else {
        SpecCode {
            namespace: "BUILTIN",
            num: 1,
            category: SpecCategory::ValidationError,
            transient: false,
        }
    }
}

/// The `run:` declared-pair mints (F-P3 · NEP-0010) — the wire code
/// follows the vocab class; a future contradiction class rides the
/// registered generic until its dedicated mint lands.
const fn run_contradiction_code(class: crate::types::RunContradiction) -> SpecCode {
    use crate::types::RunContradiction as Class;
    match class {
        Class::AmbientTimesVirtual => parse(26, SpecCategory::ValidationError),
        Class::DeterminismTimesSystem => parse(27, SpecCategory::ValidationError),
        _ => parse(19, SpecCategory::ValidationError),
    }
}

impl SchemaError {
    /// Map to the spec-facing code (spec `05-errors.md`).
    ///
    /// Exhaustive — a new variant fails compilation until mapped.
    ///
    /// NIKA-PARSE-015 is RETIRED (never reuse · canon.yaml — the
    /// allocation hole is deliberate): the typed-vars 6-enum class died
    /// with the R3b `TypeExpr` widen (the rich forms are admitted ·
    /// out-of-grammar refuses NIKA-TYPE-001 · the surviving shape
    /// refusals ride NIKA-PARSE-019).
    ///
    /// NIKA-VAR-005 (spec 05 registry) = « static expression violation —
    /// outside cel-subset/0.1 · chained relation · unknown function ·
    /// non-boolean `when:` root · jq compile error »: the class spans the
    /// non-boolean `when:` shape gate (the retired NIKA-PARSE-WHEN-001
    /// folded here), `${{ }}` inside an output binding (04 §binding rules
    /// · deep fixtures 003/007/008), and a CLOSED `${{ }}` island whose
    /// CEL is outside the subset (`ExpressionViolation` — distinct from
    /// the unclosed-opener VAR-008).
    #[must_use]
    pub fn spec_code(&self) -> SpecCode {
        use SpecCategory::{ParseError, ValidationError, VariableError};
        match self {
            // ── NIKA-PARSE · structural / shape ─────────────────────
            Self::YamlSyntax { .. } => parse(1, ParseError),
            Self::MissingEnvelopeField { .. } => parse(2, ValidationError),
            // Fixture envelope/003-nika-id-bad-shape · `validation_error`,
            // NOT `parse_error`. The code survives the envelope nuke by
            // changing MEANING and its CATEGORY moves with it: judging a
            // version marker was the file's entry contract (a parse-level
            // gate), judging a kebab-case id is a spec-rule violation in
            // well-formed input. The document parses; the name is wrong.
            Self::BadNikaId { .. } => parse(3, ValidationError),
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
            Self::DuplicateKey { .. } => parse(17, ValidationError),
            Self::MissingField { .. } => parse(18, ValidationError),
            Self::Validation { .. } | Self::D1StringCommand { .. } => parse(19, ValidationError),
            Self::RunContradiction { class, .. } => run_contradiction_code(*class),
            // W1 « the map » migration teachings (dead forms · 0.104).
            // `NIKA-PARSE-020` / `-021` are RETIRED with the envelope
            // nuke (2026-08-12) and never reused — their teachings
            // pointed at the `workflow:` object, itself now dead.
            Self::W1TasksSequence { .. } => parse(22, ValidationError),
            Self::W1TaskIdField { .. } => parse(23, ValidationError),
            // W2 « the flow » migration teaching (dead form · 0.104)
            Self::W2DependsOnField { .. } => parse(24, ValidationError),
            Self::WhenNotBoolean { .. }
            | Self::JqBindingContainsTemplate { .. }
            | Self::ExpressionViolation { .. } => var(5, ValidationError),

            // ── NIKA-DAG · topology ── (DAG-003 retired · never reuse ·
            // the with: binding IS the edge — the class is inexpressible)
            Self::Cycle { .. } => dag(1),
            Self::UnknownDependency { .. } => dag(2),
            Self::RecoverAwaitDeadlock { .. } => dag(4),
            Self::UnknownAfterPredicate { .. } => dag(5),
            // DAG-008 · a fold of a group nobody declares (the empty
            // group is the SAME fact as an absent one) · DAG-009 ·
            // cleanup never enters G_p, so it can never be folded.
            Self::UnknownGroup { .. } => dag(8),
            Self::UnwindInGroup { .. } => dag(9),

            // ── NIKA-BUILTIN · arg-shape contracts ─────────────────
            Self::BadBuiltinArgs { tool, .. } => builtin_spec_code(tool),

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
            // NIKA-VAR-021 · a tasks.* reference outside the boundary
            // (04 §the reference boundary · the hoist is machine-applicable).
            Self::RefOutsideBoundary { .. } => var(21, ValidationError),
            // ── NIKA-TYPE · the type core (spec 09 · W3) ────────────
            // TYPE-001 (grammar) or TYPE-006 (regex dialect) — the num
            // rides the payload, carried verbatim from the type core's
            // ParseTypeError (one truth · never re-derived here).
            Self::TypeExprInvalid { num, .. } => typ(*num),
            Self::TypeContractDuplicated { .. } => typ(3),
            Self::TypeUndecodable { .. } => typ(4),
            // NIKA-PARSE-025 · decode: with capture: structured (05 §registry).
            Self::DecodeWithStructuredCapture { .. } => parse(25, ValidationError),

            // ── NIKA-VALUES · the C2 dead value forms (the E-split) ──
            Self::DeadValueForm { form, .. } => values_code(form.spec_num()),
            // NIKA-VALUES-003 · outside the four-authority family (LAW-SURFACE-0201 ·
            // rides alongside VAR-001 — the layered oracle emits both).
            Self::ForeignValueNamespace { .. } => values_code(3),
            // ── NIKA-DEFAULT · the R3b default-conformance law (LAW-TYPE-0211 ·
            // c0-proposed — the vendored pack canon carries the row pending the mint).
            Self::DefaultNotConforming { .. } => default_code(1),
        }
    }
}

/// The NIKA-VALUES arm shape (the C2 E-split · the fn-length ratchet
/// keeps [`SchemaError::spec_code`] at one line per variant).
const fn values_code(num: u16) -> SpecCode {
    SpecCode {
        namespace: "VALUES",
        num,
        category: SpecCategory::ValidationError,
        transient: false,
    }
}

/// The NIKA-DEFAULT arm shape (R3b · LAW-TYPE-0211).
const fn default_code(num: u16) -> SpecCode {
    SpecCode {
        namespace: "DEFAULT",
        num,
        category: SpecCategory::ValidationError,
        transient: false,
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
    fn values_codes_map_and_register() {
        // The C2 family — all three codes map to the VALUES namespace and
        // every one is registered in the vendored canon (the
        // `every_emittable_code_is_registered` ratchet's per-code pin ·
        // the TYPE-006 precedent for payload-driven nums).
        use crate::error::{DeadForm, SchemaError};
        let vars = SchemaError::DeadValueForm {
            form: DeadForm::Vars,
            message: String::new(),
            span: None,
        };
        assert_eq!(vars.spec_code().to_string(), "NIKA-VALUES-001");
        let env = SchemaError::DeadValueForm {
            form: DeadForm::Env,
            message: String::new(),
            span: None,
        };
        assert_eq!(env.spec_code().to_string(), "NIKA-VALUES-002");
        let foreign = SchemaError::ForeignValueNamespace {
            root: "params".to_owned(),
            message: String::new(),
            span: None,
        };
        assert_eq!(foreign.spec_code().to_string(), "NIKA-VALUES-003");
        let registered: std::collections::BTreeSet<&str> = nika_pack::error_codes()
            .into_iter()
            .map(|row| row.code)
            .collect();
        for code in ["NIKA-VALUES-001", "NIKA-VALUES-002", "NIKA-VALUES-003"] {
            assert!(
                registered.contains(code),
                "{code} must be registered in the canon error_codes table"
            );
        }
    }

    #[test]
    fn fixture_critical_mappings() {
        use crate::error::SchemaError;
        // envelope/003 · bad nika id → NIKA-PARSE + parse_error.
        let code = SchemaError::BadNikaId {
            id: "Not_Kebab".into(),
            span: None,
        }
        .spec_code();
        assert_eq!(code.namespace, "PARSE");
        // validation_error since the envelope nuke — the document
        // parses, the NAME is what violates the rule (fixture 003).
        assert_eq!(code.category.as_str(), "validation_error");

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
        // 39 variants · 3 share VAR-001 · 2 share VAR-005 → 37
        // distinct codes (DAG-003 retired with its variant in W2 ·
        // PARSE-024 + DAG-005 + VAR-021 join · VAR-020 bare-envelope
        // joined 0.103 · the nika:done arm adds BUILTIN-DONE-001 only
        // when the tool matches — the enumerator carries the generic arm ·
        // the W3 type core adds TYPE-001/2/3/4 + PARSE-025; TYPE-006
        // shares the TypeExprInvalid variant, enumerated once as num 1 ·
        // the C2 dead value forms add VALUES-001 (002 rides the same
        // variant by payload) + VALUES-003 (the foreign namespace) ·
        // R3b retires BadTypedVar/PARSE-015 with the TypeExpr widen and
        // adds DEFAULT-001. The envelope nuke (2026-08-12) retires
        // PARSE-004 with `BadWorkflowId` — the id moved onto `nika:`
        // and PARSE-003 judges it — and TYPE-002 with `TypeRecursive`,
        // whose object died with the `types:` block, so the census
        // drops to 35.
        assert_eq!(seen.len(), 35, "{seen:?}");
    }

    #[test]
    fn default_001_maps_and_registers() {
        // R3b · LAW-TYPE-0211 — the DEFAULT namespace's one code maps
        // exact (the TYPE-006 precedent for the registration pin: the
        // enumerator covers the emittance, the vendored canon covers the
        // registry — the spec canon mint is PENDING, c0-proposed).
        let err = SchemaError::DefaultNotConforming {
            where_: "inputs.count.default".to_owned(),
            message: String::new(),
            span: None,
        };
        let code = err.spec_code();
        assert_eq!(code.to_string(), "NIKA-DEFAULT-001");
        assert_eq!(code.namespace, "DEFAULT");
        assert_eq!(code.category.as_str(), "validation_error");
        let registered: std::collections::BTreeSet<&str> = nika_pack::error_codes()
            .into_iter()
            .map(|row| row.code)
            .collect();
        assert!(
            registered.contains("NIKA-DEFAULT-001"),
            "DEFAULT-001 must be registered in the vendored canon error_codes table"
        );
        // NIKA-PARSE-015 is the deliberate allocation hole — the retired
        // class is never emitted again (the registry row is a tombstone).
        assert!(
            !registered.contains("NIKA-PARSE-015"),
            "PARSE-015 stays retired (never reuse)"
        );
    }

    #[test]
    fn type_expr_invalid_maps_both_wire_numbers() {
        // The ONE variant carries TWO wire codes by payload — TYPE-001
        // (grammar) and TYPE-006 (regex dialect), both from the type
        // core's ParseTypeError. Pin both arms AND their canon
        // registration (the enumerator only carries num 1, so the
        // TYPE-006 side of the emitted⊆registered ratchet lives here).
        let grammar = SchemaError::TypeExprInvalid {
            num: 1,
            detail: String::new(),
            span: None,
        }
        .spec_code();
        assert_eq!(grammar.to_string(), "NIKA-TYPE-001");
        assert_eq!(grammar.category.as_str(), "validation_error");

        let dialect = SchemaError::TypeExprInvalid {
            num: 6,
            detail: String::new(),
            span: None,
        }
        .spec_code();
        assert_eq!(dialect.to_string(), "NIKA-TYPE-006");
        assert_eq!(dialect.category.as_str(), "validation_error");

        let registered: std::collections::BTreeSet<&str> = nika_pack::error_codes()
            .into_iter()
            .map(|row| row.code)
            .collect();
        assert!(
            registered.contains("NIKA-TYPE-006"),
            "TYPE-006 must be registered in the canon error_codes table"
        );
    }

    #[test]
    fn type_core_codes_map_to_the_spec_registry() {
        // The W3 contract layer (spec 09 §errors) — each variant to its
        // exact canon row · TYPE codes are validation_error · the
        // decode/structured conflict files under PARSE (05 §registry).
        let cases: [(SchemaError, &str); 3] = [
            (
                SchemaError::TypeContractDuplicated {
                    task: String::new(),
                    verb: "infer",
                    span: None,
                },
                "NIKA-TYPE-003",
            ),
            (
                SchemaError::TypeUndecodable {
                    task: String::new(),
                    decode: String::new(),
                    span: None,
                },
                "NIKA-TYPE-004",
            ),
            (
                SchemaError::DecodeWithStructuredCapture {
                    task: String::new(),
                    span: None,
                },
                "NIKA-PARSE-025",
            ),
        ];
        for (err, code) in cases {
            let spec = err.spec_code();
            assert_eq!(spec.to_string(), code);
            assert_eq!(spec.category.as_str(), "validation_error");
        }
    }
}
