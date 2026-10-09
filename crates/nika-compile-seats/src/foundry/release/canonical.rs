// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The byte contract a knowledge release shares with its producer (the r1 contract §6.1
//! and §9.1; profile r2 adds RFC 8785 numbers).
//!
//! **Strict JSON, bounded while it is read** (§9.1).
//! - Arrays and objects nest at most [`MAX_DEPTH`]
//!   deep: the top-level one at 1, a scalar adding no depth.
//! - A text holds at most the values its caller allows. Every object, array, string, member name,
//!   number and literal counts once, every occurrence: a key stated twice counts twice, and so
//!   does the value it first held.
//! - Nothing may follow the value but JSON whitespace, and nothing at all in a line.
//! - Every bound is judged as the text is read, before the excess value is built. Every value is
//!   read and judged before the next key, so the first value of a key stated twice is judged in
//!   full: a fault in it is the text's fault.
//! - A key an object states twice is reported only in a text that is otherwise valid within its
//!   bounds: a syntax or bound fault comes first.
//! - A number is the double its literal denotes, correctly rounded from the literal's own text,
//!   whatever precision the JSON parser would give it: a verdict never depends on how a build
//!   parses floats. A literal past the largest double is not a number of this grammar.
//!
//! **The canonical form a row's digest is computed over.** It is the producer's
//! `json.dumps(row, sort_keys=True, ensure_ascii=False, separators=(",", ":"))`, byte for byte:
//! - object keys sorted by code point;
//! - no whitespace;
//! - a string escaped only where JSON requires it;
//! - an integer in decimal.
//!
//! A number that is not an integer has no canonical form there (profile r1). Profile r2 writes
//! numbers as RFC 8785 (JCS) does ([`jcs_json`]): a safe integer as its digits, any other number
//! as the ECMAScript text of its double ([`es_number`]).

use std::cell::{Cell, RefCell};
use std::fmt;

use nika_compile::surface::sha256;
use serde::Deserialize as _;
use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

/// The deepest an array or object may nest; the top-level one is at depth 1.
pub const MAX_DEPTH: usize = 16;

/// The largest integer a double holds exactly, with every smaller one (2^53 − 1).
pub const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

/// Why a text is not strict JSON within its bounds.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum StrictJsonError {
    /// An object states this key twice, in a text otherwise valid JSON within its bounds.
    DuplicateKey(String),
    /// Not JSON, something after the value, or past a bound (the parser's words).
    Malformed(String),
}

impl fmt::Display for StrictJsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateKey(key) => write!(f, "the key `{key}` is stated twice"),
            Self::Malformed(words) => f.write_str(words),
        }
    }
}

/// One JSONL line (§9.1): strict JSON of at most `max_values` values, with nothing at all after
/// its value. A value never ends with JSON whitespace, so a line that does breaks the grammar.
///
/// # Errors
/// [`StrictJsonError`]: not strict JSON within its bounds, or a key stated twice.
pub fn strict_line(line: &str, max_values: usize) -> Result<Value, StrictJsonError> {
    if line.ends_with([' ', '\t', '\n', '\r']) {
        return Err(StrictJsonError::Malformed(
            "something after the value of a line".to_owned(),
        ));
    }
    strict_json(line, max_values)
}

/// Parse one strict JSON text of at most `max_values` values (§9.1): nested at most
/// [`MAX_DEPTH`] deep, nothing after the value but whitespace, every key once.
///
/// # Errors
/// [`StrictJsonError`]: not strict JSON within its bounds, or a key stated twice.
pub fn strict_json(text: &str, max_values: usize) -> Result<Value, StrictJsonError> {
    let budget = Budget {
        values: Cell::new(max_values),
        duplicate: RefCell::new(None),
        literals: number_literals(text),
        next: Cell::new(0),
    };
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let value = Strict {
        budget: &budget,
        depth: 1,
    }
    .deserialize(&mut deserializer)
    .and_then(|value| deserializer.end().map(|()| value))
    .map_err(|error| StrictJsonError::Malformed(error.to_string()))?;
    match budget.duplicate.into_inner() {
        Some(key) => Err(StrictJsonError::DuplicateKey(key)),
        None => Ok(value),
    }
}

/// What one parse may still spend, the first key an object stated twice, and the text's number
/// literals in document order with the next one to read.
struct Budget<'t> {
    values: Cell<usize>,
    duplicate: RefCell<Option<String>>,
    literals: Vec<&'t str>,
    next: Cell<usize>,
}

