// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A rule the request states in words ("keep only the rows whose amount is strictly
//! greater than 100", "cuya cantidad es menor que 10", "whose status is refunded") becomes
//! the jq the workflow runs, deterministically, so the compiler never asks for an
//! expression the request already states.
//!
//! The grammar is closed. A clause is a FIELD that names a column (an identifier token, a
//! word of the columns hint the request states, the noun phrase between a relative pronoun
//! and the comparison, or the word left of a comparison symbol), a COMPARATOR (a symbol or
//! a multilingual cue, with a copula for equality), and a VALUE (a number, a quoted or bare
//! word for equality, or a second column). Clauses join through one conjunction. Anything
//! the grammar does not cover is `None`: the human is asked, nothing is guessed. Every
//! expression shape emitted here was run on the engine's jq before it was written down.

use super::rule_tokens::{
    ATTEMPT_UNITS, Kind, SIZE_UNITS, Token, hinted, normalized, number, phrase, tokenize,
};
use serde_json::{Value, json};

pub use super::aggregate::{AggOp, Aggregation, ArithOp, Derived, Shape, Term};
use super::rule_cues::{
    ARTICLES, COPULAS, CUE_WIDTH, EQUALITY_CUES, FILLERS, NEGATED_COPULAS, NEGATIONS, NUMERIC_CUES,
    RELATIVES, SUMMARY_CORE, SUMMARY_WORDS, UNIT_PHRASES, UNIT_WORDS,
};

mod fields;
mod lines;
pub(crate) mod numbers;
mod record;
pub use lines::{by_construction_tail, line_filter};
pub use numbers::NumberPolicy;

/// Whether a constraint only says the rows keep their order (« garde l'ordre », « keep the
/// order », « en el mismo orden »): a computation that does not sort keeps the source order
/// by construction, and the compute task carries the constraint. Folded, six languages.
#[must_use]
pub fn keeps_order(text: &str) -> bool {
    let folded = super::hot::fold(text);
    let padded = format!(
        " {} ",
        folded
            .split(|c: char| !c.is_alphanumeric() && c != '\'')
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    );
    [
        " garde l'ordre ",
        " gardez l'ordre ",
        " conserve l'ordre ",
        " conservez l'ordre ",
        " dans l'ordre ",
        " meme ordre ",
        " keep the order ",
        " keeps the order ",
        " keeping the order ",
        " in order ",
        " in the same order ",
        " in the original order ",
        " same order ",
        " manten el orden ",
        " mantener el orden ",
        " mismo orden ",
        " mantieni l'ordine ",
        " stesso ordine ",
        " reihenfolge beibehalten ",
        " gleiche reihenfolge ",
        " mantem a ordem ",
        " mantenha a ordem ",
        " mesma ordem ",
    ]
    .iter()
    .any(|cue| padded.contains(cue))
}

/// The comparisons a rule may state: six over a value, three over the text of a line or a
/// column (starts with, contains, ends with) and their negations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Comparator {
    Gt,
    Ge,
    Lt,
    Le,
    Eq,
    Ne,
    StartsWith,
    NotStartsWith,
    Contains,
    NotContains,
    EndsWith,
    NotEndsWith,
}

impl Comparator {
    /// The comparator that holds exactly when this one does not.
    #[must_use]
    pub fn negated(self) -> Self {
        match self {
            Self::Gt => Self::Le,
            Self::Ge => Self::Lt,
            Self::Lt => Self::Ge,
            Self::Le => Self::Gt,
            Self::Eq => Self::Ne,
            Self::Ne => Self::Eq,
            Self::StartsWith => Self::NotStartsWith,
            Self::NotStartsWith => Self::StartsWith,
            Self::Contains => Self::NotContains,
            Self::NotContains => Self::Contains,
            Self::EndsWith => Self::NotEndsWith,
            Self::NotEndsWith => Self::EndsWith,
        }
    }

    /// A comparator named by a word or a symbol (`gt`, `>=`, `eq`, `<>`).
    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        match word.trim().to_ascii_lowercase().as_str() {
            ">" | "gt" | "greater" => Some(Self::Gt),
            ">=" | "≥" | "ge" | "gte" => Some(Self::Ge),
            "<" | "lt" | "less" => Some(Self::Lt),
            "<=" | "≤" | "le" | "lte" => Some(Self::Le),
            "==" | "=" | "eq" | "equals" => Some(Self::Eq),
            "!=" | "<>" | "≠" | "ne" => Some(Self::Ne),
            "startswith" | "starts_with" | "^=" => Some(Self::StartsWith),
            "!startswith" | "not_startswith" => Some(Self::NotStartsWith),
            "contains" | "*=" => Some(Self::Contains),
            "!contains" | "not_contains" => Some(Self::NotContains),
            "endswith" | "ends_with" | "$=" => Some(Self::EndsWith),
            "!endswith" | "not_endswith" => Some(Self::NotEndsWith),
            _ => None,
        }
    }
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Eq => "==",
            Self::Ne => "!=",
            Self::StartsWith => "startswith",
            Self::NotStartsWith => "!startswith",
            Self::Contains => "contains",
            Self::NotContains => "!contains",
            Self::EndsWith => "endswith",
            Self::NotEndsWith => "!endswith",
        }
    }
    /// The jq function of a text comparison and whether it is negated; `None` for a
    /// comparison over a value.
    const fn textual(self) -> Option<(&'static str, bool)> {
        match self {
            Self::StartsWith => Some(("startswith", false)),
            Self::NotStartsWith => Some(("startswith", true)),
            Self::Contains => Some(("contains", false)),
            Self::NotContains => Some(("contains", true)),
            Self::EndsWith => Some(("endswith", false)),
            Self::NotEndsWith => Some(("endswith", true)),
            Self::Gt | Self::Ge | Self::Lt | Self::Le | Self::Eq | Self::Ne => None,
        }
    }
    const fn numeric(self) -> bool {
        matches!(self, Self::Gt | Self::Ge | Self::Lt | Self::Le)
    }
}

fn listed(table: &str, word: &str) -> bool {
    table.split('|').any(|w| w == word)
}

