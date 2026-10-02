// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The expected result of a requested relation over the records a run consumed. The result is
//! a sequence of blocks: rows of one sort key each (one block when no order is required). A cut
//! through tied rows leaves a partial last block, any `take` of its rows unless the request
//! keeps ties in file order. A case the request leaves open is never filled with a guess: it is
//! unverified; a record outside the stated domain makes the fixture invalid; a stop a stated
//! number policy requires is predicted with the operation that stops, the field it reads and
//! the values it may name.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use nika_compile_reader::aggregate::AggOp;
use nika_compile_reader::rules::{Comparator, Junction, NumberPolicy};

use super::Operation;
use super::numbers::{Decimal, Law, number_like};
use super::pipeline::{
    Aggregate, Filter, Naming, OnEmpty, Operand, Pipeline, Sort, Stages, Step, Test,
};
use super::values::{
    Cell, Datum, Row, Same, as_text, exact_row_form, jq_order, order_is_meaningful, reading,
    row_form, same, shown, sort_key, value_at,
};
use crate::fidelity::instant_shape;

/// The number policies a relation states, by field.
type Policies = BTreeMap<String, NumberPolicy>;

/// Why a stated rule stops the run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Halt {
    /// A value the number law does not read (a stated FAIL policy, or a column the request
    /// writes as a number): the values of the field, among the rows the operation reads, that
    /// the run may name.
    NotANumber(Vec<Datum>),
    /// No value is a number where a stated policy needs one: a total or a ranking left with
    /// nothing, or an average, minimum or maximum over nothing.
    NoNumber,
}

/// A stop a stated rule requires on these records: the operation that stops and the field it
/// reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Stop {
    pub(crate) operation: Operation,
    pub(crate) field: String,
    pub(crate) halt: Halt,
}

impl Stop {
    fn not_a_number(operation: Operation, field: &str, value: &Datum) -> Undefined {
        Undefined::Stops(Self {
            operation,
            field: field.to_owned(),
            halt: Halt::NotANumber(vec![value.clone()]),
        })
    }

    fn no_number(operation: Operation, field: &str) -> Undefined {
        Undefined::Stops(Self {
            operation,
            field: field.to_owned(),
            halt: Halt::NoNumber,
        })
    }

    /// The stop in words, for evidence messages.
    pub(crate) fn describe(&self) -> String {
        let field = &self.field;
        match &self.halt {
            Halt::NotANumber(values) => {
                let listed: Vec<String> = values.iter().map(shown).collect();
                format!("{field} holds {}, no number", listed.join(", "))
            }
            Halt::NoNumber => format!("no {field} is a number"),
        }
    }
}

/// Why a relation has no single expected result on these records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Undefined {
    /// A stated rule stops the run on these records.
    Stops(Stop),
    /// The request leaves this case open, or this component does not evaluate it.
    Unverified(String),
    /// The records break the domain the request states: the fixture is invalid.
    OutOfDomain(String),
}

fn unverified(why: impl Into<String>) -> Undefined {
    Undefined::Unverified(why.into())
}

/// A stop on a value that is no number, widened to every such value of its field among `rows`,
/// the rows the operation reads: the run may name any of them.
fn widened(undefined: Undefined, rows: &[Row]) -> Undefined {
    match undefined {
        Undefined::Stops(Stop {
            operation,
            field,
            halt: Halt::NotANumber(_),
        }) => {
            let values = rows
                .iter()
                .map(|row| value_at(row, &field))
                .filter(|value| matches!(reading(value), Law::NotANumber))
                .cloned()
                .collect();
            Undefined::Stops(Stop {
                operation,
                field,
                halt: Halt::NotANumber(values),
            })
        }
        other => other,
    }
}

/// Rows the output must hold at one place of its order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Block {
    pub(crate) rows: Vec<Row>,
    /// How many of `rows` the output holds: all of them, or fewer for the block a cut goes
    /// through.
    pub(crate) take: usize,
}

impl Block {
    fn whole(rows: Vec<Row>) -> Self {
        let take = rows.len();
        Self { rows, take }
    }

    /// Whether a cut goes through this block: only `take` of its rows are written.
    pub(crate) fn partial(&self) -> bool {
        self.take < self.rows.len()
    }
}

