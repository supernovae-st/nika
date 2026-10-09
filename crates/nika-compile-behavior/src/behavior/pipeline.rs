// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The requested computation as an ordered semantic relation. Each step filters the rows the
//! step before it wrote, then applies its stages in the reader's fixed order: duplicates by
//! key, grouping with its aggregates (or totals over every row), sort, the first N rows,
//! projection, renames, duplicates. The comparators, junctions, aggregate operations and
//! number policies are the reader's own canonical types.

use std::collections::BTreeMap;

use nika_compile_reader::aggregate::AggOp;
use nika_compile_reader::rules::{Comparator, Junction, NumberPolicy};

use super::numbers::Decimal;

/// What a field is compared to, resolved: a value the request alluded to is the answer the
/// human gave for it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Operand {
    /// A number, compared by value under the number law.
    Number(Decimal),
    /// An exact text, compared as the reader states it (case and spelling kept).
    Text(String),
    /// A truth value, matched whichever way the file encodes it (`false` or `"false"`).
    Bool(bool),
    /// Another column of the same record.
    Column(String),
    /// An answered value an equality compares as text, the field read as jq's `tostring`
    /// reads it.
    AsText(String),
}

/// One comparison of a filter.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Test {
    pub field: String,
    pub comparator: Comparator,
    pub operand: Operand,
    /// Other exact spellings a text equality also matches (the reader's grounded expansion).
    pub spellings: Vec<String>,
}

impl Test {
    /// One comparison, with no other spelling.
    #[must_use]
    pub fn new(field: impl Into<String>, comparator: Comparator, operand: Operand) -> Self {
        Self {
            field: field.into(),
            comparator,
            operand,
            spellings: Vec::new(),
        }
    }
}

/// The rows a step keeps: every test joined by one junction; no test keeps every row.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Filter {
    pub tests: Vec<Test>,
    pub junction: Junction,
}

impl Filter {
    /// A filter of `tests` joined by `junction`.
    #[must_use]
    pub fn new(tests: Vec<Test>, junction: Junction) -> Self {
        Self { tests, junction }
    }

    /// The filter that keeps every row.
    #[must_use]
    pub fn all() -> Self {
        Self::new(Vec::new(), Junction::And)
    }
}

/// What an aggregate is over no value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum OnEmpty {
    /// The identity of a total or a count: zero.
    Zero,
    /// The stated number policy stops the run: an average, a minimum or a maximum of nothing
    /// is no value.
    Stops,
    /// The request states nothing: no value is assumed (never zero), the case is unverified.
    Unstated,
}

/// One aggregate over a group or over every row.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Aggregate {
    pub op: AggOp,
    /// The column it reads; none for a count.
    pub field: Option<String>,
    /// The output name the relation uses.
    pub name: String,
    /// Where `name` comes from.
    pub naming: Naming,
    /// The decimals the request rounds to, half away from zero.
    pub round: Option<u32>,
    pub on_empty: OnEmpty,
}

/// Where the output name of an aggregate comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Naming {
    /// The request states the name: a written result uses it.
    Stated,
    /// A proven fact leaves the name to the implementation: the value is matched under a key
    /// no other output of the row reserves.
    Free,
    /// No fact says whether the request states the name: a written result that holds the
    /// right value under some key is not certified, and a wrong one still fails.
    Unknown,
}

impl Aggregate {
    /// One aggregate whose naming is unknown; over no value it follows `on_empty`.
    #[must_use]
    pub fn new(
        op: AggOp,
        field: Option<String>,
        name: impl Into<String>,
        round: Option<u32>,
        on_empty: OnEmpty,
    ) -> Self {
        Self {
            op,
            field,
            name: name.into(),
            naming: Naming::Unknown,
            round,
            on_empty,
        }
    }

    /// The same aggregate, its name coming from `naming`.
    #[must_use]
    pub fn with_naming(mut self, naming: Naming) -> Self {
        self.naming = naming;
        self
    }
}

/// The order a step states.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Sort {
    pub field: String,
    pub descending: bool,
    /// The request's tie rule: rows with equal keys keep their order in the file. Without it,
    /// tied rows may come in any order and a cut through them may keep any of them.
    pub stable_ties: bool,
}

impl Sort {
    /// An order on `field`.
    #[must_use]
    pub fn new(field: impl Into<String>, descending: bool, stable_ties: bool) -> Self {
        Self {
            field: field.into(),
            descending,
            stable_ties,
        }
    }
}

/// One side of a computed column: a column of the same row, or a number the request states.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Term {
    Column(String),
    Number(Decimal),
}

/// The arithmetic of a computed column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Arith {
    Add,
    Sub,
}

/// A column each kept row gains, computed from two terms of that row under the number law
/// (`reorder_qty = 12 - stock`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Derived {
    pub name: String,
    pub left: Term,
    pub arith: Arith,
    pub right: Term,
}

impl Derived {
    /// The column `name`, `left` then `arith` then `right`.
    #[must_use]
    pub fn new(name: impl Into<String>, left: Term, arith: Arith, right: Term) -> Self {
        Self {
            name: name.into(),
            left,
            arith,
            right,
        }
    }
}

/// The stages a step applies after its filter, in this fixed order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Stages {
    /// Columns each kept row gains right after the filter, in the request's order.
    pub derived: Vec<Derived>,
    /// Duplicates by these key columns removed right after the filter, the first kept whole.
    pub distinct_by: Vec<String>,
    pub group_by: Option<String>,
    /// With `group_by`, one row per group; without it, totals over every row.
    pub aggregates: Vec<Aggregate>,
    pub sort: Option<Sort>,
    pub limit: Option<u32>,
    /// The columns each written row holds, in the request's own names before any rename.
    pub columns: Vec<String>,
    /// Projected columns written as numbers under the number law (a non-number stops the run).
    pub number_columns: Vec<String>,
    /// Keys renamed, source name to stated name.
    pub renames: Vec<(String, String)>,
    /// Duplicates removed after the projection.
    pub distinct: bool,
}

