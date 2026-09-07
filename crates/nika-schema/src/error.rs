// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Schema error types — parse errors, analysis errors, validation errors.
//!
//! Engine-internal codes · `NikaErrorCode::nika_code()` — the gate-12
//! mechanism · `Category::Schema` range NIKA-280..329. The spec-facing
//! `NIKA-<NAMESPACE>-<NNN>` surface (`nika-spec/spec/05-errors.md`) lives
//! in [`spec_code`] · `SchemaError::spec_code()`.

pub mod spec_code;

pub use spec_code::{ERROR_DOCS_BASE, SpecCategory, SpecCode};

use nika_error::codes::{Category, NikaCode, Severity};
use nika_error::traits::NikaErrorCode;

use crate::source::Span;

/// The shared, actionable rendering of one schema refusal.
///
/// CLI and MCP both project this value instead of rebuilding the wire code
/// from prose. The code stays typed for machine consumers; [`std::fmt::Display`]
/// supplies the portable repair hand-off every human-facing channel uses.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct SchemaDiagnostic {
    /// The spec-facing error code.
    pub code: SpecCode,
    /// The schema error's human-readable explanation.
    pub message: String,
}

impl std::fmt::Display for SchemaDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] {} · → nika explain {}",
            self.code, self.message, self.code
        )
    }
}

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
    /// Spec `01-envelope.md` · « The `nika:` line and a non-empty
    /// `tasks:` map. That's the **whole minimum** to be a valid Nika
    /// workflow. » Identity lives on `nika:` — there is no `workflow:`
    /// envelope key.
    #[error("missing required envelope field: `{field}`")]
    MissingEnvelopeField {
        /// The missing field (`nika` · `tasks`).
        field: String,
        /// Source span (workflow root when known).
        span: Option<Span>,
    },

    /// `nika:` carries a value that is not a kebab-case id.
    ///
    /// Spec `01-envelope.md` §`nika` · « the value is the **file's
    /// name**, kebab-case (`^[a-z][a-z0-9-]*$`) » · « **Anti-pattern** ·
    /// do not write `nika: v1` · `nika: My_Workflow` · `nika: "1.0"`. »
    ///
    /// The code (`NIKA-PARSE-003`) SURVIVES the envelope nuke by
    /// changing meaning: it judges an id now, no longer the literal.
    #[error(
        "invalid `nika:` id `{id}` — must match ^[a-z][a-z0-9-]*$ (kebab-case · the file's name)"
    )]
    BadNikaId {
        /// The rejected id.
        id: String,
        /// Source span of the `nika:` scalar.
        span: Option<Span>,
    },

    /// Unknown field rejected in strict mode.
    ///
    /// Spec `02-verbs.md` §forward-compat · « **Reject** with a clear
    /// error (strict mode · default for tests) ».
    ///
    /// Two answers ride separately, because two consumers read them:
    /// [`suggestion`](Self::UnknownField::suggestion) is the TYPED rename
    /// target (a bare key · what `check --fix` and the editor quickfix
    /// splice), [`teaching`](Self::UnknownField::teaching) is PROSE for a
    /// human (the retired-key migration · the modeline fix · the small
    /// set's own vocabulary). They lived in one field until 2026-08-18,
    /// and the repairers spliced whichever they found: `check --fix` on
    /// a file carrying `workflow:` renamed the key to the sentence "the
    /// fields here: nika · model · …" and reported one repair applied —
    /// the shipped 0.108.0 did the same on a de-commented modeline. A
    /// sentence is not a rename; the type now says so.
    #[error(
        "unknown field `{field}` in {location} (strict mode){}{}",
        nika_types::suggest::suggestion_clause(suggestion.as_deref()),
        teaching.as_deref().map(|t| format!(" — {t}")).unwrap_or_default()
    )]
    UnknownField {
        /// The unknown key.
        field: String,
        /// Where it appeared (`workflow envelope` · `task x` · `infer:` …).
        location: String,
        /// Source span of the key.
        span: Option<Span>,
        /// The nearest known key, when one is close enough to assert
        /// (Damerau-Levenshtein · rustc threshold — the same suggestion
        /// core every check finding shares · silence beats a wrong guess).
        /// ALWAYS a bare key a splice can apply — never prose (that is
        /// [`teaching`](Self::UnknownField::teaching)).
        suggestion: Option<String>,
        /// What a human should know beyond a rename — a retired key's
        /// migration, the modeline fix, a small closed set's vocabulary.
        /// Never machine-applied.
        teaching: Option<String>,
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

    // W1 « the map » · `W1WorkflowScalar` (`NIKA-PARSE-020`) and
    // `W1TopLevelDescription` (`NIKA-PARSE-021`) are GONE with the
    // envelope nuke (2026-08-12). They taught a migration TO the
    // `workflow: { id, description }` object — a form that is itself
    // dead now that the id lives on `nika:`. A teaching whose target no
    // longer exists is worse than no teaching. Both codes are RETIRED
    // and neither is ever reused (SSOT-2 B.22).
    /// W1 « the map » · `tasks:` is a sequence — it became a map keyed
    /// by task id (spec `01-envelope.md` §tasks · dead form).
    #[error(
        "`tasks:` is a sequence — it became a map keyed by task id; drop `- id:`, the key IS the identity"
    )]
    W1TasksSequence {
        /// Span of the `tasks:` node.
        span: Option<Span>,
    },

    /// W1 « the map » · a task carries an `id:` field — the map key IS
    /// the identity (spec `03-dag.md` §the task key · dead form).
    #[error("task `{task}` carries an `id:` field — the map key IS the identity; delete the field")]
    W1TaskIdField {
        /// The task (named by its map key).
        task: String,
        /// Span of the dead `id:` node.
        span: Option<Span>,
    },

    /// W2 « the flow » · a task carries `depends_on:` — dead form.
    /// Data crosses through `with:` (the binding IS the edge) · pure
    /// control through `after:` predicates (spec `03-dag.md`
    /// §`depends_on` · `check --fix` migrates the shapes its W2 scanner
    /// reads — see [`provable`](Self::W2DependsOnField::provable), which
    /// is NECESSARY for the repair, never sufficient).
    #[error("{}", w2_depends_on_message(.task, .task_hint, *.provable))]
    W2DependsOnField {
        /// The task (named by its map key).
        task: String,
        /// The first dep name (for the teaching's `after:` example) — the
        /// placeholder `producer` when the first entry is not a task id.
        task_hint: String,
        /// Whether the SHAPE is one the W2 migrator reads: a sequence
        /// whose every entry is a bare task id (`[a-z0-9_]+` · quotes
        /// stripped · an EMPTY sequence declares no edge and the dead line
        /// simply drops). A scalar, a map or any other entry is what the
        /// scanner calls malformed — `--fix` stops there, so the message
        /// hands that shape back to the author.
        ///
        /// NECESSARY, not sufficient: on an accepted shape the migrator
        /// still stops the whole file when a producer may skip (`when:` ·
        /// `for_each:` · `on_error.skip` · S1), when a dep is read only
        /// through its status (S3), or when a hoist would leave `on_error:`
        /// armor (S4) — which is why the message promises a shape `--fix`
        /// *can* migrate, never a repair it *will* apply (wave 3 · persona
        /// 02 · the promise fired on a scalar, then `--fix` answered
        /// « rewrite by hand »).
        provable: bool,
        /// Span of the dead `depends_on:` node.
        span: Option<Span>,
    },

    /// D1 « the split » · `command:` carried a STRING — the pre-0.103
    /// implicit shell. `command:` is argv-only now (execve · each
    /// element one token); the shell string lives in `shell:` (spec
    /// `02-verbs.md` §exec · `check --fix` migrates the mechanical
    /// cases). The wire code stays the generic structural PARSE-019 —
    /// the variant exists so the fix ladder can MATCH the dead form.
    #[error(
        "`exec.command` is argv-only — [\"prog\", \"arg\", …] runs via execve, each element one token (an interpolated value can never break out) · the old string form was an IMPLICIT shell: pipes/redirects/globs now live in `shell:` explicitly (02 §exec · 0.103 · `nika check --fix` migrates)"
    )]
    D1StringCommand {
        /// Span of the string node.
        span: Option<Span>,
    },

    /// A `skills:` entry that can never resolve — a `${{ }}` template
    /// or a glob (spec 02 §Agent Skills · « paths are static »).
    #[error(
        "`skills` entry `{path}` {why} — skill paths are static (loaded at compose time, before any value exists · the same explicitness law as `permits:`)"
    )]
    SkillPathNotStatic {
        /// The entry as written.
        path: String,
        /// Which shape disqualifies it (`carries a ${{ }} template` · `is a glob`).
        why: &'static str,
        /// Span of the entry.
        span: Option<Span>,
    },

    /// A `${{ group.<name> }}` reference to a group NO task declares
    /// (`NIKA-DAG-008`) — an empty group is the same fact as an absent
    /// one, so one code covers both, and a fold can never harvest zero
    /// members and read as clean. A bare `${{ group }}` names no group
    /// and lands here too (spec 03 §group).
    #[error(
        "task `{task}` folds `group.{name}` — no task declares that group (membership is DECLARED, never matched: check the members' `group:` keys)"
    )]
    UnknownGroup {
        /// The consuming task.
        task: String,
        /// The group named by the reference (empty = the bare `${{ group }}`).
        name: String,
        /// Span of the reference.
        span: Option<Span>,
    },

    /// An `unwind` task declares a `group:` (`NIKA-DAG-009`) — cleanup
    /// is an `E_f` attachment that never enters `G_p`, so a fan-in edge
    /// from it would have no wave to schedule against (spec 03 §group).
    #[error(
        "task `{task}` is an unwind task and joins group `{group}` — cleanup never enters the precedence graph, so a fold of it would have no wave to schedule against"
    )]
    UnwindInGroup {
        /// The cleanup task.
        task: String,
        /// The group it tried to join.
        group: String,
        /// Span of the `group:` value.
        span: Option<Span>,
    },

    /// W2 · an out-of-set `after:` predicate (03 §after · `NIKA-DAG-005` · R5 dead spellings teach).
    #[error("{message}")]
    UnknownAfterPredicate {
        /// The refusal text (dead-spelling teaching or the closed set).
        message: String,
        /// The declaring task.
        task: String,
        /// The producer entry carrying the bad predicate.
        target: String,
        /// The out-of-set spelling.
        predicate: String,
        /// Span of the predicate value.
        span: Option<Span>,
    },

    /// W2 · a `tasks.*` reference outside the boundary (spec
    /// `04-variables.md` §the reference boundary · `NIKA-VAR-021`) —
    /// body fields read LOCAL names; the fix is machine-applicable.
    #[error(
        "task `{task}` {surface} references `tasks.{reference}` — outside the boundary; hoist it into `with:` and read `${{{{ with.<name> }}}}` (`nika check --fix` applies it)"
    )]
    RefOutsideBoundary {
        /// The declaring task.
        task: String,
        /// The offending surface (`when:` · `for_each:` · a verb field
        /// · `on_finally` non-parent).
        surface: String,
        /// The referenced task id.
        reference: String,
        /// Span of the offending expression.
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

    /// `on_error:` violates exactly-one-of `recover`|`skip`.
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
    /// Spec `04-variables.md` + spec 13 · « `<name>` collisions with
    /// reserved `output` · `status` · `cause` · `error` · `started_at`
    /// · `ended_at` · `duration_ms` are forbidden at parse time. »
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

    /// `${{ … }}` template syntax error — strictly an UNTERMINATED island
    /// (a `${{` with no closing `}}`) · spec `05-errors.md` `NIKA-VAR-008`
    /// (« unclosed `${{` opener »). A CLOSED island whose CEL is invalid is
    /// [`Self::ExpressionViolation`] (`NIKA-VAR-005`), not this.
    #[error("template syntax error — {reason}")]
    TemplateSyntax {
        /// Why the template was rejected.
        reason: String,
        /// Source span.
        span: Option<Span>,
    },

    /// A CLOSED `${{ … }}` island whose CEL is outside the `cel-subset/0.1`
    /// grammar — a chained relation, an unknown function, arithmetic, a
    /// stray token. Spec `05-errors.md` `NIKA-VAR-005` (« static expression
    /// violation »). Distinct from [`Self::TemplateSyntax`] (`NIKA-VAR-008`),
    /// which is reserved for the unclosed-`${{` opener.
    #[error("expression error — {reason}")]
    ExpressionViolation {
        /// Why the expression was rejected (the CEL-subset grammar reason).
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

    /// The `run:` block declares a contradicting entropy × clock pair
    /// (F-P3 · the parse-level law) — the wire code follows the class
    /// (`NIKA-PARSE-026` ambient × virtual · `NIKA-PARSE-027`
    /// determinism × system · NEP-0010).
    #[error("`run:` contradicts itself — {class}")]
    RunContradiction {
        /// Which declared pair contradicts (the vocab-level class).
        class: crate::types::RunContradiction,
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
        nika_types::suggest::suggestion_clause(.suggestion.as_deref())
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

    // ── Variable resolution · NIKA-VAR ──────────────────────────────
    /// A `${{ … }}` reference does not resolve to a declared name
    /// (`NIKA-VAR-001` · spec `04-variables.md` §resolution order).
    #[error(
        "unresolved reference `{reference}` in {location}{}",
        nika_types::suggest::suggestion_clause(.suggestion.as_deref())
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
    #[error(
        "loop-local `{local}` is not bound in task `{task}` here — `item` and \
         `index` live only inside a `for_each:` task's BODY, and `when:` / \
         `for_each:` are evaluated BEFORE the fan-out"
    )]
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
    /// Spec `04-variables.md` §result record + spec 13 · valid fields:
    /// `output` · `status` · `cause` · `error` · `started_at` ·
    /// `ended_at` · `duration_ms` · plus declared `output:` bindings.
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

    /// A bare `${{ tasks.<id> }}` — the ENVELOPE, not a value (spec
    /// `04-variables.md` §namespaces · 0.103 · #75 D2).
    ///
    /// The projection set is CLOSED (`.output` · `.status` · `.error` ·
    /// `.duration_ms` + declared bindings); unprojected record access is
    /// ill-typed. Before 0.103 the bare form silently denoted the whole
    /// envelope — the golden-drift class engine#524 taught around,
    /// killed at the root.
    #[error(
        "bare `tasks.{task}` is the envelope, not a value — pick `.output` \
         (or .status/.error/.duration_ms · 04 §namespaces · closed projection set)"
    )]
    BareTaskEnvelope {
        /// The referenced task.
        task: String,
        /// Where the reference sits (`when:` · `prompt:` · `outputs.<n>` …).
        location: String,
        /// Source span.
        span: Option<Span>,
    },

    // ── Type core · NIKA-TYPE (W3 « the contract » · spec 09) ───────
    /// A type expression outside the closed v1 grammar (`NIKA-TYPE-001`
    /// — unknown name · reserved constructor · optional outside a field
    /// · bad shape) or a regex pattern outside the locked dialect
    /// (`NIKA-TYPE-006`). The detail comes VERBATIM from the type core
    /// (`nika-types` · the one truth) — place + why + did-you-mean.
    #[error("{detail}")]
    TypeExprInvalid {
        /// The `NIKA-TYPE` wire number (1 grammar · 6 dialect) — carried
        /// from [`nika_types::types::ParseTypeError::code`].
        num: u16,
        /// The type core's teaching diagnostic.
        detail: String,
        /// Span of the offending declaration/expression.
        span: Option<Span>,
    },

    /// `returns:` and a verb-level `schema:` on the same task — two
    /// spellings of one contract (`NIKA-TYPE-003` · one-obvious-way).
    #[error(
        "task `{task}` · returns: and {verb}.schema: are two spellings of one \
         contract — keep returns: (the typed door) or the schema: hatch, never both"
    )]
    TypeContractDuplicated {
        /// The task carrying both spellings.
        task: String,
        /// The verb whose body carries `schema:` (`infer` · `agent`).
        verb: &'static str,
        /// Span of the `returns:` expression.
        span: Option<Span>,
    },

    /// A `returns:` type that cannot come out of the declared `decode:`
    /// (`NIKA-TYPE-004` · an object contract over `decode: text` · …).
    #[error(
        "task `{task}` · returns: cannot come out of decode: {decode} — an \
         object/array contract needs decode: json or jsonl"
    )]
    TypeUndecodable {
        /// The task whose contract is unreachable.
        task: String,
        /// The effective decode mode (`text` when defaulted).
        decode: String,
        /// Span of the `returns:` expression.
        span: Option<Span>,
    },

    /// `decode:` with `capture: structured` (`NIKA-PARSE-025`) — that
    /// capture already IS an object (`{stdout, stderr, exit_code}`).
    #[error(
        "task `{task}` · decode: with capture: structured — that capture \
         already IS an object · type it with returns:"
    )]
    DecodeWithStructuredCapture {
        /// The task declaring both.
        task: String,
        /// Span of the `decode:` value.
        span: Option<Span>,
    },

    // ── The C2 dead value forms · NIKA-VALUES ─────────────────────────
    /// A pre-C2 `vars:`/`env:` usage — the dead envelope field itself, or
    /// a `${{ vars.X }}`/`${{ env.X }}` read that survives it
    /// (`NIKA-VALUES-001`/`NIKA-VALUES-002` · the E-split). The teaching
    /// classifies each use into the four-authority family, never a generic
    /// unknown-field error, and names the codemod repair.
    #[error("{message}")]
    DeadValueForm {
        /// The dead form (`vars` · `env`) — drives the spec code.
        form: DeadForm,
        /// The byte-mirrored teaching (envelope-field or reference site).
        message: String,
        /// Source span.
        span: Option<Span>,
    },

    /// A `${{ <root>.X }}` read whose root is outside the four value
    /// authorities AND the runtime namespaces AND the dead forms
    /// (`NIKA-VALUES-003` · LAW-SURFACE-0201 — the family is closed).
    /// Rides ALONGSIDE the unresolved-reference refusal (`NIKA-VAR-001`
    /// carries the did-you-mean): the layered oracle emits both.
    #[error("{message}")]
    ForeignValueNamespace {
        /// The offending root (`params` in `${{ params.region }}`).
        root: String,
        /// The byte-mirrored teaching.
        message: String,
        /// Source span.
        span: Option<Span>,
    },

    /// A declared `default:` (`inputs:`) or a typed-const
    /// `value:` does not conform to its declared `type:`
    /// (`NIKA-DEFAULT-001` · R3b · LAW-TYPE-0211 — the P0 soundness
    /// hole, a value that passed check and failed at run, is closed;
    /// the one type core judges, `values_core.py::_default_errors` is
    /// the oracle twin). The teaching text descends to nika-vocab (the
    /// dead-form teachings precedent).
    #[error("{message}")]
    DefaultNotConforming {
        /// The dotted place (`inputs.count.default` · `config.timeout_s.default` ·
        /// `const.label.value`).
        where_: String,
        /// The byte-mirrored teaching (what was declared · what does not
        /// fit · why the hole is closed).
        message: String,
        /// Span of the declared `type:` (the declaration is the defect site).
        span: Option<Span>,
    },
}

