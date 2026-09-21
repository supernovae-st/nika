//! The shape of a typed computation after its filter: an aggregate over a group or over
//! every row, a sort, a projection. Each stage is a closed type the semantic frontend states
//! and the compiler validates; the composition lowers to jq in one fixed order.

use serde_json::{Value, json};

use super::rules::key;

/// An aggregate over a group or over every row: sum, count, average, minimum, maximum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AggOp {
    Sum,
    Count,
    Avg,
    Min,
    Max,
}

impl AggOp {
    pub(super) fn from_word(word: &str) -> Option<Self> {
        match word.trim().to_ascii_lowercase().as_str() {
            "sum" | "total" => Some(Self::Sum),
            "count" | "n" => Some(Self::Count),
            "avg" | "average" | "mean" => Some(Self::Avg),
            "min" | "minimum" => Some(Self::Min),
            "max" | "maximum" => Some(Self::Max),
            _ => None,
        }
    }
    const fn word(self) -> &'static str {
        match self {
            Self::Sum => "sum",
            Self::Count => "count",
            Self::Avg => "avg",
            Self::Min => "min",
            Self::Max => "max",
        }
    }
}

/// One aggregate: the source column it reads (none for a count), the output name the
/// request states, and the decimals it rounds to when the request rounds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Aggregation {
    pub field: Option<String>,
    pub op: AggOp,
    pub name: String,
    pub round: Option<u32>,
}

impl Aggregation {
    pub(super) fn jq(&self) -> String {
        let values = self
            .field
            .as_deref()
            .map(|f| format!("map({} | tonumber)", key(f)));
        let core = match (self.op, values) {
            (AggOp::Sum, Some(v)) => format!("({v} | add // 0)"),
            (AggOp::Avg, Some(v)) => {
                format!("(if length == 0 then 0 else (({v} | add) / length) end)")
            }
            (AggOp::Min, Some(v)) => format!("({v} | min)"),
            (AggOp::Max, Some(v)) => format!("({v} | max)"),
            (AggOp::Count, _) | (_, None) => "length".to_owned(),
        };
        match self.round {
            Some(n) => {
                let factor = 10_u64.pow(n);
                format!("(({core} * {factor} | round) / {factor})")
            }
            None => core,
        }
    }
    fn to_json(&self) -> Value {
        json!({"field": self.field, "op": self.op.word(), "name": self.name, "round": self.round})
    }
    fn from_json(value: &Value) -> Option<Self> {
        let op = AggOp::from_word(value.get("op")?.as_str()?)?;
        let name = value.get("name")?.as_str()?.trim().to_owned();
        if name.is_empty() {
            return None;
        }
        let field = value
            .get("field")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|f| !f.is_empty())
            .map(str::to_owned);
        let round = value
            .get("round")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok());
        Some(Self {
            field,
            op,
            name,
            round,
        })
    }
}

/// Arithmetic between two outputs (or an output and a literal of the request).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ArithOp {
    Sub,
    Add,
    Mul,
    Div,
}

impl ArithOp {
    pub(super) fn from_word(word: &str) -> Option<Self> {
        match word.trim().to_ascii_lowercase().as_str() {
            "sub" | "minus" | "-" => Some(Self::Sub),
            "add" | "plus" | "+" => Some(Self::Add),
            "mul" | "times" | "*" => Some(Self::Mul),
            "div" | "over" | "/" => Some(Self::Div),
            _ => None,
        }
    }
    const fn word(self) -> &'static str {
        match self {
            Self::Sub => "sub",
            Self::Add => "add",
            Self::Mul => "mul",
            Self::Div => "div",
        }
    }
}

/// One side of a derived output: another output by name, or a number of the request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Term {
    Name(String),
    Number(String),
}

impl Term {
    fn jq(&self) -> String {
        match self {
            Self::Name(name) => key(name),
            Self::Number(n) => n.clone(),
        }
    }
    fn to_json(&self) -> Value {
        match self {
            Self::Name(name) => json!({"name": name}),
            Self::Number(n) => json!({"number": n}),
        }
    }
    fn from_json(value: &Value) -> Option<Self> {
        if let Some(name) = value.get("name").and_then(Value::as_str) {
            return Some(Self::Name(name.to_owned()));
        }
        value
            .get("number")
            .and_then(Value::as_str)
            .map(|n| Self::Number(n.to_owned()))
    }
}

