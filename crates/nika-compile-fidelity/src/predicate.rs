// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The typed computation: a row filter, a grouping with aggregations, totals over every row,
//! a sort and a projection the semantic frontend states as meaning (columns, comparators,
//! literals, polarity, junction, output names) and the compiler validates part by part
//! against the request before lowering it deterministically. The model is never asked to
//! write the jq that becomes the contract; a computation that does not check out is no rule.
//!
//! The law is shared by creation and replay (R4 S0): a seat's computation is admitted by it
//! when the compiler reads the proposal, and a recorded rule is re-derived by it when a record
//! comes back. It descended from `nika-compile-cognition`, where the seat's wire is decoded
//! (strictly, once); here the same meaning is read from that JSON with the same field rules.

use nika_compile_reader::{cardinality, columns, plan, rule_tokens, rules, shape};
use rules::{AggOp, Aggregation, ArithOp, Derived, Shape, Term};
use serde_json::Value;

/// A computation stated as meaning: the rows kept or dropped, the grouping, the aggregates,
/// the ordering and the output columns, all in the request's own words.
struct ProposedComputation {
    polarity: String,
    join: String,
    clauses: Vec<ProposedClause>,
    group_by: String,
    aggregations: Vec<ProposedAggregation>,
    sort_by: String,
    order: String,
    ties: String,
    columns: Vec<String>,
    numbers: Vec<String>,
    derived: Vec<ProposedDerived>,
    limit: String,
    renames: Vec<ProposedRename>,
    distinct_by: Vec<String>,
}
struct ProposedRename {
    from: String,
    to: String,
}
struct ProposedDerived {
    name: String,
    op: String,
    left: String,
    right: String,
}
struct ProposedClause {
    field: String,
    op: String,
    value: String,
    value_field: String,
}
struct ProposedAggregation {
    field: String,
    op: String,
    name: String,
    round: String,
}

/// The fields of one object of the meaning: only the listed keys, a missing or null optional
/// field read as empty, a required one present and a string.
struct Fields<'a>(&'a serde_json::Map<String, Value>);
impl<'a> Fields<'a> {
    fn of(value: &'a Value, keys: &[&str]) -> Option<Self> {
        let object = value.as_object()?;
        object
            .keys()
            .all(|k| keys.contains(&k.as_str()))
            .then_some(Self(object))
    }
    fn text(&self, key: &str) -> Option<String> {
        match self.0.get(key) {
            None | Some(Value::Null) => Some(String::new()),
            Some(value) => value.as_str().map(str::to_owned),
        }
    }
    fn required(&self, key: &str) -> Option<String> {
        self.0.get(key)?.as_str().map(str::to_owned)
    }
    fn list<T>(&self, key: &str, item: impl Fn(&Value) -> Option<T>) -> Option<Vec<T>> {
        match self.0.get(key) {
            None | Some(Value::Null) => Some(Vec::new()),
            Some(value) => value.as_array()?.iter().map(item).collect(),
        }
    }
}

impl ProposedComputation {
    /// The meaning as the seat's wire states it (the fields of the computation schema).
    fn from_json(value: &Value) -> Option<Self> {
        let keys = [
            "present",
            "polarity",
            "join",
            "clauses",
            "group_by",
            "aggregations",
            "sort_by",
            "order",
            "ties",
            "columns",
            "numbers",
            "derived",
            "limit",
            "renames",
            "distinct_by",
        ];
        let f = Fields::of(value, &keys)?;
        f.0.get("present")?.as_bool()?;
        let clause = |v: &Value| {
            let c = Fields::of(v, &["field", "op", "value", "value_field"])?;
            Some(ProposedClause {
                field: c.required("field")?,
                op: c.required("op")?,
                value: c.text("value")?,
                value_field: c.text("value_field")?,
            })
        };
        let aggregation = |v: &Value| {
            let a = Fields::of(v, &["field", "op", "as", "round"])?;
            Some(ProposedAggregation {
                field: a.text("field")?,
                op: a.required("op")?,
                name: a.text("as")?,
                round: a.text("round")?,
            })
        };
        let derived = |v: &Value| {
            let d = Fields::of(v, &["as", "op", "left", "right"])?;
            Some(ProposedDerived {
                name: d.text("as")?,
                op: d.required("op")?,
                left: d.text("left")?,
                right: d.text("right")?,
            })
        };
        let rename = |v: &Value| {
            let r = Fields::of(v, &["from", "to"])?;
            Some(ProposedRename {
                from: r.text("from")?,
                to: r.text("to")?,
            })
        };
        let word = |v: &Value| v.as_str().map(str::to_owned);
        Some(Self {
            polarity: f.text("polarity")?,
            join: f.text("join")?,
            clauses: f.list("clauses", clause)?,
            group_by: f.text("group_by")?,
            aggregations: f.list("aggregations", aggregation)?,
            sort_by: f.text("sort_by")?,
            order: f.text("order")?,
            ties: f.text("ties")?,
            columns: f.list("columns", word)?,
            numbers: f.list("numbers", word)?,
            derived: f.list("derived", derived)?,
            limit: f.text("limit")?,
            renames: f.list("renames", rename)?,
            distinct_by: f.list("distinct_by", word)?,
        })
    }
}

