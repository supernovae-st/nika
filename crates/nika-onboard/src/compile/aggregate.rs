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
}

impl Shape {
    /// The names the shape produces: the group column and every aggregate.
    pub(super) fn produced(&self) -> Vec<&str> {
        self.group_by
            .iter()
            .map(String::as_str)
            .chain(self.aggregations.iter().map(|a| a.name.as_str()))
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
            self.aggregations.iter().map(|a| a.name.clone()).collect()
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
        Some(Self {
            group_by: text("group_by"),
            aggregations,
            sort_by: text("sort_by").map(|f| (f, descending)),
            columns,
        })
    }
}
