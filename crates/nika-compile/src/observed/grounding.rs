// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! One grounding law for the source keys a typed rule reads (R4 S1): the evidence a key has,
//! its grade, what binds it to the request, and whether it may reach lowering. The assembler runs
//! it on creation and on replay alike, the decision record carries exactly what it decided
//! (`decision.grounding`), and a key without admissible evidence never reaches a READY candidate.
//! - A CSV/TSV header declares its columns (`declared`); a host that read the whole artifact
//!   observed it (`observed_complete`); a bounded sample shows a key in some or every sampled
//!   record (`observed_partial`: presence, never absence, and never the unread tail).
//! - A human asserts a key: the request's own column list (« (columns vehicle, driver, km) »), or
//!   an answer given in the very context it was asked (`user_asserted`).
//! - Anything else is `inferred`: a request word no observation or declaration supports, an
//!   observed key the request never states (a seat's choice). Never admissible alone.
//! - A key present in some sampled records only is grounded, but what a rule does with records
//!   lacking it is an operator law (S3) the request must state: that obligation stays open.
use serde_json::{Value, json};

/// How strongly the source supports a key (closed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Grade {
    Declared,
    ObservedComplete,
    ObservedPartial,
    UserAsserted,
    Inferred,
}

impl Grade {
    pub(crate) const fn word(self) -> &'static str {
        match self {
            Self::Declared => "declared",
            Self::ObservedComplete => "observed_complete",
            Self::ObservedPartial => "observed_partial",
            Self::UserAsserted => "user_asserted",
            Self::Inferred => "inferred",
        }
    }
}

/// What one observation row shows: every key seen, the keys every sampled record holds,
/// whether the file declares them (a header) and whether the host read everything.
pub(crate) struct Seen {
    pub all: Vec<String>,
    pub everywhere: Vec<String>,
    pub declared: bool,
    pub complete: bool,
}

/// The one observation row of a path; `None` when the world holds none or several.
pub(crate) fn row<'a>(world: Option<&'a Value>, path: &str) -> Option<&'a Value> {
    let bare = |p: &str| p.strip_prefix("./").unwrap_or(p).to_owned();
    let path = bare(path);
    let rows = world?.get("observed")?.as_array()?;
    let mut matched = rows
        .iter()
        .filter(|row| (row.get("path").and_then(Value::as_str)).is_some_and(|p| bare(p) == path));
    let first = matched.next()?;
    matched.next().is_none().then_some(first)
}

/// The keys an observed row shows; `None` when it observed no record (absent, unreadable,
/// empty, unknown, outside the project, or no row at all).
pub(crate) fn seen(row: Option<&Value>) -> Option<Seen> {
    let row = row?;
    if row.get("state").and_then(Value::as_str) != Some("observed") {
        return None;
    }
    let names = |key: &str| -> Option<Vec<String>> {
        let values = row.get(key)?.as_array()?;
        (values.iter())
            .map(|v| v.as_str().filter(|s| !s.is_empty()).map(str::to_owned))
            .collect()
    };
    let all = names("columns")?;
    let declared = row.get("kind").and_then(Value::as_str) == Some("csv");
    let everywhere = if declared {
        all.clone()
    } else {
        names("common_columns").unwrap_or_default()
    };
    let complete = row.get("complete").and_then(Value::as_bool) == Some(true);
    Some(Seen {
        all,
        everywhere,
        declared,
        complete,
    })
}

/// A key's grade and whether every sampled record holds it. A header lists every name, so a
/// key it lacks is no key the request can declare; a partial sample disproves nothing.
pub(crate) fn grade(key: &str, seen: Option<&Seen>, stated_columns: &[String]) -> (Grade, bool) {
    match seen {
        Some(seen) if seen.all.iter().any(|k| k == key) => {
            let everywhere = seen.everywhere.iter().any(|k| k == key);
            let grade = if seen.declared {
                Grade::Declared
            } else if seen.complete {
                Grade::ObservedComplete
            } else {
                Grade::ObservedPartial
            };
            (grade, everywhere)
        }
        Some(seen) if seen.declared || seen.complete => (Grade::Inferred, false),
        _ if stated_columns.iter().any(|c| c == key) => (Grade::UserAsserted, true),
        _ => (Grade::Inferred, false),
    }
}

/// The revision an observation binds: the hash of its bounded peek, never the unread rest; the
/// state word when it read no record; `unobserved` with no row.
pub(crate) fn revision(row: Option<&Value>) -> String {
    let word = |key: &str| row.and_then(|r| r.get(key)).and_then(Value::as_str);
    (word("peek_sha256").or_else(|| word("state")))
        .unwrap_or("unobserved")
        .to_owned()
}

/// Whether an answer round's observation of `path` differs from the one the replayed record
/// asked its questions against: its answers then map another revision. With no fresh
/// observation, the record's own is all there is to judge.
pub(crate) fn stale(request: &crate::CompileRequest, path: &str) -> bool {
    let Some(record) = request.plan.as_ref() else {
        return false;
    };
    let asked = record.get("observed_world");
    let now = request.knowledge.as_ref().or(asked);
    row(asked, path) != row(now, path)
}

/// One key's grounding as the decision carries it: admissible when graded above `inferred` and
/// bound by the request's words, an answer or an approval; open when some sampled records lack
/// the key.
pub(crate) struct Entry<'a> {
    pub rule: &'a str,
    pub key: &'a str,
    pub source: &'a str,
    pub row: Option<&'a Value>,
    pub grade: Grade,
    pub everywhere: bool,
    pub bound_by: Option<&'static str>,
}

impl Entry<'_> {
    pub(crate) fn admissible(&self) -> bool {
        self.grade != Grade::Inferred && self.bound_by.is_some()
    }
    pub(crate) fn open(&self) -> bool {
        self.admissible() && !self.everywhere
    }
    pub(crate) fn to_json(&self) -> Value {
        json!({"rule": self.rule, "field": self.key, "source": self.source,
            "revision": revision(self.row), "grade": self.grade.word(),
            "in_every_sampled_record": self.everywhere, "bound_by": self.bound_by,
            "admissible": self.admissible(),
            "open": self.open().then_some("records lacking the key")})
    }
}
