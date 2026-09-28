//! The typed computation: a row filter, a grouping with aggregations, totals over every row,
//! a sort and a projection the semantic frontend states as meaning (columns, comparators,
//! literals, polarity, junction, output names) and the compiler validates part by part
//! against the request before lowering it deterministically. The model is never asked to
//! write the jq that becomes the contract; a computation that does not check out is no rule.
//!
//! Here the seat's wire is decoded, strictly and once; the law that validates and lowers it
//! is `nika_compile_fidelity::predicate::typed_rule`, shared with the replay (R4 S0).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::cognition::nullable_default;

/// A computation stated as meaning: the rows kept or dropped, the grouping, the aggregates,
/// the ordering and the output columns, all in the request's own words.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProposedComputation {
    pub(super) present: bool,
    #[serde(default, deserialize_with = "nullable_default")]
    pub(super) polarity: String,
    #[serde(default, deserialize_with = "nullable_default")]
    pub(super) join: String,
    #[serde(default, deserialize_with = "nullable_default")]
    pub(super) clauses: Vec<ProposedClause>,
    #[serde(default, deserialize_with = "nullable_default")]
    pub(super) group_by: String,
    #[serde(default, deserialize_with = "nullable_default")]
    pub(super) aggregations: Vec<ProposedAggregation>,
    #[serde(default, deserialize_with = "nullable_default")]
    pub(super) sort_by: String,
    #[serde(default, deserialize_with = "nullable_default")]
    pub(super) order: String,
    #[serde(default, deserialize_with = "nullable_default")]
    pub(super) columns: Vec<String>,
    #[serde(default, deserialize_with = "nullable_default")]
    pub(super) derived: Vec<ProposedDerived>,
    #[serde(default, deserialize_with = "nullable_default")]
    pub(super) limit: String,
    #[serde(default, deserialize_with = "nullable_default")]
    pub(super) renames: Vec<ProposedRename>,
    #[serde(default, deserialize_with = "nullable_default")]
    pub(super) distinct_by: Vec<String>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProposedRename {
    #[serde(default, deserialize_with = "nullable_default")]
    pub(super) from: String,
    #[serde(default, deserialize_with = "nullable_default")]
    pub(super) to: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProposedDerived {
    #[serde(rename = "as", default, deserialize_with = "nullable_default")]
    pub(super) name: String,
    pub(super) op: String,
    #[serde(default, deserialize_with = "nullable_default")]
    pub(super) left: String,
    #[serde(default, deserialize_with = "nullable_default")]
    pub(super) right: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProposedClause {
    pub(super) field: String,
    pub(super) op: String,
    #[serde(default, deserialize_with = "nullable_default")]
    pub(super) value: String,
    #[serde(default, deserialize_with = "nullable_default")]
    pub(super) value_field: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProposedAggregation {
    #[serde(default, deserialize_with = "nullable_default")]
    pub(super) field: String,
    pub(super) op: String,
    #[serde(rename = "as", default, deserialize_with = "nullable_default")]
    pub(super) name: String,
    #[serde(default, deserialize_with = "nullable_default")]
    pub(super) round: String,
}

/// The schema of a typed computation on a compute step: every key required (a strict
/// schema needs no optional), empty strings and arrays meaning absent.
pub(super) fn computation_schema() -> Value {
    serde_json::from_str(include_str!("../assets/computation_schema.json"))
        .unwrap_or_else(|_| json!({"type": "object"}))
}
