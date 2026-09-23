//! The typed computation: a row filter, a grouping with aggregations, totals over every row,
//! a sort and a projection the semantic frontend states as meaning (columns, comparators,
//! literals, polarity, junction, output names) and the compiler validates part by part
//! against the request before lowering it deterministically. The model is never asked to
//! write the jq that becomes the contract; a computation that does not check out is no rule.

use serde::Deserialize;
use serde_json::{Value, json};

use super::rules::{AggOp, Aggregation, ArithOp, Derived, Shape, Term};

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
    #[serde(default, deserialize_with = "nullable_derived")]
    pub(super) derived: Vec<ProposedDerived>,
    #[serde(default, deserialize_with = "super::cognition::nullable_string")]
    pub(super) limit: String,
    #[serde(default, deserialize_with = "nullable_renames")]
    pub(super) renames: Vec<ProposedRename>,
    #[serde(default, deserialize_with = "super::cognition::nullable_vec")]
    pub(super) distinct_by: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProposedRename {
    #[serde(default, deserialize_with = "super::cognition::nullable_string")]
    pub(super) from: String,
    #[serde(default, deserialize_with = "super::cognition::nullable_string")]
    pub(super) to: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProposedDerived {
    #[serde(
        rename = "as",
        default,
        deserialize_with = "super::cognition::nullable_string"
    )]
    pub(super) name: String,
    pub(super) op: String,
    #[serde(default, deserialize_with = "super::cognition::nullable_string")]
    pub(super) left: String,
    #[serde(default, deserialize_with = "super::cognition::nullable_string")]
    pub(super) right: String,
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
fn nullable_renames<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<ProposedRename>, D::Error> {
    Ok(Option::<Vec<ProposedRename>>::deserialize(d)?.unwrap_or_default())
}
fn nullable_derived<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<ProposedDerived>, D::Error> {
    Ok(Option::<Vec<ProposedDerived>>::deserialize(d)?.unwrap_or_default())
}
fn nullable_aggregations<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<ProposedAggregation>, D::Error> {
    Ok(Option::<Vec<ProposedAggregation>>::deserialize(d)?.unwrap_or_default())
}

/// Whether the request spells the number as a word (« trois », « drei », « tre »).
fn number_word_states(intent: &str, n: u32) -> bool {
    super::shape::fold(intent)
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| {
            super::cardinality::NUMBER_WORDS
                .iter()
                .any(|(word, value)| *value == n && *word == w)
        })
}

/// The truth value a word spells, in six languages; anything else is text.
fn boolean_word(word: &str) -> Option<bool> {
    match super::shape::fold(word).as_str() {
        "true" | "vrai" | "vraie" | "verdadero" | "verdadera" | "vero" | "vera" | "wahr"
        | "verdadeiro" | "verdadeira" => Some(true),
        "false" | "faux" | "fausse" | "falso" | "falsa" | "falsch" => Some(false),
        _ => None,
    }
}

