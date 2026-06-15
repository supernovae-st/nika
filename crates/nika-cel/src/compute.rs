// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `cel-subset/0.1` computer — a typed walk of the AST over a
//! [`Resolver`], producing a `serde_json::Value`.
//!
//! Strong typing, no implicit coercion (spec §typing): `42 == "42"` is
//! `NIKA-VAR-006`, never silently `false`. The ONE exception is the
//! `null` operand (`x == null` · the defined-null test · always bool).
//! `has()` converts a lookup that would raise `NIKA-VAR-001` into a
//! bool. Short-circuit `&&`/`||` (a false/true left does not compute
//! the right).

use serde_json::Value;

use crate::ast::{Node, RelOp, Step};
use crate::error::CelError;

/// The evaluator's own recursion bound — a defense-in-depth backstop to the
/// parser's `MAX_DEPTH`. The parser already refuses an AST deeper than its
/// cap (so a normally-parsed tree never reaches this), but `compute` is a
/// PUBLIC entry that could be handed an `Expr` built by other means; a
/// crafted deep tree must surface a clean `NIKA-VAR-006` instead of
/// overflowing the native stack. Set one level ABOVE the parser cap so a
/// legitimately-parsed maximal expression always computes — only a tree
/// that bypassed the parser can trip it.
const MAX_EVAL_DEPTH: usize = 256;

/// The host's namespace lookup — resolves a ROOT identifier (`vars` ·
/// `tasks` · `item` · …) to its value. The computer owns `.field` /
/// `[index]` navigation on top. `None` ⇒ `NIKA-VAR-001`.
pub trait Resolver {
    /// Resolve a namespace root to its value (`None` = unresolved).
    fn resolve_root(&self, name: &str) -> Option<Value>;
}

/// Compute a parsed expression to a value.
///
/// # Errors
///
/// `NIKA-VAR-001` (unresolved reference) · `NIKA-VAR-006` (type error /
/// cross-type compare) per the spec typing rules.
pub fn compute(expr: &crate::Expr, resolver: &dyn Resolver) -> Result<Value, CelError> {
    eval_node(expr.node(), resolver, 0)
}

/// Compute as a boolean (the `when:` gate).
///
/// # Errors
///
/// As [`compute`], plus `NIKA-VAR-006` when the result is not a bool.
pub fn compute_bool(expr: &crate::Expr, resolver: &dyn Resolver) -> Result<bool, CelError> {
    match compute(expr, resolver)? {
        Value::Bool(b) => Ok(b),
        other => Err(CelError::type_err(
            format!(
                "`when:` must evaluate to a boolean · got {}",
                kind_of(&other)
            ),
            (0, 0),
        )),
    }
}

fn eval_node(node: &Node, res: &dyn Resolver, depth: usize) -> Result<Value, CelError> {
    // Defense-in-depth: a tree deeper than the parser would ever build
    // (e.g. a wide `||` chain that bypassed the parser) must NOT overflow
    // the native stack — it surfaces a clean NIKA-VAR-006 instead.
    if depth > MAX_EVAL_DEPTH {
        return Err(CelError::type_err(
            "expression nests too deeply to evaluate",
            (0, 0),
        ));
    }
    let depth = depth + 1;
    match node {
        Node::Lit(v) => Ok(v.clone()),
        Node::List(items) => items
            .iter()
            .map(|n| eval_node(n, res, depth))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Node::Root { name, span } => res
            .resolve_root(name)
            .ok_or_else(|| CelError::unresolved(format!("unresolved reference `{name}`"), *span)),
        Node::Postfix { base, steps } => eval_postfix(base, steps, res, depth),
        Node::Call { name, arg, span } => eval_call(name, arg, *span, res, depth),
        Node::Not(inner) => {
            let v = eval_node(inner, res, depth)?;
            as_bool(&v, (0, 0)).map(|b| Value::Bool(!b))
        }
        Node::Rel { op, lhs, rhs, span } => eval_rel(*op, lhs, rhs, *span, res, depth),
        Node::And(a, b) => {
            // Short-circuit: a false left never computes the right.
            if as_bool(&eval_node(a, res, depth)?, (0, 0))? {
                Ok(Value::Bool(as_bool(&eval_node(b, res, depth)?, (0, 0))?))
            } else {
                Ok(Value::Bool(false))
            }
        }
        Node::Or(a, b) => {
            if as_bool(&eval_node(a, res, depth)?, (0, 0))? {
                Ok(Value::Bool(true))
            } else {
                Ok(Value::Bool(as_bool(&eval_node(b, res, depth)?, (0, 0))?))
            }
        }
        Node::Ternary {
            cond,
            then,
            otherwise,
        } => {
            if as_bool(&eval_node(cond, res, depth)?, (0, 0))? {
                eval_node(then, res, depth)
            } else {
                eval_node(otherwise, res, depth)
            }
        }
    }
}

