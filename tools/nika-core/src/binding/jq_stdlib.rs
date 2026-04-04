//! jq standard library for jaq-interpret 1.5.0
//!
//! Registers native builtins and pure jq definitions into a `ParseCtx`,
//! giving Nika's `| jq(...)` transform access to the essential jq
//! vocabulary: `length`, `keys`, `map`, `select`, `sort_by`, etc.
//!
//! # Design
//!
//! jaq-interpret 1.5.0 ships with **zero** builtins -- `ParseCtx::new()`
//! only understands the core filter syntax (`.`, `|`, `,`, `if/then/else`,
//! `reduce`, `try/catch`, path expressions, arithmetic, and comparison).
//!
//! To make `| jq(...)` useful we register:
//! - ~30 native functions via `insert_native` (implemented as Rust `fn` pointers)
//! - ~15 pure jq definitions via `insert_defs` (parsed from jq source strings)
//!
//! This is intentionally a self-contained subset -- no math, time, regex-flags,
//! I/O, or format builtins. Just the data-manipulation core.

use jaq_interpret::{Args, Ctx, Error, FilterT, Native, ParseCtx, RunPtr, Val, ValT};
use std::rc::Rc;

// ── Type aliases matching jaq-interpret internals ────────────────────

/// `(Ctx, Val)` -- the context + current value pair passed to every filter.
type Cv<'a> = (Ctx<'a, Val>, Val);

/// Boxed iterator of `Result<Val, Error>` -- the output stream of a filter.
type ValR2s<'a> = Box<dyn Iterator<Item = Result<Val, Error>> + 'a>;

// ── Helpers ─────────────────────────────────────────────────────────

/// Return a boxed iterator yielding exactly one value.
#[inline]
fn box_once<'a>(v: Result<Val, Error>) -> ValR2s<'a> {
    Box::new(core::iter::once(v))
}

/// Return an empty boxed iterator (the jq `empty` builtin).
#[inline]
fn box_empty<'a>() -> ValR2s<'a> {
    Box::new(core::iter::empty())
}

/// Construct a `Val::Str` from a Rust string.
#[inline]
fn val_str(s: String) -> Val {
    Val::Str(Rc::new(s))
}

/// Construct a `Val::Arr` from a `Vec<Val>`.
#[inline]
fn val_arr(v: Vec<Val>) -> Val {
    Val::Arr(Rc::new(v))
}

// ── Public entry point ──────────────────────────────────────────────

/// Register the essential jq standard library into a `ParseCtx`.
///
/// Call this after `ParseCtx::new()` and before `ctx.compile()`.
pub fn register_stdlib(ctx: &mut ParseCtx) {
    // ── Native functions ────────────────────────────────────────
    for (name, arity, run) in native_fns() {
        ctx.insert_native(name.to_string(), arity, Native::new(run));
    }

    // ── Pure jq definitions ─────────────────────────────────────
    let defs_src = JQ_DEFS;
    let (parsed, errs) = jaq_parse::parse(defs_src, jaq_parse::defs());
    debug_assert!(
        errs.is_empty(),
        "jq_stdlib: parse errors in builtin defs: {errs:?}"
    );
    if let Some(defs) = parsed {
        ctx.insert_defs(defs);
    }
}

// ── Native function table ───────────────────────────────────────────

fn native_fns() -> Vec<(&'static str, usize, RunPtr)> {
    vec![
        // ── arity 0 ─────────────────────────────────────────────
        ("length", 0, native_length as RunPtr),
        ("keys", 0, native_keys as RunPtr),
        ("values", 0, native_values as RunPtr),
        ("type", 0, native_type as RunPtr),
        ("empty", 0, native_empty as RunPtr),
        ("not", 0, native_not as RunPtr),
        ("add", 0, native_add as RunPtr),
        ("flatten", 0, native_flatten as RunPtr),
        ("to_entries", 0, native_to_entries as RunPtr),
        ("from_entries", 0, native_from_entries as RunPtr),
        ("tostring", 0, native_tostring as RunPtr),
        ("tonumber", 0, native_tonumber as RunPtr),
        ("ascii_downcase", 0, native_ascii_downcase as RunPtr),
        ("ascii_upcase", 0, native_ascii_upcase as RunPtr),
        ("reverse", 0, native_reverse as RunPtr),
        ("sort", 0, native_sort as RunPtr),
        ("unique", 0, native_unique as RunPtr),
        ("min", 0, native_min as RunPtr),
        ("max", 0, native_max as RunPtr),
        ("floor", 0, native_floor as RunPtr),
        ("ceil", 0, native_ceil as RunPtr),
        ("round", 0, native_round as RunPtr),
        ("null", 0, native_null as RunPtr),
        ("true", 0, native_true as RunPtr),
        ("false", 0, native_false as RunPtr),
        ("nan", 0, native_nan as RunPtr),
        ("infinite", 0, native_infinite as RunPtr),
        ("error", 0, native_error as RunPtr),
        ("abs", 0, native_abs as RunPtr),
        // ── arity 1 (filter argument) ───────────────────────────
        ("has", 1, native_has as RunPtr),
        ("contains", 1, native_contains as RunPtr),
        ("startswith", 1, native_startswith as RunPtr),
        ("endswith", 1, native_endswith as RunPtr),
        ("ltrimstr", 1, native_ltrimstr as RunPtr),
        ("rtrimstr", 1, native_rtrimstr as RunPtr),
        ("split", 1, native_split as RunPtr),
        ("join", 1, native_join as RunPtr),
        ("test", 1, native_test as RunPtr),
        ("sort_by", 1, native_sort_by as RunPtr),
        ("group_by", 1, native_group_by as RunPtr),
        ("unique_by", 1, native_unique_by as RunPtr),
        ("min_by", 1, native_min_by as RunPtr),
        ("max_by", 1, native_max_by as RunPtr),
        ("first", 1, native_first as RunPtr),
        ("last", 1, native_last as RunPtr),
        // ── arity 2 / 3 ────────────────────────────────────────
        ("limit", 2, native_limit as RunPtr),
        ("range", 3, native_range as RunPtr),
    ]
}