/// The typed rule a seat's computation (its wire JSON) states, validated part by part against
/// the request and lowered deterministically, with the slots it asks; `None` when any part does
/// not check out or the JSON is not that meaning.
#[must_use]
pub fn typed_rule(
    intent: &str,
    evidence: &str,
    computation: &Value,
    unknowns: &[String],
    columns: &[String],
) -> Option<(rules::Rule, Vec<plan::Slot>)> {
    typed(
        intent,
        evidence,
        &ProposedComputation::from_json(computation)?,
        unknowns,
        columns,
    )
}

/// Where a literal of the computation must be stated: the reader's own clauses of the request
/// that hold its evidence (its sentences, cut where the reader cuts them), never the seat's
/// citation boundary and never another clause (a schedule's hour, another filter's number).
/// Evidence the request does not hold verbatim is its own scope.
fn clause_scope(intent: &str, evidence: &str) -> String {
    let evidence = evidence.trim();
    let Some(at) = intent.find(evidence) else {
        return evidence.to_owned();
    };
    let (mut start, mut end) = (at, at + evidence.len());
    let mut cursor = 0;
    for sentence in nika_compile_reader::lexicon::split_sentences(intent) {
        let Some(from) = intent.get(cursor..).and_then(|rest| rest.find(sentence)) else {
            continue;
        };
        cursor += from;
        for clause in nika_compile_reader::lexicon::split_clauses(sentence) {
            let Some(inner) = intent.get(cursor..).and_then(|rest| rest.find(clause)) else {
                continue;
            };
            let (from, to) = (cursor + inner, cursor + inner + clause.len());
            if from < at + evidence.len() && at < to {
                (start, end) = (start.min(from), end.max(to));
            }
            cursor = to;
        }
    }
    intent.get(start..end).unwrap_or(evidence).to_owned()
}

/// The numbers a text writes in digits, canonical (`1,5` as `1.5`, no surrounding stops).
fn digit_runs(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_digit() && c != '.' && c != ',')
        .filter(|run| run.chars().any(|c| c.is_ascii_digit()))
        .map(|run| run.trim_matches(['.', ',']).replace(',', "."))
        .collect()
}

/// Whether the request spells the number as a word (« trois », « drei », « tre »).
fn number_word_states(intent: &str, n: u32) -> bool {
    shape::fold(intent)
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| {
            cardinality::NUMBER_WORDS
                .iter()
                .any(|(word, value)| *value == n && *word == w)
        })
}

/// The truth value a word spells, in six languages; anything else is text.
fn boolean_word(word: &str) -> Option<bool> {
    match shape::fold(word).as_str() {
        "true" | "vrai" | "vraie" | "verdadero" | "verdadera" | "vero" | "vera" | "wahr"
        | "verdadeiro" | "verdadeira" => Some(true),
        "false" | "faux" | "fausse" | "falso" | "falsa" | "falsch" => Some(false),
        _ => None,
    }
}

