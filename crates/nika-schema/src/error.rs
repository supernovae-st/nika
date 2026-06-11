// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Schema error types — parse errors, analysis errors, validation errors.
//!
//! Engine-internal codes · `NikaErrorCode::nika_code()` — the gate-12
//! mechanism · `Category::Schema` range NIKA-280..329. The spec-facing
//! `NIKA-<NAMESPACE>-<NNN>` surface (`nika-spec/spec/05-errors.md`) lives
//! in [`spec_code`] · `SchemaError::spec_code()`.

pub mod spec_code;

pub use spec_code::{SpecCategory, SpecCode};

use nika_error::codes::{Category, NikaCode, Severity};
use nika_error::traits::NikaErrorCode;

use crate::source::Span;

/// Errors from the schema layer (parser, analyzer, validator).
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum SchemaError {
    // ── Parse-level · NIKA-PARSE ────────────────────────────────────
    /// YAML syntax error.
    #[error("YAML parse error: {message}")]
    YamlSyntax {
        /// Description of the syntax error.
        message: String,
        /// Source span where the error occurred.
        span: Option<Span>,
    },

    /// A required envelope field is missing (or `tasks:` is empty).
    ///
    /// Spec `01-envelope.md` · « Two required lines (`nika:` +
    /// `workflow:`) and a non-empty `tasks:`. That's the **whole
    /// minimum** to be a valid Nika workflow. »
    #[error("missing required envelope field: `{field}`")]
    MissingEnvelopeField {
        /// The missing field (`nika` · `workflow` · `tasks`).
        field: String,
        /// Source span (workflow root when known).
        span: Option<Span>,
    },

    /// `nika:` carries a value other than exactly `v1`.
    ///
    /// Spec `01-envelope.md` · « **Anti-pattern** · do not write
    /// `nika: v1.0` · `nika: "1"` · or `nika: 1.0`. The value is
    /// exactly `v1`. »
    #[error("invalid `nika:` version `{version}` — the value is exactly `v1`")]
    BadNikaVersion {
        /// The rejected version string.
        version: String,
        /// Source span of the `nika:` scalar.
        span: Option<Span>,
    },

    /// `workflow:` is not kebab-case.
    ///
    /// Spec `01-envelope.md` · « Must match · `^[a-z][a-z0-9-]*$`. »
    #[error("invalid `workflow:` id `{id}` — must match ^[a-z][a-z0-9-]*$ (kebab-case)")]
    BadWorkflowId {
        /// The rejected workflow id.
        id: String,
        /// Source span.
        span: Option<Span>,
    },

    /// Unknown field rejected in strict mode.
    ///
    /// Spec `02-verbs.md` §forward-compat · « **Reject** with a clear
    /// error (strict mode · default for tests) ».
    #[error("unknown field `{field}` in {location} (strict mode)")]
    UnknownField {
        /// The unknown key.
        field: String,
        /// Where it appeared (`workflow envelope` · `task x` · `infer:` …).
        location: String,
        /// Source span of the key.
        span: Option<Span>,
    },

    /// Task `id:` is not `snake_case`.
    ///
    /// Spec `03-dag.md` · « Match · `^[a-z][a-z0-9_]*$` (`snake_case` ·
    /// no hyphens) » — a hyphen would parse as CEL subtraction.
    #[error("invalid task id `{id}` — must match ^[a-z][a-z0-9_]*$ (snake_case · CEL-safe)")]
    BadTaskId {
        /// The rejected task id.
        id: String,
        /// Source span.
        span: Option<Span>,
    },

    /// Duplicate task id within the workflow.
    #[error("duplicate task id: `{id}`")]
    DuplicateTaskId {
        /// The duplicated id.
        id: String,
        /// Span of the second declaration.
        span: Option<Span>,
    },

    /// A task binds zero verbs.
    ///
    /// Spec `02-verbs.md` · « A task **must** specify exactly one of
    /// these. »
    #[error("task `{task}` has no verb — exactly one of infer, exec, invoke, agent required")]
    MissingVerb {
        /// The offending task id (or `<unnamed>`).
        task: String,
        /// Source span of the task mapping.
        span: Option<Span>,
    },

    /// A task binds two or more verbs.
    ///
    /// Spec `02-verbs.md` · « Multiple verbs on a single task is a
    /// validation error. »
    #[error("task `{task}` has multiple verbs ({verbs}) — exactly one required")]
    MultipleVerbs {
        /// The offending task id (or `<unnamed>`).
        task: String,
        /// The verbs found, comma-joined.
        verbs: String,
        /// Source span of the task mapping.
        span: Option<Span>,
    },

    /// `timeout:` is not a valid Go-duration string.
    ///
    /// Spec `03-dag.md` §timeout · quoted Go-duration · `> 0` · `≤ 24h` ·
    /// descending units · a bare YAML number is ambiguous and rejected.
    #[error("invalid `timeout:` — {reason}")]
    BadTimeout {
        /// Why the timeout was rejected.
        reason: String,
        /// Source span.
        span: Option<Span>,
    },

    /// `retry:` block violates the spec shape (spec `05-errors.md` §retry).
    #[error("invalid `retry:` — {reason}")]
    BadRetry {
        /// Why the retry block was rejected.
        reason: String,
        /// Source span.
        span: Option<Span>,
    },

    /// `on_error:` violates exactly-one-of `recover`|`skip`|`fail_workflow`.
    ///
    /// Spec `05-errors.md` §`on_error` · « Fields (mutually exclusive) ».
    #[error("invalid `on_error:` — {reason}")]
    BadOnError {
        /// Why the `on_error` block was rejected.
        reason: String,
        /// Source span.
        span: Option<Span>,
    },

    /// An `output:` binding name collides with a reserved result-record
    /// field.
    ///
    /// Spec `04-variables.md` · « `<name>` collisions with reserved words
    /// `output` · `status` · `error` · `started_at` · `ended_at` ·
    /// `duration_ms` are forbidden at parse time. »
    #[error("output binding `{name}` in task `{task}` collides with a reserved field")]
    ReservedBindingName {
        /// The reserved name used.
        name: String,
        /// The task declaring the binding.
        task: String,
        /// Source span.
        span: Option<Span>,
    },

    /// A `secrets:` entry is malformed — inline literal, unknown source,
    /// or missing key.
    ///
    /// Spec `01-envelope.md` §secrets · « A secret is always a
    /// **reference to a store** — never an inline literal. »
    #[error("invalid secret reference — {reason}")]
    BadSecretRef {
        /// Why the secret reference was rejected.
        reason: String,
        /// Source span.
        span: Option<Span>,
    },

    /// A typed `vars:` declaration is malformed (spec `01-envelope.md`
    /// §vars · type ∈ string·number·integer·boolean·array·object).
    #[error("invalid typed var `{name}` — {reason}")]
    BadTypedVar {
        /// The var name.
        name: String,
        /// Why it was rejected.
        reason: String,
        /// Source span.
        span: Option<Span>,
    },

    /// `${{ … }}` template syntax error — unterminated island or invalid
    /// CEL inside (spec `04-variables.md` + `03-dag.md` §CEL-subset).
    #[error("template syntax error — {reason}")]
    TemplateSyntax {
        /// Why the template was rejected.
        reason: String,
        /// Source span.
        span: Option<Span>,
    },

    /// A task `output:` jq binding contains `${{` — the two expression
    /// layers never nest.
    ///
    /// Spec `04-variables.md` §binding rules · « An `output:` jq
    /// expression is pure jq over the task's raw output — it does NOT
    /// contain `${{ }}`. »
    #[error(
        "output binding `{name}` in task `{task}` contains a template — jq bindings are pure jq"
    )]
    JqBindingContainsTemplate {
        /// The binding name.
        name: String,
        /// The task declaring the binding.
        task: String,
        /// Source span.
        span: Option<Span>,
    },

    /// Duplicate mapping key (vars · env · secrets · outputs · with ·
    /// output · or any YAML mapping — no silent last-wins).
    #[error("duplicate key: {message}")]
    DuplicateKey {
        /// The loader's duplicate-key description (carries both markers).
        message: String,
        /// Source span when known.
        span: Option<Span>,
    },

    /// Missing required field in a verb body (e.g. `infer.prompt` ·
    /// `exec.command` · `invoke.tool`).
    #[error("missing required field: {field}")]
    MissingField {
        /// Name of the missing field.
        field: String,
        /// Source span.
        span: Option<Span>,
    },

    /// Generic structural validation error (wrong YAML shape — a mapping
    /// where a scalar was required, etc.).
    #[error("validation error: {message}")]
    Validation {
        /// Description.
        message: String,
        /// Source span.
        span: Option<Span>,
    },

    /// A builtin `invoke:` violates its statically-checkable arg
    /// contract (`stdlib/builtins-v0.1.md` · deep fixtures 009-012).
    #[error("task `{task}` · `{tool}` {reason}")]
    BadBuiltinArgs {
        /// The task carrying the invoke.
        task: String,
        /// The builtin tool id.
        tool: String,
        /// The violated contract (prescriptive · names the fix).
        reason: String,
        /// The tool reference's span.
        span: Option<Span>,
    },

    /// `on_error.recover` references a task that transitively depends
    /// on the declaring task — the recovery-time await would deadlock
    /// (spec `05-errors.md` §recover resolution · `NIKA-DAG-004`).
    #[error(
        "task `{task}` on_error.recover reads tasks.{target} — `{target}` depends \
         (transitively) on `{task}` · the recovery await would deadlock · recover \
         from an upstream or independent source"
    )]
    RecoverAwaitDeadlock {
        /// The task declaring the `on_error.recover`.
        task: String,
        /// The recovery source that loops back.
        target: String,
        /// The recover value's span.
        span: Option<Span>,
    },

    /// `when:` (or `for_each:`) is not a single CEL island of the
    /// required shape.
    ///
    /// Spec `03-dag.md` §when shape rules · statically-non-boolean-shaped
    /// roots are rejected at parse time (the spec's `NIKA-VAR-005` class ·
    /// the retired `NIKA-PARSE-WHEN-001` name was folded there) — the
    /// YAML boolean literal (`when: true` · the always-pattern) is the
    /// OTHER legal form and never reaches this error.
    #[error("invalid `{field}:` in task `{task}` — {reason}")]
    WhenNotBoolean {
        /// The field (`when` or `for_each`).
        field: String,
        /// The task carrying it.
        task: String,
        /// Why it was rejected.
        reason: String,
        /// Source span.
        span: Option<Span>,
    },

    // ── DAG topology · NIKA-DAG ─────────────────────────────────────
    /// Dependency cycle detected in the task DAG (`NIKA-DAG-001`).
    ///
    /// Spec `03-dag.md` · « The engine MUST reject any workflow with
    /// cyclic dependencies at parse time with a clear error. »
    #[error("dependency cycle: {}", cycle.join(" → "))]
    Cycle {
        /// Task ids forming the cycle.
        cycle: Vec<String>,
    },

    /// `depends_on` references a task that does not exist
    /// (`NIKA-DAG-002`).
    #[error(
        "unknown dependency: task `{from}` depends on `{to}`, which does not exist{}",
        crate::suggest::suggestion_clause(.suggestion.as_deref())
    )]
    UnknownDependency {
        /// The task that has the dependency.
        from: String,
        /// The referenced task that doesn't exist.
        to: String,
        /// The nearest declared task id, when one is close enough — the
        /// deterministic repair (rustc's did-you-mean model).
        suggestion: Option<String>,
        /// Source span.
        span: Option<Span>,
    },

    /// A `tasks.<id>` reference without the matching `depends_on` edge
    /// (`NIKA-DAG-003`).
    ///
    /// Spec `03-dag.md` · « The engine **rejects the workflow at parse
    /// time** otherwise … it does **not** silently infer the edge. »
    #[error(
        "task `{task}` references `tasks.{referenced}` without declaring `{referenced}` in depends_on"
    )]
    MissingDependsOnEdge {
        /// The referencing task.
        task: String,
        /// The referenced task id.
        referenced: String,
        /// Source span of the reference.
        span: Option<Span>,
    },

    // ── Variable resolution · NIKA-VAR ──────────────────────────────
    /// A `${{ … }}` reference does not resolve to a declared name
    /// (`NIKA-VAR-001` · spec `04-variables.md` §resolution order).
    #[error(
        "unresolved reference `{reference}` in {location}{}",
        crate::suggest::suggestion_clause(.suggestion.as_deref())
    )]
    UnresolvedNamespaceRef {
        /// The unresolved reference (e.g. `vars.ghost` · `tasks.ghost`).
        reference: String,
        /// Where it appeared (task id or `outputs:`).
        location: String,
        /// The nearest declared name in the SAME namespace, fully
        /// qualified (`vars.topic`), when one is close enough — the
        /// deterministic repair (rustc's did-you-mean model).
        suggestion: Option<String>,
        /// Source span.
        span: Option<Span>,
    },

    /// `item` / `index` used outside a `for_each` task body.
    ///
    /// Spec `04-variables.md` · « They are **loop-scoped locals**, alive
    /// only within that task's body. »
    #[error("loop-local `{local}` used in task `{task}` which has no `for_each:`")]
    LoopLocalOutsideForEach {
        /// `item` or `index`.
        local: String,
        /// The offending task.
        task: String,
        /// Source span.
        span: Option<Span>,
    },

    /// `tasks.<id>.<field>` names an unknown result-record field.
    ///
    /// Spec `04-variables.md` §result record · valid fields are `output`
    /// · `status` · `error` · `started_at` · `ended_at` · `duration_ms`
    /// · plus the task's declared `output:` binding names.
    #[error("unknown field `tasks.{task}.{field}` — not a result-record field or declared binding")]
    UnknownTaskField {
        /// The referenced task.
        task: String,
        /// The unknown field.
        field: String,
        /// Source span.
        span: Option<Span>,
    },

    /// A `tasks.<id>.output.<path>` reference the producing task's
    /// declared `schema:` PROVABLY forbids (`NIKA-VAR-003` · spec
    /// `04-variables.md` §Static binding validation · sound · only
    /// provable violations are rejected).
    #[error("invalid output path `tasks.{task}.output{path}` — {reason}")]
    OutputPathProvablyInvalid {
        /// The producing task (the one declaring the schema).
        task: String,
        /// The offending path suffix (rendered `.key` / `[0]` steps).
        path: String,
        /// Which static rule the path violates.
        reason: String,
        /// Source span.
        span: Option<Span>,
    },
}