impl<'t> Budget<'t> {
    /// Count one value, or stop the parse at the bound.
    fn spend<E: de::Error>(&self) -> Result<(), E> {
        let Some(left) = self.values.get().checked_sub(1) else {
            return Err(E::custom("more values than this text may hold"));
        };
        self.values.set(left);
        Ok(())
    }

    /// The literal of the number the parser reads now: numbers are read in document order.
    fn literal(&self) -> Option<&'t str> {
        let at = self.next.get();
        self.next.set(at.saturating_add(1));
        self.literals.get(at).copied()
    }
}

/// The number literals of a JSON text in document order: a strict parse reads each number from
/// its own literal. A text that is not JSON is refused by the parse whatever this finds in it.
fn number_literals(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut at = 0;
    let mut in_string = false;
    while let Some(&byte) = bytes.get(at) {
        if in_string {
            match byte {
                b'\\' => at = at.saturating_add(1),
                b'"' => in_string = false,
                _ => {}
            }
            at = at.saturating_add(1);
        } else if byte == b'"' {
            in_string = true;
            at = at.saturating_add(1);
        } else if byte == b'-' || byte.is_ascii_digit() {
            let rest = bytes.get(at..).unwrap_or_default();
            let len = rest
                .iter()
                .position(|b| !matches!(b, b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E'))
                .unwrap_or(rest.len());
            let end = at.saturating_add(len);
            found.extend(text.get(at..end));
            at = end;
        } else {
            at = at.saturating_add(1);
        }
    }
    found
}

/// One value read at `depth` against the parse's budget: a seed and its own visitor.
#[derive(Clone, Copy)]
struct Strict<'a, 't> {
    budget: &'a Budget<'t>,
    depth: usize,
}

impl Strict<'_, '_> {
    /// Enter an array or object: within the depth bound, and one value spent.
    fn open<E: de::Error>(self) -> Result<Self, E> {
        if self.depth > MAX_DEPTH {
            return Err(E::custom("nested deeper than the bound"));
        }
        self.budget.spend()?;
        Ok(Self {
            budget: self.budget,
            depth: self.depth + 1,
        })
    }
}

impl<'de> DeserializeSeed<'de> for Strict<'_, '_> {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Strict<'_, '_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Value, E> {
        self.budget.spend()?;
        Ok(Value::Bool(value))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Value, E> {
        self.budget.spend()?;
        self.budget.literal();
        Ok(Value::from(value))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Value, E> {
        self.budget.spend()?;
        self.budget.literal();
        Ok(Value::from(value))
    }

    /// Any other number: the double its literal denotes, correctly rounded by `str::parse`.
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Value, E> {
        self.budget.spend()?;
        let exact = self
            .budget
            .literal()
            .and_then(|literal| literal.parse::<f64>().ok())
            .unwrap_or(value);
        Number::from_f64(exact)
            .map(Value::Number)
            .ok_or_else(|| E::custom("a number that is not finite"))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Value, E> {
        self.budget.spend()?;
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Value, E> {
        self.budget.spend()?;
        Ok(Value::String(value))
    }

    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        self.budget.spend()?;
        Ok(Value::Null)
    }

    fn visit_none<E: de::Error>(self) -> Result<Value, E> {
        self.visit_unit()
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let inner = self.open()?;
        let mut items = Vec::new();
        while let Some(item) = seq.next_element_seed(inner)? {
            items.push(item);
        }
        Ok(Value::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let inner = self.open()?;
        let mut object = Map::new();
        while let Some(key) = map.next_key_seed(Key(self.budget))? {
            let value = map.next_value_seed(inner)?;
            if object.contains_key(&key) {
                let mut first = self.budget.duplicate.borrow_mut();
                if first.is_none() {
                    *first = Some(key);
                }
            } else {
                object.insert(key, value);
            }
        }
        Ok(Value::Object(object))
    }
}

/// A member name read against the parse's budget: a member name is a value (§9.1).
struct Key<'a, 't>(&'a Budget<'t>);

impl<'de> DeserializeSeed<'de> for Key<'_, '_> {
    type Value = String;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<String, D::Error> {
        self.0.spend()?;
        String::deserialize(deserializer)
    }
}

/// Whether a value holds a number anywhere (a row line holds none, §6.1).
pub fn holds_number(value: &Value) -> bool {
    match value {
        Value::Number(_) => true,
        Value::Array(items) => items.iter().any(holds_number),
        Value::Object(fields) => fields.values().any(holds_number),
        Value::Null | Value::Bool(_) | Value::String(_) => false,
    }
}

/// The canonical JSON text of a value, as the release producer writes it: object keys sorted by
/// code point, no whitespace, strings escaped only where JSON requires it, integers in decimal.
/// `None` when the value holds a number that is not an integer (no canonical form here).
#[must_use]
pub fn canonical_json(value: &Value) -> Option<String> {
    let mut out = String::new();
    write_canonical(value, &mut out, integer_text).then_some(out)
}

/// The canonical JSON text of a value under profile r2: [`canonical_json`]'s keys, separators and
/// strings, with RFC 8785 numbers ([`jcs_number`]).
#[must_use]
pub fn jcs_json(value: &Value) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out, |number| Some(jcs_number(number)));
    out
}

/// The digest a profile r2 row carries in its `sha256` field: the sha256 of [`jcs_json`] of the
/// row without that field. `None` when the row is not an object.
#[must_use]
pub fn jcs_row_digest(row: &Value) -> Option<String> {
    let mut unsigned = row.as_object()?.clone();
    unsigned.remove("sha256");
    Some(sha256(&jcs_json(&Value::Object(unsigned))))
}

/// The RFC 8785 text of a number: an integer of at most 2^53 − 1 in magnitude as its digits,
/// any other number as the ECMAScript text of the double it denotes ([`es_number`]).
#[must_use]
pub fn jcs_number(number: &Number) -> String {
    let double = if let Some(n) = number.as_u64() {
        if n <= MAX_SAFE_INTEGER {
            return n.to_string();
        }
        integer_double(n, false)
    } else if let Some(n) = number.as_i64() {
        if n.unsigned_abs() <= MAX_SAFE_INTEGER {
            return n.to_string();
        }
        integer_double(n.unsigned_abs(), true)
    } else {
        number.as_f64().unwrap_or_default()
    };
    es_number(double).unwrap_or_else(|| "0".to_owned())
}

/// The double an integer past 2^53 − 1 denotes: the nearest one, ties to even, as JCS reads it.
#[expect(
    clippy::cast_precision_loss,
    reason = "JCS has doubles only: an integer past 2^53 is the double nearest to it"
)]
fn integer_double(magnitude: u64, negative: bool) -> f64 {
    let double = magnitude as f64;
    if negative { -double } else { double }
}

/// The ECMAScript `Number::toString` text of a finite double (RFC 8785 §3.2.2.3): its shortest
/// round-trip digits, the closest to the double and the even one on a tie, laid out as
/// ECMAScript lays them out, as the JCS canonicalizer (ryu-js) writes them. A zero of either sign
/// is `0`. `None` for a double that is not finite.
#[must_use]
pub fn es_number(value: f64) -> Option<String> {
    if !value.is_finite() {
        return None;
    }
    serde_json_canonicalizer::to_string(&value).ok()
}

/// Profile r1's numbers: an integer in decimal, nothing else.
fn integer_text(number: &Number) -> Option<String> {
    match number.as_i64() {
        Some(n) => Some(n.to_string()),
        None => number.as_u64().map(|n| n.to_string()),
    }
}

/// The digest a release row carries in its `sha256` field: the sha256 of the canonical JSON of
/// the row without that field. `None` when the row is not an object or holds a number that is
/// not an integer.
#[must_use]
pub fn row_digest(row: &Value) -> Option<String> {
    let mut unsigned = row.as_object()?.clone();
    unsigned.remove("sha256");
    canonical_json(&Value::Object(unsigned)).map(|text| sha256(&text))
}

/// Append the canonical text of `value`, each number as `number` writes it; `false` when it
/// writes none.
fn write_canonical(value: &Value, out: &mut String, number: fn(&Number) -> Option<String>) -> bool {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Value::Number(n) => match number(n) {
            Some(text) => out.push_str(&text),
            None => return false,
        },
        Value::String(text) => out.push_str(&Value::String(text.clone()).to_string()),
        Value::Array(items) => {
            out.push('[');
            for (at, item) in items.iter().enumerate() {
                if at > 0 {
                    out.push(',');
                }
                if !write_canonical(item, out, number) {
                    return false;
                }
            }
            out.push(']');
        }
        Value::Object(object) => {
            let mut keys: Vec<&String> = object.keys().collect();
            keys.sort();
            out.push('{');
            for (at, key) in keys.into_iter().enumerate() {
                if at > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String(key.clone()).to_string());
                out.push(':');
                if !object
                    .get(key)
                    .is_some_and(|item| write_canonical(item, out, number))
                {
                    return false;
                }
            }
            out.push('}');
        }
    }
    true
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;
