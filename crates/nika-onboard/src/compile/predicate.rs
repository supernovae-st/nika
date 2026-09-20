//! The typed predicate: a row filter the semantic frontend states as meaning (columns,
//! comparators, literals, polarity, junction) and the compiler validates part by part
//! against the request before lowering it deterministically. The model is never asked to
//! write the jq that becomes the contract; a predicate that does not check out is no rule.

use serde::Deserialize;

/// A row filter stated as meaning: columns, comparators and literals of the request itself.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProposedPredicate {
    pub(super) present: bool,
    #[serde(default, deserialize_with = "super::cognition::nullable_string")]
    pub(super) polarity: String,
    #[serde(default, deserialize_with = "super::cognition::nullable_string")]
    pub(super) join: String,
    #[serde(default, deserialize_with = "nullable_clauses")]
    pub(super) clauses: Vec<ProposedClause>,
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
fn nullable_clauses<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<ProposedClause>, D::Error> {
    Ok(Option::<Vec<ProposedClause>>::deserialize(d)?.unwrap_or_default())
}

/// A typed predicate the proposal stated, validated part by part against the request: every
/// field is a column the request names (a columns hint or a word of the text), every literal
/// value occurs in the request, every comparator is one of the closed set. Anything else is
/// no rule (the closed grammar or a question takes over), never a guess.
pub(super) fn typed_rule(
    intent: &str,
    evidence: &str,
    predicate: &ProposedPredicate,
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
    for clause in &predicate.clauses {
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
    if clauses.is_empty() {
        return None;
    }
    let mut junction = match predicate.join.trim() {
        "or" => Junction::Or,
        _ => Junction::And,
    };
    if predicate.polarity.trim().eq_ignore_ascii_case("drop") {
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
    Some(Rule::typed(evidence, clauses, junction))
}
