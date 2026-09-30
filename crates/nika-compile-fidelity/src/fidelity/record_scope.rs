// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Laws 24 and 25: a `nika:jq` expression read in the scope the host's observation gives its
//! records. The input document is the task's `args.input` object (the keys the candidate
//! states) or one template; the records of a key are traced to an observed file through `with:`
//! bindings and task outputs: a `nika:read` of a literal or `const:` path, then a `nika:convert`
//! to JSON or a `fromjson` `nika:jq`. The walk follows jq's scope: `.K[]` and `.K | .[]` give
//! one record; `map`, `sort_by`, `group_by`, `unique_by`, `min_by`, `max_by`, `any` and `all`
//! take their filter per record, `select` keeps its input; `E as $x | F` keeps `.` and gives
//! `$x` the scope of `E`. A path on another variable, a `def` body, the update of `reduce` or
//! `foreach` and an unknown call are never judged.
//!
//! - Law 24 · record scope: on one record, a path whose first key is a key of the input object
//!   and no observed column of the records' file reads null (a window compared against that
//!   null keeps no records; converting that null fails at Run).
//! - Law 25 · instant order (the `instants` module): on one record, text order over date-times
//!   observed in several offsets or forms, or against a bound in another offset.
//!
//! Only a `nika:jq` task without `for_each` whose expression parses is judged, and only with the
//! host's observation: without it, nothing here can tell what a record carries.

use super::instants::{self, Order};
use super::{Diagnostic, data_sources, resolved, tool_of};
use jaq_core::load::lex::StrPart;
use jaq_core::load::parse::{BinaryOp, Pattern, Term};
use jaq_core::ops::Cmp;
use jaq_core::path::{Opt, Part};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// The most tasks a trace climbs from an input key to the read of its file.
const TRACE_DEPTH: usize = 4;

/// The records of one input key, as the host observed their file.
#[derive(Clone, Copy)]
pub(super) struct Records<'a> {
    /// The key of the input document that holds them (empty for the whole input).
    key: &'a str,
    /// The observed row of their file (`path`, `columns`, `values`).
    pub(super) row: &'a Value,
    /// The kinds entry of their file, when the host recorded one.
    pub(super) kinds: Option<&'a Value>,
}

impl Records<'_> {
    /// The path the host observed.
    pub(super) fn path(&self) -> &str {
        self.row["path"].as_str().unwrap_or_default()
    }

    /// Whether the observed records carry `field` (a CSV header column, a JSON record key).
    fn carries(&self, field: &str) -> bool {
        self.row["columns"]
            .as_array()
            .is_some_and(|columns| columns.iter().any(|c| c == field))
    }
}

/// What `.` is at a point of the expression.
#[derive(Clone, Copy)]
enum Scope<'a> {
    /// The input document: the object the task states.
    Document,
    /// The array of observed records.
    Rows(Records<'a>),
    /// One record of it.
    Row(Records<'a>),
    /// Anything else: never judged.
    Other,
}

impl Scope<'_> {
    /// The scope of the filter a per-record builtin applies to each element.
    fn each(self) -> Self {
        match self {
            Self::Rows(records) => Self::Row(records),
            _ => Self::Other,
        }
    }
}

/// What the walk of one expression found.
struct Walk<'a> {
    /// The keys of the input document, each with its observed records.
    keys: BTreeMap<&'a str, Option<Records<'a>>>,
    /// The bound variables, innermost last.
    vars: Vec<(&'a str, Scope<'a>)>,
    /// Law 24: each input-document key read on a record, with those records.
    misread: Vec<(&'a str, Records<'a>)>,
    /// Law 25: each order over a record field.
    orders: Vec<Order<'a>>,
}

/// Laws 24 and 25 over a candidate, with the host's observation (see the module note).
pub(super) fn record_laws(doc: &Value, world: Option<&Value>, out: &mut Vec<Diagnostic>) {
    let (Some(world), Some(tasks)) = (world, doc.get("tasks").and_then(Value::as_object)) else {
        return;
    };
    for (id, task) in tasks {
        if tool_of(doc, id) != "nika:jq" || task.get("for_each").is_some() {
            continue;
        }
        let (Some(input), Some(expression)) = (
            task.pointer("/invoke/args/input"),
            task.pointer("/invoke/args/expression")
                .and_then(Value::as_str),
        ) else {
            continue;
        };
        let Some(term) = jaq_core::load::parse(expression, |parser| parser.term()) else {
            continue;
        };
        let mut walk = Walk {
            keys: BTreeMap::new(),
            vars: Vec::new(),
            misread: Vec::new(),
            orders: Vec::new(),
        };
        let scope = if let Value::Object(fields) = input {
            walk.keys = fields
                .iter()
                .map(|(key, value)| (key.as_str(), records(doc, task, value, world, key)))
                .collect();
            Scope::Document
        } else {
            records(doc, task, input, world, "").map_or(Scope::Other, Scope::Rows)
        };
        walk.walk(&term, scope);
        misread(id, &walk.misread, out);
        instants::orders(id, &walk.orders, out);
    }
}