/// Navigate `.field` / `[index]` / `.method(...)` over a base value.
fn eval_postfix(
    base: &Node,
    steps: &[Step],
    res: &dyn Resolver,
    depth: usize,
) -> Result<Value, CelError> {
    let mut cur = eval_node(base, res, depth)?;
    for step in steps {
        cur = match step {
            Step::Field(name) => member(&cur, name)?,
            Step::Index(idx_node) => {
                let idx = eval_node(idx_node, res, depth)?;
                index(&cur, &idx)?
            }
            Step::Method { name, args } => eval_method(&cur, name, args, res, depth)?,
        };
    }
    Ok(cur)
}

/// `value.field` — an object member (NIKA-VAR-001 on a missing key or a
/// non-object · the defined-null exception is handled by `has()` + the
/// `== null` relation, not here).
fn member(value: &Value, name: &str) -> Result<Value, CelError> {
    match value {
        Value::Object(map) => map
            .get(name)
            .cloned()
            .ok_or_else(|| CelError::unresolved(format!("no field `{name}`"), (0, 0))),
        _ => Err(CelError::unresolved(
            format!("cannot read field `{name}` of {}", kind_of(value)),
            (0, 0),
        )),
    }
}

/// `value[index]` — a list element (int index) or an object key (string
/// index). Out of range / wrong key = NIKA-VAR-001.
fn index(value: &Value, idx: &Value) -> Result<Value, CelError> {
    match (value, idx) {
        (Value::Array(items), Value::Number(n)) => {
            let i = n
                .as_i64()
                .filter(|i| *i >= 0)
                .and_then(|i| usize::try_from(i).ok());
            i.and_then(|i| items.get(i))
                .cloned()
                .ok_or_else(|| CelError::unresolved(format!("index {n} out of range"), (0, 0)))
        }
        (Value::Object(map), Value::String(key)) => map
            .get(key)
            .cloned()
            .ok_or_else(|| CelError::unresolved(format!("no key `{key}`"), (0, 0))),
        _ => Err(CelError::type_err(
            format!("cannot index {} with {}", kind_of(value), kind_of(idx)),
            (0, 0),
        )),
    }
}

/// The closed method set: `size()` (0-arg · len) · `contains`/
/// `startsWith`/`endsWith` (1 string arg · string tests).
fn eval_method(
    recv: &Value,
    name: &str,
    args: &[Node],
    res: &dyn Resolver,
    depth: usize,
) -> Result<Value, CelError> {
    match name {
        "size" => size_of(recv).map(Value::from),
        "contains" | "startsWith" | "endsWith" => {
            let s = as_string(recv, "the receiver of a string test")?;
            let arg = eval_node(&args[0], res, depth)?;
            let needle = as_string(&arg, "the argument of a string test")?;
            let hit = match name {
                "contains" => s.contains(needle.as_str()),
                "startsWith" => s.starts_with(needle.as_str()),
                _ => s.ends_with(needle.as_str()),
            };
            Ok(Value::Bool(hit))
        }
        // The parser admits only the closed set · this arm is defensive.
        other => Err(CelError::type_err(
            format!("unknown method `.{other}()`"),
            (0, 0),
        )),
    }
}

/// `size(x)` / `has(x)` — the two free functions.
fn eval_call(
    name: &str,
    arg: &Node,
    span: (usize, usize),
    res: &dyn Resolver,
    depth: usize,
) -> Result<Value, CelError> {
    match name {
        "size" => {
            let v = eval_node(arg, res, depth)?;
            size_of(&v).map(Value::from)
        }
        "has" => {
            // has(x): true iff x resolves to a defined non-null value.
            // It NEVER raises NIKA-VAR-001 (a VAR-001 → false; a VAR-006
            // stays an error · `has` only converts the *presence* class).
            match eval_node(arg, res, depth) {
                Ok(Value::Null) => Ok(Value::Bool(false)),
                Ok(_) => Ok(Value::Bool(true)),
                Err(e) if e.spec_code() == "NIKA-VAR-001" => Ok(Value::Bool(false)),
                Err(e) => Err(e),
            }
        }
        other => Err(CelError::type_err(
            format!("unknown function `{other}()`"),
            span,
        )),
    }
}

