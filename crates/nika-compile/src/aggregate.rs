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
    /// The first N rows after the sort (« the top 3 », « les 5 plus vendus »).
    pub limit: Option<u32>,
    /// Output keys renamed, source name to stated name (« rename country to region »).
    pub renames: Vec<(String, String)>,
}

/// Words that rank rows (« the top-selling », « les plus vendus », « die meistverkauften »):
/// a descending sort under one of them keeps a count of rows the request must state.
const RANKING_WORDS: &[&str] = &[
    "top",
    "most",
    "best",
    "highest",
    "largest",
    "biggest",
    "best-selling",
    "top-selling",
    "les plus",
    "le plus",
    "la plus",
    "meilleurs",
    "meilleures",
    "plus vendus",
    "plus vendues",
    "los mas",
    "las mas",
    "el mas",
    "la mas",
    "mejores",
    "mas vendidos",
    "mas vendidas",
    "i piu",
    "le piu",
    "il piu",
    "la piu",
    "migliori",
    "piu venduti",
    "piu vendute",
    "die meisten",
    "meistverkauft",
    "meistverkauften",
    "meistverkaufte",
    "besten",
    "hochsten",
    "grossten",
    "os mais",
    "as mais",
    "o mais",
    "a mais",
    "melhores",
    "mais vendidos",
    "mais vendidas",
    "maiores",
];

/// Whether a computation's text ranks the rows.
pub(super) fn ranking_cue(text: &str) -> bool {
    let folded: String = super::shape::fold(text)
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                ' '
            }
        })
        .collect();
    let padded = format!(
        " {} ",
        folded.split_whitespace().collect::<Vec<_>>().join(" ")
    );
    RANKING_WORDS
        .iter()
        .any(|w| padded.contains(&format!(" {w} ")))
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
        let renamed = |names: Vec<String>| -> Vec<String> {
            names
                .into_iter()
                .map(|name| {
                    self.renames
                        .iter()
                        .find(|(from, _)| *from == name)
                        .map_or(name, |(_, to)| to.clone())
                })
                .collect()
        };
        if !self.columns.is_empty() {
            return Some(renamed(self.columns.clone()));
        }
        if self.group_by.is_some() {
            return Some(renamed(
                self.produced().iter().map(|s| (*s).to_owned()).collect(),
            ));
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
        if let Some(n) = self.limit
            && !self.is_totals()
        {
            jq = format!("{jq} | .[:{n}]");
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
        if !self.renames.is_empty() && !self.is_totals() {
            let arms = self
                .renames
                .iter()
                .map(|(from, to)| {
                    format!(
                        "if .key == {} then .key = {} else . end",
                        json!(from),
                        json!(to)
                    )
                })
                .collect::<Vec<_>>()
                .join(" | ");
            jq = format!("{jq} | map(with_entries({arms}))");
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
            "limit": self.limit,
            "renames": self.renames.iter().map(|(from, to)| json!({"from": from, "to": to})).collect::<Vec<_>>(),
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
        let limit = match value.get("limit") {
            None | Some(Value::Null) => None,
            Some(n) => Some(u32::try_from(n.as_u64()?).ok()?),
        };
        let renames = value.get("renames").and_then(Value::as_array).map_or_else(
            || Some(Vec::new()),
            |items| {
                items
                    .iter()
                    .map(|r| {
                        Some((
                            r.get("from")?.as_str()?.to_owned(),
                            r.get("to")?.as_str()?.to_owned(),
                        ))
                    })
                    .collect::<Option<Vec<_>>>()
            },
        )?;
        Some(Self {
            group_by: text("group_by"),
            aggregations,
            sort_by: text("sort_by").map(|f| (f, descending)),
            columns,
            derived,
            limit,
            renames,
        })
    }
}