/// The observed records a value of `node` reads: it names exactly one task's output (directly
/// or through a `with:` binding), and that task is a `nika:read` of a path the host observed, or
/// a `nika:convert` to JSON or a `fromjson` `nika:jq` over such records.
fn records<'a>(
    doc: &'a Value,
    node: &'a Value,
    value: &'a Value,
    world: &'a Value,
    key: &'a str,
) -> Option<Records<'a>> {
    let (mut node, mut value) = (node, value);
    for _ in 0..TRACE_DEPTH {
        let [id] = <[String; 1]>::try_from(data_sources(node, Some(value))).ok()?;
        let upstream = doc.get("tasks")?.get(&id)?;
        let args = upstream.pointer("/invoke/args")?;
        let arg = |name: &str| args.get(name).and_then(Value::as_str).map(str::trim);
        match tool_of(doc, &id) {
            _ if upstream.get("for_each").is_some() => return None,
            "nika:read" => return observed(world, &resolved(doc, arg("path")?)?, key),
            "nika:convert" if arg("to") == Some("json") => {}
            "nika:jq" if arg("expression") == Some("fromjson") => {}
            _ => return None,
        }
        (node, value) = (upstream, args.get("input")?);
    }
    None
}

/// The host's row and kinds entry for `path`, when it observed records there.
fn observed<'a>(world: &'a Value, path: &str, key: &'a str) -> Option<Records<'a>> {
    let bare = |p: &str| p.strip_prefix("./").unwrap_or(p).to_owned();
    let row = world.get("observed")?.as_array()?.iter().find(|row| {
        row["state"] == "observed"
            && row["path"].as_str().is_some_and(|p| bare(p) == bare(path))
            && row["columns"].as_array().is_some_and(|c| !c.is_empty())
    })?;
    let kinds = world
        .get("kinds")
        .and_then(Value::as_object)
        .and_then(|kinds| kinds.iter().find(|(p, _)| bare(p) == bare(path)))
        .map(|(_, entry)| entry);
    Some(Records { key, row, kinds })
}

