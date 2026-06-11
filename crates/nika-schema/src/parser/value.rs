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

use super::Cx;
use crate::error::SchemaError;

/// Maximum nesting depth for free-form YAML values (`schema:` · `args:`
/// · `with:` · `vars` defaults · `for_each` lists · `recover`). Real
/// workflow values nest < 30; 128 is generous headroom. The cap is a
/// STACK-SAFETY bound: every downstream walker (schema lint · dataflow
/// typing · strictness · the runtime `compile_schema`) recurses over
/// this value — bounding it HERE bounds them all (one seam, no
/// per-walker guards), and recursive `Drop` of the built `Value` stays
/// shallow too. Closes the pre-admission note in `parser/mod.rs`
/// (« `node_to_json` recurses unbounded »).
pub(super) const MAX_VALUE_DEPTH: usize = 128;

/// Convert a free-form YAML node to a JSON value, depth-capped.
///
/// # Errors
///
/// Rejects (loud, with span) any value nested deeper than
/// [`MAX_VALUE_DEPTH`] — silently truncating would hand the runtime a
/// DIFFERENT value than the author wrote.
pub(super) fn json_value(cx: &Cx<'_>, node: &Node) -> Result<Value, SchemaError> {
    to_json(node, 0).ok_or_else(|| SchemaError::Validation {
        message: format!(
            "value nesting exceeds {MAX_VALUE_DEPTH} levels — a workflow value \
             this deep is rejected (stack-safety bound)"
        ),
        span: cx.span(node.span()),
    })
}

/// The recursive core — `None` past the depth cap (≤ [`MAX_VALUE_DEPTH`]
/// frames by construction).
fn to_json(node: &Node, depth: usize) -> Option<Value> {
    if depth >= MAX_VALUE_DEPTH {
        return None;
    }
    Some(match node {
        Node::Scalar(s) => scalar_to_json(s),
        Node::Sequence(seq) => Value::Array(
            seq.iter()
                .map(|n| to_json(n, depth + 1))
                .collect::<Option<_>>()?,
        ),
        Node::Mapping(map) => Value::Object(
            map.iter()
                .map(|(key, value)| Some((key.as_str().to_owned(), to_json(value, depth + 1)?)))
                .collect::<Option<_>>()?,
        ),
    })
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
        to_json(v, 0).expect("within depth cap")
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
        let v = to_json(mapping.get_node("v").expect("v"), 0).expect("within depth cap");
        assert_eq!(v, json!([1, { "name": "a", "ok": true }]));
    }

    #[test]
    fn nesting_past_the_cap_is_rejected_not_built() {
        // depth MAX+10 in flow style (small string · marked-yaml's own
        // flow recursion limit is above this) → to_json returns None
        // instead of building (and later Drop-recursing) a deep Value.
        let depth = MAX_VALUE_DEPTH + 10;
        let yaml = format!("v: {}1{}\n", "[".repeat(depth), "]".repeat(depth));
        let options = marked_yaml::LoaderOptions::default().prevent_coercion(true);
        let node = marked_yaml::parse_yaml_with_options(0, &yaml, options).expect("yaml");
        let mapping = node.as_mapping().expect("mapping");
        assert!(
            to_json(mapping.get_node("v").expect("v"), 0).is_none(),
            "past the cap → None, loud at the caller"
        );
        // exactly AT the boundary: depth MAX-1 still converts
        let depth = MAX_VALUE_DEPTH - 1;
        let yaml = format!("v: {}1{}\n", "[".repeat(depth), "]".repeat(depth));
        let options = marked_yaml::LoaderOptions::default().prevent_coercion(true);
        let node = marked_yaml::parse_yaml_with_options(0, &yaml, options).expect("yaml");
        let mapping = node.as_mapping().expect("mapping");
        assert!(to_json(mapping.get_node("v").expect("v"), 0).is_some());
    }

    #[test]
    fn template_islands_stay_strings() {
        assert_eq!(
            convert("${{ tasks.research.output }}"),
            json!("${{ tasks.research.output }}")
        );
    }
}
