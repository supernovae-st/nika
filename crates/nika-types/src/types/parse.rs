// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Parsing the v1 type grammar (spec 09 §grammar) from a neutral
//! `serde_json::Value` — plus the LOCKED regex dialect scanner
//! (spec 09 §the regex dialect · `NIKA-TYPE-006`).
//!
//! `{ optional: T }` is a FIELD-PRESENCE modifier: legal only at field
//! positions inside `{ object: … }` (the object parser handles it);
//! anywhere else it is refused with the teaching (spec 09 §optional is
//! presence, not null).

use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use serde_json::Value;

use super::{Field, NikaType, NumBounds, Primitive, StrBounds};

/// Reserved constructors — named in the spec, landing with their waves.
const RESERVED: [(&str, &str); 4] = [
    ("result", "outcomes (W5)"),
    ("artifact", "artifact lanes (W5)"),
    ("secret", "the authority wave (W4)"),
    (
        "money",
        "the decision core (W-DEC) — fixed-point + ISO-4217, never binary floats",
    ),
];

/// A pattern longer than this is out of dialect (spec 09).
const MAX_PATTERN_LEN: usize = 512;

/// A type-expression parse failure — `code` is the spec wire code this
/// refusal rides (`NIKA-TYPE-001` grammar · `NIKA-TYPE-006` dialect).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ParseTypeError {
    /// The spec wire code.
    pub code: &'static str,
    /// Human diagnostic (place + why + did-you-mean when close).
    pub detail: String,
}

impl ParseTypeError {
    fn grammar(detail: String) -> Self {
        Self {
            code: "NIKA-TYPE-001",
            detail,
        }
    }
    fn dialect(detail: String) -> Self {
        Self {
            code: "NIKA-TYPE-006",
            detail,
        }
    }
}

/// Is this a legal declared-type name (`PascalCase` · spec 09)?
#[must_use]
pub fn is_type_name(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_uppercase())
        && chars.all(|c| c.is_ascii_alphanumeric())
}

/// Every `PascalCase` name referenced anywhere inside a raw type
/// expression — the acyclicity graph (`NIKA-TYPE-002`) walks THESE
/// before any parse recurses.
#[must_use]
pub fn type_name_refs(value: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    collect_refs(value, &mut out);
    out
}

fn collect_refs(value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::String(s) if is_type_name(s) => {
            out.insert(s.clone());
        }
        Value::Object(map) => {
            for v in map.values() {
                collect_refs(v, out);
            }
        }
        Value::Array(items) => {
            for v in items {
                collect_refs(v, out);
            }
        }
        _ => {}
    }
}

/// The locked regex dialect — `None` when inside, else the offending
/// construct (the `NIKA-TYPE-006` detail). A HAND scanner, never the
/// host regex engine's parser: both evaluators must accept/refuse
/// identically (mirrors `conformance/type_core.py`).
#[must_use]
pub fn regex_dialect_violation(pattern: &str) -> Option<String> {
    const ESCAPABLE: &[u8] = b"dDwWsS\\.^$+*?()[]{}|/-nrt";
    if pattern.len() > MAX_PATTERN_LEN {
        return Some(format!("pattern longer than {MAX_PATTERN_LEN} chars"));
    }
    let bytes = pattern.as_bytes();
    let n = bytes.len();
    let mut i = 0usize;
    let mut in_class = false;
    let mut quantifiable = false;
    while i < n {
        let c = bytes[i];
        if in_class {
            match c {
                b'\\' => match bytes.get(i + 1) {
                    Some(&nxt) if ESCAPABLE.contains(&nxt) => {
                        i += 2;
                        continue;
                    }
                    Some(&nxt) => {
                        return Some(format!("escape \\{} (out of dialect)", nxt as char));
                    }
                    None => return Some("trailing backslash".to_owned()),
                },
                b']' => in_class = false,
                _ => {}
            }
            i += 1;
            continue;
        }
        match c {
            b'\\' => {
                if let Some(offense) = escape_violation(bytes.get(i + 1).copied()) {
                    return Some(offense);
                }
                quantifiable = true;
                i += 2;
                continue;
            }
            b'(' => {
                if pattern[i..].starts_with("(?:") {
                    i += 3;
                    quantifiable = false;
                    continue;
                }
                if pattern[i..].starts_with("(?") {
                    let upto = pattern[i..].chars().take(4).collect::<String>();
                    return Some(format!(
                        "group construct {upto:?} (only (…) and (?:…) are in dialect)"
                    ));
                }
                quantifiable = false;
            }
            b'[' => {
                in_class = true;
                i += 1;
                if bytes.get(i) == Some(&b'^') {
                    i += 1;
                }
                quantifiable = true;
                continue;
            }
            b'*' | b'+' | b'?' => {
                if !quantifiable {
                    return Some(format!("quantifier {:?} with nothing to repeat", c as char));
                }
                if matches!(bytes.get(i + 1), Some(b'?' | b'+')) {
                    return Some(format!(
                        "lazy/possessive quantifier {:?}",
                        &pattern[i..(i + 2).min(n)]
                    ));
                }
                quantifiable = false;
            }
            b'{' => match scan_brace_quantifier(pattern, i, quantifiable) {
                Ok(end) => {
                    quantifiable = false;
                    i = end;
                    continue;
                }
                Err(offense) => return Some(offense),
            },
            b'|' | b'^' | b'$' => quantifiable = false,
            _ => quantifiable = true,
        }
        i += 1;
    }
    if in_class {
        return Some("unterminated character class".to_owned());
    }
    None
}

