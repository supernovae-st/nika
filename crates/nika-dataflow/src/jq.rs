// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `output:` named-binding evaluation — jq over a task's raw output.
//!
//! Spec 04 §Output binding · « Output binding uses a jq expression — the
//! SAME jq as the `nika:jq` builtin. » This module runs ONE jq program
//! over a `serde_json::Value` and returns the binding's single value (the
//! 04 §binding rules single-value law). It is the runtime-settle twin of
//! `nika-builtin`'s `nika:jq` (same `jaq` engine · same exactly-one law ·
//! same 16 MiB rendered ceiling) — the runtime owns this seam rather than
//! depending on the L1.5 builtin crate, exactly as it owns the CEL seam
//! (`nika-cel`) for `${{ }}` resolution.
//!
//! Errors map to the spec-plane binding codes (resolvable via
//! `nika_pack::error_codes()`) ·
//! - `NIKA-VAR-002` · zero or MORE than one output (cardinality).
//! - `NIKA-VAR-004` · the jq program errored at runtime (incl. a program
//!   that does not compile · authoring error · single class at v0).

use jaq_core::load::{Arena, File, Loader};
use jaq_core::{Compiler, Ctx, Vars, data as jaq_data};
use jaq_json::{Val, read};
use nika_cap::JqClock;
use serde_json::Value;

use crate::errors::DataflowError;

/// The spec-plane binding cardinality code (zero / multiple outputs).
const VAR_CARDINALITY: &str = "NIKA-VAR-002";
/// The spec-plane binding runtime-error code (jq program failed).
const VAR_RUNTIME: &str = "NIKA-VAR-004";

/// The rendered-output ceiling for one binding value (16 MiB · the same
/// bound the `nika:jq` builtin applies · a model-controlled `output:` jq
/// must not balloon the settle pass). jaq's INTERNAL evaluation cost is
/// the engine's task-supervision concern (same delegation as the builtin).
const MAX_BINDING_OUTPUT_BYTES: usize = 16 * 1024 * 1024;

fn exceeds_binding_output_limit(len: usize) -> bool {
    len > MAX_BINDING_OUTPUT_BYTES
}

/// Evaluate one `output:` binding — run `program` over `input` (the
/// task's raw output) and return the binding's SINGLE value.
///
/// `name` is the binding name (for the error message only).
/// `clock` is a pure run-start value captured by the effect-owning runtime.
///
/// # Errors
///
/// [`DataflowError::OutputBinding`] · `NIKA-VAR-004` when the program does
/// not compile or errors at runtime · `NIKA-VAR-002` when it emits zero or
/// more than one value (the single-value law · spec 04 §binding rules).
pub fn eval_binding(
    name: &str,
    program: &str,
    input: &Value,
    clock: JqClock,
) -> Result<Value, DataflowError> {
    let bytes = serde_json::to_vec(input).map_err(|e| runtime_err(name, &e.to_string()))?;
    let val = read::parse_single(&bytes)
        .map_err(|e| runtime_err(name, &format!("input not JSON: {e:?}")))?;

    let defs = jaq_core::defs()
        .chain(
            jaq_std::defs().filter(|definition| nika_cap::install_jq_definition(definition.name)),
        )
        .chain(jaq_json::defs())
        .chain(clock_defs(name)?);
    // The typed nika-cap policy removes every host-reaching native. Clock
    // spellings return through pure definitions over the caller's value.
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
        .map_err(|errs| runtime_err(name, &format!("jq program error: {errs:?}")))?;
    let filter = Compiler::default()
        .with_funs(funs)
        .with_global_vars([nika_cap::JQ_RUN_START_VAR])
        .compile(modules)
        .map_err(|errs| runtime_err(name, &render_compile(&errs)))?;

    let ctx = Ctx::<jaq_data::JustLut<Val>>::new(
        &filter.lut,
        Vars::new([Val::from(clock.unix_seconds())]),
    );
    let mut single: Option<Value> = None;
    for result in filter.id.run((ctx, val)) {
        let value = unwrap_without_exit(name, result)?;
        // The cardinality law fires BEFORE serializing a second value — a
        // long stream never pays per-element render cost past the law.
        if single.is_some() {
            return Err(cardinality_err(
                name,
                "emitted MORE than one value — wrap it in `[ … ]` to collect a stream into an array",
            ));
        }
        let text = value.to_string();
        if exceeds_binding_output_limit(text.len()) {
            return Err(runtime_err(
                name,
                &format!(
                    "output is {} bytes — the per-binding ceiling is {MAX_BINDING_OUTPUT_BYTES} (narrow the expression)",
                    text.len()
                ),
            ));
        }
        single = Some(
            serde_json::from_str(&text)
                .map_err(|e| runtime_err(name, &format!("output not JSON: {e}")))?,
        );
    }
    single.ok_or_else(|| {
        cardinality_err(
            name,
            "emitted NO value — a binding needs exactly one (use `// default` or `first(…)`)",
        )
    })
}

