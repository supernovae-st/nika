// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Reads that break when their producer is SKIPPED — a check HINT.
//!
//! Spec `04-variables.md` §defined-null reads: the value of a skipped task
//! (`tasks.X.output` · a named binding `tasks.X.<name>`) reads `null`, and a
//! value edge ADMITS a skipped producer (spec 03 · pass-set `{success,
//! skipped}`), so its reader runs. The spec stops at the null; the runtime
//! (`nika-cel`) raises on any step beneath it — a member step is
//! `NIKA-VAR-001` (the one `has()` reads as `false`), an index, `size()`, a
//! string method, an ordering, a boolean use or a fan-out over it is
//! `NIKA-VAR-006`. The trap is the dispatch ORDER (spec 03 · GATE → bindings
//! → `when:` → expansion → verb): a `with:` binding runs BEFORE its task's
//! own `when:`, so `when: ${{ with.u != null }}` never gets to guard
//! `u: ${{ tasks.X.output.url }}`. A binding error settles `failure`
//! (`on_error:` is not consulted), so the reader FAILS where its author
//! wrote a skip — and `check` used to say « run ready ✔ » over that file.
//!
//! The lane evaluates every read of a skippable producer in the world where
//! that producer was skipped, in the runtime's order:
//!
//! - **a `with:` binding** stepping beneath `tasks.X.output` /
//!   `tasks.X.<name>`;
//! - **the body** — the task's `when:`, its `for_each:` and its verb —
//!   stepping beneath a binding that carries that value WHOLE
//!   (`o: ${{ tasks.X.output }}` then `with.o.url`). The body runs only if
//!   the `when:` lets it: a gate proven FALSE in the skipped world
//!   (`with.o != null` · `has(with.o.url)` · a status or a derived-flag
//!   binding) guards it, and nothing past it is named.
//!
//! A read the expression itself guards is silent: `has()` (member steps
//! only — an index step raises `NIKA-VAR-006`, which `has()` lets through),
//! the short-circuit `x != null && …` / `x == null || …`, a ternary whose
//! condition the skip decides. A guard the walk cannot decide (an input,
//! another task's value) protects nothing for sure: the read CAN raise.
//!
//! The skipped world is more than a null. When the producer's `when:` is
//! its only way to skip, that gate was FALSE: a reader that tests the same
//! thing — `with.go == true` over the same `tasks.approve.output`, the
//! repeated consent guard — reads it false too, so its own guard holds.
//! Each reader's fate there feeds the settled-state fold
//! ([`crate::settle`]) that decides who is admitted at all: a reader whose
//! read raises on every path FAILS, one whose gate the skip closes SKIPS,
//! and whatever waits on its success is cancelled — never named. « Can be
//! skipped » itself reads the producer's declarations (a `when:` other than
//! `true` · a `for_each:`, whose empty collection skips · an `on_error:
//! skip`) through the same fold, which drops a producer that is never
//! admitted. A body read through a binding that is not one whole island (a
//! list holding the value · a rendered string) is not followed: no claim,
//! never a guess. A verb read the task's `on_error:` covers is the author's
//! failure to handle (measured: `recover:` settles it a success) and stays
//! silent; `on_error:` never reaches a binding, the gate or the collection.
//!
//! Advisory, never a refusal: a skippable producer is not a skipped one,
//! and the spec makes no such read illegal. The hint names the reader, the
//! read, the producer and why it can skip, and the repair.
//!
//! A null handed WHOLE to a `nika:jq` filter raises nothing here: whether
//! the filter fails on it is jq's to say, which this walk does not read.
//! [`null_into_jq`] names those readers, in the same skipped world, for a
//! host that evaluates jq in a bounded process of its own (a rehearsal).

use std::collections::BTreeMap;

use nika_schema::expression::{Expr, Literal, NamespaceRef, RelOp, expr_refs, scan_templates};
use nika_schema::raw::{ForEachValue, RawAction, RawTask, RawWorkflow};
use nika_schema::types::{OnErrorAction, WhenGate};

use crate::edges::{Edge, producer_ids};
use crate::gates::{K3, parse_gate, sole_island};
use crate::scan::{RESERVED_RECORD_FIELDS, action_surfaces};
use crate::settle::{S_ALL, S_CANCELLED, S_FAILURE, S_SKIPPED, S_SUCCESS, fold_settled};

mod jq_inputs;
pub use jq_inputs::{NullIntoJq, null_into_jq};

/// One site: the reading task and the advice body (the wrap adds the kind).
pub type Site = (String, String);

/// A member step on null — the one code `has()` reads as `false`.
const VAR_001: &str = "NIKA-VAR-001";
/// Every other use of null that raises (index · `size()` · a string method
/// · an ordering · a boolean use · a fan-out).
const VAR_006: &str = "NIKA-VAR-006";
/// Reads one row names before it counts the rest.
const LISTED: usize = 3;
/// The reason an expression gate gives — the one skip that proves a fact.
const GATE_CAN_CLOSE: &str = "its `when:` can close";

