// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The typed targeted edits a revision states, and their meaning on the
//! document's literal projection. The projection of the edited bytes must
//! equal exactly the projection these edits predict.

use serde_json::{Map, Value, json};

use super::{Path, Refusal};

/// One targeted edit of a document, addressed by [`Path`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Edit {
    /// Replace the existing value at `path` (scalar or collection).
    Set {
        /// The node whose value changes.
        path: Path,
        /// Its new literal value.
        value: Value,
    },
    /// Add a new `key` to the mapping at `path`, after its last entry.
    Insert {
        /// The mapping that receives the entry.
        path: Path,
        /// The new key; absent from the mapping.
        key: String,
        /// The new entry's literal value.
        value: Value,
    },
    /// Add a new `key` whose value is the exact YAML `text` (block or flow,
    /// comments included), re-indented under the key and otherwise kept byte
    /// for byte.
    InsertText {
        /// The mapping that receives the entry.
        path: Path,
        /// The new key; absent from the mapping.
        key: String,
        /// The value's YAML source.
        text: String,
    },
    /// Append `value` to the sequence at `path`.
    Push {
        /// The sequence that receives the item.
        path: Path,
        /// The new item's literal value.
        value: Value,
    },
    /// Remove the mapping entry or sequence item at `path`.
    Remove {
        /// The entry or item that goes.
        path: Path,
    },
    /// Rename the mapping entry at `path` to `to`, and every reference its
    /// syntactic owners hold (a task, an input, a constant, a secret, a
    /// task's binding or extract): text that only spells the name is kept.
    Rename {
        /// The entry whose key changes.
        path: Path,
        /// Its new key.
        to: String,
    },
}

/// The fields each operation's JSON form carries, exactly.
const FIELDS: &[(&str, &[&str])] = &[
    ("set", &["op", "path", "value"]),
    ("insert", &["op", "path", "key", "value"]),
    ("insert_text", &["op", "path", "key", "text"]),
    ("push", &["op", "path", "value"]),
    ("remove", &["op", "path"]),
    ("rename", &["op", "path", "to"]),
];

impl Edit {
    /// Replace the value at `path`.
    #[must_use]
    pub fn set(path: Path, value: Value) -> Self {
        Self::Set { path, value }
    }

    /// Add `key: value` to the mapping at `path`.
    #[must_use]
    pub fn insert(path: Path, key: impl Into<String>, value: Value) -> Self {
        Self::Insert {
            path,
            key: key.into(),
            value,
        }
    }

    /// Add `key` with the exact YAML `text` as its value to the mapping at `path`.
    #[must_use]
    pub fn insert_text(path: Path, key: impl Into<String>, text: impl Into<String>) -> Self {
        Self::InsertText {
            path,
            key: key.into(),
            text: text.into(),
        }
    }

    /// Append `value` to the sequence at `path`.
    #[must_use]
    pub fn push(path: Path, value: Value) -> Self {
        Self::Push { path, value }
    }

    /// Remove the entry or item at `path`.
    #[must_use]
    pub fn remove(path: Path) -> Self {
        Self::Remove { path }
    }

    /// Rename the entry at `path` to `to`, with its references.
    #[must_use]
    pub fn rename(path: Path, to: impl Into<String>) -> Self {
        Self::Rename {
            path,
            to: to.into(),
        }
    }

    /// The node the edit addresses: the replaced or removed node, or the
    /// collection that receives a new entry or item.
    #[must_use]
    pub fn path(&self) -> &Path {
        match self {
            Self::Set { path, .. }
            | Self::Insert { path, .. }
            | Self::InsertText { path, .. }
            | Self::Push { path, .. }
            | Self::Remove { path }
            | Self::Rename { path, .. } => path,
        }
    }