/// A relation (`a <relop> b`) — strong typing per spec.
fn eval_rel(
    op: RelOp,
    lhs: &Node,
    rhs: &Node,
    span: (usize, usize),
    res: &dyn Resolver,
    depth: usize,
) -> Result<Value, CelError> {
    if op == RelOp::In {
        let needle = eval_node(lhs, res, depth)?;
        let haystack = eval_node(rhs, res, depth)?;
        return membership(&needle, &haystack, span).map(Value::Bool);
    }
    let a = eval_node(lhs, res, depth)?;
    let b = eval_node(rhs, res, depth)?;
    let result = match op {
        RelOp::Eq => equals(&a, &b, span)?,
        RelOp::Ne => !equals(&a, &b, span)?,
        RelOp::Lt | RelOp::Le | RelOp::Gt | RelOp::Ge => order(op, &a, &b, span)?,
        RelOp::In => unreachable!("handled above"),
    };
    Ok(Value::Bool(result))
}

/// Compare two JSON numbers on the continuous number line (CEL langdef +
/// proposal-210: an int and a double of equal magnitude are equal · the
/// defining identity `x == y` ⟺ `!(x < y || x > y)`). This is the ONE
/// source of truth shared by `==`/`!=`, `<`/`<=`/`>`/`>=`, and `in` (list
/// membership), so they can never disagree. `None` only for a
/// non-f64-representable number (exotic for JSON · treated as incomparable,
/// like NaN). Caveat: comparison is via `f64`, so two distinct integers above
/// 2^53 can compare equal (the pre-existing `order` ceiling · acceptable for
/// JSON-scale workflow values · revisit here for both ops if ever needed).
fn numeric_cmp(x: &serde_json::Number, y: &serde_json::Number) -> Option<std::cmp::Ordering> {
    x.as_f64()
        .zip(y.as_f64())
        .and_then(|(xf, yf)| xf.partial_cmp(&yf))
}

/// `==` / `!=` — cross-type is VAR-006 EXCEPT a `null` operand (the
/// defined-null test · always typed bool · spec 04).
fn equals(a: &Value, b: &Value, span: (usize, usize)) -> Result<bool, CelError> {
    if a.is_null() || b.is_null() {
        return Ok(a == b); // null == null → true · x == null → x is null
    }
    match (a, b) {
        // Numbers compare by VALUE on the continuous number line, NOT by
        // serde_json representation: `9.0 == 9` is true. This is NOT
        // cross-class coercion (`42 == "42"` below stays NIKA-VAR-006) —
        // it is the correct numeric model. serde_json::Number's PartialEq
        // distinguishes int 9 from double 9.0, which would otherwise make
        // a comparison against a jq-produced JSON float silently false.
        (Value::Number(x), Value::Number(y)) => {
            Ok(matches!(numeric_cmp(x, y), Some(std::cmp::Ordering::Equal)))
        }
        _ if same_class(a, b) => Ok(a == b),
        _ => Err(CelError::type_err(
            format!("cross-type compare: {} == {}", kind_of(a), kind_of(b)),
            span,
        )),
    }
}

/// `<` / `<=` / `>` / `>=` — only (number,number) or (string,string).
fn order(op: RelOp, a: &Value, b: &Value, span: (usize, usize)) -> Result<bool, CelError> {
    let ord = match (a, b) {
        (Value::Number(x), Value::Number(y)) => numeric_cmp(x, y),
        (Value::String(x), Value::String(y)) => Some(x.cmp(y)),
        _ => {
            return Err(CelError::type_err(
                format!(
                    "`{}` needs two numbers or two strings · got {} and {}",
                    relop_str(op),
                    kind_of(a),
                    kind_of(b)
                ),
                span,
            ));
        }
    };
    let Some(ord) = ord else {
        return Err(CelError::type_err("incomparable values (NaN?)", span));
    };
    Ok(match op {
        RelOp::Lt => ord.is_lt(),
        RelOp::Le => ord.is_le(),
        RelOp::Gt => ord.is_gt(),
        RelOp::Ge => ord.is_ge(),
        _ => unreachable!(),
    })
}