/// How the rows of a result are ordered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Order {
    /// The order of the records the run consumed, which every stage so far kept.
    File,
    /// No request fixes the order of the rows (groups, for one).
    Free,
    /// Blocks of equal sort keys in the requested order; `stable`: each block keeps its rows
    /// in file order.
    Sorted { stable: bool },
}

/// The expected result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Expected {
    pub(crate) blocks: Vec<Block>,
    pub(crate) order: Order,
    /// Totals over every row: one row keyed by the totals' names.
    pub(crate) totals: bool,
    /// The cut of a sort under a stated number policy goes through distinct tied rows: the
    /// stated law gives that cut no answer, so a run that stops there, naming this field,
    /// follows the request as much as any valid choice does.
    pub(crate) tie_stop: Option<String>,
    /// Output columns named by an aggregate whose name is not stated, and their naming.
    pub(crate) unstated: BTreeMap<String, Naming>,
}

impl Expected {
    /// How many rows the output holds.
    pub(crate) fn size(&self) -> usize {
        self.blocks.iter().map(|block| block.take).sum()
    }
}

/// The comparisons of a filter over values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Compare {
    Gt,
    Ge,
    Lt,
    Le,
    Eq,
    Ne,
}

impl Compare {
    fn holds(self, order: Ordering) -> bool {
        match self {
            Self::Gt => order == Ordering::Greater,
            Self::Ge => order != Ordering::Less,
            Self::Lt => order == Ordering::Less,
            Self::Le => order != Ordering::Greater,
            Self::Eq => order == Ordering::Equal,
            Self::Ne => order != Ordering::Equal,
        }
    }

    fn orders(self) -> bool {
        matches!(self, Self::Gt | Self::Ge | Self::Lt | Self::Le)
    }
}

/// The text functions a filter applies to a value read as text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Textual {
    Starts,
    Contains,
    Ends,
}

enum Kind {
    Value(Compare),
    Text(Textual, bool),
}

fn kind(comparator: Comparator) -> Option<Kind> {
    Some(match comparator {
        Comparator::Gt => Kind::Value(Compare::Gt),
        Comparator::Ge => Kind::Value(Compare::Ge),
        Comparator::Lt => Kind::Value(Compare::Lt),
        Comparator::Le => Kind::Value(Compare::Le),
        Comparator::Eq => Kind::Value(Compare::Eq),
        Comparator::Ne => Kind::Value(Compare::Ne),
        Comparator::StartsWith => Kind::Text(Textual::Starts, false),
        Comparator::NotStartsWith => Kind::Text(Textual::Starts, true),
        Comparator::Contains => Kind::Text(Textual::Contains, false),
        Comparator::NotContains => Kind::Text(Textual::Contains, true),
        Comparator::EndsWith => Kind::Text(Textual::Ends, false),
        Comparator::NotEndsWith => Kind::Text(Textual::Ends, true),
        _ => return None,
    })
}

/// The number `field` holds in `row` under its stated policy, as `operation` reads it; `None`
/// when SKIP leaves the record out. With no stated policy the field's domain is the law: any
/// other value is outside it.
fn number_at(
    row: &Row,
    field: &str,
    policies: &Policies,
    operation: Operation,
) -> Result<Option<Decimal>, Undefined> {
    let value = value_at(row, field);
    match reading(value) {
        Law::Number(number) => Ok(Some(number)),
        Law::Beyond => Err(unverified(format!(
            "{field} holds a number beyond the supported precision"
        ))),
        Law::NotANumber => match policies.get(field) {
            Some(NumberPolicy::Skip) => Ok(None),
            Some(NumberPolicy::Fail) => Err(Stop::not_a_number(operation, field, value)),
            Some(_) => Err(unverified(format!("an unknown number policy for {field}"))),
            None => Err(Undefined::OutOfDomain(format!(
                "{field} holds {}, not a number, and the request states no policy for it",
                shown(value)
            ))),
        },
    }
}

/// The value of `column` read as text, when that reading does not depend on the runtime.
fn text_of(row: &Row, column: &str) -> Result<String, Undefined> {
    as_text(value_at(row, column)).ok_or_else(|| {
        unverified(format!(
            "{column} read as text depends on how the runtime writes a number"
        ))
    })
}

