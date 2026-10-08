// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A component kept behind a child-workflow boundary (spec 14) instead of flattened into the
//! document: the bound component becomes a child program the parent calls through the existing
//! `invoke: { workflow: … }`, so its task names never meet the parent's.
//!
//! The child is the bound bytes under the child file's own name. The component's `model:` and
//! `permits:` do not survive: the child carries the person's own boundary, copied from the
//! parent, and the effective authority of a child is the parent's and the child's together, so a
//! component can never widen it. The parent gains one task. Both documents are proven by the
//! parser's literal projection; the child is checked alone, and the parent's source-only Check
//! leaves the call unresolved until a host that reads the child file judges the composition
//! (`nika check` does).

use nika_compile::surface::{literal_projection, sha256};
use serde_json::{Map, Value, json};

use super::bind::{BindingError, edit_literal};
use super::instance::{
    ExpandError, Instance, MERGED, check, merge_section, nodes, place_section, section,
};

/// The law an invocation receipt states.
pub const INVOKED: &str = "invoked: an admitted component's exact bytes, bound at its holes, kept behind a child-workflow boundary the document calls; the child carries the person's boundary, never the component's permits, model or name";

/// A component kept behind a child-workflow boundary: the child program, the parent with its
/// calling task, and the receipt.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Invocation {
    /// The child program, to be written at the receipt's `invocation.workflow` path.
    pub child: String,
    /// The parent document with the calling task added.
    pub candidate: String,
    /// The receipt: component, bindings, the call, the child's nodes and both Checks.
    pub receipt: Value,
}

/// Keep `instance` behind a child-workflow boundary: the child program for `child_path` (an
/// owned-relative `./….nika` path) and `parent` with a task `task` that invokes it.
///
/// # Errors
/// [`ExpandError`]: open holes, a task name the parent holds, a child path that is not an
/// owned-relative program path, `secrets` in the component, or a document the parser does not
/// read as expected.
pub fn invoke(
    parent: &str,
    instance: &Instance,
    task: &str,
    child_path: &str,
) -> Result<Invocation, ExpandError> {
    if !instance.open.is_empty() {
        return Err(ExpandError::Binding(BindingError::Unbound(
            instance.open.clone(),
        )));
    }
    let name = child_name(child_path).ok_or_else(|| {
        ExpandError::Unproven(format!(
            "`{child_path}` is not an owned-relative `./….nika` program path"
        ))
    })?;
    let base = literal_projection(parent).ok_or(ExpandError::Parent)?;
    let bound = literal_projection(&instance.source)
        .ok_or_else(|| ExpandError::Unproven("the bound component cannot be read".to_owned()))?;
    if bound.get("secrets").is_some() {
        return Err(ExpandError::Unmergeable("secrets".to_owned()));
    }
    if base.get("tasks").and_then(|t| t.get(task)).is_some() {
        return Err(ExpandError::Collision {
            section: "tasks".to_owned(),
            name: task.to_owned(),
        });
    }
    let child = child_program(parent, instance, &base, &bound, name)?;
    let call = vec![
        format!("{task}:"),
        "  invoke:".to_owned(),
        format!("    workflow: {}", json!(child_path)),
    ];
    let candidate = merge_section(parent, "tasks", &call)?;
    let mut expected = base;
    let tasks = expected
        .as_object_mut()
        .ok_or(ExpandError::Parent)?
        .entry("tasks")
        .or_insert_with(|| Value::Object(Map::new()));
    if let Some(tasks) = tasks.as_object_mut() {
        tasks.insert(task.to_owned(), json!({"invoke": {"workflow": child_path}}));
    }
    if literal_projection(&candidate).as_ref() != Some(&expected) {
        return Err(ExpandError::Unproven(
            "the parent does not read as itself plus the calling task".to_owned(),
        ));
    }
    Ok(receipted(
        candidate, child, instance, &bound, task, child_path,
    ))
}

/// The child file's own name: the stem of an owned-relative `./<dirs>/<stem>.nika` path.
fn child_name(path: &str) -> Option<&str> {
    let relative = path.strip_prefix("./")?;
    let owned = !relative.contains("..") && !relative.contains('\\') && !relative.contains("${{");
    let stem = relative.rsplit('/').next()?.strip_suffix(".nika")?;
    let token = !stem.is_empty()
        && stem
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    (owned && token).then_some(stem)
}

/// The child program: the bound bytes under `name`, the component's model and permits removed,
/// the parent's own `permits:` section copied in; proven by the projection.
fn child_program(
    parent: &str,
    instance: &Instance,
    base: &Value,
    bound: &Value,
    name: &str,
) -> Result<String, ExpandError> {
    let unproven = |why: &str| ExpandError::Unproven(format!("the child program {why}"));
    let mut child = edit_literal(&instance.source, "nika", &json!(name))
        .map_err(|_| unproven("cannot take its own name"))?;
    for key in ["model", "permits"] {
        if let Some(found) = section(&child, key) {
            child = format!("{}{}", &child[..found.start], &child[found.body_end..]);
        }
    }
    if let Some(found) = section(parent, "permits") {
        let mut boundary = parent[found.start..found.body_end].to_owned();
        if !boundary.ends_with('\n') {
            boundary.push('\n');
        }
        child = place_section(&child, "permits", &boundary);
    }
    let mut expected = bound.clone();
    if let Some(map) = expected.as_object_mut() {
        map.insert("nika".to_owned(), json!(name));
        map.remove("model");
        match base.get("permits") {
            Some(permits) => map.insert("permits".to_owned(), permits.clone()),
            None => map.remove("permits"),
        };
    }
    if literal_projection(&child).as_ref() != Some(&expected) {
        return Err(unproven(
            "does not read as the bound component under its own boundary",
        ));
    }
    Ok(child)
}

/// The invocation checked (the child alone, the parent source-only) and receipted.
fn receipted(
    candidate: String,
    child: String,
    instance: &Instance,
    bound: &Value,
    task: &str,
    child_path: &str,
) -> Invocation {
    let (_, parent_check, _) = check(&candidate);
    let (_, child_check, needed) = check(&child);
    let parent_doc = literal_projection(&candidate).unwrap_or(Value::Null);
    let child_doc = literal_projection(&child).unwrap_or(Value::Null);
    let call = parent_doc["tasks"][task].to_string();
    let component = &instance.component;
    let mut produced = Map::new();
    for section in MERGED {
        if let Some(entries) = bound.get(section).and_then(Value::as_object) {
            let names = entries.keys().map(|n| (n.clone(), Value::Null)).collect();
            produced.insert(section.to_owned(), Value::Object(names));
        }
    }
    let receipt = json!({
        "law": INVOKED,
        "component": component.record(),
        "bindings": instance.bindings_record(),
        "open": instance.open,
        "not_inherited": {
            "nika": bound.get("nika"),
            "model": bound.get("model"),
            "permits": bound.get("permits"),
        },
        "authority": {
            "inherited": false,
            "child_carries": "the parent's own permits; the effective boundary is the parent's and the child's together",
            "child_needs": needed,
        },
        "invocation": {"task": task, "workflow": child_path},
        "nodes": {"tasks": {task: sha256(&call)}},
        "candidate_sha256": sha256(&candidate),
        "check": parent_check,
        "child": {
            "component": {"id": component.id},
            "nodes": nodes(&child_doc, &Value::Object(produced)),
            "bindings": instance.bindings_record(),
            "candidate_sha256": sha256(&child),
            "check": child_check,
        },
    });
    Invocation {
        child,
        candidate,
        receipt,
    }
}
