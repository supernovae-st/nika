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
//! - A key the request never names is bound by the observation when the request states its value
//!   ([`witness`], F2-Q1): a literal the rule compares the key to, recorded by the host among that
//!   key's values and no other column's. A sample that never shows it proves nothing.
use crate::rules::Rule;
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

/// The literal that witnesses a seat's `field` the request never names (F2-Q1): the rule compares
/// `field` to it (a typed text equality or inequality, or a verified program's literal comparison,
/// [`compares`]), the request states it with identifier boundaries, it names no observed column,
/// and the host recorded it, exactly or canonically equivalent (the one spelling law, R4 A5),
/// among the values of `field` and of no other column. A sample that never shows it, or a column
/// whose values the host did not record, proves nothing: no witness, and the question stays.
pub(crate) fn witness(
    rule: &Rule,
    field: &str,
    intent: &str,
    row: Option<&Value>,
    seen: &Seen,
) -> Option<String> {
    let recorded = row?.get("values")?.as_object()?;
    let spellings = |values: &Value| -> Vec<String> {
        let texts = values.as_array().into_iter().flatten();
        texts.filter_map(Value::as_str).map(str::to_owned).collect()
    };
    let holds = |values: &Value, literal: &str| {
        let observed = spellings(values);
        observed.iter().any(|v| v == literal)
            || !crate::surface::observed::equivalent_spellings(literal, &observed).is_empty()
    };
    let own = recorded.get(field)?;
    let compared: Vec<String> = match rule.verified_program() {
        Some(program) => (spellings(own).into_iter())
            .filter(|value| compares(&program.jq, field, value))
            .collect(),
        None => (rule.text_equalities().into_iter())
            .filter_map(|(key, literal)| (key == field).then_some(literal))
            .collect(),
    };
    compared.into_iter().find(|literal| {
        super::names_field(intent, literal)
            && !seen.all.iter().any(|column| column == literal)
            && holds(own, literal)
            && (recorded.iter())
                .filter(|(column, _)| column.as_str() != field)
                .all(|(_, values)| !holds(values, literal))
    })
}

/// Whether a program's text compares `field` to the string `literal` literally (F2-Q1): `.F` (a
/// bare key) or `."F"`, then `==` or `!=`, then the literal as JSON, or the operands reversed,
/// apart only by whitespace and set off on each side by a token that binds more loosely
/// ([`looser`]). No jq is parsed: any other shape (a nested path, a longer key, a tighter
/// operator, the literal elsewhere) is no comparison.
fn compares(jq: &str, field: &str, literal: &str) -> bool {
    let quoted = |text: &str| Value::String(text.to_owned()).to_string();
    let bare = field
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && field.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    let mut keys = vec![format!(".{}", quoted(field))];
    if bare {
        keys.push(format!(".{field}"));
    }
    let value = quoted(literal);
    keys.iter().any(|key| {
        ["==", "!="].into_iter().any(|op| {
            in_order(jq, [key.as_str(), op, value.as_str()])
                || in_order(jq, [value.as_str(), op, key.as_str()])
        })
    })
}

/// Whether `text` holds the three tokens in order, apart only by whitespace, between tokens that
/// bind more loosely than a comparison.
fn in_order(text: &str, [left, op, right]: [&str; 3]) -> bool {
    text.match_indices(left).any(|(at, _)| {
        let rest = text[at + left.len()..].trim_start();
        let Some(rest) = rest.strip_prefix(op) else {
            return false;
        };
        let Some(rest) = rest.trim_start().strip_prefix(right) else {
            return false;
        };
        looser(text[..at].trim_end(), false) && looser(rest.trim_start(), true)
    })
}

/// Whether the jq beside a comparison binds more loosely than it: nothing, a bracket, a pipe, a
/// comma, `;`, `//`, `and` or `or`, or a conditional's keyword (`after`: the text that follows it,
/// else the text before it).
fn looser(beside: &str, after: bool) -> bool {
    let marks: &[&str] = if after {
        &[")", "]", "}", ",", "|", ";", "//"]
    } else {
        &["(", "[", "{", ",", "|", ";", "//"]
    };
    let words: &[&str] = if after {
        &["and", "or", "then", "elif", "else", "end"]
    } else {
        &["and", "or", "if", "elif", "then", "else"]
    };
    let word = |c: char| c.is_alphanumeric() || matches!(c, '_' | '.' | '$' | '@');
    let bounded = |w: &&str| {
        if after {
            beside
                .strip_prefix(*w)
                .is_some_and(|r| !r.starts_with(word))
        } else {
            beside.strip_suffix(*w).is_some_and(|r| !r.ends_with(word))
        }
    };
    beside.is_empty()
        || marks.iter().any(|m| {
            if after {
                beside.starts_with(m)
            } else {
                beside.ends_with(m)
            }
        })
        || words.iter().any(bounded)
}

/// One key's grounding as the decision carries it: admissible when graded above `inferred` and
/// bound by the request's words, an answer, an approval or the observation of a value the request
/// states ([`witness`]); open when some sampled records lack the key.
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