/// An output the request defines as arithmetic over other outputs (`solde = credit - debit`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Derived {
    pub name: String,
    pub op: ArithOp,
    pub left: Term,
    pub right: Term,
}

impl Derived {
    fn jq(&self) -> String {
        let (l, r) = (self.left.jq(), self.right.jq());
        match self.op {
            ArithOp::Sub => format!("({l} - {r})"),
            ArithOp::Add => format!("({l} + {r})"),
            ArithOp::Mul => format!("({l} * {r})"),
            ArithOp::Div => format!("(if ({r}) == 0 then null else ({l} / {r}) end)"),
        }
    }
    fn to_json(&self) -> Value {
        json!({"name": self.name, "op": self.op.word(), "left": self.left.to_json(), "right": self.right.to_json()})
    }
    fn from_json(value: &Value) -> Option<Self> {
        Some(Self {
            name: value.get("name")?.as_str()?.to_owned(),
            op: ArithOp::from_word(value.get("op")?.as_str()?)?,
            left: Term::from_json(value.get("left")?)?,
            right: Term::from_json(value.get("right")?)?,
        })
    }
}

/// What happens to the rows after the filter: one output row per distinct value of a
/// column with its aggregates, or totals over every row, then a sort, then a projection.
/// Every stage is typed and closed; the composition lowers in that fixed order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Shape {
    pub group_by: Option<String>,
    pub aggregations: Vec<Aggregation>,
    /// The column sorted on and whether the order is descending.
    pub sort_by: Option<(String, bool)>,
    pub columns: Vec<String>,
    /// Outputs defined as arithmetic over the aggregates.
    pub derived: Vec<Derived>,
}