fn clock_defs(name: &str) -> Result<Vec<jaq_core::load::parse::Def<&'static str>>, DataflowError> {
    jaq_core::load::parse(nika_cap::JQ_CLOCK_DEFS, |parser| parser.defs()).ok_or_else(|| {
        runtime_err(
            name,
            "internal: the canonical jq clock definitions failed to parse",
        )
    })
}

/// Convert jaq's value-or-exception without calling `jaq_core::unwrap_valr`.
///
/// The upstream helper implements jq's CLI semantics by calling
/// `std::process::exit` for `halt`. A library evaluator must turn every such
/// exception into its typed error channel even if a future dependency change
/// accidentally makes a halt primitive reachable again.
fn unwrap_without_exit(name: &str, result: jaq_core::ValX<'_, Val>) -> Result<Val, DataflowError> {
    match result {
        Ok(value) => Ok(value),
        Err(exception) => match exception.get_err() {
            Ok(error) => Err(runtime_err(name, &format!("jq runtime error: {error}"))),
            Err(exception) => match exception.get_halt() {
                Ok(exit_code) => Err(runtime_err(
                    name,
                    &format!("jq process control is withheld (halt code {exit_code})"),
                )),
                Err(exception) => Err(runtime_err(
                    name,
                    &format!("jq internal control-flow exception: {exception:?}"),
                )),
            },
        },
    }
}

/// Render a jaq COMPILE error set (undefined filters/variables) as ONE clean
/// line — and, when the undefined name is one this engine WITHHELDS, say so
/// and name the class it would have read (D-2026-08-11-N26) instead of letting
/// the author read « undefined filter » about a filter jq really does define.
///
/// The same three-line shape is mirrored at the other two jq seams; the shared
/// half (the list, the sentence) lives in `nika_cap::expr`.
#[allow(clippy::type_complexity)] // the shape is jaq's `compile::Errors`, not ours
fn render_compile<U>(errs: &[(File<&str, ()>, Vec<(&str, U)>)]) -> String {
    errs.first().and_then(|(_, v)| v.first()).map_or_else(
        || "jq compile error".to_owned(),
        |(name, _)| {
            withheld_reason(name)
                .unwrap_or_else(|| format!("undefined filter or variable `{name}`"))
        },
    )
}

fn withheld_reason(name: &str) -> Option<String> {
    nika_cap::withheld_jq_policy_reason(name)
}

/// A `NIKA-VAR-004` binding runtime error (named for the failing binding).
fn runtime_err(name: &str, detail: &str) -> DataflowError {
    DataflowError::OutputBinding {
        code: VAR_RUNTIME,
        message: format!("output binding `{name}` · {detail}"),
    }
}