/// The const key of a value the request alludes to: its words, lowercased, ASCII letters
/// and digits joined by underscores (« seuil d'alerte » → `seuil_d_alerte`).
pub(super) fn slot_slug(words: &str) -> String {
    let mut slug = String::new();
    for c in words.to_lowercase().chars() {
        let mapped = match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' | 'å' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' | 'ì' => 'i',
            'ô' | 'ö' | 'ó' | 'ò' | 'õ' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            other => other,
        };
        if mapped.is_ascii_alphanumeric() {
            slug.push(mapped);
        } else if mapped == 'ß' {
            slug.push_str("ss");
        } else if !slug.is_empty() && !slug.ends_with('_') {
            slug.push('_');
        }
    }
    slug.trim_end_matches('_').to_owned()
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
    unknowns: &[String],
) -> Option<(super::rules::Rule, Vec<super::plan::Slot>)> {
    use super::rules::{Clause, Comparator, Junction, Operand, Rule};
    let mut slots: Vec<super::plan::Slot> = Vec::new();
    let lower = intent.to_lowercase();
    let hint = super::columns::columns_hint(intent);
    // A source column is one the request lists when it lists its columns; only a request
    // that names no columns lets any of its words stand for one. An output name the request
    // states ("as") is a word of the request, never a source column.
    let names_field = |field: &str| {
        let field = field.trim();
        if field.is_empty() || field.len() > 64 {
            return false;
        }
        if hint.is_empty() {
            lower.contains(&field.to_lowercase())
        } else {
            hint.iter().any(|c| c.eq_ignore_ascii_case(field))
        }
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
                // « no », “no”, 'no': the quotes a request wears around a value are not the value.
                let unquoted = literal
                    .trim_matches(['"', '\'', '“', '”', '‘', '’', '«', '»', '‹', '›'])
                    .trim();
                // « temperature > "seuil d'alerte" »: the value is one the seat listed as
                // unknown — a slot the compiler asks for, never the words compared.
                if let Some(unknown) = unknowns
                    .iter()
                    .find(|u| super::rule_tokens::fold(u) == super::rule_tokens::fold(unquoted))
                {
                    let slug = slot_slug(unknown);
                    if slug.is_empty() {
                        return None;
                    }
                    let key = format!("const.{slug}");
                    if !slots.iter().any(|s| s.key == key) {
                        slots.push(super::plan::Slot::new(
                            key,
                            unknown.trim().to_owned(),
                            Rule::compares_numbers(comparator),
                        ));
                    }
                    clauses.push(Clause::new(
                        clause.field.trim(),
                        comparator,
                        Operand::Slot(slug),
                    ));
                    continue;
                }
                if unquoted.is_empty() || !lower.contains(&unquoted.to_lowercase()) {
                    return None;
                }
                // « explicito == false », « attivo = vero »: the truth value, whichever way
                // the file encodes it (a boolean in JSON, its spelling in CSV).
                match boolean_word(unquoted) {
                    Some(truth) => Operand::Bool(truth),
                    None => Operand::Text(unquoted.to_owned()),
                }
            }
        } else {
            if !names_field(other) {
                return None;
            }
            Operand::Column(other.to_owned())
        };
        clauses.push(Clause::new(clause.field.trim(), comparator, value));
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
            other => other,
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
        aggregations.push(Aggregation::new(field, op, name, round));
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
    // Arithmetic over the outputs: each side is an output already produced or a number
    // of the request; an output the request defines that way is never an aggregate.
    let mut derived: Vec<Derived> = Vec::new();
    for entry in &computation.derived {
        let name = entry.name.trim();
        if name.is_empty() || name.len() > 64 || !lower.contains(&name.to_lowercase()) {
            return None;
        }
        let op = ArithOp::from_word(&entry.op)?;
        let known: Vec<&str> = produced
            .iter()
            .copied()
            .chain(derived.iter().map(|d| d.name.as_str()))
            .collect();
        let term = |text: &str| -> Option<Term> {
            let text = text.trim();
            if known.contains(&text) {
                return Some(Term::Name(text.to_owned()));
            }
            let numeric = text.replace(',', ".");
            if numeric.parse::<f64>().is_ok() && digit_runs.iter().any(|run| run == &numeric) {
                return Some(Term::Number(numeric));
            }
            None
        };
        let (left, right) = (term(&entry.left)?, term(&entry.right)?);
        derived.push(Derived::new(name, op, left, right));
    }
    if !derived.is_empty() && aggregations.is_empty() {
        return None;
    }
    // A renamed key is a column the request names, renamed to a word of the request; a
    // limit is a number the request states, as digits or as a word.
    let mut renames = Vec::new();
    for rename in &computation.renames {
        let (from, to) = (rename.from.trim(), rename.to.trim());
        if from.is_empty() || to.is_empty() || from == to || to.len() > 64 {
            return None;
        }
        let known =
            names_field(from) || produced.contains(&from) || columns.iter().any(|c| c == from);
        if !known || !lower.contains(&to.to_lowercase()) {
            return None;
        }
        renames.push((from.to_owned(), to.to_owned()));
    }
    let limit = computation.limit.trim();
    let limit = if limit.is_empty() {
        None
    } else {
        let n: u32 = limit.parse().ok()?;
        if n == 0 || !(digit_runs.iter().any(|run| run == limit) || number_word_states(intent, n)) {
            return None;
        }
        Some(n)
    };
    // The key columns of a removal of duplicates: every one a column the request names.
    let mut distinct_by = Vec::new();
    for column in &computation.distinct_by {
        let column = column.trim();
        if column.is_empty() {
            continue;
        }
        if !names_field(column) || distinct_by.iter().any(|k| k == column) {
            return None;
        }
        distinct_by.push(column.to_owned());
    }
    let mut shape = Shape::default();
    shape.distinct_by = distinct_by;
    shape.group_by = group_by;
    shape.aggregations = aggregations;
    shape.sort_by = sort_by;
    shape.columns = columns;
    shape.derived = derived;
    shape.limit = limit;
    shape.renames = renames;
    if clauses.is_empty() && shape == Shape::default() {
        return None;
    }
    Some((Rule::typed(evidence, clauses, junction, shape), slots))
}