// ════════════════════════════════════════════════════════════════════
//  ARITY-0 NATIVES
// ════════════════════════════════════════════════════════════════════

fn native_length<'a>(_args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let v = cv.1;
    let result = match &v {
        Val::Null => Ok(Val::Int(0)),
        Val::Bool(_) => Err(Error::Type(v, jaq_interpret::error::Type::Iter)),
        Val::Int(i) => Ok(Val::Int(i.unsigned_abs() as isize)),
        Val::Float(f) => Ok(Val::Float(f.abs())),
        Val::Num(n) => match n.parse::<f64>() {
            Ok(f) => Ok(Val::Float(f.abs())),
            Err(_) => Err(Error::Type(v, jaq_interpret::error::Type::Num)),
        },
        Val::Str(s) => Ok(Val::Int(s.chars().count() as isize)),
        Val::Arr(a) => Ok(Val::Int(a.len() as isize)),
        Val::Obj(o) => Ok(Val::Int(o.len() as isize)),
    };
    box_once(result)
}

fn native_keys<'a>(_args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let v = cv.1;
    let result = match &v {
        Val::Arr(a) => Ok(val_arr((0..a.len() as isize).map(Val::Int).collect())),
        Val::Obj(o) => {
            let mut keys: Vec<Val> = o.keys().map(|k| Val::Str(Rc::clone(k))).collect();
            keys.sort();
            Ok(val_arr(keys))
        }
        _ => Err(Error::Type(v, jaq_interpret::error::Type::Iter)),
    };
    box_once(result)
}

fn native_values<'a>(_args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let v = cv.1;
    match &v {
        Val::Arr(a) => box_once(Ok(val_arr(a.iter().cloned().collect()))),
        Val::Obj(o) => box_once(Ok(val_arr(o.values().cloned().collect()))),
        _ => box_once(Err(Error::Type(v, jaq_interpret::error::Type::Iter))),
    }
}

fn native_type<'a>(_args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let name = match &cv.1 {
        Val::Null => "null",
        Val::Bool(_) => "boolean",
        Val::Int(_) | Val::Float(_) | Val::Num(_) => "number",
        Val::Str(_) => "string",
        Val::Arr(_) => "array",
        Val::Obj(_) => "object",
    };
    box_once(Ok(val_str(name.to_string())))
}

fn native_empty<'a>(_args: Args<'a, Val>, _cv: Cv<'a>) -> ValR2s<'a> {
    box_empty()
}

fn native_not<'a>(_args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    box_once(Ok(Val::Bool(!cv.1.as_bool())))
}

fn native_add<'a>(_args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let result = match cv.1 {
        Val::Arr(a) => {
            let mut iter = a.iter().cloned();
            match iter.next() {
                None => Ok(Val::Null),
                Some(first) => iter.try_fold(first, |acc, v| acc + v),
            }
        }
        v => Err(Error::Type(v, jaq_interpret::error::Type::Arr)),
    };
    box_once(result)
}

fn native_flatten<'a>(_args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let result = match cv.1 {
        Val::Arr(a) => {
            let mut out = Vec::new();
            for item in a.iter() {
                match item {
                    Val::Arr(inner) => out.extend(inner.iter().cloned()),
                    other => out.push(other.clone()),
                }
            }
            Ok(val_arr(out))
        }
        v => Err(Error::Type(v, jaq_interpret::error::Type::Arr)),
    };
    box_once(result)
}

fn native_to_entries<'a>(_args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let result = match cv.1 {
        Val::Obj(o) => {
            let entries: Result<Vec<Val>, _> = o
                .iter()
                .map(|(k, v)| {
                    Val::from_map([
                        (val_str("key".to_string()), Val::Str(Rc::clone(k))),
                        (val_str("value".to_string()), v.clone()),
                    ])
                })
                .collect();
            entries.map(val_arr)
        }
        v => Err(Error::Type(v, jaq_interpret::error::Type::Iter)),
    };
    box_once(result)
}