impl SchemaError {
    /// Create a validation error with no span.
    #[must_use]
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
            span: None,
        }
    }

    /// Create a validation error with a source span.
    #[must_use]
    pub fn validation_at(message: impl Into<String>, span: Span) -> Self {
        Self::Validation {
            message: message.into(),
            span: Some(span),
        }
    }
}

// ─── NIKA-280..329 schema error codes (engine-internal · gate 12) ────

macro_rules! schema_code {
    ($name:ident, $num:expr, $slug:expr) => {
        const $name: NikaCode = NikaCode {
            num: $num,
            category: Category::Schema,
            severity: Severity::Error,
            slug: $slug,
        };
    };
}

schema_code!(SCHEMA_280, 280, "yaml-syntax");
schema_code!(SCHEMA_281, 281, "missing-envelope-field");
schema_code!(SCHEMA_282, 282, "bad-nika-version");
schema_code!(SCHEMA_283, 283, "bad-workflow-id");
schema_code!(SCHEMA_284, 284, "unknown-field");
schema_code!(SCHEMA_285, 285, "bad-task-id");
schema_code!(SCHEMA_286, 286, "duplicate-task-id");
schema_code!(SCHEMA_287, 287, "missing-verb");
schema_code!(SCHEMA_288, 288, "multiple-verbs");
schema_code!(SCHEMA_289, 289, "bad-timeout");
schema_code!(SCHEMA_290, 290, "bad-retry");
schema_code!(SCHEMA_291, 291, "bad-on-error");
schema_code!(SCHEMA_292, 292, "reserved-binding-name");
schema_code!(SCHEMA_293, 293, "bad-secret-ref");
schema_code!(SCHEMA_294, 294, "bad-typed-var");
schema_code!(SCHEMA_295, 295, "template-syntax");
schema_code!(SCHEMA_296, 296, "jq-binding-contains-template");
schema_code!(SCHEMA_297, 297, "duplicate-key");
schema_code!(SCHEMA_298, 298, "missing-field");
schema_code!(SCHEMA_299, 299, "validation");
schema_code!(SCHEMA_300, 300, "when-not-boolean");
schema_code!(SCHEMA_301, 301, "cycle");
schema_code!(SCHEMA_302, 302, "unknown-dependency");
schema_code!(SCHEMA_303, 303, "missing-depends-on-edge");
schema_code!(SCHEMA_304, 304, "unresolved-namespace-ref");
schema_code!(SCHEMA_305, 305, "loop-local-outside-for-each");
schema_code!(SCHEMA_306, 306, "unknown-task-field");
schema_code!(SCHEMA_307, 307, "output-path-provably-invalid");
schema_code!(SCHEMA_308, 308, "recover-await-deadlock");
schema_code!(SCHEMA_309, 309, "bad-builtin-args");

