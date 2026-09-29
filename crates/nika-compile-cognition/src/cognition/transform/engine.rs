// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The verifier's one jq language: the runtime mirror of `nika:jq` (jaq core, the
//! capability-filtered std, jaq-json, the runtime shadows of `stdlib.jq`, the fixed run-start
//! clock and the input-bound variables), assembled once. [`run`] judges a seat's program;
//! [`run_with`] runs the same language with natives only a private probe copy may call.

use super::Refusal;
use jaq_core::load::{Arena, Error as LoadError, File, Loader};
use jaq_core::{Compiler, Ctx, Vars, data as jaq_data};
use jaq_json::{Val, read};
use serde_json::Value;

/// The engine's data: jaq's lookup-table context over JSON values.
pub(super) type Data = jaq_data::JustLut<Val>;

/// The most output bytes a verification run may produce (the runtime's own ceiling is
/// larger; an example is small by construction).
const MAX_OUTPUT_BYTES: usize = 65_536;

/// Run `program` over `input` exactly as the runtime builtin would, with the run-start
/// clock bound to a fixed instant: the one value it emits, or the reason it cannot.
pub(super) fn run(program: &str, input: &Value) -> Result<Value, Refusal> {
    run_with(program, input, [])
}

/// The same language with `extra` natives chained after the capability filter: only a
/// private probe copy calls them, and a program naming one does not compile in [`run`].
pub(super) fn run_with(
    program: &str,
    input: &Value,
    extra: impl IntoIterator<Item = jaq_core::native::Fun<Data>>,
) -> Result<Value, Refusal> {
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
        .filter(|f| nika_cap::install_jq_native(f.0))
        .chain(extra);
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

pub(super) fn to_val(value: &Value) -> Result<Val, Refusal> {
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

/// The runtime shadows, verbatim: global `scan` and exactly one value from `tonumber`.
const JQ_STD_CORRECTIONS: &str = include_str!("stdlib.jq");

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

#[cfg(test)]
mod tests {
    use super::{run, run_with};
    use serde_json::json;

    /// One language: the verifier's runner and the extensible builder with no natives answer
    /// alike, strict `tonumber`, the `scan` shadow, the fixed clock and withheld effects included.
    #[test]
    fn run_and_run_with_nothing_are_one_language() {
        let cases = [
            ("[.[] | tonumber] | add", json!(["1", "2"])),
            ("[.[] | tonumber] | add", json!(["", "2"])),
            ("[.[] | tonumber] | add", json!(["1 2", "3"])),
            ("[.[] | tonumber] | add", json!([" 12 ", "-2.5"])),
            ("[.[] | scan(\"an\")]", json!(["banana"])),
            (
                ".records | map(.qty | tonumber) | add",
                json!({"records": [{"qty": "40"}, {"qty": "2"}]}),
            ),
            ("$records | length", json!({"records": [1, 2, 3]})),
            ("$nika_run_start", json!(null)),
            ("env", json!(null)),
            ("empty", json!(null)),
            (".[]", json!([1, 2])),
        ];
        for (program, input) in cases {
            assert_eq!(
                run(program, &input),
                run_with(program, &input, []),
                "{program}"
            );
        }
    }
}
