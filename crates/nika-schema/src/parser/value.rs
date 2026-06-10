// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! YAML node → `serde_json::Value` conversion.
//!
//! A Nika file is **YAML 1.2** (spec `01-envelope.md` §YAML conventions ·
//! « a strict superset of JSON »). Free-form value positions (`with:`
//! values · untyped `vars:` defaults · `invoke.args:` · verb `schema:`)
//! convert to JSON values with YAML-1.2-core scalar typing ·
//!
//! - quoted scalars are ALWAYS strings (marked-yaml `may_coerce=false`)
//! - plain scalars coerce · `true`/`false` · `null`/`~`/empty · integers
//!   · floats · everything else stays a string (the « Norway problem »
//!   is dead — `no` is a string in YAML 1.2 core)

use marked_yaml::Node;
use marked_yaml::types::MarkedScalarNode;
use serde_json::Value;

/// Convert a YAML node to a JSON value.
///
/// Infallible · marked-yaml mapping keys are always scalars, so every
/// YAML shape has a JSON image.
pub(super) fn node_to_json(node: &Node) -> Value {
    match node {
        Node::Scalar(s) => scalar_to_json(s),
        Node::Sequence(seq) => Value::Array(seq.iter().map(node_to_json).collect()),
        Node::Mapping(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.as_str().to_owned(), node_to_json(value)))
                .collect(),
        ),
    }
}

/// Convert a YAML scalar with YAML-1.2-core typing.
fn scalar_to_json(scalar: &MarkedScalarNode) -> Value {
    let text = scalar.as_str();
    // Quoted scalars never coerce — they are strings, full stop.
    if !scalar.may_coerce() {
        return Value::String(text.to_owned());
    }
    // Null forms (YAML 1.2 core) · empty plain scalar is null too.
    if matches!(text, "" | "~" | "null" | "Null" | "NULL") {
        return Value::Null;
    }
    // Booleans · marked-yaml accepts true/True/TRUE + false/False/FALSE
    // (YAML 1.2 core schema — `yes`/`on` stay strings).
    if let Some(b) = scalar.as_bool() {
        return Value::Bool(b);
    }
    // Numbers · only attempt when it LOOKS numeric, so `inf`/`nan`-like
    // words and version strings stay strings.
    if looks_like_number(text) {
        if let Ok(i) = text.parse::<i64>() {
            return Value::Number(i.into());
        }
        if let Ok(f) = text.parse::<f64>()
            && let Some(n) = serde_json::Number::from_f64(f)
        {
            return Value::Number(n);
        }
    }
    Value::String(text.to_owned())
}

/// Heuristic gate before numeric parsing · first char is a digit (or a
/// sign followed by a digit) and every char is numeric-shaped.
fn looks_like_number(s: &str) -> bool {
    let mut chars = s.chars();
    let first_ok = match chars.next() {
        Some(c) if c.is_ascii_digit() => true,
        Some('-' | '+') => chars.next().is_some_and(|c| c.is_ascii_digit()),
        _ => false,
    };
    first_ok
        && s.chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Parse a tiny YAML doc (with the parser's canonical loader
    /// options) and convert the `v:` value to JSON.
    fn convert(yaml_value: &str) -> Value {
        let yaml = format!("v: {yaml_value}\n");
        let options = marked_yaml::LoaderOptions::default().prevent_coercion(true);
        let node = marked_yaml::parse_yaml_with_options(0, &yaml, options).expect("yaml");
        let mapping = node.as_mapping().expect("mapping");
        let v = mapping.get_node("v").expect("v");
        node_to_json(v)
    }

    #[test]
    fn plain_scalars_coerce_yaml12_core() {
        assert_eq!(convert("true"), json!(true));
        assert_eq!(convert("false"), json!(false));
        assert_eq!(convert("null"), json!(null));
        assert_eq!(convert("~"), json!(null));
        assert_eq!(convert("42"), json!(42));
        assert_eq!(convert("-7"), json!(-7));
        assert_eq!(convert("2.5"), json!(2.5));
        assert_eq!(convert("hello"), json!("hello"));
    }

    #[test]
    fn norway_problem_is_dead() {
        // YAML 1.2 core · `no` / `yes` / `on` / `off` are STRINGS.
        assert_eq!(convert("no"), json!("no"));
        assert_eq!(convert("yes"), json!("yes"));
        assert_eq!(convert("off"), json!("off"));
    }

    #[test]
    fn quoted_scalars_stay_strings() {
        assert_eq!(convert("\"true\""), json!("true"));
        assert_eq!(convert("\"42\""), json!("42"));
        assert_eq!(convert("'2.5'"), json!("2.5"));
        assert_eq!(convert("\"1.10\""), json!("1.10")); // version-like
    }

    #[test]
    fn words_that_parse_as_f64_stay_strings() {
        // Rust f64::from_str accepts "inf"/"nan" — the looks_like_number
        // gate keeps them strings (YAML 1.2 core uses `.inf`).
        assert_eq!(convert("inf"), json!("inf"));
        assert_eq!(convert("nan"), json!("nan"));
    }

    #[test]
    fn sequences_and_mappings_nest() {
        let yaml = "v:\n  - 1\n  - name: a\n    ok: true\n";
        let options = marked_yaml::LoaderOptions::default().prevent_coercion(true);
        let node = marked_yaml::parse_yaml_with_options(0, yaml, options).expect("yaml");
        let mapping = node.as_mapping().expect("mapping");
        let v = node_to_json(mapping.get_node("v").expect("v"));
        assert_eq!(v, json!([1, { "name": "a", "ok": true }]));
    }

    #[test]
    fn template_islands_stay_strings() {
        assert_eq!(
            convert("${{ tasks.research.output }}"),
            json!("${{ tasks.research.output }}")
        );
    }
}