fn cue_in(table: &[(Comparator, &str)], phrase: &str) -> Option<Comparator> {
    table
        .iter()
        .find(|(_, cues)| cues.split('|').any(|cue| cue == phrase))
        .map(|(comparator, _)| *comparator)
}

/// The comparator a folded phrase states as a numeric comparison, if any.
#[must_use]
pub fn numeric_cue(phrase: &str) -> Option<Comparator> {
    cue_in(NUMERIC_CUES, phrase)
}

/// Whether a text states a numeric comparison anywhere, typed or not (« above the agreed
/// threshold » compares to a value the grammar cannot type).
pub(crate) fn compares(text: &str) -> bool {
    let tokens = tokenize(text);
    (0..tokens.len()).any(|at| {
        (1..=CUE_WIDTH)
            .any(|width| phrase(&tokens, at, width).is_some_and(|p| numeric_cue(&p).is_some()))
    })
}

fn equality_cue(phrase: &str) -> Option<Comparator> {
    cue_in(EQUALITY_CUES, phrase)
}

/// The cue starting at `at`, longest first: the comparator and the width consumed.
fn cue_at(tokens: &[Token], at: usize) -> Option<(Comparator, usize)> {
    (1..=CUE_WIDTH).rev().find_map(|width| {
        let phrase = phrase(tokens, at, width)?;
        numeric_cue(&phrase)
            .or_else(|| equality_cue(&phrase))
            .map(|c| (c, width))
    })
}

/// A column-shaped identifier: letters, digits and underscores, and more than a plain
/// lowercase word (`total_eur`, `q1`, `unitPrice`).
pub(crate) fn identifier_shaped(word: &str) -> bool {
    let mut chars = word.chars();
    let starts = chars.next().is_some_and(|c| c.is_alphabetic() || c == '_');
    let joined = word.chars().all(|c| c.is_alphanumeric() || c == '_');
    let digit = word.chars().any(|c| c.is_ascii_digit());
    let letter = word.chars().any(char::is_alphabetic);
    let inner_upper = word.chars().skip(1).any(char::is_uppercase);
    let lower = word.chars().any(char::is_lowercase);
    starts && joined && (word.contains('_') || (digit && letter) || (inner_upper && lower))
}

/// A token that names a column: a hint column when a hint exists, else an identifier.
fn column_named(token: &Token, columns: &[String]) -> Option<String> {
    token.word()?;
    if columns.is_empty() {
        identifier_shaped(&token.original).then(|| token.original.clone())
    } else {
        hinted(&token.original, columns)
    }
}

// ── the rule ─────────────────────────────────────────────────────────────────────

/// What a field is compared to.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Operand {
    /// A number in canonical text, compared after `tonumber`.
    Number(String),
    /// An exact string, compared case-sensitively.
    Text(String),
    /// A truth value, matched whichever way the file encodes it (`false` or `"false"`).
    Bool(bool),
    /// Another column of the same record.
    Column(String),
    /// A value the request alludes to without stating it, read at run under
    /// `$in.slots.<slug>` from the const the compiler asked for.
    Slot(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Clause {
    pub field: String,
    pub comparator: Comparator,
    pub value: Operand,
    /// Other exact spellings a text equality also matches: the bounded canonical-spelling
    /// expansion the compiler grounds in observed values (R4 A5), never read from words.
    spellings: Vec<String>,
}

impl Clause {
    /// One clause of a typed rule: the column, the comparison and what it is compared to.
    #[must_use]
    pub fn new(field: impl Into<String>, comparator: Comparator, value: Operand) -> Self {
        Self {
            field: field.into(),
            comparator,
            value,
            spellings: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Junction {
    And,
    Or,
}

impl Junction {
    const fn word(self) -> &'static str {
        match self {
            Self::And => "and",
            Self::Or => "or",
        }
    }
}

/// A rule synthesized from the request: its clauses, how they join, and the text it
/// came from.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Rule {
    text: String,
    clauses: Vec<Clause>,
    junction: Junction,
    /// The text also asked how many rows were kept or what they add up to.
    summary: bool,
    /// What happens to the rows after the filter.
    shape: Shape,
    /// The records are the lines of a text source, and the result is written back as
    /// lines: a removal of duplicate lines over a `.txt` file.
    lines: bool,
    /// A program a seat wrote for a computation the typed stages cannot state, verified by
    /// the compiler on the seat's own example before it was bound: it runs verbatim over
    /// `.records`, and the columns it declares are the guard's fields.
    program: Option<Program>,
    /// The number policies the compiler stated from what it observed (R4 A5), by field.
    numbers: numbers::Numbers,
    /// The steps that run after the filter and the shape, in the order the request states.
    then: Vec<Then>,
}

/// One step a rule runs on the rows the step before it wrote, in the order the request states
/// it (R4 F5, V9 A10): its filter, then its stages, under the rule's one lowering.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Then {
    pub clauses: Vec<Clause>,
    pub junction: Junction,
    pub shape: Shape,
}

impl Then {
    /// One ordered step: the filter it applies and the stages after it.
    #[must_use]
    pub fn new(clauses: Vec<Clause>, junction: Junction, shape: Shape) -> Self {
        Self {
            clauses,
            junction,
            shape,
        }
    }
    fn to_json(&self) -> Value {
        let clauses: Vec<Value> = self.clauses.iter().map(Clause::to_json).collect();
        json!({"clauses": clauses, "junction": self.junction.word(), "shape": self.shape.to_json()})
    }
    fn from_json(value: &Value) -> Option<Self> {
        let clauses = value.get("clauses")?.as_array()?;
        let clauses = clauses
            .iter()
            .map(Clause::from_json)
            .collect::<Option<_>>()?;
        let junction = match value.get("junction")?.as_str()? {
            "and" => Junction::And,
            "or" => Junction::Or,
            _ => return None,
        };
        let shape = Shape::from_json(Some(value.get("shape")?))?;
        Some(Self::new(clauses, junction, shape))
    }
}

/// The filter a list of clauses states, joined by one junction.
fn predicate(clauses: &[Clause], junction: Junction, numbers: &numbers::Numbers) -> String {
    clauses
        .iter()
        .map(|clause| clause.jq(numbers))
        .collect::<Vec<_>>()
        .join(&format!(" {} ", junction.word()))
}

/// A verified program and the columns it reads.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Program {
    pub jq: String,
    pub columns: Vec<String>,
}

