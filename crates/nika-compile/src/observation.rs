// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The pure half of what a host observes about a file a request states (R4 A5, descended from
//! `nika-cli-host`'s observer, which keeps the I/O: resolution under the project root, the
//! bounded peek, the whole read of a small JSON file and the links it never follows out). From
//! a peeked head or parsed records: a CSV header's columns and delimiter, the records' keys, the
//! short categorical values of a column (a status · a kind · a currency — never free text), and
//! the raw kind of every sampled value, counted, never quoted. A sample is never a schema: it
//! proves nothing about the unread rest of a file.

use nika_compile_fidelity::observed::temporal_shapes;
use serde_json::{Map, Value, json};

/// The most rows a value set or a kind count is read from, and the most distinct values a column
/// may hold to count as categorical (a status · a kind · a currency — never free text).
const SAMPLE_ROWS: usize = 200;
const CATEGORICAL_MAX: usize = 8;
const VALUE_MAX_CHARS: usize = 32;

/// What one peeked head or one set of parsed records shows.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Sample {
    /// Every column or key positively observed: a CSV header, the union of the records' keys.
    pub columns: Vec<String>,
    /// The keys every record carries (JSON and JSONL only).
    pub common: Option<Vec<String>>,
    /// The delimiter that cut a CSV header.
    pub delimiter: Option<char>,
    /// The distinct short values of every categorical column.
    pub values: Vec<(String, Value)>,
    /// `{"sampled": n, "keys": {key: {kind: count}}}` (with `"nonobject": n` for records that
    /// are no object): how many sampled values of each key are of each raw kind — `number`,
    /// `number_text`, `text`, `empty`, `null`, `boolean`, `array`, `object`, `absent`.
    /// Optional `temporal` counts mask date-time digits; they prove no valid instant.
    pub kinds: Value,
    /// The bounded nested key paths and kinds below the records' arrays and objects, with their
    /// coverage (`nika_compile_fidelity::observed::nested`); `Value::Null` when there are none.
    pub nested: Value,
}

/// A CSV or TSV head: its header columns and delimiter, the categorical values and the kinds of
/// its first data rows.
#[must_use]
pub fn csv(head: &str, tsv: bool) -> Sample {
    let (columns, delimiter) = header_columns(head, tsv);
    let values = csv_values(head, delimiter, &columns);
    let kinds = csv_kinds(head, delimiter, &columns);
    Sample {
        columns,
        common: None,
        delimiter: Some(delimiter),
        values,
        kinds,
        nested: Value::Null,
    }
}

/// Parsed JSON or JSONL records: their keys, the keys every record carries, the categorical
/// values and the kinds of the first records.
#[must_use]
pub fn records(rows: &[Value]) -> Sample {
    Sample {
        columns: keys_of_rows(rows),
        common: Some(common_keys(rows)),
        delimiter: None,
        values: categorical(rows),
        kinds: record_kinds(rows),
        nested: nika_compile_fidelity::observed::nested(rows, &kind),
    }
}

/// The parsed lines of a JSONL head (a cut last line is skipped).
#[must_use]
pub fn jsonl(head: &str) -> Vec<Value> {
    head.lines()
        .filter(|l| !l.trim().is_empty())
        .take(SAMPLE_ROWS)
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .collect()
}

// The observed-number law belongs to the shared observation owner.
pub use nika_compile_fidelity::observed::basis::number_text;

/// The header line's columns and the delimiter that cut it (the most frequent of `,` `;`
/// `\t` `|` on the first line; a TSV is tab-cut).
fn header_columns(head: &str, tsv: bool) -> (Vec<String>, char) {
    let first = head
        .lines()
        .next()
        .unwrap_or("")
        .trim_start_matches('\u{feff}');
    let delimiter = if tsv {
        '\t'
    } else {
        [',', ';', '\t', '|']
            .into_iter()
            .max_by_key(|d| first.matches(*d).count())
            .filter(|d| first.contains(*d))
            .unwrap_or(',')
    };
    let columns = first
        .split(delimiter)
        .map(|c| {
            c.trim()
                .trim_matches('"')
                .trim_matches('\'')
                .trim()
                .to_owned()
        })
        .filter(|c| !c.is_empty())
        .collect();
    (columns, delimiter)
}

/// The data rows of a CSV head the samples read, each cut into its trimmed fields (naive cut: a
/// row whose field count differs from the header's is skipped).
fn csv_rows<'a>(head: &'a str, delimiter: char, columns: &[String]) -> Vec<Vec<&'a str>> {
    head.lines()
        .skip(1)
        .take(SAMPLE_ROWS)
        .map(|line| {
            line.split(delimiter)
                .map(|field| field.trim().trim_matches('"').trim())
                .collect::<Vec<_>>()
        })
        .filter(|fields| fields.len() == columns.len())
        .collect()
}