/// Every read of a skippable producer that raises once it is skipped, in
/// task order — one site per (reader, surface, producer).
///
/// `edges` and `topo_waves` are the analyzed graph: call this on a
/// conformant DAG only (the caller's gating, as for every folded lane).
#[must_use]
pub fn scan(wf: &RawWorkflow, edges: &[Edge], topo_waves: &[Vec<usize>]) -> Vec<Site> {
    let tasks = &wf.tasks;
    let ids: BTreeMap<&str, usize> = tasks
        .iter()
        .enumerate()
        .map(|(i, t)| (t.value.id.value.as_str(), i))
        .collect();
    let may = |n: usize| tasks.get(n).map_or(S_ALL, |t| may_settle(&t.value));
    let base = fold_settled(tasks.len(), edges, topo_waves, None, may);
    let mut skips: BTreeMap<usize, Option<Skip<'_>>> = BTreeMap::new();
    // producer → (discovery order, reader, what the reader does there)
    let mut readers: BTreeMap<usize, Vec<(usize, usize, Reads)>> = BTreeMap::new();
    let mut order = 0;
    for (reader, task) in tasks.iter().enumerate() {
        for producer in producer_ids(&task.value) {
            let Some(&p) = ids.get(producer.as_str()) else {
                continue; // DAG-002's report
            };
            let skip = skips
                .entry(p)
                .or_insert_with(|| tasks.get(p).and_then(|t| Skip::of(&t.value, base.of(p))));
            if let Some(skip) = skip {
                let reads = reads_of(&task.value, skip);
                readers.entry(p).or_default().push((order, reader, reads));
                order += 1;
            }
        }
    }
    let mut rows: Vec<(usize, Site)> = Vec::new();
    for (p, walks) in &readers {
        let Some(Some(skip)) = skips.get(p) else {
            continue;
        };
        if walks.iter().all(|(_, _, reads)| reads.rows.is_empty()) {
            continue;
        }
        // The world where `p` skipped: the skip is its FACT, and each reader
        // settles as its walk decided (a sure raise fails it · a closed gate
        // skips it) — whatever waits on that reader's success is cancelled.
        let fates: BTreeMap<usize, u8> = walks
            .iter()
            .filter_map(|(_, reader, reads)| reads.settles.map(|s| (*reader, s | S_CANCELLED)))
            .collect();
        let world = fold_settled(tasks.len(), edges, topo_waves, Some((*p, S_SKIPPED)), |n| {
            fates.get(&n).copied().unwrap_or_else(|| may(n))
        });
        for (seen, reader, reads) in walks {
            let Some(task) = tasks.get(*reader) else {
                continue;
            };
            if world.of(*reader) == S_CANCELLED {
                continue; // the skip cancels the reader: none of its reads runs
            }
            for read in &reads.rows {
                if let Some(row) = advice(&task.value, skip, read) {
                    rows.push((*seen, (task.value.id.value.clone(), row)));
                }
            }
        }
    }
    rows.sort_by_key(|(seen, _)| *seen);
    rows.into_iter().map(|(_, site)| site).collect()
}

/// Why a producer can settle `skipped`, by its OWN declarations (spec 03
/// §Task states) — empty when it cannot.
fn skip_reasons(task: &RawTask) -> Vec<&'static str> {
    let gate = task.when.as_ref().map(|w| &w.value);
    if matches!(gate, Some(WhenGate::Literal(false))) {
        // It never reaches its fan-out or its verb: the gate is the reason.
        return vec!["its `when:` is `false`, so it always skips"];
    }
    let mut why = Vec::new();
    if matches!(gate, Some(WhenGate::Expr(_))) {
        why.push(GATE_CAN_CLOSE);
    }
    if task.for_each.is_some() {
        why.push("its `for_each:` collection can be empty");
    }
    if task
        .on_error
        .as_ref()
        .is_some_and(|e| matches!(e.value.action, OnErrorAction::Skip))
    {
        why.push("its `on_error: skip` settles a failure as a skip");
    }
    why
}

/// The states a task can settle once its gate admits it — the fold's
/// `admitted` reading. An operator stop or a timeout can cancel anything,
/// as the dead-gate lane holds.
fn may_settle(task: &RawTask) -> u8 {
    let mut set = S_CANCELLED;
    if !matches!(
        task.when.as_ref().map(|w| &w.value),
        Some(WhenGate::Literal(false))
    ) {
        set |= S_SUCCESS | S_FAILURE;
    }
    if !skip_reasons(task).is_empty() {
        set |= S_SKIPPED;
    }
    set
}

/// One producer's skip: who, why it can, and what the skip proves.
struct Skip<'a> {
    producer: &'a str,
    reasons: Vec<&'static str>,
    /// An expression (printed through its task's bindings, see [`spell`])
    /// → the value the skip proves it has.
    facts: BTreeMap<String, bool>,
}

