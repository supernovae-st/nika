// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What the latest published SDK closes (the lead's rule, 12 Oct 2026). A published SDK refuses a
//! key or an enum value it does not know on the objects its parsers close. Such a change may
//! reach a served document only in the commit that also moves a clock that SDK enforces. The SDK
//! then refuses this engine at identity (`GET /health`, `nika --sdk-identity`), before any job is
//! admitted, never in the middle of a run: never a Job it cannot read after the engine admitted
//! it, never a cancel it reports failed after the engine applied it.
//!
//! `published_sdk.json` holds that SDK's closed sets, read from its published `dist`
//! (`@supernovae-st/nika@0.123.1`). It moves only when a newer SDK is published. That SDK also
//! reads the generation-2 compile outcome with the same checks: `CompileOutcomeV2` points at
//! `CompileOutcome`'s own status, questions, diagnostics and check preview, so the
//! `CompileOutcome` entries hold them, and only its own provenance is listed under its name.

use serde_json::Value;

/// The closed sets of the latest published SDK.
const PUBLISHED: &str = include_str!("published_sdk.json");

#[test]
fn what_a_published_sdk_closes_changes_only_with_a_clock_it_enforces() {
    let published: Value =
        serde_json::from_str(PUBLISHED).expect("the published SDK's closed sets");
    let enforced = published["enforced_clocks"]["machineProtocolVersion"].as_u64();
    let moved = enforced != Some(u64::from(nika_runtime::MACHINE_PROTOCOL_VERSION));
    // The widest document a resident serves: native authoring, the cost review, the Session door.
    let refused = refused_by(&published, &super::served(true, true, true));
    assert!(
        moved || refused.is_empty(),
        "{} refuses what this engine serves, and no clock it enforces moved: {refused:#?}\n\
         Raise MACHINE_PROTOCOL_VERSION in the same commit (that SDK then refuses this engine at \
         /health and --sdk-identity, before any job), or serve the object as it was.",
        published["sdk"]
    );
}

/// The comparison itself, on a document that changes each kind of closed thing once.
#[test]
fn a_gained_key_a_gained_value_or_an_opened_object_is_named() {
    let published = serde_json::json!({
        "closed": [
            {"schema": "Job", "keys": ["id"]},
            {"schema": "Error", "member": "error", "keys": ["code"]},
            {"schema": "JobOrigin", "variant": "manual", "keys": ["kind"]}
        ],
        "enums": [
            {"schema": "JobStatus", "values": ["queued"]},
            {"schema": "CompileOutcome", "member": "questions[].type", "values": ["text"]}
        ]
    });
    let document = serde_json::json!({"components": {"schemas": {
        "Job": {"additionalProperties": false, "properties": {"id": {}, "spend": {}}},
        "Error": {"additionalProperties": false, "properties": {
            "error": {"properties": {"code": {}}}
        }},
        "JobOrigin": {"oneOf": [
            {"additionalProperties": false, "properties": {"kind": {"const": "manual"}}}
        ]},
        "JobStatus": {"enum": ["queued", "parked"]},
        "CompileOutcome": {"properties": {
            "questions": {"items": {"$ref": "#/components/schemas/Question"}}
        }},
        "Question": {"properties": {"type": {"enum": ["text", "choice"]}}}
    }}});
    assert_eq!(
        refused_by(&published, &document),
        [
            "Job gains [\"spend\"]",
            "Error.error is no longer closed",
            "JobStatus gains [\"parked\"]",
            "CompileOutcome.questions[].type gains [\"choice\"]",
        ]
    );
}

/// Every served key or enum value the published SDK refuses, in the fixture's order.
fn refused_by(published: &Value, document: &Value) -> Vec<String> {
    let mut refused = Vec::new();
    for place in published["closed"].as_array().into_iter().flatten() {
        let label = label(place);
        let Some(node) = locate(document, place) else {
            refused.push(format!("{label} is not in the served document"));
            continue;
        };
        let keys = texts(&place["keys"]);
        let gained: Vec<&str> = (node["properties"].as_object().into_iter())
            .flat_map(|properties| properties.keys())
            .map(String::as_str)
            .filter(|key| !keys.contains(key))
            .collect();
        if !gained.is_empty() {
            refused.push(format!("{label} gains {gained:?}"));
        }
        if node["additionalProperties"] != Value::Bool(false) {
            refused.push(format!("{label} is no longer closed"));
        }
    }
    for place in published["enums"].as_array().into_iter().flatten() {
        let label = label(place);
        let Some(node) = locate(document, place) else {
            refused.push(format!("{label} is not in the served document"));
            continue;
        };
        let values = texts(&place["values"]);
        let gained: Vec<String> = (served_values(document, node).into_iter())
            .filter(|value| !values.contains(&value.as_str()))
            .collect();
        if !gained.is_empty() {
            refused.push(format!("{label} gains {gained:?}"));
        }
    }
    refused
}

/// `Schema<variant>.member`, as the fixture names a place.
fn label(place: &Value) -> String {
    let schema = place["schema"].as_str().unwrap_or_default();
    let variant = (place["variant"].as_str()).map_or_else(String::new, |kind| format!("<{kind}>"));
    let member = (place["member"].as_str()).map_or_else(String::new, |path| format!(".{path}"));
    format!("{schema}{variant}{member}")
}

/// The schema node a place names: the component, its `kind` variant, then the member path
/// (`a.b`, `[]` for an array's items), following `$ref`s on the way.
fn locate<'a>(document: &'a Value, place: &Value) -> Option<&'a Value> {
    let schemas = &document["components"]["schemas"];
    let mut node = resolve(document, schemas.get(place["schema"].as_str()?)?);
    if let Some(kind) = place["variant"].as_str() {
        node = alternatives(node)
            .map(|option| resolve(document, option))
            .find(|option| option["properties"]["kind"]["const"] == kind)?;
    }
    for step in (place["member"].as_str().into_iter()).flat_map(|path| path.split('.')) {
        let (name, items) = step
            .strip_suffix("[]")
            .map_or((step, false), |name| (name, true));
        node = resolve(document, node["properties"].get(name)?);
        if items {
            node = resolve(document, node.get("items")?);
        }
    }
    Some(node)
}

/// Follow `$ref`s to the schema they name (bounded: a cycle is never followed forever).
fn resolve<'a>(document: &'a Value, mut node: &'a Value) -> &'a Value {
    for _ in 0..8 {
        let Some(name) = (node["$ref"].as_str()).and_then(|reference| reference.rsplit('/').next())
        else {
            break;
        };
        node = &document["components"]["schemas"][name];
    }
    node
}

/// A node's `oneOf` and `anyOf` alternatives.
fn alternatives(node: &Value) -> impl Iterator<Item = &Value> {
    ["oneOf", "anyOf"]
        .into_iter()
        .filter_map(|key| node[key].as_array())
        .flatten()
}

/// The values a schema node admits: its `enum`, its `const`, and those of its alternatives,
/// `null` aside.
fn served_values(document: &Value, node: &Value) -> Vec<String> {
    let node = resolve(document, node);
    let mut values: Vec<String> = (node["enum"].as_array().into_iter())
        .flatten()
        .chain(node.get("const"))
        .filter(|value| !value.is_null())
        .map(|value| {
            value
                .as_str()
                .map_or_else(|| value.to_string(), str::to_owned)
        })
        .collect();
    for option in alternatives(node) {
        values.extend(served_values(document, option));
    }
    values
}

/// The strings of a JSON list.
fn texts(list: &Value) -> Vec<&str> {
    (list.as_array().into_iter())
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}