    /// The operation's machine word (`set` · `insert` · `insert_text` · `push` ·
    /// `remove` · `rename`).
    #[must_use]
    pub fn op(&self) -> &'static str {
        match self {
            Self::Set { .. } => "set",
            Self::Insert { .. } => "insert",
            Self::InsertText { .. } => "insert_text",
            Self::Push { .. } => "push",
            Self::Remove { .. } => "remove",
            Self::Rename { .. } => "rename",
        }
    }

    /// An edit read exactly from its JSON form: `{"op", "path", ...}` with the
    /// path an RFC 6901 pointer. An unknown op, a missing or extra field or a
    /// field of the wrong type is refused by name; nothing is guessed.
    ///
    /// # Errors
    /// Why the record is not one edit, naming the field.
    pub fn from_json(record: &Value) -> Result<Self, String> {
        let map = record
            .as_object()
            .ok_or_else(|| "an edit must be an object".to_owned())?;
        let op = map
            .get("op")
            .and_then(Value::as_str)
            .ok_or_else(|| "an edit needs an `op` string".to_owned())?;
        let (_, fields) = FIELDS.iter().find(|(name, _)| *name == op).ok_or_else(|| {
            "`op` must be one of set, insert, insert_text, push, remove, rename".to_owned()
        })?;
        if let Some(extra) = map.keys().find(|k| !fields.contains(&k.as_str())) {
            return Err(format!("a `{op}` edit carries no `{extra}` field"));
        }
        if let Some(missing) = fields.iter().find(|f| !map.contains_key(**f)) {
            return Err(format!("a `{op}` edit needs its `{missing}` field"));
        }
        let path = map
            .get("path")
            .and_then(Value::as_str)
            .and_then(Path::pointer)
            .ok_or_else(|| "`path` must be an RFC 6901 pointer string".to_owned())?;
        let text = |field: &str| {
            map.get(field)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| format!("`{field}` must be a string"))
        };
        let value = || map.get("value").cloned().unwrap_or(Value::Null);
        Ok(match op {
            "set" => Self::set(path, value()),
            "insert" => Self::insert(path, text("key")?, value()),
            "insert_text" => Self::insert_text(path, text("key")?, text("text")?),
            "push" => Self::push(path, value()),
            "rename" => Self::rename(path, text("to")?),
            _ => Self::remove(path),
        })
    }

    /// The JSON form [`Edit::from_json`] reads back.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let path = self.path().to_pointer();
        match self {
            Self::Set { value, .. } => json!({"op": "set", "path": path, "value": value}),
            Self::Insert { key, value, .. } => {
                json!({"op": "insert", "path": path, "key": key, "value": value})
            }
            Self::InsertText { key, text, .. } => {
                json!({"op": "insert_text", "path": path, "key": key, "text": text})
            }
            Self::Push { value, .. } => json!({"op": "push", "path": path, "value": value}),
            Self::Remove { .. } => json!({"op": "remove", "path": path}),
            Self::Rename { to, .. } => json!({"op": "rename", "path": path, "to": to}),
        }
    }
}

/// What a JSON node is, for a refusal that names it.
pub(super) fn shape_word(value: &Value) -> &'static str {
    match value {
        Value::Null => "an empty (null) value",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "a sequence",
        Value::Object(_) => "a mapping",
    }
}

/// The literal projection after `edit`, predicted from the projection before
/// it: the node replaced, the entry or item added, or the entry removed.
/// `inserted` is the value an [`Edit::InsertText`] text reads as.
///
/// # Errors
/// The path addresses nothing, or a node that cannot take this edit.
pub(super) fn expected(
    base: &Value,
    edit: &Edit,
    inserted: Option<&Value>,
) -> Result<Value, Refusal> {
    let path = edit.path();
    let mut after = base.clone();
    let unknown = || Refusal::UnknownPath { path: path.clone() };
    let shape = |detail: String| Refusal::Shape {
        path: path.clone(),
        detail,
    };
    match edit {
        Edit::Set { value, .. } => {
            if path.is_root() {
                return Err(shape(
                    "the whole document is replaced by a whole-source revision, not in place"
                        .to_owned(),
                ));
            }
            *after.pointer_mut(&path.to_pointer()).ok_or_else(unknown)? = value.clone();
        }
        Edit::Insert { key, value, .. } => {
            entries(&mut after, path, key, &unknown, &shape)?.insert(key.clone(), value.clone());
        }
        Edit::InsertText { key, .. } => {
            let value = inserted.cloned().unwrap_or(Value::Null);
            entries(&mut after, path, key, &unknown, &shape)?.insert(key.clone(), value);
        }
        Edit::Push { value, .. } => {
            let node = after.pointer_mut(&path.to_pointer()).ok_or_else(unknown)?;
            let word = shape_word(node);
            node.as_array_mut()
                .ok_or_else(|| shape(format!("it is {word}, not a sequence")))?
                .push(value.clone());
        }
        Edit::Rename { to, .. } => {
            // The key alone; the references a rename rewrites are edits of their own.
            let (Some(parent), Some(last)) = (path.parent(), path.last()) else {
                return Err(shape("the document root has no name".to_owned()));
            };
            let node = after
                .pointer_mut(&parent.to_pointer())
                .ok_or_else(unknown)?;
            let word = shape_word(node);
            let map = node
                .as_object_mut()
                .ok_or_else(|| shape(format!("it sits in {word}, not a mapping")))?;
            if map.contains_key(to.as_str()) {
                return Err(shape(format!("the mapping already has the key `{to}`")));
            }
            let value = map.remove(last).ok_or_else(unknown)?;
            map.insert(to.clone(), value);
        }
        Edit::Remove { .. } => {
            let (Some(parent), Some(last)) = (path.parent(), path.last()) else {
                return Err(shape("the document root cannot be removed".to_owned()));
            };
            let node = after
                .pointer_mut(&parent.to_pointer())
                .ok_or_else(unknown)?;
            let removed = match node {
                Value::Object(map) => map.remove(last).is_some(),
                Value::Array(items) => match last.parse::<usize>() {
                    Ok(index) if index < items.len() => {
                        items.remove(index);
                        true
                    }
                    _ => false,
                },
                _ => false,
            };
            if !removed {
                return Err(unknown());
            }
        }
    }
    Ok(after)
}