/// Judge one escape OUTSIDE a class — `None` when `\\<nxt>` is in the
/// closed dialect (the caller then consumes both bytes and the atom is
/// quantifiable), else the offense narrative.
fn escape_violation(nxt: Option<u8>) -> Option<String> {
    const ESCAPABLE: &[u8] = b"dDwWsS\\.^$+*?()[]{}|/-nrt";
    let Some(nxt) = nxt else {
        return Some("trailing backslash".to_owned());
    };
    if nxt.is_ascii_digit() {
        return Some(format!("backreference \\{}", nxt as char));
    }
    match nxt {
        b'b' | b'B' => Some(format!("word boundary \\{}", nxt as char)),
        b'p' | b'P' => Some("unicode property class \\p{…}".to_owned()),
        b'x' | b'u' => Some(format!(
            "hex/unicode escape \\{} (out of dialect)",
            nxt as char
        )),
        _ if ESCAPABLE.contains(&nxt) => None,
        _ => Some(format!("escape \\{} (out of dialect)", nxt as char)),
    }
}

/// Judge a `{m,n}` quantifier at `i` — `Ok(end)` (the index past `}`)
/// when well-formed, applied to a quantifiable atom, and not
/// lazy/possessive; else the offense narrative.
fn scan_brace_quantifier(pattern: &str, i: usize, quantifiable: bool) -> Result<usize, String> {
    let bytes = pattern.as_bytes();
    let end = match brace_quantifier_end(&pattern[i..]) {
        Some(off) => i + off,
        None => return Err("malformed {m,n} quantifier".to_owned()),
    };
    if !quantifiable {
        return Err("quantifier {…} with nothing to repeat".to_owned());
    }
    if matches!(bytes.get(end), Some(b'?' | b'+')) {
        return Err(format!(
            "lazy/possessive quantifier {:?}",
            &pattern[i..(end + 1).min(pattern.len())]
        ));
    }
    Ok(end)
}

/// `{m}` · `{m,}` · `{m,n}` — returns the offset just past `}`.
fn brace_quantifier_end(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 1usize;
    let start = i;
    while bytes.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
    }
    if i == start {
        return None;
    }
    if bytes.get(i) == Some(&b',') {
        i += 1;
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
    }
    (bytes.get(i) == Some(&b'}')).then_some(i + 1)
}

/// Parse + normalize a v1 type expression (spec 09 §grammar).
///
/// # Errors
///
/// `NIKA-TYPE-001` on anything outside the closed grammar (with the
/// teaching — reserved names say their wave, near-misses say
/// did-you-mean) · `NIKA-TYPE-006` on a pattern outside the locked
/// regex dialect.
pub fn parse_type(
    value: &Value,
    names: &BTreeSet<String>,
    where_: &str,
) -> Result<NikaType, ParseTypeError> {
    match value {
        // the bare YAML null scalar spells the null type
        Value::Null => Ok(NikaType::Prim(Primitive::Null)),
        Value::String(s) => parse_name(s, names, where_),
        Value::Object(map) => parse_composite(map, names, where_),
        other => Err(ParseTypeError::grammar(format!(
            "{where_} · not a type: {other} (a primitive name · a PascalCase reference · or a constructor map)"
        ))),
    }
}