/// The schema of a typed computation on a compute step: every key required (a strict
/// schema needs no optional), empty strings and arrays meaning absent.
pub(super) fn computation_schema() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["present","polarity","join","clauses","group_by","aggregations","sort_by","order","columns","derived","limit","renames","distinct_by"],"properties":{
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
        "columns":{"type":"array","items":{"type":"string"}},
        "derived":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["as","op","left","right"],"properties":{
            "as":{"type":"string"},"op":{"type":"string","enum":["sub","add","mul","div"]},
            "left":{"type":"string"},"right":{"type":"string"}}}},
        "limit":{"type":"string"},
        "renames":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["from","to"],"properties":{
            "from":{"type":"string"},"to":{"type":"string"}}}},
        "distinct_by":{"type":"array","items":{"type":"string"}}}})
}

#[cfg(test)]
mod tests {
    use super::{ProposedComputation, typed_rule};

    /// The typed rule alone, no unknowns to slot.
    fn typed(
        intent: &str,
        evidence: &str,
        computation: &ProposedComputation,
    ) -> Option<crate::rules::Rule> {
        typed_rule(intent, evidence, computation, &[]).map(|(rule, _)| rule)
    }
    use serde_json::json;

    // wave28 v2-02: the seat compared a boolean column to the string "false" and jq kept no
    // row; the run was green on an empty file.
    #[test]
    fn a_truth_value_matches_the_boolean_and_its_spelling() {
        let computation = serde_json::from_value::<ProposedComputation>(json!({
            "present": true, "polarity": "keep", "join": "and",
            "clauses": [{"field": "explicito", "op": "==", "value": "false", "value_field": ""}],
            "group_by": "", "aggregations": [], "sort_by": "duracao_s", "order": "desc",
            "columns": ["id", "titulo"], "derived": []
        }))
        .unwrap();
        let intent = "Lê ./musica/faixas.json (id, titulo, artista, duracao_s, explicito), guarda as faixas com explicito == false ordenadas por duracao_s decrescente e escreve id e titulo em ./out/limpa.json";
        let rule = typed(
            intent,
            "guarda as faixas com explicito == false",
            &computation,
        )
        .expect("a rule");
        assert!(
            rule.jq()
                .contains("select((.explicito == false or .explicito == \"false\"))"),
            "{}",
            rule.jq()
        );
        let record = rule.to_json();
        assert_eq!(record["clauses"][0]["value_kind"], "bool");
        assert_eq!(record["clauses"][0]["value"], "false");
    }

    // arc 3: a rename and a limit are typed stages of the computation; a ranking that states
    // no count is flagged so the binder asks for it (wave28 v2-53 compiled a full sort).
    #[test]
    fn a_rename_and_a_limit_are_typed_and_a_ranking_without_a_count_is_flagged() {
        let proposed = |limit: &str| {
            json!({
                "present": true, "polarity": "keep", "join": "and", "clauses": [],
                "group_by": "", "aggregations": [], "sort_by": "units", "order": "desc",
                "columns": ["item", "units"], "derived": [], "limit": limit,
                "renames": [{"from": "item", "to": "product"}]
            })
        };
        let computation = serde_json::from_value::<ProposedComputation>(proposed("3")).unwrap();
        let intent = "Read ./shop/sales.csv (columns item,units), keep the top 3 items by units, rename item to product and write ./out/top.csv";
        let rule = typed(intent, "keep the top 3 items by units", &computation).expect("a rule");
        assert!(rule.jq().contains("| .[:3] |"), "{}", rule.jq());
        assert!(
            rule.jq().contains(
                "map(with_entries(if .key == \"item\" then .key = \"product\" else . end))"
            ),
            "{}",
            rule.jq()
        );
        assert_eq!(
            rule.output_columns(),
            Some(vec!["product".to_owned(), "units".to_owned()])
        );
        assert_eq!(rule.renames().len(), 1);
        assert!(!rule.ranking_without_count());
        let record = rule.to_json();
        assert_eq!(record["shape"]["limit"], 3);
        assert_eq!(record["shape"]["renames"][0]["to"], "product");
        // A limit the request does not state is no rule; a count spelled as a word is stated.
        let unstated = serde_json::from_value::<ProposedComputation>(proposed("5")).unwrap();
        assert!(typed(intent, "keep the top 3 items by units", &unstated).is_none());
        let spelled = "Read ./shop/sales.csv (columns item,units), keep the three best items by units, rename item to product and write ./out/top.csv";
        assert!(typed(spelled, "keep the three best items by units", &computation).is_some());
        // No limit under a ranking word: the count is missing and must be asked.
        let ranked = serde_json::from_value::<ProposedComputation>(proposed("")).unwrap();
        let rule = typed(intent, "the top-selling items by units", &ranked).expect("a rule");
        assert!(rule.ranking_without_count());
        assert!(!rule.with_limit(3).ranking_without_count());
        let plain = typed(intent, "sorted by units, descending", &ranked).expect("a rule");
        assert!(!plain.ranking_without_count());
    }
}