// The dead-form vocabulary + the teaching texts DESCENDED to
// `nika-vocab` at the C2 wall (the 15k prod-LOC budget — the teachings
// are message vocabulary, the vocabulary crate is their home · the
// `keysets` precedent). This re-export keeps the `crate::error::DeadForm`
// path byte-stable for every matcher (the CLI's fix verb included) ·
// `foreign_namespace_teaching` rides `nika_vocab` directly since its only
// caller descended to nika-check (2026-07-21).
pub use nika_vocab::DeadForm;

impl SchemaError {
    /// Project this refusal into the shared CLI/MCP diagnostic contract.
    #[must_use]
    pub fn diagnostic(&self) -> SchemaDiagnostic {
        SchemaDiagnostic {
            code: self.spec_code(),
            message: self.to_string(),
        }
    }

    /// The machine-applicable RENAME this error teaches, as the exact
    /// `(offending source token, replacement)` pair — `Some` only for the
    /// rename-shaped variants whose deterministic did-you-mean asserted a
    /// target (`buidl` → `build` for an unknown dependency · `tasks.buidl`
    /// → `tasks.build` for an unresolved reference, both sides fully
    /// qualified so a splice never strips the namespace). This is the
    /// typed half the human message renders as « did you mean ___? » —
    /// the `--fix` repair loop and any agent loop consume it without
    /// scraping prose.
    #[must_use]
    pub fn rename_repair(&self) -> Option<(String, String)> {
        match self {
            // The parse-fatal rename: an unknown key with a typed near-miss
            // (`promt` → `prompt`). `teaching` never rides here — it is
            // prose, and this door is the ONE place a repairer reads a
            // rename from (the 2026-08-18 splice-a-sentence corruption).
            Self::UnknownField {
                field,
                suggestion: Some(s),
                ..
            } => Some((field.clone(), s.clone())),
            Self::UnknownDependency {
                to,
                suggestion: Some(s),
                ..
            } => Some((to.clone(), s.clone())),
            Self::UnresolvedNamespaceRef {
                reference,
                suggestion: Some(s),
                ..
            } => Some((reference.clone(), s.clone())),
            _ => None,
        }
    }