fn native_from_entries<'a>(_args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let result = match cv.1 {
        Val::Arr(entries) => {
            let mut pairs: Vec<(Val, Val)> = Vec::new();
            for entry in entries.iter() {
                if let Val::Obj(o) = entry {
                    let key_str = Rc::new("key".to_string());
                    let name_str = Rc::new("name".to_string());
                    let value_str = Rc::new("value".to_string());
                    let key_val = o
                        .get(&key_str)
                        .or_else(|| o.get(&name_str))
                        .cloned()
                        .unwrap_or(Val::Null);
                    let val = o.get(&value_str).cloned().unwrap_or(Val::Null);
                    pairs.push((key_val, val));
                }
            }
            Val::from_map(pairs)
        }
        v => Err(Error::Type(v, jaq_interpret::error::Type::Arr)),
    };
    box_once(result)
}

fn native_tostring<'a>(_args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let result = match &cv.1 {
        Val::Str(_) => Ok(cv.1),
        _ => Ok(val_str(cv.1.to_string())),
    };
    box_once(result)
}

fn native_tonumber<'a>(_args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let result = match &cv.1 {
        Val::Int(_) | Val::Float(_) | Val::Num(_) => Ok(cv.1),
        Val::Str(s) => {
            if let Ok(i) = s.parse::<isize>() {
                Ok(Val::Int(i))
            } else if let Ok(f) = s.parse::<f64>() {
                Ok(Val::Float(f))
            } else {
                Err(Error::Type(cv.1, jaq_interpret::error::Type::Num))
            }
        }
        _ => Err(Error::Type(cv.1, jaq_interpret::error::Type::Num)),
    };
    box_once(result)
}

fn native_ascii_downcase<'a>(_args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    box_once(cv.1.mutate_str(|s| *s = s.to_ascii_lowercase()))
}

fn native_ascii_upcase<'a>(_args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    box_once(cv.1.mutate_str(|s| *s = s.to_ascii_uppercase()))
}

fn native_reverse<'a>(_args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    box_once(cv.1.mutate_arr(|a| a.reverse()))
}

fn native_sort<'a>(_args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    box_once(cv.1.mutate_arr(|a| a.sort()))
}

fn native_unique<'a>(_args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    box_once(cv.1.mutate_arr(|a| {
        a.sort();
        a.dedup();
    }))
}

fn native_min<'a>(_args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let result = match cv.1 {
        Val::Arr(a) if a.is_empty() => Ok(Val::Null),
        Val::Arr(a) => Ok(a.iter().min().cloned().unwrap_or(Val::Null)),
        v => Err(Error::Type(v, jaq_interpret::error::Type::Arr)),
    };
    box_once(result)
}

fn native_max<'a>(_args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let result = match cv.1 {
        Val::Arr(a) if a.is_empty() => Ok(Val::Null),
        Val::Arr(a) => Ok(a.iter().max().cloned().unwrap_or(Val::Null)),
        v => Err(Error::Type(v, jaq_interpret::error::Type::Arr)),
    };
    box_once(result)
}

fn native_floor<'a>(_args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    box_once(cv.1.round(f64::floor))
}

fn native_ceil<'a>(_args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    box_once(cv.1.round(f64::ceil))
}

fn native_round<'a>(_args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    box_once(cv.1.round(f64::round))
}

fn native_null<'a>(_args: Args<'a, Val>, _cv: Cv<'a>) -> ValR2s<'a> {
    box_once(Ok(Val::Null))
}

fn native_true<'a>(_args: Args<'a, Val>, _cv: Cv<'a>) -> ValR2s<'a> {
    box_once(Ok(Val::Bool(true)))
}

fn native_false<'a>(_args: Args<'a, Val>, _cv: Cv<'a>) -> ValR2s<'a> {
    box_once(Ok(Val::Bool(false)))
}

fn native_nan<'a>(_args: Args<'a, Val>, _cv: Cv<'a>) -> ValR2s<'a> {
    box_once(Ok(Val::Float(f64::NAN)))
}

fn native_infinite<'a>(_args: Args<'a, Val>, _cv: Cv<'a>) -> ValR2s<'a> {
    box_once(Ok(Val::Float(f64::INFINITY)))
}

fn native_error<'a>(_args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    box_once(Err(Error::Val(cv.1)))
}

fn native_abs<'a>(_args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let result = match cv.1 {
        Val::Int(i) => Ok(Val::Int(i.abs())),
        Val::Float(f) => Ok(Val::Float(f.abs())),
        Val::Num(ref n) => match n.parse::<f64>() {
            Ok(f) => Ok(Val::Float(f.abs())),
            Err(_) => Err(Error::Type(cv.1, jaq_interpret::error::Type::Num)),
        },
        v => Err(Error::Type(v, jaq_interpret::error::Type::Num)),
    };
    box_once(result)
}

// ════════════════════════════════════════════════════════════════════
//  ARITY-1 NATIVES (filter argument)
// ════════════════════════════════════════════════════════════════════

