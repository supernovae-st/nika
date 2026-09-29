// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The relation-canonical probe: a private copy of the seat's program in which every string
//! RELATION compares the canonical (NFC) forms of its operands while every VALUE operation keeps
//! its exact bytes. The execution parser reads the program; two copies are printed back from its
//! tree, fully parenthesized: the identity copy, and the canonical copy with its relations
//! rewritten and nothing else. No literal is edited, nothing is evaluated here, and the emitted
//! program is never changed. A construct without a faithful print, a definition that shadows a
//! relation, a probe error, or an identity copy that does not reproduce the program's own
//! answers leaves the probe inconclusive: never a proof and never a pass.
//!
//! Relations: `==`, `!=`, `<`, `<=`, `>`, `>=`; containment, position and keys (`contains`,
//! `inside`, `startswith`, `endswith`, `ltrimstr`, `rtrimstr`, `index`, `rindex`, `indices`,
//! `split/1`, `has`, `in`); regular expressions (`test`, `match`, `capture`, `scan`, `splits`,
//! `split/2`, `sub`, `gsub`), the input and the pattern both canonical; the orderings of `sort`,
//! `sort_by`, `group_by`, `unique`, `unique_by`, `min`, `max`, `min_by` and `max_by`; and an
//! object lookup by a key that is not a constant ASCII name or a number. Values: everything
//! else, including `length`, `utf8bytelength`, `explode`, `implode`, slices, `ascii_downcase`,
//! `ascii_upcase`, the `@` formats, `tojson`, concatenation, arithmetic and every output a
//! program builds.

use super::super::{
    JQ_STD_CORRECTIONS, MAX_OUTPUT_BYTES, Refusal, render_compile, render_load, to_val, variables,
};
use jaq_core::load::lex::StrPart;
use jaq_core::load::parse::{BinaryOp, Pattern, Term};
use jaq_core::load::{Arena, File, Loader};
use jaq_core::ops::Cmp;
use jaq_core::path::{Opt, Part, Path};
use jaq_core::{Compiler, Ctx, Vars, data as jaq_data};
use jaq_json::Val;
use serde_json::Value;
use std::fmt::Write as _;
use unicode_normalization::UnicodeNormalization;

type Data = jaq_data::JustLut<Val>;

/// Relations of one argument: the input and the argument compared canonical.
const ONE: &[&str] = &[
    "contains",
    "inside",
    "startswith",
    "endswith",
    "ltrimstr",
    "rtrimstr",
    "index",
    "rindex",
    "indices",
    "split",
    "has",
    "in",
    "test",
    "match",
    "capture",
    "scan",
    "splits",
];
/// Regular-expression relations of two or three arguments: the input and the pattern canonical.
const REGEX: &[&str] = &[
    "test", "match", "capture", "scan", "splits", "split", "sub", "gsub",
];
/// Orderings by the element itself, and the keyed form each becomes.
const ORDERINGS: &[(&str, &str)] = &[
    ("sort", "sort_by"),
    ("unique", "unique_by"),
    ("min", "min_by"),
    ("max", "max_by"),
];
/// Orderings and groupings by a key.
const BY: &[&str] = &["sort_by", "group_by", "unique_by", "min_by", "max_by"];

/// The identity copy and the relation-canonical copy of `program`, both printed from the
/// execution parser's own tree; `None` when it does not parse, when a construct has no faithful
/// print here, or when the program defines a function under a relation's name.
pub(super) fn copies(program: &str) -> Option<(String, String)> {
    let term = jaq_core::load::parse(program, |p| p.term())?;
    let identity = Printer { canonical: false }.term(&term)?;
    let canonical = Printer { canonical: true }.term(&term)?;
    Some((identity, canonical))
}