impl NikaErrorCode for SchemaError {
    fn nika_code(&self) -> NikaCode {
        match self {
            Self::YamlSyntax { .. } => SCHEMA_280,
            Self::MissingEnvelopeField { .. } => SCHEMA_281,
            Self::BadNikaVersion { .. } => SCHEMA_282,
            Self::BadWorkflowId { .. } => SCHEMA_283,
            Self::UnknownField { .. } => SCHEMA_284,
            Self::BadTaskId { .. } => SCHEMA_285,
            Self::DuplicateTaskId { .. } => SCHEMA_286,
            Self::MissingVerb { .. } => SCHEMA_287,
            Self::MultipleVerbs { .. } => SCHEMA_288,
            Self::BadTimeout { .. } => SCHEMA_289,
            Self::BadRetry { .. } => SCHEMA_290,
            Self::BadOnError { .. } => SCHEMA_291,
            Self::ReservedBindingName { .. } => SCHEMA_292,
            Self::BadSecretRef { .. } => SCHEMA_293,
            Self::BadTypedVar { .. } => SCHEMA_294,
            Self::TemplateSyntax { .. } => SCHEMA_295,
            Self::JqBindingContainsTemplate { .. } => SCHEMA_296,
            Self::DuplicateKey { .. } => SCHEMA_297,
            Self::MissingField { .. } => SCHEMA_298,
            Self::Validation { .. } => SCHEMA_299,
            Self::WhenNotBoolean { .. } => SCHEMA_300,
            Self::RecoverAwaitDeadlock { .. } => SCHEMA_308,
            Self::BadBuiltinArgs { .. } => SCHEMA_309,
            Self::Cycle { .. } => SCHEMA_301,
            Self::UnknownDependency { .. } => SCHEMA_302,
            Self::MissingDependsOnEdge { .. } => SCHEMA_303,
            Self::UnresolvedNamespaceRef { .. } => SCHEMA_304,
            Self::LoopLocalOutsideForEach { .. } => SCHEMA_305,
            Self::UnknownTaskField { .. } => SCHEMA_306,
            Self::OutputPathProvablyInvalid { .. } => SCHEMA_307,
        }
    }