fn native_has<'a>(args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let f = args.get(0);
    let ctx = cv.0.clone();
    let v = cv.1;
    Box::new(f.run((ctx, v.clone())).map(move |key_result| {
        let key = key_result?;
        v.has(&key).map(Val::Bool)
    }))
}

fn native_contains<'a>(args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let f = args.get(0);
    let ctx = cv.0.clone();
    let v = cv.1;
    Box::new(f.run((ctx, v.clone())).map(move |other_result| {
        let other = other_result?;
        Ok(Val::Bool(v.contains(&other)))
    }))
}

fn native_startswith<'a>(args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let f = args.get(0);
    let ctx = cv.0.clone();
    let v = cv.1;
    Box::new(f.run((ctx, v.clone())).map(move |prefix_result| {
        let prefix = prefix_result?;
        match (&v, &prefix) {
            (Val::Str(s), Val::Str(p)) => Ok(Val::Bool(s.starts_with(&**p))),
            _ => Err(Error::Type(v.clone(), jaq_interpret::error::Type::Str)),
        }
    }))
}

fn native_endswith<'a>(args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let f = args.get(0);
    let ctx = cv.0.clone();
    let v = cv.1;
    Box::new(f.run((ctx, v.clone())).map(move |suffix_result| {
        let suffix = suffix_result?;
        match (&v, &suffix) {
            (Val::Str(s), Val::Str(p)) => Ok(Val::Bool(s.ends_with(&**p))),
            _ => Err(Error::Type(v.clone(), jaq_interpret::error::Type::Str)),
        }
    }))
}

fn native_ltrimstr<'a>(args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let f = args.get(0);
    let ctx = cv.0.clone();
    let v = cv.1;
    Box::new(f.run((ctx, v.clone())).map(move |prefix_result| {
        let prefix = prefix_result?;
        match (&v, &prefix) {
            (Val::Str(s), Val::Str(p)) => match s.strip_prefix(&**p) {
                Some(stripped) => Ok(val_str(stripped.to_string())),
                None => Ok(v.clone()),
            },
            _ => Err(Error::Type(v.clone(), jaq_interpret::error::Type::Str)),
        }
    }))
}

fn native_rtrimstr<'a>(args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let f = args.get(0);
    let ctx = cv.0.clone();
    let v = cv.1;
    Box::new(f.run((ctx, v.clone())).map(move |suffix_result| {
        let suffix = suffix_result?;
        match (&v, &suffix) {
            (Val::Str(s), Val::Str(p)) => match s.strip_suffix(&**p) {
                Some(stripped) => Ok(val_str(stripped.to_string())),
                None => Ok(v.clone()),
            },
            _ => Err(Error::Type(v.clone(), jaq_interpret::error::Type::Str)),
        }
    }))
}

fn native_split<'a>(args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let f = args.get(0);
    let ctx = cv.0.clone();
    let v = cv.1;
    Box::new(f.run((ctx, v.clone())).map(move |sep_result| {
        let sep = sep_result?;
        match (&v, &sep) {
            (Val::Str(s), Val::Str(p)) => {
                let parts: Vec<Val> = s.split(&**p).map(|part| val_str(part.to_string())).collect();
                Ok(val_arr(parts))
            }
            _ => Err(Error::Type(v.clone(), jaq_interpret::error::Type::Str)),
        }
    }))
}

fn native_join<'a>(args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let f = args.get(0);
    let ctx = cv.0.clone();
    let v = cv.1;
    Box::new(f.run((ctx, v.clone())).map(move |sep_result| {
        let sep = sep_result?;
        match (&v, &sep) {
            (Val::Arr(a), Val::Str(s)) => {
                let parts: Vec<String> = a
                    .iter()
                    .map(|item| match item {
                        Val::Str(s) => (**s).clone(),
                        Val::Null => String::new(),
                        other => other.to_string(),
                    })
                    .collect();
                Ok(val_str(parts.join(&**s)))
            }
            _ => Err(Error::Type(v.clone(), jaq_interpret::error::Type::Arr)),
        }
    }))
}

fn native_test<'a>(args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let f = args.get(0);
    let ctx = cv.0.clone();
    let v = cv.1;
    Box::new(f.run((ctx, v.clone())).map(move |re_result| {
        let re = re_result?;
        match (&v, &re) {
            (Val::Str(s), Val::Str(pattern)) => match regex::Regex::new(pattern) {
                Ok(re) => Ok(Val::Bool(re.is_match(s))),
                Err(e) => Err(Error::Val(val_str(format!("invalid regex: {e}")))),
            },
            _ => Err(Error::Type(v.clone(), jaq_interpret::error::Type::Str)),
        }
    }))
}

fn native_sort_by<'a>(args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let f = args.get(0);
    let ctx = cv.0.clone();
    let v = cv.1;
    match v {
        Val::Arr(a) => {
            let mut indexed: Vec<(Vec<Val>, Val)> = Vec::with_capacity(a.len());
            for item in a.iter() {
                let keys: Result<Vec<Val>, _> =
                    f.clone().run((ctx.clone(), item.clone())).collect();
                match keys {
                    Ok(k) => indexed.push((k, item.clone())),
                    Err(e) => return box_once(Err(e)),
                }
            }
            indexed.sort_by(|(a_keys, _), (b_keys, _)| a_keys.cmp(b_keys));
            let sorted: Vec<Val> = indexed.into_iter().map(|(_, v)| v).collect();
            box_once(Ok(val_arr(sorted)))
        }
        _ => box_once(Err(Error::Type(v, jaq_interpret::error::Type::Arr))),
    }
}