/// The jq path of one column: a bare identifier as `.name`, anything else bracketed.
pub(crate) fn key(field: &str) -> String {
    // The record itself: a line of a text source has no columns.
    if field == "." {
        return ".".to_owned();
    }
    let bare = field
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && field.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if bare {
        format!(".{field}")
    } else {
        format!(".[{}]", json!(field))
    }
}

impl Clause {
    fn jq(&self, numbers: &numbers::Numbers) -> String {
        let field = key(&self.field);
        let skip = |name: &str| numbers.get(name) == Some(&NumberPolicy::Skip);
        // A field with no stated policy reads as a recorded plan always read it (R4 A5).
        let law = |k: &str, name: &str| match numbers.get(name) {
            Some(_) => numbers::number(k, name),
            None => format!("({k} | tonumber)"),
        };
        let read = law(&field, &self.field);
        if let Operand::Slot(slug) = &self.value {
            let slot = format!("$in.slots.{slug}");
            if let Some((function, negated)) = self.comparator.textual() {
                let test = format!("({field} | tostring | {function}({slot} | tostring))");
                return if negated {
                    format!("({test} | not)")
                } else {
                    test
                };
            }
            return if self.comparator.numeric() {
                let (op, bound) = (self.comparator.symbol(), numbers.contains_key(&self.field));
                let test = numbers::compared(&read, op, &format!("({slot} | tonumber)"), bound);
                numbers::guarded(skip(&self.field), &field, test)
            } else {
                format!(
                    "({field} | tostring) {} ({slot} | tostring)",
                    self.comparator.symbol()
                )
            };
        }
        if let Some((function, negated)) = self.comparator.textual() {
            let literal = match &self.value {
                Operand::Number(text) | Operand::Text(text) => json!(text).to_string(),
                Operand::Bool(truth) => json!(truth.to_string()).to_string(),
                Operand::Column(other) => format!("({} | tostring)", key(other)),
                Operand::Slot(slug) => format!("($in.slots.{slug} | tostring)"),
            };
            let test = format!("({field} | tostring | {function}({literal}))");
            return if negated {
                format!("({test} | not)")
            } else {
                test
            };
        }
        match (&self.value, self.comparator.numeric()) {
            // A bound number compares with the literal as the request states it (R4 A8).
            (Operand::Number(n), _) => {
                let bound = numbers.contains_key(&self.field);
                let other = if bound {
                    json!(n).to_string()
                } else {
                    n.clone()
                };
                let test = numbers::compared(&read, self.comparator.symbol(), &other, bound);
                numbers::guarded(skip(&self.field), &field, test)
            }
            (Operand::Column(other), true) => {
                let right = key(other);
                let bound = numbers.contains_key(&self.field) || numbers.contains_key(other);
                let (op, other_law) = (self.comparator.symbol(), law(&right, other));
                let test = numbers::compared(&read, op, &other_law, bound);
                let test = numbers::guarded(skip(other), &right, test);
                numbers::guarded(skip(&self.field), &field, test)
            }
            (Operand::Column(other), false) => {
                format!("{field} {} {}", self.comparator.symbol(), key(other))
            }
            (Operand::Text(text), _) if !self.spellings.is_empty() => {
                let join = if self.comparator == Comparator::Ne {
                    " and "
                } else {
                    " or "
                };
                let arms: Vec<String> = std::iter::once(text)
                    .chain(&self.spellings)
                    .map(|s| format!("{field} {} {}", self.comparator.symbol(), json!(s)))
                    .collect();
                format!("({})", arms.join(join))
            }
            (Operand::Text(text), _) => {
                format!("{field} {} {}", self.comparator.symbol(), json!(text))
            }
            // A JSON file holds the boolean, a CSV its spelling: both are the same truth.
            // A slot is rendered before this match; the arm keeps it exhaustive.
            (Operand::Slot(slug), true) => {
                let test = format!(
                    "{read} {} ($in.slots.{slug} | tonumber)",
                    self.comparator.symbol()
                );
                numbers::guarded(skip(&self.field), &field, test)
            }
            (Operand::Slot(slug), false) => format!(
                "({field} | tostring) {} ($in.slots.{slug} | tostring)",
                self.comparator.symbol()
            ),
            (Operand::Bool(truth), _) => match self.comparator {
                Comparator::Eq => format!("({field} == {truth} or {field} == \"{truth}\")"),
                Comparator::Ne => format!("({field} != {truth} and {field} != \"{truth}\")"),
                other => format!("{field} {} {truth}", other.symbol()),
            },
        }
    }
    fn to_json(&self) -> Value {
        let (value, kind) = match &self.value {
            Operand::Number(n) => (n.clone(), "number"),
            Operand::Text(t) => (t.clone(), "text"),
            Operand::Bool(b) => (b.to_string(), "bool"),
            Operand::Column(c) => (c.clone(), "column"),
            Operand::Slot(s) => (s.clone(), "slot"),
        };
        let mut record = json!({"field": self.field, "comparator": self.comparator.symbol(), "value": value, "value_kind": kind});
        if !self.spellings.is_empty() {
            record["spellings"] = json!(self.spellings);
        }
        record
    }
    fn from_json(value: &Value) -> Option<Self> {
        // Spellings are grounded again from what is observed on every compile, never replayed.
        if value.get("spellings").is_some() {
            return None;
        }
        let field = value.get("field")?.as_str()?.trim().to_owned();
        let comparator = Comparator::from_word(value.get("comparator")?.as_str()?)?;
        let literal = value.get("value")?.as_str()?.to_owned();
        let operand = match value.get("value_kind").and_then(Value::as_str) {
            Some("number") if super::rule_tokens::recorded_number(&literal) => {
                Operand::Number(literal)
            }
            Some("bool") => Operand::Bool(literal == "true"),
            Some("column") => Operand::Column(literal),
            Some("slot") => Operand::Slot(literal),
            Some(_) => Operand::Text(literal), // faithful reports an unknown present kind
            None => return None,
        };
        if field.is_empty() {
            return None;
        }
        Some(Self {
            field,
            comparator,
            value: operand,
            spellings: Vec::new(),
        })
    }
}

