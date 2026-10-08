// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The document revision (R5): a change in words to a base no semantic record binds is applied
//! to the complete document the base is, never rebuilt from a smaller projection of it. Beside
//! the destination links of the source-anchored revision, the seat may state operations over
//! the base's own nodes, in order:
//!
//! - `set` a literal at a path (`/const/window_hours` or `const.window_hours`), the bounded edit
//!   the parser's literal projection proves, every other byte kept;
//! - `compose` an admitted component of the catalogue the host lent, by its reference and the
//!   literals bound at its holes: resolved by id and version or release digest, bound, expanded
//!   into the document and checked as a whole, its receipt kept;
//! - `rebind` a component an earlier revision composed, by its receipt: the bound literals
//!   revised and the receipt carried to the new bytes.
//!
//! When the change cannot be stated so, the seat may give the whole revised source instead
//! (`replace`): it is read by the strict parser like any candidate, and the record says the
//! bytes were replaced, claiming no preservation. Every result goes through the same finish
//! (strict parse and Check) and the round's judge; a refused operation names why, and nothing
//! of a refused revision is applied. Reuse grants nothing: a component's permits, model and
//! name are never inherited, and what the document needs is Check's to say.
//!
//! This module applies the operations and states their record; the revision door of
//! `nika-compile-cognition` asks the seat for them, finishes the result and has it judged.

use super::{Binding, ComponentCatalog, ComponentRef, edit_literal, expand, instantiate, revise};
use serde_json::{Value, json};

/// The route a document revision records.
pub const ROUTE: &str = "edit: document revision over the complete base";

/// What the seat is told it may state beside destination links: operations over the complete
/// base document, or its whole revised source.
pub const OPERATIONS: &str = "Any change the destination links cannot state is stated as `operations` over the complete base document (`base_document` lists every literal node with its path, `base_source` is the exact text), applied in order: {\"op\": \"set\", \"path\": \"/const/name\", \"value\": <JSON literal>} replaces the literal at that path (a typed constant's value, an input's default, a task argument, a retry count, a threshold); {\"op\": \"compose\", \"component\": {\"id\": \"block:<name>\", \"version\": \"<release version>\"}, \"bindings\": {\"<hole path>\": <JSON literal>}} adds an admitted component of `components` with every hole bound; {\"op\": \"rebind\", \"component\": {\"id\": \"block:<name>\"}, \"bindings\": {\"<hole path>\": <JSON literal>}} changes the values bound in a component an earlier revision composed (`composed`). Every other byte of the base is kept. Only when no operation can state the change, give the whole revised source in `replace` instead: it is checked like any workflow and nothing of the base is promised to survive. State only what the request and the change ask; never invent a value they leave open.";

/// What the operations made of the base: the revised source, what changed, the receipts of the
/// components the revised source holds (new, rebound or carried), and whether the whole source
/// was replaced.
#[derive(Debug)]
#[non_exhaustive]
pub struct Applied {
    /// The revised source.
    pub source: String,
    /// The node paths and components changed, in the operations' order.
    pub changed: Vec<String>,
    /// The receipts of the components the revised source holds.
    pub receipts: Vec<Value>,
    /// The whole source was replaced: no preservation is claimed.
    pub replaced: bool,
}

/// The JSON schema of what a seat states over the document: the `operations` (every field a
/// text, a value as its JSON text, so a strict structured-output dialect can carry it) and the
/// whole `replace`ment.
#[must_use]
pub fn answer_schema() -> (Value, Value) {
    let text = || json!({"type": "string"});
    let operations = json!({"type": "array", "items": {
    "type": "object", "additionalProperties": false,
    "required": ["op"],
    "properties": {
        "op": {"type": "string", "enum": ["set", "compose", "rebind"]},
        "path": text(), "value_json": text(),
        "component": text(), "version": text(), "bindings_json": text(),
    }}});
    (operations, text())
}

