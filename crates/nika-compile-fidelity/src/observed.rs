// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The nested structure of observed records (pure, bounded): below each record's keys, the key
//! paths of its arrays and objects (`stock[]`, `stock[].on_hand`, `meta.owner`), each with the raw
//! kinds of the values found there, counted — never a value. Every collection states how many
//! elements it holds and how many were read, and the structure states whether it is complete:
//! a bound reached (depth, paths, elements) makes it incomplete, never silently whole. A sample is
//! never a schema: a complete structure covers the values read, nothing more.
//!
//! A host keeps the structure beside a file's raw kinds (`{"sampled", "keys", "nested"}`, keyed by
//! the file's path beside the rows, never inside one). [`uncovered`] reads that entry back: the
//! keys whose values are arrays or objects that no complete nested structure covers. A name under
//! one of them is not stated by the observation.

/// Identity of a request's observed files and the shared numeric kind law.
pub mod basis;
mod temporal;
pub use temporal::temporal_shapes;

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};

/// The deepest nesting read below a record's keys.
pub const MAX_DEPTH: usize = 4;
/// The most distinct nested paths kept across values and collections, including empty arrays.
pub const MAX_PATHS: usize = 64;
/// The most elements read from one collection.
pub const MAX_ELEMENTS: usize = 200;

/// What a walk of the records found, and whether a bound cut it.
struct Walk<'k> {
    kind: &'k dyn Fn(&Value) -> &'static str,
    paths: Map<String, Value>,
    temporal: BTreeMap<String, temporal::Temporal>,
    kept: BTreeSet<String>,
    collections: Map<String, Value>,
    complete: bool,
}

impl Walk<'_> {
    /// Admit one distinct path before counting or descending, even for an empty collection.
    fn admit(&mut self, path: &str) -> bool {
        if self.kept.contains(path) {
            return true;
        }
        if self.kept.len() >= MAX_PATHS {
            self.complete = false;
            return false;
        }
        self.kept.insert(path.to_owned());
        true
    }

    /// Count one value of kind `kind` at an admitted `path`.
    fn count(&mut self, path: &str, kind: &str) {
        let counts = self
            .paths
            .entry(path.to_owned())
            .or_insert_with(|| json!({}));
        let n = counts[kind].as_u64().unwrap_or(0) + 1;
        counts[kind] = json!(n);
    }

    /// Walk the inside of `value`, found at `path` at `depth` below a record's key.
    fn inside(&mut self, value: &Value, path: &str, depth: usize) {
        match value {
            Value::Array(items) => {
                let element = format!("{path}[]");
                if !self.admit(&element) {
                    return;
                }
                let read = items.len().min(MAX_ELEMENTS);
                let entry = self
                    .collections
                    .entry(element.clone())
                    .or_insert_with(|| json!({"elements": 0, "sampled": 0}));
                entry["elements"] =
                    json!(entry["elements"].as_u64().unwrap_or(0) + items.len() as u64);
                entry["sampled"] = json!(entry["sampled"].as_u64().unwrap_or(0) + read as u64);
                if read < items.len() {
                    self.complete = false;
                }
                for item in &items[..read] {
                    self.value(item, &element, depth + 1);
                }
            }
            Value::Object(map) => {
                for (key, item) in map {
                    self.value(item, &format!("{path}.{key}"), depth + 1);
                }
            }
            _ => {}
        }
    }

    /// Count `value` at `path`, then walk inside it while the depth allows.
    fn value(&mut self, value: &Value, path: &str, depth: usize) {
        if !self.admit(path) {
            return;
        }
        self.count(path, (self.kind)(value));
        self.temporal
            .entry(path.to_owned())
            .or_default()
            .observe(value.as_str());
        if matches!(value, Value::Array(_) | Value::Object(_)) {
            if depth >= MAX_DEPTH {
                self.complete = false;
            } else {
                self.inside(value, path, depth);
            }
        }
    }
}

