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
//!
//! Pure over the host's observation (`serde_json::Value`): the compiler that grounds a typed rule
//! and the semantic route that records what its candidate reads ([`semantic`]) share it, and the
//! source basis a host judges at a yes is graded again by the same [`grade`].

pub mod semantic;

use serde_json::{Value, json};

/// How strongly the source supports a key (closed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grade {
    /// A CSV/TSV header lists it.
    Declared,
    /// A host that read the whole artifact observed it.
    ObservedComplete,
    /// A bounded sample shows it.
    ObservedPartial,
    /// The request's column list or an answer asserts it.
    UserAsserted,
    /// Nothing supports it.
    Inferred,
}

impl Grade {
    /// The grade as the decision record spells it.
    #[must_use]
    pub const fn word(self) -> &'static str {
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
pub struct Seen {
    /// Every key seen.
    pub all: Vec<String>,
    /// The keys every sampled record holds.
    pub everywhere: Vec<String>,
    /// Whether a header declares them.
    pub declared: bool,
    /// Whether the host read the whole artifact.
    pub complete: bool,
}

/// The one observation row of a path; `None` when the world holds none or several.
#[must_use]
pub fn row<'a>(world: Option<&'a Value>, path: &str) -> Option<&'a Value> {
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
#[must_use]
pub fn seen(row: Option<&Value>) -> Option<Seen> {
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
#[must_use]
pub fn grade(key: &str, seen: Option<&Seen>, stated_columns: &[String]) -> (Grade, bool) {
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
#[must_use]
pub fn revision(row: Option<&Value>) -> String {
    let word = |key: &str| row.and_then(|r| r.get(key)).and_then(Value::as_str);
    (word("peek_sha256").or_else(|| word("state")))
        .unwrap_or("unobserved")
        .to_owned()
}

/// Whether a program's text compares `field` to the string `literal` literally (F2-Q1): `.F` (a
/// bare key) or `."F"`, then `==` or `!=`, then the literal as JSON, or the operands reversed,
/// apart only by whitespace and set off on each side by a token that binds more loosely
/// ([`looser`]). No jq is parsed: any other shape (a nested path, a longer key, a tighter
/// operator, the literal elsewhere) is no comparison.
#[must_use]
pub fn compares(jq: &str, field: &str, literal: &str) -> bool {
    let quoted = |text: &str| Value::String(text.to_owned()).to_string();
    let mut keys = vec![format!(".{}", quoted(field))];
    if bare(field) {
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

/// Whether a key may be written `.key` in jq: an identifier.
fn bare(field: &str) -> bool {
    field
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && field.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
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
/// bound by the request's words, an answer, an approval, the observation of a value the request
/// states, or a semantic candidate's literal read ([`semantic`]); open when some sampled records
/// lack the key.
pub struct Entry<'a> {
    /// The rule (or the candidate) that reads the key.
    pub rule: &'a str,
    /// The key.
    pub key: &'a str,
    /// The source path the key is read from.
    pub source: &'a str,
    /// The observation row of that source.
    pub row: Option<&'a Value>,
    /// The key's grade.
    pub grade: Grade,
    /// Whether every sampled record holds it.
    pub everywhere: bool,
    /// What binds it to the request.
    pub bound_by: Option<&'static str>,
}

impl Entry<'_> {
    /// Graded above `inferred` and bound.
    #[must_use]
    pub fn admissible(&self) -> bool {
        self.grade != Grade::Inferred && self.bound_by.is_some()
    }
    /// Admissible while some sampled records lack the key.
    #[must_use]
    pub fn open(&self) -> bool {
        self.admissible() && !self.everywhere
    }
    /// The entry as `decision.grounding` records it.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({"rule": self.rule, "field": self.key, "source": self.source,
            "revision": revision(self.row), "grade": self.grade.word(),
            "in_every_sampled_record": self.everywhere, "bound_by": self.bound_by,
            "admissible": self.admissible(),
            "open": self.open().then_some("records lacking the key")})
    }
}

#[cfg(test)]
mod tests;