impl<'a> Skip<'a> {
    /// `None` when the task cannot skip — by its declarations, or because
    /// its base fold (`settled`) says it is never admitted to.
    fn of(task: &'a RawTask, settled: u8) -> Option<Self> {
        let reasons = skip_reasons(task);
        if reasons.is_empty() || settled & S_SKIPPED == 0 {
            return None;
        }
        let mut facts = BTreeMap::new();
        // A gate that is the ONLY way to skip was false when it did.
        if reasons == [GATE_CAN_CLOSE]
            && let Some(WhenGate::Expr(src)) = task.when.as_ref().map(|w| &w.value)
            && let Some(gate) = parse_gate(src)
        {
            learn(&gate, false, &carriers(task), &mut facts);
        }
        Some(Self {
            producer: task.id.value.as_str(),
            reasons,
            facts,
        })
    }
}

/// A task's whole-island bindings: `with.<key>` → the expression it holds.
fn carriers(task: &RawTask) -> BTreeMap<String, Expr> {
    task.with
        .iter()
        .filter_map(|(key, value)| Some((key.value.clone(), sole_island(value.value.as_str()?)?)))
        .collect()
}

/// Record that `e` has `value`, and what follows: `!x` flips, a false
/// `||` makes both sides false, a true `&&` both sides true, `==` and `!=`
/// negate each other, and either side of them may come first.
fn learn(e: &Expr, value: bool, with: &BTreeMap<String, Expr>, facts: &mut BTreeMap<String, bool>) {
    note(e, value, with, facts);
    match e {
        Expr::Not(inner) => learn(inner, !value, with, facts),
        Expr::Or(a, b) if !value => {
            learn(a, false, with, facts);
            learn(b, false, with, facts);
        }
        Expr::And(a, b) if value => {
            learn(a, true, with, facts);
            learn(b, true, with, facts);
        }
        Expr::Relation { op, lhs, rhs } if matches!(op, RelOp::Eq | RelOp::Ne) => {
            let negated = if *op == RelOp::Eq {
                RelOp::Ne
            } else {
                RelOp::Eq
            };
            for (op, holds) in [(*op, value), (negated, !value)] {
                for (l, r) in [(lhs, rhs), (rhs, lhs)] {
                    let fact = Expr::Relation {
                        op,
                        lhs: l.clone(),
                        rhs: r.clone(),
                    };
                    note(&fact, holds, with, facts);
                }
            }
        }
        _ => {}
    }
}

/// Keep one fact — only when it is GROUNDED: every `with.<key>` it reads
/// resolves to the binding's own expression. A `with.go` left as written
/// is this task's private name, and another task's `with.go` is another
/// value: printing them alike must never make them meet.
fn note(e: &Expr, value: bool, with: &BTreeMap<String, Expr>, facts: &mut BTreeMap<String, bool>) {
    let grounded = expr_refs(e)
        .iter()
        .all(|r| !matches!(r, NamespaceRef::With(key) if !with.contains_key(key)));
    if grounded {
        facts.insert(spell(e, with), value);
    }
}

/// Where a read sits — in the order the runtime evaluates it (spec 03 ·
/// bindings → `when:` → expansion → verb).
enum Surface {
    /// A `with:` binding (its key) — evaluated before the task's `when:`.
    Binding(String),
    /// The task's `when:`.
    When,
    /// The task's `for_each:` collection.
    ForEach,
    /// The verb's fields.
    Verb,
}

/// What one reader does once the producer skipped.
struct Reads {
    /// The surfaces whose reads raise there.
    rows: Vec<Read>,
    /// How the reader settles there when the walk decides it: a read that
    /// raises on every path fails it, a gate proven false skips it.
    settles: Option<u8>,
}

/// The reads of one surface that raise once the producer skipped.
struct Read {
    surface: Surface,
    raises: Vec<Raise>,
}

impl Read {
    /// Whether a read here raises on every path to it.
    fn certain(&self) -> bool {
        self.raises.iter().any(|r| r.certain)
    }
}

/// One use of the skipped value that raises.
struct Raise {
    /// The expression that raises, as written (`tasks.maybe.output.url`).
    read: String,
    /// How it raises, when the expression alone does not say it.
    how: Option<&'static str>,
    /// The value that reads null there (`tasks.maybe.output` · `with.o`).
    base: String,
    /// What the read adds to `base` (`.url` · `['k']`) — `None` when it is
    /// not a step an author can move behind a guard as written.
    step: Option<String>,
    /// The code the runtime raises.
    code: &'static str,
    /// No undecided guard stands between the surface and this read.
    certain: bool,
}

/// What `task` does once `skip.producer` skipped — its bindings first; its
/// body only when every binding lets the task reach it.
fn reads_of(task: &RawTask, skip: &Skip<'_>) -> Reads {
    let view = View::bindings(skip);
    let mut rows = Vec::new();
    for (key, value) in &task.with {
        let mut walk = Walk::new(&view);
        walk.value(&value.value);
        if !walk.raises.is_empty() {
            rows.push(Read {
                surface: Surface::Binding(key.value.clone()),
                raises: walk.raises,
            });
        }
    }
    if rows.is_empty() {
        return body_reads(task, &View::body(skip, task));
    }
    let settles = rows.iter().any(Read::certain).then_some(S_FAILURE);
    Reads { rows, settles }
}

