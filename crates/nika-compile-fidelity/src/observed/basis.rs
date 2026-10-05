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

/// The complete host observation kept as historical data with its exact digest. Graph reads
/// can be placed below a bare name the words state, so a word-only projection would lose them.
/// The existing program-record owner withholds a whole plan over 128 KiB; nothing is truncated.
#[must_use]
pub fn keep(world: Option<&Value>) -> Value {
    let value = world.unwrap_or(&Value::Null);
    serde_json::json!({"value": value, "sha256": format!("{:x}", Sha256::digest(value.to_string().as_bytes()))})
}

fn for_request(world: Option<&Value>, words: &str) -> Value {
    use nika_compile_reader::hot::{stated_destinations, stated_sources};
    scoped(
        world,
        &[stated_sources(words), stated_destinations(words)].concat(),
    )
}

/// Recover the world in which a base was assembled, with its current read sources unchanged.
/// New records carry the whole observation, checked against its digest and the request digest. Old
/// records can recover only a previously absent write-only destination, and only if replacing
/// its observation by absence reproduces that exact digest. A missing historical fact is never
/// guessed past that comparison. This is a reconstruction, never current Save/Run authority.
#[must_use]
pub fn for_base(world: Option<&Value>, words: &str, graph: &Value, basis: &Value) -> Option<Value> {
    let expected = basis["read"]["world_sha256"].as_str()?;
    let current = for_request(world, words);
    let sketch = crate::sketch::Sketch::from_json(graph).ok()?;
    let mut sources = nika_compile_reader::hot::stated_sources(words);
    sources.extend(sketch.tasks.iter().flat_map(|t| t.reads.clone()));
    let opaque_read = sources
        .iter()
        .any(|path| path.contains(['*', '?', '[', '$']));
    if let Some(kept) = basis.get("world") {
        let history = kept.get("value")?;
        return (keep(Some(history)) == *kept
            && of_request(Some(history), words) == expected
            && if opaque_read {
                // No pattern matcher is invented here: require the complete world unchanged.
                keep(Some(history)) == keep(world)
            } else {
                identity(Some(history), &sources) == identity(world, &sources)
            })
        .then(|| history.clone());
    }
    // A legacy word-only hash cannot establish the history of an additionally placed graph read.
    if opaque_read || identity(Some(&current), &sources) != identity(world, &sources) {
        return None;
    }
    if of_request(Some(&current), words) == expected {
        return Some(current);
    }
    let literal = |path: &str| {
        !name(path).is_empty()
            && name(path)
                .split('/')
                .all(|part| !matches!(part, "" | "." | ".."))
            && !path.contains(['*', '?', '[', '$', '\\'])
    };
    // A glob or dynamic read could overlap a destination: legacy recovery then knows too little.
    if sources.iter().any(|path| !literal(path)) {
        return None;
    }
    let writes: Vec<_> = sketch
        .tasks
        .iter()
        .filter(|t| t.tool.as_deref() == Some("nika:write"))
        .flat_map(|t| t.writes.iter())
        .filter(|path| literal(path) && !sources.iter().any(|p| name(p) == name(path)))
        .collect();
    if writes.is_empty() {
        return None;
    }
    let written = |path: &str| writes.iter().any(|p| name(p) == name(path));
    let mut historical = current;
    let rows = historical["observed"].as_array_mut()?;
    if rows.iter().any(|row| {
        row["path"].as_str().is_some_and(written)
            && !matches!(row["state"].as_str(), Some("observed" | "absent"))
    }) {
        return None;
    }
    rows.retain(|row| !row["path"].as_str().is_some_and(written));
    rows.extend(writes.iter().map(|path| {
        serde_json::json!({
            "path": path, "state": "absent", "complete": false,
        })
    }));
    if let Some(kinds) = historical["kinds"].as_object_mut() {
        kinds.retain(|path, _| !written(path));
    }
    let historical = for_request(Some(&historical), words);
    (of_request(Some(&historical), words) == expected).then_some(historical)
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod historical_tests {
    use super::*;
    use serde_json::json;

    const WORDS: &str = "Read ./in.json and write it to ./out.json.";

    fn graph() -> Value {
        json!({"name": "copy", "tasks": [
            {"id": "read", "verb": "invoke", "tool": "nika:read", "reads": ["./in.json"], "purpose": "input"},
            {"id": "write", "verb": "invoke", "tool": "nika:write", "writes": ["./out.json"], "purpose": "output"}],
            "outputs": []})
    }

    fn worlds() -> (Value, Value) {
        let old = json!({"observed": [
            {"path": "./in.json", "state": "observed", "columns": ["amount"], "peek_sha256": "input"},
            {"path": "./out.json", "state": "absent", "complete": false}],
            "kinds": {"./in.json": {"amount": "number"}}});
        let mut now = old.clone();
        now["observed"][1] =
            json!({"path": "./out.json", "state": "observed", "peek_sha256": "output"});
        now["kinds"]["./out.json"] = json!({"total": "number"});
        (old, now)
    }

    #[test]
    fn a_legacy_absent_destination_is_recovered_only_by_its_exact_digest() {
        let (old, now) = worlds();
        let read = json!({"read": {"world_sha256": of_request(Some(&old), WORDS)}});
        assert_eq!(
            for_base(Some(&now), WORDS, &graph(), &read),
            Some(for_request(Some(&old), WORDS))
        );
        for field in ["peek_sha256", "columns"] {
            let mut changed = now.clone();
            changed["observed"][0][field] = json!("changed");
            assert!(for_base(Some(&changed), WORDS, &graph(), &read).is_none());
        }
        let mut changed = now.clone();
        changed["kinds"]["./in.json"] = json!({"amount": "text"});
        assert!(for_base(Some(&changed), WORDS, &graph(), &read).is_none());
        changed = now;
        changed["observed"][1]["state"] = json!("outside_project");
        assert!(for_base(Some(&changed), WORDS, &graph(), &read).is_none());
        assert!(
            for_base(
                Some(&old),
                WORDS,
                &graph(),
                &json!({"read": {"world_sha256": "wrong"}})
            )
            .is_none()
        );
    }

    #[test]
    fn durable_history_keeps_sources_strict_after_an_existing_output_changes() {
        let (_, written) = worlds();
        let read = json!({"read": {"world_sha256": of_request(Some(&written), WORDS)},
            "world": keep(Some(&written))});
        let mut current = written.clone();
        current["observed"][1]["peek_sha256"] = json!("second-output");
        assert_eq!(
            for_base(Some(&current), WORDS, &graph(), &read),
            Some(written.clone())
        );
        assert_eq!(
            current["observed"][1]["peek_sha256"], "second-output",
            "current world stays current"
        );
        let mut tampered = read.clone();
        tampered["world"]["value"]["observed"][0]["state"] = json!("forged");
        assert!(for_base(Some(&current), WORDS, &graph(), &tampered).is_none());
        current["observed"][0]["peek_sha256"] = json!("changed-input");
        assert!(for_base(Some(&current), WORDS, &graph(), &read).is_none());
    }

    #[test]
    fn a_destination_also_read_or_a_dynamic_read_cannot_be_recovered_as_absent() {
        let (old, now) = worlds();
        let read = json!({"read": {"world_sha256": of_request(Some(&old), WORDS)}});
        for path in ["./out.json", "./*.json"] {
            let mut graph = graph();
            graph["tasks"][0]["reads"] = json!(["./in.json", path]);
            assert!(for_base(Some(&now), WORDS, &graph, &read).is_none());
            let durable = json!({"read": read["read"], "world": keep(Some(&old))});
            assert!(for_base(Some(&now), WORDS, &graph, &durable).is_none());
        }
    }

    #[test]
    fn full_history_keeps_graph_reads_placed_outside_the_literal_words() {
        let words = "Read in.json and write it to ./out.json.";
        let (mut old, _) = worlds();
        old["observed"][0]["path"] = json!("./data/in.json");
        old["kinds"] = json!({"./data/in.json": {"amount": "number"}});
        let mut graph = graph();
        graph["tasks"][0]["reads"] = json!(["./data/in.json"]);
        let basis = json!({"read": {"world_sha256": of_request(Some(&old), words)}, "world": keep(Some(&old))});
        let mut now = old.clone();
        now["observed"][1] =
            json!({"path": "./out.json", "state": "observed", "peek_sha256": "new-output"});
        assert_eq!(
            for_base(Some(&now), words, &graph, &basis),
            Some(old.clone())
        );
        let mut legacy = basis.clone();
        legacy.as_object_mut().unwrap().remove("world");
        assert!(
            for_base(Some(&now), words, &graph, &legacy).is_none(),
            "old word-only evidence cannot prove a placed source"
        );
        let mut moved = old.clone();
        moved["observed"][0]["peek_sha256"] = json!("changed-input");
        assert_eq!(
            of_request(Some(&moved), words),
            of_request(Some(&old), words),
            "the old word projection alone cannot see this source"
        );
        assert!(
            for_base(Some(&moved), words, &graph, &basis).is_none(),
            "full history sees the graph source even before destination movement"
        );
    }
}