impl<'a> Walk<'a> {
    /// The scope of `term`'s output given the scope of its input, noting every read and order on
    /// a record on the way.
    fn walk(&mut self, term: &Term<&'a str>, scope: Scope<'a>) -> Scope<'a> {
        match term {
            Term::Id => scope,
            Term::Path(head, path) => {
                let from = self.walk(head, scope);
                self.path(from, &path.0, scope)
            }
            Term::BinOp(left, op, right) => self.binop(left, op, right, scope),
            Term::Call(name, args) => self.call(name, args, scope),
            Term::Arr(Some(inner)) => match self.walk(inner, scope) {
                Scope::Row(records) => Scope::Rows(records),
                _ => Scope::Other,
            },
            Term::Var(name) => self
                .vars
                .iter()
                .rev()
                .find(|(bound, _)| bound == name)
                .map_or(Scope::Other, |(_, bound)| *bound),
            Term::Fold(_, source, pattern, args) => {
                let bound = self.walk(source, scope);
                let depth = self.bind(pattern, bound);
                // The first argument starts from the input; the update runs on the accumulator.
                for (at, arg) in args.iter().enumerate() {
                    self.walk(arg, if at == 0 { scope } else { Scope::Other });
                }
                self.vars.truncate(depth);
                Scope::Other
            }
            Term::Def(defs, body) => {
                for def in defs {
                    self.walk(&def.body, Scope::Other);
                }
                self.walk(body, scope)
            }
            Term::Label(_, body) => self.walk(body, scope),
            other => {
                self.inner(other, scope);
                Scope::Other
            }
        }
    }

    /// The sub-terms of a term whose output is never records, each on the same input.
    fn inner(&mut self, term: &Term<&'a str>, scope: Scope<'a>) {
        match term {
            Term::IfThenElse(branches, otherwise) => {
                for (condition, then) in branches {
                    self.walk(condition, scope);
                    self.walk(then, scope);
                }
                if let Some(otherwise) = otherwise {
                    self.walk(otherwise, scope);
                }
            }
            Term::TryCatch(body, handler) => {
                self.walk(body, scope);
                if let Some(handler) = handler {
                    self.walk(handler, Scope::Other);
                }
            }
            Term::Obj(entries) => {
                for (key, value) in entries {
                    match (value, plain(key)) {
                        // `{name}` reads `.name`.
                        (None, Some(name)) => {
                            self.index(scope, name);
                        }
                        (value, _) => {
                            self.walk(key, scope);
                            if let Some(value) = value {
                                self.walk(value, scope);
                            }
                        }
                    }
                }
            }
            Term::Str(_, parts) => {
                for part in parts {
                    if let StrPart::Term(inner) = part {
                        self.walk(inner, scope);
                    }
                }
            }
            Term::Neg(inner) => {
                self.walk(inner, scope);
            }
            _ => {}
        }
    }

    /// The scope a path's parts reach from `now`; its index terms read `input`.
    fn path(
        &mut self,
        mut now: Scope<'a>,
        parts: &[(Part<Term<&'a str>>, Opt)],
        input: Scope<'a>,
    ) -> Scope<'a> {
        for (part, _) in parts {
            now = match part {
                Part::Index(key) => {
                    if let Some(name) = plain(key) {
                        self.index(now, name)
                    } else {
                        self.walk(key, input);
                        Scope::Other
                    }
                }
                Part::Range(None, None) => now.each(),
                Part::Range(from, upto) => {
                    for bound in from.iter().chain(upto) {
                        self.walk(bound, input);
                    }
                    Scope::Other
                }
            };
        }
        now
    }

    /// `.name` on `now`: the records of an input key, or a field of one record (Law 24 notes an
    /// input-document key the records do not carry).
    fn index(&mut self, now: Scope<'a>, name: &'a str) -> Scope<'a> {
        match now {
            Scope::Document => self
                .keys
                .get(name)
                .copied()
                .flatten()
                .map_or(Scope::Other, Scope::Rows),
            Scope::Row(records) => {
                if self.keys.contains_key(name) && !records.carries(name) {
                    self.misread.push((name, records));
                }
                Scope::Other
            }
            _ => Scope::Other,
        }
    }

    /// A binary operation: a pipe carries the scope, `E as $x | F` keeps it for `F`, an order on
    /// a record is noted for Law 25, and every other result is never records.
    fn binop(
        &mut self,
        left: &Term<&'a str>,
        op: &BinaryOp<&'a str>,
        right: &Term<&'a str>,
        scope: Scope<'a>,
    ) -> Scope<'a> {
        match op {
            BinaryOp::Pipe(None) => {
                let through = self.walk(left, scope);
                self.walk(right, through)
            }
            BinaryOp::Pipe(Some(pattern)) => {
                let bound = self.walk(left, scope);
                let depth = self.bind(pattern, bound);
                let result = self.walk(right, scope);
                self.vars.truncate(depth);
                result
            }
            _ => {
                if let (BinaryOp::Cmp(cmp), Scope::Row(records)) = (op, scope) {
                    self.compare(*cmp, left, right, records);
                }
                self.walk(left, scope);
                self.walk(right, scope);
                Scope::Other
            }
        }
    }

    /// Law 25: an order between a record field and the other operand.
    fn compare(
        &mut self,
        cmp: Cmp,
        left: &Term<&'a str>,
        right: &Term<&'a str>,
        records: Records<'a>,
    ) {
        let op = match cmp {
            Cmp::Lt => "<",
            Cmp::Le => "<=",
            Cmp::Gt => ">",
            Cmp::Ge => ">=",
            _ => return,
        };
        for (term, other) in [(left, right), (right, left)] {
            if let Some(field) = field_of(term) {
                let bound = plain(other);
                self.orders.push(Order {
                    field,
                    op,
                    bound,
                    records,
                });
            }
        }
    }

    /// A call: the builtins whose filter reads each element or keeps the input; any other call's
    /// arguments are never judged.
    fn call(&mut self, name: &'a str, args: &[Term<&'a str>], scope: Scope<'a>) -> Scope<'a> {
        match (name, args) {
            ("select", [filter]) => {
                self.walk(filter, scope);
                scope
            }
            ("first" | "last", [filter]) => self.walk(filter, scope),
            ("map", [filter]) => match (scope, self.walk(filter, scope.each())) {
                (Scope::Rows(records), Scope::Row(_)) => Scope::Rows(records),
                _ => Scope::Other,
            },
            ("sort_by" | "min_by" | "max_by", [key]) => {
                let op = match name {
                    "sort_by" => "sort_by",
                    "min_by" => "min_by",
                    _ => "max_by",
                };
                if let (Scope::Row(records), Some(field)) = (scope.each(), field_of(key)) {
                    let bound = None;
                    self.orders.push(Order {
                        field,
                        op,
                        bound,
                        records,
                    });
                }
                self.walk(key, scope.each());
                if op == "sort_by" { scope } else { scope.each() }
            }
            ("group_by" | "unique_by" | "any" | "all", [filter]) => {
                self.walk(filter, scope.each());
                if name == "unique_by" {
                    scope
                } else {
                    Scope::Other
                }
            }
            _ => {
                for arg in args {
                    self.walk(arg, Scope::Other);
                }
                Scope::Other
            }
        }
    }

    /// Binds a pattern's variables (a plain variable to `bound`, a destructured one to nothing
    /// known) and returns the depth to restore.
    fn bind(&mut self, pattern: &Pattern<&'a str>, bound: Scope<'a>) -> usize {
        let depth = self.vars.len();
        if let Pattern::Var(name) = pattern {
            self.vars.push((*name, bound));
        } else {
            let mut names = Vec::new();
            destructured(pattern, &mut names);
            self.vars
                .extend(names.into_iter().map(|name| (name, Scope::Other)));
        }
        depth
    }
}

