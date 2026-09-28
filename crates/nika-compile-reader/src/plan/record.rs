// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Replay must be lossless over every field this plan owns. Its canonical
//! serializer is the field vocabulary, avoiding a second independently drifting
//! JSON schema. Legacy absent optional fields may default; present data may not
//! disappear, change type or silently normalize into a different computation.

use serde_json::Value;

pub(super) fn slots(plan: &super::Plan) -> Result<(), String> {
    let mut declared = std::collections::BTreeSet::new();
    for (index, slot) in plan.slots.iter().enumerate() {
        let Some(slug) = slot.key.strip_prefix("const.") else {
            return Err(format!("`slots[{index}].key` must name const.<identifier>"));
        };
        if slug.is_empty() || !slug.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') {
            return Err(format!("`slots[{index}].key` is not a slot identifier"));
        }
        if !declared.insert(slug) {
            return Err(format!("`slots[{index}].key` duplicates an existing slot"));
        }
    }
    for (index, rule) in plan.rules.iter().enumerate() {
        for slug in rule.slots() {
            if !declared.contains(slug.as_str()) {
                return Err(format!(
                    "`rules[{index}]` refers to undeclared slot `{slug}`"
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn optional_array<'a>(record: &'a Value, key: &str) -> Result<&'a [Value], String> {
    match record.get(key) {
        None => Ok(&[]),
        Some(Value::Array(items)) => Ok(items),
        Some(_) => Err(format!("`{key}` is not an array")),
    }
}

/// Host metadata such as strategy and `pending_transform` is read by its owner,
/// not projected into Plan. Inside owned fields the vocabulary is closed.
pub(super) fn owned_fields(record: &Value, canonical: &Value) -> Result<(), String> {
    if let Value::Object(fields) = canonical {
        for (key, value) in fields {
            if let Some(authored) = record.get(key) {
                faithful(authored, value, key)?;
            }
        }
    }
    Ok(())
}

/// An accepted record is a left inverse on every provided field. Checking the
/// element before an array's length names the first dropped or coerced element.
pub(super) fn faithful(record: &Value, canonical: &Value, path: &str) -> Result<(), String> {
    match (record, canonical) {
        (Value::Object(authored), Value::Object(decoded)) => {
            for (key, value) in authored {
                let field = format!("{path}.{key}");
                let Some(back) = decoded.get(key) else {
                    return Err(format!("`{field}` is not a recorded field"));
                };
                faithful(value, back, &field)?;
            }
        }
        (Value::Array(authored), Value::Array(decoded)) => {
            for (index, value) in authored.iter().enumerate() {
                let field = format!("{path}[{index}]");
                let Some(back) = decoded.get(index) else {
                    return Err(format!(
                        "`{field}` cannot be read without dropping an element"
                    ));
                };
                faithful(value, back, &field)?;
            }
            if authored.len() != decoded.len() {
                return Err(format!(
                    "`{path}` does not preserve the recorded element count"
                ));
            }
        }
        _ if record != canonical => {
            return Err(format!(
                "`{path}` has a malformed or noncanonical recorded value"
            ));
        }
        _ => {}
    }
    Ok(())
}
