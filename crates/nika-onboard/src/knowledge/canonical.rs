// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The byte contract a knowledge release shares with its producer (the r1 contract §6.1
//! and §9.1).
//!
//! **Strict JSON, bounded while it is read** (§9.1).
//! - Arrays and objects nest at most [`MAX_DEPTH`] deep: the top-level one at 1, a scalar adding
//!   no depth.
//! - A text holds at most the values its caller allows. Every object, array, string, member name,
//!   number and literal counts once, every occurrence: a key stated twice counts twice, and so
//!   does the value it first held.
//! - Nothing may follow the value but JSON whitespace, and nothing at all in a line.
//! - Every bound is judged as the text is read, before the excess value is built. Every value is
//!   read and judged before the next key, so the first value of a key stated twice is judged in
//!   full: a fault in it is the text's fault.
//! - A key an object states twice is reported only in a text that is otherwise valid within its
//!   bounds: a syntax or bound fault comes first.
//!
//! **The canonical form a row's digest is computed over.** It is the producer's
//! `json.dumps(row, sort_keys=True, ensure_ascii=False, separators=(",", ":"))`, byte for byte:
//! - object keys sorted by code point;
//! - no whitespace;
//! - a string escaped only where JSON requires it;
//! - an integer in decimal.
//!
//! A number that is not an integer has no canonical form here.

use std::cell::{Cell, RefCell};
use std::fmt;

use nika_event::source_id::sha256_hex;
use serde::Deserialize as _;
use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

/// The deepest an array or object may nest; the top-level one is at depth 1.
pub(crate) const MAX_DEPTH: usize = 16;

/// Why a text is not strict JSON within its bounds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StrictJsonError {
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
pub(crate) fn strict_line(line: &str, max_values: usize) -> Result<Value, StrictJsonError> {
    if line.ends_with([' ', '\t', '\n', '\r']) {
        return Err(StrictJsonError::Malformed(
            "something after the value of a line".to_owned(),
        ));
    }
    strict_json(line, max_values)
}

/// Parse one strict JSON text of at most `max_values` values (§9.1): nested at most
/// [`MAX_DEPTH`] deep, nothing after the value but whitespace, every key once.
pub(crate) fn strict_json(text: &str, max_values: usize) -> Result<Value, StrictJsonError> {
    let budget = Budget {
        values: Cell::new(max_values),
        duplicate: RefCell::new(None),
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

/// What one parse may still spend, and the first key an object stated twice.
struct Budget {
    values: Cell<usize>,
    duplicate: RefCell<Option<String>>,
}

impl Budget {
    /// Count one value, or stop the parse at the bound.
    fn spend<E: de::Error>(&self) -> Result<(), E> {
        let Some(left) = self.values.get().checked_sub(1) else {
            return Err(E::custom("more values than this text may hold"));
        };
        self.values.set(left);
        Ok(())
    }
}

/// One value read at `depth` against the parse's budget: a seed and its own visitor.
#[derive(Clone, Copy)]
struct Strict<'a> {
    budget: &'a Budget,
    depth: usize,
}

impl Strict<'_> {
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

impl<'de> DeserializeSeed<'de> for Strict<'_> {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Strict<'_> {
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
        Ok(Value::from(value))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Value, E> {
        self.budget.spend()?;
        Ok(Value::from(value))
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Value, E> {
        self.budget.spend()?;
        Number::from_f64(value)
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
struct Key<'a>(&'a Budget);

impl<'de> DeserializeSeed<'de> for Key<'_> {
    type Value = String;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<String, D::Error> {
        self.0.spend()?;
        String::deserialize(deserializer)
    }
}

/// Whether a value holds a number anywhere (a row line holds none, §6.1).
pub(crate) fn holds_number(value: &Value) -> bool {
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
pub(crate) fn canonical_json(value: &Value) -> Option<String> {
    let mut out = String::new();
    write_canonical(value, &mut out).then_some(out)
}

/// The digest a release row carries in its `sha256` field: the sha256 of the canonical JSON of
/// the row without that field. `None` when the row is not an object or holds a number that is
/// not an integer.
#[must_use]
pub(crate) fn row_digest(row: &Value) -> Option<String> {
    let mut unsigned = row.as_object()?.clone();
    unsigned.remove("sha256");
    canonical_json(&Value::Object(unsigned)).map(|text| sha256_hex(text.as_bytes()))
}

/// Append the canonical text of `value`; `false` when a number is not an integer.
fn write_canonical(value: &Value, out: &mut String) -> bool {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Value::Number(number) => match number.as_i64().map(|n| n.to_string()) {
            Some(text) => out.push_str(&text),
            None => match number.as_u64() {
                Some(n) => out.push_str(&n.to_string()),
                None => return false,
            },
        },
        Value::String(text) => out.push_str(&Value::String(text.clone()).to_string()),
        Value::Array(items) => {
            out.push('[');
            for (at, item) in items.iter().enumerate() {
                if at > 0 {
                    out.push(',');
                }
                if !write_canonical(item, out) {
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
                    .is_some_and(|item| write_canonical(item, out))
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