/// Run `program` over `input` as the verifier's runtime mirror runs a seat's program (the same
/// stack, capability filter, variables and fixed clock), with the probe's two natives added,
/// which only the canonical copy calls.
pub(super) fn run(program: &str, input: &Value) -> Result<Value, Refusal> {
    let val = to_val(input)?;
    let (names, vals) = variables(input)?;
    let corrections = jaq_core::load::parse(JQ_STD_CORRECTIONS, |p| p.defs())
        .ok_or_else(|| Refusal("internal: the jq std corrections do not parse".to_owned()))?;
    let clock = jaq_core::load::parse(nika_cap::JQ_CLOCK_DEFS, |p| p.defs())
        .ok_or_else(|| Refusal("internal: the jq clock definitions do not parse".to_owned()))?;
    let defs = jaq_core::defs()
        .chain(jaq_std::defs().filter(|d| nika_cap::install_jq_definition(d.name)))
        .chain(jaq_json::defs())
        .chain(corrections)
        .chain(clock);
    let funs = jaq_core::funs()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs())
        .filter(|f| nika_cap::install_jq_native(f.0))
        .chain(natives());
    let arena = Arena::default();
    let file = File {
        code: program,
        path: (),
    };
    let modules = Loader::new(defs)
        .load(&arena, file)
        .map_err(|errs| Refusal(format!("the probe does not parse: {}", render_load(&errs))))?;
    let filter = Compiler::default()
        .with_funs(funs)
        .with_global_vars(
            std::iter::once(nika_cap::JQ_RUN_START_VAR).chain(names.iter().map(String::as_str)),
        )
        .compile(modules)
        .map_err(|errs| {
            Refusal(format!(
                "the probe does not compile: {}",
                render_compile(&errs)
            ))
        })?;
    let ctx = Ctx::<Data>::new(
        &filter.lut,
        Vars::new(std::iter::once(Val::from(1_700_000_000_isize)).chain(vals)),
    );
    let mut single: Option<Value> = None;
    for result in filter.id.run((ctx, val)) {
        let value = result.map_err(|_| Refusal("the probe fails".to_owned()))?;
        let text = value.to_string();
        if single.is_some() || text.len() > MAX_OUTPUT_BYTES {
            return Err(Refusal(
                "the probe emits more than one bounded value".to_owned(),
            ));
        }
        single = Some(
            serde_json::from_str(&text)
                .map_err(|e| Refusal(format!("the probe's output is not JSON: {e}")))?,
        );
    }
    single.ok_or_else(|| Refusal("the probe emits no value".to_owned()))
}

/// The probe's natives: `__nika_canon` puts every string and key of its input in NFC;
/// `__nika_canon_keys` puts only an object's own keys in NFC and leaves its values as they are.
fn natives() -> [jaq_core::native::Fun<Data>; 2] {
    [
        jaq_core::native::run(("__nika_canon", jaq_core::native::v(0), |cv| {
            jaq_core::native::bome(canonical(&cv.1, true))
        })),
        jaq_core::native::run(("__nika_canon_keys", jaq_core::native::v(0), |cv| {
            jaq_core::native::bome(canonical(&cv.1, false))
        })),
    ]
}

fn canonical(value: &Val, deep: bool) -> jaq_core::ValR<Val> {
    let json: Value =
        serde_json::from_str(&value.to_string()).map_err(jaq_core::Error::<Val>::str)?;
    let json = if deep { nfc_deep(json) } else { nfc_keys(json) };
    to_val(&json).map_err(|Refusal(reason)| jaq_core::Error::<Val>::str(reason))
}

fn nfc(text: &str) -> String {
    text.nfc().collect()
}

fn nfc_deep(value: Value) -> Value {
    match value {
        Value::String(text) => Value::String(nfc(&text)),
        Value::Array(items) => Value::Array(items.into_iter().map(nfc_deep).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(key, item)| (nfc(&key), nfc_deep(item)))
                .collect(),
        ),
        other => other,
    }
}

fn nfc_keys(value: Value) -> Value {
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(key, item)| (nfc(&key), item))
                .collect(),
        ),
        other => other,
    }
}

/// Whether a lookup key is a constant ASCII name or a number: a field of the record, never a
/// relation between texts.
fn constant_name(key: &Term<&str>) -> bool {
    match key {
        Term::Num(_) => true,
        Term::Str(None, parts) => parts.iter().all(|part| match part {
            StrPart::Str(text) => text.is_ascii(),
            StrPart::Char(c) => c.is_ascii(),
            StrPart::Term(_) => false,
        }),
        _ => false,
    }
}

fn cmp(op: Cmp) -> &'static str {
    match op {
        Cmp::Lt => "<",
        Cmp::Le => "<=",
        Cmp::Gt => ">",
        Cmp::Ge => ">=",
        Cmp::Eq => "==",
        Cmp::Ne => "!=",
    }
}