impl Rule {
    /// A rule the semantic frontend stated as a typed predicate over the request's own
    /// columns and literals, validated by the compiler; lowered exactly like a parsed one.
    #[must_use]
    pub fn typed(text: &str, clauses: Vec<Clause>, junction: Junction, shape: Shape) -> Self {
        Self {
            text: text.to_owned(),
            clauses,
            junction,
            summary: false,
            shape,
            lines: false,
            program: None,
            numbers: numbers::Numbers::new(),
            then: Vec::new(),
        }
    }
    /// A rule whose computation is a verified program the seat wrote: no typed stage, the
    /// program itself over `.records`, its declared columns as fields.
    #[must_use]
    pub fn program(text: &str, jq: &str, columns: Vec<String>) -> Self {
        Self {
            text: text.to_owned(),
            clauses: Vec::new(),
            junction: Junction::And,
            summary: false,
            shape: Shape::default(),
            lines: false,
            program: Some(Program {
                jq: jq.to_owned(),
                columns,
            }),
            numbers: numbers::Numbers::new(),
            then: Vec::new(),
        }
    }
    /// The verified program the rule carries, when a seat wrote it.
    #[must_use]
    pub fn verified_program(&self) -> Option<&Program> {
        self.program.as_ref()
    }
    /// The columns the computation writes, in order, when it fixes them.
    #[must_use]
    pub fn output_columns(&self) -> Option<Vec<String>> {
        self.last_shape().output_columns()
    }
    /// The output keys the computation renames (source name, stated name).
    #[must_use]
    pub fn renames(&self) -> &[(String, String)] {
        &self.last_shape().renames
    }
    /// The steps that run after the filter and the shape, in the order the request states
    /// them (R4 F5): each on the rows the step before it wrote.
    #[must_use]
    pub fn then(&self) -> &[Then] {
        &self.then
    }
    /// Every step, the rule's own filter and shape first.
    pub(crate) fn steps(&self) -> impl Iterator<Item = (&[Clause], &Shape)> {
        let then = self.then.iter().map(|s| (s.clauses.as_slice(), &s.shape));
        std::iter::once((self.clauses.as_slice(), &self.shape)).chain(then)
    }
    /// The shape of the last step that shapes the rows: what the computation writes.
    fn last_shape(&self) -> &Shape {
        let mut shaped = self.then.iter().rev().map(|s| &s.shape);
        shaped
            .find(|s| **s != Shape::default())
            .unwrap_or(&self.shape)
    }
    /// A ranking (« the top-selling », « les plus vendus », « die meistverkauften ») sorted
    /// descending without the count of rows to keep: the count is asked, never assumed.
    #[must_use]
    pub fn ranking_without_count(&self) -> bool {
        self.shape.limit.is_none()
            && self.shape.sort_by.as_ref().is_some_and(|(_, desc)| *desc)
            && super::aggregate::ranking_cue(&self.text)
    }
    /// The same computation keeping the first `n` rows after its sort.
    #[must_use]
    pub fn with_limit(mut self, n: u32) -> Self {
        self.shape.limit = Some(n);
        self
    }
    /// Whether the rule joins several parsed sources on a column: its records are then one
    /// array per source, first source first.
    #[must_use]
    pub fn joins(&self) -> bool {
        self.shape.join_on.is_some()
    }
    /// Whether the rule runs over the lines of a text source.
    #[must_use]
    pub const fn lines(&self) -> bool {
        self.lines
    }
    /// The same rule over the lines of a text source, when its only work is the removal of
    /// duplicates or a filter on the line itself: a line has no columns to filter, group,
    /// sort or project. Anything else over a text source is `None`: the human is asked.
    #[must_use]
    pub fn over_lines(&self) -> Option<Self> {
        if self.lines {
            return Some(self.clone());
        }
        let s = &self.shape;
        let only_distinct = s.distinct
            && self.clauses.is_empty()
            && s.distinct_by.is_empty()
            && s.join_on.is_none()
            && s.group_by.is_none()
            && s.aggregations.is_empty()
            && s.sort_by.is_none()
            && s.limit.is_none()
            && s.columns.is_empty()
            && self.then.is_empty();
        only_distinct.then(|| Self {
            lines: true,
            ..self.clone()
        })
    }
    /// The names of the totals, when the computation is totals over every row.
    #[must_use]
    pub fn totals_names(&self) -> Vec<String> {
        self.last_shape().totals_names()
    }
    /// Whether the computation keeps or drops rows (a row filter), as opposed to a pure
    /// aggregation, sort or projection over every row.
    #[must_use]
    pub fn filters(&self) -> bool {
        self.steps().any(|(clauses, _)| !clauses.is_empty())
    }
    /// The excerpt the rule was read from.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
    /// Whether a stage follows the filter (a grouping, an aggregation, a sort, a
    /// projection, a limit, a rename or a join): a plain rule only keeps or drops rows.
    #[must_use]
    pub fn shaped(&self) -> bool {
        self.shape != Shape::default() || self.program.is_some() || !self.then.is_empty()
    }
    /// The inverse of [`Rule::to_json`], for a recorded plan replayed on an answer round.
    pub(crate) fn from_json(value: &Value) -> Option<Self> {
        // Number policies are grounded again from the answers on every compile, never replayed.
        if value.get("numbers").is_some() {
            return None;
        }
        let text = value.get("text")?.as_str()?.to_owned();
        let clauses = value
            .get("clauses")?
            .as_array()?
            .iter()
            .map(Clause::from_json)
            .collect::<Option<Vec<_>>>()?;
        let shape = Shape::from_json(Some(value.get("shape")?))?;
        let program = value.get("program").filter(|p| !p.is_null()).map(|p| {
            Some(Program {
                jq: p.get("jq")?.as_str()?.to_owned(),
                columns: p
                    .get("columns")?
                    .as_array()?
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect(),
            })
        });
        let program = match program {
            Some(program) => Some(program?),
            None => None,
        };
        let junction = match value.get("junction").and_then(Value::as_str) {
            Some("or") => Junction::Or,
            Some(_) => Junction::And, // faithful rejects unknown present values
            None if clauses.len() == 1 => Junction::And, // the original single-clause format
            None => return None,
        };
        let then = match value.get("then") {
            Some(steps) => steps
                .as_array()?
                .iter()
                .map(Then::from_json)
                .collect::<Option<_>>()?,
            None => Vec::new(),
        };
        let summary = value.get("summary")?.as_bool().unwrap_or(false);
        let lines = value.get("lines").and_then(Value::as_bool).unwrap_or(false);
        // An empty rule is the identity a conversion states, re-read by its binding law; a flag
        // over no clause and no stage is no complete rule.
        let empty = clauses.is_empty() && shape == Shape::default() && program.is_none();
        let empty = empty && then.is_empty();
        if empty && (summary || lines) {
            return None;
        }
        let rule = Self {
            text,
            clauses,
            junction,
            summary,
            shape,
            lines,
            program,
            numbers: numbers::Numbers::new(),
            then,
        };
        record::valid(&rule).then_some(rule)
    }
    /// Whether the text also asked for the count and totals the summary stage computes.
    #[must_use]
    pub const fn summary(&self) -> bool {
        self.summary
    }
    /// Every source column the rule reads, first use first: the join key, the clauses, the
    /// group column, the aggregated columns, a sort or a projection on a source column (a
    /// sort or a projection on a produced name reads nothing from the source).
    pub(crate) fn fields(&self) -> Vec<String> {
        if let Some(program) = &self.program {
            return program.columns.clone();
        }
        let mut out: Vec<String> = Vec::new();
        let mut push = |name: &str| {
            if !out.iter().any(|f| f == name) {
                out.push(name.to_owned());
            }
        };
        if let Some(key) = &self.shape.join_on {
            push(key);
        }
        for (clauses, shape) in self.steps() {
            for key in &shape.distinct_by {
                push(key);
            }
            for clause in clauses {
                if clause.field != "." {
                    push(&clause.field);
                }
                if let Operand::Column(other) = &clause.value {
                    push(other);
                }
            }
            let produced = shape.produced();
            if let Some(group) = &shape.group_by {
                push(group);
            }
            for aggregation in &shape.aggregations {
                if let Some(field) = &aggregation.field {
                    push(field);
                }
            }
            if let Some((field, _)) = &shape.sort_by
                && !produced.contains(&field.as_str())
            {
                push(field);
            }
            if produced.is_empty() {
                for column in &shape.columns {
                    push(column);
                }
            }
        }
        out
    }
    fn predicate(&self) -> String {
        predicate(&self.clauses, self.junction, &self.numbers)
    }
    /// The computation over the parsed records: the join of the sources when the rule joins,
    /// the filter, then the shape's stages in their fixed order; over the lines of a text
    /// source, the result is written back as lines with the file's final newline.
    #[must_use]
    pub fn jq(&self) -> String {
        if let Some(program) = &self.program {
            return program.jq.clone();
        }
        let base = match &self.shape.join_on {
            Some(on) => format!(
                ".records | reduce .[1:][] as $right (.[0]; [.[] as $a | $right[] | select({k} == ($a | {k})) | $a + .])",
                k = key(on)
            ),
            None => ".records".to_owned(),
        };
        let filtered = if self.clauses.is_empty() {
            base
        } else if self.shape.join_on.is_some() {
            format!("{base} | [.[] | select({})]", self.predicate())
        } else {
            format!("[.records[] | select({})]", self.predicate())
        };
        let mut jq = self.shape.lower(filtered, &self.numbers);
        for step in &self.then {
            if !step.clauses.is_empty() {
                let kept = predicate(&step.clauses, step.junction, &self.numbers);
                jq = format!("{jq} | map(select({kept}))");
            }
            jq = step.shape.lower(jq, &self.numbers);
        }
        if self.lines {
            jq.push_str(" | join(\"\\n\") | if length > 0 then . + \"\\n\" else . end");
        }
        if self.slots().is_empty() {
            jq
        } else {
            // The slots ride the input beside the records: `$in` keeps them in reach
            // inside the filter, where `.` is one record.
            format!(". as $in | {jq}")
        }
    }
    /// The slugs of the slots the clauses compare to, in clause order.
    #[must_use]
    pub fn slots(&self) -> Vec<String> {
        self.steps()
            .flat_map(|(clauses, _)| clauses)
            .filter_map(|c| match &c.value {
                Operand::Slot(slug) => Some(slug.clone()),
                _ => None,
            })
            .collect()
    }
    /// Whether the comparison is over numbers.
    #[must_use]
    pub const fn compares_numbers(comparator: Comparator) -> bool {
        comparator.numeric()
    }
    /// True when the records are an array whose first record carries every column the
    /// rule reads (an empty array passes): a wrong column fails loudly, never filters
    /// everything in silence. A join judges every source's first record; lines are strings.
    #[must_use]
    pub fn guard(&self) -> String {
        if self.lines {
            return "(.records | type) == \"array\" and all(.records[]; type == \"string\")"
                .to_owned();
        }
        let has = self
            .fields()
            .iter()
            .filter(|f| self.numbers.get(*f) != Some(&NumberPolicy::Skip))
            .map(|f| format!(" and has({})", json!(f)))
            .collect::<Vec<_>>()
            .concat();
        if self.joins() {
            return format!(
                "(.records | type) == \"array\" and (.records | length) >= 2 and all(.records[]; type == \"array\" and (length == 0 or (.[0] | type == \"object\"{has})))"
            );
        }
        format!(
            "(.records | type) == \"array\" and ((.records | length) == 0 or (.records[0] | type == \"object\"{has}))"
        )
    }
    #[must_use]
    pub fn guard_message(&self) -> String {
        if self.lines {
            return format!(
                "The rule `{}` runs over the lines of the source, but the source was not read as lines.",
                self.text
            );
        }
        let fields = self
            .fields()
            .iter()
            .map(|f| format!("`{f}`"))
            .collect::<Vec<_>>()
            .join(", ");
        if self.joins() {
            return format!(
                "The rule `{}` joins the sources on {fields}, but a source is missing or its first parsed record has no such field; check every source header.",
                self.text
            );
        }
        format!(
            "The rule `{}` reads the column(s) {fields}, but the first parsed record has no such field; check the source header.",
            self.text
        )
    }
    /// The provenance record: observational, never authority.
    pub fn to_json(&self) -> Value {
        let mut record = json!({
            "text": self.text,
            "fields": self.fields(),
            "clauses": self.clauses.iter().map(Clause::to_json).collect::<Vec<_>>(),
            "jq": self.jq(),
            "synthesized": true,
            "summary": self.summary,
            "junction": self.junction.word(),
            "shape": self.shape.to_json(),
            "lines": self.lines,
            "program": self.program.as_ref().map(|p| json!({"jq": p.jq, "columns": p.columns})),
        });
        if let [only] = self.clauses.as_slice() {
            let clause = only.to_json();
            record["field"] = clause["field"].clone();
            record["comparator"] = clause["comparator"].clone();
            record["value"] = clause["value"].clone();
        }
        if !self.then.is_empty() {
            record["then"] = self.then.iter().map(Then::to_json).collect();
        }
        if !self.numbers.is_empty() {
            record["numbers"] = self
                .numbers
                .iter()
                .map(|(f, p)| (f.clone(), json!(p.word())))
                .collect();
        }
        record
    }
}