impl Stages {
    /// Whether these stages make totals over every row: aggregates without a group.
    #[must_use]
    pub fn totals(&self) -> bool {
        self.group_by.is_none() && !self.aggregates.is_empty()
    }

    /// Whether `name` is produced by these stages (the group column or an aggregate) rather
    /// than read from the rows the step receives.
    #[must_use]
    pub fn produces(&self, name: &str) -> bool {
        self.group_by.as_deref() == Some(name)
            || self.aggregates.iter().any(|a| a.name == name)
            || self.derived.iter().any(|d| d.name == name)
    }
}

/// One step of the relation.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Step {
    pub filter: Filter,
    pub stages: Stages,
}

impl Step {
    /// A step of `filter` then `stages`.
    #[must_use]
    pub fn new(filter: Filter, stages: Stages) -> Self {
        Self { filter, stages }
    }
}

/// The whole relation: its steps in the order the request states them, and the number policy
/// the request stated for each field read as a number. A field read as a number with no
/// stated policy has the law's domain: a record whose value there is no number is outside it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Pipeline {
    pub steps: Vec<Step>,
    pub policies: BTreeMap<String, NumberPolicy>,
    /// The request keeps the source order where it does not sort: rows still in file order are
    /// compared as a sequence, not as a multiset.
    pub keep_order: bool,
}

impl Pipeline {
    /// The relation of `steps`, with no stated number policy and no stated order.
    #[must_use]
    pub fn new(steps: Vec<Step>) -> Self {
        Self {
            steps,
            policies: BTreeMap::new(),
            keep_order: false,
        }
    }

    /// Whether the last step writes totals over every row.
    #[must_use]
    pub fn totals(&self) -> bool {
        self.steps.last().is_some_and(|step| step.stages.totals())
    }

    /// The relation in words, for the preview's « requested result ».
    #[must_use]
    pub fn describe(&self) -> String {
        let steps: Vec<String> = self.steps.iter().map(describe_step).collect();
        let mut out = if steps.is_empty() {
            "every record".to_owned()
        } else {
            steps.join("; then ")
        };
        if self.keep_order {
            out.push_str("; in the source order");
        }
        out
    }
}

fn describe_operand(operand: &Operand) -> String {
    match operand {
        Operand::Number(number) => number.to_string(),
        Operand::Text(text) => format!("{text:?}"),
        Operand::Bool(truth) => truth.to_string(),
        Operand::Column(column) => format!("column {column}"),
        Operand::AsText(text) => format!("{text:?} as text"),
    }
}

fn describe_filter(filter: &Filter) -> Option<String> {
    if filter.tests.is_empty() {
        return None;
    }
    let junction = if filter.junction == Junction::Or {
        " or "
    } else {
        " and "
    };
    let tests: Vec<String> = filter
        .tests
        .iter()
        .map(|t| {
            let operand = describe_operand(&t.operand);
            format!("{} {} {operand}", t.field, t.comparator.symbol())
        })
        .collect();
    Some(format!("keep {}", tests.join(junction)))
}

fn describe_term(term: &Term) -> String {
    match term {
        Term::Column(column) => column.clone(),
        Term::Number(number) => number.to_string(),
    }
}

fn describe_step(step: &Step) -> String {
    let mut parts: Vec<String> = describe_filter(&step.filter).into_iter().collect();
    let stages = &step.stages;
    for derived in &stages.derived {
        let sign = match derived.arith {
            Arith::Add => "+",
            Arith::Sub => "-",
        };
        let (left, right) = (describe_term(&derived.left), describe_term(&derived.right));
        parts.push(format!("{} = {left} {sign} {right}", derived.name));
    }
    if !stages.distinct_by.is_empty() {
        parts.push(format!("one row per {}", stages.distinct_by.join(", ")));
    }
    let aggregates: Vec<String> = stages
        .aggregates
        .iter()
        .map(|a| {
            let of = a
                .field
                .as_ref()
                .map_or_else(String::new, |field| format!(" of {field}"));
            format!("{} = {:?}{of}", a.name, a.op)
        })
        .collect();
    match (&stages.group_by, aggregates.is_empty()) {
        (Some(group), true) => parts.push(format!("one row per {group}")),
        (Some(group), false) => parts.push(format!("per {group}: {}", aggregates.join(", "))),
        (None, false) => parts.push(format!("totals {}", aggregates.join(", "))),
        (None, true) => {}
    }
    if let Some(sort) = &stages.sort {
        let way = if sort.descending {
            "descending"
        } else {
            "ascending"
        };
        let ties = if sort.stable_ties {
            ", ties in file order"
        } else {
            ""
        };
        parts.push(format!("sort by {} {way}{ties}", sort.field));
    }
    if let Some(limit) = stages.limit {
        parts.push(format!("the first {limit}"));
    }
    if !stages.columns.is_empty() {
        parts.push(format!("columns {}", stages.columns.join(", ")));
    }
    for (from, to) in &stages.renames {
        parts.push(format!("{from} renamed {to}"));
    }
    if stages.distinct {
        parts.push("without duplicates".to_owned());
    }
    if parts.is_empty() {
        "every row".to_owned()
    } else {
        parts.join(", ")
    }
}