/// The const key of a value the request alludes to: its words, lowercased, ASCII letters
/// and digits joined by underscores (« seuil d'alerte » → `seuil_d_alerte`).
fn slot_slug(words: &str) -> String {
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
fn typed(
    intent: &str,
    evidence: &str,
    computation: &ProposedComputation,
    unknowns: &[String],
    columns: &[String],
) -> Option<(rules::Rule, Vec<plan::Slot>)> {
    use rules::{Clause, Comparator, Junction, Operand, Rule};
    let mut slots: Vec<plan::Slot> = Vec::new();
    let lower = intent.to_lowercase();
    let hint = columns::columns_hint(intent);
    // A source column is one the request lists when it lists its columns; only a request
    // that names no columns lets any of its words stand for one. An output name the request
    // states ("as") is a word of the request, never a source column.
    let names_field = |field: &str| {
        let field = field.trim();
        if field.is_empty() || field.len() > 64 {
            return false;
        }
        // Observed keys may ground a proposal; a request word remains provisional.
        // The shared binding boundary checks every resulting source field and asks
        // for an explicit mapping before any candidate can become READY.
        columns.iter().any(|c| c == field)
            || if hint.is_empty() {
                lower.contains(&field.to_lowercase())
            } else {
                hint.iter().any(|c| c.eq_ignore_ascii_case(field))
            }
    };
    // A literal is grounded in the clause that states it (R4 S0 option 2): a value or number
    // the request states in another clause (« every weekday at 8 ») is not this computation's.
    let scope = clause_scope(intent, evidence);
    let (said, digit_runs) = (scope.to_lowercase(), digit_runs(&scope));
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
                    .find(|u| rule_tokens::fold(u) == rule_tokens::fold(unquoted))
                {
                    let slug = slot_slug(unknown);
                    if slug.is_empty() {
                        return None;
                    }
                    let key = format!("const.{slug}");
                    if !slots.iter().any(|s| s.key == key) {
                        slots.push(plan::Slot::new(
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
                if unquoted.is_empty() || !said.contains(&unquoted.to_lowercase()) {
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
        if n == 0 || !(digit_runs.iter().any(|run| run == limit) || number_word_states(&scope, n)) {
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
    let shape = with_order(computation, shape)?;
    if clauses.is_empty() && shape == Shape::default() {
        return None;
    }
    Some((Rule::typed(evidence, clauses, junction, shape), slots))
}

/// The shape with the tie rule and the output columns written as JSON numbers the computation
/// states (E38), each admitted only where it can hold: the tie rule settles a sort over rows still
/// in file order (never a grouping), and a number column is a projected column, named once, never
/// over totals. Any other word, or either where it cannot hold, is `None`: no rule.
fn with_order(computation: &ProposedComputation, mut shape: Shape) -> Option<Shape> {
    shape.ties_first_in_file = match computation.ties.trim() {
        "" => false,
        "first_in_file" if shape.sort_by.is_some() && shape.group_by.is_none() => true,
        _ => return None,
    };
    let totals = shape.group_by.is_none() && !shape.aggregations.is_empty();
    for column in computation
        .numbers
        .iter()
        .map(|c| c.trim())
        .filter(|c| !c.is_empty())
    {
        let projected = shape.columns.iter().any(|c| c == column);
        if totals || !projected || shape.numbers.iter().any(|n| n == column) {
            return None;
        }
        shape.numbers.push(column.to_owned());
    }
    Some(shape)
}

/// Whether a recorded typed rule is exactly what this law admits for its own meaning over its
/// own words (R4 S0 B3, the replay's fixpoint of creation): the record read back as the seat's
/// meaning, validated against the request and lowered again, equals the rule, and every slot it
/// asks is recorded. This binds a rule to the law that admitted it, not to what its words mean:
/// an element the law grounds only somewhere in the request (a stated field, value, number or
/// limit) or not at all (a comparator, an aggregate, a junction, a direction) can still be
/// swapped for another the law admits — that hole is open (see the crate spec).
#[must_use]
pub fn rederives(
    rule: &rules::Rule,
    intent: &str,
    slots: &[plan::Slot],
    columns: &[String],
) -> bool {
    let Some(meaning) = meaning_of(&rule.to_json(), intent, slots) else {
        return false;
    };
    let unknowns: Vec<String> = slots.iter().map(|s| s.label.clone()).collect();
    typed(intent, rule.text(), &meaning, &unknowns, columns)
        .is_some_and(|(again, asked)| again == *rule && asked.iter().all(|s| slots.contains(s)))
}

/// The seat's meaning a recorded typed rule states, the inverse of the lowering: the kept rows
/// and their junction, every clause (a slot by its recorded label, a truth value by a word of
/// the request that spells it), the grouping, aggregates, ordering, projection, arithmetic,
/// limit, renames and duplicate keys. `None` for what the law never lowers (a join, plain
/// duplicates, a summary, lines, a program).
fn meaning_of(record: &Value, intent: &str, slots: &[plan::Slot]) -> Option<ProposedComputation> {
    let shape = record.get("shape")?;
    let never = shape.get("join_on").is_some_and(|v| !v.is_null())
        || shape.get("distinct") == Some(&Value::Bool(true))
        || record.get("summary") == Some(&Value::Bool(true))
        || record.get("lines") == Some(&Value::Bool(true))
        || record.get("program").is_some_and(|v| !v.is_null());
    if never {
        return None;
    }
    let word = |v: Option<&Value>| v.and_then(Value::as_str).map(str::to_owned);
    let words = |key: &str| -> Option<Vec<String>> {
        shape
            .get(key)?
            .as_array()?
            .iter()
            .map(|v| word(Some(v)))
            .collect()
    };
    let clauses = record.get("clauses")?.as_array()?.iter().map(|c| {
        let value = word(c.get("value"))?;
        let (value, value_field) = match c.get("value_kind").and_then(Value::as_str) {
            Some("column") => (String::new(), value),
            Some("slot") => {
                let key = format!("const.{value}");
                (
                    slots.iter().find(|s| s.key == key)?.label.clone(),
                    String::new(),
                )
            }
            Some("bool") => (truth_word(intent, value == "true")?, String::new()),
            _ => (value, String::new()),
        };
        Some(ProposedClause {
            field: word(c.get("field"))?,
            op: word(c.get("comparator"))?,
            value,
            value_field,
        })
    });
    let aggregations = shape.get("aggregations")?.as_array()?.iter().map(|a| {
        Some(ProposedAggregation {
            field: word(a.get("field")).unwrap_or_default(),
            op: word(a.get("op"))?,
            name: word(a.get("name"))?,
            round: a
                .get("round")
                .and_then(Value::as_u64)
                .map(|n| n.to_string())
                .unwrap_or_default(),
        })
    });
    let term = |t: &Value| word(t.get("name")).or_else(|| word(t.get("number")));
    let derived = shape.get("derived")?.as_array()?.iter().map(|d| {
        Some(ProposedDerived {
            name: word(d.get("name"))?,
            op: word(d.get("op"))?,
            left: term(d.get("left")?)?,
            right: term(d.get("right")?)?,
        })
    });
    let renames = shape.get("renames")?.as_array()?.iter().map(|r| {
        Some(ProposedRename {
            from: word(r.get("from"))?,
            to: word(r.get("to"))?,
        })
    });
    let descending = shape.get("descending") == Some(&Value::Bool(true));
    Some(ProposedComputation {
        polarity: "keep".to_owned(),
        join: word(record.get("junction")).unwrap_or_default(),
        clauses: clauses.collect::<Option<_>>()?,
        group_by: word(shape.get("group_by")).unwrap_or_default(),
        aggregations: aggregations.collect::<Option<_>>()?,
        sort_by: word(shape.get("sort_by")).unwrap_or_default(),
        order: if descending { "desc" } else { "asc" }.to_owned(),
        ties: word(shape.get("ties")).unwrap_or_default(),
        columns: words("columns")?,
        numbers: words("numbers").unwrap_or_default(),
        derived: derived.collect::<Option<_>>()?,
        limit: shape
            .get("limit")
            .and_then(Value::as_u64)
            .map(|n| n.to_string())
            .unwrap_or_default(),
        renames: renames.collect::<Option<_>>()?,
        distinct_by: words("distinct_by")?,
    })
}

/// A word of the request that spells this truth value (« vrai », « false »), as a seat's
/// value must be a word of the request.
fn truth_word(intent: &str, truth: bool) -> Option<String> {
    intent
        .split(|c: char| !c.is_alphanumeric())
        .find(|w| boolean_word(w) == Some(truth))
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::typed_rule;
    use serde_json::Value;

    /// The typed rule alone, no unknowns to slot.
    fn typed(
        intent: &str,
        evidence: &str,
        computation: &Value,
    ) -> Option<nika_compile_reader::rules::Rule> {
        typed_rule(intent, evidence, computation, &[], &[]).map(|(rule, _)| rule)
    }
    use serde_json::json;

    // wave28 v2-02: the seat compared a boolean column to the string "false" and jq kept no
    // row; the run was green on an empty file.
    #[test]
    fn a_truth_value_matches_the_boolean_and_its_spelling() {
        let computation = json!({
            "present": true, "polarity": "keep", "join": "and",
            "clauses": [{"field": "explicito", "op": "==", "value": "false", "value_field": ""}],
            "group_by": "", "aggregations": [], "sort_by": "duracao_s", "order": "desc",
            "columns": ["id", "titulo"], "derived": []
        });
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
        let computation = proposed("3");
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
        let unstated = proposed("5");
        assert!(typed(intent, "keep the top 3 items by units", &unstated).is_none());
        let spelled = "Read ./shop/sales.csv (columns item,units), keep the three best items by units, rename item to product and write ./out/top.csv";
        assert!(typed(spelled, "keep the three best items by units", &computation).is_some());
        // No limit under a ranking word: the count is missing and must be asked.
        let ranked = proposed("");
        let rule = typed(intent, "the top-selling items by units", &ranked).expect("a rule");
        assert!(rule.ranking_without_count());
        assert!(!rule.with_limit(3).ranking_without_count());
        let plain = typed(intent, "sorted by units, descending", &ranked).expect("a rule");
        assert!(!plain.ranking_without_count());
    }

    // E38 V2 C3: a stated tie rule and output columns written as numbers are typed stages of
    // the computation; each is admitted only where it can hold, and a rule re-derives with them.
    #[test]
    fn a_stated_tie_rule_and_number_columns_are_typed_where_they_hold() {
        let intent = "Read ./shop/orders.csv, order the rows by amount from highest to lowest, the first in the file first on equal amounts, keep the first 2 rows and write id and amount as a number to ./out/top.json";
        let evidence = "order the rows by amount from highest to lowest, the first in the file first on equal amounts, keep the first 2 rows";
        let proposed = |extra: &Value| {
            let mut computation = json!({"present": true, "polarity": "keep", "join": "and",
                "clauses": [], "group_by": "", "aggregations": [], "sort_by": "amount",
                "order": "desc", "columns": ["id", "amount"], "derived": [], "limit": "2"});
            for (key, value) in extra.as_object().expect("an object") {
                computation[key] = value.clone();
            }
            computation
        };
        let stated = proposed(&json!({"ties": "first_in_file", "numbers": ["amount"]}));
        let rule = typed(intent, evidence, &stated).expect("a rule");
        let record = rule.to_json();
        assert_eq!(record["shape"]["ties"], "first_in_file");
        assert_eq!(record["shape"]["numbers"], json!(["amount"]));
        assert!(rule.jq().contains(" | reverse | sort_by("), "{}", rule.jq());
        assert!(!rule.jq().contains("dtie("), "{}", rule.jq());
        assert!(super::rederives(&rule, intent, &[], &[]));
        // Unstated, the record says nothing of either, as before.
        let plain = typed(intent, evidence, &proposed(&json!({}))).expect("a rule");
        let shape = &plain.to_json()["shape"];
        assert!(shape.get("ties").is_none() && shape.get("numbers").is_none());
        // Grouped rows are ordered by their key, not the file: a tie rule there is no rule.
        let grouped = json!({"group_by": "id", "aggregations": [{"field": "amount", "op": "sum", "as": "amount", "round": ""}]});
        assert!(typed(intent, evidence, &proposed(&grouped)).is_some());
        let mut tied = grouped;
        tied["ties"] = json!("first_in_file");
        assert!(typed(intent, evidence, &proposed(&tied)).is_none());
        // Over no sort, a word the law does not know, a column the rows are not projected on,
        // one named twice, or totals: no rule.
        let totals = json!({"aggregations": [{"field": "amount", "op": "sum", "as": "amount", "round": ""}],
            "sort_by": "", "order": "", "limit": "", "numbers": ["amount"]});
        for extra in [
            json!({"ties": "first_in_file", "sort_by": "", "order": ""}),
            json!({"ties": "last_in_file"}),
            json!({"numbers": ["region"]}),
            json!({"numbers": ["amount", "amount"]}),
            totals,
        ] {
            assert!(
                typed(intent, evidence, &proposed(&extra)).is_none(),
                "{extra}"
            );
        }
    }
}
