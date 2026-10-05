// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A computation the reader states in pieces rather than as one rule. When the plan holds no
//! rule, or one rule beside other computing words (a drafting or classifying step, a
//! constraint that may select or shape rows), each of these steps and constraints is read
//! again as a sequence of pieces, each typed WHOLE by the reader's own rule grammar
//! (`rules::synthesize`), or a column computed from the row (`x equal to 12 minus stock`), or
//! function words alone. Every word of such a step belongs to a typed piece, or to the heading
//! before its colon, which stays an explicit unsupported obligation: no word is dropped to make
//! a reading fit, and a step with one untyped piece leaves the whole computation unsupported.
//! Two different readings with as few pieces leave it unsupported too.

use std::collections::BTreeMap;

use nika_compile_reader::aggregate::ArithOp;
use nika_compile_reader::plan::{Op, Plan};
use nika_compile_reader::rules::{self, Junction, NumberPolicy};
use nika_compile_reader::{shape, structure};
use serde_json::Value;

use super::numbers::Decimal;
use super::pipeline::{Arith, Derived, Filter, Pipeline, Stages, Step, Term};
use super::requested::pipeline_of_record;

type Answers = BTreeMap<String, String>;

/// The longest step this reading segments, in words.
const MOST_WORDS: usize = 64;

/// The computation composed from the pieces of the plan's steps.
#[derive(Clone, Debug)]
pub(crate) struct Composed {
    pub(crate) pipeline: Pipeline,
    /// The words of the composed steps, in order: where an order the request keeps is stated.
    pub(crate) text: String,
    /// Each composed step's heading before its colon, which states no piece: (step, words).
    pub(crate) headings: Vec<(usize, String)>,
}

/// One typed piece of a step.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Piece {
    /// Words that state no operation of their own (« et », « with »).
    Glue,
    /// A relation the reader's rule grammar types whole.
    Relation(Pipeline),
    /// A column computed from the row.
    Column(Derived),
}

/// The fewest pieces covering the words read so far.
#[derive(Clone, Debug)]
struct Best {
    cost: usize,
    pieces: Vec<Piece>,
    /// Another reading with as few pieces states a different relation.
    tied: bool,
}

/// A step whose words a relation may state: the reader typed it as model work or as a
/// computation without a rule. A retrieval, an extraction, a validation or an exploration
/// is never read as one.
pub(crate) fn composable(op: Op) -> bool {
    matches!(op, Op::Classify | Op::Draft | Op::Compute)
}

/// A constraint that states no rows: a prohibition, a context or structure law, a kept order.
pub(crate) fn neutral(constraint: &str) -> bool {
    shape::prohibits(constraint)
        || structure::binds_no_operation(constraint)
        || rules::keeps_order(constraint)
}

/// The computation the plan's computing steps and row-shaping constraints state together, when
/// each of their words is read. `None` when the plan holds an unknown or an obligation across
/// runs, when one rule stands alone (its own reading is the contract), when a rule's words lie
/// outside what is read, or when a step or a constraint leaves a word unread.
pub(crate) fn composed(plan: &Plan, answers: &Answers) -> Option<Composed> {
    if !plan.unknowns.is_empty() || !plan.obligations.is_empty() {
        return None;
    }
    let mut pieces = Vec::new();
    let mut headings = Vec::new();
    let mut text = Vec::new();
    for (at, step) in plan.steps.iter().enumerate() {
        match step.op {
            Op::Read => continue,
            op if !composable(op) || !step.categories.is_empty() => return None,
            _ => {}
        }
        let (heading, read) = step_pieces(&step.evidence, answers)?;
        if let Some(heading) = heading {
            headings.push((at, heading));
        }
        pieces.extend(read);
        text.push(step.evidence.as_str());
    }
    let shaping: Vec<&String> = plan.constraints.iter().filter(|c| !neutral(c)).collect();
    for constraint in &shaping {
        let (None, read) = step_pieces(constraint, answers)? else {
            return None;
        };
        pieces.extend(read);
        text.push(constraint.as_str());
    }
    // Every rule the reader typed is read again: a rule whose words no step or constraint holds
    // is read as a text of its own, never lost.
    let apart: Vec<&str> = (plan.rules.iter())
        .map(|rule| rule.text().trim())
        .filter(|words| !text.iter().any(|read| read.contains(words)))
        .collect();
    for words in apart {
        let (None, read) = step_pieces(words, answers)? else {
            return None;
        };
        pieces.extend(read);
        text.push(words);
    }
    // One rule alone in its own words: the reader's reading is already the contract.
    if plan.rules.len() == 1 && text.len() < 2 {
        return None;
    }
    let pipeline = merged(pieces)?;
    Some(Composed {
        pipeline,
        text: text.join(" "),
        headings,
    })
}