/// The body's reads through the bindings that carry the skipped value.
/// Silent past a `when:` the skipped world proves false (the guard the hint
/// teaches); a `when:` or a `for_each:` that raises ends the walk there
/// (the task fails at that step, nothing after it runs).
fn body_reads(task: &RawTask, view: &View<'_>) -> Reads {
    let mut reads = Reads {
        rows: Vec::new(),
        settles: None,
    };
    // Whether the gate certainly lets the task through.
    let open = match task.when.as_ref().map(|w| &w.value) {
        Some(WhenGate::Literal(false)) => {
            reads.settles = Some(S_SKIPPED);
            return reads;
        }
        Some(WhenGate::Literal(true)) | None => true,
        Some(WhenGate::Expr(src)) => {
            let Some(expr) = parse_gate(src) else {
                return reads; // malformed · the scan layer reports it
            };
            let mut walk = Walk::new(view);
            let gate = walk.eval(&expr);
            if !walk.raises.is_empty() {
                // `on_error:` is not consulted for the gate (measured).
                return reads.with(Surface::When, walk.raises);
            }
            match gate {
                Val::Lit(Literal::Bool(false)) => {
                    reads.settles = Some(S_SKIPPED);
                    return reads;
                }
                Val::Lit(Literal::Bool(true)) => true,
                _ => false,
            }
        }
    };
    if view.with.is_empty() {
        return reads; // nothing in the body carries the skipped value
    }
    if let Some(each) = &task.for_each {
        let mut walk = Walk::new(view);
        walk.doubt = usize::from(!open);
        match &each.value {
            ForEachValue::Expression(src) => {
                if let Some(expr) = parse_gate(src)
                    && walk.eval(&expr) == Val::Skipped
                {
                    walk.raise(&expr, Some("as its collection"), &expr, VAR_006);
                }
            }
            ForEachValue::List(items) => walk.value(items),
            _ => {} // a future form: no claim
        }
        if !walk.raises.is_empty() {
            // `on_error:` does not recover a collection error (measured).
            return reads.with(Surface::ForEach, walk.raises);
        }
    }
    let (strings, jsons) = action_surfaces(&task.action);
    let mut walk = Walk::new(view);
    walk.doubt = usize::from(!open);
    for text in strings {
        walk.text(&text.value);
    }
    for value in jsons {
        walk.value(&value.value);
    }
    // A verb-field error is the verb's own: an `on_error:` covering its
    // code owns it (measured: `recover:` settles the task a success), and
    // a failure the author already handles is not this hint's to name.
    let raises: Vec<Raise> = walk
        .raises
        .into_iter()
        .filter(|raise| !owns(task, raise.code))
        .collect();
    if raises.is_empty() {
        return reads;
    }
    reads.with(Surface::Verb, raises)
}

/// Whether the task's `on_error:` handles `code` (no `on_codes:` filter,
/// or one that lists it).
fn owns(task: &RawTask, code: &str) -> bool {
    task.on_error.as_ref().is_some_and(|e| {
        e.value.on_codes.is_empty() || e.value.on_codes.iter().any(|c| c.value == code)
    })
}

impl Reads {
    /// Add the surface that raises — a certain raise fails the task.
    fn with(mut self, surface: Surface, raises: Vec<Raise>) -> Self {
        let read = Read { surface, raises };
        if read.certain() {
            self.settles = Some(S_FAILURE);
        }
        self.rows.push(read);
        self
    }
}

/// What a value is once ONE producer skipped — the walk's abstract domain.
#[derive(Debug, Clone, PartialEq)]
enum Val {
    /// The defined-null the skipped producer's value reads (spec 04).
    Skipped,
    /// A value the world fixes (a literal · the `'skipped'` status · a
    /// decided boolean).
    Lit(Literal),
    /// The `tasks` namespace.
    Tasks,
    /// A task's result record — `true` for the skipped producer's.
    Record(bool),
    /// The `with` namespace.
    With,
    /// Undecided — never evidence either way.
    Unknown,
}

/// The skipped world as one surface of one task sees it.
struct View<'s> {
    skip: &'s Skip<'s>,
    /// `with.<key>` → what the binding holds once the producer skipped
    /// (the body's view · empty for the bindings, which read `tasks.*`).
    with: BTreeMap<String, Val>,
    /// The task's whole-island bindings — how its guards print, so they
    /// meet the facts the skip proved.
    sources: BTreeMap<String, Expr>,
}

impl<'s> View<'s> {
    /// The bindings' view: they read `tasks.*` directly.
    fn bindings(skip: &'s Skip<'s>) -> Self {
        Self {
            skip,
            with: BTreeMap::new(),
            sources: BTreeMap::new(),
        }
    }

