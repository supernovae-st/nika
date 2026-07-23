// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `RawTask` — a single task in the raw workflow AST.
//!
//! Canonical v1 task shape per spec `03-dag.md` §forward-compat +
//! NEP-0004 law 7's one grammar addition ·
//! « v1 ships with these task fields · `with` · `after` · `when` ·
//! `for_each` · `max_parallel` · `fail_fast` · `retry` · `on_error` ·
//! `timeout` · `on_finally` · `output` · `declassify` · plus the verb
//! selector. »
//! The set is CLOSED — strict mode rejects anything else.

use std::time::Duration;

use crate::source::Spanned;
use crate::types::{AfterPredicate, OnError, RetryConfig, WhenGate};

use super::action::RawAction;

/// One `declassify:` entry (NEP-0004 law 5 · the ONLY door through the
/// permit-parameterization taint): raise ONE binding from untrusted to
/// trusted, authored and check-visible. Lifts the TAINT law only — the
/// value is still matched against the declared boundary (never a permit
/// bypass) — and the run receipt records it (taint path · `because` ·
/// value digest).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DeclassifyEntry {
    /// `from:` — the ONE binding this entry raises (`inputs.p` ·
    /// `config.region` · `tasks.fetch.output`) — a dotted value-binding
    /// path, kept verbatim (the taint oracle matches it against the
    /// canonical dotted form of each reference).
    pub from: Spanned<String>,
    /// `to:` — the target integrity label · v1 knows exactly one raise:
    /// `trusted` (parser-enforced).
    pub to: Spanned<String>,
    /// `because:` — the non-empty justification (parser-enforced) ·
    /// recorded in the run receipt.
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
    /// `output:` — named jq bindings over the verb's raw response.
    pub output: Vec<(Spanned<String>, Spanned<String>)>,
    /// `returns:` — the task's output contract (spec 09 · a named type
    /// or an inline type expression · RAW here, parsed by the type core
    /// at check time).
    pub returns: Option<Spanned<serde_json::Value>>,
    /// `on_finally:` — cleanup mini-tasks · ALWAYS run (spec 03).
    pub on_finally: Vec<Spanned<RawFinallyTask>>,
    /// `declassify:` — the task-level taint-lift declarations (NEP-0004
    /// law 5 · the only door through the re-gate · receipt-recorded).
    pub declassify: Vec<DeclassifyEntry>,
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
            output: Vec::new(),
            returns: None,
            on_finally: Vec::new(),
            declassify: Vec::new(),
            action,
        }
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
/// Spec `03-dag.md` §`on_finally` · « **List of mini-tasks** · zero or
/// more · each with its own verb » · may carry its own `when:` (e.g.
/// only-on-error notification) and a per-cleanup `timeout:`.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RawFinallyTask {
    /// `when:` — conditional cleanup (sees the parent's status/error) ·
    /// same two forms as the task-level gate.
    pub when: Option<Spanned<WhenGate>>,
    /// `timeout:` — per-cleanup-task override (default 30s · engine).
    pub timeout: Option<Spanned<Duration>>,
    /// The cleanup verb.
    pub action: RawAction,
}

impl RawFinallyTask {
    /// Create a cleanup mini-task with the given action.
    #[must_use]
    pub fn new(action: RawAction) -> Self {
        Self {
            when: None,
            timeout: None,
            action,
        }
    }
}

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
        assert!(task.output.is_empty());
        assert!(task.returns.is_none());
        assert!(task.on_finally.is_empty());
    }

    #[test]
    fn finally_task_new() {
        let action = RawAction::Exec(super::super::action::RawExecAction::new(span_str(
            "rm -f x",
        )));
        let f = RawFinallyTask::new(action);
        assert!(f.when.is_none());
        assert!(f.timeout.is_none());
    }
}
