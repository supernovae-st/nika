// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The verified transform: a jq program a seat writes for a computation the typed stages
//! cannot state (a dedup by a composite key, a join, a set difference, a string rewrite),
//! judged before it is bound. The seat answers ONE bounded call with the program, the
//! columns it reads, a small example input and the output it expects; the compiler runs the
//! program in-process on that example with the SAME jaq stack and capability policy the
//! runtime `nika:jq` builtin installs (mirrored, as `nika-check-analyzer` mirrors it: the
//! layering hosts no shared engine below both), refuses a program that does not parse, that
//! does not return the seat's own expected output, that reads a column the request never
//! names or that carries a literal the request never wrote, and binds the rest as a rule
//! whose jq is visible at the task. A human is never asked for a jq expression; a refused
//! program leaves the question the assembler already asks, and the receipt says why.

use jaq_core::load::{Arena, Error as LoadError, File, Loader};
use jaq_core::{Compiler, Ctx, Vars, data as jaq_data};
use jaq_json::{Val, read};
use serde_json::Value;

/// The seat's answer to the transform question, decoded from its JSON.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProposedTransform {
    pub(super) jq: String,
    #[serde(default, deserialize_with = "super::nullable_default")]
    pub(super) columns_read: Vec<String>,
    pub(super) example_input: Value,
    pub(super) expected_output: Value,
}

/// Why a proposed program was refused: the verifier's counterexample, never a word about
/// meaning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Refusal(pub(super) String);

/// The most output bytes a verification run may produce (the runtime's own ceiling is
/// larger; an example is small by construction).
const MAX_OUTPUT_BYTES: usize = 65_536;

/// Run `program` over `input` exactly as the runtime builtin would, with the run-start
/// clock bound to a fixed instant: the one value it emits, or the reason it cannot.
pub(super) fn run(program: &str, input: &Value) -> Result<Value, Refusal> {
    let val = to_val(input)?;
    let (names, vals) = variables(input)?;
    let corrections = jaq_core::load::parse(JQ_STD_CORRECTIONS, |p| p.defs())
        .ok_or_else(|| Refusal("internal: the jq std corrections do not parse".to_owned()))?;
    let clock = jaq_core::load::parse(nika_cap::JQ_CLOCK_DEFS, |p| p.defs())
        .ok_or_else(|| Refusal("internal: the jq clock definitions do not parse".to_owned()))?;
    let defs = jaq_core::defs()
        .chain(jaq_std::defs().filter(|d| nika_cap::install_jq_definition(d.name)))
        .chain(jaq_json::defs())
        .chain(corrections)
        .chain(clock);
    let funs = jaq_core::funs()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs())
        .filter(|f| nika_cap::install_jq_native(f.0));
    let arena = Arena::default();
    let modules = Loader::new(defs)
        .load(
            &arena,
            File {
                code: program,
                path: (),
            },
        )
        .map_err(|errs| {
            Refusal(format!(
                "the program does not parse: {}",
                render_load(&errs)
            ))
        })?;
    let filter = Compiler::default()
        .with_funs(funs)
        .with_global_vars(
            std::iter::once(nika_cap::JQ_RUN_START_VAR).chain(names.iter().map(String::as_str)),
        )
        .compile(modules)
        .map_err(|errs| {
            Refusal(format!(
                "the program does not compile: {}",
                render_compile(&errs)
            ))
        })?;
    let ctx = Ctx::<jaq_data::JustLut<Val>>::new(
        &filter.lut,
        Vars::new(std::iter::once(Val::from(1_700_000_000_isize)).chain(vals)),
    );
    let mut single: Option<Value> = None;
    for result in filter.id.run((ctx, val)) {
        let value = match result {
            Ok(value) => value,
            Err(exception) => {
                return Err(Refusal(match exception.get_err() {
                    Ok(error) => format!("the program fails on the example: {error}"),
                    Err(_) => "the program uses process control the engine withholds".to_owned(),
                }));
            }
        };
        if single.is_some() {
            return Err(Refusal(
                "the program emits more than one value; a binding needs exactly one".to_owned(),
            ));
        }
        let text = value.to_string();
        if text.len() > MAX_OUTPUT_BYTES {
            return Err(Refusal(
                "the example output is larger than the verifier reads".to_owned(),
            ));
        }
        single = Some(
            serde_json::from_str(&text)
                .map_err(|e| Refusal(format!("the program's output is not JSON: {e}")))?,
        );
    }
    single.ok_or_else(|| {
        Refusal("the program emits no value; a binding needs exactly one".to_owned())
    })
}

fn to_val(value: &Value) -> Result<Val, Refusal> {
    let bytes = serde_json::to_vec(value).map_err(|e| Refusal(e.to_string()))?;
    read::parse_single(&bytes).map_err(|e| Refusal(format!("the input is not valid JSON: {e}")))
}

/// The variables an object input binds (`$records`, `$slots`), as the runtime binds them.
fn variables(input: &Value) -> Result<(Vec<String>, Vec<Val>), Refusal> {
    let mut names = Vec::new();
    let mut vals = Vec::new();
    for (key, value) in input.as_object().into_iter().flatten() {
        let identifier = key
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        let name = format!("${key}");
        if name == nika_cap::JQ_RUN_START_VAR || !identifier {
            continue;
        }
        names.push(name);
        vals.push(to_val(value)?);
    }
    Ok((names, vals))
}

/// The jq-std shadow the runtime installs (`scan` global by construction), verbatim.
const JQ_STD_CORRECTIONS: &str = r#"
def scan(re; flags): matches(re; "g" + flags)[] | .[0].string;
def scan(re): scan(re; "");
"#;

