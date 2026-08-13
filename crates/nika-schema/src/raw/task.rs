// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `RawTask` — a single task in the raw workflow AST.
//!
//! Canonical v1 task shape per spec `03-dag.md` §forward-compat +
//! NEP-0004 law 7's one grammar addition ·
//! « v1 ships with these task fields · `with` · `after` · `when` ·
//! `for_each` · `max_parallel` · `fail_fast` · `retry` · `on_error` ·
//! `timeout` · `on_finally` · `extract` · `declassify` · plus the verb
//! selector. »
//! The set is CLOSED — strict mode rejects anything else.

use std::time::Duration;

use crate::source::Spanned;
use crate::types::{AfterPredicate, OnError, RetryConfig, WhenGate};

use super::action::RawAction;

/// WHICH law one `lift:` entry opens (spec `10-authority.md` rule 1 ·
/// the enum is CLOSED: v1 knows two doors, and 24 error-bearing laws
/// exist. A law with no door cannot be lifted at all — that is the
/// default and the common case).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LiftLaw {
    /// Raise ONE binding from untrusted to trusted (NEP-0004 ·
    /// LAW-AUTH-0325). Requires `from:`.
    Taint,
    /// Declare this task's fetch a code-bearing artifact it will never
    /// load or run (NEP-0006 · LAW-AUTH-0327). Forbids `from:`.
    DataAsCode,
}

/// One `lift:` entry — the authored door. Each opens exactly ONE named
/// law, with a mandatory reason, check-visible and receipt-recorded.
///
/// A lift is NEVER a permit bypass: the value still sits inside the
/// declared boundary, it never touches the `net.http` host set, and it
/// never lowers the SSRF floor. It moves the named law and nothing else.
///
/// This replaces the two predecessors — a `declassify:` list and an
/// `inert:` string — which did the same job in two spellings (spec 10
/// §the authored doors · « the law is a parameter of `lift:`, not a
/// field »).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LiftEntry {
    /// `law:` — which door (closed enum · parser-enforced).
    pub law: Spanned<LiftLaw>,
    /// `from:` — the ONE binding a `taint` entry raises (`inputs.p` ·
    /// `config.region` · `tasks.fetch.output`), a dotted value-binding
    /// path kept verbatim (the taint oracle matches it against the
    /// canonical dotted form of each reference). Law-specific: required
    /// by `taint`, `None` for every other law (parser-enforced).
    pub from: Option<Spanned<String>>,
    /// `because:` — the non-empty justification (parser-enforced) ·
    /// recorded in the run receipt and projected in the certificate.
    pub because: Spanned<String>,
}