    /// The body's view: each whole-island binding that reads the producer,
    /// evaluated in the skipped world — `o: ${{ tasks.X.output }}` holds the
    /// null · `ok: ${{ tasks.X.output != null }}` holds `false` · `s: ${{
    /// tasks.X.status }}` holds `'skipped'`. Any other binding is undecided.
    fn body(skip: &'s Skip<'s>, task: &RawTask) -> Self {
        let sources = carriers(task);
        let plain = Self::bindings(skip);
        let mut with = BTreeMap::new();
        for (key, expr) in &sources {
            let reads_producer = expr_refs(expr)
                .iter()
                .any(|r| matches!(r, NamespaceRef::Tasks { id, .. } if id == skip.producer));
            if !reads_producer {
                continue;
            }
            match Walk::new(&plain).eval(expr) {
                Val::Skipped | Val::Lit(Literal::Null) => {
                    with.insert(key.clone(), Val::Skipped);
                }
                held @ Val::Lit(_) => {
                    with.insert(key.clone(), held);
                }
                _ => {}
            }
        }
        Self {
            skip,
            with,
            sources,
        }
    }
}

/// One evaluation in a [`View`], in the runtime's order (`nika-cel` ·
/// `&&`/`||` short-circuit, a ternary computes one branch, a relation both
/// sides), recording every use of the skipped value that raises.
struct Walk<'w> {
    view: &'w View<'w>,
    /// One frame per open `has()`, innermost last — set once a member step
    /// on the skipped value raised inside it (`NIKA-VAR-001`, which `has()`
    /// reads as `false`; the other codes go through it).
    has_frames: Vec<bool>,
    /// Above zero inside a branch the runtime may not take.
    doubt: usize,
    raises: Vec<Raise>,
    /// Read text → its index in `raises` (one row per read, linear on a
    /// hostile island list).
    seen: BTreeMap<String, usize>,
}

impl<'w> Walk<'w> {
    fn new(view: &'w View<'w>) -> Self {
        Self {
            view,
            has_frames: Vec::new(),
            doubt: 0,
            raises: Vec::new(),
            seen: BTreeMap::new(),
        }
    }

    /// Every island of a `with:`-shaped value, nested values included.
    fn value(&mut self, value: &serde_json::Value) {
        match value {
            serde_json::Value::String(s) => self.text(s),
            serde_json::Value::Array(items) => {
                for item in items {
                    self.value(item);
                }
            }
            serde_json::Value::Object(map) => {
                for item in map.values() {
                    self.value(item);
                }
            }
            _ => {}
        }
    }

    /// Every island of one string (a malformed one is the scan layer's).
    fn text(&mut self, text: &str) {
        let Ok(islands) = scan_templates(text) else {
            return;
        };
        for island in islands {
            self.eval(&island.expr);
        }
    }

    /// The value of `e` in the world, recording each use that raises.
    fn eval(&mut self, e: &Expr) -> Val {
        if let Some(known) = self.known(e) {
            return Val::Lit(Literal::Bool(known));
        }
        match e {
            Expr::Lit(lit) => Val::Lit(lit.clone()),
            Expr::Ident(root) => match root.as_str() {
                "tasks" => Val::Tasks,
                "with" => Val::With,
                _ => Val::Unknown,
            },
            Expr::Member { base, field } => {
                let of = self.eval(base);
                self.step(e, base, &of, Some(field), true)
            }
            Expr::Index { base, index } => {
                let of = self.eval(base);
                if self.eval(index) == Val::Skipped {
                    self.raise(e, None, index, VAR_006); // indexing BY null
                }
                let key = match index.as_ref() {
                    Expr::Lit(Literal::Str(key)) => Some(key.as_str()),
                    _ => None,
                };
                self.step(e, base, &of, key, false)
            }
            Expr::SizeCall(inner) | Expr::SizeMethod(inner) => {
                if self.eval(inner) == Val::Skipped {
                    self.raise(e, None, inner, VAR_006);
                }
                Val::Unknown
            }
            Expr::StringMethod { base, arg, .. } => {
                for side in [base, arg] {
                    if self.eval(side) == Val::Skipped {
                        self.raise(e, None, side, VAR_006);
                    }
                }
                Val::Unknown
            }
            Expr::HasCall(inner) => self.presence(inner),
            Expr::List(items) => {
                for item in items {
                    self.eval(item);
                }
                Val::Unknown
            }
            Expr::Not(inner) => {
                let v = self.eval(inner);
                decided(self.truth(inner, &v).negate())
            }
            Expr::And(lhs, rhs) => self.logic(lhs, rhs, K3::False),
            Expr::Or(lhs, rhs) => self.logic(lhs, rhs, K3::True),
            Expr::Ternary { cond, then, else_ } => self.choice(cond, then, else_),
            Expr::Relation { op, lhs, rhs } => {
                let (l, r) = (self.eval(lhs), self.eval(rhs));
                self.relation(e, *op, (lhs.as_ref(), &l), (rhs.as_ref(), &r))
            }
        }
    }

    /// A test whose value the skip proved — the producer computed the same
    /// expression over the same records and found it so, without raising.
    fn known(&self, e: &Expr) -> Option<bool> {
        let test = matches!(
            e,
            Expr::Relation { .. }
                | Expr::Not(_)
                | Expr::And(..)
                | Expr::Or(..)
                | Expr::HasCall(_)
                | Expr::StringMethod { .. }
        );
        if !test || self.view.skip.facts.is_empty() {
            return None;
        }
        self.view
            .skip
            .facts
            .get(&spell(e, &self.view.sources))
            .copied()
    }