fn render_load(errs: &[(File<&str, ()>, LoadError<&str>)]) -> String {
    let Some((_, first)) = errs.first() else {
        return "does not parse".to_owned();
    };
    let near = |expected: &str, at: &str| {
        let at = at.trim();
        if at.is_empty() {
            format!("expected {expected} (unexpected end of input)")
        } else {
            format!(
                "expected {expected} near `{}`",
                at.chars().take(24).collect::<String>()
            )
        }
    };
    match first {
        LoadError::Io(v) => v
            .first()
            .map_or_else(|| "io error".to_owned(), |(_, m)| format!("io: {m}")),
        LoadError::Lex(v) => v.first().map_or_else(
            || "lexing error".to_owned(),
            |(exp, at)| near(exp.as_str(), at),
        ),
        LoadError::Parse(v) => v.first().map_or_else(
            || "parse error".to_owned(),
            |(exp, at)| near(exp.as_str(), at),
        ),
    }
}

#[allow(clippy::type_complexity)] // the shape is jaq's `compile::Errors`, not ours
fn render_compile<U>(errs: &[(File<&str, ()>, Vec<(&str, U)>)]) -> String {
    errs.first().and_then(|(_, v)| v.first()).map_or_else(
        || "compile error".to_owned(),
        |(name, _)| {
            nika_cap::withheld_jq_policy_reason(name)
                .unwrap_or_else(|| format!("undefined filter or variable `{name}`"))
        },
    )
}

// ── the one bounded call and the laws that judge its answer ─────────────────────────────

use super::{AuthoringPolicy, CompileOutcome, DiagnosticKind};
use crate::plan::{Op, Plan, Step};
use nika_compile::surface::pending_transform::PendingTransform;
use nika_kernel::ai::provider::{ContentBlock, Message, ProviderInferDyn, Role, StopReason};
use serde_json::json;
mod pending;
pub(super) use pending::resume;

/// The most transform calls one request may buy: two clauses the typed stages cannot state.
const MAX_CALLS: usize = 2;

/// The instruction of the transform call: the input shape, the closed rules, the answer.
const INSTRUCTION: &str = "You write ONE jq program for a workflow compiler. The program receives {records: [...]}: the parsed rows of the source file, JSON objects keyed by the request's own column names exactly as the file spells them (a CSV cell is text). It must return exactly one JSON value: the rows or the result the clause asks for, in the source order unless the request states a sort. Use only the supplied columns, and honor explicit field_choices when supplied; never invent a column, a literal, a default or an ordering; never use env, input, now, halt, any I/O, and never call a model. Return only one JSON object {jq, columns_read, example_input, expected_output}: jq is the program; columns_read lists every column it reads, as the file spells them; example_input is an array of 3 to 5 example records exercising the clause (duplicates, boundary values, the order kept) using those columns; expected_output is exactly what the program returns on example_input.";

/// The JSON schema of the transform answer.
fn schema() -> serde_json::Value {
    json!({"type":"object","additionalProperties":false,"required":["jq","columns_read","example_input","expected_output"],"properties":{
        "jq":{"type":"string","minLength":1},
        "columns_read":{"type":"array","items":{"type":"string"}},
        "example_input":{"type":"array","items":{"type":"object"}},
        "expected_output":{}}})
}

/// The compute steps of a plan whose computation no rule states: the typed stages could not
/// say it and the closed grammar cannot parse it. Each is one transform question.
fn unstated_computations<'a>(plan: &'a Plan, hint: &[String]) -> Vec<&'a Step> {
    plan.steps
        .iter()
        .filter(|step| step.op == Op::Compute)
        .filter(|step| {
            let detail = step.detail.trim();
            !plan
                .rules
                .iter()
                .any(|r| r.text() == detail || r.text() == step.evidence.trim())
                && crate::rules::synthesize(detail, hint).is_none()
        })
        .collect()
}

/// Ask the seat for a verified program on every computation the typed stages could not
/// state, at most [`MAX_CALLS`] per request; a verified program joins the plan's rules, a
/// refused one is recorded with its counterexample and leaves the assembler's own question.
pub(super) async fn synthesize<P: ProviderInferDyn>(
    intent: &str,
    plan: &mut Plan,
    policy: &AuthoringPolicy,
    provider: &P,
    request: &crate::CompileRequest,
    out: &mut CompileOutcome,
) -> Option<PendingTransform> {
    // An explicit answer to the rule question wins: nothing is asked of the seat.
    if request.answers.contains_key("const.rule_expression") {
        return None;
    }
    let observed = nika_compile::surface::observed::for_intent(request.knowledge.as_ref(), intent);
    let hint = observed
        .clone()
        .unwrap_or_else(|| crate::columns::columns_hint(intent));
    let pending: Vec<Step> = unstated_computations(plan, &hint)
        .into_iter()
        .take(MAX_CALLS)
        .cloned()
        .collect();
    let mut records = Vec::new();
    let mut pending_state = None;
    for step in pending {
        let state = json!({
            "request": intent,
            "clause": step.evidence,
            "computation": step.detail,
            "columns": hint,
        });
        let verdict = propose(policy, provider, state, out)
            .await
            .and_then(|proposed| {
                if let Some(columns) = &observed
                    && let Some(field) = proposed
                        .columns_read
                        .iter()
                        .find(|field| !columns.contains(field))
                {
                    let missing: Vec<String> = proposed
                        .columns_read
                        .iter()
                        .filter(|field| !columns.contains(field))
                        .cloned()
                        .collect();
                    pending_state = PendingTransform::new(intent, plan, &step, &missing, request);
                    return Err(Refusal(format!(
                        "`{field}` is not among the source's observed fields"
                    )));
                }
                verify(intent, &hint, &proposed).map(|()| proposed)
            });
        match verdict {
            Ok(proposed) => {
                crate::finding(
                    out,
                    DiagnosticKind::Applied,
                    "authoring_plan",
                    format!(
                        "`{}` is a computation the typed stages cannot state; the seat's program was verified on its own example and runs as the compute task.",
                        step.evidence.trim()
                    ),
                );
                records.push(json!({"clause": step.evidence, "accepted": true, "jq": proposed.jq, "columns": proposed.columns_read}));
                plan.rules.push(crate::rules::Rule::program(
                    step.detail.trim(),
                    proposed.jq.trim(),
                    proposed.columns_read,
                ));
            }
            Err(Refusal(why)) => {
                crate::finding(
                    out,
                    DiagnosticKind::RequiresHuman,
                    "authoring_transform",
                    format!(
                        "The seat's program for `{}` was refused: {why}. The computation stays asked.",
                        step.evidence.trim()
                    ),
                );
                records.push(json!({"clause": step.evidence, "accepted": false, "why": why}));
            }
        }
        if pending_state.is_some() {
            break;
        }
    }
    if !records.is_empty() {
        let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
        decision["transforms"] = json!(records);
        out.provenance.decision = Some(decision);
    }
    pending_state
}

