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
use jaq_core::{Compiler, Ctx, Vars, data as jaq_data, unwrap_valr};
use jaq_json::{Val, read};
use serde_json::Value;

use crate::errors::RuntimeError;

/// The spec-plane binding cardinality code (zero / multiple outputs).
const VAR_CARDINALITY: &str = "NIKA-VAR-002";
/// The spec-plane binding runtime-error code (jq program failed).
const VAR_RUNTIME: &str = "NIKA-VAR-004";

/// The rendered-output ceiling for one binding value (16 MiB · the same
/// bound the `nika:jq` builtin applies · a model-controlled `output:` jq
/// must not balloon the settle pass). jaq's INTERNAL evaluation cost is
/// the engine's task-supervision concern (same delegation as the builtin).
const MAX_BINDING_OUTPUT_BYTES: usize = 16 * 1024 * 1024;

/// Evaluate one `output:` binding — run `program` over `input` (the
/// task's raw output) and return the binding's SINGLE value.
///
/// `name` is the binding name (for the error message only).
///
/// # Errors
///
/// [`RuntimeError::OutputBinding`] · `NIKA-VAR-004` when the program does
/// not compile or errors at runtime · `NIKA-VAR-002` when it emits zero or
/// more than one value (the single-value law · spec 04 §binding rules).
pub(crate) fn eval_binding(
    name: &str,
    program: &str,
    input: &Value,
) -> Result<Value, RuntimeError> {
    let bytes = serde_json::to_vec(input).map_err(|e| runtime_err(name, &e.to_string()))?;
    let val = read::parse_single(&bytes)
        .map_err(|e| runtime_err(name, &format!("input not JSON: {e:?}")))?;

    let defs = jaq_core::defs()
        .chain(jaq_std::defs())
        .chain(jaq_json::defs());
    let funs = jaq_core::funs()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs());
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
        .compile(modules)
        .map_err(|errs| runtime_err(name, &format!("jq compile error: {errs:?}")))?;

    let ctx = Ctx::<jaq_data::JustLut<Val>>::new(&filter.lut, Vars::new([]));
    let mut single: Option<Value> = None;
    for result in filter.id.run((ctx, val)).map(unwrap_valr) {
        let value = result.map_err(|e| runtime_err(name, &format!("jq runtime error: {e}")))?;
        // The cardinality law fires BEFORE serializing a second value — a
        // long stream never pays per-element render cost past the law.
        if single.is_some() {
            return Err(cardinality_err(
                name,
                "emitted MORE than one value — wrap it in `[ … ]` to collect a stream into an array",
            ));
        }
        let text = value.to_string();
        if text.len() > MAX_BINDING_OUTPUT_BYTES {
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

/// A `NIKA-VAR-004` binding runtime error (named for the failing binding).
fn runtime_err(name: &str, detail: &str) -> RuntimeError {
    RuntimeError::OutputBinding {
        code: VAR_RUNTIME,
        message: format!("output binding `{name}` · {detail}"),
    }
}

/// A `NIKA-VAR-002` binding cardinality error (named for the binding).
fn cardinality_err(name: &str, detail: &str) -> RuntimeError {
    RuntimeError::OutputBinding {
        code: VAR_CARDINALITY,
        message: format!("output binding `{name}` · {detail}"),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn extracts_a_single_path_value() {
        let input = serde_json::json!({ "count": 7, "users": [{ "email": "a@x" }] });
        assert_eq!(
            eval_binding("c", ".count", &input).expect("path"),
            serde_json::json!(7)
        );
        assert_eq!(
            eval_binding("first", ".users[0]", &input).expect("index"),
            serde_json::json!({ "email": "a@x" })
        );
    }

    #[test]
    fn collects_a_stream_into_an_array() {
        // The spec idiom: `[ … ]` collects a stream into ONE value.
        let input = serde_json::json!({ "users": [{ "email": "a" }, { "email": "b" }] });
        assert_eq!(
            eval_binding("emails", "[.users[].email]", &input).expect("collect"),
            serde_json::json!(["a", "b"])
        );
    }

    #[test]
    fn a_pipeline_reshapes() {
        let input = serde_json::json!({ "items": [{ "price": 2 }, { "price": 3 }] });
        assert_eq!(
            eval_binding("total", ".items | map(.price) | add", &input).expect("pipeline"),
            serde_json::json!(5)
        );
    }

    #[test]
    fn zero_output_is_var_002() {
        let input = serde_json::json!({ "a": 1 });
        let err = eval_binding("nothing", "empty", &input).expect_err("zero");
        assert!(matches!(
            &err,
            RuntimeError::OutputBinding { code, .. } if *code == "NIKA-VAR-002"
        ));
        assert_eq!(err.spec_code(), "NIKA-VAR-002");
    }

    #[test]
    fn multiple_outputs_is_var_002() {
        // A trailing iterator with no collecting wrapper emits a stream.
        let input = serde_json::json!({ "users": [1, 2, 3] });
        let err = eval_binding("each", ".users[]", &input).expect_err("multi");
        assert!(matches!(
            &err,
            RuntimeError::OutputBinding { code, message } if *code == "NIKA-VAR-002" && message.contains("each")
        ));
    }

    #[test]
    fn jq_runtime_error_is_var_004() {
        // Indexing a number is a jq runtime error (not a cardinality one).
        let input = serde_json::json!({ "n": 5 });
        let err = eval_binding("bad", ".n.deep", &input).expect_err("runtime");
        assert!(matches!(
            &err,
            RuntimeError::OutputBinding { code, .. } if *code == "NIKA-VAR-004"
        ));
    }

    #[test]
    fn uncompilable_program_is_var_004() {
        let input = serde_json::json!({});
        let err = eval_binding("nope", "this is not jq", &input).expect_err("compile");
        assert!(matches!(
            &err,
            RuntimeError::OutputBinding { code, .. } if *code == "NIKA-VAR-004"
        ));
    }
}
