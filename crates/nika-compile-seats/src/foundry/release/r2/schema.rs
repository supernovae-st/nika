// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! F7 · a value against a closed schema of profile r2, in three passes over the whole value:
//! unknown keys, then missing keys, then types. The first pass that finds a fault gives the code.
//! The unknown and missing passes descend into the fields an object holds and the items of a list
//! of objects, as the producer's reference does.

use std::collections::BTreeSet;

use serde_json::{Map, Value};

use super::profile::{Obj, Profile, Ty};
use super::{MAX_COUNT, MAX_TEXT_BYTES};
use crate::foundry::release::grammar::{hex64, id_name, line, spec_sha, text};

/// The first fault of `value` against the closed `schema`: its code and detail.
pub(super) fn fault(
    value: &Value,
    schema: &Ty,
    profile: &Profile,
) -> Option<(&'static str, String)> {
    unknown(value, schema, "")
        .map(|detail| ("ROW_UNKNOWN_FIELD", detail))
        .or_else(|| missing(value, schema, "").map(|detail| ("ROW_MISSING_FIELD", detail)))
        .or_else(|| problem(value, schema, "", profile).map(|detail| ("ROW_FIELD_TYPE", detail)))
}

/// The values a pass descends into: the inner type of a nullable value, the fields an object
/// holds, the items of a list.
fn children<'v>(value: &'v Value, ty: &'v Ty, path: &str) -> Vec<(&'v Value, &'v Ty, String)> {
    match (ty, value) {
        (Ty::Nullable(_), Value::Null) => Vec::new(),
        (Ty::Nullable(inner), _) => children(value, inner, path),
        (Ty::Obj(obj), Value::Object(fields)) => obj
            .fields
            .iter()
            .filter_map(|(name, sub)| {
                fields
                    .get(name)
                    .map(|child| (child, sub, format!("{path}{name}.")))
            })
            .collect(),
        (Ty::List(items, _, _), Value::Array(list)) => list
            .iter()
            .map(|item| (item, items.as_ref(), format!("{path}[].")))
            .collect(),
        _ => Vec::new(),
    }
}

fn unknown(value: &Value, ty: &Ty, path: &str) -> Option<String> {
    if let (Ty::Obj(obj), Value::Object(fields)) = (ty, value) {
        let known = |key: &String| obj.fields.iter().any(|(name, _)| name == key);
        if let Some(key) = fields.keys().find(|key| !known(key)) {
            return Some(format!("{path}{key} is not a field of this schema"));
        }
    }
    children(value, ty, path)
        .into_iter()
        .find_map(|(child, sub, at)| unknown(child, sub, &at))
}

fn missing(value: &Value, ty: &Ty, path: &str) -> Option<String> {
    if let (Ty::Obj(obj), Value::Object(fields)) = (ty, value) {
        let required = |name: &&String| !obj.optional.contains(name);
        let absent = obj
            .fields
            .iter()
            .map(|(name, _)| name)
            .filter(required)
            .find(|name| !fields.contains_key(*name));
        if let Some(name) = absent {
            return Some(format!("{path}{name} is missing"));
        }
    }
    children(value, ty, path)
        .into_iter()
        .find_map(|(child, sub, at)| missing(child, sub, &at))
}

/// The type pass: the first value that is not of its type.
fn problem(value: &Value, ty: &Ty, path: &str, profile: &Profile) -> Option<String> {
    let at = || {
        let at = path.trim_end_matches('.');
        if at.is_empty() { "the row" } else { at }.to_owned()
    };
    let fits = match ty {
        Ty::Nullable(_) if value.is_null() => return None,
        Ty::Nullable(inner) => return problem(value, inner, path, profile),
        Ty::Obj(obj) => return object(value, obj, path, profile, &at),
        Ty::List(..) | Ty::Lines(..) | Ty::Ids(..) => return list(value, ty, path, profile, &at),
        Ty::Map(values, max) => return map(value, values, *max, path, profile, &at),
        Ty::Count => value.as_u64().is_some_and(|n| n <= MAX_COUNT),
        Ty::Number => value.is_number(),
        Ty::Json => return json(value).map(|why| format!("{}: {why}", at())),
        // A number never stands where a schema types anything but a count, a number or json.
        _ if value.is_number() => false,
        Ty::Line => value.as_str().is_some_and(line),
        Ty::Text => value
            .as_str()
            .is_some_and(|s| text(s, MAX_TEXT_BYTES, false)),
        Ty::MaybeText => value
            .as_str()
            .is_some_and(|s| text(s, MAX_TEXT_BYTES, true)),
        Ty::Hex64 => hex64(value),
        Ty::SpecSha => spec_sha(value),
        Ty::Bool => value.is_boolean(),
        Ty::Enum(values) => value
            .as_str()
            .is_some_and(|s| values.iter().any(|v| v == s)),
        Ty::Body(kind) => value
            .as_str()
            .and_then(|path| profile.body_kind(path))
            .is_some_and(|found| found.name() == kind),
        Ty::Id(kinds) => id(value, kinds, profile),
    };
    (!fits).then(|| format!("{}: not of its type {ty:?}", at()))
}