// ── parsing ──────────────────────────────────────────────────────────────────────

fn junction_of(token: &Token) -> Option<Junction> {
    match token.word()? {
        "and" | "et" | "y" | "e" | "und" => Some(Junction::And),
        "or" | "ou" | "o" | "oder" => Some(Junction::Or),
        _ => None,
    }
}

/// A trailing count-or-total request ("how many rows were kept and the total of their
/// amounts"): every word is a summary word, a junction or a column of the hint (singular
/// or plural), and at least one asks for a count or a total.
fn summary_residual(tokens: &[Token], from: usize, columns: &[String]) -> bool {
    let rest = tokens.get(from..).unwrap_or_default();
    let mut core = false;
    for token in rest {
        let Some(word) = token.word() else {
            return false;
        };
        if listed(SUMMARY_CORE, word) {
            core = true;
            continue;
        }
        let column = columns.iter().any(|c| {
            let name = normalized(c);
            word == name || word == format!("{name}s") || word == format!("{name}es")
        });
        if !(listed(SUMMARY_WORDS, word) || junction_of(token).is_some() || column) {
            return false;
        }
    }
    core
}

/// Where a clause's comparison sits: the field ends before `field_end`, the comparator
/// (when the anchor is a copula, whatever follows it) starts at `value_from`.
struct Anchor {
    comparator: Option<Comparator>,
    negated: bool,
    symbol: bool,
    field_end: usize,
    value_from: usize,
}

