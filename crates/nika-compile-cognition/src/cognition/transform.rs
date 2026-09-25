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
        assert_eq!(
            select(json!([{"id": "42", "value": "first"}, {"id": 42}]), "42"),
            json!({"id": "42", "value": "first"})
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
}