/// `x in coll` — membership in a list, or a substring in a string.
fn membership(needle: &Value, haystack: &Value, span: (usize, usize)) -> Result<bool, CelError> {
    match haystack {
        // Element-wise via `equals`, NOT `Vec::contains` (serde repr equality),
        // so membership obeys the same continuous-number-line equality as `==`:
        // `3.0 in [1, 2, 3]` is true. A type-mismatched element is simply not
        // the needle (`equals` → VAR-006 → `false` for that element), never an
        // error — `1 in ['a', 'b']` is false, as it is for `==`.
        Value::Array(items) => Ok(items
            .iter()
            .any(|elem| equals(elem, needle, span).unwrap_or(false))),
        Value::String(s) => match needle {
            Value::String(sub) => Ok(s.contains(sub.as_str())),
            _ => Err(CelError::type_err(
                format!(
                    "`in` on a string needs a string left operand · got {}",
                    kind_of(needle)
                ),
                span,
            )),
        },
        _ => Err(CelError::type_err(
            format!(
                "`in` needs a list or string right operand · got {}",
                kind_of(haystack)
            ),
            span,
        )),
    }
}

/// `size()` — collection/string length. VAR-006 on a scalar.
fn size_of(v: &Value) -> Result<i64, CelError> {
    let n = match v {
        Value::String(s) => s.chars().count(),
        Value::Array(a) => a.len(),
        Value::Object(m) => m.len(),
        _ => {
            return Err(CelError::type_err(
                format!("size() needs a string/list/map · got {}", kind_of(v)),
                (0, 0),
            ));
        }
    };
    Ok(i64::try_from(n).unwrap_or(i64::MAX))
}

fn as_bool(v: &Value, span: (usize, usize)) -> Result<bool, CelError> {
    match v {
        Value::Bool(b) => Ok(*b),
        other => Err(CelError::type_err(
            format!("expected a boolean · got {}", kind_of(other)),
            span,
        )),
    }
}

fn as_string<'a>(v: &'a Value, what: &str) -> Result<&'a String, CelError> {
    match v {
        Value::String(s) => Ok(s),
        other => Err(CelError::type_err(
            format!("{what} must be a string · got {}", kind_of(other)),
            (0, 0),
        )),
    }
}

/// Two values share a comparison class (both numbers · both strings ·
/// both bools · both lists · both maps).
fn same_class(a: &Value, b: &Value) -> bool {
    matches!(
        (a, b),
        (Value::Number(_), Value::Number(_))
            | (Value::String(_), Value::String(_))
            | (Value::Bool(_), Value::Bool(_))
            | (Value::Array(_), Value::Array(_))
            | (Value::Object(_), Value::Object(_))
    )
}

fn kind_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "list",
        Value::Object(_) => "map",
    }
}