impl Shape {
    /// The names the shape produces: the group column and every aggregate.
    pub(super) fn produced(&self) -> Vec<&str> {
        self.group_by
            .iter()
            .map(String::as_str)
            .chain(self.aggregations.iter().map(|a| a.name.as_str()))
            .chain(self.derived.iter().map(|d| d.name.as_str()))
            .collect()
    }
    /// The columns the shape writes, in order, when it fixes them: the projection, or the
    /// group column and the aggregates. Totals are one object, never columns.
    pub(super) fn output_columns(&self) -> Option<Vec<String>> {
        if self.is_totals() {
            return None;
        }
        if !self.columns.is_empty() {
            return Some(self.columns.clone());
        }
        if self.group_by.is_some() {
            return Some(self.produced().iter().map(|s| (*s).to_owned()).collect());
        }
        None
    }
    /// The names of the totals, when the shape is totals over every row.
    pub(super) fn totals_names(&self) -> Vec<String> {
        if self.is_totals() {
            self.aggregations
                .iter()
                .map(|a| a.name.clone())
                .chain(self.derived.iter().map(|d| d.name.clone()))
                .collect()
        } else {
            Vec::new()
        }
    }
    /// The stages after the filter, lowered in a fixed order onto the filtered rows: the
    /// grouping with its aggregates or the totals, then the sort, then the projection.
    pub(super) fn lower(&self, filtered: String) -> String {
        let mut jq = filtered;
        let entries = |aggregations: &[Aggregation]| {
            aggregations
                .iter()
                .map(|a| format!("{}: {}", json!(a.name), a.jq()))
                .collect::<Vec<_>>()
                .join(", ")
        };
        if let Some(group) = &self.group_by {
            let aggregates = if self.aggregations.is_empty() {
                "\"count\": length".to_owned()
            } else {
                entries(&self.aggregations)
            };
            jq = format!(
                "{jq} | group_by({k}) | map({{{name}: (.[0] | {k}), {aggregates}}})",
                k = key(group),
                name = json!(group)
            );
        } else if !self.aggregations.is_empty() {
            jq = format!("{jq} | {{{}}}", entries(&self.aggregations));
        }
        if !self.derived.is_empty() {
            let extra = self
                .derived
                .iter()
                .map(|d| format!("{}: {}", json!(d.name), d.jq()))
                .collect::<Vec<_>>()
                .join(", ");
            jq = if self.is_totals() {
                format!("{jq} | . + {{{extra}}}")
            } else {
                format!("{jq} | map(. + {{{extra}}})")
            };
        }
        if let Some((field, descending)) = &self.sort_by {
            jq = format!("{jq} | sort_by({})", key(field));
            if *descending {
                jq.push_str(" | reverse");
            }
        }
        if !self.columns.is_empty() && !self.is_totals() {
            let projection = self
                .columns
                .iter()
                .map(|c| format!("{}: {}", json!(c), key(c)))
                .collect::<Vec<_>>()
                .join(", ");
            jq = format!("{jq} | map({{{projection}}})");
        }
        jq
    }
    /// Totals over every row (aggregates without a group): one object, not rows.
    pub(super) fn is_totals(&self) -> bool {
        self.group_by.is_none() && !self.aggregations.is_empty()
    }
    pub(super) fn to_json(&self) -> Value {
        json!({
            "group_by": self.group_by,
            "aggregations": self.aggregations.iter().map(Aggregation::to_json).collect::<Vec<_>>(),
            "sort_by": self.sort_by.as_ref().map(|(f, _)| f.clone()),
            "descending": self.sort_by.as_ref().is_some_and(|(_, d)| *d),
            "columns": self.columns,
            "derived": self.derived.iter().map(Derived::to_json).collect::<Vec<_>>(),
        })
    }
    pub(super) fn from_json(value: Option<&Value>) -> Option<Self> {
        let Some(value) = value else {
            return Some(Self::default());
        };
        let text = |k: &str| {
            value
                .get(k)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_owned)
        };
        let aggregations = value
            .get("aggregations")
            .and_then(Value::as_array)
            .map_or_else(
                || Some(Vec::new()),
                |items| {
                    items
                        .iter()
                        .map(Aggregation::from_json)
                        .collect::<Option<Vec<_>>>()
                },
            )?;
        let descending = value
            .get("descending")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let columns = value
            .get("columns")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let derived = value.get("derived").and_then(Value::as_array).map_or_else(
            || Some(Vec::new()),
            |items| {
                items
                    .iter()
                    .map(Derived::from_json)
                    .collect::<Option<Vec<_>>>()
            },
        )?;
        Some(Self {
            group_by: text("group_by"),
            aggregations,
            sort_by: text("sort_by").map(|f| (f, descending)),
            columns,
            derived,
        })
    }
}

/// Words that state an aggregate in the request's own language (folded), each with the
/// operation it names. The word as written is the output name.
const AGG_WORDS: &[(&str, AggOp)] = &[
    ("total", AggOp::Sum),
    ("sum", AggOp::Sum),
    ("somme", AggOp::Sum),
    ("suma", AggOp::Sum),
    ("totale", AggOp::Sum),
    ("somma", AggOp::Sum),
    ("summe", AggOp::Sum),
    ("gesamtsumme", AggOp::Sum),
    ("average", AggOp::Avg),
    ("mean", AggOp::Avg),
    ("avg", AggOp::Avg),
    ("moyenne", AggOp::Avg),
    ("media", AggOp::Avg),
    ("promedio", AggOp::Avg),
    ("durchschnitt", AggOp::Avg),
    ("mittelwert", AggOp::Avg),
    ("count", AggOp::Count),
    ("number", AggOp::Count),
    ("nombre", AggOp::Count),
    ("numero", AggOp::Count),
    ("anzahl", AggOp::Count),
    ("conteo", AggOp::Count),
    ("conteggio", AggOp::Count),
    ("maximum", AggOp::Max),
    ("max", AggOp::Max),
    ("massimo", AggOp::Max),
    ("maximo", AggOp::Max),
    ("minimum", AggOp::Min),
    ("min", AggOp::Min),
    ("minimo", AggOp::Min),
];

