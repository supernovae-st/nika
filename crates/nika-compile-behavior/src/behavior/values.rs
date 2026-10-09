// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The values a judgment compares. A JSON value is typed: its kind counts, and two numbers
//! compare by value (`70` equals `70.0`). A CSV cell is loose text: it carries no kind, so a
//! comparison involving one renders both sides as text and compares numbers by value wherever
//! the number law reads one. A missing field and `null` read the same, as they do in jq. No
//! text is read as a date or an instant: text order stands for time order only where the two
//! texts share their date-time form and offset.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use super::numbers::{Decimal, Law, law, number_like};
use crate::instant_shape;

/// One value a source holds or an output states.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Datum {
    /// JSON `null`, or a field a record does not carry.
    Null,
    /// A JSON truth value.
    Bool(bool),
    /// A JSON number, exact.
    Number(Decimal),
    /// A JSON string or a CSV cell.
    Text(String),
    /// A JSON array.
    List(Vec<Datum>),
    /// A JSON object (the order of its keys in a file never matters).
    Record(BTreeMap<String, Datum>),
}

/// A value and how its source types it: `loose` for a CSV cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Cell {
    pub(crate) datum: Datum,
    pub(crate) loose: bool,
}

impl Cell {
    pub(crate) fn typed(datum: Datum) -> Self {
        Self {
            datum,
            loose: false,
        }
    }

    pub(crate) fn of(datum: Datum, loose: bool) -> Self {
        Self { datum, loose }
    }
}

/// One record or one written row, by column.
pub(crate) type Row = BTreeMap<String, Cell>;

/// A record read from a source whose values all carry the same typing.
pub(crate) fn row_of(fields: BTreeMap<String, Datum>, loose: bool) -> Row {
    fields
        .into_iter()
        .map(|(key, datum)| (key, Cell::of(datum, loose)))
        .collect()
}

/// The value of `column` in `row`; `null` for a column the row does not carry.
pub(crate) fn value_at<'a>(row: &'a Row, column: &str) -> &'a Datum {
    const NULL: &Datum = &Datum::Null;
    row.get(column).map_or(NULL, |cell| &cell.datum)
}

/// What the number law reads in a value: a JSON number, or a text the law's grammar accepts.
pub(crate) fn reading(datum: &Datum) -> Law {
    match datum {
        Datum::Number(number) => Law::Number(number.clone()),
        Datum::Text(text) => law(text),
        Datum::Null | Datum::Bool(_) | Datum::List(_) | Datum::Record(_) => Law::NotANumber,
    }
}

/// A value read as text the way jq's `tostring` reads it, when that reading does not depend on
/// how the runtime formats a number (a number, list or object has no such reading here).
pub(crate) fn as_text(datum: &Datum) -> Option<String> {
    match datum {
        Datum::Null => Some("null".to_owned()),
        Datum::Text(text) => Some(text.clone()),
        Datum::Bool(truth) => Some(truth.to_string()),
        Datum::Number(_) | Datum::List(_) | Datum::Record(_) => None,
    }
}

/// The key a sort with no stated number policy orders a source column by, as the canonical
/// reading states it: a value holding a number sorts as that number, any other value as
/// itself.
///
/// # Errors
///
/// The reason, when the value is a text that looks like a number the law does not read: the
/// runtime's reading of it decides its place, which is no stated fact.
pub(crate) fn sort_key(datum: &Datum) -> Result<Datum, String> {
    match reading(datum) {
        Law::Number(number) => Ok(Datum::Number(number)),
        Law::Beyond => Err(format!(
            "{} is a number beyond the supported precision",
            shown(datum)
        )),
        Law::NotANumber => {
            if let Datum::Text(text) = datum
                && number_like(text)
            {
                return Err(format!(
                    "{text:?} looks like a number the number law does not read"
                ));
            }
            Ok(datum.clone())
        }
    }
}

/// The rank of a value's kind in jq's total order.
fn rank(datum: &Datum) -> u8 {
    match datum {
        Datum::Null => 0,
        Datum::Bool(false) => 1,
        Datum::Bool(true) => 2,
        Datum::Number(_) => 3,
        Datum::Text(_) => 4,
        Datum::List(_) => 5,
        Datum::Record(_) => 6,
    }
}

/// jq's order between two values, when it does not depend on comparing two lists or two
/// objects (their order is left unjudged here).
pub(crate) fn jq_order(left: &Datum, right: &Datum) -> Option<Ordering> {
    match (left, right) {
        (Datum::Number(a), Datum::Number(b)) => Some(a.cmp(b)),
        (Datum::Text(a), Datum::Text(b)) => Some(a.cmp(b)),
        (Datum::List(_), Datum::List(_)) | (Datum::Record(_), Datum::Record(_)) => None,
        _ => Some(rank(left).cmp(&rank(right))),
    }
}

/// Whether the order of two values is the order of what they denote: not for two date-time
/// texts of different forms or offsets, whose text order is not their time order.
pub(crate) fn order_is_meaningful(left: &Datum, right: &Datum) -> bool {
    if let (Datum::Text(a), Datum::Text(b)) = (left, right)
        && let (Some(first), Some(second)) = (instant_shape(a), instant_shape(b))
    {
        return first == second;
    }
    true
}