fn parse_name(s: &str, names: &BTreeSet<String>, where_: &str) -> Result<NikaType, ParseTypeError> {
    if let Some(p) = Primitive::from_name(s) {
        return Ok(NikaType::Prim(p));
    }
    if is_type_name(s) {
        if names.contains(s) {
            return Ok(NikaType::Ref(s.to_owned()));
        }
        let hint = closest(s, names)
            .map(|c| format!(" — did you mean `{c}`?"))
            .unwrap_or_default();
        return Err(ParseTypeError::grammar(format!(
            "{where_} · unknown type name `{s}`{hint}"
        )));
    }
    let base = s.split('<').next().unwrap_or(s);
    if let Some((_, wave)) = RESERVED.iter().find(|(r, _)| *r == base) {
        return Err(ParseTypeError::grammar(format!(
            "{where_} · `{s}` is reserved — lands with {wave}"
        )));
    }
    Err(ParseTypeError::grammar(format!(
        "{where_} · `{s}` is not a type (primitives are lowercase · declared names are PascalCase)"
    )))
}

fn parse_composite(
    map: &serde_json::Map<String, Value>,
    names: &BTreeSet<String>,
    where_: &str,
) -> Result<NikaType, ParseTypeError> {
    let keys: Vec<&str> = map
        .keys()
        .map(String::as_str)
        .filter(|k| *k != "additional")
        .collect();
    if keys == ["optional"] {
        return Err(ParseTypeError::grammar(format!(
            "{where_} · optional is a field-presence modifier — for a nullable \
             value write union: [T, null]"
        )));
    }
    let [key] = keys.as_slice() else {
        return Err(ParseTypeError::grammar(format!(
            "{where_} · a type constructor is ONE key (array · map · object · union · \
             enum · integer · number · string), got: {keys:?}"
        )));
    };
    match *key {
        "array" => Ok(NikaType::Array(Box::new(parse_type(
            &map["array"],
            names,
            &format!("{where_}.array"),
        )?))),
        "map" => Ok(NikaType::Map(Box::new(parse_type(
            &map["map"],
            names,
            &format!("{where_}.map"),
        )?))),
        "object" => parse_object(map, names, where_),
        "union" => {
            let Value::Array(items) = &map["union"] else {
                return Err(ParseTypeError::grammar(format!(
                    "{where_}.union · needs a list of ≥ 2 members"
                )));
            };
            if items.len() < 2 {
                return Err(ParseTypeError::grammar(format!(
                    "{where_}.union · needs ≥ 2 members (one member is just the member)"
                )));
            }
            let members = items
                .iter()
                .map(|m| parse_type(m, names, &format!("{where_}.union")))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(NikaType::union_of(members))
        }
        "enum" => {
            let Value::Array(items) = &map["enum"] else {
                return Err(ParseTypeError::grammar(format!(
                    "{where_}.enum · needs a non-empty list of strings"
                )));
            };
            let mut values: Vec<String> = Vec::with_capacity(items.len());
            for it in items {
                let Value::String(s) = it else {
                    return Err(ParseTypeError::grammar(format!(
                        "{where_}.enum · members are strings, got {it}"
                    )));
                };
                if !values.contains(s) {
                    values.push(s.clone());
                }
            }
            if values.is_empty() {
                return Err(ParseTypeError::grammar(format!(
                    "{where_}.enum · needs ≥ 1 member"
                )));
            }
            values.sort();
            Ok(NikaType::Enum(values))
        }
        "integer" | "number" => {
            let bounds = num_bounds(&map[*key], where_, key)?;
            // an unbounded refinement IS its primitive (spec 09
            // §normalization — canonical forms)
            if bounds.min.is_none() && bounds.max.is_none() {
                return Ok(NikaType::Prim(if *key == "integer" {
                    Primitive::Integer
                } else {
                    Primitive::Number
                }));
            }
            Ok(if *key == "integer" {
                NikaType::BoundedInt(bounds)
            } else {
                NikaType::BoundedNum(bounds)
            })
        }
        "string" => parse_refined_string(&map["string"], where_),
        other => Err(ParseTypeError::grammar(format!(
            "{where_} · `{other}:` is not a v1 type constructor"
        ))),
    }
}