/// The preposition between an aggregate and the column it reads.
const OF_WORDS: &[&str] = &[
    "of", "de", "des", "du", "d", "della", "del", "dei", "delle", "di", "da", "dos", "das", "der",
    "von", "vom",
];

/// Articles skipped before an aggregate word or a column name.
const DETERMINERS: &[&str] = &[
    "the", "a", "an", "le", "la", "les", "l", "el", "los", "las", "il", "lo", "i", "gli", "o",
    "os", "as", "die", "das", "dem", "den",
];

/// A word that marks the name beside it as a column.
const COLUMN_WORDS: &[&str] = &[
    "column", "field", "colonne", "champ", "columna", "campo", "colonna", "spalte", "feld",
];

/// A word that may trail a column name without changing the aggregate.
const VALUE_WORDS: &[&str] = &["values", "valeurs", "valores", "valori", "werte"];

/// The generic nouns a count may range over: every row, never a subset the request would
/// have to describe as a filter.
const ROW_WORDS: &[&str] = &[
    "rows",
    "row",
    "records",
    "record",
    "lines",
    "line",
    "entries",
    "entry",
    "items",
    "item",
    "lignes",
    "ligne",
    "enregistrements",
    "enregistrement",
    "filas",
    "fila",
    "registros",
    "registro",
    "righe",
    "riga",
    "zeilen",
    "zeile",
    "datensatze",
    "datensatz",
    "eintrage",
    "eintrag",
];

fn normalized(name: &str) -> String {
    super::shape::fold(name).replace([' ', '-'], "_")
}

/// The hint column a name designates, in the hint's own spelling.
fn hinted(name: &str, columns: &[String]) -> Option<String> {
    let wanted = normalized(name);
    columns.iter().find(|c| normalized(c) == wanted).cloned()
}

