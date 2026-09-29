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

use super::super::Refusal;
use super::super::engine::{Data, run_with, to_val};
use jaq_core::load::lex::StrPart;
use jaq_core::load::parse::{BinaryOp, Pattern, Term};
use jaq_core::ops::Cmp;
use jaq_core::path::{Opt, Part, Path};
use jaq_json::Val;
use serde_json::Value;
use std::fmt::Write as _;
use unicode_normalization::UnicodeNormalization;

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

/// Run `program` over `input` in the verifier's one jq language ([`run_with`]: the same stack,
/// capability filter, runtime shadows, variables and fixed clock), with the probe's two natives
/// added, which only the canonical copy calls.
pub(super) fn run(program: &str, input: &Value) -> Result<Value, Refusal> {
    run_with(program, input, natives())
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

#[cfg(test)]
mod tests {
    use super::{copies, run};
    use serde_json::{Value, json};

    /// The row a probe runs on: one record spelled as the clause states « livré ».
    fn row() -> Value {
        json!({"records": [{"id": "a1", "item": "x", "status": "livr\u{e9}", "qty": "40"}]})
    }

    /// The probe natives exist only in the private copy: the verifier's own runner refuses a
    /// program naming one, which the probe runner evaluates.
    #[test]
    fn a_program_naming_a_probe_native_does_not_compile_in_the_verifier() {
        for program in ["\"livre\u{301}\" | __nika_canon", "{} | __nika_canon_keys"] {
            let refused = super::super::super::run(program, &row());
            assert!(
                refused.is_err_and(|r| r.0.contains("__nika_canon")),
                "{program}"
            );
            assert!(run(program, &row()).is_ok(), "{program}");
        }
        let canonical = run("\"livre\u{301}\" | __nika_canon", &row());
        assert_eq!(canonical.unwrap(), json!("livr\u{e9}"));
    }

    /// Every construct the printer knows round-trips: the identity copy answers exactly as the
    /// program on the probed row, and the canonical copy runs.
    #[test]
    fn the_identity_copy_answers_as_the_program() {
        let programs = [
            ".records | map(select(.status == \"livr\u{e9}\") | .qty | tonumber) | add // 0",
            ".records | map(\"livr\u{e9}\" as $l | select(.status == $l and ($l | length) == 5) | .qty | tonumber) | add // 0",
            ".records | map(select(.status == (\"liv\" + \"r\u{e9}\")) | .qty | tonumber) | add // 0",
            ".records | map({\"livr\u{e9}\": (.qty | tonumber)}[.status] // 0) | add // 0",
            ".records | map(select(.status | test(\"livr\u{e9}\")) | .qty | tonumber) | add // 0",
            ".records | map(select((.status | ltrimstr(\"livr\u{e9}\")) == \"\")) | length",
            "reduce .records[] as $r (0; . + ($r.qty | tonumber))",
            "[foreach .records[] as {qty: $q} (0; . + ($q | tonumber); .)]",
            "label $out | .records[] | if .qty == \"40\" then .id, break $out else empty end",
            "try (.records[0].qty | tonumber) catch 0",
            "[.records[] | .status?, .missing?] | length",
            "def twice(f): f | f; .records | map(.qty | tonumber | twice(. * 2)) | add",
            "{id: .records[0].id, n: (.records | length), \"s\": .records[0].status, (.records[0].item): 1}",
            "\"\\(.records[0].id)-\\(.records[0].qty)\" | @uri",
            ".records[0].status | [.[0:4], .[-1:], explode[0]]",
            ".records | [sort_by(.status)[].id, (group_by(.item) | length), (unique_by(.status) | length)]",
            "-(.records | length) + 1",
            ".records[0] | .qty |= tonumber",
            "[.records[] | select(.status != \"x\")] | if length > 0 then \"some\" elif length == 0 then \"none\" else \"?\" end",
            ".records | map(.status | ascii_downcase | @base64) | first",
        ];
        for program in programs {
            let (identity, canonical) = copies(program).expect(program);
            assert_eq!(
                run(&identity, &row()),
                run(program, &row()),
                "{program}\n{identity}"
            );
            assert!(run(&canonical, &row()).is_ok(), "{program}\n{canonical}");
        }
    }

    /// A definition shadowing a relation leaves no canonical copy: inconclusive, never a proof.
    #[test]
    fn a_definition_shadowing_a_relation_has_no_canonical_copy() {
        let program = "def test(x): true; .records | map(select(.status | test(\"a\"))) | length";
        assert!(copies(program).is_none());
    }
}
