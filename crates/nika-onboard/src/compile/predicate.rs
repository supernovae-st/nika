//! The typed computation: a row filter, a grouping with aggregations, totals over every row,
//! a sort and a projection the semantic frontend states as meaning (columns, comparators,
//! literals, polarity, junction, output names) and the compiler validates part by part
//! against the request before lowering it deterministically. The model is never asked to
//! write the jq that becomes the contract; a computation that does not check out is no rule.

use serde::Deserialize;
use serde_json::{Value, json};

use super::rules::{AggOp, Aggregation, Shape};

/// A computation stated as meaning: the rows kept or dropped, the grouping, the aggregates,
/// the ordering and the output columns, all in the request's own words.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProposedComputation {
    pub(super) present: bool,
    #[serde(default, deserialize_with = "super::cognition::nullable_string")]
    pub(super) polarity: String,
    #[serde(default, deserialize_with = "super::cognition::nullable_string")]
    pub(super) join: String,
    #[serde(default, deserialize_with = "nullable_clauses")]
    pub(super) clauses: Vec<ProposedClause>,
    #[serde(default, deserialize_with = "super::cognition::nullable_string")]
    pub(super) group_by: String,
    #[serde(default, deserialize_with = "nullable_aggregations")]
    pub(super) aggregations: Vec<ProposedAggregation>,
    #[serde(default, deserialize_with = "super::cognition::nullable_string")]
    pub(super) sort_by: String,
    #[serde(default, deserialize_with = "super::cognition::nullable_string")]
    pub(super) order: String,
    #[serde(default, deserialize_with = "super::cognition::nullable_vec")]
    pub(super) columns: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProposedClause {
    pub(super) field: String,
    pub(super) op: String,
    #[serde(default, deserialize_with = "super::cognition::nullable_string")]
    pub(super) value: String,
    #[serde(default, deserialize_with = "super::cognition::nullable_string")]
    pub(super) value_field: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProposedAggregation {
    #[serde(default, deserialize_with = "super::cognition::nullable_string")]
    pub(super) field: String,
    pub(super) op: String,
    #[serde(
        rename = "as",
        default,
        deserialize_with = "super::cognition::nullable_string"
    )]
    pub(super) name: String,
    #[serde(default, deserialize_with = "super::cognition::nullable_string")]
    pub(super) round: String,
}
fn nullable_clauses<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<ProposedClause>, D::Error> {
    Ok(Option::<Vec<ProposedClause>>::deserialize(d)?.unwrap_or_default())
}
fn nullable_aggregations<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<ProposedAggregation>, D::Error> {
    Ok(Option::<Vec<ProposedAggregation>>::deserialize(d)?.unwrap_or_default())
}