/// The pieces of one step: all its words, or the words after the colon that closes its
/// heading. A step with no typed piece states no computation.
fn step_pieces(evidence: &str, answers: &Answers) -> Option<(Option<String>, Vec<Piece>)> {
    let typed = |pieces: &[Piece]| pieces.iter().any(|piece| *piece != Piece::Glue);
    if let Some(pieces) = segmented(evidence, answers).filter(|pieces| typed(pieces)) {
        return Some((None, pieces));
    }
    let (heading, rest) = evidence.split_once(':')?;
    let pieces = segmented(rest, answers).filter(|pieces| typed(pieces))?;
    Some((Some(heading.trim().to_owned()), pieces))
}

/// The byte spans of the words of `text`.
fn spans(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = None;
    for (at, c) in text.char_indices() {
        match (c.is_whitespace(), start) {
            (true, Some(from)) => {
                out.push((from, at));
                start = None;
            }
            (false, None) => start = Some(at),
            _ => {}
        }
    }
    if let Some(from) = start {
        out.push((from, text.len()));
    }
    out
}

/// Whether a piece may end before word `at`: after a punctuation mark, or beside a function
/// word. A piece never ends inside a run of content words.
fn cut(text: &str, words: &[(usize, usize)], at: usize) -> bool {
    let word = |i: usize| {
        words
            .get(i)
            .and_then(|(from, to)| text.get(*from..*to))
            .unwrap_or_default()
    };
    let before = word(at.wrapping_sub(1));
    before.ends_with([',', ';', ':'])
        || structure::only_function_words(before)
        || structure::only_function_words(word(at))
}

/// The pieces covering every word of `text`, the fewest typed ones first; `None` when a word
/// stays unread or two readings with as few pieces state different relations.
fn segmented(text: &str, answers: &Answers) -> Option<Vec<Piece>> {
    let words = spans(text);
    let count = words.len();
    if count == 0 || count > MOST_WORDS {
        return None;
    }
    let mut best: Vec<Option<Best>> = vec![None; count + 1];
    best[0] = Some(Best {
        cost: 0,
        pieces: Vec::new(),
        tied: false,
    });
    for end in 1..=count {
        if end < count && !cut(text, &words, end) {
            continue;
        }
        for start in 0..end {
            if start > 0 && !cut(text, &words, start) {
                continue;
            }
            let Some(before) = best[start].clone() else {
                continue;
            };
            let (from, to) = (words[start].0, words[end - 1].1);
            let Some(piece) = text
                .get(from..to)
                .and_then(|words| piece_of(words, answers))
            else {
                continue;
            };
            best[end] = Some(better(best[end].take(), before, piece));
        }
    }
    let last = best[count].take()?;
    (!last.tied).then_some(last.pieces)
}

/// The fewer typed pieces of `held` and `before` followed by `piece`; at a tie, a different
/// relation marks the reading tied.
fn better(held: Option<Best>, before: Best, piece: Piece) -> Best {
    let cost = before.cost + usize::from(piece != Piece::Glue);
    let mut pieces = before.pieces;
    pieces.push(piece);
    let reading = Best {
        cost,
        pieces,
        tied: before.tied,
    };
    let Some(held) = held else {
        return reading;
    };
    let typed = |pieces: &[Piece]| -> Vec<Piece> {
        pieces
            .iter()
            .filter(|piece| **piece != Piece::Glue)
            .cloned()
            .collect()
    };
    match cost.cmp(&held.cost) {
        std::cmp::Ordering::Less => reading,
        std::cmp::Ordering::Greater => held,
        std::cmp::Ordering::Equal => {
            let differs = typed(&held.pieces) != typed(&reading.pieces);
            Best {
                tied: held.tied || reading.tied || differs,
                ..held
            }
        }
    }
}

/// What a run of words states as one piece, read whole.
fn piece_of(words: &str, answers: &Answers) -> Option<Piece> {
    let words = words
        .trim()
        .trim_matches(|c: char| matches!(c, ',' | ';' | ':' | '.'))
        .trim();
    if words.is_empty() {
        return None;
    }
    if structure::only_function_words(words) {
        return Some(Piece::Glue);
    }
    if let Some(column) = computed_column(words) {
        return Some(Piece::Column(column));
    }
    let rule = rules::synthesize(words, &[])?;
    if rule.verified_program().is_some() || rule.lines() || rule.joins() {
        return None;
    }
    let pipeline = pipeline_of_record(&rule.to_json(), answers).ok()?;
    (pipeline.steps.len() == 1).then_some(Piece::Relation(pipeline))
}

