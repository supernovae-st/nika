// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Identity of the files a request names. Additional destinations observed by an EDIT do not
//! change its base's world. Facts about the original files stay bound, including their kinds.

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

fn name(path: &str) -> &str {
    path.trim_start_matches("./")
}

/// Identity of the observation relevant to the reader's stated sources and destinations.
#[must_use]
pub fn of_request(world: Option<&Value>, words: &str) -> String {
    use nika_compile_reader::hot::{stated_destinations, stated_sources};
    identity(
        world,
        &[stated_sources(words), stated_destinations(words)].concat(),
    )
}

/// Digest the observation of stated paths and files below stated folders in canonical order.
/// Non-observation knowledge stays whole. This is identity, never a freshness assertion:
/// the host must supply a current observation and the compiler still judges the complete base.
#[must_use]
pub fn identity(world: Option<&Value>, paths: &[String]) -> String {
    format!(
        "{:x}",
        Sha256::digest(scoped(world, paths).to_string().as_bytes())
    )
}

fn scoped(world: Option<&Value>, paths: &[String]) -> Value {
    let world = world.unwrap_or(&Value::Null);
    let Some(rows) = world["observed"].as_array() else {
        return world.clone();
    };
    let wanted = |path: &str| {
        paths
            .iter()
            .any(|p| name(p) == name(path) || (p.ends_with('/') && name(path).starts_with(name(p))))
    };
    let mut kept: Vec<Value> = rows
        .iter()
        .filter(|row| row["path"].as_str().is_some_and(wanted))
        .cloned()
        .map(|mut row| {
            if let Some(path) = row["path"].as_str() {
                row["path"] = Value::from(name(path));
            }
            row
        })
        .collect();
    kept.sort_by_cached_key(Value::to_string);
    kept.dedup();
    if kept.is_empty() {
        return Value::Null;
    }
    let mut result = world.clone();
    result["observed"] = Value::Array(kept);
    let mut kinds = Map::new();
    for (path, value) in world["kinds"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(p, _)| wanted(p))
    {
        let key = name(path).to_owned();
        match kinds.get(&key) {
            // Conflicting facts through aliases stay a disagreement, never silently selected.
            Some(previous) if previous != value => return world.clone(),
            _ => {
                kinds.insert(key, value.clone());
            }
        }
    }
    if let Some(object) = result.as_object_mut() {
        object.remove("kinds");
    }
    if !kinds.is_empty() {
        result["kinds"] = Value::Object(kinds);
    }
    result
}

/// Whether `text` is a number the number-text law reads (`nika_compile_reader::text::
/// NUMBER_TEXT`, the pattern the compiled jq tests): blanks around, an optional minus, `0` or
/// digits without a leading zero, an optional fraction, an optional exponent (R4 A6), and a
/// finite value; nothing else.
#[must_use]
pub fn number_text(text: &str) -> bool {
    let body = text.trim_matches([' ', '\t']);
    let unsigned = body.strip_prefix('-').unwrap_or(body);
    let (mantissa, exponent) = unsigned
        .split_once(['e', 'E'])
        .map_or((unsigned, None), |(m, e)| (m, Some(e)));
    let (whole, fraction) = mantissa
        .split_once('.')
        .map_or((mantissa, None), |(w, f)| (w, Some(f)));
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    digits(whole)
        && (whole == "0" || !whole.starts_with('0'))
        && fraction.is_none_or(digits)
        && exponent.is_none_or(|e| digits(e.strip_prefix(['+', '-']).unwrap_or(e)))
        // An overflowing value is no number (`1e999`): the law tests finiteness too.
        && body.parse::<f64>().is_ok_and(f64::is_finite)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unrelated_destination_and_alias_preserve_identity_but_changed_source_does_not() {
        let paths = vec!["./in.json".to_owned(), "old.json".to_owned()];
        let old = json!({"observed": [{"path": "./in.json", "columns": ["stock"]},
            {"path": "old.json", "state": "absent"}], "kinds": {"./in.json": {"stock": "number"}}});
        let next = json!({"observed": [{"path": "in.json", "columns": ["stock"]},
            {"path": "old.json", "state": "absent"}, {"path": "./old.json", "state": "absent"},
            {"path": "new.json", "state": "absent"}], "kinds": {"in.json": {"stock": "number"}}});
        assert_eq!(identity(Some(&old), &paths), identity(Some(&next), &paths));
        let mut changed = next.clone();
        changed["observed"][0]["columns"] = json!(["qty"]);
        assert_ne!(
            identity(Some(&old), &paths),
            identity(Some(&changed), &paths)
        );
        changed = next;
        changed["kinds"]["in.json"]["stock"] = json!("text");
        assert_ne!(
            identity(Some(&old), &paths),
            identity(Some(&changed), &paths)
        );
    }
}