/// One bounded transform call shared by initial synthesis and field-answer regeneration.
async fn propose<P: ProviderInferDyn>(
    policy: &AuthoringPolicy,
    provider: &P,
    state: Value,
    out: &mut CompileOutcome,
) -> Result<ProposedTransform, Refusal> {
    let messages = vec![
        Message::text(Role::System, INSTRUCTION),
        Message::text(Role::User, state.to_string()),
    ];
    super::call_with_schema(policy, provider, "transform", messages, schema(), out)
        .await
        .ok_or_else(|| Refusal("the seat returned no transform".to_owned()))
        .and_then(|response| match response.content.as_slice() {
            [ContentBlock::Text { text }] if response.stop_reason == StopReason::EndTurn => {
                Ok(text.clone())
            }
            _ => Err(Refusal(
                "the seat did not return one complete JSON text".to_owned(),
            )),
        })
        .and_then(|text| {
            serde_json::from_str::<ProposedTransform>(&text)
                .map_err(|e| Refusal(format!("the answer is not a transform: {e}")))
        })
}

/// The laws a proposed program must pass before it is bound: it parses and compiles under
/// the runtime's policy; every declared column is a column the request names and the example
/// records carry it; every `.name` it reads is declared (or one of jq's own `key`/`value`
/// and the input's `records`/`slots`); every quoted string it carries is a word of the
/// request or a declared column; every number of two digits or more, or with a fraction, is
/// written in the request; and it returns, on the seat's own example, exactly the output the
/// seat expects — a program the seat cannot predict is not understood.
pub(super) fn verify(
    intent: &str,
    hint: &[String],
    proposed: &ProposedTransform,
) -> Result<(), Refusal> {
    let program = proposed.jq.trim();
    if program.is_empty() || program.len() > 4096 {
        return Err(Refusal(
            "the program is empty or longer than 4096 bytes".to_owned(),
        ));
    }
    let lower = intent.to_lowercase();
    let names_column = |name: &str| {
        let name = name.trim();
        !name.is_empty()
            && name.len() <= 64
            && if hint.is_empty() {
                lower.contains(&name.to_lowercase())
            } else {
                hint.iter().any(|c| c.eq_ignore_ascii_case(name))
            }
    };
    let columns: Vec<String> = proposed
        .columns_read
        .iter()
        .map(|c| c.trim().to_owned())
        .filter(|c| !c.is_empty())
        .collect();
    if columns.is_empty() {
        return Err(Refusal(
            "the program declares no column it reads".to_owned(),
        ));
    }
    for column in &columns {
        if !names_column(column) {
            return Err(Refusal(format!(
                "`{column}` is not a column the request names"
            )));
        }
    }
    let Some(example) = proposed.example_input.as_array() else {
        return Err(Refusal(
            "the example input is not an array of records".to_owned(),
        ));
    };
    if example.len() < 2 || example.iter().any(|r| !r.is_object()) {
        return Err(Refusal("the example needs at least two records".to_owned()));
    }
    for column in &columns {
        if example.iter().any(|r| r.get(column).is_none()) {
            return Err(Refusal(format!(
                "an example record lacks the declared column `{column}`"
            )));
        }
    }
    for name in read_names(program) {
        if matches!(name.as_str(), "records" | "slots" | "key" | "value") {
            continue;
        }
        if !columns.iter().any(|c| c == &name) {
            return Err(Refusal(format!(
                "the program reads `.{name}`, a column it did not declare"
            )));
        }
    }
    for literal in string_literals(program) {
        let structural = literal.is_empty() || literal.chars().all(|c| !c.is_alphanumeric());
        if structural || columns.iter().any(|c| c.eq_ignore_ascii_case(&literal)) {
            continue;
        }
        if !lower.contains(&literal.to_lowercase()) {
            return Err(Refusal(format!(
                "the literal `{literal}` is not in the request"
            )));
        }
    }
    let digit_runs: Vec<String> = intent
        .split(|c: char| !c.is_ascii_digit() && c != '.' && c != ',')
        .filter(|run| run.chars().any(|c| c.is_ascii_digit()))
        .map(|run| run.trim_matches(['.', ',']).replace(',', "."))
        .collect();
    for number in number_literals(program) {
        if number.len() >= 2 && !digit_runs.iter().any(|run| run == &number) {
            return Err(Refusal(format!(
                "the number `{number}` is not in the request"
            )));
        }
    }
    let output = run(program, &json!({"records": example}))?;
    if output != proposed.expected_output {
        return Err(Refusal(
            "on its own example the program does not return the output the seat expected"
                .to_owned(),
        ));
    }
    Ok(())
}

