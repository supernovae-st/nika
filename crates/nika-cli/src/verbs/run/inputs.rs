// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `--var KEY=VALUE` input seam — parse, key-validate, type-honor.
//!
//! Extracted from `run/mod.rs` (2026-07-11 · the input gauntlet's
//! type-coercion fix pushed the file past the 1500-LOC ratchet): the
//! run-time input surface is one coherent unit — the raw pairs in, the
//! validated `BTreeMap<String, Value>` out, the declared `vars:` block
//! the sole authority on both keys and types.

use std::collections::BTreeMap;

use nika_schema::raw::RawWorkflow;
use nika_schema::types::VarDecl;
use serde_json::Value;

use super::epilogue;
use crate::verbs::exit;

/// Parse the repeatable `--var KEY=VALUE` overrides and validate every
/// key against the workflow's declared `vars:` — an unknown key is
/// refused with the declared set (a typo'd override silently doing
/// nothing would be the worst outcome). A TYPED var's declared `type:`
/// DRIVES the value parse (spec 01 §vars · « the engine validate
/// inputs »): `--var count=notanumber` on an `integer` input is refused
/// up front, and a `string` var takes the raw text verbatim (`--var
/// name=5` is the string `"5"`). An UNTYPED var keeps the JSON-or-string
/// guess: `--var limit=5` the number `5`, `--var topic=news` the string.
pub(super) fn parse_var_overrides(
    pairs: &[String],
    wf: &RawWorkflow,
) -> Result<BTreeMap<String, Value>, String> {
    let mut overrides = BTreeMap::new();
    for pair in pairs {
        let (key, raw) = match pair.split_once('=') {
            Some((k, v)) if !k.trim().is_empty() => (k.trim(), v),
            _ => return Err(format!("--var expects KEY=VALUE, got `{pair}`")),
        };
        let Some((_, decl)) = wf.vars.iter().find(|(k, _)| k.value == key) else {
            let declared: Vec<&str> = wf.vars.iter().map(|(k, _)| k.value.as_str()).collect();
            return Err(if declared.is_empty() {
                format!("--var {key}: this workflow declares no `vars:`")
            } else {
                format!(
                    "--var {key}: unknown var — the workflow declares: {}",
                    declared.join(" · ")
                )
            });
        };
        let value = match decl {
            // The declared type drives the parse (spec-mandated input
            // validation) — a mismatch is refused with the type + value.
            VarDecl::Typed { r#type, .. } => r#type
                .coerce_cli(raw)
                .map_err(|why| format!("--var {key}: {why}"))?,
            // Untyped var (or a future non-exhaustive variant): the
            // JSON-or-string guess — no declared type to honor, the
            // historical behavior + the safe default for an unknown form.
            _ => {
                serde_json::from_str::<Value>(raw).unwrap_or_else(|_| Value::String(raw.to_owned()))
            }
        };
        overrides.insert(key.to_owned(), value);
    }
    Ok(overrides)
}

/// [`parse_var_overrides`] mapped to the ENV-class refusal the caller
/// returns — the message rides stderr + the machine error envelope.
pub(super) fn validated_var_overrides(
    vars: &[String],
    wf: &RawWorkflow,
    output_json: bool,
) -> Result<BTreeMap<String, Value>, u8> {
    parse_var_overrides(vars, wf).map_err(|message| {
        eprintln!("nika run: {message}");
        epilogue::emit_error_envelope(&message, output_json);
        exit::ENV
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    fn parse(yaml: &str) -> RawWorkflow {
        nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses")
    }

    use super::*;

    #[test]
    fn parse_var_overrides_types_json_else_string() {
        let wf = parse(
            "nika: v1\nworkflow: t\nvars:\n  topic: { type: string, required: true }\n  limit: { type: integer, default: 3 }\n  flags: [\"a\"]\ntasks:\n  - id: t\n    exec: { command: [\"true\"] }\n",
        );

        // string verbatim · integer typed · untyped JSON-guess (array).
        let overrides = parse_var_overrides(
            &[
                "topic=quantum news".to_owned(),
                "limit=5".to_owned(),
                "flags=[\"x\",\"y\"]".to_owned(),
            ],
            &wf,
        )
        .expect("valid overrides");
        assert_eq!(overrides["topic"], json!("quantum news"));
        assert_eq!(overrides["limit"], json!(5));
        assert_eq!(overrides["flags"], json!(["x", "y"]));

        // The unknown-key refusal NAMES the declared set (actionable).
        let err = parse_var_overrides(&["ghost=1".to_owned()], &wf).expect_err("unknown key");
        assert!(err.contains("ghost"), "{err}");
        assert!(err.contains("topic"), "lists the declared vars: {err}");

        // `=` in the VALUE is preserved (split_once · key=v=w).
        let eq = parse_var_overrides(&["topic=a=b".to_owned()], &wf).expect("value may carry '='");
        assert_eq!(eq["topic"], json!("a=b"));
    }

    #[test]
    fn typed_var_overrides_honor_the_declared_type() {
        // Input gauntlet (2026-07-11): a declared `type:` is the input
        // CONTRACT — the CLI value must honor it, not be embedded
        // type-blind (`count=notanumber` used to ride through as a string).
        let wf = parse(
            "nika: v1\nworkflow: t\nvars:\n  count: { type: integer, required: true }\n  ratio: { type: number, default: 1.0 }\n  on: { type: boolean, default: false }\n  name: { type: string, required: true }\ntasks:\n  - id: t\n    exec: { command: [\"true\"] }\n",
        );

        // The type DRIVES the parse — well-typed values land as their type.
        let ok = parse_var_overrides(
            &[
                "count=42".to_owned(),
                "ratio=2.5".to_owned(),
                "on=true".to_owned(),
                "name=5".to_owned(), // a STRING var takes the raw text verbatim
            ],
            &wf,
        )
        .expect("well-typed overrides");
        assert_eq!(ok["count"], json!(42));
        assert_eq!(ok["ratio"], json!(2.5));
        assert_eq!(ok["on"], json!(true));
        assert_eq!(ok["name"], json!("5"), "string var never JSON-coerces");

        // A mismatch is refused UP FRONT, naming the type + the value.
        for (bad, want) in [
            ("count=notanumber", "an integer"),
            ("ratio=lots", "a number"),
            ("on=maybe", "a boolean"),
        ] {
            let err = parse_var_overrides(&[bad.to_owned()], &wf).expect_err("type mismatch");
            assert!(
                err.contains(want) && err.contains(bad.split('=').next_back().unwrap()),
                "{err}"
            );
        }
    }
}
