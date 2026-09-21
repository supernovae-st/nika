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

use super::rule_tokens::{Kind, Token, number, phrase, tokenize};
use super::shape::{ATTEMPT_UNITS, SIZE_UNITS, fold};
use serde_json::{Value, json};

pub(super) use super::aggregate::{AggOp, Aggregation, Shape};
use super::rule_cues::{
    ARTICLES, COPULAS, CUE_WIDTH, EQUALITY_CUES, FILLERS, NEGATED_COPULAS, NEGATIONS, NUMERIC_CUES,
    RELATIVES, SUMMARY_CORE, SUMMARY_WORDS, UNIT_PHRASES, UNIT_WORDS,
};

/// The six comparisons a rule may state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Comparator {
    Gt,
    Ge,
    Lt,
    Le,
    Eq,
    Ne,
}

impl Comparator {
    /// The comparator that holds exactly when this one does not.
    pub(super) fn negated(self) -> Self {
        match self {
            Self::Gt => Self::Le,
            Self::Ge => Self::Lt,
            Self::Lt => Self::Ge,
            Self::Le => Self::Gt,
            Self::Eq => Self::Ne,
            Self::Ne => Self::Eq,
        }
    }

    /// A comparator named by a word or a symbol (`gt`, `>=`, `eq`, `<>`).
    pub(super) fn from_word(word: &str) -> Option<Self> {
        match word.trim().to_ascii_lowercase().as_str() {
            ">" | "gt" | "greater" => Some(Self::Gt),
            ">=" | "≥" | "ge" | "gte" => Some(Self::Ge),
            "<" | "lt" | "less" => Some(Self::Lt),
            "<=" | "≤" | "le" | "lte" => Some(Self::Le),
            "==" | "=" | "eq" | "equals" => Some(Self::Eq),
            "!=" | "<>" | "≠" | "ne" => Some(Self::Ne),
            _ => None,
        }
    }
    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Eq => "==",
            Self::Ne => "!=",
        }
    }
    const fn numeric(self) -> bool {
        !matches!(self, Self::Eq | Self::Ne)
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
pub(super) fn numeric_cue(phrase: &str) -> Option<Comparator> {
    cue_in(NUMERIC_CUES, phrase)
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
pub(super) fn identifier_shaped(word: &str) -> bool {
    let mut chars = word.chars();
    let starts = chars.next().is_some_and(|c| c.is_alphabetic() || c == '_');
    let joined = word.chars().all(|c| c.is_alphanumeric() || c == '_');
    let digit = word.chars().any(|c| c.is_ascii_digit());
    let letter = word.chars().any(char::is_alphabetic);
    let inner_upper = word.chars().skip(1).any(char::is_uppercase);
    let lower = word.chars().any(char::is_lowercase);
    starts && joined && (word.contains('_') || (digit && letter) || (inner_upper && lower))
}

fn normalized(name: &str) -> String {
    fold(name).replace([' ', '-'], "_")
}

/// The hint column a name designates, in the hint's own spelling.
fn hinted(name: &str, columns: &[String]) -> Option<String> {
    let wanted = normalized(name);
    columns.iter().find(|c| normalized(c) == wanted).cloned()
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
pub(super) enum Operand {
    /// A number in canonical text, compared after `tonumber`.
    Number(String),
    /// An exact string, compared case-sensitively.
    Text(String),
    /// Another column of the same record.
    Column(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Clause {
    pub field: String,
    pub comparator: Comparator,
    pub value: Operand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Junction {
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
pub(super) struct Rule {
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
}

/// The jq path of one column: a bare identifier as `.name`, anything else bracketed.
pub(super) fn key(field: &str) -> String {
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
    fn jq(&self) -> String {
        let field = key(&self.field);
        match (&self.value, self.comparator.numeric()) {
            (Operand::Number(n), _) => {
                format!("({field} | tonumber) {} {n}", self.comparator.symbol())
            }
            (Operand::Column(other), true) => format!(
                "({field} | tonumber) {} ({} | tonumber)",
                self.comparator.symbol(),
                key(other)
            ),
            (Operand::Column(other), false) => {
                format!("{field} {} {}", self.comparator.symbol(), key(other))
            }
            (Operand::Text(text), _) => {
                format!("{field} {} {}", self.comparator.symbol(), json!(text))
            }
        }
    }
    fn to_json(&self) -> Value {
        let (value, kind) = match &self.value {
            Operand::Number(n) => (n, "number"),
            Operand::Text(t) => (t, "text"),
            Operand::Column(c) => (c, "column"),
        };
        json!({"field": self.field, "comparator": self.comparator.symbol(), "value": value, "value_kind": kind})
    }
    fn from_json(value: &Value) -> Option<Self> {
        let field = value.get("field")?.as_str()?.trim().to_owned();
        let comparator = Comparator::from_word(value.get("comparator")?.as_str()?)?;
        let literal = value.get("value")?.as_str()?.to_owned();
        let operand = match value.get("value_kind").and_then(Value::as_str) {
            Some("number") => Operand::Number(literal),
            Some("column") => Operand::Column(literal),
            _ => Operand::Text(literal),
        };
        if field.is_empty() {
            return None;
        }
        Some(Self {
            field,
            comparator,
            value: operand,
        })
    }
}

impl Rule {
    /// A rule the semantic frontend stated as a typed predicate over the request's own
    /// columns and literals, validated by the compiler; lowered exactly like a parsed one.
    pub(super) fn typed(
        text: &str,
        clauses: Vec<Clause>,
        junction: Junction,
        shape: Shape,
    ) -> Self {
        Self {
            text: text.to_owned(),
            clauses,
            junction,
            summary: false,
            shape,
            lines: false,
        }
    }
    /// The columns the computation writes, in order, when it fixes them.
    pub(super) fn output_columns(&self) -> Option<Vec<String>> {
        self.shape.output_columns()
    }
    /// Whether the rule joins several parsed sources on a column: its records are then one
    /// array per source, first source first.
    pub(super) fn joins(&self) -> bool {
        self.shape.join_on.is_some()
    }
    /// Whether the rule runs over the lines of a text source.
    pub(super) const fn lines(&self) -> bool {
        self.lines
    }
    /// The same rule over the lines of a text source, when its only work is the removal of
    /// duplicates: a line has no columns to filter, group, sort or project. Anything else
    /// over a text source is `None`: the human is asked.
    pub(super) fn over_lines(&self) -> Option<Self> {
        let s = &self.shape;
        let only_distinct = s.distinct
            && self.clauses.is_empty()
            && s.join_on.is_none()
            && s.group_by.is_none()
            && s.aggregations.is_empty()
            && s.sort_by.is_none()
            && s.limit.is_none()
            && s.columns.is_empty();
        only_distinct.then(|| Self {
            lines: true,
            ..self.clone()
        })
    }
    /// The names of the totals, when the computation is totals over every row.
    pub(super) fn totals_names(&self) -> Vec<String> {
        self.shape.totals_names()
    }
    /// The excerpt the rule was read from.
    pub(super) fn text(&self) -> &str {
        &self.text
    }
    /// The inverse of [`Rule::to_json`], for a recorded plan replayed on an answer round.
    pub(super) fn from_json(value: &Value) -> Option<Self> {
        let text = value.get("text")?.as_str()?.to_owned();
        let clauses = value
            .get("clauses")?
            .as_array()?
            .iter()
            .map(Clause::from_json)
            .collect::<Option<Vec<_>>>()?;
        let shape = Shape::from_json(value.get("shape"))?;
        if clauses.is_empty() && shape == Shape::default() {
            return None;
        }
        let junction = match value.get("junction").and_then(Value::as_str) {
            Some("or") => Junction::Or,
            _ => Junction::And,
        };
        let summary = value
            .get("summary")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let lines = value.get("lines").and_then(Value::as_bool).unwrap_or(false);
        Some(Self {
            text,
            clauses,
            junction,
            summary,
            shape,
            lines,
        })
    }
    /// Whether the text also asked for the count and totals the summary stage computes.
    pub(super) const fn summary(&self) -> bool {
        self.summary
    }
    /// Every source column the rule reads, first use first: the join key, the clauses, the
    /// group column, the aggregated columns, a sort or a projection on a source column (a
    /// sort or a projection on a produced name reads nothing from the source).
    pub(super) fn fields(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut push = |name: &str| {
            if !out.iter().any(|f| f == name) {
                out.push(name.to_owned());
            }
        };
        if let Some(key) = &self.shape.join_on {
            push(key);
        }
        for clause in &self.clauses {
            push(&clause.field);
            if let Operand::Column(other) = &clause.value {
                push(other);
            }
        }
        let shape = &self.shape;
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
        out
    }
    fn predicate(&self) -> String {
        self.clauses
            .iter()
            .map(Clause::jq)
            .collect::<Vec<_>>()
            .join(&format!(" {} ", self.junction.word()))
    }
    /// The computation over the parsed records: the join of the sources when the rule joins,
    /// the filter, then the shape's stages in their fixed order; over the lines of a text
    /// source, the result is written back as lines with the file's final newline.
    pub(super) fn jq(&self) -> String {
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
        let mut jq = self.shape.lower(filtered);
        if self.lines {
            jq.push_str(" | join(\"\\n\") | if length > 0 then . + \"\\n\" else . end");
        }
        jq
    }
    /// True when the records are an array whose first record carries every column the
    /// rule reads (an empty array passes): a wrong column fails loudly, never filters
    /// everything in silence. A join judges every source's first record; lines are strings.
    pub(super) fn guard(&self) -> String {
        if self.lines {
            return "(.records | type) == \"array\" and all(.records[]; type == \"string\")"
                .to_owned();
        }
        let has = self
            .fields()
            .iter()
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
    pub(super) fn guard_message(&self) -> String {
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
    pub(super) fn to_json(&self) -> Value {
        let mut record = json!({
            "text": self.text,
            "fields": self.fields(),
            "clauses": self.clauses.iter().map(Clause::to_json).collect::<Vec<_>>(),
            "jq": self.jq(),
            "synthesized": true,
            "summary": self.summary,
            "shape": self.shape.to_json(),
            "lines": self.lines,
        });
        if let [only] = self.clauses.as_slice() {
            let clause = only.to_json();
            record["field"] = clause["field"].clone();
            record["comparator"] = clause["comparator"].clone();
            record["value"] = clause["value"].clone();
        } else {
            record["junction"] = json!(self.junction.word());
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

/// The last relative marker in a region: its index and width.
fn last_relative(region: &[Token]) -> Option<(usize, usize)> {
    let mut found = None;
    for at in 0..region.len() {
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
const EXCLUSION_LEADS: &[&str] = &[
    "exclude",
    "excludes",
    "excluding",
    "drop",
    "drops",
    "remove",
    "removes",
    "delete",
    "deletes",
    "discard",
    "discards",
    "omit",
    "omits",
    "skip",
    "skips",
    "ignore",
    "ignores",
    "strip",
    "out",
    "exclus",
    "exclure",
    "excluez",
    "supprime",
    "supprimez",
    "supprimer",
    "retire",
    "retirez",
    "retirer",
    "enleve",
    "enlevez",
    "enlever",
    "elimine",
    "eliminez",
    "eliminer",
    "ignorez",
    "ecarte",
    "ecartez",
    "elimina",
    "quita",
    "descarta",
    "excluye",
    "omite",
    "rimuovi",
    "escludi",
    "scarta",
    "entferne",
    "losche",
    "verwerfe",
];

fn negated_lead(lead: &[Token]) -> bool {
    let words: Vec<&str> = lead.iter().filter_map(Token::word).collect();
    words.iter().enumerate().any(|(at, word)| {
        if matches!(*word, "ne" | "n") {
            return !words
                .get(at + 1..(at + 4).min(words.len()))
                .is_some_and(|window| window.contains(&"que"));
        }
        NEGATIONS.contains(word)
            || EXCLUSION_LEADS.contains(word)
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

/// The field named left of the comparison. `None` when the lead carries a number, a
/// symbol, a quote or a negation the grammar did not consume; `Unnamed` when nothing
/// there names a column.
fn left_field(tokens: &[Token], from: usize, anchor: &Anchor, columns: &[String]) -> Option<Left> {
    let region = tokens.get(from..anchor.field_end).unwrap_or_default();
    let (lead, phrase, relative) = match last_relative(region) {
        Some((at, width)) => (
            region.get(..at).unwrap_or_default(),
            region.get(at + width..).unwrap_or_default(),
            true,
        ),
        None => match region.split_last() {
            Some((last, lead)) => (lead, std::slice::from_ref(last), false),
            None => (region, region, false),
        },
    };
    if lead.iter().any(|t| t.word().is_none()) || negated_lead(lead) {
        return None;
    }
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
            .is_some_and(|w| UNIT_WORDS.contains(&w) || COPULAS.contains(&w));
        if !trailing {
            return None;
        }
        at += 1;
    }
}

/// One clause from `from`: the clause and the index of the token after it.
fn parse_clause(tokens: &[Token], from: usize, columns: &[String]) -> Option<(Clause, usize)> {
    let anchor = (from..tokens.len()).find_map(|at| anchor_at(tokens, at))?;
    let (comparator, value_from) = comparator_after(tokens, &anchor)?;
    let left = left_field(tokens, from, &anchor, columns)?;
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
    Some((
        Clause {
            field,
            comparator,
            value,
        },
        next,
    ))
}

/// Sentences and `;`-joined rules, each of which must parse whole.
fn segments(text: &str) -> impl Iterator<Item = &str> {
    text.split(';')
        .flat_map(|part| part.split(". "))
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// The rule the text states, or `None` when any part is outside the grammar.
pub(super) fn synthesize(text: &str, columns: &[String]) -> Option<Rule> {
    let text = text.trim();
    let mut clauses = Vec::new();
    let mut junction: Option<Junction> = None;
    let mut summary = false;
    let mut shape = Shape::default();
    for segment in segments(text) {
        let tokens = tokenize(segment);
        if tokens.is_empty() {
            continue;
        }
        if !clauses.is_empty() {
            if junction == Some(Junction::Or) {
                return None;
            }
            junction = Some(Junction::And);
        }
        let mut at = 0;
        loop {
            let Some((clause, next)) = parse_clause(&tokens, at, columns) else {
                // A whole segment stating a stage (an aggregate over a column, a count per
                // column, a sort, a top-N, a projection, a removal of duplicates, a join) is
                // the shape's work: the filter (if any) runs first, the stages follow.
                if at == 0
                    && let Some(stage) = super::stages::stated(segment, columns)
                {
                    shape = shape.merge(stage)?;
                    break;
                }
                // After a clause, a count-or-total request is the summary stage's work.
                let trailing = (at > 0 || !clauses.is_empty()) && junction != Some(Junction::Or);
                if trailing && summary_residual(&tokens, at, columns) {
                    summary = true;
                    break;
                }
                return None;
            };
            // The same clause twice (a promoted constraint beside the seat's paraphrase
            // of it) is one clause.
            if !clauses.contains(&clause) {
                clauses.push(clause);
            }
            let Some(token) = tokens.get(next) else {
                break;
            };
            let joined = junction_of(token)?;
            if junction.is_some_and(|j| j != joined) {
                return None;
            }
            junction = Some(joined);
            at = next + 1;
            if at >= tokens.len() {
                return None;
            }
        }
    }
    if clauses.is_empty() && shape == Shape::default() {
        return None;
    }
    Some(Rule {
        text: text.to_owned(),
        clauses,
        junction: junction.unwrap_or(Junction::And),
        summary,
        shape,
        lines: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cols(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    fn jq(text: &str) -> Option<String> {
        synthesize(text, &[]).map(|r| r.jq())
    }

    fn jq_with(text: &str, columns: &[&str]) -> Option<String> {
        synthesize(text, &cols(columns)).map(|r| r.jq())
    }

    #[test]
    fn a_trailing_count_or_total_request_is_the_summary_stage() {
        let hint = cols(&["order_id", "customer", "amount", "status"]);
        let folded = "keep only the rows whose amount is strictly greater than 100 and how many rows were kept and the total of their amounts";
        let rule = synthesize(folded, &hint).expect("a rule");
        assert!(rule.summary());
        assert_eq!(
            rule.jq(),
            "[.records[] | select((.amount | tonumber) > 100)]"
        );
        assert_eq!(rule.to_json()["summary"], true);
        let french = "garde les lignes dont le montant est plus grand que 100 et le nombre de lignes gardées et le total de leurs montants";
        assert!(synthesize(french, &[]).expect("a rule").summary());
        let sentence = "amount > 100. Count how many rows were kept";
        assert!(synthesize(sentence, &[]).expect("a rule").summary());
        // The live seat's paraphrase beside the promoted constraint: one clause, summary.
        let paraphrase = "filter rows with amount strictly greater than 100 and compute count plus total amount ; keep only the rows whose amount is strictly greater than 100";
        let rule = synthesize(paraphrase, &hint).expect("a rule");
        assert!(rule.summary());
        assert_eq!(
            rule.jq(),
            "[.records[] | select((.amount | tonumber) > 100)]"
        );
        assert_eq!(rule.to_json()["clauses"].as_array().map(Vec::len), Some(1));
        assert!(!synthesize("amount > 100", &[]).expect("a rule").summary());
        for none in [
            "how many rows were kept and the total of their amounts",
            "amount > 100 or how many rows were kept",
            "amount > 100 and how many rows were kept and whose status is open",
            "amount > 100 and the total per country",
            "keep only the rows whose status is \"shipped\" and whose total_eur is above 120. Write the count of those orders per country as JSON",
        ] {
            assert_eq!(synthesize(none, &hint), None, "{none}");
        }
    }

    #[test]
    fn a_numeric_rule_in_six_languages_becomes_one_select() {
        let gt = Some("[.records[] | select((.amount | tonumber) > 100)]".to_owned());
        assert_eq!(
            jq("keep only the rows whose amount is strictly greater than 100"),
            gt
        );
        assert_eq!(jq("whose amount is above 100"), gt);
        assert_eq!(jq("amount > 100"), gt);
        assert_eq!(jq("rows where amount > 100"), gt);
        assert_eq!(jq("amount>100"), gt);
        assert_eq!(
            jq("dont le montant est plus grand que 200"),
            Some("[.records[] | select((.montant | tonumber) > 200)]".to_owned())
        );
        assert_eq!(
            jq("les lignes dont le montant est strictement supérieur à 200 €"),
            Some("[.records[] | select((.montant | tonumber) > 200)]".to_owned())
        );
        assert_eq!(
            jq("cuya cantidad es menor que 10"),
            Some("[.records[] | select((.cantidad | tonumber) < 10)]".to_owned())
        );
        assert_eq!(
            jq("las filas cuyo total es mayor o igual a 50"),
            Some("[.records[] | select((.total | tonumber) >= 50)]".to_owned())
        );
        assert_eq!(
            jq("le righe la cui quantità è minore di 5"),
            Some("[.records[] | select((.[\"quantità\"] | tonumber) < 5)]".to_owned())
        );
        assert_eq!(
            jq("as linhas cuja quantidade é menor que 10"),
            Some("[.records[] | select((.quantidade | tonumber) < 10)]".to_owned())
        );
        assert_eq!(
            jq("die Zeilen, deren Betrag größer als 100 ist"),
            Some("[.records[] | select((.Betrag | tonumber) > 100)]".to_owned())
        );
        assert_eq!(
            jq("whose total_eur is at least 120 EUR"),
            Some("[.records[] | select((.total_eur | tonumber) >= 120)]".to_owned())
        );
        assert_eq!(
            jq("whose total_eur is at most 12.5"),
            Some("[.records[] | select((.total_eur | tonumber) <= 12.5)]".to_owned())
        );
        assert_eq!(
            jq("montant ≥ 1,5"),
            Some("[.records[] | select((.montant | tonumber) >= 1.5)]".to_owned())
        );
        assert_eq!(
            jq("total_eur >= 1,000"),
            Some("[.records[] | select((.total_eur | tonumber) >= 1000)]".to_owned())
        );
        assert_eq!(
            jq("whose qty is 0"),
            Some("[.records[] | select((.qty | tonumber) == 0)]".to_owned())
        );
        assert_eq!(
            jq("whose qty is not 0"),
            Some("[.records[] | select((.qty | tonumber) != 0)]".to_owned())
        );
    }

    #[test]
    fn a_columns_hint_names_the_field_in_its_own_spelling() {
        let hint = ["order_id", "customer", "Amount", "status", "unit price"];
        assert_eq!(
            jq_with("keep the rows with amount above 100", &hint),
            Some("[.records[] | select((.Amount | tonumber) > 100)]".to_owned())
        );
        assert_eq!(
            jq_with("whose unit price is below 3", &hint),
            Some("[.records[] | select((.[\"unit price\"] | tonumber) < 3)]".to_owned())
        );
        assert_eq!(
            jq_with("whose unit_price is below 3", &hint),
            Some("[.records[] | select((.[\"unit price\"] | tonumber) < 3)]".to_owned())
        );
        assert_eq!(
            jq_with("products with fewer than 10 units", &["sku", "units"]),
            Some("[.records[] | select((.units | tonumber) < 10)]".to_owned())
        );
        // A word outside the hint is not a column: the human is asked.
        assert_eq!(jq_with("whose total is above 100", &hint), None);
        assert_eq!(jq_with("whose amount_eur is above 100", &hint), None);
    }

    #[test]
    fn an_equality_rule_keeps_the_exact_case_of_its_value() {
        assert_eq!(
            jq("whose status is refunded"),
            Some("[.records[] | select(.status == \"refunded\")]".to_owned())
        );
        assert_eq!(
            jq("whose status is \"Refunded\""),
            Some("[.records[] | select(.status == \"Refunded\")]".to_owned())
        );
        assert_eq!(
            jq("dont le statut est « expédié »"),
            Some("[.records[] | select(.statut == \"expédié\")]".to_owned())
        );
        assert_eq!(
            jq("cuyo estado no es reembolsado"),
            Some("[.records[] | select(.estado != \"reembolsado\")]".to_owned())
        );
        assert_eq!(
            jq("whose status is not 'shipped'"),
            Some("[.records[] | select(.status != \"shipped\")]".to_owned())
        );
        assert_eq!(
            jq("whose status equals shipped"),
            Some("[.records[] | select(.status == \"shipped\")]".to_owned())
        );
        assert_eq!(
            jq("status = shipped"),
            Some("[.records[] | select(.status == \"shipped\")]".to_owned())
        );
        assert_eq!(
            jq("whose country is different from FR"),
            Some("[.records[] | select(.country != \"FR\")]".to_owned())
        );
        assert_eq!(
            jq("deren Status ungleich offen"),
            Some("[.records[] | select(.Status != \"offen\")]".to_owned())
        );
    }

    #[test]
    fn two_columns_compare_when_both_name_columns() {
        assert_eq!(
            jq("la cui quantita e inferiore alla soglia_minima"),
            Some(
                "[.records[] | select((.quantita | tonumber) < (.soglia_minima | tonumber))]"
                    .to_owned()
            )
        );
        assert_eq!(
            jq_with(
                "cuja quantidade é menor que minimo",
                &["sku", "nome", "quantidade", "minimo"]
            ),
            Some(
                "[.records[] | select((.quantidade | tonumber) < (.minimo | tonumber))]".to_owned()
            )
        );
        assert_eq!(
            jq("whose stock_qty is below reorder_level"),
            Some(
                "[.records[] | select((.stock_qty | tonumber) < (.reorder_level | tonumber))]"
                    .to_owned()
            )
        );
        // A bare word right of a numeric comparison is not a column.
        assert_eq!(jq("whose quantity is below minimum"), None);
        assert_eq!(jq("whose stock_qty is below stock_qty"), None);
    }

    #[test]
    fn clauses_join_through_one_conjunction() {
        assert_eq!(
            jq("keep only the rows whose status is \"shipped\" and whose total_eur is above 120"),
            Some(
                "[.records[] | select(.status == \"shipped\" and (.total_eur | tonumber) > 120)]"
                    .to_owned()
            )
        );
        assert_eq!(
            jq("amount > 100 or amount < 10"),
            Some(
                "[.records[] | select((.amount | tonumber) > 100 or (.amount | tonumber) < 10)]"
                    .to_owned()
            )
        );
        assert_eq!(
            jq("dont le montant est plus grand que 100 et dont le statut est ouvert"),
            Some(
                "[.records[] | select((.montant | tonumber) > 100 and .statut == \"ouvert\")]"
                    .to_owned()
            )
        );
        assert_eq!(
            jq("cuya cantidad es menor que 10 y cuyo estado es activo"),
            Some(
                "[.records[] | select((.cantidad | tonumber) < 10 and .estado == \"activo\")]"
                    .to_owned()
            )
        );
        // Two promoted rules joined by the plan's separator.
        assert_eq!(
            jq("amount > 100 ; whose status is open"),
            Some(
                "[.records[] | select((.amount | tonumber) > 100 and .status == \"open\")]"
                    .to_owned()
            )
        );
        // Mixed junctions have no fixed precedence in prose: the human is asked.
        assert_eq!(jq("amount > 100 and amount < 10 or qty > 3"), None);
        assert_eq!(jq("amount > 100 or qty > 3 ; status = open"), None);
        assert_eq!(jq("amount > 100 and"), None);
    }

    #[test]
    fn a_stated_aggregate_is_the_shape_after_the_filter() {
        assert_eq!(
            jq("the total of the amount column"),
            Some(".records | {\"total\": (map(.amount | tonumber) | add // 0)}".to_owned())
        );
        assert_eq!(
            jq("the total of the amount column ; whose client is acme"),
            Some(
                "[.records[] | select(.client == \"acme\")] | {\"total\": (map(.amount | tonumber) | add // 0)}"
                    .to_owned()
            )
        );
        let rule = synthesize("la moyenne de la colonne montant", &[]).expect("a rule");
        assert_eq!(rule.totals_names(), ["moyenne"]);
        assert_eq!(rule.fields(), ["montant"]);
        assert!(!rule.summary());
        // A grouping is a stage of the shape: one row per client with its total.
        let grouped = synthesize("the total of the amount column per client", &[]).expect("a rule");
        assert_eq!(
            grouped.jq(),
            ".records | group_by(.client) | map({\"client\": (.[0] | .client), \"total\": (map(.amount | tonumber) | add // 0)})"
        );
        assert_eq!(grouped.fields(), ["client", "amount"]);
        assert_eq!(
            grouped.output_columns(),
            Some(vec!["client".to_owned(), "total".to_owned()])
        );
        assert!(grouped.totals_names().is_empty());
    }

    #[test]
    fn a_join_a_top_n_a_projection_and_a_dedup_lower_after_the_filter() {
        let join = synthesize("merge them on the id column", &[]).expect("a join");
        assert!(join.joins());
        assert_eq!(join.fields(), ["id"]);
        assert_eq!(
            join.jq(),
            ".records | reduce .[1:][] as $right (.[0]; [.[] as $a | $right[] | select(.id == ($a | .id)) | $a + .])"
        );
        assert_eq!(
            join.guard(),
            "(.records | type) == \"array\" and (.records | length) >= 2 and all(.records[]; type == \"array\" and (length == 0 or (.[0] | type == \"object\" and has(\"id\"))))"
        );
        assert!(join.guard_message().contains("joins the sources on `id`"));
        // A filter after the join selects over the joined rows.
        assert_eq!(
            jq("merge them on the id column ; whose amount is above 100"),
            Some(".records | reduce .[1:][] as $right (.[0]; [.[] as $a | $right[] | select(.id == ($a | .id)) | $a + .]) | [.[] | select((.amount | tonumber) > 100)]".to_owned())
        );
        assert_eq!(
            jq("keep the 2 rows with the highest amount"),
            Some(".records | sort_by(.amount | tonumber? // .) | reverse | .[:2]".to_owned())
        );
        assert_eq!(
            jq("whose client is acme ; keep the 2 rows with the highest amount"),
            Some("[.records[] | select(.client == \"acme\")] | sort_by(.amount | tonumber? // .) | reverse | .[:2]".to_owned())
        );
        let slim =
            synthesize("keep only the id and title of each ticket", &[]).expect("a projection");
        assert_eq!(
            slim.jq(),
            ".records | map({\"id\": .id, \"title\": .title})"
        );
        assert_eq!(slim.fields(), ["id", "title"]);
        assert_eq!(
            slim.output_columns(),
            Some(vec!["id".to_owned(), "title".to_owned()])
        );
        // A removal of duplicates over a text source runs over its lines and writes lines.
        let distinct = synthesize("remove the duplicate lines", &[]).expect("a dedup");
        assert!(!distinct.lines());
        let lines = distinct.over_lines().expect("over lines");
        assert!(lines.lines());
        assert_eq!(
            lines.jq(),
            ".records | reduce .[] as $r ([]; if any(.[]; . == $r) then . else . + [$r] end) | join(\"\\n\") | if length > 0 then . + \"\\n\" else . end"
        );
        assert_eq!(
            lines.guard(),
            "(.records | type) == \"array\" and all(.records[]; type == \"string\")"
        );
        assert_eq!(Rule::from_json(&lines.to_json()), Some(lines));
        // A filter, a sort or a projection has no meaning over lines: asked, never guessed.
        for text in [
            "whose status is open",
            "sort the rows by amount",
            "keep only the id and title of each ticket",
            "count the rows per client",
        ] {
            assert_eq!(
                synthesize(text, &[]).and_then(|r| r.over_lines()),
                None,
                "{text}"
            );
        }
        // Two stages of the same kind, or a dedup beside a top-N, are not one computation.
        assert_eq!(
            jq("sort the rows by amount ; sort the rows by client"),
            None
        );
        assert_eq!(
            jq("remove the duplicate lines ; keep the 2 rows with the highest amount"),
            None
        );
    }

    #[test]
    fn a_negation_among_the_lead_words_is_read_as_nothing_never_inverted() {
        for text in [
            "do not keep the tickets whose status is closed",
            "never keep the tickets whose status is closed",
            "Read ./tickets.json, do not keep the tickets whose status is closed",
            "ne garde pas les lignes dont amount dépasse 200",
            "ne garde jamais les lignes dont amount dépasse 200",
            "don't keep rows whose amount is above 100",
            // An exclusion names what leaves: never read as a keep of those rows.
            "exclude the rows whose amount is below 100 or whose status is refunded",
            "drop the rows whose status is closed",
            "filter out the rows whose amount is above 100",
            "remove the rows whose status is closed",
            "supprime les lignes dont le montant est plus grand que 100",
        ] {
            assert_eq!(synthesize(text, &[]), None, "{text}");
        }
        // The French restriction is "only": a filter, read as stated.
        assert_eq!(
            jq("ne garde que les lignes dont amount dépasse 200"),
            Some("[.records[] | select((.amount | tonumber) > 200)]".to_owned())
        );
        // A negation after the copula is the clause's own polarity, still read.
        assert_eq!(
            jq("whose status is not closed"),
            Some("[.records[] | select(.status != \"closed\")]".to_owned())
        );
    }

    #[test]
    fn what_the_grammar_does_not_cover_is_none() {
        for text in [
            "a brief of under 150 words",
            "au plus 12 lignes",
            "at most 3 attempts",
            "Process at most 2 products at a time",
            "products with fewer than 10 units",
            "quantité inférieure à 10",
            "au moins 3 articles en stock",
            "the top 3 countries",
            "in a warm tone",
            "keep only the rows whose status is \"shipped\" and whose total_eur is above 120. Write the count of those orders per country as JSON",
            "whose amount is above 100 per country",
            "whose status is refunded today",
            "the 3 rows whose amount > 100",
            "whose status is shipped or refunded",
            "whose amount is between 10 and 20",
            "whose amount is not above 100",
            "whose amount > \"high\"",
            "",
        ] {
            assert_eq!(synthesize(text, &[]), None, "{text}");
        }
    }

    #[test]
    fn the_guard_names_every_column_and_the_record_is_observational() {
        let rule = synthesize(
            "whose status is \"shipped\" and whose total_eur is above stock_min",
            &[],
        )
        .expect("a rule");
        assert_eq!(rule.fields(), ["status", "total_eur", "stock_min"]);
        assert_eq!(
            rule.guard(),
            "(.records | type) == \"array\" and ((.records | length) == 0 or (.records[0] | type == \"object\" and has(\"status\") and has(\"total_eur\") and has(\"stock_min\")))"
        );
        assert!(
            rule.guard_message()
                .contains("`status`, `total_eur`, `stock_min`")
        );
        let record = rule.to_json();
        assert_eq!(record["synthesized"], true);
        assert_eq!(record["junction"], "and");
        assert_eq!(record["clauses"][1]["value_kind"], "column");
        assert!(record.get("field").is_none());
        let one = synthesize("whose amount is above 100", &[]).expect("a rule");
        let record = one.to_json();
        assert_eq!(record["field"], "amount");
        assert_eq!(record["comparator"], ">");
        assert_eq!(record["value"], "100");
        assert_eq!(record["text"], "whose amount is above 100");
        assert_eq!(key("unit price"), ".[\"unit price\"]");
        assert_eq!(key("_a1"), "._a1");
    }
}