/// The nested structure below the keys of the first `MAX_ELEMENTS` `rows`, each value's raw kind
/// given by `kind`: `{"paths": {path: {kind: count}}, "collections": {path: {"elements",
/// "sampled"}}, "complete": bool}`, plus masked `temporal` formats when recognized; `Value::Null` when no record holds an array or an object.
#[must_use]
pub fn nested(rows: &[Value], kind: &dyn Fn(&Value) -> &'static str) -> Value {
    let mut walk = Walk {
        kind,
        paths: Map::new(),
        temporal: BTreeMap::new(),
        kept: BTreeSet::new(),
        collections: Map::new(),
        complete: rows.len() <= MAX_ELEMENTS,
    };
    for row in rows.iter().take(MAX_ELEMENTS) {
        for (key, value) in row.as_object().into_iter().flatten() {
            if matches!(value, Value::Array(_) | Value::Object(_)) {
                walk.inside(value, key, 1);
            }
        }
    }
    if walk.paths.is_empty() && walk.collections.is_empty() {
        return Value::Null;
    }
    let temporal: Map<String, Value> = walk
        .temporal
        .into_iter()
        .filter_map(|(path, counts)| {
            let shapes = counts.finish();
            (!shapes.is_null()).then_some((path, shapes))
        })
        .collect();
    let mut out =
        json!({"paths": walk.paths, "collections": walk.collections, "complete": walk.complete});
    if !temporal.is_empty() {
        out["temporal"] = Value::Object(temporal);
    }
    out
}

/// The keys of one observed file's kinds entry (`{"keys": {key: {kind: count}}, "nested"?}`)
/// whose values are arrays or objects that no complete nested structure covers.
#[must_use]
pub fn uncovered(kinds: &Value) -> Vec<String> {
    let complete = kinds["nested"]["complete"].as_bool() == Some(true);
    let paths = kinds["nested"]["paths"].as_object();
    let structured = |counts: &Value| {
        counts
            .as_object()
            .is_some_and(|c| c.contains_key("array") || c.contains_key("object"))
    };
    let covered = |key: &str| {
        complete
            && paths.is_some_and(|paths| {
                paths.keys().any(|p| {
                    p.starts_with(&format!("{key}[]")) || p.starts_with(&format!("{key}."))
                })
            })
    };
    (kinds["keys"].as_object().into_iter().flatten())
        .filter(|(key, counts)| structured(counts) && !covered(key))
        .map(|(key, _)| key.clone())
        .collect()
}