fn parse_refined_string(value: &Value, where_: &str) -> Result<NikaType, ParseTypeError> {
    let Value::Object(b) = value else {
        return Err(ParseTypeError::grammar(format!(
            "{where_}.string · refinement must be a map (pattern · min_len · max_len)"
        )));
    };
    let mut out = StrBounds::new(None, None, None);
    for (k, v) in b {
        match (k.as_str(), v) {
            ("pattern", Value::String(p)) => {
                if let Some(offense) = regex_dialect_violation(p) {
                    return Err(ParseTypeError::dialect(format!(
                        "{where_}.string.pattern · out of the locked dialect: {offense}"
                    )));
                }
                out.pattern = Some(p.clone());
            }
            ("min_len" | "max_len", Value::Number(v0)) => {
                let Some(n) = v0.as_u64() else {
                    return Err(ParseTypeError::grammar(format!(
                        "{where_}.string.{k} · must be a non-negative integer"
                    )));
                };
                if k == "min_len" {
                    out.min_len = Some(n);
                } else {
                    out.max_len = Some(n);
                }
            }
            _ => {
                return Err(ParseTypeError::grammar(format!(
                    "{where_}.string.{k} · not a string refinement (pattern · min_len · max_len)"
                )));
            }
        }
    }
    if let (Some(lo), Some(hi)) = (out.min_len, out.max_len)
        && lo > hi
    {
        return Err(ParseTypeError::grammar(format!(
            "{where_}.string · empty range: min_len > max_len"
        )));
    }
    // an unbounded refinement IS its primitive (spec 09 §normalization)
    if out.pattern.is_none() && out.min_len.is_none() && out.max_len.is_none() {
        return Ok(NikaType::Prim(Primitive::String));
    }
    Ok(NikaType::RefinedStr(out))
}

fn parse_object(
    map: &serde_json::Map<String, Value>,
    names: &BTreeSet<String>,
    where_: &str,
) -> Result<NikaType, ParseTypeError> {
    let Value::Object(raw_fields) = &map["object"] else {
        return Err(ParseTypeError::grammar(format!(
            "{where_}.object · fields must be a map of name → type"
        )));
    };
    let mut fields: BTreeMap<String, Field> = BTreeMap::new();
    for (fname, fval) in raw_fields {
        // `{ optional: T }` at a FIELD position records presence —
        // the ONLY place the modifier is legal (spec 09).
        let (ty, optional) = match fval {
            Value::Object(m) if m.len() == 1 && m.contains_key("optional") => (
                parse_type(&m["optional"], names, &format!("{where_}.object.{fname}"))?,
                true,
            ),
            other => (
                parse_type(other, names, &format!("{where_}.object.{fname}"))?,
                false,
            ),
        };
        fields.insert(fname.clone(), Field::new(ty, optional));
    }
    let additional = matches!(map.get("additional"), Some(Value::Bool(true)));
    Ok(NikaType::Object { fields, additional })
}

fn num_bounds(value: &Value, where_: &str, kind: &str) -> Result<NumBounds, ParseTypeError> {
    let Value::Object(b) = value else {
        return Err(ParseTypeError::grammar(format!(
            "{where_}.{kind} · refinement must be a map (min · max)"
        )));
    };
    let mut out = NumBounds::new(None, None);
    for (k, v) in b {
        let n = v.as_f64().ok_or_else(|| {
            ParseTypeError::grammar(format!("{where_}.{kind}.{k} · not a number: {v}"))
        })?;
        match k.as_str() {
            "min" => out.min = Some(n),
            "max" => out.max = Some(n),
            other => {
                return Err(ParseTypeError::grammar(format!(
                    "{where_}.{kind}.{other} · not a numeric refinement (min · max)"
                )));
            }
        }
    }
    if let (Some(lo), Some(hi)) = (out.min, out.max)
        && lo > hi
    {
        return Err(ParseTypeError::grammar(format!(
            "{where_}.{kind} · empty range: min > max"
        )));
    }
    Ok(out)
}

/// Closest declared name within distance 2 (silence past the threshold).
fn closest<'n>(name: &str, names: &'n BTreeSet<String>) -> Option<&'n str> {
    let mut best: Option<(&str, usize)> = None;
    for cand in names {
        let d = lev(name, cand);
        if d <= 2 && best.is_none_or(|(_, bd)| d < bd) {
            best = Some((cand.as_str(), d));
        }
    }
    best.map(|(c, _)| c)
}