fn anchor_at(tokens: &[Token], at: usize) -> Option<Anchor> {
    let token = tokens.get(at)?;
    if let Kind::Symbol(comparator) = token.kind {
        return Some(Anchor {
            comparator: Some(comparator),
            negated: false,
            symbol: true,
            field_end: at,
            value_from: at + 1,
        });
    }
    if let Some((comparator, width)) = cue_at(tokens, at) {
        return Some(Anchor {
            comparator: Some(comparator),
            negated: false,
            symbol: false,
            field_end: at,
            value_from: at + width,
        });
    }
    let word = token.word()?;
    let negated_copula = NEGATED_COPULAS.contains(&word);
    if !negated_copula && !COPULAS.contains(&word) {
        return None;
    }
    let negation = |i: usize| {
        tokens
            .get(i)
            .and_then(Token::word)
            .is_some_and(|w| NEGATIONS.contains(&w))
    };
    let before = at > 0 && negation(at - 1);
    let after = negation(at + 1);
    Some(Anchor {
        comparator: None,
        negated: negated_copula || before || after,
        symbol: false,
        field_end: if before { at - 1 } else { at },
        value_from: if after { at + 2 } else { at + 1 },
    })
}

/// The comparator a copula anchor states: a cue or symbol after it, else equality.
fn comparator_after(tokens: &[Token], anchor: &Anchor) -> Option<(Comparator, usize)> {
    if let Some(comparator) = anchor.comparator {
        return Some((comparator, anchor.value_from));
    }
    if let Some((comparator, width)) = cue_at(tokens, anchor.value_from) {
        return (!anchor.negated).then_some((comparator, anchor.value_from + width));
    }
    if let Some(Kind::Symbol(comparator)) = tokens.get(anchor.value_from).map(|t| t.kind.clone()) {
        return (!anchor.negated).then_some((comparator, anchor.value_from + 1));
    }
    let comparator = if anchor.negated {
        Comparator::Ne
    } else {
        Comparator::Eq
    };
    Some((comparator, anchor.value_from))
}

/// What the words left of the comparison name.
enum Left {
    Column(String),
    /// Nothing names a column there; the noun after the number may.
    Unnamed,
}

/// The last relative marker in a region: its index and width. A word inside a wider marker
/// just found (« cui » of « la cui ») is that marker, never a second one after it.
fn last_relative(region: &[Token]) -> Option<(usize, usize)> {
    let mut found: Option<(usize, usize)> = None;
    for at in 0..region.len() {
        if found.is_some_and(|(start, width)| at < start + width) {
            continue;
        }
        for width in [2, 1] {
            if phrase(region, at, width).is_some_and(|p| RELATIVES.contains(&p.as_str())) {
                found = Some((at, width));
                break;
            }
        }
    }
    found
}

/// A negation among the words that lead a clause ("do not keep the tickets whose status
/// is closed", "never keep …", "ne garde pas …") inverts the whole clause; the grammar
/// reads no polarity there, so it reads nothing. The French restriction "ne … que" is
/// "only", never a negation.
/// A verb that drops the rows it describes ("exclude the rows whose …", "filter out …",
/// "supprime les lignes dont …"): the clauses name what leaves, and the grammar reads no
/// polarity there. Reading them as a keep would run the complement of the request.
const EXCLUSION_LEADS: &str = include_str!("../assets/exclusion_leads.txt");