/// Apply `operations` to `base` in order, or take `replace` as the whole revised source.
/// `carried` are the receipts the base's record holds (a component an earlier revision
/// composed); they are kept beside the new ones, and a `rebind` revises one of them.
///
/// # Errors
/// Every operation refused, each named with its index; nothing of a refused revision applies.
pub fn apply(
    base: &str,
    (operations, replace): (&[Value], Option<&str>),
    catalog: Option<&dyn ComponentCatalog>,
    carried: &[Value],
) -> Result<Applied, Vec<String>> {
    if let Some(source) = replace {
        if !operations.is_empty() {
            return Err(vec![
                "state either operations or a whole `replace`, never both".to_owned(),
            ]);
        }
        if let Err(error) = nika_compile::parse(source) {
            return Err(vec![format!(
                "the replacement is not a workflow the strict parser reads: {error}"
            )]);
        }
        return Ok(Applied {
            source: source.to_owned(),
            changed: vec!["the whole source (replaced)".to_owned()],
            receipts: carried.to_vec(),
            replaced: true,
        });
    }
    if operations.is_empty() {
        return Err(vec!["no operation was stated".to_owned()]);
    }
    let mut applied = Applied {
        source: base.to_owned(),
        changed: Vec::new(),
        receipts: carried.to_vec(),
        replaced: false,
    };
    let mut why = Vec::new();
    for (at, operation) in operations.iter().enumerate() {
        if let Err(reason) = one(&mut applied, operation, catalog) {
            why.push(format!("operation {at}: {reason}"));
        }
    }
    if why.is_empty() {
        Ok(applied)
    } else {
        Err(why)
    }
}

/// One operation on the source as it stands; the source is unchanged by a refusal.
fn one(
    applied: &mut Applied,
    operation: &Value,
    catalog: Option<&dyn ComponentCatalog>,
) -> Result<(), String> {
    let operation = canonical(operation)?;
    match operation["op"].as_str() {
        Some("set") => set(applied, &operation),
        Some("compose") => compose(applied, &operation, catalog),
        _ => rebind(applied, &operation, catalog),
    }
}

/// The fields an operation may carry: its object form (`value`, `component` as an object,
/// `bindings` as an object) and the text form a strict structured-output dialect carries (every
/// field a text, a value as its JSON text in `value_json` or `bindings_json`, an empty text
/// meaning absent).
const FIELDS: &[&str] = &[
    "op",
    "path",
    "value",
    "value_json",
    "component",
    "version",
    "bindings",
    "bindings_json",
];

/// An operation in its one canonical form, `{op, path, value}` or `{op, component: {id,
/// version?}, bindings}`, from either form; an unknown field or a value that is not JSON is
/// refused, never guessed.
fn canonical(operation: &Value) -> Result<Value, String> {
    let object = operation.as_object().ok_or("an operation is an object")?;
    if let Some(key) = object.keys().find(|key| !FIELDS.contains(&key.as_str())) {
        return Err(format!("`{key}` is not a field of an operation"));
    }
    let present = |key: &str| {
        object
            .get(key)
            .filter(|value| !value.is_null() && value.as_str() != Some(""))
    };
    let either = |plain: &str, text: &str| -> Result<Option<Value>, String> {
        match (present(plain), present(text)) {
            (Some(_), Some(_)) => Err(format!("state `{plain}` or `{text}`, never both")),
            (Some(value), None) => Ok(Some(value.clone())),
            (None, Some(Value::String(json))) => serde_json::from_str(json)
                .map(Some)
                .map_err(|error| format!("`{text}` is not one JSON value: {error}")),
            (None, Some(_)) => Err(format!("`{text}` is a JSON text")),
            (None, None) => Ok(None),
        }
    };
    let op = present("op").and_then(Value::as_str).unwrap_or_default();
    match op {
        "set" => {
            let value = either("value", "value_json")?.ok_or("`set` states no value")?;
            Ok(json!({"op": "set", "path": present("path"), "value": value}))
        }
        "compose" | "rebind" => {
            let component = match present("component") {
                Some(Value::String(id)) => {
                    let mut reference = json!({"id": id});
                    if let Some(version) = present("version") {
                        reference["version"] = version.clone();
                    }
                    reference
                }
                Some(reference @ Value::Object(_)) => reference.clone(),
                _ => return Err(format!("`{op}` names no `component`")),
            };
            let bindings = either("bindings", "bindings_json")?.unwrap_or_else(|| json!({}));
            Ok(json!({"op": op, "component": component, "bindings": bindings}))
        }
        _ => Err(format!(
            "`{op}` is not an operation (set · compose · rebind)"
        )),
    }
}