    /// The source span of this error, when one is attached — the ONE
    /// uniform surface diagnostics renderers (the check report · the
    /// future LSP) read spans through. `Cycle` has no single span (a
    /// cycle is a path property, not a location).
    #[must_use]
    pub fn span(&self) -> Option<Span> {
        match self {
            Self::YamlSyntax { span, .. }
            | Self::MissingEnvelopeField { span, .. }
            | Self::BadNikaId { span, .. }
            | Self::UnknownField { span, .. }
            | Self::BadTaskId { span, .. }
            | Self::DuplicateTaskId { span, .. }
            | Self::W1TasksSequence { span, .. }
            | Self::W1TaskIdField { span, .. }
            | Self::MissingVerb { span, .. }
            | Self::MultipleVerbs { span, .. }
            | Self::BadTimeout { span, .. }
            | Self::BadRetry { span, .. }
            | Self::BadOnError { span, .. }
            | Self::ReservedBindingName { span, .. }
            | Self::BadSecretRef { span, .. }
            | Self::TemplateSyntax { span, .. }
            | Self::ExpressionViolation { span, .. }
            | Self::JqBindingContainsTemplate { span, .. }
            | Self::DuplicateKey { span, .. }
            | Self::MissingField { span, .. }
            | Self::Validation { span, .. }
            | Self::RunContradiction { span, .. }
            | Self::BadBuiltinArgs { span, .. }
            | Self::RecoverAwaitDeadlock { span, .. }
            | Self::WhenNotBoolean { span, .. }
            | Self::UnknownDependency { span, .. }
            | Self::W2DependsOnField { span, .. }
            | Self::D1StringCommand { span, .. }
            | Self::SkillPathNotStatic { span, .. }
            | Self::UnknownGroup { span, .. }
            | Self::UnwindInGroup { span, .. }
            | Self::UnknownAfterPredicate { span, .. }
            | Self::RefOutsideBoundary { span, .. }
            | Self::UnresolvedNamespaceRef { span, .. }
            | Self::LoopLocalOutsideForEach { span, .. }
            | Self::UnknownTaskField { span, .. }
            | Self::OutputPathProvablyInvalid { span, .. }
            | Self::BareTaskEnvelope { span, .. }
            | Self::TypeExprInvalid { span, .. }
            | Self::TypeContractDuplicated { span, .. }
            | Self::TypeUndecodable { span, .. }
            | Self::DecodeWithStructuredCapture { span, .. }
            | Self::DeadValueForm { span, .. }
            | Self::ForeignValueNamespace { span, .. }
            | Self::DefaultNotConforming { span, .. } => *span,
            Self::Cycle { .. } => None,
        }
    }

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
schema_code!(SCHEMA_282, 282, "bad-nika-id");
// 283 `bad-workflow-id` RETIRED with the envelope nuke (2026-08-12) —
// the id lives on `nika:` now and 282 judges it. Never reused.
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
// SCHEMA_294 « bad-typed-var » RETIRED (never reuse) — the R3b TypeExpr
// widen killed the variant: the declaration `type:` grammar is the type
// core's (SCHEMA_319 / NIKA-TYPE-001), the surviving shape refusals ride
// SCHEMA_299 / NIKA-PARSE-019.
schema_code!(SCHEMA_295, 295, "template-syntax");
schema_code!(SCHEMA_296, 296, "jq-binding-contains-template");
schema_code!(SCHEMA_297, 297, "duplicate-key");
schema_code!(SCHEMA_298, 298, "missing-field");
schema_code!(SCHEMA_299, 299, "validation");
schema_code!(SCHEMA_300, 300, "when-not-boolean");
schema_code!(SCHEMA_301, 301, "cycle");
schema_code!(SCHEMA_302, 302, "unknown-dependency");
// SCHEMA_303 « missing-depends-on-edge » RETIRED (never reuse) — the W2
// boundary made the class inexpressible (the with: binding IS the edge ·
// a reference outside the boundary is SCHEMA_318 / NIKA-VAR-021).
schema_code!(SCHEMA_304, 304, "unresolved-namespace-ref");
schema_code!(SCHEMA_305, 305, "loop-local-outside-for-each");
schema_code!(SCHEMA_306, 306, "unknown-task-field");
schema_code!(SCHEMA_307, 307, "output-path-provably-invalid");
schema_code!(SCHEMA_308, 308, "recover-await-deadlock");
schema_code!(SCHEMA_309, 309, "bad-builtin-args");
schema_code!(SCHEMA_310, 310, "expression-violation");
schema_code!(SCHEMA_311, 311, "bare-task-envelope");
// 312 `w1-workflow-scalar` + 313 `w1-top-level-description` RETIRED with
// the envelope nuke (2026-08-12) — both taught a migration TO the
// `workflow:` object, itself dead. Never reused.
schema_code!(SCHEMA_314, 314, "w1-tasks-sequence");
schema_code!(SCHEMA_315, 315, "w1-task-id-field");
schema_code!(SCHEMA_316, 316, "w2-depends-on-field");
schema_code!(SCHEMA_317, 317, "unknown-after-predicate");
schema_code!(SCHEMA_318, 318, "ref-outside-boundary");
schema_code!(SCHEMA_319, 319, "type-expr-invalid");
// SCHEMA_320 « type-recursive » RETIRED (never reuse) — it carried
// NIKA-TYPE-002, and the `types:` block that gave the class an object
// died with the 9-key envelope (2026-08-12). A type expression is
// self-contained, so there is no reference graph left to cycle.
schema_code!(SCHEMA_321, 321, "type-contract-duplicated");
schema_code!(SCHEMA_322, 322, "type-undecodable");
schema_code!(SCHEMA_323, 323, "decode-with-structured-capture");
schema_code!(SCHEMA_324, 324, "dead-value-form");
schema_code!(SCHEMA_325, 325, "foreign-value-namespace");
schema_code!(SCHEMA_326, 326, "default-not-conforming");
schema_code!(SCHEMA_327, 327, "run-contradiction");
schema_code!(SCHEMA_328, 328, "unknown-group");
schema_code!(SCHEMA_329, 329, "unwind-in-group");

impl NikaErrorCode for SchemaError {
    fn nika_code(&self) -> NikaCode {
        match self {
            Self::YamlSyntax { .. } => SCHEMA_280,
            Self::MissingEnvelopeField { .. } => SCHEMA_281,
            Self::BadNikaId { .. } => SCHEMA_282,
            Self::UnknownField { .. } => SCHEMA_284,
            Self::BadTaskId { .. } => SCHEMA_285,
            Self::DuplicateTaskId { .. } => SCHEMA_286,
            Self::W1TasksSequence { .. } => SCHEMA_314,
            Self::W1TaskIdField { .. } => SCHEMA_315,
            Self::W2DependsOnField { .. } => SCHEMA_316,
            Self::UnknownAfterPredicate { .. } => SCHEMA_317,
            Self::UnknownGroup { .. } => SCHEMA_328,
            Self::UnwindInGroup { .. } => SCHEMA_329,
            Self::RefOutsideBoundary { .. } => SCHEMA_318,
            Self::MissingVerb { .. } => SCHEMA_287,
            Self::MultipleVerbs { .. } => SCHEMA_288,
            Self::BadTimeout { .. } => SCHEMA_289,
            Self::BadRetry { .. } => SCHEMA_290,
            Self::BadOnError { .. } => SCHEMA_291,
            Self::ReservedBindingName { .. } => SCHEMA_292,
            Self::BadSecretRef { .. } => SCHEMA_293,
            Self::TemplateSyntax { .. } => SCHEMA_295,
            Self::ExpressionViolation { .. } => SCHEMA_310,
            Self::JqBindingContainsTemplate { .. } => SCHEMA_296,
            Self::DuplicateKey { .. } => SCHEMA_297,
            Self::MissingField { .. } => SCHEMA_298,
            // D1 keeps the GENERIC structural code (PARSE-019) — the
            // variant exists for the fix ladder's match, not the wire.
            Self::Validation { .. }
            | Self::D1StringCommand { .. }
            | Self::SkillPathNotStatic { .. } => SCHEMA_299,
            Self::RunContradiction { .. } => SCHEMA_327,
            Self::WhenNotBoolean { .. } => SCHEMA_300,
            Self::RecoverAwaitDeadlock { .. } => SCHEMA_308,
            Self::BadBuiltinArgs { .. } => SCHEMA_309,
            Self::Cycle { .. } => SCHEMA_301,
            Self::UnknownDependency { .. } => SCHEMA_302,
            Self::UnresolvedNamespaceRef { .. } => SCHEMA_304,
            Self::LoopLocalOutsideForEach { .. } => SCHEMA_305,
            Self::UnknownTaskField { .. } => SCHEMA_306,
            Self::OutputPathProvablyInvalid { .. } => SCHEMA_307,
            Self::BareTaskEnvelope { .. } => SCHEMA_311,
            Self::TypeExprInvalid { .. } => SCHEMA_319,
            Self::TypeContractDuplicated { .. } => SCHEMA_321,
            Self::TypeUndecodable { .. } => SCHEMA_322,
            Self::DecodeWithStructuredCapture { .. } => SCHEMA_323,
            Self::DeadValueForm { .. } => SCHEMA_324,
            Self::ForeignValueNamespace { .. } => SCHEMA_325,
            Self::DefaultNotConforming { .. } => SCHEMA_326,
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
        SchemaError::BadNikaId {
            id: String::new(),
            span: None,
        },
        SchemaError::UnknownField {
            field: String::new(),
            location: String::new(),
            span: None,
            suggestion: None,
            teaching: None,
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
        SchemaError::DeadValueForm {
            form: crate::error::DeadForm::Vars,
            message: String::new(),
            span: None,
        },
        SchemaError::ForeignValueNamespace {
            root: String::new(),
            message: String::new(),
            span: None,
        },
        SchemaError::DefaultNotConforming {
            where_: String::new(),
            message: String::new(),
            span: None,
        },
        SchemaError::Cycle { cycle: vec![] },
        SchemaError::UnknownDependency {
            from: String::new(),
            to: String::new(),
            suggestion: None,
            span: None,
        },
        SchemaError::W2DependsOnField {
            task: String::new(),
            task_hint: String::new(),
            provable: false,
            span: None,
        },
        SchemaError::UnknownAfterPredicate {
            message: String::new(),
            task: String::new(),
            target: String::new(),
            predicate: String::new(),
            span: None,
        },
        SchemaError::RefOutsideBoundary {
            task: String::new(),
            surface: String::new(),
            reference: String::new(),
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
        SchemaError::BareTaskEnvelope {
            task: String::new(),
            location: String::new(),
            span: None,
        },
    ]
    .into_iter()
    .chain(type_level_variants())
    .collect()
}

/// The W3 type-core variants (spec 09 · split out of
/// [`analysis_level_variants`] at the 100-line fn ratchet).
#[cfg(test)]
fn type_level_variants() -> Vec<SchemaError> {
    vec![
        // NIKA-TYPE-001 here — the SAME variant also emits TYPE-006
        // (regex dialect) via `num: 6`; enumerating it once keeps the
        // engine-internal `nika_code()` uniqueness law intact, and the
        // TYPE-006 wire mapping is pinned by its own spec_code test.
        SchemaError::TypeExprInvalid {
            num: 1,
            detail: String::new(),
            span: None,
        },
        SchemaError::TypeContractDuplicated {
            task: String::new(),
            verb: "infer",
            span: None,
        },
        SchemaError::TypeUndecodable {
            task: String::new(),
            decode: String::new(),
            span: None,
        },
        SchemaError::DecodeWithStructuredCapture {
            task: String::new(),
            span: None,
        },
    ]
}

/// The W2 teaching, honest per shape AND about its own limits: the shape
/// clause is spoken only for a sequence the migrator's scanner reads (no
/// entry anything but a bare task id), any other shape is named as the
/// author's to rewrite — and the accepted shape says out loud that the
/// whole-file stops still apply, so the finding never promises a repair
/// the fixer then refuses on the next screen.
fn w2_depends_on_message(task: &str, task_hint: &str, provable: bool) -> String {
    if provable {
        format!(
            "task `{task}` carries `depends_on:` — dead since W2; data → `with:` bindings (the binding IS the edge) · control → `after: {{{task_hint}: success}}` (`nika check --fix` can migrate this shape — no entry is anything but a bare task id; it still stops the file when a producer may skip (`when:` · `for_each:` · `on_error.skip`) or is read only through its status)"
        )
    } else {
        format!(
            "task `{task}` carries `depends_on:` — dead since W2; data → `with:` bindings (the binding IS the edge) · control → `after: {{{task_hint}: success}}` (`nika check --fix` leaves this shape to you — it is not a list of bare task ids; write the `after:` map by hand)"
        )
    }
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
    fn bad_nika_id_display() {
        let err = SchemaError::BadNikaId {
            id: "My_Workflow".into(),
            span: None,
        };
        assert!(err.to_string().contains("My_Workflow"), "{err}");
        assert!(err.to_string().contains("kebab-case"), "{err}");
        // The teaching names the SHAPE, never the dead literal — a
        // reader must not learn `v1` from the refusal that killed it.
        assert!(!err.to_string().contains("exactly `v1`"), "{err}");
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

    #[test]
    fn schema_diagnostic_is_typed_and_names_the_next_action_exactly() {
        let err = SchemaError::UnknownField {
            field: "entropy".into(),
            location: "the workflow envelope".into(),
            span: None,
            suggestion: None,
            teaching: Some("the fields here: nika · tasks".into()),
        };
        let diagnostic = err.diagnostic();
        assert_eq!(diagnostic.code.to_string(), "NIKA-PARSE-005");
        assert_eq!(diagnostic.message, err.to_string());
        assert_eq!(
            diagnostic.to_string(),
            "[NIKA-PARSE-005] unknown field `entropy` in the workflow envelope (strict mode) — the fields here: nika · tasks · → nika explain NIKA-PARSE-005"
        );
    }
}