    /// `of.<name>` (`member`) or `of[…]` (`name` = a string-literal key) —
    /// navigation through `tasks` · a record · `with`, and a RAISE on the
    /// skipped value: `NIKA-VAR-001` for a member step (an open `has()`
    /// catches it), `NIKA-VAR-006` for an index (`cannot index null`).
    fn step(&mut self, e: &Expr, base: &Expr, of: &Val, name: Option<&str>, member: bool) -> Val {
        match (of, name) {
            (Val::Skipped, _) if member => {
                if let Some(frame) = self.has_frames.last_mut() {
                    *frame = true;
                } else {
                    self.raise(e, None, base, VAR_001);
                }
                Val::Unknown
            }
            (Val::Skipped, _) => {
                self.raise(e, None, base, VAR_006);
                Val::Unknown
            }
            (Val::Tasks, Some(id)) => Val::Record(id == self.view.skip.producer),
            (Val::Record(true), Some("output")) => Val::Skipped,
            (Val::Record(true), Some("status")) => Val::Lit(Literal::Str("skipped".to_owned())),
            // A named binding reads null as well (spec 04); the other record
            // fields (`error` · the timings) are not this lane's to decide.
            (Val::Record(true), Some(field)) if !RESERVED_RECORD_FIELDS.contains(&field) => {
                Val::Skipped
            }
            (Val::With, Some(key)) => self.view.with.get(key).cloned().unwrap_or(Val::Unknown),
            _ => Val::Unknown,
        }
    }

    /// `has(x)` — `false` on null, and once a member step raised on the
    /// skipped value inside it.
    fn presence(&mut self, inner: &Expr) -> Val {
        self.has_frames.push(false);
        let v = self.eval(inner);
        let caught = self.has_frames.pop().unwrap_or(false);
        match v {
            _ if caught => Val::Lit(Literal::Bool(false)),
            Val::Skipped | Val::Lit(Literal::Null) => Val::Lit(Literal::Bool(false)),
            Val::Lit(_) => Val::Lit(Literal::Bool(true)),
            _ => Val::Unknown,
        }
    }

    /// `&&` (stops on `false`) and `||` (stops on `true`): the left side
    /// always runs, the right one only when the left did not decide.
    fn logic(&mut self, lhs: &Expr, rhs: &Expr, stop: K3) -> Val {
        let l = self.eval(lhs);
        let left = self.truth(lhs, &l);
        if left == stop {
            return decided(stop);
        }
        let unsure = usize::from(left == K3::Unknown);
        self.doubt += unsure;
        let r = self.eval(rhs);
        let right = self.truth(rhs, &r);
        self.doubt -= unsure;
        decided(if stop == K3::False {
            left.and(right)
        } else {
            left.or(right)
        })
    }

    /// `cond ? then : else_` — one branch, or both in doubt.
    fn choice(&mut self, cond: &Expr, then: &Expr, else_: &Expr) -> Val {
        let c = self.eval(cond);
        match self.truth(cond, &c) {
            K3::True => self.eval(then),
            K3::False => self.eval(else_),
            K3::Unknown => {
                self.doubt += 1;
                let (a, b) = (self.eval(then), self.eval(else_));
                self.doubt -= 1;
                if a == b { a } else { Val::Unknown }
            }
        }
    }

    /// A value used as a condition (`!` · `&&` · `||` · `?:`) — the
    /// skipped value raises there (« expected a boolean · got null »).
    fn truth(&mut self, at: &Expr, v: &Val) -> K3 {
        match v {
            Val::Lit(Literal::Bool(true)) => K3::True,
            Val::Lit(Literal::Bool(false)) => K3::False,
            Val::Skipped => {
                self.raise(at, Some("as a boolean"), at, VAR_006);
                K3::Unknown
            }
            _ => K3::Unknown,
        }
    }

    /// A relation: `==`/`!=` never raise on null (the defined-null test ·
    /// spec 04) and decide when both sides are fixed; `in` reads a null
    /// needle as « not there » and raises on a null collection; an
    /// ordering raises on null.
    fn relation(&mut self, e: &Expr, op: RelOp, lhs: (&Expr, &Val), rhs: (&Expr, &Val)) -> Val {
        match op {
            RelOp::Eq | RelOp::Ne => match (fixed(lhs.1), fixed(rhs.1)) {
                (Some(a), Some(b)) => equal(&a, &b).map_or(Val::Unknown, |same| {
                    Val::Lit(Literal::Bool(same == (op == RelOp::Eq)))
                }),
                _ => Val::Unknown,
            },
            RelOp::In => {
                if *rhs.1 == Val::Skipped {
                    self.raise(e, None, rhs.0, VAR_006);
                }
                member_of(lhs.1, rhs.0)
            }
            RelOp::Lt | RelOp::Le | RelOp::Gt | RelOp::Ge => {
                for (side, v) in [lhs, rhs] {
                    if *v == Val::Skipped {
                        self.raise(e, None, side, VAR_006);
                    }
                }
                Val::Unknown
            }
        }
    }