const fn relop_str(op: RelOp) -> &'static str {
    match op {
        RelOp::Eq => "==",
        RelOp::Ne => "!=",
        RelOp::Lt => "<",
        RelOp::Le => "<=",
        RelOp::Gt => ">",
        RelOp::Ge => ">=",
        RelOp::In => "in",
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::parse;
    use serde_json::json;

    /// A resolver over a fixed namespace map.
    struct Ns(Value);
    impl Resolver for Ns {
        fn resolve_root(&self, name: &str) -> Option<Value> {
            self.0.get(name).cloned()
        }
    }

    fn ns() -> Ns {
        Ns(json!({
            "vars": { "publish": "yes", "count": 3, "tags": ["a", "b"], "empty": [], "ratio": 0.5 },
            "tasks": { "build": { "status": "success", "output": null } },
            "item": "elem",
            "index": 2,
        }))
    }

    fn b(src: &str) -> bool {
        compute_bool(&parse(src).unwrap(), &ns()).unwrap()
    }

    #[test]
    fn equality_and_inequality() {
        assert!(b("vars.publish == 'yes'"));
        assert!(b("vars.publish != 'no'"));
        assert!(b("vars.count == 3"));
    }

    #[test]
    fn numeric_equality_is_value_based_continuous_line() {
        // CEL langdef + proposal-210: numbers compare on a continuous number
        // line — `x == y` iff `!(x < y || x > y)`. An int and a double of equal
        // value are EQUAL (crucial for jq-produced JSON floats vs int literals;
        // serde_json::Number's PartialEq would otherwise make `vars.count == 3.0`
        // false because it distinguishes the int/float representation).
        assert!(b("vars.count == 3.0")); // int 3 == double 3.0
        assert!(!b("vars.count != 3.0"));
        assert!(b("vars.ratio == 0.5")); // same-type float unaffected
        // equality stays consistent with ordering (the spec's defining identity)
        assert!(!b("vars.count < 3.0"));
        assert!(!b("vars.count > 3.0"));
        // membership obeys the same numeric equality (`in` ⟺ `==` per element)
        assert!(b("3.0 in [1, 2, 3]")); // int list, float needle
        assert!(b("3 in [1, 2, 3.0]")); // float list, int needle
        assert!(!b("9 in [1, 2, 3]")); // true negative, unchanged
    }

    #[test]
    fn cross_type_compare_is_var_006_not_false() {
        let err = compute_bool(&parse("vars.count == 'three'").unwrap(), &ns()).unwrap_err();
        assert_eq!(err.spec_code(), "NIKA-VAR-006");
    }

    #[test]
    fn defined_null_is_the_one_cross_type_exception() {
        // tasks.build.output is null → `== null` is true, typed bool.
        assert!(b("tasks.build.output == null"));
        assert!(!b("vars.publish == null"));
        assert!(b("vars.publish != null"));
    }

    #[test]
    fn ordering_numbers_and_strings_only() {
        assert!(b("vars.count > 0"));
        assert!(b("vars.ratio < 1.0"));
        assert!(b("vars.publish < 'z'"));
        let err = compute_bool(&parse("vars.count > 'x'").unwrap(), &ns()).unwrap_err();
        assert_eq!(err.spec_code(), "NIKA-VAR-006");
    }

    #[test]
    fn boolean_operators_short_circuit() {
        assert!(b("vars.count == 3 && vars.publish == 'yes'"));
        assert!(!b("vars.count == 9 && vars.publish == 'yes'"));
        assert!(b("vars.count == 9 || vars.publish == 'yes'"));
        assert!(b("!(vars.count == 9)"));
    }

    #[test]
    fn membership_in_list_and_string() {
        assert!(b("'a' in vars.tags"));
        assert!(!b("'z' in vars.tags"));
        assert!(b("'ye' in vars.publish"));
        assert!(b("tasks.build.status in ['success', 'skipped']"));
    }

    #[test]
    fn size_and_the_empty_check_idiom() {
        assert!(b("size(vars.tags) > 0"));
        assert!(!b("size(vars.empty) > 0"));
        assert!(b("vars.publish.size() == 3"));
        let err = compute_bool(&parse("size(vars.count) > 0").unwrap(), &ns()).unwrap_err();
        assert_eq!(
            err.spec_code(),
            "NIKA-VAR-006",
            "size of a scalar is a type error"
        );
    }

    #[test]
    fn has_never_raises_unresolved() {
        assert!(b("has(vars.publish)"));
        assert!(!b("has(vars.missing)"));
        assert!(
            !b("has(tasks.build.output)"),
            "a null field is absent for has()"
        );
        assert!(
            !b("has(nonexistent_root)"),
            "an unknown root is absent, not an error"
        );
    }

    #[test]
    fn string_methods() {
        assert!(b("vars.publish.contains('e')"));
        assert!(b("vars.publish.startsWith('ye')"));
        assert!(b("vars.publish.endsWith('es')"));
    }

    #[test]
    fn ternary_selects_a_value() {
        let v = compute(
            &parse("vars.count > 0 ? 'positive' : 'zero'").unwrap(),
            &ns(),
        )
        .unwrap();
        assert_eq!(v, json!("positive"));
        // The branches need NOT share a type (value selection).
        let v = compute(&parse("vars.count > 9 ? 1 : 'fallback'").unwrap(), &ns()).unwrap();
        assert_eq!(v, json!("fallback"));
    }

    #[test]
    fn unresolved_root_and_field_are_var_001() {
        assert_eq!(
            compute(&parse("ghost.x").unwrap(), &ns())
                .unwrap_err()
                .spec_code(),
            "NIKA-VAR-001"
        );
        assert_eq!(
            compute(&parse("vars.nope").unwrap(), &ns())
                .unwrap_err()
                .spec_code(),
            "NIKA-VAR-001"
        );
    }

    #[test]
    fn for_each_locals_resolve() {
        assert!(b("item == 'elem'"));
        assert!(b("index == 2"));
    }

    #[test]
    fn index_access() {
        assert!(b("vars.tags[0] == 'a'"));
        assert!(b("vars.tags[1] == 'b'"));
        assert_eq!(
            compute(&parse("vars.tags[9]").unwrap(), &ns())
                .unwrap_err()
                .spec_code(),
            "NIKA-VAR-001"
        );
    }

    #[test]
    fn non_boolean_when_is_var_006() {
        let err = compute_bool(&parse("vars.count").unwrap(), &ns()).unwrap_err();
        assert_eq!(err.spec_code(), "NIKA-VAR-006");
    }

    #[test]
    fn type_error_messages_name_the_offending_types() {
        // `kind_of` feeds these messages · a `kind_of -> "xyzzy"` mutant
        // replaces the real type names, so assert them verbatim.
        let err = compute_bool(&parse("vars.count == 'three'").unwrap(), &ns()).unwrap_err();
        let msg = err.message();
        assert!(
            msg.contains("number") && msg.contains("string"),
            "cross-type names both sides · got: {msg}"
        );
    }

    #[test]
    fn order_error_message_names_the_operator() {
        // `relop_str` feeds the order() type error · a `-> ""` mutant
        // blanks the operator symbol.
        let err = compute_bool(&parse("vars.count > 'x'").unwrap(), &ns()).unwrap_err();
        assert!(
            err.message().contains('>'),
            "names the `>` op · got: {}",
            err.message()
        );
    }

    #[test]
    fn string_methods_are_not_trivially_true() {
        // `as_string` returns the REAL receiver/argument · an `Ok(empty)`
        // mutant collapses every test to `"".op("")` → true, so the FALSE
        // cases are what pin it.
        assert!(!b("vars.publish.contains('zzz')"));
        assert!(!b("vars.publish.startsWith('no')"));
        assert!(!b("vars.publish.endsWith('xx')"));
    }

    #[test]
    fn bare_literals_compute_to_their_values() {
        assert_eq!(
            compute(&parse("true").unwrap(), &ns()).unwrap(),
            json!(true)
        );
        assert_eq!(
            compute(&parse("false").unwrap(), &ns()).unwrap(),
            json!(false)
        );
        assert_eq!(
            compute(&parse("null").unwrap(), &ns()).unwrap(),
            json!(null)
        );
    }

    #[test]
    fn object_index_by_key_and_size_of_a_map() {
        // index's (Object, String) arm + size_of's Object arm — both
        // deleted by mutants, both invisible to the list-only index tests.
        assert!(b("tasks['build'].status == 'success'"));
        assert!(b("size(tasks) == 1"));
        assert!(b("size(vars) == 5"));
    }

    #[test]
    fn has_swallows_unresolved_but_propagates_type_errors() {
        // The has() guard is `== NIKA-VAR-001`, NOT "any error": a VAR-006
        // (type error) inside has() must still surface, never collapse to
        // false. `size()` of a scalar is the VAR-006 trigger.
        let err = compute_bool(&parse("has(size(vars.count))").unwrap(), &ns()).unwrap_err();
        assert_eq!(err.spec_code(), "NIKA-VAR-006");
        // …while an absent reference stays the swallowed false.
        assert!(!b("has(vars.missing)"));
    }

    #[test]
    fn the_evaluator_refuses_a_too_deep_tree_instead_of_overflowing() {
        // Defense-in-depth (BUG#1b): the parser caps AST depth, but `compute`
        // is a PUBLIC entry that could receive a tree built by other means.
        // A deep left-nested `Or` chain (the wide-`||` AST shape) must
        // surface a clean NIKA-VAR-006, NEVER overflow the native stack.
        // Build it DIRECTLY past MAX_EVAL_DEPTH (bypassing the parser cap).
        let mut node = Node::Lit(Value::Bool(true));
        for _ in 0..(super::MAX_EVAL_DEPTH + 50) {
            node = Node::Or(Box::new(node), Box::new(Node::Lit(Value::Bool(false))));
        }
        let expr = crate::ast::Expr::new(node);
        // (the test module allows unwrap_used, not expect_used)
        let err = compute(&expr, &ns()).unwrap_err();
        assert_eq!(
            err.spec_code(),
            "NIKA-VAR-006",
            "the eval-depth backstop is the type/eval class"
        );
    }

    #[test]
    fn a_normal_width_chain_evaluates_fine() {
        // The flip side: a chain WITHIN the cap computes correctly (the
        // depth guard must not reject legitimate expressions). A 20-term
        // `||` of equalities short-circuits to true on the first match.
        let chain = std::iter::repeat_n("vars.publish == 'yes'", 20)
            .collect::<Vec<_>>()
            .join(" || ");
        assert!(b(&chain), "a 20-term || chain evaluates");
        // …and a deep-but-within-limit nested group computes too.
        let nested = format!("{}vars.count > 0{}", "(".repeat(40), ")".repeat(40));
        assert!(b(&nested), "40-deep grouping is within the cap");
    }
}