/// The literal at a path replaced by the bounded edit the parser proves.
fn set(applied: &mut Applied, operation: &Value) -> Result<(), String> {
    let path = dotted(operation["path"].as_str().unwrap_or_default())?;
    let value = operation
        .get("value")
        .ok_or_else(|| format!("`set {path}` states no `value`"))?;
    applied.source = edit_literal(&applied.source, &path, value)
        .map_err(|refusal| format!("`set {path}`: {refusal}"))?;
    applied.changed.push(path);
    Ok(())
}

/// An admitted component resolved, bound at its holes and expanded into the document.
fn compose(
    applied: &mut Applied,
    operation: &Value,
    catalog: Option<&dyn ComponentCatalog>,
) -> Result<(), String> {
    let catalog = catalog.ok_or("no component catalogue was lent to this revision")?;
    let reference =
        ComponentRef::from_value(&operation["component"]).map_err(|error| error.to_string())?;
    let component = catalog
        .resolve(&reference)
        .map_err(|error| error.to_string())?;
    let bindings = bindings(&operation["bindings"])?;
    let instance = instantiate(&component, &bindings).map_err(|error| error.to_string())?;
    let expansion = expand(&applied.source, &instance).map_err(|error| error.to_string())?;
    applied.source = expansion.candidate;
    applied.changed.push(format!("component {}", reference.id));
    applied.receipts.push(expansion.receipt);
    Ok(())
}

/// A composed component's bound literals revised through its receipt, which follows the bytes.
fn rebind(
    applied: &mut Applied,
    operation: &Value,
    catalog: Option<&dyn ComponentCatalog>,
) -> Result<(), String> {
    let catalog = catalog.ok_or("no component catalogue was lent to this revision")?;
    let reference =
        ComponentRef::from_value(&operation["component"]).map_err(|error| error.to_string())?;
    let at = (applied.receipts.iter())
        .position(|receipt| receipt["component"]["id"] == reference.id.as_str())
        .ok_or_else(|| format!("no composed `{}` holds a receipt here", reference.id))?;
    // The receipt's own pins resolve it: a revision never takes a later release silently.
    let pinned = ComponentRef::new(reference.id.clone())
        .at_version(
            applied.receipts[at]["component"]["release"]["version"]
                .as_str()
                .unwrap_or_default(),
        )
        .in_release(
            applied.receipts[at]["component"]["release"]["snapshot_sha256"]
                .as_str()
                .unwrap_or_default(),
        );
    let component = catalog
        .resolve(&pinned)
        .map_err(|error| error.to_string())?;
    let changes = bindings(&operation["bindings"])?;
    let (source, carried) = revise(&applied.source, &applied.receipts[at], &component, &changes)
        .map_err(|error| error.to_string())?;
    applied.source = source;
    applied.receipts[at] = carried;
    applied
        .changed
        .extend(changes.iter().map(|change| change.path.clone()));
    Ok(())
}

/// Bindings as `{"<path>": <literal>}` or `[{"path", "value"}]`; anything else is refused.
fn bindings(value: &Value) -> Result<Vec<Binding>, String> {
    match value {
        Value::Object(map) => Ok(map
            .iter()
            .map(|(path, literal)| Binding::new(path.clone(), literal.clone()))
            .collect()),
        Value::Array(rows) => (rows.iter())
            .map(|row| match (row["path"].as_str(), row.get("value")) {
                (Some(path), Some(literal)) if row.as_object().is_some_and(|o| o.len() == 2) => {
                    Ok(Binding::new(path, literal.clone()))
                }
                _ => Err("a binding is {\"path\", \"value\"}".to_owned()),
            })
            .collect(),
        Value::Null => Ok(Vec::new()),
        _ => Err("`bindings` is an object of hole paths to literals".to_owned()),
    }
}