/// A single aggregate the request states over one column, in its own words: "the total
/// of the amount column", "la moyenne de la colonne montant", "the sum of amount", "the
/// number of rows". The aggregate word is the operation and the output name; the column
/// is the one token beside the `of` (a columns hint fixes its spelling), and nothing else
/// may follow. A count ranges over every row only under a generic row noun: "the number of
/// open tickets" describes a filter the grammar does not read, so it is no aggregate.
/// Anything else is `None`: the human is asked, nothing is guessed.
pub(super) fn stated(text: &str, columns: &[String]) -> Option<Aggregation> {
    let words: Vec<(String, String)> = text
        .split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| {
                matches!(
                    c,
                    '.' | ',' | ';' | ':' | '(' | ')' | '"' | '\'' | '`' | '«' | '»' | '!' | '?'
                )
            })
        })
        .filter(|w| !w.is_empty())
        .map(|w| (w.to_owned(), super::shape::fold(w).replace('\'', "")))
        .collect();
    let folded = |at: usize| words.get(at).map(|(_, f)| f.as_str());
    let mut at = 0;
    while folded(at).is_some_and(|w| DETERMINERS.contains(&w)) {
        at += 1;
    }
    let agg = folded(at)?;
    let (name, op) = AGG_WORDS
        .iter()
        .find(|(word, _)| *word == agg)
        .map(|(word, op)| ((*word).to_owned(), *op))?;
    at += 1;
    if !OF_WORDS.contains(&folded(at)?) {
        return None;
    }
    at += 1;
    while folded(at).is_some_and(|w| DETERMINERS.contains(&w)) {
        at += 1;
    }
    if op == AggOp::Count {
        let noun = folded(at)?;
        if !ROW_WORDS.contains(&noun) || at + 1 != words.len() {
            return None;
        }
        return Some(Aggregation {
            field: None,
            op,
            name,
            round: None,
        });
    }
    // `the amount column` · `the column amount` · `amount values` · `amount`.
    let (field_at, next) = if folded(at).is_some_and(|w| COLUMN_WORDS.contains(&w)) {
        let mut name_at = at + 1;
        while folded(name_at).is_some_and(|w| DETERMINERS.contains(&w)) {
            name_at += 1;
        }
        (name_at, name_at + 1)
    } else {
        let trailing =
            folded(at + 1).is_some_and(|w| COLUMN_WORDS.contains(&w) || VALUE_WORDS.contains(&w));
        (at, if trailing { at + 2 } else { at + 1 })
    };
    if next != words.len() {
        return None;
    }
    let (original, _) = words.get(field_at)?;
    if !original.chars().next().is_some_and(char::is_alphabetic) {
        return None;
    }
    let field = if columns.is_empty() {
        original.clone()
    } else {
        hinted(original, columns)?
    };
    Some(Aggregation {
        field: Some(field),
        op,
        name,
        round: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cols(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    fn agg(text: &str) -> Option<(Option<String>, AggOp, String)> {
        stated(text, &[]).map(|a| (a.field, a.op, a.name))
    }

    #[test]
    fn a_stated_aggregate_names_its_operation_its_column_and_its_output() {
        let some =
            |field: &str, op, name: &str| Some((Some(field.to_owned()), op, name.to_owned()));
        assert_eq!(
            agg("the total of the amount column"),
            some("amount", AggOp::Sum, "total")
        );
        assert_eq!(
            agg("the average of the amount column"),
            some("amount", AggOp::Avg, "average")
        );
        assert_eq!(agg("the sum of amount"), some("amount", AggOp::Sum, "sum"));
        assert_eq!(
            agg("the mean of the `unit_price` values"),
            some("unit_price", AggOp::Avg, "mean")
        );
        assert_eq!(
            agg("the maximum of the Amount column"),
            some("Amount", AggOp::Max, "maximum"),
            "the column keeps its spelling"
        );
        assert_eq!(
            agg("le total de la colonne montant"),
            some("montant", AggOp::Sum, "total")
        );
        assert_eq!(
            agg("la moyenne de la colonne amount"),
            some("amount", AggOp::Avg, "moyenne")
        );
        assert_eq!(
            agg("la somme du montant"),
            some("montant", AggOp::Sum, "somme")
        );
        assert_eq!(
            agg("il totale della colonna importo"),
            some("importo", AggOp::Sum, "totale")
        );
        assert_eq!(
            agg("die Summe der Spalte Betrag"),
            some("Betrag", AggOp::Sum, "summe")
        );
        assert_eq!(
            agg("the number of rows"),
            Some((None, AggOp::Count, "number".to_owned()))
        );
        assert_eq!(
            agg("le nombre de lignes"),
            Some((None, AggOp::Count, "nombre".to_owned()))
        );
        // A columns hint fixes the spelling; a name outside the hint is no column.
        let hint = cols(&["date", "Amount", "client"]);
        assert_eq!(
            stated("the total of the amount column", &hint).and_then(|a| a.field),
            Some("Amount".to_owned())
        );
        assert_eq!(stated("the total of the price column", &hint), None);
    }

    #[test]
    fn what_the_aggregate_grammar_does_not_cover_is_none() {
        for text in [
            "the total",
            "the total amount",
            "the total of",
            "the total of the amount column per client",
            "the total of the amount column and the average",
            "the total of the amount column rounded to 2 decimals",
            "the number of open tickets",
            "the number of tickets",
            "the count of the amount column",
            "the total of 3",
            "the highest amount",
            "keep only the rows whose amount is above 100",
            "",
        ] {
            assert_eq!(stated(text, &[]), None, "{text}");
        }
    }

    #[test]
    fn a_stated_aggregate_lowers_to_one_object_of_totals() {
        let total = stated("the total of the amount column", &[]).map(|a| a.jq());
        assert_eq!(
            total.as_deref(),
            Some("(map(.amount | tonumber) | add // 0)")
        );
        let shape = Shape {
            aggregations: stated("the average of the amount column", &[])
                .into_iter()
                .collect(),
            ..Shape::default()
        };
        assert!(shape.is_totals());
        assert_eq!(shape.totals_names(), ["average"]);
        assert_eq!(
            shape.lower(".records".to_owned()),
            ".records | {\"average\": (if length == 0 then 0 else ((map(.amount | tonumber) | add) / length) end)}"
        );
    }
}