fn object(
    value: &Value,
    obj: &Obj,
    path: &str,
    profile: &Profile,
    at: &dyn Fn() -> String,
) -> Option<String> {
    let Some(fields) = value.as_object() else {
        return Some(format!("{}: not an object", at()));
    };
    obj.fields.iter().find_map(|(name, sub)| {
        fields
            .get(name)
            .and_then(|child| problem(child, sub, &format!("{path}{name}."), profile))
    })
}

fn list(
    value: &Value,
    ty: &Ty,
    path: &str,
    profile: &Profile,
    at: &dyn Fn() -> String,
) -> Option<String> {
    let (min, max) = match ty {
        Ty::List(_, min, max) | Ty::Lines(min, max) | Ty::Ids(_, min, max) => (*min, *max),
        _ => return None,
    };
    let Some(items) = value
        .as_array()
        .filter(|items| (min..=max).contains(&items.len()))
    else {
        return Some(format!("{}: not a list of {min} to {max} items", at()));
    };
    if let Ty::Ids(kinds, ..) = ty {
        let distinct: BTreeSet<String> = items.iter().map(Value::to_string).collect();
        if distinct.len() != items.len() {
            return Some(format!("{}: an id stated twice", at()));
        }
        return items
            .iter()
            .any(|item| !id(item, kinds, profile))
            .then(|| format!("{}[]: not an id of {}", at(), kinds.join("/")));
    }
    let item_ty = match ty {
        Ty::List(items, ..) => items.as_ref(),
        _ => &Ty::Line,
    };
    items
        .iter()
        .find_map(|item| problem(item, item_ty, &format!("{path}[]."), profile))
}

fn map(
    value: &Value,
    values: &Ty,
    max: usize,
    path: &str,
    profile: &Profile,
    at: &dyn Fn() -> String,
) -> Option<String> {
    let Some(entries) = value.as_object().filter(|entries| entries.len() <= max) else {
        return Some(format!("{}: not an object of at most {max} entries", at()));
    };
    entries.iter().find_map(|(key, child)| {
        if facet_name(key) {
            problem(child, values, &format!("{path}{key}."), profile)
        } else {
            Some(format!("{path}{key}: not a facet name"))
        }
    })
}

/// An id of one of `kinds`: `prefix:name`, the prefix a kind's, the name the id grammar's.
fn id(value: &Value, kinds: &[String], profile: &Profile) -> bool {
    value
        .as_str()
        .and_then(|id| id.split_once(':'))
        .is_some_and(|(prefix, name)| {
            prefix_token(prefix)
                && id_name(name)
                && profile
                    .kind_of_prefix(prefix)
                    .is_some_and(|kind| kinds.iter().any(|k| k == kind.name()))
        })
}

/// A json value: strings in the text grammar (empty admitted), numbers (canonical, as the line
/// layer judged them), booleans, null, arrays, and objects whose keys are lines.
fn json(value: &Value) -> Option<String> {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => None,
        Value::String(s) => {
            (!text(s, MAX_TEXT_BYTES, true)).then(|| "a string outside the text grammar".to_owned())
        }
        Value::Array(items) => items.iter().find_map(json),
        Value::Object(fields) => keyed(fields),
    }
}

fn keyed(fields: &Map<String, Value>) -> Option<String> {
    fields.iter().find_map(|(key, child)| {
        if line(key) {
            json(child)
        } else {
            Some(format!("the key {key:?} is not a line"))
        }
    })
}

/// An id prefix: `[a-z][a-z_]*`.
fn prefix_token(prefix: &str) -> bool {
    prefix
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase())
        && prefix.chars().all(|c| c.is_ascii_lowercase() || c == '_')
}

/// A facet name: `[a-z0-9][a-z0-9_-]{0,63}`.
pub(super) fn facet_name(key: &str) -> bool {
    key.chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && key.len() <= 64
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}