fn lev(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.len().abs_diff(b.len()) > 2 {
        return 9;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = alloc::vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(ca != cb);
            let ins = cur[j] + 1;
            let del = prev[j + 1] + 1;
            cur.push(sub.min(ins).min(del));
        }
        prev = cur;
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic)] // let-else in tests — the panic IS the assertion
    use super::*;
    use serde_json::json;

    fn names(list: &[&str]) -> BTreeSet<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn primitives_parse_and_bare_null_spells_null() {
        let n = names(&[]);
        assert_eq!(
            parse_type(&json!("string"), &n, "t"),
            Ok(NikaType::Prim(Primitive::String))
        );
        assert_eq!(
            parse_type(&Value::Null, &n, "t"),
            Ok(NikaType::Prim(Primitive::Null))
        );
        assert!(parse_type(&json!("boolean"), &n, "t").is_err());
    }

    #[test]
    fn optional_refused_outside_field_positions() {
        let n = names(&[]);
        let err = parse_type(&json!({"optional": "string"}), &n, "t").unwrap_err();
        assert_eq!(err.code, "NIKA-TYPE-001");
        assert!(err.detail.contains("field-presence"), "{}", err.detail);
        // …but legal AT a field position
        let t = parse_type(&json!({"object": {"b": {"optional": "string"}}}), &n, "t").unwrap();
        let NikaType::Object { fields, .. } = &t else {
            panic!()
        };
        assert!(fields["b"].optional);
        assert_eq!(fields["b"].ty, NikaType::Prim(Primitive::String));
    }

    #[test]
    fn reserved_names_teach_their_wave_and_money_is_reserved() {
        let n = names(&[]);
        for (name, needle) in [
            ("result", "W5"),
            ("artifact", "W5"),
            ("secret", "W4"),
            ("money", "W-DEC"),
        ] {
            let err = parse_type(&json!(name), &n, "t").unwrap_err();
            assert!(err.detail.contains(needle), "{name}: {}", err.detail);
        }
    }

    #[test]
    fn unknown_name_carries_did_you_mean() {
        let n = names(&["Summary"]);
        let err = parse_type(&json!("Sumary"), &n, "t").unwrap_err();
        assert!(err.detail.contains("`Summary`"), "{}", err.detail);
    }

    #[test]
    fn the_dialect_accepts_the_whitelist() {
        for pat in [
            "^abc$",
            "a|b",
            "(?:ab)+c*",
            "[a-z0-9_]{2,8}",
            r"\d+\.\d{2}",
            r"a\.b\+c",
            "x{3}",
            "x{3,}",
            "[^abc]",
        ] {
            assert_eq!(regex_dialect_violation(pat), None, "{pat}");
        }
    }

    #[test]
    fn the_dialect_refuses_out_of_set_constructs() {
        for (pat, why) in [
            (r"(a)\1", "backreference"),
            ("(?=x)a", "group construct"),
            ("(?<=x)a", "group construct"),
            ("(?P<n>a)", "group construct"),
            ("(?i)abc", "group construct"),
            ("a*?", "lazy"),
            (r"\bword\b", "word boundary"),
            (r"\p{L}+", "unicode property"),
            (r"\x41", "hex"),
            ("a{2,1", "malformed"),
            ("*a", "nothing to repeat"),
        ] {
            let v = regex_dialect_violation(pat);
            assert!(
                v.as_deref().is_some_and(|d| d.contains(why)),
                "{pat} → {v:?} (wanted {why})"
            );
        }
        assert!(regex_dialect_violation(&"x".repeat(513)).is_some());
    }

    #[test]
    fn dialect_violation_is_type_006_at_declaration() {
        let n = names(&[]);
        let err = parse_type(&json!({"string": {"pattern": "(a)\\1"}}), &n, "t").unwrap_err();
        assert_eq!(err.code, "NIKA-TYPE-006");
    }

    #[test]
    fn unions_flatten_and_enums_dedup_sorted() {
        let n = names(&[]);
        let u = parse_type(
            &json!({"union": [{"union": ["string", null]}, "integer"]}),
            &n,
            "t",
        )
        .unwrap();
        assert!(matches!(&u, NikaType::Union(ms) if ms.len() == 3));
        let e = parse_type(&json!({"enum": ["b", "a", "b"]}), &n, "t").unwrap();
        assert_eq!(
            e,
            NikaType::Enum(alloc::vec!["a".to_owned(), "b".to_owned()])
        );
    }

    #[test]
    fn name_refs_collect_deep() {
        let refs = type_name_refs(&json!({"object": {"a": "Inner", "b": {"array": "Other"}}}));
        assert!(refs.contains("Inner") && refs.contains("Other"));
    }
}