/// The nested names of one observed file's kinds entry, one collection or object per part:
/// `movements[]: kind, part, qty, seq` — or None when it states no nested structure.
#[must_use]
pub fn names(kinds: &Value) -> Option<String> {
    let paths = kinds["nested"]["paths"].as_object()?;
    let mut parts: Vec<(String, Vec<String>)> = Vec::new();
    for path in paths.keys() {
        let (parent, name) = match path.rfind('.') {
            Some(at) if !path.ends_with("[]") => (&path[..at], &path[at + 1..]),
            _ => continue,
        };
        match parts.iter_mut().find(|(p, _)| p == parent) {
            Some((_, names)) => names.push(name.to_owned()),
            None => parts.push((parent.to_owned(), vec![name.to_owned()])),
        }
    }
    (!parts.is_empty()).then(|| {
        parts
            .iter()
            .map(|(parent, names)| format!("{parent}: {}", names.join(", ")))
            .collect::<Vec<_>>()
            .join("; ")
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn kind(value: &Value) -> &'static str {
        match value {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "text",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        }
    }

    /// A supply file: two collections of objects, one heterogeneous, one nested object.
    fn supplies() -> Value {
        json!({"stock": [
                {"part": "A", "on_hand": 3},
                {"part": "B", "on_hand": 0, "reorder_level": 2}],
            "movements": [{"kind": "issue", "part": "A", "qty": 1, "seq": 1}],
            "meta": {"owner": "ops"}})
    }

    #[test]
    fn nested_keys_and_kinds_are_kept_never_values() {
        let out = nested(&[supplies()], &kind);
        assert_eq!(out["complete"], true);
        assert_eq!(out["paths"]["stock[]"], json!({"object": 2}));
        assert_eq!(out["paths"]["stock[].on_hand"], json!({"number": 2}));
        assert_eq!(out["paths"]["stock[].reorder_level"], json!({"number": 1}));
        assert_eq!(out["paths"]["movements[].qty"], json!({"number": 1}));
        assert_eq!(out["paths"]["meta.owner"], json!({"text": 1}));
        assert_eq!(
            out["collections"]["stock[]"],
            json!({"elements": 2, "sampled": 2})
        );
        let text = out.to_string();
        assert!(!text.contains("issue") && !text.contains("ops"), "{text}");
        assert_eq!(nested(&[json!({"id": 1})], &kind), Value::Null);
    }

    #[test]
    fn a_reached_bound_is_incomplete_never_silently_whole() {
        let long: Vec<Value> = (0..=MAX_ELEMENTS).map(|n| json!({"n": n})).collect();
        let out = nested(&[json!({"items": long})], &kind);
        assert_eq!(out["complete"], false);
        assert_eq!(out["collections"]["items[]"]["elements"], MAX_ELEMENTS + 1);
        assert_eq!(out["collections"]["items[]"]["sampled"], MAX_ELEMENTS);
        let mut deep = json!("leaf");
        for _ in 0..MAX_DEPTH + 2 {
            deep = json!({"d": deep});
        }
        assert_eq!(nested(&[json!({"top": deep})], &kind)["complete"], false);
        let wide: Map<String, Value> = (0..=MAX_PATHS)
            .map(|n| (format!("k{n}"), json!(n)))
            .collect();
        assert_eq!(nested(&[json!({"o": wide})], &kind)["complete"], false);
    }

    #[test]
    fn wide_objects_bound_collections_and_stop_descending_into_omitted_paths() {
        let wide: Map<String, Value> = (0..MAX_PATHS * 4)
            .map(|n| (format!("k{n:03}"), json!({"items": vec![0; 16]})))
            .collect();
        let read = std::cell::Cell::new(0usize);
        let out = nested(&[json!({"object": wide})], &|value| {
            read.set(read.get() + 1);
            kind(value)
        });
        let paths = out["paths"].as_object().unwrap();
        let collections = out["collections"].as_object().unwrap();
        let retained: BTreeSet<_> = paths.keys().chain(collections.keys()).collect();
        assert_eq!(out["complete"], false);
        assert_eq!(retained.len(), MAX_PATHS);
        assert!(collections.len() < MAX_PATHS);
        assert!(read.get() <= MAX_PATHS * 16, "read {} values", read.get());
        assert!(!paths.contains_key("object.k255.items[]"));
    }

    #[test]
    fn root_arrays_use_the_path_bound_even_when_empty() {
        for empty in [true, false] {
            for count in [MAX_PATHS, MAX_PATHS + 1] {
                let row: Map<String, Value> = (0..count)
                    .map(|n| {
                        let values = if empty { json!([]) } else { json!([1]) };
                        (format!("k{n:03}"), values)
                    })
                    .collect();
                let out = nested(&[Value::Object(row.clone()), Value::Object(row)], &kind);
                let paths = out["paths"].as_object().unwrap();
                let collections = out["collections"].as_object().unwrap();
                assert_eq!(out["complete"], count <= MAX_PATHS);
                assert_eq!(collections.len(), MAX_PATHS);
                assert_eq!(paths.len(), if empty { 0 } else { MAX_PATHS });
                let elements = if empty { 0 } else { 2 };
                assert_eq!(
                    collections["k000[]"],
                    json!({"elements": elements, "sampled": elements})
                );
                if !empty {
                    assert_eq!(paths["k000[]"], json!({"number": 2}));
                }
            }
        }
    }

    #[test]
    fn a_structured_key_is_covered_only_by_a_complete_nested_structure() {
        let flat = json!({"keys": {"stock": {"array": 1}, "movements": {"array": 1},
            "meta": {"object": 1}, "id": {"number": 1}}});
        let mut entry = flat.clone();
        entry["nested"] = nested(&[supplies()], &kind);
        assert!(uncovered(&entry).is_empty());
        let mut partial = entry.clone();
        partial["nested"]["complete"] = json!(false);
        assert_eq!(uncovered(&partial), ["meta", "movements", "stock"]);
        assert_eq!(uncovered(&flat), ["meta", "movements", "stock"]);
        assert!(uncovered(&json!({"keys": {"id": {"number": 1}}})).is_empty());
    }

    #[test]
    fn the_nested_names_read_one_part_per_collection() {
        let row = json!({"nested": nested(&[supplies()], &kind)});
        assert_eq!(
            names(&row).unwrap(),
            "meta: owner; movements[]: kind, part, qty, seq; stock[]: on_hand, part, reorder_level"
        );
        assert_eq!(names(&json!({})), None);
    }
}