/// Every `.name` the program reads (an identifier right after a dot, outside strings).
fn read_names(program: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_string = false;
    let mut prev: Option<char> = None;
    for (at, c) in program.char_indices() {
        if c == '"' && prev != Some('\\') {
            in_string = !in_string;
        } else if !in_string && c == '.' && !prev.is_some_and(|p| p.is_ascii_digit()) {
            let rest = &program[at + 1..];
            let name: String = rest
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                .collect();
            if name
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
                && !names.contains(&name)
            {
                names.push(name);
            }
        }
        prev = Some(c);
    }
    names
}

/// Every double-quoted literal of the program, unescaped only for `\"`.
fn string_literals(program: &str) -> Vec<String> {
    let mut literals = Vec::new();
    let mut current: Option<String> = None;
    let mut prev: Option<char> = None;
    for c in program.chars() {
        match (&mut current, c) {
            (None, '"') => current = Some(String::new()),
            (Some(text), '"') if prev != Some('\\') => {
                literals.push(text.clone());
                current = None;
            }
            (Some(text), c) => text.push(c),
            _ => {}
        }
        prev = Some(c);
    }
    literals
}

/// Every number of the program outside strings, as written (`120`, `0.5`).
fn number_literals(program: &str) -> Vec<String> {
    let mut numbers = Vec::new();
    let mut in_string = false;
    let mut prev: Option<char> = None;
    let mut current = String::new();
    for c in program.chars() {
        if c == '"' && prev != Some('\\') {
            in_string = !in_string;
        }
        if !in_string && (c.is_ascii_digit() || (c == '.' && !current.is_empty())) {
            let identifier =
                current.is_empty() && prev.is_some_and(|p| p.is_ascii_alphanumeric() || p == '_');
            if !identifier {
                current.push(c);
            }
        } else if !current.is_empty() {
            numbers.push(current.trim_end_matches('.').to_owned());
            current.clear();
        }
        prev = Some(c);
    }
    if !current.is_empty() {
        numbers.push(current.trim_end_matches('.').to_owned());
    }
    numbers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_lookup_matches_numeric_identifiers_without_rewriting_string_ids() {
        let select = |rows: Value, id: &str| {
            run(
                crate::laws::SELECT_BY_FIELD,
                &json!({"directory": rows.to_string(), "field": "id", "id": id}),
            )
            .unwrap()
        };
        let numeric = json!({"id": 42, "subject": "the requested record"});
        assert_eq!(
            select(json!([{"id": 7}, numeric.clone(), {"id": 420}]), "42"),
            numeric
        );
        let padded = json!({"id": "042", "subject": "a distinct identifier"});
        assert_eq!(select(json!([numeric, padded.clone()]), "042"), padded);
        // The string `"42"` and the number 42 both match « 42 »: two different records, an
        // ambiguity the law states (R4 A7) — it no longer keeps the first and loses the other.
        assert!(
            run(
                crate::laws::SELECT_BY_FIELD,
                &json!({"directory": json!([{"id": "42", "value": "first"}, {"id": 42}]).to_string(), "field": "id", "id": "42"}),
            )
            .is_err()
        );
        assert_eq!(
            select(json!([{"id": true}, {"id": null}, {}]), "true"),
            Value::Null
        );
        assert_eq!(
            select(json!([{"id": "T-42"}]), "T-42"),
            json!({"id": "T-42"})
        );
        assert_eq!(
            select(json!({"42": {"value": "keyed"}}), "42"),
            json!({"value": "keyed"})
        );
    }

    /// One requested record by identifier (R4 A7): exactly one distinct record under the
    /// identity relation is the record, whatever the input order; JSON-equal copies are that one
    /// record; two different records refuse, naming the count, the field and the identifier —
    /// input order is never a reason to pick one; no match is `null` (the admit refuses it).
    #[test]
    fn a_literal_lookup_is_one_distinct_record_or_a_refusal() {
        let select = |rows: Value| {
            run(
                crate::laws::SELECT_BY_FIELD,
                &json!({"directory": rows.to_string(), "field": "id", "id": "42"}),
            )
        };
        let a = json!({"id": "42", "title": "A", "status": "open"});
        let b = json!({"id": "42", "title": "B", "status": "closed"});
        let decoy = json!({"id": "142", "title": "decoy"});
        for rows in [
            json!([a.clone(), decoy.clone()]),
            json!([decoy.clone(), a.clone()]),
            json!([null, "42", 42, ["42"], {"id": null}, {"ID": "42"}, {"id": {"v": "42"}}, a.clone()]),
            json!([a.clone(), decoy.clone(), a.clone()]),
            json!([a.clone(), {"status": "open", "title": "A", "id": "42"}]),
        ] {
            assert_eq!(select(rows.clone()).unwrap(), a, "{rows}");
        }
        for rows in [
            json!([a.clone(), b.clone()]),
            json!([b.clone(), a.clone()]),
            json!([b.clone(), decoy.clone(), a.clone()]),
            json!([{"id": "42", "title": "A"}, {"id": 42, "title": "A"}]),
        ] {
            // The error value renders as a JSON string: its inner quotes arrive escaped.
            let why = select(rows.clone()).unwrap_err().0;
            assert!(
                why.contains("records have `id`")
                    && why.contains("42")
                    && why.contains("no single record"),
                "{rows}: {why}"
            );
        }
        assert!(
            select(json!([a.clone(), b, a.clone()]))
                .unwrap_err()
                .0
                .contains("3 records")
        );
        assert_eq!(select(json!([decoy])).unwrap(), Value::Null);
        assert_eq!(select(json!([])).unwrap(), Value::Null);
        // An object directory is looked up by key, unchanged.
        assert_eq!(
            select(json!({"42": {"title": "A"}, "7": {"title": "other"}})).unwrap(),
            json!({"title": "A"})
        );
    }

    #[test]
    fn the_program_scanners_read_names_strings_and_numbers_outside_strings() {
        let program = r#"[.records[] | select(.status == "paid" and (.amount | tonumber) > 120.5)] | .[0:10]"#;
        assert_eq!(read_names(program), ["records", "status", "amount"]);
        assert_eq!(string_literals(program), ["paid"]);
        assert_eq!(number_literals(program), ["120.5", "0", "10"]);
        // A quoted dot is not a read; an escaped quote stays inside its string.
        assert_eq!(read_names(r#"split(".") | .[0]"#), Vec::<String>::new());
        assert_eq!(string_literals(r#"select(.a == "x\"y")"#), [r#"x\"y"#]);
    }

    #[test]
    fn the_verifier_runs_the_runtime_jq_and_refuses_what_it_withholds() {
        let records = json!({"records": [{"a": 1}, {"a": 2}]});
        assert_eq!(run("[.records[] | .a] | add", &records).unwrap(), json!(3));
        assert!(
            run("[.records[] | .a", &records)
                .unwrap_err()
                .0
                .contains("does not parse")
        );
        assert!(run("env", &records).unwrap_err().0.contains("withheld"));
        assert!(
            run(".records[]", &records)
                .unwrap_err()
                .0
                .contains("more than one value")
        );
        assert!(run("empty", &records).unwrap_err().0.contains("no value"));
    }

    /// The expression the compile EMITS at `task` for `intent` over the observed JSON `files`
    /// (R4 A8): the READY candidate is parsed and its task of exactly that id is read (ids are
    /// unique map keys); the test fails when the candidate is not READY, when the task is absent
    /// or holds no expression, and when it does not carry the decimal laws it is meant to test.
    /// The laws are exercised as generated artifacts, not as a copy of their source.
    fn emitted(request: &nika_compile::CompileRequest, task: &str) -> String {
        let out = nika_compile::compile(request).unwrap();
        assert_eq!(out.status, nika_compile::CompileStatus::Ready, "{out:#?}");
        let doc: Value = serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap();
        let expression = doc["tasks"][task]["invoke"]["args"]["expression"]
            .as_str()
            .unwrap_or_else(|| panic!("no task `{task}` with an expression: {doc:#}"))
            .to_owned();
        assert!(
            expression.starts_with("# Exact decimal order laws (R4 A8)"),
            "`{task}` carries no decimal law: {expression}"
        );
        expression
    }

    /// A compile request over JSON files the host observed, with their record keys.
    fn over(intent: &str, files: &[(&str, &[&str])]) -> nika_compile::CompileRequest {
        let observed: Vec<Value> = files
            .iter()
            .map(|(path, keys)| json!({"path": path, "state": "observed", "complete": false, "kind": "json", "columns": keys, "common_columns": keys}))
            .collect();
        nika_compile::CompileRequest::create(intent).with_knowledge(json!({"observed": observed}))
    }

    #[test]
    fn a_decoded_json_number_crosses_to_the_next_task_exactly_or_the_run_stops() {
        let parse = emitted(
            &over(
                "Read ./rows.json and write it to ./copy.csv",
                &[("./rows.json", &["name", "id", "weight"])],
            ),
            "parse_source",
        );
        // Ordinary numbers pass: an integer serde keeps (within [-2^63, 2^64-1]), a decimal
        // whose shortest f64 text states the same value, zero and negative zero.
        let kept = r#"[{"name": "a", "id": 42, "weight": 0.1, "big": 18446744073709551615, "low": -9223372036854775808, "e": 1e2, "f": 2.5e-3, "z": -0, "t": 0.30000000000000004}]"#;
        assert!(run(&parse, &json!(kept)).is_ok(), "{kept}");
        // A number the transport would change stops the run, named with what it would become.
        for (text, at, carried) in [
            (
                r#"[{"id": 123456789012345678901234567890}]"#,
                "0.id is 123456789012345678901234567890",
                "1.2345678901234568e29",
            ),
            (
                r#"[{"name": "a", "weight": 1.000000000000000001}]"#,
                "0.weight is 1.000000000000000001",
                "1.0",
            ),
            (
                r#"[{"n": 18446744073709551616}]"#,
                "0.n is 18446744073709551616",
                "1.8446744073709552e19",
            ),
            (
                r#"[{"n": -9223372036854775809}]"#,
                "0.n is -9223372036854775809",
                "-9.223372036854776e18",
            ),
            (
                r#"[{"deep": {"x": [1, 2.000000000000000001]}}]"#,
                "0.deep.x.1 is 2.000000000000000001",
                "2.0",
            ),
        ] {
            let why = run(&parse, &json!(text)).unwrap_err().0;
            assert!(
                why.contains(&format!("the number at {at}"))
                    && why.contains(&format!("carry it as {carried}"))
                    && why.contains("nothing is written"),
                "{text}: {why}"
            );
        }
    }

    #[test]
    fn a_scoped_guard_decides_and_names_only_the_fields_the_rule_reads() {
        // « keep only the columns name and points » writes two fields: the guard's scope.
        let parse = emitted(
            &over(
                "Read ./rows.json, keep only the columns name and points and write them to ./names.json",
                &[("./rows.json", &["name", "points", "id", "weight"])],
            ),
            "parse_source",
        );
        assert!(
            parse.ends_with(r#"fromjson | dguard(["name","points"])"#),
            "{parse}"
        );
        // A precise payload the projection drops never stops the run, nor is it named.
        for kept in [
            r#"[{"name": "a", "points": "1.000000000000000001", "id": 123456789012345678901234567890, "weight": 1.000000000000000001}]"#,
            r#"[{"weight": 2.000000000000000002, "name": "b", "points": 2.5}]"#,
        ] {
            assert!(run(&parse, &json!(kept)).is_ok(), "{kept}");
        }
        // Where a dropped payload comes FIRST, the refusal still names the read field that lost
        // its value, never the dropped one (root's correction 1).
        for (text, at) in [
            (
                r#"[{"weight": 1.000000000000000001, "points": 2.000000000000000002}]"#,
                "0.points is 2.000000000000000002",
            ),
            (
                r#"[{"id": 123456789012345678901234567890, "name": "a", "points": "1"}, {"id": 1, "name": "b", "points": 1.000000000000000001}]"#,
                "1.points is 1.000000000000000001",
            ),
        ] {
            let why = run(&parse, &json!(text)).unwrap_err().0;
            assert!(
                why.contains(&format!("the number at {at}"))
                    && !why.contains("weight")
                    && !why.contains(".id "),
                "{text}: {why}"
            );
        }
    }

    /// The names a computed row list holds, in order.
    fn names(rows: &Value) -> Vec<String> {
        rows.as_array()
            .unwrap()
            .iter()
            .map(|r| r["name"].as_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn a_rank_orders_stated_numbers_exactly_whatever_the_input_order() {
        let compute = emitted(
            &over(
                "Read ./rows.json, keep the top 2 rows by points and write them to ./top.json",
                &[("./rows.json", &["name", "points"])],
            ),
            "compute",
        );
        let top = |rows: Value| run(&compute, &json!({"records": rows}));
        // Root's counterexample: every order of the three fine decimals keeps …003 then …002.
        let (a, b, c) = (
            json!({"name": "a", "points": "1.000000000000000001"}),
            json!({"name": "b", "points": "1.000000000000000003"}),
            json!({"name": "c", "points": "1.000000000000000002"}),
        );
        for order in [
            [&a, &b, &c],
            [&a, &c, &b],
            [&b, &a, &c],
            [&b, &c, &a],
            [&c, &a, &b],
            [&c, &b, &a],
        ] {
            let rows = json!(order);
            assert_eq!(names(&top(rows.clone()).unwrap()), ["b", "c"], "{rows}");
        }
        // Signs, zero, negative zero and exponents far beyond f64 order exactly, cheaply.
        let rows = json!([
            {"name": "z", "points": "-0"},
            {"name": "u", "points": "1e-1000000000"},
            {"name": "n", "points": "-1e-1000000000"},
            {"name": "m", "points": "-1.5"}
        ]);
        assert_eq!(names(&top(rows).unwrap()), ["u", "z"]);
        // Equal values the cut keeps together stay; JSON-equal copies satisfy the count.
        let rows = json!([
            {"name": "a", "points": "1.5"},
            {"name": "a", "points": "1.5"},
            {"name": "c", "points": "2"},
            {"name": "d", "points": "1"}
        ]);
        assert_eq!(names(&top(rows).unwrap()), ["c", "a"]);
        // Distinct records that tie across the cut have no answer in the request: it stops.
        for rows in [
            json!([{"name": "a", "points": "1.5"}, {"name": "b", "points": "1.50"}, {"name": "c", "points": "2"}]),
            json!([{"name": "b", "points": "15e-1"}, {"name": "c", "points": "2"}, {"name": "a", "points": "1.5"}]),
        ] {
            let why = top(rows.clone()).unwrap_err().0;
            assert!(
                why.contains("tie between 2 different records")
                    && why.contains("would choose among them by input order"),
                "{rows}: {why}"
            );
        }
    }

    #[test]
    fn a_comparison_reads_the_literal_the_request_states() {
        let compute = emitted(
            &over(
                "Read ./rows.json, keep only the rows whose points is above 1.000000000000000002 and write them to ./above.json",
                &[("./rows.json", &["name", "points"])],
            ),
            "compute",
        );
        assert!(
            compute.contains(r#"| dkey) > ("1.000000000000000002" | dkey)"#),
            "{compute}"
        );
        let rows = json!([
            {"name": "a", "points": "1.000000000000000001"},
            {"name": "b", "points": "1.000000000000000003"},
            {"name": "c", "points": "1.000000000000000002"},
            {"name": "d", "points": " 1.000000000000000003\t"}
        ]);
        assert_eq!(
            names(&run(&compute, &json!({"records": rows})).unwrap()),
            ["b", "d"]
        );
    }

    /// A request over one JSON source the host observed with its value kinds, as the CLI's
    /// observation reports them (`observation::records`): the kinds decide the number policy.
    fn observed_rows(intent: &str, rows: &[Value]) -> nika_compile::CompileRequest {
        let sample = nika_compile::observation::records(rows);
        let row = json!({"path": "./rows.json", "state": "observed", "complete": false, "kind": "json",
            "columns": sample.columns, "common_columns": sample.common});
        nika_compile::CompileRequest::create(intent)
            .with_knowledge(json!({"observed": [row], "kinds": {"./rows.json": sample.kinds}}))
    }

    #[test]
    fn under_skip_a_non_number_is_left_out_and_the_numbers_keep_their_exact_order() {
        let rows = [
            json!({"name": "a", "points": "1.000000000000000003"}),
            json!({"name": "n", "points": "n-a"}),
            json!({"name": "b", "points": 1}),
            json!({"name": "c", "points": "1.000000000000000001"}),
        ];
        let skip =
            |intent: &str| observed_rows(intent, &rows).answer("const.rule_number_1", r#""skip""#);
        let above = emitted(
            &skip(
                "Read ./rows.json, keep only the rows whose points is above 1.000000000000000002 and write them to ./above.json",
            ),
            "compute",
        );
        assert_eq!(
            names(&run(&above, &json!({"records": rows})).unwrap()),
            ["a"]
        );
        let top = emitted(
            &skip("Read ./rows.json, keep the top 2 rows by points and write them to ./top.json"),
            "compute",
        );
        assert_eq!(
            names(&run(&top, &json!({"records": rows})).unwrap()),
            ["a", "c"]
        );
        // Under FAIL the same non-number stops the run by name, as it did before (R4 A5).
        let fail = emitted(
            &observed_rows(
                "Read ./rows.json, keep the top 2 rows by points and write them to ./top.json",
                &rows,
            )
            .answer("const.rule_number_1", r#""fail""#),
            "compute",
        );
        let why = run(&fail, &json!({"records": rows})).unwrap_err().0;
        assert!(
            why.contains("`points` is") && why.contains("not a number"),
            "{why}"
        );
    }

    #[test]
    fn a_plain_sort_over_observed_numbers_orders_them_exactly() {
        let rows = [
            json!({"name": "a", "points": "1.000000000000000003"}),
            json!({"name": "b", "points": "1.000000000000000001"}),
            json!({"name": "c", "points": "1.000000000000000002"}),
        ];
        let sort = emitted(
            &observed_rows(
                "Read ./rows.json, sort the rows by points and write them to ./sorted.json",
                &rows,
            ),
            "compute",
        );
        assert_eq!(
            names(&run(&sort, &json!({"records": rows})).unwrap()),
            ["b", "c", "a"]
        );
    }

    #[test]
    fn the_record_a_lookup_selects_crosses_exactly_or_the_run_stops() {
        let request = nika_compile::CompileRequest::create(
            "Route support tickets, look up the customer, draft a reply, and ask me before any refund",
        )
        .answer("model", r#""mock/echo""#)
        .answer("const.customer_directory", r#""customers.json""#)
        .answer("const.refund_endpoint", r#""https://refund.example.invalid/refunds""#)
        .answer(
            "const.refund_policy",
            r#"{"cap":100,"currency":"EUR","criteria":"unused purchase within 14 days"}"#,
        );
        let pick = emitted(&request, "lookup_customer");
        let look = |directory: &str| run(&pick, &json!({"directory": directory, "id": "c1"}));
        assert_eq!(
            look(r#"{"c1": {"name": "Ada", "balance": 12.5}}"#).unwrap(),
            json!({"name": "Ada", "balance": 12.5})
        );
        assert_eq!(look(r#"{"c2": {"name": "Bo"}}"#).unwrap(), Value::Null);
        let why = look(r#"{"c1": {"name": "Ada", "balance": 1.000000000000000001}}"#)
            .unwrap_err()
            .0;
        assert!(
            why.contains("the number at balance is 1.000000000000000001")
                && why.contains("carry it as 1.0"),
            "{why}"
        );
    }

    /// The public DEV fixtures of the fused count (R4 F1), each row `(id, amount_usd, status)`
    /// as the CSV parse writes it (every cell text); the counts below were stated before any run.
    const FRIENDLY: &[(&str, &str, &str)] = &[("A", "20", "paid"), ("B", "10", "paid")];
    const DISCRIMINATING: &[(&str, &str, &str)] = &[
        ("A", "9", "paid"),
        ("B", "50", "open"),
        ("C", "100", "paid"),
        ("D", "20", "paid"),
        ("E", "10", "paid"),
        ("C", "3", "paid"),
        ("F", "1", "open"),
    ];
    const NONE_PAID: &[(&str, &str, &str)] = &[("G", "10", "open"), ("H", "3", "open")];

    fn csv_records(rows: &[(&str, &str, &str)]) -> Value {
        let rows: Vec<Value> = rows
            .iter()
            .map(|(id, amount, status)| json!({"id": id, "amount_usd": amount, "status": status}))
            .collect();
        json!({ "records": rows })
    }

    /// The expression the compile EMITS at `compute` for `intent` over the observed CSV
    /// `./data/input.csv` (its header only, no sampled value): the READY candidate is parsed and
    /// its `compute` task read, whatever laws run in front of the rule.
    fn emitted_compute(intent: &str) -> String {
        let observed = json!({"observed": [{"path": "./data/input.csv", "state": "observed", "complete": false, "kind": "csv", "columns": ["id", "amount_usd", "status"]}]});
        let request = nika_compile::CompileRequest::create(intent).with_knowledge(observed);
        let out = nika_compile::compile(&request).unwrap();
        assert_eq!(out.status, nika_compile::CompileStatus::Ready, "{out:#?}");
        let doc: Value = serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap();
        doc["tasks"]["compute"]["invoke"]["args"]["expression"]
            .as_str()
            .unwrap_or_else(|| panic!("no compute expression: {doc:#}"))
            .to_owned()
    }

    /// « count the rows where … » compiles to a program that counts the rows the clause keeps
    /// (R4 F1), run by the runtime's own jq on the emitted bytes: one object `{"count": n}`,
    /// exact for zero matches, two predicates, a threshold, later rows and exact decimals.
    #[test]
    fn a_fused_count_emits_a_program_that_counts_the_kept_rows() {
        let intent = |clause: &str| {
            format!("read ./data/input.csv, {clause}, write the count to ./out/result.json")
        };
        for (clause, counts) in [
            ("count the rows where status is paid", [2, 5, 0]),
            ("count the rows where amount_usd is over 10", [1, 3, 0]),
            (
                "count the rows where status is paid and amount_usd is over 10",
                [1, 2, 0],
            ),
        ] {
            let program = emitted_compute(&intent(clause));
            for (rows, n) in [FRIENDLY, DISCRIMINATING, NONE_PAID].iter().zip(counts) {
                assert_eq!(
                    run(&program, &csv_records(rows)).unwrap(),
                    json!({ "count": n }),
                    "{clause}"
                );
            }
        }
        // Rows no compile observed: 250 of them, the paid ones spread and last, counted by an
        // oracle that never reads the program.
        let paid = |i: usize| i % 7 == 3 || i >= 240;
        let many: Vec<(String, String, &str)> = (0..250)
            .map(|i| {
                let status = if paid(i) { "paid" } else { "open" };
                (format!("R{i}"), i.to_string(), status)
            })
            .collect();
        let rows: Vec<(&str, &str, &str)> = many
            .iter()
            .map(|(id, amount, status)| (id.as_str(), amount.as_str(), *status))
            .collect();
        let program = emitted_compute(&intent("count the rows where status is paid"));
        assert_eq!(
            run(&program, &csv_records(&rows)).unwrap(),
            json!({ "count": (0..250).filter(|i| paid(*i)).count() })
        );
        // A threshold stated finer than an f64: counted on the exact values, never collapsed.
        let fine = [
            ("A", "1.000000000000000001", "paid"),
            ("B", "1.000000000000000002", "paid"),
            ("C", "1.000000000000000003", "paid"),
        ];
        let program = emitted_compute(&intent(
            "count the rows where amount_usd is over 1.000000000000000001",
        ));
        assert_eq!(
            run(&program, &csv_records(&fine)).unwrap(),
            json!({ "count": 2 })
        );
        // The adverse witness: the pre-fix program (the filter alone, faithfully compiled from
        // an IR without the count) writes rows, never the count a strict reading asks.
        let filter_only = r#"[.records[] | select(.status == "paid")]"#;
        let rows = run(filter_only, &csv_records(DISCRIMINATING)).unwrap();
        assert!(rows.is_array() && rows != json!({ "count": 5 }), "{rows}");
    }

    /// A filter and a top-N run in the order the request states them (R4 F5), by the runtime's
    /// own jq on the emitted bytes. The discriminating rows separate the orders: the top two by
    /// amount are 100 (paid) and 50 (open), so the paid rows among them are 100 alone, while the
    /// top two paid rows are 100 and 20. Expected rows were stated before any run.
    #[test]
    fn a_filter_and_a_top_n_emit_programs_in_the_stated_order() {
        let row = |(id, amount, status): (&str, &str, &str)| json!({"id": id, "amount_usd": amount, "status": status});
        let rows =
            |picked: &[(&str, &str, &str)]| Value::Array(picked.iter().copied().map(row).collect());
        let intent = |clauses: &str, write: &str| {
            format!("read ./data/input.csv, {clauses}, {write} to ./out/result.json")
        };
        let (c100, d20, a20, b10) = (
            ("C", "100", "paid"),
            ("D", "20", "paid"),
            ("A", "20", "paid"),
            ("B", "10", "paid"),
        );
        let cases: [(&str, &str, [Value; 3]); 5] = [
            (
                "keep the rows where status is paid, keep the 2 rows with the highest amount_usd",
                "write them",
                [rows(&[a20, b10]), rows(&[c100, d20]), rows(&[])],
            ),
            (
                "keep the 2 rows with the highest amount_usd, keep the rows where status is paid",
                "write them",
                [rows(&[a20, b10]), rows(&[c100]), rows(&[])],
            ),
            (
                "keep the 2 rows with the highest amount_usd, then keep the rows where status is paid",
                "write them",
                [rows(&[a20, b10]), rows(&[c100]), rows(&[])],
            ),
            (
                "keep the rows where status is paid, keep the 2 rows with the highest amount_usd, keep the rows where amount_usd is over 50",
                "write them",
                [rows(&[]), rows(&[c100]), rows(&[])],
            ),
            (
                "keep the 2 rows with the highest amount_usd, count them",
                "write the count",
                [
                    json!({"count": 2}),
                    json!({"count": 2}),
                    json!({"count": 2}),
                ],
            ),
        ];
        for (clauses, write, expected) in cases {
            let program = emitted_compute(&intent(clauses, write));
            for (fixture, want) in [FRIENDLY, DISCRIMINATING, NONE_PAID].iter().zip(expected) {
                assert_eq!(
                    run(&program, &csv_records(fixture)).unwrap(),
                    want,
                    "{clauses}"
                );
            }
        }
    }
}