/// A raw task — a single step in the workflow DAG.
///
/// Semantic validation (cycle detection · reference resolution · the
/// `when:` boolean-shape rule) happens in the analyzer, not here.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RawTask {
    /// `id:` — `snake_case` · unique within the workflow (CEL-safe).
    pub id: Spanned<String>,
    /// `after:` — the CONTROL boundary · `{producer: predicate}` ·
    /// each entry is one control edge (spec 03 §after · W2).
    pub after: Vec<(Spanned<String>, Spanned<AfterPredicate>)>,
    /// `when:` — the LOCAL business condition · evaluated POST-gate ·
    /// a single boolean CEL island over the value authorities + loop
    /// locals `{inputs · config · const · secrets · with · item · index}`
    /// OR the YAML boolean literal (spec 03 §when).
    pub when: Option<Spanned<WhenGate>>,
    /// `for_each:` — map this task over a collection (spec `03-dag.md`
    /// · « The collection is either a literal list or a reference to
    /// an upstream task's array output »).
    pub for_each: Option<Spanned<ForEachValue>>,
    /// `max_parallel:` — cap concurrent `for_each` iterations (≥ 1).
    pub max_parallel: Option<Spanned<u32>>,
    /// `fail_fast:` — abort-on-error policy for `for_each` (default true).
    pub fail_fast: Option<Spanned<bool>>,
    /// `retry:` — transient-error retry policy (spec 05).
    pub retry: Option<Spanned<RetryConfig>>,
    /// `on_error:` — terminal-error recovery (spec 05).
    pub on_error: Option<Spanned<OnError>>,
    /// `timeout:` — Go-duration hard kill · parsed + range-checked.
    pub timeout: Option<Spanned<Duration>>,
    /// `with:` — task-scope variable injection (`${{ with.X }}`).
    pub with: Vec<(Spanned<String>, Spanned<serde_json::Value>)>,
    /// `extract:` — named jq bindings over the verb's raw response,
    /// read downstream as `${{ tasks.X.<name> }}`. The field names the
    /// OPERATION (run this jq); it does NOT write `output` —
    /// `${{ tasks.X.output }}` stays the raw response. The old spelling
    /// `output:` lied twice, measured at 0.108.0: `output: { output: "." }`
    /// refused its own name, and the field never wrote `output`.
    pub extract: Vec<(Spanned<String>, Spanned<String>)>,
    /// `returns:` — the task's output contract (spec 09 · a named type
    /// or an inline type expression · RAW here, parsed by the type core
    /// at check time).
    pub returns: Option<Spanned<serde_json::Value>>,
    /// `lift:` — the authored doors (spec 10 §the authored doors). ONE
    /// construct for every law; read it through [`RawTask::taint_lifts`]
    /// and [`RawTask::data_as_code_because`] rather than matching the
    /// enum at each call site.
    pub lift: Vec<LiftEntry>,
    /// `group:` — fan-in MEMBERSHIP (spec 03 §group). This task JOINS
    /// the named set; a consumer folds the whole set with one
    /// `${{ group.<name> }}` binding. Membership is DECLARED, never
    /// matched: a renamed member leaves its group loudly
    /// (`NIKA-DAG-008` on the reference), where a glob would shrink
    /// the fold in silence and the run would stay green.
    pub group: Option<Spanned<String>>,
    /// The verb (exactly one · parser-enforced).
    pub action: RawAction,
}

impl RawTask {
    /// Create a new raw task with the given id and action.
    #[must_use]
    pub fn new(id: Spanned<String>, action: RawAction) -> Self {
        Self {
            id,
            after: Vec::new(),
            when: None,
            for_each: None,
            max_parallel: None,
            fail_fast: None,
            retry: None,
            on_error: None,
            timeout: None,
            with: Vec::new(),
            extract: Vec::new(),
            returns: None,
            lift: Vec::new(),
            group: None,
            action,
        }
    }

    /// The `taint` doors — each raises the ONE binding its `from:` names
    /// (NEP-0004). The two laws share one construct; consumers that care
    /// about a specific law read it through here so the enum match lives
    /// in ONE place.
    pub fn taint_lifts(&self) -> impl Iterator<Item = &LiftEntry> {
        self.lift
            .iter()
            .filter(|e| matches!(e.law.value, LiftLaw::Taint))
    }

    /// The `data-as-code` door's justification, when this task declares
    /// one (NEP-0006) — the reason the fetch is archived and never run.
    #[must_use]
    pub fn data_as_code_because(&self) -> Option<&Spanned<String>> {
        self.lift
            .iter()
            .find(|e| matches!(e.law.value, LiftLaw::DataAsCode))
            .map(|e| &e.because)
    }
}

/// The `for_each:` collection source.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ForEachValue {
    /// `for_each: ${{ … }}` — an expression string (a single island).
    Expression(String),
    /// `for_each: [a, b, c]` — a literal YAML list.
    List(serde_json::Value),
}

/// One `on_finally:` cleanup mini-task.
///
#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Span;

    fn span_str(s: &str) -> Spanned<String> {
        Spanned::new(s.to_owned(), Span::default())
    }

    #[test]
    fn new_has_empty_optionals() {
        let action = RawAction::Exec(super::super::action::RawExecAction::new(span_str("echo")));
        let task = RawTask::new(span_str("my_task"), action);
        assert_eq!(task.id.value, "my_task");
        assert!(task.after.is_empty());
        assert!(task.when.is_none());
        assert!(task.for_each.is_none());
        assert!(task.max_parallel.is_none());
        assert!(task.fail_fast.is_none());
        assert!(task.retry.is_none());
        assert!(task.on_error.is_none());
        assert!(task.timeout.is_none());
        assert!(task.with.is_empty());
        assert!(task.extract.is_empty());
        assert!(task.returns.is_none());
    }
}