    /// Record one use that raises — once per expression, certain as soon as
    /// one path to it is.
    fn raise(&mut self, read: &Expr, how: Option<&'static str>, base: &Expr, code: &'static str) {
        let read = show(read);
        let certain = self.doubt == 0;
        if let Some(&at) = self.seen.get(&read) {
            if let Some(known) = self.raises.get_mut(at) {
                known.certain |= certain;
            }
            return;
        }
        let base = show(base);
        let step = read
            .strip_prefix(base.as_str())
            .filter(|rest| how.is_none() && rest.starts_with(['.', '[']))
            .map(str::to_owned);
        self.seen.insert(read.clone(), self.raises.len());
        self.raises.push(Raise {
            read,
            how,
            base,
            step,
            code,
            certain,
        });
    }
}

/// A Kleene value as a world value.
fn decided(k: K3) -> Val {
    match k {
        K3::True => Val::Lit(Literal::Bool(true)),
        K3::False => Val::Lit(Literal::Bool(false)),
        K3::Unknown => Val::Unknown,
    }
}

/// The literal a value is fixed to — the skipped value is `null`.
fn fixed(v: &Val) -> Option<Literal> {
    match v {
        Val::Skipped => Some(Literal::Null),
        Val::Lit(lit) => Some(lit.clone()),
        _ => None,
    }
}

/// `a == b` when it is decidable here: a `null` side always is (spec 04),
/// so are two literals of one class. An int against a float compares on
/// the number line at run, and a cross-class compare raises: undecided.
fn equal(a: &Literal, b: &Literal) -> Option<bool> {
    match (a, b) {
        (Literal::Null, _) | (_, Literal::Null) => Some(a == b),
        (Literal::Bool(x), Literal::Bool(y)) => Some(x == y),
        (Literal::Str(x), Literal::Str(y)) => Some(x == y),
        (Literal::Int(x), Literal::Int(y)) => Some(x == y),
        _ => None,
    }
}

/// `needle in [literals]`, decided — the runtime reads a mismatched
/// element as « not the needle », never an error.
fn member_of(needle: &Val, haystack: &Expr) -> Val {
    let (Some(needle), Expr::List(items)) = (fixed(needle), haystack) else {
        return Val::Unknown;
    };
    let mut undecided = false;
    for item in items {
        let Expr::Lit(element) = item else {
            return Val::Unknown;
        };
        match equal(&needle, element) {
            Some(true) => return Val::Lit(Literal::Bool(true)),
            Some(false) => {}
            None => undecided = true,
        }
    }
    if undecided {
        Val::Unknown
    } else {
        Val::Lit(Literal::Bool(false))
    }
}

/// The row: what reads what, why the producer can skip, why the read
/// raises there, and the repair in the author's own names.
fn advice(task: &RawTask, skip: &Skip<'_>, read: &Read) -> Option<String> {
    let first = read.raises.first()?;
    let (reader, producer) = (task.id.value.as_str(), skip.producer);
    let mut codes: Vec<&str> = read.raises.iter().map(|r| r.code).collect();
    codes.sort_unstable();
    codes.dedup();
    let codes = codes.join(" / ");
    let base = &first.base;
    // A step beneath the value reads « beneath »; a use of the value itself
    // (a boolean · a fan-out · a computation over it) says so.
    let what = if first.step.is_some() {
        format!("reads {} beneath `{base}`", listed(&read.raises))
    } else {
        format!("uses {}", listed(&read.raises))
    };
    let skippable = format!(
        "`{producer}` can be skipped ({}), and a skipped task's value reads null",
        skip.reasons.join(" · ")
    );
    Some(match &read.surface {
        Surface::Binding(key) => {
            let field = verb_field(&task.action);
            let then = first.step.as_deref().map_or_else(
                || format!("then read `with.out` in {field}"),
                |step| format!("then `${{{{ with.out{step} }}}}` in {field}"),
            );
            format!(
                "binding `with.{key}` of task `{reader}` {what}: {skippable}. Bindings are \
                 evaluated BEFORE the task's own `when:`, so the read raises {codes} and \
                 `{reader}` fails instead of skipping. Bind the whole value and test it before \
                 stepping in: `out: ${{{{ {base} }}}}` in `with:` · `when: ${{{{ with.out != \
                 null }}}}` · {then} (or guard the binding itself: `${{{{ {base} != null ? … : \
                 null }}}}`)"
            )
        }
        Surface::When => format!(
            "the `when:` of task `{reader}` {what}, and `{base}` binds the value of \
             `{producer}`: {skippable}, so the gate itself raises {codes} and `{reader}` fails \
             instead of skipping. Test the value before stepping in: `when: ${{{{ {base} != \
             null && … }}}}`"
        ),
        Surface::ForEach | Surface::Verb => {
            let place = match read.surface {
                Surface::ForEach => "`for_each:`",
                _ => verb_field(&task.action),
            };
            let gate = match task.when.as_ref().map(|w| &w.value) {
                Some(WhenGate::Expr(_)) => {
                    format!("the `when:` of `{reader}` does not rule null out")
                }
                Some(WhenGate::Literal(_)) => format!("`{reader}` runs on `when: true`"),
                None => format!("`{reader}` has no `when:` to stop it"),
            };
            format!(
                "the {place} of task `{reader}` {what}, and `{base}` binds the value of \
                 `{producer}`: {skippable}; {gate}, so it raises {codes} and `{reader}` fails. \
                 Guard the task: `when: ${{{{ {base} != null }}}}`"
            )
        }
    })
}