/// A node path in the dotted form the bounded edit reads: an RFC 6901 pointer (`/a/b/0`) or a
/// dotted path (`a.b.0`). A key holding a dot has no dotted spelling: refused, never guessed.
fn dotted(path: &str) -> Result<String, String> {
    let path = path.trim();
    if path.is_empty() || path == "/" {
        return Err("an operation names no path".to_owned());
    }
    let Some(pointer) = path.strip_prefix('/') else {
        return Ok(path.to_owned());
    };
    let mut segments = Vec::new();
    for raw in pointer.split('/') {
        let segment = raw.replace("~1", "/").replace("~0", "~");
        if segment.is_empty() || segment.contains('.') {
            return Err(format!(
                "the path `{path}` names a key this edit cannot address (empty or holding a dot)"
            ));
        }
        segments.push(segment);
    }
    Ok(segments.join("."))
}

/// Every literal node of the document as the seat reads it: `[{"path": "/…", "value": …}]`, the
/// leaves of the parser's literal projection in document order. `None` when it cannot be read.
#[must_use]
pub fn nodes(source: &str) -> Option<Value> {
    let projection = nika_compile::surface::literal_projection(source)?;
    let mut rows = Vec::new();
    leaves(&projection, &mut String::new(), &mut rows);
    Some(Value::Array(rows))
}

fn leaves(value: &Value, at: &mut String, rows: &mut Vec<Value>) {
    let mut down = |key: &str, child: &Value, at: &mut String| {
        let length = at.len();
        at.push('/');
        at.push_str(&key.replace('~', "~0").replace('/', "~1"));
        leaves(child, at, rows);
        at.truncate(length);
    };
    match value {
        Value::Object(map) if !map.is_empty() => {
            for (key, child) in map {
                down(key, child, at);
            }
        }
        Value::Array(items) if !items.is_empty() => {
            for (index, child) in items.iter().enumerate() {
                down(&index.to_string(), child, at);
            }
        }
        leaf => rows.push(json!({"path": at.clone(), "value": leaf})),
    }
}

/// The components a revision may compose: each admitted executable entry of the catalogue by its
/// reference, purpose and holes (as its row states them). Empty without a catalogue.
#[must_use]
pub fn components(catalog: Option<&dyn ComponentCatalog>) -> Value {
    let Some(catalog) = catalog else {
        return json!([]);
    };
    let release = catalog.release();
    let rows: Vec<Value> = (catalog.entries().iter())
        .filter(|row| {
            row["id"]
                .as_str()
                .is_some_and(|id| id.starts_with("block:"))
        })
        .map(|row| {
            json!({
                "component": {"id": row["id"], "version": release.version},
                "title": row["title"],
                "purpose": row["purpose"],
                "holes": row["holes"],
                "effects": row["effects"],
            })
        })
        .collect();
    Value::Array(rows)
}

/// The receipts a base's record carries for the components an earlier revision composed.
#[must_use]
pub fn carried(record: Option<&Value>) -> Vec<Value> {
    (record.and_then(|r| r["document_revision"]["components"].as_array()))
        .cloned()
        .unwrap_or_default()
}

/// The record a document revision leaves: it binds the revised bytes (as a source revision's
/// record does, so a saved file keeps it) under the words they now answer, and states the
/// operations' mode, what changed and the receipts of the components the bytes hold.
#[must_use]
pub fn record(
    (base, revised): (&str, &str),
    resolved: &str,
    intent_sha256: &str,
    applied: &Applied,
) -> Value {
    let sha = nika_compile::surface::sha256;
    json!({
        "source_revision": {
            "candidate_sha256": sha(revised),
            "resolved": resolved,
            "base_sha256": sha(base),
        },
        "intent_sha256": intent_sha256,
        "document_revision": {
            "route": ROUTE,
            "mode": if applied.replaced { "replaced" } else { "operations" },
            "base_sha256": sha(base),
            "candidate_sha256": sha(revised),
            "changed": applied.changed,
            "preservation": if applied.replaced {
                "none claimed: the whole source was replaced"
            } else {
                "by construction: each edited literal replaced in its own span and each component's entries inserted; not yet re-verified byte by byte"
            },
            "components": applied.receipts,
        },
    })
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