/// A number the request states, or one column name of the row.
fn term(word: &str) -> Option<Term> {
    if let Some(number) = Decimal::from_law(word) {
        return Some(Term::Number(number));
    }
    let column = !word.is_empty() && word.chars().all(|c| c.is_alphanumeric() || c == '_');
    column.then(|| Term::Column(word.to_owned()))
}

/// `name <equality> left <arithmetic> right`: the equality is the one the rule grammar reads
/// in `name <equality> left` (one clause, `name == left`, nothing else), the arithmetic one
/// the reader names; only a sum or a difference is computed.
fn computed_column(words: &str) -> Option<Derived> {
    let all: Vec<&str> = words.split_whitespace().collect();
    let [head @ .., left, op, right] = all.as_slice() else {
        return None;
    };
    let name = *head.first()?;
    if head.len() < 2 {
        return None;
    }
    let arith = match ArithOp::from_word(op)? {
        ArithOp::Sub => Arith::Sub,
        ArithOp::Add => Arith::Add,
        _ => return None,
    };
    let stated = rules::synthesize(&format!("{} {left}", head.join(" ")), &[])?.to_json();
    let [equality] = stated.get("clauses").and_then(Value::as_array)?.as_slice() else {
        return None;
    };
    let text = |key: &str| equality.get(key).and_then(Value::as_str);
    let shaped = stated
        .get("shape")
        .is_some_and(|shape| shape.get("sort_by").is_some_and(|sort| !sort.is_null()));
    if text("field") != Some(name)
        || text("comparator") != Some("==")
        || text("value") != Some(left)
        || shaped
    {
        return None;
    }
    Some(Derived::new(name, term(left)?, arith, term(right)?))
}

/// The relation the typed pieces state together: their filters joined (several pieces with
/// filters join by « and » only), at most one order, one projection, the computed columns in
/// order, and the number policies they agree on.
fn merged(pieces: Vec<Piece>) -> Option<Pipeline> {
    let mut tests = Vec::new();
    let mut junction = Junction::And;
    let mut filters = 0_usize;
    let mut stages = Stages::default();
    let mut policies: BTreeMap<String, NumberPolicy> = BTreeMap::new();
    let mut typed = false;
    for piece in pieces {
        match piece {
            Piece::Glue => {}
            Piece::Column(column) => {
                if stages.derived.iter().any(|held| held.name == column.name) {
                    return None;
                }
                stages.derived.push(column);
                typed = true;
            }
            Piece::Relation(pipeline) => {
                typed = true;
                for (field, policy) in pipeline.policies {
                    if policies
                        .insert(field, policy)
                        .is_some_and(|held| held != policy)
                    {
                        return None;
                    }
                }
                let [step] = pipeline.steps.as_slice() else {
                    return None;
                };
                if !step.filter.tests.is_empty() {
                    filters += 1;
                    if step.filter.junction == Junction::Or {
                        junction = Junction::Or;
                    }
                    tests.extend(step.filter.tests.iter().cloned());
                }
                merge_stages(&mut stages, &step.stages)?;
            }
        }
    }
    let reads_computed = tests
        .iter()
        .any(|test| stages.derived.iter().any(|d| d.name == test.field));
    if !typed || (filters > 1 && junction == Junction::Or) || reads_computed {
        return None;
    }
    let mut pipeline = Pipeline::new(vec![Step::new(Filter::new(tests, junction), stages)]);
    pipeline.policies = policies;
    Some(pipeline)
}

/// The order and the projection of one piece added to `stages`: a piece states nothing else,
/// and no two pieces state an order, or a projection.
fn merge_stages(stages: &mut Stages, piece: &Stages) -> Option<()> {
    let rest = Stages {
        sort: None,
        columns: Vec::new(),
        number_columns: Vec::new(),
        ..piece.clone()
    };
    if rest != Stages::default() {
        return None;
    }
    if let Some(sort) = &piece.sort {
        if stages.sort.is_some() {
            return None;
        }
        stages.sort = Some(sort.clone());
    }
    if !piece.columns.is_empty() {
        if !stages.columns.is_empty() {
            return None;
        }
        stages.columns.clone_from(&piece.columns);
        stages.number_columns.clone_from(&piece.number_columns);
    }
    Some(())
}