/// The reads one row names — the first few, then a count.
fn listed(raises: &[Raise]) -> String {
    let mut shown: Vec<String> = raises
        .iter()
        .take(LISTED)
        .map(|r| match r.how {
            Some(how) => format!("`{}` {how}", r.read),
            None => format!("`{}`", r.read),
        })
        .collect();
    if raises.len() > LISTED {
        shown.push(format!("{} more", raises.len() - LISTED));
    }
    shown.join(" · ")
}

/// Where the verb reads a bound value (the repair's example).
fn verb_field(action: &RawAction) -> &'static str {
    match action {
        RawAction::Invoke(_) => "`args:`",
        RawAction::Exec(_) => "`command:`",
        RawAction::Infer(_) | RawAction::Agent(_) => "`prompt:`",
        _ => "the verb",
    }
}

/// The expression as an author writes it — the read a row names (the AST
/// carries no spans).
fn show(e: &Expr) -> String {
    spell(e, &BTreeMap::new())
}

/// `e` printed with each `with.<key>` in `with` replaced by the expression
/// its binding holds — so two tasks that test the same records the same way
/// print the same, whatever they named their bindings. Exhaustive on
/// purpose: a new node breaks here loudly.
fn spell(e: &Expr, with: &BTreeMap<String, Expr>) -> String {
    if let Some(source) = with_key(e).and_then(|key| with.get(key)) {
        return operand(source, 6, &BTreeMap::new());
    }
    match e {
        Expr::Or(a, b) => format!("{} || {}", operand(a, 2, with), operand(b, 3, with)),
        Expr::And(a, b) => format!("{} && {}", operand(a, 3, with), operand(b, 4, with)),
        Expr::Not(a) => format!("!{}", operand(a, 5, with)),
        // Relations do not chain (cel-subset/0.1): either side nests.
        Expr::Relation { op, lhs, rhs } => {
            let (l, r) = (operand(lhs, 5, with), operand(rhs, 5, with));
            format!("{l} {} {r}", op.as_str())
        }
        Expr::Ternary { cond, then, else_ } => format!(
            "{} ? {} : {}",
            operand(cond, 2, with),
            spell(then, with),
            spell(else_, with)
        ),
        Expr::SizeCall(a) => format!("size({})", spell(a, with)),
        Expr::HasCall(a) => format!("has({})", spell(a, with)),
        Expr::StringMethod { base, method, arg } => format!(
            "{}.{}({})",
            operand(base, 6, with),
            method.as_str(),
            spell(arg, with)
        ),
        Expr::SizeMethod(a) => format!("{}.size()", operand(a, 6, with)),
        Expr::Member { base, field } => format!("{}.{field}", operand(base, 6, with)),
        Expr::Index { base, index } => {
            format!("{}[{}]", operand(base, 6, with), spell(index, with))
        }
        Expr::Ident(name) => name.clone(),
        Expr::List(items) => {
            let items: Vec<String> = items.iter().map(|item| spell(item, with)).collect();
            format!("[{}]", items.join(", "))
        }
        Expr::Lit(lit) => match lit {
            Literal::Bool(b) => b.to_string(),
            Literal::Int(i) => i.to_string(),
            Literal::Float(f) => f.to_string(),
            Literal::Str(s) if s.contains('\'') => format!("\"{s}\""),
            Literal::Str(s) => format!("'{s}'"),
            Literal::Null => "null".to_owned(),
        },
    }
}

/// An operand printed where a binding of at least `min` is due, in
/// parentheses when it binds looser (ternary 1 · `||` 2 · `&&` 3 · a
/// relation 4 · `!` 5 · a postfix chain or an atom 6).
fn operand(e: &Expr, min: u8, with: &BTreeMap<String, Expr>) -> String {
    let binds = match e {
        Expr::Ternary { .. } => 1,
        Expr::Or(..) => 2,
        Expr::And(..) => 3,
        Expr::Relation { .. } => 4,
        Expr::Not(_) => 5,
        _ => 6,
    };
    if binds < min {
        format!("({})", spell(e, with))
    } else {
        spell(e, with)
    }
}

/// `with.<key>` / `with['<key>']` → the key.
fn with_key(e: &Expr) -> Option<&str> {
    match e {
        Expr::Member { base, field } if matches!(base.as_ref(), Expr::Ident(root) if root == "with") => {
            Some(field)
        }
        Expr::Index { base, index } => match (base.as_ref(), index.as_ref()) {
            (Expr::Ident(root), Expr::Lit(Literal::Str(key))) if root == "with" => Some(key),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests;