fn textual(test: &Test, row: &Row, function: Textual, negated: bool) -> Result<bool, Undefined> {
    let content = text_of(row, &test.field)?;
    let argument = match &test.operand {
        Operand::Text(literal) | Operand::AsText(literal) => literal.clone(),
        Operand::Number(number) => number.to_string(),
        Operand::Bool(truth) => truth.to_string(),
        Operand::Column(column) => text_of(row, column)?,
    };
    let found = match function {
        Textual::Starts => content.starts_with(argument.as_str()),
        Textual::Contains => content.contains(argument.as_str()),
        Textual::Ends => content.ends_with(argument.as_str()),
    };
    Ok(found != negated)
}

fn text_compare(
    test: &Test,
    value: &Datum,
    literal: &str,
    compare: Compare,
) -> Result<bool, Undefined> {
    if compare.orders() {
        let literal = Datum::Text(literal.to_owned());
        if !order_is_meaningful(value, &literal) {
            return Err(unverified(format!(
                "{} and {} are date-times of different forms or offsets: their text order is \
                 not their time order",
                shown(value),
                shown(&literal)
            )));
        }
        let order = jq_order(value, &literal)
            .ok_or_else(|| unverified(format!("an order over the value of {}", test.field)))?;
        return Ok(compare.holds(order));
    }
    let equal = std::iter::once(literal)
        .chain(test.spellings.iter().map(String::as_str))
        .any(|spelling| matches!(value, Datum::Text(content) if content == spelling));
    Ok(equal == (compare == Compare::Eq))
}

fn valued(
    test: &Test,
    row: &Row,
    compare: Compare,
    policies: &Policies,
    operation: Operation,
) -> Result<bool, Undefined> {
    let value = value_at(row, &test.field);
    match &test.operand {
        Operand::Number(literal) => {
            let Some(number) = number_at(row, &test.field, policies, operation)? else {
                return Ok(false);
            };
            Ok(compare.holds(number.cmp(literal)))
        }
        Operand::Column(other) if compare.orders() => {
            let Some(left) = number_at(row, &test.field, policies, operation)? else {
                return Ok(false);
            };
            let Some(right) = number_at(row, other, policies, operation)? else {
                return Ok(false);
            };
            Ok(compare.holds(left.cmp(&right)))
        }
        Operand::Column(other) => match same(row.get(&test.field), row.get(other)) {
            Same::Yes => Ok(compare == Compare::Eq),
            Same::No => Ok(compare == Compare::Ne),
            Same::Loosely => Err(unverified(format!(
                "{} and {other} differ as text and agree as numbers",
                test.field
            ))),
        },
        Operand::Text(literal) => text_compare(test, value, literal, compare),
        Operand::AsText(literal) => {
            if compare.orders() {
                return Err(unverified("an order comparison with a value read as text"));
            }
            let content = text_of(row, &test.field)?;
            Ok((content == *literal) == (compare == Compare::Eq))
        }
        Operand::Bool(truth) => {
            if compare.orders() {
                return Err(unverified("an order comparison with a truth value"));
            }
            let spelled = Datum::Text(truth.to_string());
            let equal = *value == Datum::Bool(*truth) || *value == spelled;
            Ok(equal == (compare == Compare::Eq))
        }
    }
}

fn holds(
    test: &Test,
    row: &Row,
    policies: &Policies,
    operation: Operation,
) -> Result<bool, Undefined> {
    match kind(test.comparator) {
        Some(Kind::Text(function, negated)) => textual(test, row, function, negated),
        Some(Kind::Value(compare)) => valued(test, row, compare, policies, operation),
        None => Err(unverified(format!(
            "an unknown comparison on {}",
            test.field
        ))),
    }
}

/// Whether `filter` keeps `row`; its tests run in order and stop as jq's `and` and `or` do.
/// `operation` names what the tests are: a filter of the relation or a condition.
fn keeps(
    filter: &Filter,
    row: &Row,
    policies: &Policies,
    operation: Operation,
) -> Result<bool, Undefined> {
    let decisive = match filter.junction {
        Junction::And => false,
        Junction::Or => true,
        _ => return Err(unverified("an unknown junction")),
    };
    for test in &filter.tests {
        if holds(test, row, policies, operation)? == decisive {
            return Ok(decisive);
        }
    }
    Ok(!decisive || filter.tests.is_empty())
}