/// The distinct values of every categorical column of a CSV head.
fn csv_values(head: &str, delimiter: char, columns: &[String]) -> Vec<(String, Value)> {
    let rows = csv_rows(head, delimiter, columns);
    if rows.len() < 2 {
        return Vec::new();
    }
    let mut sets: Vec<Vec<String>> = vec![Vec::new(); columns.len()];
    for fields in &rows {
        for (set, value) in sets.iter_mut().zip(fields) {
            if !value.is_empty() && !set.iter().any(|v| v == value) {
                set.push((*value).to_owned());
            }
        }
    }
    columns
        .iter()
        .zip(sets)
        .filter(|(_, set)| categorical_set(set, rows.len()))
        .map(|(column, set)| {
            (
                column.clone(),
                Value::Array(set.into_iter().map(Value::String).collect()),
            )
        })
        .collect()
}

/// A value set is categorical when it is small, shorter than the rows it came from, and every
/// value is short.
fn categorical_set(set: &[String], rows: usize) -> bool {
    !set.is_empty()
        && set.len() <= CATEGORICAL_MAX
        && set.len() < rows
        && set.iter().all(|v| v.chars().count() <= VALUE_MAX_CHARS)
}

/// The distinct string values of every categorical key across the sampled objects.
fn categorical(rows: &[Value]) -> Vec<(String, Value)> {
    if rows.len() < 2 {
        return Vec::new();
    }
    keys_of_rows(rows)
        .into_iter()
        .filter_map(|key| {
            let mut set: Vec<String> = Vec::new();
            let mut present = 0_usize;
            for row in rows.iter().take(SAMPLE_ROWS) {
                let Some(value) = row.get(&key).and_then(Value::as_str) else {
                    continue;
                };
                present += 1;
                if !value.is_empty() && !set.iter().any(|v| v == value) {
                    set.push(value.to_owned());
                }
            }
            (present >= 2 && categorical_set(&set, present)).then(|| {
                (
                    key,
                    Value::Array(set.into_iter().map(Value::String).collect()),
                )
            })
        })
        .collect()
}

/// Every key positively observed; neither this union nor the sample is a schema.
fn keys_of_rows(rows: &[Value]) -> Vec<String> {
    rows.iter()
        .filter_map(Value::as_object)
        .flat_map(|o| o.keys().cloned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn common_keys(rows: &[Value]) -> Vec<String> {
    keys_of_rows(rows)
        .into_iter()
        .filter(|key| rows.iter().all(|row| row.get(key).is_some()))
        .collect()
}

/// The raw kind of a text: blank, a decimal the number-text law reads, or any other text.
fn text_kind(text: &str) -> &'static str {
    if text.trim().is_empty() {
        "empty"
    } else if number_text(text) {
        "number_text"
    } else {
        "text"
    }
}

/// The raw kind of one JSON value.
fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(text) => text_kind(text),
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// One key's kinds, counted.
fn tally<'a>(kinds: impl Iterator<Item = &'a str>) -> Value {
    let mut counts: Map<String, Value> = Map::new();
    for kind in kinds {
        let n = counts.get(kind).and_then(Value::as_u64).unwrap_or(0) + 1;
        counts.insert(kind.to_owned(), json!(n));
    }
    Value::Object(counts)
}

/// The kinds of the sampled data rows of a CSV head, by column.
fn csv_kinds(head: &str, delimiter: char, columns: &[String]) -> Value {
    let rows = csv_rows(head, delimiter, columns);
    let keys: Map<String, Value> = columns
        .iter()
        .enumerate()
        .map(|(i, column)| (column.clone(), tally(rows.iter().map(|r| text_kind(r[i])))))
        .collect();
    let temporal: Map<String, Value> = columns
        .iter()
        .enumerate()
        .filter_map(|(i, key)| {
            let formats = temporal_shapes(rows.iter().map(|r| Some(r[i])));
            (!formats.is_null()).then(|| (key.clone(), formats))
        })
        .collect();
    let mut out = json!({"sampled": rows.len(), "keys": keys});
    if !temporal.is_empty() {
        out["temporal"] = Value::Object(temporal);
    }
    out
}

/// The kinds of the sampled records, by key; a key a record lacks is `absent` there.
fn record_kinds(rows: &[Value]) -> Value {
    let sampled = &rows[..rows.len().min(SAMPLE_ROWS)];
    let objects: Vec<&Map<String, Value>> = sampled.iter().filter_map(Value::as_object).collect();
    let mut temporal = Map::new();
    let keys: Map<String, Value> = keys_of_rows(sampled)
        .into_iter()
        .map(|key| {
            let formats =
                temporal_shapes(objects.iter().map(|o| o.get(&key).and_then(Value::as_str)));
            if !formats.is_null() {
                temporal.insert(key.clone(), formats);
            }
            let kinds = objects.iter().map(|o| o.get(&key).map_or("absent", kind));
            (key.clone(), tally(kinds))
        })
        .collect();
    let mut entry = json!({"sampled": sampled.len(), "keys": keys});
    if !temporal.is_empty() {
        entry["temporal"] = Value::Object(temporal);
    }
    if objects.len() < sampled.len() {
        entry["nonobject"] = json!(sampled.len() - objects.len());
    }
    entry
}

#[cfg(test)]
mod tests;