    fn is_transient(&self) -> bool {
        false // Schema errors are never transient — the YAML is wrong.
    }
}

/// Every variant once — keeps the code-mapping tests exhaustive.
#[cfg(test)]
pub(crate) fn all_error_variants() -> Vec<SchemaError> {
    let mut variants = parse_level_variants();
    variants.extend(analysis_level_variants());
    variants
}

/// Parse-level variants (YAML shape · envelope · task shape · verbs).
#[cfg(test)]
fn parse_level_variants() -> Vec<SchemaError> {
    vec![
        SchemaError::YamlSyntax {
            message: String::new(),
            span: None,
        },
        SchemaError::MissingEnvelopeField {
            field: String::new(),
            span: None,
        },
        SchemaError::BadNikaVersion {
            version: String::new(),
            span: None,
        },
        SchemaError::BadWorkflowId {
            id: String::new(),
            span: None,
        },
        SchemaError::UnknownField {
            field: String::new(),
            location: String::new(),
            span: None,
        },
        SchemaError::BadTaskId {
            id: String::new(),
            span: None,
        },
        SchemaError::DuplicateTaskId {
            id: String::new(),
            span: None,
        },
        SchemaError::MissingVerb {
            task: String::new(),
            span: None,
        },
        SchemaError::MultipleVerbs {
            task: String::new(),
            verbs: String::new(),
            span: None,
        },
        SchemaError::BadTimeout {
            reason: String::new(),
            span: None,
        },
        SchemaError::BadRetry {
            reason: String::new(),
            span: None,
        },
        SchemaError::BadOnError {
            reason: String::new(),
            span: None,
        },
        SchemaError::ReservedBindingName {
            name: String::new(),
            task: String::new(),
            span: None,
        },
        SchemaError::BadSecretRef {
            reason: String::new(),
            span: None,
        },
        SchemaError::BadTypedVar {
            name: String::new(),
            reason: String::new(),
            span: None,
        },
        SchemaError::TemplateSyntax {
            reason: String::new(),
            span: None,
        },
        SchemaError::JqBindingContainsTemplate {
            name: String::new(),
            task: String::new(),
            span: None,
        },
        SchemaError::DuplicateKey {
            message: String::new(),
            span: None,
        },
        SchemaError::MissingField {
            field: String::new(),
            span: None,
        },
        SchemaError::Validation {
            message: String::new(),
            span: None,
        },
        SchemaError::WhenNotBoolean {
            field: String::new(),
            task: String::new(),
            reason: String::new(),
            span: None,
        },
    ]
}