/// Whether some record passes `filter`: a condition on the records a run consumed.
pub(crate) fn any_passes(
    filter: &Filter,
    records: &[Row],
    policies: &Policies,
) -> Result<bool, Undefined> {
    for row in records {
        let kept = keeps(filter, row, policies, Operation::Condition)
            .map_err(|undefined| widened(undefined, records))?;
        if kept {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Whether two rows share every key: `None` when two values of a key differ as text and agree
/// as numbers, which no request settles.
fn share_keys(left: &Row, right: &Row, keys: &[String]) -> Option<bool> {
    let mut all = true;
    for key in keys {
        match same(left.get(key), right.get(key)) {
            Same::Yes => {}
            Same::No => all = false,
            Same::Loosely => return None,
        }
    }
    Some(all)
}

/// Duplicates by `keys` removed, block by block, the first kept whole. Where the order inside a
/// block is not fixed, two rows sharing the keys but not the rest leave the kept one undecided.
fn distinct_by(blocks: Vec<Block>, keys: &[String], fixed: bool) -> Result<Vec<Block>, Undefined> {
    let mut kept: Vec<(usize, Row)> = Vec::new();
    for (at, block) in blocks.into_iter().enumerate() {
        for row in block.rows {
            let mut duplicate: Option<(usize, bool)> = None;
            for (earlier_at, earlier) in &kept {
                let shared = share_keys(earlier, &row, keys).ok_or_else(|| {
                    unverified("whether two values of a key are the same is not stated")
                })?;
                if shared {
                    let differs = exact_row_form(earlier) != exact_row_form(&row);
                    duplicate = Some((*earlier_at, differs));
                    break;
                }
            }
            let Some((earlier_at, differs)) = duplicate else {
                kept.push((at, row));
                continue;
            };
            if differs && earlier_at == at && !fixed {
                return Err(unverified(
                    "which of two rows sharing the key comes first is not fixed",
                ));
            }
        }
    }
    let mut out: Vec<Block> = Vec::new();
    let mut last = None;
    for (at, row) in kept {
        if last != Some(at) {
            out.push(Block::whole(Vec::new()));
        }
        if let Some(block) = out.last_mut() {
            block.rows.push(row);
            block.take += 1;
        }
        last = Some(at);
    }
    Ok(out)
}

fn numeric_aggregate(
    aggregate: &Aggregate,
    rows: &[Row],
    policies: &Policies,
) -> Result<Decimal, Undefined> {
    let name = &aggregate.name;
    let field = aggregate
        .field
        .as_deref()
        .ok_or_else(|| unverified(format!("the aggregate {name} names no column")))?;
    let mut values = Vec::new();
    for row in rows {
        if let Some(number) = number_at(row, field, policies, Operation::Aggregate)? {
            values.push(number);
        }
    }
    if values.is_empty() {
        let skip = matches!(policies.get(field), Some(NumberPolicy::Skip));
        let no_number = || Stop::no_number(Operation::Aggregate, field);
        return match aggregate.on_empty {
            OnEmpty::Zero if skip && !rows.is_empty() => Err(no_number()),
            OnEmpty::Zero => Ok(Decimal::zero()),
            OnEmpty::Stops => Err(no_number()),
            OnEmpty::Unstated => Err(unverified(format!(
                "{name} over no value: the request states no value for it"
            ))),
        };
    }
    let total = || {
        values
            .iter()
            .fold(Decimal::zero(), |sum, value| sum.plus(value))
    };
    let value = match aggregate.op {
        AggOp::Sum => total(),
        AggOp::Min => values.iter().min().cloned().unwrap_or_else(Decimal::zero),
        AggOp::Max => values.iter().max().cloned().unwrap_or_else(Decimal::zero),
        AggOp::Avg => {
            let count = u64::try_from(values.len()).unwrap_or(u64::MAX);
            return total().divided(count, aggregate.round).ok_or_else(|| {
                unverified(format!(
                    "the mean {name} never ends and the request states no rounding"
                ))
            });
        }
        AggOp::Count => return Err(unverified(format!("the count {name} reads no column"))),
        _ => return Err(unverified(format!("the aggregate {name} is not evaluated"))),
    };
    Ok(match aggregate.round {
        Some(places) => value.rounded(places),
        None => value,
    })
}

fn aggregate_cell(
    aggregate: &Aggregate,
    rows: &[Row],
    policies: &Policies,
) -> Result<Cell, Undefined> {
    let value = if aggregate.op == AggOp::Count {
        Decimal::from_count(u64::try_from(rows.len()).unwrap_or(u64::MAX))
    } else {
        numeric_aggregate(aggregate, rows, policies)
            .map_err(|undefined| widened(undefined, rows))?
    };
    Ok(Cell::typed(Datum::Number(value)))
}

/// One row per group, keyed by jq equality of the group column, the first member's value kept.
/// No request fixes the order of the groups.
fn grouped(
    rows: Vec<Row>,
    group: &str,
    aggregates: &[Aggregate],
    policies: &Policies,
) -> Result<Vec<Row>, Undefined> {
    if aggregates.is_empty() {
        return Err(unverified(format!(
            "rows per {group} with no aggregate the request names"
        )));
    }
    let mut groups: Vec<(Option<Cell>, Vec<Row>)> = Vec::new();
    for row in rows {
        let key = row.get(group).cloned();
        let mut found = None;
        for (at, (first, _)) in groups.iter().enumerate() {
            match same(first.as_ref(), key.as_ref()) {
                Same::Yes => {
                    found = Some(at);
                    break;
                }
                Same::No => {}
                Same::Loosely => {
                    return Err(unverified(format!(
                        "whether two values of {group} are one group is not stated"
                    )));
                }
            }
        }
        let Some((_, members)) = found.and_then(|at| groups.get_mut(at)) else {
            groups.push((key, vec![row]));
            continue;
        };
        members.push(row);
    }
    let mut out = Vec::with_capacity(groups.len());
    for (first, members) in groups {
        let mut written = Row::new();
        let key = first.unwrap_or_else(|| Cell::typed(Datum::Null));
        written.insert(group.to_owned(), key);
        for aggregate in aggregates {
            let cell = aggregate_cell(aggregate, &members, policies)?;
            written.insert(aggregate.name.clone(), cell);
        }
        out.push(written);
    }
    Ok(out)
}

/// The sort key of one row; `None` when SKIP leaves the row out of the ranking.
fn key_of(
    row: &Row,
    sort: &Sort,
    stages: &Stages,
    policies: &Policies,
) -> Result<Option<Datum>, Undefined> {
    let field = sort.field.as_str();
    let value = value_at(row, field);
    if stages.produces(field) {
        // A produced name is typed already: an aggregate is a number, a group key the value
        // of its source column, ordered as it is.
        if let Datum::Text(text) = value
            && (matches!(reading(value), Law::Number(_)) || number_like(text))
        {
            return Err(unverified(format!(
                "{field} holds {}: its order as text and as a number may disagree",
                shown(value)
            )));
        }
        return Ok(Some(value.clone()));
    }
    if policies.contains_key(field) {
        return Ok(number_at(row, field, policies, Operation::Rank)?.map(Datum::Number));
    }
    sort_key(value).map(Some).map_err(Undefined::Unverified)
}

/// The rows ordered by `sort` and grouped by equal keys, each block in the order the rows came.
/// A stated policy reads the key under the number law (SKIP leaves a record without a number
/// out of the ranking); without one a value holding a number sorts as that number, else as
/// itself.
fn sorted_blocks(
    rows: &[Row],
    sort: &Sort,
    stages: &Stages,
    policies: &Policies,
) -> Result<Vec<Block>, Undefined> {
    let had_rows = !rows.is_empty();
    let mut keyed: Vec<(Datum, Row)> = Vec::with_capacity(rows.len());
    for row in rows {
        let key =
            key_of(row, sort, stages, policies).map_err(|undefined| widened(undefined, rows))?;
        if let Some(key) = key {
            keyed.push((key, row.clone()));
        }
    }
    if had_rows && keyed.is_empty() {
        return Err(Stop::no_number(Operation::Rank, &sort.field));
    }
    let shapes: BTreeSet<(String, String)> = keyed
        .iter()
        .filter_map(|(key, _)| {
            let Datum::Text(text) = key else {
                return None;
            };
            instant_shape(text)
        })
        .collect();
    if shapes.len() > 1 {
        return Err(unverified(format!(
            "{} holds date-times of different forms or offsets: their text order is not their \
             time order",
            sort.field
        )));
    }
    let mut incomparable = false;
    keyed.sort_by(|a, b| {
        let (first, second) = if sort.descending {
            (&b.0, &a.0)
        } else {
            (&a.0, &b.0)
        };
        jq_order(first, second).unwrap_or_else(|| {
            incomparable = true;
            Ordering::Equal
        })
    });
    if incomparable {
        return Err(unverified(format!(
            "{} holds lists or objects, whose order is not judged",
            sort.field
        )));
    }
    let mut blocks: Vec<Block> = Vec::new();
    let mut previous: Option<Datum> = None;
    for (key, row) in keyed {
        let joins = previous
            .as_ref()
            .is_some_and(|earlier| jq_order(earlier, &key) == Some(Ordering::Equal));
        if !joins {
            blocks.push(Block::whole(Vec::new()));
        }
        if let Some(block) = blocks.last_mut() {
            block.rows.push(row);
            block.take += 1;
        }
        previous = Some(key);
    }
    Ok(blocks)
}

/// The first `limit` rows of ordered blocks: whole blocks, then `take` rows of the block the
/// cut goes through (its first rows when ties keep file order).
fn cut(blocks: Vec<Block>, limit: usize, stable: bool) -> Vec<Block> {
    let mut out = Vec::new();
    let mut left = limit;
    for mut block in blocks {
        if left == 0 {
            break;
        }
        if block.rows.len() <= left {
            left -= block.rows.len();
            out.push(block);
        } else {
            if stable {
                block.rows.truncate(left);
            }
            block.take = left;
            out.push(block);
            left = 0;
        }
    }
    out
}

/// One written row: its projection (number columns read under the law, where a non-number
/// stops the run) and its renames.
fn projected(row: Row, stages: &Stages) -> Result<Row, Undefined> {
    let mut out = if stages.columns.is_empty() {
        row
    } else {
        let mut picked = Row::new();
        for column in &stages.columns {
            let cell = if stages.number_columns.contains(column) {
                let value = value_at(&row, column);
                match reading(value) {
                    Law::Number(number) => Cell::typed(Datum::Number(number)),
                    Law::Beyond => {
                        return Err(unverified(format!(
                            "{column} holds a number beyond the supported precision"
                        )));
                    }
                    Law::NotANumber => {
                        return Err(Stop::not_a_number(Operation::Column, column, value));
                    }
                }
            } else {
                row.get(column)
                    .cloned()
                    .unwrap_or_else(|| Cell::typed(Datum::Null))
            };
            picked.insert(column.clone(), cell);
        }
        picked
    };
    for (from, to) in &stages.renames {
        if let Some(cell) = out.remove(from) {
            out.insert(to.clone(), cell);
        }
    }
    Ok(out)
}

/// Duplicate rows removed across the blocks in order, the first occurrence kept. Two rows equal
/// only once a CSV cell is read loosely leave the removal unstated.
fn without_duplicates(blocks: Vec<Block>) -> Result<Vec<Block>, Undefined> {
    let none = BTreeSet::new();
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    let mut out = Vec::new();
    for mut block in blocks {
        let mut rows = Vec::new();
        for row in block.rows {
            let (read, exact) = (row_form(&row, &none), exact_row_form(&row));
            match seen.get(&read).cloned() {
                Some(first) if first != exact => {
                    return Err(unverified(
                        "two rows differ as text and agree as numbers: whether they are \
                         duplicates is not stated",
                    ));
                }
                Some(_) => {}
                None => {
                    seen.insert(read, exact);
                    rows.push(row);
                }
            }
        }
        block.take = block.take.min(rows.len());
        block.rows = rows;
        if !block.rows.is_empty() {
            out.push(block);
        }
    }
    Ok(out)
}

/// The result of the steps so far.
struct State {
    blocks: Vec<Block>,
    order: Order,
    totals: bool,
    tie_stop: Option<String>,
    /// Columns of the rows named by an aggregate whose name is not stated, and their naming.
    unstated: BTreeMap<String, Naming>,
}

/// The rows a step keeps, before its aggregates.
fn filtered(state: State, step: &Step, policies: &Policies) -> Result<State, Undefined> {
    if state.totals {
        return Err(unverified("a step after totals is not evaluated"));
    }
    if state.blocks.iter().any(Block::partial) {
        return Err(unverified(
            "a later step depends on which tied rows the cut keeps",
        ));
    }
    let fixed = matches!(state.order, Order::File | Order::Sorted { stable: true });
    let read: Vec<Row> = state
        .blocks
        .iter()
        .flat_map(|block| block.rows.iter().cloned())
        .collect();
    let mut blocks = Vec::with_capacity(state.blocks.len());
    for block in state.blocks {
        let mut rows = Vec::new();
        for row in block.rows {
            let kept = keeps(&step.filter, &row, policies, Operation::Test)
                .map_err(|undefined| widened(undefined, &read))?;
            if kept {
                rows.push(row);
            }
        }
        if !rows.is_empty() {
            blocks.push(Block::whole(rows));
        }
    }
    if !step.stages.distinct_by.is_empty() {
        blocks = distinct_by(blocks, &step.stages.distinct_by, fixed)?;
    }
    Ok(State {
        blocks,
        order: state.order,
        totals: false,
        tie_stop: None,
        unstated: state.unstated,
    })
}

/// The group key and the aggregates of a step, when no two of them share an output name: two
/// outputs under one name would lose one result before any comparison, and which one a written
/// key holds is no stated fact.
fn distinct_outputs(stages: &Stages) -> Result<(), Undefined> {
    let mut names: BTreeSet<&str> = stages.group_by.iter().map(String::as_str).collect();
    for aggregate in &stages.aggregates {
        if !names.insert(aggregate.name.as_str()) {
            return Err(unverified(format!(
                "two outputs share the name {}: which result a written key holds is not established",
                aggregate.name
            )));
        }
    }
    Ok(())
}

/// The output names of `aggregates` that are not stated, and their naming.
fn unstated(aggregates: &[Aggregate]) -> BTreeMap<String, Naming> {
    aggregates
        .iter()
        .filter(|aggregate| aggregate.naming != Naming::Stated)
        .map(|aggregate| (aggregate.name.clone(), aggregate.naming))
        .collect()
}

/// The rows of a step's grouping or totals, when it states one.
fn aggregated(state: State, stages: &Stages, policies: &Policies) -> Result<State, Undefined> {
    if stages.group_by.is_none() && stages.aggregates.is_empty() {
        return Ok(state);
    }
    distinct_outputs(stages)?;
    let rows: Vec<Row> = state.blocks.into_iter().flat_map(|b| b.rows).collect();
    let names = unstated(&stages.aggregates);
    if let Some(group) = &stages.group_by {
        // The runtime may meet a value that is no number in any group first.
        let groups = grouped(rows.clone(), group, &stages.aggregates, policies)
            .map_err(|undefined| widened(undefined, &rows))?;
        return Ok(State {
            blocks: vec![Block::whole(groups)],
            order: Order::Free,
            totals: false,
            tie_stop: None,
            unstated: names,
        });
    }
    let mut totals = Row::new();
    for aggregate in &stages.aggregates {
        let cell = aggregate_cell(aggregate, &rows, policies)?;
        totals.insert(aggregate.name.clone(), cell);
    }
    Ok(State {
        blocks: vec![Block::whole(vec![totals])],
        order: Order::File,
        totals: true,
        tie_stop: None,
        unstated: names,
    })
}

/// The rows of a step's sort and cut.
fn ordered(mut state: State, stages: &Stages, policies: &Policies) -> Result<State, Undefined> {
    if state.totals && (stages.sort.is_some() || stages.limit.is_some()) {
        return Err(unverified("a sort or a cut of totals is not evaluated"));
    }
    if let Some(sort) = &stages.sort {
        if sort.stable_ties && state.order != Order::File {
            return Err(unverified(
                "ties keep their file order, but an earlier stage already reordered the rows",
            ));
        }
        let rows: Vec<Row> = state.blocks.into_iter().flat_map(|b| b.rows).collect();
        state.blocks = sorted_blocks(&rows, sort, stages, policies)?;
        state.order = Order::Sorted {
            stable: sort.stable_ties,
        };
    }
    let Some(limit) = stages.limit else {
        return Ok(state);
    };
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    match state.order {
        Order::Sorted { stable } => {
            state.blocks = cut(state.blocks, limit, stable);
            let bound = stages
                .sort
                .as_ref()
                .filter(|sort| policies.contains_key(&sort.field));
            let through_distinct_ties = state.blocks.iter().any(|block| {
                block.partial()
                    && block
                        .rows
                        .iter()
                        .map(exact_row_form)
                        .collect::<BTreeSet<_>>()
                        .len()
                        > 1
            });
            state.tie_stop = bound
                .filter(|_| through_distinct_ties)
                .map(|sort| sort.field.clone());
        }
        Order::File => {
            let rows: Vec<Row> = state
                .blocks
                .into_iter()
                .flat_map(|b| b.rows)
                .take(limit)
                .collect();
            state.blocks = vec![Block::whole(rows)];
        }
        Order::Free => {
            let size: usize = state.blocks.iter().map(|b| b.rows.len()).sum();
            if size > limit {
                return Err(unverified(
                    "the first rows of rows whose order no request fixes",
                ));
            }
        }
    }
    Ok(state)
}

/// The rows a step writes: its projection, renames and removal of duplicates.
fn written(state: State, stages: &Stages) -> Result<State, Undefined> {
    let read: Vec<Row> = state
        .blocks
        .iter()
        .flat_map(|block| block.rows.iter().cloned())
        .collect();
    let mut blocks = Vec::with_capacity(state.blocks.len());
    for block in state.blocks {
        let take = block.take;
        let rows = if state.totals {
            block.rows
        } else {
            block
                .rows
                .into_iter()
                .map(|row| projected(row, stages))
                .collect::<Result<Vec<Row>, Undefined>>()
                .map_err(|undefined| widened(undefined, &read))?
        };
        blocks.push(Block { rows, take });
    }
    if stages.distinct {
        if blocks.iter().any(Block::partial) {
            return Err(unverified(
                "duplicates removed after a cut through tied rows",
            ));
        }
        blocks = without_duplicates(blocks)?;
    }
    let mut names = state.unstated;
    if !state.totals {
        if !stages.columns.is_empty() {
            names.retain(|name, _| stages.columns.contains(name));
        }
        // A rename's target is a name the request states.
        for (from, _) in &stages.renames {
            names.remove(from);
        }
    }
    Ok(State {
        blocks,
        order: state.order,
        totals: state.totals,
        tie_stop: state.tie_stop,
        unstated: names,
    })
}

fn apply(step: &Step, state: State, policies: &Policies) -> Result<State, Undefined> {
    let state = filtered(state, step, policies)?;
    let state = aggregated(state, &step.stages, policies)?;
    let state = ordered(state, &step.stages, policies)?;
    written(state, &step.stages)
}

/// The expected result of `pipeline` over `records`, the rows the run consumed in file order.
pub(crate) fn evaluate(pipeline: &Pipeline, records: Vec<Row>) -> Result<Expected, Undefined> {
    let mut state = State {
        blocks: vec![Block::whole(records)],
        order: Order::File,
        totals: false,
        tie_stop: None,
        unstated: BTreeMap::new(),
    };
    for step in &pipeline.steps {
        state = apply(step, state, &pipeline.policies)?;
    }
    Ok(Expected {
        blocks: state.blocks,
        order: state.order,
        totals: state.totals,
        tie_stop: state.tie_stop,
        unstated: state.unstated,
    })
}

/// Whether a stop is possible for `pipeline` on records the judgment could not evaluate: a
/// stated number policy, or a column written as a number.
pub(crate) fn may_stop(pipeline: &Pipeline) -> bool {
    !pipeline.policies.is_empty()
        || pipeline
            .steps
            .iter()
            .any(|step| !step.stages.number_columns.is_empty())
}