/// A `NIKA-VAR-002` binding cardinality error (named for the binding).
fn cardinality_err(name: &str, detail: &str) -> DataflowError {
    DataflowError::OutputBinding {
        code: VAR_CARDINALITY,
        message: format!("output binding `{name}` · {detail}"),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use nika_types::timestamp::Timestamp;

    fn eval(name: &str, program: &str, input: &Value) -> Result<Value, DataflowError> {
        eval_binding(
            name,
            program,
            input,
            JqClock::at(Timestamp::from_unix_ns(1_700_000_000_125_000_000)),
        )
    }

    #[test]
    fn binding_output_ceiling_is_exactly_sixteen_mib_and_inclusive() {
        assert_eq!(MAX_BINDING_OUTPUT_BYTES, 16_777_216);
        assert!(!exceeds_binding_output_limit(16_777_215));
        assert!(!exceeds_binding_output_limit(16_777_216));
        assert!(exceeds_binding_output_limit(16_777_217));

        let oversized = Value::String("x".repeat(MAX_BINDING_OUTPUT_BYTES));
        let err = eval("oversized", ".", &oversized)
            .expect_err("the public evaluator must enforce the rendered ceiling");
        assert!(matches!(
            &err,
            DataflowError::OutputBinding { code, message }
                if *code == "NIKA-VAR-004"
                    && message.contains("16777218 bytes")
                    && message.contains("ceiling is 16777216")
        ));
    }

    #[test]
    fn extracts_a_single_path_value() {
        let input = serde_json::json!({ "count": 7, "users": [{ "email": "a@x" }] });
        assert_eq!(
            eval("c", ".count", &input).expect("path"),
            serde_json::json!(7)
        );
        assert_eq!(
            eval("first", ".users[0]", &input).expect("index"),
            serde_json::json!({ "email": "a@x" })
        );
    }

    #[test]
    fn collects_a_stream_into_an_array() {
        // The spec idiom: `[ … ]` collects a stream into ONE value.
        let input = serde_json::json!({ "users": [{ "email": "a" }, { "email": "b" }] });
        assert_eq!(
            eval("emails", "[.users[].email]", &input).expect("collect"),
            serde_json::json!(["a", "b"])
        );
    }

    #[test]
    fn a_pipeline_reshapes() {
        let input = serde_json::json!({ "items": [{ "price": 2 }, { "price": 3 }] });
        assert_eq!(
            eval("total", ".items | map(.price) | add", &input).expect("pipeline"),
            serde_json::json!(5)
        );
    }

    #[test]
    fn zero_output_is_var_002() {
        let input = serde_json::json!({ "a": 1 });
        let err = eval("nothing", "empty", &input).expect_err("zero");
        assert!(matches!(
            &err,
            DataflowError::OutputBinding { code, .. } if *code == "NIKA-VAR-002"
        ));
        assert_eq!(err.spec_code(), "NIKA-VAR-002");
    }

    #[test]
    fn multiple_outputs_is_var_002() {
        // A trailing iterator with no collecting wrapper emits a stream.
        let input = serde_json::json!({ "users": [1, 2, 3] });
        let err = eval("each", ".users[]", &input).expect_err("multi");
        assert!(matches!(
            &err,
            DataflowError::OutputBinding { code, message } if *code == "NIKA-VAR-002" && message.contains("each")
        ));
    }

    #[test]
    fn jq_runtime_error_is_var_004() {
        // Indexing a number is a jq runtime error (not a cardinality one).
        let input = serde_json::json!({ "n": 5 });
        let err = eval("bad", ".n.deep", &input).expect_err("runtime");
        assert!(matches!(
            &err,
            DataflowError::OutputBinding { code, .. } if *code == "NIKA-VAR-004"
        ));
    }

    #[test]
    fn a_binding_cannot_read_the_ambient_environment() {
        // D-2026-08-11-N26 · an expression sees only its INPUT. `env` is not
        // in the function set this seam compiles with, so the program never
        // becomes runnable — the refusal is the compiler's, not a scan's.
        let input = serde_json::json!({});
        let err = eval("leak", "env.PATH", &input).expect_err("withheld");
        assert_eq!(err.spec_code(), "NIKA-VAR-004");
        let msg = err.to_string();
        assert!(
            msg.contains("ambient process environment"),
            "the refusal must NAME the class it withheld · got: {msg}"
        );
        // Bare `env` (the whole map) is the same refusal.
        let bare = eval("leak", "env", &input).expect_err("withheld");
        assert!(bare.to_string().contains("ambient process environment"));
    }

    #[test]
    fn clock_forms_are_rebound_and_other_host_effects_are_withheld() {
        let input = serde_json::json!({});
        assert_eq!(
            eval("clock", "now", &input).expect("rebound"),
            serde_json::json!(1_700_000_000.125)
        );
        assert_eq!(
            eval("timezone", "0 | localtime | .[0]", &input).expect("UTC"),
            serde_json::json!(1970)
        );
        assert_eq!(
            eval("timezone-format", "0 | strflocaltime(\"%Z\")", &input).expect("UTC"),
            serde_json::json!("UTC")
        );
        for (name, program) in [
            ("debug", "debug"),
            ("stderr", "stderr"),
            ("halt-zero", "halt"),
            ("halt-code", "halt(7)"),
            ("halt-error", "halt_error"),
            ("halt-error-code", "halt_error(9)"),
        ] {
            let err = eval(name, program, &input).expect_err("host effect withheld");
            assert_eq!(err.spec_code(), "NIKA-VAR-004", "{program}");
            assert!(err.to_string().contains("withheld"), "{program}: {err}");
        }
    }

    #[test]
    fn a_halt_exception_becomes_a_typed_error_instead_of_exiting() {
        let result: jaq_core::ValX<'_, Val> = Err(jaq_core::Exn::halt(7));
        let err = unwrap_without_exit("stop", result).expect_err("halt is an error");
        assert_eq!(err.spec_code(), "NIKA-VAR-004");
        assert!(err.to_string().contains("halt code 7"), "{err}");
    }

    #[test]
    fn the_pure_date_family_still_works() {
        // The subtraction is SCOPED: a function of its own argument stays.
        let input = serde_json::json!({ "t": 0 });
        assert_eq!(
            eval("y", ".t | gmtime | .[0]", &input).expect("gmtime is pure"),
            serde_json::json!(1970)
        );
        assert_eq!(
            eval("y", ".t | strftime(\"%Y\")", &input).expect("strftime is pure"),
            serde_json::json!("1970")
        );
    }

    #[test]
    fn an_undefined_name_keeps_jaqs_own_wording() {
        // A typo is NOT dressed up as a boundary refusal.
        let input = serde_json::json!({});
        let err = eval("typo", "envv", &input).expect_err("undefined");
        let msg = err.to_string();
        assert!(msg.contains("undefined filter or variable"), "{msg}");
        assert!(!msg.contains("withheld"), "{msg}");
    }

    #[test]
    fn uncompilable_program_is_var_004() {
        let input = serde_json::json!({});
        let err = eval("nope", "this is not jq", &input).expect_err("compile");
        assert!(matches!(
            &err,
            DataflowError::OutputBinding { code, .. } if *code == "NIKA-VAR-004"
        ));
    }
}