/// Whether a folded word is one of the exclusion leads.
pub(crate) fn exclusion_lead(word: &str) -> bool {
    EXCLUSION_LEADS.lines().any(|lead| lead == word)
}

fn negated_lead(lead: &[Token]) -> bool {
    let words: Vec<&str> = lead.iter().filter_map(Token::word).collect();
    words.iter().enumerate().any(|(at, word)| {
        if matches!(*word, "ne" | "n") {
            return !words
                .get(at + 1..(at + 4).min(words.len()))
                .is_some_and(|window| window.contains(&"que"));
        }
        NEGATIONS.contains(word)
            || exclusion_lead(word)
            || matches!(
                *word,
                "never"
                    | "jamais"
                    | "nunca"
                    | "mai"
                    | "niemals"
                    | "nie"
                    | "don't"
                    | "doesn't"
                    | "won't"
                    | "isn't"
                    | "aren't"
            )
    })
}

/// What the words before a clause's field state (R4 F1).
enum Lead {
    /// Nothing the filter drops: the clause's own verb, the grammar's words, the rows' noun.
    Plain,
    /// A count or an aggregate the stage grammar reads whole over the rows the clause keeps.
    Stage(Box<Shape>),
}

/// The words left of the comparison: the lead (before a relative marker, or before the last
/// word when none) and the phrase naming the field, and whether a relative marker split them.
fn split_region<'a>(
    tokens: &'a [Token],
    from: usize,
    anchor: &Anchor,
) -> (&'a [Token], &'a [Token], bool) {
    let region = tokens.get(from..anchor.field_end).unwrap_or_default();
    match last_relative(region) {
        Some((at, width)) => (
            region.get(..at).unwrap_or_default(),
            region.get(at + width..).unwrap_or_default(),
            true,
        ),
        None => match region.split_last() {
            Some((last, lead)) => (lead, std::slice::from_ref(last), false),
            None => (region, region, false),
        },
    }
}