/// Every variable a destructuring pattern binds.
fn destructured<'a>(pattern: &Pattern<&'a str>, names: &mut Vec<&'a str>) {
    match pattern {
        Pattern::Var(name) => names.push(*name),
        Pattern::Arr(items) => items.iter().for_each(|item| destructured(item, names)),
        Pattern::Obj(entries) => entries
            .iter()
            .for_each(|(_, item)| destructured(item, names)),
    }
}

/// The field `term` reads when it is exactly `.name` or `."name"` on its input.
fn field_of<'a>(term: &Term<&'a str>) -> Option<&'a str> {
    match term {
        Term::Path(head, path) if matches!(**head, Term::Id) => match path.0.as_slice() {
            [(Part::Index(key), _)] => plain(key),
            _ => None,
        },
        _ => None,
    }
}

/// The text of a string literal with no interpolation and no format.
fn plain<'a>(term: &Term<&'a str>) -> Option<&'a str> {
    match term {
        Term::Str(None, parts) => match parts.as_slice() {
            [StrPart::Str(text)] => Some(*text),
            _ => None,
        },
        _ => None,
    }
}

/// Law 24: one finding per iterated key, naming every input-document key read on its records.
fn misread(task: &str, reads: &[(&str, Records<'_>)], out: &mut Vec<Diagnostic>) {
    let mut by_key: BTreeMap<&str, (Records<'_>, BTreeSet<&str>)> = BTreeMap::new();
    for (field, records) in reads {
        let entry = by_key
            .entry(records.key)
            .or_insert_with(|| (*records, BTreeSet::new()));
        entry.1.insert(*field);
    }
    for (key, (records, fields)) in by_key {
        let columns: Vec<&str> = records.row["columns"]
            .as_array()
            .map(|c| c.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let (read, named) = (listed(&fields, "`.", "`"), listed(&fields, "`", "`"));
        let (bound, path) = (listed(&fields, "`$doc.", "`"), records.path());
        let are = if fields.len() == 1 {
            "is a key"
        } else {
            "are keys"
        };
        let columns = columns.join(", ");
        out.push(Diagnostic { kind: "records", message: format!("RECORD SCOPE: the task `{task}` reads {read} on one record of `.{key}[]`, but the records observed in `{path}` carry no such field (columns: {columns}): {named} {are} of the jq input document and read null on a record. Bind the document before the iteration (`. as $doc | [ .{key}[] | … ]`) and read {bound}.") });
    }
}

/// `a`, `a and b`, `a, b and c`, each item between `open` and `close`.
pub(super) fn listed<'s>(
    items: impl IntoIterator<Item = &'s &'s str>,
    open: &str,
    close: &str,
) -> String {
    let wrapped: Vec<String> = items
        .into_iter()
        .map(|item| format!("{open}{item}{close}"))
        .collect();
    match wrapped.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
        None => String::new(),
    }
}