fn native_group_by<'a>(args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let f = args.get(0);
    let ctx = cv.0.clone();
    let v = cv.1;
    match v {
        Val::Arr(a) => {
            let mut keyed: Vec<(Vec<Val>, Val)> = Vec::with_capacity(a.len());
            for item in a.iter() {
                let keys: Result<Vec<Val>, _> =
                    f.clone().run((ctx.clone(), item.clone())).collect();
                match keys {
                    Ok(k) => keyed.push((k, item.clone())),
                    Err(e) => return box_once(Err(e)),
                }
            }
            keyed.sort_by(|(a, _), (b, _)| a.cmp(b));
            let mut groups: Vec<Val> = Vec::new();
            let mut iter = keyed.into_iter();
            if let Some((mut cur_key, first)) = iter.next() {
                let mut group = vec![first];
                for (k, v) in iter {
                    if k == cur_key {
                        group.push(v);
                    } else {
                        groups.push(val_arr(std::mem::take(&mut group)));
                        cur_key = k;
                        group.push(v);
                    }
                }
                if !group.is_empty() {
                    groups.push(val_arr(group));
                }
            }
            box_once(Ok(val_arr(groups)))
        }
        _ => box_once(Err(Error::Type(v, jaq_interpret::error::Type::Arr))),
    }
}

fn native_unique_by<'a>(args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let f = args.get(0);
    let ctx = cv.0.clone();
    let v = cv.1;
    match v {
        Val::Arr(a) => {
            let mut keyed: Vec<(Vec<Val>, Val)> = Vec::with_capacity(a.len());
            for item in a.iter() {
                let keys: Result<Vec<Val>, _> =
                    f.clone().run((ctx.clone(), item.clone())).collect();
                match keys {
                    Ok(k) => keyed.push((k, item.clone())),
                    Err(e) => return box_once(Err(e)),
                }
            }
            keyed.sort_by(|(a, _), (b, _)| a.cmp(b));
            keyed.dedup_by(|(a, _), (b, _)| a == b);
            let result: Vec<Val> = keyed.into_iter().map(|(_, v)| v).collect();
            box_once(Ok(val_arr(result)))
        }
        _ => box_once(Err(Error::Type(v, jaq_interpret::error::Type::Arr))),
    }
}

fn native_min_by<'a>(args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let f = args.get(0);
    let ctx = cv.0.clone();
    match cv.1 {
        Val::Arr(a) if a.is_empty() => box_once(Ok(Val::Null)),
        Val::Arr(a) => {
            let mut best_val = a[0].clone();
            let mut best_key: Vec<Val> =
                match f.clone().run((ctx.clone(), best_val.clone())).collect() {
                    Ok(k) => k,
                    Err(e) => return box_once(Err(e)),
                };
            for item in a.iter().skip(1) {
                let key: Result<Vec<Val>, _> =
                    f.clone().run((ctx.clone(), item.clone())).collect();
                match key {
                    Ok(k) if k < best_key => {
                        best_key = k;
                        best_val = item.clone();
                    }
                    Ok(_) => {}
                    Err(e) => return box_once(Err(e)),
                }
            }
            box_once(Ok(best_val))
        }
        v => box_once(Err(Error::Type(v, jaq_interpret::error::Type::Arr))),
    }
}

fn native_max_by<'a>(args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let f = args.get(0);
    let ctx = cv.0.clone();
    match cv.1 {
        Val::Arr(a) if a.is_empty() => box_once(Ok(Val::Null)),
        Val::Arr(a) => {
            let mut best_val = a[0].clone();
            let mut best_key: Vec<Val> =
                match f.clone().run((ctx.clone(), best_val.clone())).collect() {
                    Ok(k) => k,
                    Err(e) => return box_once(Err(e)),
                };
            for item in a.iter().skip(1) {
                let key: Result<Vec<Val>, _> =
                    f.clone().run((ctx.clone(), item.clone())).collect();
                match key {
                    Ok(k) if k >= best_key => {
                        best_key = k;
                        best_val = item.clone();
                    }
                    Ok(_) => {}
                    Err(e) => return box_once(Err(e)),
                }
            }
            box_once(Ok(best_val))
        }
        v => box_once(Err(Error::Type(v, jaq_interpret::error::Type::Arr))),
    }
}

fn native_first<'a>(args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let f = args.get(0);
    match f.run(cv).next() {
        Some(v) => box_once(v),
        None => box_empty(),
    }
}

fn native_last<'a>(args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let f = args.get(0);
    let mut last: Option<Result<Val, Error>> = None;
    for r in f.run(cv) {
        match r {
            Err(e) => return box_once(Err(e)),
            ok => last = Some(ok),
        }
    }
    match last {
        Some(v) => box_once(v),
        None => box_empty(),
    }
}