/// jq's equality between two values: numbers by value, everything else by kind and content.
pub(crate) fn jq_equal(left: &Datum, right: &Datum) -> bool {
    left == right
}

/// Append `text` with its length, so that no text can end inside another one's form.
fn push_text(out: &mut String, text: &str) {
    out.push_str(&text.len().to_string());
    out.push(':');
    out.push_str(text);
}

/// The typed comparison form of a value: its kind and content, numbers by value.
fn typed_form(datum: &Datum, out: &mut String) {
    match datum {
        Datum::Null => out.push('z'),
        Datum::Bool(true) => out.push('t'),
        Datum::Bool(false) => out.push('f'),
        Datum::Number(number) => {
            out.push('n');
            push_text(out, &number.to_string());
        }
        Datum::Text(text) => {
            out.push('s');
            push_text(out, text);
        }
        Datum::List(items) => {
            out.push('l');
            push_text(out, &items.len().to_string());
            for item in items {
                typed_form(item, out);
            }
        }
        Datum::Record(fields) => {
            out.push('r');
            push_text(out, &fields.len().to_string());
            for (key, value) in fields {
                push_text(out, key);
                typed_form(value, out);
            }
        }
    }
}

/// The loose comparison form of a value: its text, a number where the law reads one; `None`
/// for a value with no text (null, a missing field or empty text), which a CSV cannot tell
/// apart.
fn loose_form(datum: &Datum) -> Option<String> {
    let text = match datum {
        Datum::Null => return None,
        Datum::Bool(truth) => truth.to_string(),
        Datum::Number(number) => number.to_string(),
        Datum::Text(text) => text.clone(),
        Datum::List(_) | Datum::Record(_) => {
            let mut out = String::from("x");
            typed_form(datum, &mut out);
            return Some(out);
        }
    };
    if text.is_empty() {
        return None;
    }
    let mut out = String::new();
    if let Law::Number(number) = law(&text) {
        out.push('n');
        push_text(&mut out, &number.to_string());
    } else {
        out.push('s');
        push_text(&mut out, &text);
    }
    Some(out)
}

/// The comparison form of one value, loose when either side of the comparison is loose; `None`
/// for a value that reads as absent (typed `null`, or no text in a loose comparison).
pub(crate) fn form(datum: &Datum, loose: bool) -> Option<String> {
    if loose {
        return loose_form(datum);
    }
    if *datum == Datum::Null {
        return None;
    }
    let mut out = String::new();
    typed_form(datum, &mut out);
    Some(out)
}

/// The comparison form of a whole row: each column that holds a value, in column order, loose
/// for a column any side reads loosely.
pub(crate) fn row_form(row: &Row, loose_columns: &BTreeSet<String>) -> String {
    let mut out = String::new();
    for (column, cell) in row {
        let loose = cell.loose || loose_columns.contains(column);
        if let Some(value) = form(&cell.datum, loose) {
            push_text(&mut out, column);
            out.push_str(&value);
        }
    }
    out
}

/// The exact form of a whole row: every value typed, a CSV cell compared as the text it is.
pub(crate) fn exact_row_form(row: &Row) -> String {
    let mut out = String::new();
    for (column, cell) in row {
        if let Some(value) = form(&cell.datum, false) {
            push_text(&mut out, column);
            out.push_str(&value);
        }
    }
    out
}

/// How two cells of one column compare.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Same {
    /// Equal under jq's equality.
    Yes,
    /// Different under every reading.
    No,
    /// Different texts that read as the same value once a CSV cell is read loosely (`70` and
    /// `70.0`): whether they are the same is no stated fact.
    Loosely,
}

/// How the values of one column compare in two rows (a missing column reads as `null`).
pub(crate) fn same(left: Option<&Cell>, right: Option<&Cell>) -> Same {
    const NULL: &Datum = &Datum::Null;
    let (a, b) = (
        left.map_or(NULL, |cell| &cell.datum),
        right.map_or(NULL, |cell| &cell.datum),
    );
    if jq_equal(a, b) {
        return Same::Yes;
    }
    let loose = left.is_some_and(|cell| cell.loose) || right.is_some_and(|cell| cell.loose);
    if loose && form(a, true) == form(b, true) {
        Same::Loosely
    } else {
        Same::No
    }
}

/// A short human rendering of a value for evidence messages.
pub(crate) fn shown(datum: &Datum) -> String {
    match datum {
        Datum::Null => "null".to_owned(),
        Datum::Bool(truth) => truth.to_string(),
        Datum::Number(number) => number.to_string(),
        Datum::Text(text) => format!("{text:?}"),
        Datum::List(items) => format!("a list of {}", items.len()),
        Datum::Record(fields) => format!("an object of {} keys", fields.len()),
    }
}

/// A row as a reader sees it, for evidence messages.
pub(crate) fn shown_row(row: &Row) -> String {
    let cells: Vec<String> = row
        .iter()
        .map(|(column, cell)| format!("{column}: {}", shown(&cell.datum)))
        .collect();
    format!("{{{}}}", cells.join(", "))
}