fn escape(c: char) -> String {
    match c {
        '"' => "\\\"".to_owned(),
        '\\' => "\\\\".to_owned(),
        '\n' => "\\n".to_owned(),
        '\t' => "\\t".to_owned(),
        '\r' => "\\r".to_owned(),
        '\u{8}' => "\\b".to_owned(),
        '\u{c}' => "\\f".to_owned(),
        c if u32::from(c) < 0x20 => format!("\\u{:04x}", u32::from(c)),
        c => c.to_string(),
    }
}

/// Prints a parsed term back to jq, fully parenthesized; `canonical` rewrites the relations.
struct Printer {
    canonical: bool,
}

impl Printer {
    fn term(&self, term: &Term<&str>) -> Option<String> {
        Some(match term {
            Term::Id => ".".to_owned(),
            Term::Recurse => "..".to_owned(),
            Term::Num(n) => (*n).to_owned(),
            Term::Str(fmt, parts) => self.string(*fmt, parts)?,
            Term::Arr(None) => "[]".to_owned(),
            Term::Arr(Some(items)) => format!("[{}]", self.term(items)?),
            Term::Obj(entries) => self.object(entries)?,
            Term::Neg(inner) => format!("(-({}))", self.term(inner)?),
            Term::BinOp(l, op, r) => self.binop(l, op, r)?,
            Term::Label(x, body) => format!("(label {x} | {})", self.term(body)?),
            Term::Break(x) => format!("(break {x})"),
            Term::Fold(fold, xs, x, args) => {
                let args = self.all(args)?;
                let (xs, x) = (self.term(xs)?, self.pattern(x)?);
                format!("({fold} ({xs}) as {x} ({}))", args.join("; "))
            }
            Term::TryCatch(body, None) => format!("(try ({}))", self.term(body)?),
            Term::TryCatch(body, Some(catch)) => {
                format!("(try ({}) catch ({}))", self.term(body)?, self.term(catch)?)
            }
            Term::IfThenElse(branches, otherwise) => {
                let mut out = String::from("(if ");
                for (k, (cond, then)) in branches.iter().enumerate() {
                    let lead = if k == 0 { "" } else { " elif " };
                    let (cond, then) = (self.term(cond)?, self.term(then)?);
                    write!(out, "{lead}({cond}) then ({then})").ok()?;
                }
                if let Some(otherwise) = otherwise {
                    write!(out, " else ({})", self.term(otherwise)?).ok()?;
                }
                out.push_str(" end)");
                out
            }
            Term::Def(defs, rest) => {
                let mut out = String::from("(");
                for def in defs {
                    let shadows = ONE.contains(&def.name)
                        || REGEX.contains(&def.name)
                        || BY.contains(&def.name)
                        || ORDERINGS.iter().any(|(name, _)| *name == def.name);
                    if self.canonical && shadows {
                        return None;
                    }
                    let args = if def.args.is_empty() {
                        String::new()
                    } else {
                        format!("({})", def.args.join("; "))
                    };
                    write!(out, "def {}{args}: {}; ", def.name, self.term(&def.body)?).ok()?;
                }
                out.push_str(&self.term(rest)?);
                out.push(')');
                out
            }
            Term::Call(name, args) => self.call(name, args)?,
            Term::Var(v) => (*v).to_owned(),
            Term::Path(base, path) => self.path(base, path)?,
        })
    }

    fn all(&self, terms: &[Term<&str>]) -> Option<Vec<String>> {
        terms.iter().map(|t| self.term(t)).collect()
    }

    fn string(&self, fmt: Option<&str>, parts: &[StrPart<&str, Term<&str>>]) -> Option<String> {
        let mut out = fmt.map(|f| format!("{f} ")).unwrap_or_default();
        out.push('"');
        for part in parts {
            match part {
                StrPart::Str(text) => out.push_str(text),
                StrPart::Char(c) => out.push_str(&escape(*c)),
                StrPart::Term(inner) => write!(out, "\\({})", self.term(inner)?).ok()?,
            }
        }
        out.push('"');
        Some(out)
    }

    fn object(&self, entries: &[(Term<&str>, Option<Term<&str>>)]) -> Option<String> {
        let mut items = Vec::new();
        for (key, value) in entries {
            let plain = matches!(key, Term::Str(..) | Term::Var(_));
            let k = if plain {
                self.term(key)?
            } else {
                format!("({})", self.term(key)?)
            };
            items.push(match (value, plain) {
                (Some(value), _) => format!("{k}: ({})", self.term(value)?),
                (None, true) => k,
                (None, false) => return None,
            });
        }
        Some(format!("{{{}}}", items.join(", ")))
    }