// ════════════════════════════════════════════════════════════════════
//  ARITY-2 / ARITY-3 NATIVES
// ════════════════════════════════════════════════════════════════════

fn native_limit<'a>(args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let n_filter = args.get(0);
    let f = args.get(1);
    let ctx = cv.0.clone();
    let mut n_iter = n_filter.run((ctx, cv.1.clone()));
    let n = match n_iter.next() {
        Some(Ok(Val::Int(i))) if i >= 0 => i as usize,
        Some(Ok(Val::Int(_))) => 0,
        Some(Ok(v)) => return box_once(Err(Error::Type(v, jaq_interpret::error::Type::Int))),
        Some(Err(e)) => return box_once(Err(e)),
        None => return box_empty(),
    };
    if n == 0 {
        return box_empty();
    }
    Box::new(f.run(cv).take(n))
}

fn native_range<'a>(args: Args<'a, Val>, cv: Cv<'a>) -> ValR2s<'a> {
    let from_f = args.get(0);
    let to_f = args.get(1);
    let by_f = args.get(2);

    let ctx = cv.0.clone();
    let from_val = match from_f.run((ctx.clone(), cv.1.clone())).next() {
        Some(Ok(v)) => v,
        Some(Err(e)) => return box_once(Err(e)),
        None => return box_empty(),
    };
    let to_val = match to_f.run((ctx.clone(), cv.1.clone())).next() {
        Some(Ok(v)) => v,
        Some(Err(e)) => return box_once(Err(e)),
        None => return box_empty(),
    };
    let by_val = match by_f.run((ctx, cv.1)).next() {
        Some(Ok(v)) => v,
        Some(Err(e)) => return box_once(Err(e)),
        None => return box_empty(),
    };

    match (&from_val, &to_val, &by_val) {
        (Val::Int(from), Val::Int(to), Val::Int(by)) => {
            let (from, to, by) = (*from, *to, *by);
            if by == 0 {
                return box_empty();
            }
            let mut cur = from;
            Box::new(core::iter::from_fn(move || {
                if (by > 0 && cur < to) || (by < 0 && cur > to) {
                    let v = cur;
                    cur += by;
                    Some(Ok(Val::Int(v)))
                } else {
                    None
                }
            }))
        }
        _ => {
            let from_f = match from_val.as_float() {
                Ok(f) => f,
                Err(e) => return box_once(Err(e)),
            };
            let to_f = match to_val.as_float() {
                Ok(f) => f,
                Err(e) => return box_once(Err(e)),
            };
            let by_f = match by_val.as_float() {
                Ok(f) => f,
                Err(e) => return box_once(Err(e)),
            };
            if by_f == 0.0 {
                return box_empty();
            }
            let mut cur = from_f;
            Box::new(core::iter::from_fn(move || {
                if (by_f > 0.0 && cur < to_f) || (by_f < 0.0 && cur > to_f) {
                    let v = cur;
                    cur += by_f;
                    Some(Ok(Val::Float(v)))
                } else {
                    None
                }
            }))
        }
    }
}

// ════════════════════════════════════════════════════════════════════
//  PURE JQ DEFINITIONS
// ════════════════════════════════════════════════════════════════════

const JQ_DEFS: &str = r#"
def map(f): [.[] | f];
def select(f): if f then . else empty end;
def recurse(f): def r: ., (f | r); r;
def recurse: recurse(.[]?);
def walk(f): def w: (.[]? |= w) | f; w;
def any(f): reduce .[] as $x (false; . or ($x | f));
def all(f): reduce .[] as $x (true; . and ($x | f));
def any: any(.);
def all: all(.);
def range(n): range(0; n; 1);
def range(m; n): range(m; n; 1);
def with_entries(f): to_entries | map(f) | from_entries;
def map_values(f): .[] |= f;
def repeat(f): def r: f | r; r;
def while(cond; update): def r: if cond then ., (update | r) else empty end; r;
def until(cond; update): def r: if cond then . else update | r end; r;
def isempty(f): first((f | false), true);
def first: .[0];
def last: .[-1];
def nth(n): .[ n];
def nth(n; f): last(limit(n + 1; f));
def flatten(depth): if depth > 0 then [.[] | if type == "array" then flatten(depth - 1) | .[] else . end] else . end;
"#;