/// A typed computation the proposal stated, validated part by part against the request:
/// every field is a column the request names (a columns hint or a word of the text), every
/// literal value occurs in the request, every comparator and aggregate is one of the closed
/// set, every output name is a word of the request. Anything else is no rule (the closed
/// grammar or a question takes over), never a guess.
#[allow(clippy::too_many_lines)] // one validation walk over one closed shape
pub(super) fn typed_rule(
    intent: &str,
    evidence: &str,
    computation: &ProposedComputation,
) -> Option<super::rules::Rule> {
    use super::rules::{Clause, Comparator, Junction, Operand, Rule};
    let lower = intent.to_lowercase();
    let hint = super::columns::columns_hint(intent);
    let names_field = |field: &str| {
        let field = field.trim();
        !field.is_empty()
            && field.len() <= 64
            && (hint.iter().any(|c| c.eq_ignore_ascii_case(field))
                || lower.contains(&field.to_lowercase()))
    };
    let digit_runs: Vec<String> = intent
        .split(|c: char| !c.is_ascii_digit() && c != '.' && c != ',')
        .filter(|run| run.chars().any(|c| c.is_ascii_digit()))
        .map(|run| run.trim_matches(['.', ',']).replace(',', "."))
        .collect();
    let mut clauses = Vec::new();
    for clause in &computation.clauses {
        if !names_field(&clause.field) {
            return None;
        }
        let comparator = Comparator::from_word(&clause.op)?;
        let other = clause.value_field.trim();
        let value = if other.is_empty() {
            let literal = clause.value.trim();
            if literal.is_empty() {
                return None;
            }
            let numeric = literal.replace(',', ".");
            if numeric.parse::<f64>().is_ok() {
                let canonical = numeric.trim_start_matches('+').to_owned();
                if !digit_runs.iter().any(|run| run == &canonical) {
                    return None;
                }
                Operand::Number(canonical)
            } else {
                let unquoted = literal.trim_matches(['"', '\'', '“', '”', '‘', '’']);
                if unquoted.is_empty() || !lower.contains(&unquoted.to_lowercase()) {
                    return None;
                }
                Operand::Text(unquoted.to_owned())
            }
        } else {
            if !names_field(other) {
                return None;
            }
            Operand::Column(other.to_owned())
        };
        clauses.push(Clause {
            field: clause.field.trim().to_owned(),
            comparator,
            value,
        });
    }
    let mut junction = match computation.join.trim() {
        "or" => Junction::Or,
        _ => Junction::And,
    };
    if computation.polarity.trim().eq_ignore_ascii_case("drop") {
        // The clauses describe the rows to exclude: the rows kept are the complement,
        // negated clause by clause with the junction flipped (De Morgan), never re-read.
        for clause in &mut clauses {
            clause.comparator = clause.comparator.negated();
        }
        junction = match junction {
            Junction::And => Junction::Or,
            Junction::Or => Junction::And,
        };
    }
    // The shape after the filter: every column read is a column of the request, every
    // output name is a word of the request.
    let group_by = computation.group_by.trim();
    let group_by = if group_by.is_empty() {
        None
    } else {
        if !names_field(group_by) {
            return None;
        }
        Some(group_by.to_owned())
    };
    let mut aggregations = Vec::new();
    for aggregation in &computation.aggregations {
        let op = AggOp::from_word(&aggregation.op)?;
        let field = aggregation.field.trim();
        let field = if field.is_empty() {
            if op != AggOp::Count {
                return None;
            }
            None
        } else {
            if !names_field(field) {
                return None;
            }
            Some(field.to_owned())
        };
        let name = aggregation.name.trim();
        if name.is_empty() || name.len() > 64 || !lower.contains(&name.to_lowercase()) {
            return None;
        }
        let round = aggregation.round.trim();
        let round = if round.is_empty() {
            None
        } else {
            let n: u32 = round.parse().ok()?;
            if n > 6 {
                return None;
            }
            Some(n)
        };
        aggregations.push(Aggregation {
            field,
            op,
            name: name.to_owned(),
            round,
        });
    }
    if group_by.is_some() && aggregations.is_empty() {
        return None;
    }
    let produced: Vec<&str> = group_by
        .iter()
        .map(String::as_str)
        .chain(aggregations.iter().map(|a| a.name.as_str()))
        .collect();
    let sort_by = computation.sort_by.trim();
    let sort_by = if sort_by.is_empty() {
        None
    } else {
        if !(names_field(sort_by) || produced.contains(&sort_by)) {
            return None;
        }
        Some((
            sort_by.to_owned(),
            computation.order.trim().eq_ignore_ascii_case("desc"),
        ))
    };
    let mut columns = Vec::new();
    for column in &computation.columns {
        let column = column.trim();
        if column.is_empty() {
            continue;
        }
        if !(names_field(column) || produced.contains(&column)) {
            return None;
        }
        columns.push(column.to_owned());
    }
    let shape = Shape {
        group_by,
        aggregations,
        sort_by,
        columns,
        ..Shape::default()
    };
    if clauses.is_empty() && shape == Shape::default() {
        return None;
    }
    Some(Rule::typed(evidence, clauses, junction, shape))
}

/// The schema of a typed computation on a compute step: every key required (a strict
/// schema needs no optional), empty strings and arrays meaning absent.
pub(super) fn computation_schema() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["present","polarity","join","clauses","group_by","aggregations","sort_by","order","columns"],"properties":{
        "present":{"type":"boolean"},
        "polarity":{"type":"string","enum":["keep","drop"]},
        "join":{"type":"string","enum":["and","or"]},
        "clauses":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["field","op","value","value_field"],"properties":{
            "field":{"type":"string"},"op":{"type":"string","enum":["gt","ge","lt","le","eq","ne"]},
            "value":{"type":"string"},"value_field":{"type":"string"}}}},
        "group_by":{"type":"string"},
        "aggregations":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["field","op","as","round"],"properties":{
            "field":{"type":"string"},"op":{"type":"string","enum":["sum","count","avg","min","max"]},
            "as":{"type":"string"},"round":{"type":"string"}}}},
        "sort_by":{"type":"string"},
        "order":{"type":"string","enum":["asc","desc",""]},
        "columns":{"type":"array","items":{"type":"string"}}}})
}