    fn pattern(&self, pattern: &Pattern<&str>) -> Option<String> {
        Some(match pattern {
            Pattern::Var(x) => (*x).to_owned(),
            Pattern::Arr(items) => {
                let items: Option<Vec<String>> = items.iter().map(|p| self.pattern(p)).collect();
                format!("[{}]", items?.join(", "))
            }
            Pattern::Obj(entries) => {
                let mut items = Vec::new();
                for (key, inner) in entries {
                    let key = match key {
                        Term::Str(..) => self.term(key)?,
                        _ => format!("({})", self.term(key)?),
                    };
                    items.push(format!("{key}: {}", self.pattern(inner)?));
                }
                format!("{{{}}}", items.join(", "))
            }
        })
    }

    fn binop(&self, l: &Term<&str>, op: &BinaryOp<&str>, r: &Term<&str>) -> Option<String> {
        let (l, r) = (self.term(l)?, self.term(r)?);
        Some(match op {
            BinaryOp::Pipe(None) => format!("(({l}) | ({r}))"),
            BinaryOp::Pipe(Some(p)) => format!("(({l}) as {} | ({r}))", self.pattern(p)?),
            BinaryOp::Comma => format!("(({l}), ({r}))"),
            BinaryOp::Alt => format!("(({l}) // ({r}))"),
            BinaryOp::Or => format!("(({l}) or ({r}))"),
            BinaryOp::And => format!("(({l}) and ({r}))"),
            BinaryOp::Math(m) => format!("(({l}) {} ({r}))", m.as_str()),
            BinaryOp::Cmp(c) if self.canonical => {
                format!(
                    "((({l}) | __nika_canon) {} (({r}) | __nika_canon))",
                    cmp(*c)
                )
            }
            BinaryOp::Cmp(c) => format!("(({l}) {} ({r}))", cmp(*c)),
            BinaryOp::Assign => format!("(({l}) = ({r}))"),
            BinaryOp::Update => format!("(({l}) |= ({r}))"),
            BinaryOp::UpdateMath(m) => format!("(({l}) {}= ({r}))", m.as_str()),
            BinaryOp::UpdateAlt => format!("(({l}) //= ({r}))"),
        })
    }

    fn call(&self, name: &str, args: &[Term<&str>]) -> Option<String> {
        let printed = self.all(args)?;
        if self.canonical {
            let arity = printed.len();
            if (arity == 1 && ONE.contains(&name))
                || ((2..=3).contains(&arity) && REGEX.contains(&name))
            {
                let rest: String = printed[1..]
                    .iter()
                    .flat_map(|a| ["; ", a.as_str()])
                    .collect();
                let first = &printed[0];
                return Some(format!(
                    "((({first}) | __nika_canon) as $__nika_r | __nika_canon | {name}($__nika_r{rest}))"
                ));
            }
            if arity == 0
                && let Some((_, by)) = ORDERINGS.iter().find(|(n, _)| *n == name)
            {
                return Some(format!("({by}(__nika_canon))"));
            }
            if arity == 1 && BY.contains(&name) {
                return Some(format!("({name}(({}) | __nika_canon))", printed[0]));
            }
        }
        Some(if printed.is_empty() {
            name.to_owned()
        } else {
            format!("{name}({})", printed.join("; "))
        })
    }

    fn path(&self, base: &Term<&str>, path: &Path<Term<&str>>) -> Option<String> {
        let mut out = format!("({})", self.term(base)?);
        for (part, opt) in &path.0 {
            let q = if matches!(opt, Opt::Optional) {
                "?"
            } else {
                ""
            };
            out = match part {
                Part::Index(key) if self.canonical && !constant_name(key) => format!(
                    "(({out}) | __nika_canon_keys)[(({}) | __nika_canon)]{q}",
                    self.term(key)?
                ),
                Part::Index(key) => format!("{out}[{}]{q}", self.term(key)?),
                Part::Range(None, None) => format!("{out}[]{q}"),
                Part::Range(from, upto) => {
                    let side = |t: &Option<Term<&str>>| match t {
                        Some(t) => self.term(t),
                        None => Some(String::new()),
                    };
                    let (from, upto) = (side(from)?, side(upto)?);
                    format!("{out}[{from}:{upto}]{q}")
                }
            };
        }
        Some(out)
    }
}