/// How a clause's lead is read (R4 F1): a count or an aggregate the stage grammar reads whole
/// over the rows a relative clause keeps runs after the filter (« count the rows where … »);
/// another word stating a stage of its own, or a word that is no function word after the first
/// one (a modifier: « the paid rows where … », « les lignes payées dont … »), leaves the clause
/// unread (`None`), never read without it. The clause's own verb (« filter », « show me »), the
/// grammar's function words and the rows' noun state nothing the filter drops.
fn lead_reading(lead: &[Token], relative: bool, columns: &[String]) -> Option<Lead> {
    let words: Vec<&str> = lead.iter().filter_map(Token::word).collect();
    if words.is_empty() {
        return Some(Lead::Plain);
    }
    if relative {
        let text = lead
            .iter()
            .map(|t| t.original.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        if let Some(stage) = super::stages::lead_stage(&text, columns) {
            return Some(Lead::Stage(Box::new(stage)));
        }
    }
    if words
        .iter()
        .any(|w| super::stages::operation_word(w) || listed(SUMMARY_CORE, w))
    {
        return None;
    }
    let function = |w: &&str| super::stages::lead_word(w) || ARTICLES.contains(w);
    let noun = words.len() - usize::from(relative);
    let first = words.iter().position(function).unwrap_or(noun);
    let between = words.get(first..noun).unwrap_or_default();
    between.iter().all(function).then_some(Lead::Plain)
}

/// The field named left of the comparison, from the phrase [`split_region`] cut; `Unnamed`
/// when nothing there names a column.
fn left_field(
    phrase: &[Token],
    relative: bool,
    anchor: &Anchor,
    columns: &[String],
) -> Option<Left> {
    let mut phrase = phrase;
    while let Some((head, rest)) = phrase.split_first()
        && head.word().is_some_and(|w| ARTICLES.contains(&w))
    {
        phrase = rest;
    }
    if phrase.is_empty() {
        return Some(Left::Unnamed);
    }
    if phrase.len() > 3 || phrase.iter().any(|t| t.word().is_none()) {
        return if relative { None } else { Some(Left::Unnamed) };
    }
    let name = phrase
        .iter()
        .map(|t| t.original.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    if !columns.is_empty() {
        return match hinted(&name, columns) {
            Some(column) => Some(Left::Column(column)),
            None if relative => None,
            None => Some(Left::Unnamed),
        };
    }
    let column = relative || anchor.symbol || (phrase.len() == 1 && identifier_shaped(&name));
    Some(if column {
        Left::Column(name)
    } else {
        Left::Unnamed
    })
}

/// The value right of the comparator and the index after it.
fn parse_value(
    tokens: &[Token],
    from: usize,
    comparator: Comparator,
    columns: &[String],
) -> Option<(Operand, usize)> {
    let mut at = from;
    while tokens
        .get(at)
        .and_then(Token::word)
        .is_some_and(|w| FILLERS.contains(&w))
    {
        at += 1;
    }
    let token = tokens.get(at)?;
    let operand = match &token.kind {
        Kind::Number(n) => Operand::Number(n.clone()),
        Kind::Quoted if comparator.numeric() => Operand::Number(number(&token.original)?),
        Kind::Quoted => Operand::Text(token.original.clone()),
        Kind::Word if comparator.numeric() => Operand::Column(column_named(token, columns)?),
        Kind::Word => match hinted(&token.original, columns) {
            Some(column) => Operand::Column(column),
            None => Operand::Text(token.original.clone()),
        },
        Kind::Symbol(_) => return None,
    };
    Some((operand, at + 1))
}

/// A number bounded by a size, attempt or turn unit is prose or a loop bound, not data.
fn unit_after(tokens: &[Token], at: usize) -> bool {
    let unit = |i: usize| {
        tokens
            .get(i)
            .and_then(Token::word)
            .is_some_and(|w| SIZE_UNITS.contains(&w) || ATTEMPT_UNITS.contains(&w))
    };
    let bridge = tokens
        .get(at)
        .and_then(Token::word)
        .is_some_and(|w| matches!(w, "a" | "de" | "di" | "of"));
    unit(at) || (bridge && unit(at + 1))
}

/// After the value: unit words, a verb-final copula, then the end or a junction.
fn residual(tokens: &[Token], from: usize) -> Option<usize> {
    let mut at = from;
    loop {
        let Some(token) = tokens.get(at) else {
            return Some(at);
        };
        if junction_of(token).is_some() {
            return Some(at);
        }
        if phrase(tokens, at, 2).is_some_and(|p| UNIT_PHRASES.contains(&p.as_str())) {
            at += 2;
            continue;
        }
        let trailing = token
            .word()
            .is_some_and(|w| UNIT_WORDS.lines().any(|u| u == w) || COPULAS.contains(&w));
        if !trailing {
            return None;
        }
        at += 1;
    }
}

/// One clause from `from`: the clause, the index of the token after it and what its lead
/// states. `None` when the lead carries a number, a symbol, a quote, a negation or words the
/// grammar cannot account for.
fn parse_clause(
    tokens: &[Token],
    from: usize,
    columns: &[String],
) -> Option<(Clause, usize, Lead)> {
    let anchor = (from..tokens.len()).find_map(|at| anchor_at(tokens, at))?;
    let (comparator, value_from) = comparator_after(tokens, &anchor)?;
    let (lead, phrase, relative) = split_region(tokens, from, &anchor);
    if lead.iter().any(|t| t.word().is_none()) || negated_lead(lead) {
        return None;
    }
    let lead = lead_reading(lead, relative, columns)?;
    let left = left_field(phrase, relative, &anchor, columns)?;
    let (value, mut next) = parse_value(tokens, value_from, comparator, columns)?;
    let numeric_value = matches!(value, Operand::Number(_));
    if numeric_value && unit_after(tokens, next) {
        return None;
    }
    let field = match left {
        Left::Column(field) => field,
        Left::Unnamed => {
            if !numeric_value {
                return None;
            }
            let field = column_named(tokens.get(next)?, columns)?;
            next += 1;
            field
        }
    };
    if let Operand::Column(other) = &value
        && *other == field
    {
        return None;
    }
    let next = residual(tokens, next)?;
    Some((Clause::new(field, comparator, value), next, lead))
}

/// Sentences and `;`-joined rules, each of which must parse whole.
fn segments(text: &str) -> impl Iterator<Item = &str> {
    text.split(';')
        .flat_map(|part| part.split(". "))
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// What one segment states: clauses and their junction, the stage its lead or its whole words
/// state, and whether a count-or-total request trails its clauses.
#[derive(Default)]
struct Segment {
    clauses: Vec<Clause>,
    junction: Option<Junction>,
    stage: Option<Shape>,
    summary: bool,
}

/// One segment read whole, or `None` when any part is outside the grammar: clauses joined by
/// one junction, the stage the first clause's lead states running after them, or a whole
/// segment stating a stage (an aggregate, a count per column, a sort, a top-N, a projection, a
/// removal of duplicates, a join).
fn read_segment(
    tokens: &[Token],
    segment: &str,
    columns: &[String],
    read_before: bool,
) -> Option<Segment> {
    let mut read = Segment::default();
    let mut at = 0;
    loop {
        let Some((clause, next, lead)) = parse_clause(tokens, at, columns) else {
            if at == 0
                && let Some(stage) = super::stages::stated(segment, columns)
            {
                read.stage = Some(stage);
                return Some(read);
            }
            // After a clause, a count-or-total request is the summary stage's work.
            let trailing = (at > 0 || read_before) && read.junction != Some(Junction::Or);
            read.summary = trailing && summary_residual(tokens, at, columns);
            return read.summary.then_some(read);
        };
        // The stage the first clause's lead states runs after every clause of its segment.
        if let Lead::Stage(stage) = lead {
            if at != 0 {
                return None;
            }
            read.stage = Some(*stage);
        }
        // The same clause twice (a promoted constraint beside the seat's paraphrase of it) is
        // one clause.
        if !read.clauses.contains(&clause) {
            read.clauses.push(clause);
        }
        let Some(token) = tokens.get(next) else {
            return Some(read);
        };
        let joined = junction_of(token)?;
        if read.junction.is_some_and(|j| j != joined) {
            return None;
        }
        read.junction = Some(joined);
        at = next + 1;
        if at >= tokens.len() {
            return None;
        }
    }
}

/// The rule the text states, or `None` when any part is outside the grammar. Its segments keep
/// the order the request states (R4 F5): each one's clauses and stage go where
/// `stages::place_clauses` and `stages::place_stage` put them, in one step or a later one.
#[must_use]
pub fn synthesize(text: &str, columns: &[String]) -> Option<Rule> {
    let text = text.trim();
    // « the lines that start with # »: a filter on the line itself, over a text source.
    if let Some(rule) = line_filter(text) {
        return Some(rule);
    }
    let mut steps = vec![super::stages::Step::default()];
    let mut summary = false;
    for segment in segments(text) {
        let tokens = tokenize(segment);
        if tokens.is_empty() {
            continue;
        }
        // Clauses joined by « or » admit no later segment: their junctions would mix.
        if steps.iter().any(|s| s.junction == Some(Junction::Or)) {
            return None;
        }
        let read_before = steps.iter().any(|s| !s.clauses.is_empty());
        let read = read_segment(&tokens, segment, columns, read_before)?;
        summary |= read.summary;
        if !read.clauses.is_empty() {
            super::stages::place_clauses(&mut steps, read.clauses, read.junction)?;
        }
        if let Some(stage) = read.stage {
            super::stages::place_stage(&mut steps, stage)?;
        }
    }
    let mut steps = steps.into_iter();
    let first = steps.next()?;
    let and = |junction: Option<Junction>| junction.unwrap_or(Junction::And);
    let then: Vec<Then> = steps
        .map(|s| Then::new(s.clauses, and(s.junction), s.shape))
        .collect();
    if first.clauses.is_empty() && first.shape == Shape::default() && then.is_empty() {
        return None;
    }
    Some(Rule {
        text: text.to_owned(),
        clauses: first.clauses,
        junction: and(first.junction),
        summary,
        shape: first.shape,
        lines: false,
        program: None,
        numbers: numbers::Numbers::new(),
        then,
    })
}

#[cfg(test)]
mod tests;