// ════════════════════════════════════════════════════════════════════
//  TESTS
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use jaq_interpret::{Ctx, FilterT, RcIter, Val};
    use serde_json::json;

    fn run_jq(expr: &str, input: serde_json::Value) -> Vec<Result<serde_json::Value, String>> {
        let (main, errs) = jaq_parse::parse(expr, jaq_parse::main());
        assert!(errs.is_empty(), "parse errors for `{expr}`: {errs:?}");
        let main = main.expect("parsed jq expression");

        let mut ctx = ParseCtx::new(Vec::new());
        register_stdlib(&mut ctx);
        let filter = ctx.compile(main);
        assert!(ctx.errs.is_empty(), "compile errors for `{expr}`: {} issues", ctx.errs.len());

        let inputs = RcIter::new(core::iter::empty());
        let jaq_input = Val::from(input);
        filter
            .run((Ctx::new([], &inputs), jaq_input))
            .map(|r| r.map(serde_json::Value::from).map_err(|e| format!("{e}")))
            .collect()
    }

    fn assert_jq(expr: &str, input: serde_json::Value, expected: serde_json::Value) {
        let results = run_jq(expr, input);
        assert_eq!(results.len(), 1, "expected 1 result for `{expr}`, got {}: {results:?}", results.len());
        assert_eq!(results[0], Ok(expected), "unexpected result for `{expr}`");
    }

    fn assert_jq_multi(expr: &str, input: serde_json::Value, expected: Vec<serde_json::Value>) {
        let results = run_jq(expr, input);
        let ok_results: Vec<serde_json::Value> =
            results.into_iter().map(|r| r.expect("expected Ok result")).collect();
        assert_eq!(ok_results, expected, "unexpected results for `{expr}`");
    }

    #[test]
    fn test_length_array() { assert_jq("length", json!([1, 2, 3]), json!(3)); }

    #[test]
    fn test_length_string() { assert_jq("length", json!("hello"), json!(5)); }

    #[test]
    fn test_length_object() { assert_jq("length", json!({"a": 1, "b": 2}), json!(2)); }

    #[test]
    fn test_length_null() { assert_jq("length", json!(null), json!(0)); }

    #[test]
    fn test_length_number() { assert_jq("length", json!(-5), json!(5)); }

    #[test]
    fn test_keys_object() { assert_jq("keys", json!({"b": 2, "a": 1}), json!(["a", "b"])); }

    #[test]
    fn test_keys_array() { assert_jq("keys", json!([10, 20, 30]), json!([0, 1, 2])); }

    #[test]
    fn test_values_object() {
        let results = run_jq("values", json!({"a": 1, "b": 2}));
        assert_eq!(results.len(), 1);
        let arr = results[0].as_ref().unwrap();
        assert!(arr.is_array());
        let vals: Vec<&serde_json::Value> = arr.as_array().unwrap().iter().collect();
        assert_eq!(vals.len(), 2);
        assert!(vals.contains(&&json!(1)));
        assert!(vals.contains(&&json!(2)));
    }

    #[test]
    fn test_type() {
        assert_jq("type", json!(null), json!("null"));
        assert_jq("type", json!(true), json!("boolean"));
        assert_jq("type", json!(42), json!("number"));
        assert_jq("type", json!("hi"), json!("string"));
        assert_jq("type", json!([1]), json!("array"));
        assert_jq("type", json!({}), json!("object"));
    }

    #[test]
    fn test_empty() { assert!(run_jq("empty", json!(null)).is_empty()); }

    #[test]
    fn test_not() {
        assert_jq("not", json!(false), json!(true));
        assert_jq("not", json!(null), json!(true));
        assert_jq("not", json!(true), json!(false));
        assert_jq("not", json!(42), json!(false));
    }

    #[test]
    fn test_add_numbers() { assert_jq("add", json!([1, 2, 3]), json!(6)); }

    #[test]
    fn test_add_strings() { assert_jq("add", json!(["a", "b", "c"]), json!("abc")); }

    #[test]
    fn test_add_empty() { assert_jq("add", json!([]), json!(null)); }

    #[test]
    fn test_flatten() { assert_jq("flatten", json!([[1, 2], [3], [4, [5]]]), json!([1, 2, 3, 4, [5]])); }

    #[test]
    fn test_to_entries() {
        let result = run_jq("to_entries", json!({"a": 1}));
        assert_eq!(result.len(), 1);
        let entry = &result[0].as_ref().unwrap().as_array().unwrap()[0];
        assert_eq!(entry["key"], json!("a"));
        assert_eq!(entry["value"], json!(1));
    }

    #[test]
    fn test_from_entries() {
        assert_jq("from_entries", json!([{"key": "a", "value": 1}]), json!({"a": 1}));
    }

    #[test]
    fn test_tostring() {
        assert_jq("tostring", json!(42), json!("42"));
        assert_jq("tostring", json!("already"), json!("already"));
    }

    #[test]
    fn test_tonumber() {
        assert_jq("tonumber", json!("42"), json!(42));
        assert_jq("tonumber", json!(3.14), json!(3.14));
    }

    #[test]
    fn test_ascii_case() {
        assert_jq("ascii_downcase", json!("Hello"), json!("hello"));
        assert_jq("ascii_upcase", json!("Hello"), json!("HELLO"));
    }

    #[test]
    fn test_has_object() {
        assert_jq(r#"has("a")"#, json!({"a": 1}), json!(true));
        assert_jq(r#"has("b")"#, json!({"a": 1}), json!(false));
    }

    #[test]
    fn test_has_array() {
        assert_jq("has(1)", json!([10, 20, 30]), json!(true));
        assert_jq("has(5)", json!([10, 20, 30]), json!(false));
    }

    #[test]
    fn test_contains_string() {
        assert_jq(r#"contains("ell")"#, json!("hello"), json!(true));
        assert_jq(r#"contains("xyz")"#, json!("hello"), json!(false));
    }

    #[test]
    fn test_startswith() {
        assert_jq(r#"startswith("hel")"#, json!("hello"), json!(true));
        assert_jq(r#"startswith("xyz")"#, json!("hello"), json!(false));
    }

    #[test]
    fn test_endswith() { assert_jq(r#"endswith("llo")"#, json!("hello"), json!(true)); }

    #[test]
    fn test_ltrimstr() {
        assert_jq(r#"ltrimstr("hel")"#, json!("hello"), json!("lo"));
        assert_jq(r#"ltrimstr("xyz")"#, json!("hello"), json!("hello"));
    }

    #[test]
    fn test_rtrimstr() { assert_jq(r#"rtrimstr("llo")"#, json!("hello"), json!("he")); }

    #[test]
    fn test_split() { assert_jq(r#"split(",")"#, json!("a,b,c"), json!(["a", "b", "c"])); }

    #[test]
    fn test_join() { assert_jq(r#"join("-")"#, json!(["a", "b", "c"]), json!("a-b-c")); }

    #[test]
    fn test_regex_test() {
        assert_jq(r#"test("^[0-9]+$")"#, json!("42"), json!(true));
        assert_jq(r#"test("^[0-9]+$")"#, json!("abc"), json!(false));
    }

    #[test]
    fn test_sort_by() {
        assert_jq(
            "sort_by(.x)",
            json!([{"x": 3}, {"x": 1}, {"x": 2}]),
            json!([{"x": 1}, {"x": 2}, {"x": 3}]),
        );
    }

    #[test]
    fn test_group_by() {
        assert_jq(
            "group_by(.a)",
            json!([{"a": 1}, {"a": 2}, {"a": 1}]),
            json!([[{"a": 1}, {"a": 1}], [{"a": 2}]]),
        );
    }

    #[test]
    fn test_unique_by() {
        assert_jq(
            "unique_by(.a)",
            json!([{"a": 1, "b": "x"}, {"a": 2, "b": "y"}, {"a": 1, "b": "z"}]),
            json!([{"a": 1, "b": "x"}, {"a": 2, "b": "y"}]),
        );
    }

    #[test]
    fn test_min_by() {
        assert_jq("min_by(.x)", json!([{"x": 3}, {"x": 1}, {"x": 2}]), json!({"x": 1}));
    }

    #[test]
    fn test_max_by() {
        assert_jq("max_by(.x)", json!([{"x": 3}, {"x": 1}, {"x": 2}]), json!({"x": 3}));
    }

    #[test]
    fn test_first() { assert_jq("first(.[])", json!([10, 20, 30]), json!(10)); }

    #[test]
    fn test_last() { assert_jq("last(.[])", json!([10, 20, 30]), json!(30)); }

    #[test]
    fn test_limit() { assert_jq_multi("limit(2; .[])", json!([10, 20, 30]), vec![json!(10), json!(20)]); }

    #[test]
    fn test_map() { assert_jq("map(. + 1)", json!([1, 2, 3]), json!([2, 3, 4])); }

    #[test]
    fn test_select() { assert_jq("[.[] | select(. > 2)]", json!([1, 2, 3, 4]), json!([3, 4])); }

    #[test]
    fn test_with_entries() {
        assert_jq(
            r#"with_entries(select(.value > 1))"#,
            json!({"a": 1, "b": 2, "c": 3}),
            json!({"b": 2, "c": 3}),
        );
    }

    #[test]
    fn test_range() { assert_jq("[range(3)]", json!(null), json!([0, 1, 2])); }

    #[test]
    fn test_sort() { assert_jq("sort", json!([3, 1, 2]), json!([1, 2, 3])); }

    #[test]
    fn test_reverse() { assert_jq("reverse", json!([1, 2, 3]), json!([3, 2, 1])); }

    #[test]
    fn test_unique() { assert_jq("unique", json!([1, 2, 1, 3, 2]), json!([1, 2, 3])); }

    #[test]
    fn test_abs() { assert_jq("abs", json!(-42), json!(42)); }

    #[test]
    fn test_floor() { assert_jq("floor", json!(3.7), json!(3)); }

    #[test]
    fn test_ceil() { assert_jq("ceil", json!(3.2), json!(4)); }

    #[test]
    fn test_round() { assert_jq("round", json!(3.5), json!(4)); }

    #[test]
    fn test_pipeline_map_select_length() {
        assert_jq(
            "[.[] | select(length > 3)]",
            json!(["ab", "abcd", "a", "abcde"]),
            json!(["abcd", "abcde"]),
        );
    }

    #[test]
    fn test_pipeline_group_by_map_length() {
        assert_jq(
            "[group_by(.t) | .[] | length]",
            json!([{"t": "a"}, {"t": "b"}, {"t": "a"}]),
            json!([2, 1]),
        );
    }
}