/// Analysis-level variants (DAG topology · variable resolution).
#[cfg(test)]
fn analysis_level_variants() -> Vec<SchemaError> {
    vec![
        SchemaError::Cycle { cycle: vec![] },
        SchemaError::UnknownDependency {
            from: String::new(),
            to: String::new(),
            suggestion: None,
            span: None,
        },
        SchemaError::MissingDependsOnEdge {
            task: String::new(),
            referenced: String::new(),
            span: None,
        },
        SchemaError::RecoverAwaitDeadlock {
            task: String::new(),
            target: String::new(),
            span: None,
        },
        SchemaError::BadBuiltinArgs {
            task: String::new(),
            tool: String::new(),
            reason: String::new(),
            span: None,
        },
        SchemaError::UnresolvedNamespaceRef {
            reference: String::new(),
            location: String::new(),
            suggestion: None,
            span: None,
        },
        SchemaError::LoopLocalOutsideForEach {
            local: String::new(),
            task: String::new(),
            span: None,
        },
        SchemaError::UnknownTaskField {
            task: String::new(),
            field: String::new(),
            span: None,
        },
        SchemaError::OutputPathProvablyInvalid {
            task: String::new(),
            path: String::new(),
            reason: String::new(),
            span: None,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_syntax_error_display() {
        let err = SchemaError::YamlSyntax {
            message: "unexpected token".into(),
            span: None,
        };
        assert!(err.to_string().contains("YAML parse error"));
        assert!(err.to_string().contains("unexpected token"));
    }

    #[test]
    fn cycle_display() {
        let err = SchemaError::Cycle {
            cycle: vec!["a".into(), "b".into(), "c".into()],
        };
        assert_eq!(err.to_string(), "dependency cycle: a → b → c");
    }

    #[test]
    fn bad_nika_version_display() {
        let err = SchemaError::BadNikaVersion {
            version: "v1.0".into(),
            span: None,
        };
        assert!(err.to_string().contains("v1.0"));
        assert!(err.to_string().contains("exactly `v1`"));
    }

    #[test]
    fn error_codes_are_in_schema_range() {
        for err in &all_error_variants() {
            let code = err.nika_code();
            assert!(
                (280..=329).contains(&code.num),
                "SchemaError code {} should be in 280-329 range",
                code.num,
            );
            assert_eq!(code.category, Category::Schema);
        }
    }

    #[test]
    fn all_codes_unique() {
        let codes: Vec<u16> = all_error_variants()
            .iter()
            .map(|e| e.nika_code().num)
            .collect();
        let mut deduped = codes.clone();
        deduped.sort_unstable();
        deduped.dedup();
        assert_eq!(codes.len(), deduped.len(), "all error codes must be unique");
    }

    #[test]
    fn schema_errors_are_never_transient() {
        let err = SchemaError::YamlSyntax {
            message: "bad".into(),
            span: None,
        };
        assert!(!err.is_transient());
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn schema_error_is_send_sync() {
        _assert_send_sync::<SchemaError>();
    }
}