/// The mapping at `path` that receives `key`, refusing a non-mapping or a key
/// it already holds.
fn entries<'a>(
    after: &'a mut Value,
    path: &Path,
    key: &str,
    unknown: &dyn Fn() -> Refusal,
    shape: &dyn Fn(String) -> Refusal,
) -> Result<&'a mut Map<String, Value>, Refusal> {
    let node = after.pointer_mut(&path.to_pointer()).ok_or_else(unknown)?;
    let word = shape_word(node);
    let map = node
        .as_object_mut()
        .ok_or_else(|| shape(format!("it is {word}, not a mapping")))?;
    if map.contains_key(key) {
        return Err(shape(format!("the mapping already has the key `{key}`")));
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{Edit, Path, expected};

    #[test]
    fn the_json_form_round_trips_and_refuses_extra_fields() {
        for edit in [
            Edit::set(Path::new(["const", "hours"]), json!(72)),
            Edit::insert(
                Path::new(["tasks"]),
                "late",
                json!({"exec": {"command": ["true"]}}),
            ),
            Edit::insert_text(
                Path::new(["tasks"]),
                "late",
                "exec:\n  command: [\"true\"]\n",
            ),
            Edit::push(Path::new(["permits", "tools"]), json!("nika:log")),
            Edit::remove(Path::new(["outputs", "old"])),
            Edit::rename(Path::new(["tasks", "fetch"]), "grab"),
        ] {
            assert_eq!(Edit::from_json(&edit.to_json()), Ok(edit));
        }
        let extra = json!({"op": "remove", "path": "/a", "value": 1});
        assert!(Edit::from_json(&extra).unwrap_err().contains("`value`"));
        let unknown = json!({"op": "move", "path": "/a"});
        assert!(Edit::from_json(&unknown).unwrap_err().contains("one of"));
        let missing = json!({"op": "set", "path": "/a"});
        assert!(Edit::from_json(&missing).unwrap_err().contains("`value`"));
        let dotted = json!({"op": "remove", "path": "a.b"});
        assert!(Edit::from_json(&dotted).unwrap_err().contains("pointer"));
    }

    #[test]
    fn the_prediction_changes_only_the_addressed_node() {
        let base = json!({"const": {"hours": 48, "name": "x"}, "items": [1, 2]});
        let set = Edit::set(Path::new(["const", "hours"]), json!(72));
        assert_eq!(
            expected(&base, &set, None).ok(),
            Some(json!({"const": {"hours": 72, "name": "x"}, "items": [1, 2]}))
        );
        let push = Edit::push(Path::new(["items"]), json!(3));
        assert_eq!(
            expected(&base, &push, None)
                .ok()
                .map(|v| v["items"].clone()),
            Some(json!([1, 2, 3]))
        );
        let removed = Edit::remove(Path::new(["items", "0"]));
        assert_eq!(
            expected(&base, &removed, None)
                .ok()
                .map(|v| v["items"].clone()),
            Some(json!([2]))
        );
    }

    #[test]
    fn a_wrong_shape_or_an_absent_node_is_named() {
        let base = json!({"const": {"hours": 48}, "items": [1]});
        let kinds = [
            (
                Edit::set(Path::new(["const", "days"]), json!(1)),
                "unknown_path",
            ),
            (Edit::insert(Path::new(["items"]), "k", json!(1)), "shape"),
            (
                Edit::insert(Path::new(["const"]), "hours", json!(1)),
                "shape",
            ),
            (Edit::push(Path::new(["const"]), json!(1)), "shape"),
            (Edit::remove(Path::new(["items", "4"])), "unknown_path"),
            (Edit::set(Path::root(), json!({})), "shape"),
        ];
        for (edit, kind) in kinds {
            let refusal = expected(&base, &edit, None).expect_err("refused");
            assert_eq!(refusal.kind(), kind, "{edit:?}");
        }
    }
}
