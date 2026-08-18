// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Static JSON Schema meta-check (the deep-static tier · spec
//! `07-conformance.md` §Levels · `NIKA-PARSE-019` « generic structural
//! validation — wrong shape for a field »).
//!
//! An `infer:`/`agent:` `schema:` block is a JSON Schema (structured-output
//! validation · spec `02-verbs.md`). The runtime compiles it with
//! `jsonschema::validator_for` (`nika-verb-infer` `structured::compile_schema`)
//! — « a schema that does not compile is a task-authoring error ». This static
//! tier runs the SAME call on every literal `schema:`, so an authoring mistake
//! (`type: objet`, a non-array `required:`, an unknown `$schema` dialect) is
//! caught at check time, not first run.
//!
//! Parity is structural — the SAME `jsonschema` crate at one workspace pin, the
//! SAME `validator_for` entry the runtime uses — so a schema the checker
//! accepts the runtime also accepts (verified empirically: `validator_for` and
//! `meta::try_validate` return the same verdict on the fixture cases · valid →
//! both ok · `type: objet` → both reject · bad `$schema` URI → both reject).
//! The call is pure/sync with no I/O — the workspace `default-features = false`
//! build compiles out the http/file retrievers, so an external `$ref` is not
//! resolvable and fails fast (not sovereign · L0-legal), exactly like the net
//! `url_host` and jq `jq_compiles` splits below the layer boundary.
//!
//! COMPLEMENTARY to `check::schema_lint` (the full-`check()` advisory).
//! The split is deliberate, by concern · this gate answers « is the schema a
//! VALID (compilable) JSON Schema? » → an authoring HARD error (the runtime
//! cannot compile it · `NIKA-PARSE-019` · invalidating). The advisory answers
//! « is the schema SATISFIABLE / typo-free? » — `required` not in `properties`,
//! `enum`-vs-`type`, min>max bounds — all of which COMPILE FINE (so this gate
//! stays silent) yet burn every model retry, with a path-aware « did you mean »
//! fix. A valid-but-unsatisfiable schema must NOT fail conformance (the runtime
//! accepts it), so those stay advisory · only a non-compiling schema is hard.

use nika_schema::error::SchemaError;
use nika_schema::raw::{RawAction, RawWorkflow};

/// Meta-validate one JSON Schema with the runtime's exact compiler. `Ok(())`
/// when it compiles; `Err(reason)` with a clean one-line message otherwise.
pub(super) fn schema_compiles(schema: &serde_json::Value) -> Result<(), String> {
    jsonschema::validator_for(schema)
        .map(|_| ())
        .map_err(|e| first_line(&e.to_string()))
}

/// A jsonschema error can span several lines; keep the first for a clean code.
fn first_line(msg: &str) -> String {
    msg.lines()
        .next()
        .map_or_else(|| msg.trim().to_owned(), |l| l.trim().to_owned())
}

/// Scan every LITERAL `schema:` (infer + agent · including `on_finally`
/// cleanups) and meta-validate it against its detected JSON Schema draft.
pub(super) fn scan_schemas(wf: &RawWorkflow, errors: &mut Vec<SchemaError>) {
    for task in &wf.tasks {
        check_action(&task.value.action, errors);
    }
}

fn check_action(action: &RawAction, errors: &mut Vec<SchemaError>) {
    let schema = match action {
        RawAction::Infer(infer) => infer.schema.as_ref(),
        RawAction::Agent(agent) => agent.schema.as_ref(),
        _ => None,
    };
    let Some(schema) = schema else {
        return;
    };
    if let Err(reason) = schema_compiles(&schema.value) {
        errors.push(SchemaError::Validation {
            message: format!("`schema:` is not a valid JSON Schema — {reason}"),
            span: Some(schema.span),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;
    use serde_json::json;

    fn errors_of(yaml: &str) -> Vec<SchemaError> {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let mut errors = Vec::new();
        scan_schemas(&wf, &mut errors);
        errors
    }

    #[test]
    fn first_line_returns_the_actual_first_line() {
        // `first_line` must echo the input's first line — a mutant that
        // returns a fixed string instead is caught here.
        assert_eq!(first_line("real first\nsecond\nthird"), "real first");
        // single-line input round-trips unchanged.
        assert_eq!(first_line("only line"), "only line");
    }

    #[test]
    fn infer_task_with_bad_schema_is_flagged() {
        // `type: objet` does not compile → the runtime-parity meta-check
        // (scan_schemas → check_action → the Infer arm) must push an error.
        let errors = errors_of(
            "nika: wftest\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  t:\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: objet\n",
        );
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(
            matches!(&errors[0], SchemaError::Validation { message, .. }
                if message.contains("not a valid JSON Schema")),
            "{errors:?}"
        );
    }

    #[test]
    fn agent_task_with_bad_schema_is_flagged() {
        // Same bad schema under an `agent:` task — the Agent arm of
        // `check_action` must also reach the meta-check.
        let errors = errors_of(
            "nika: wftest\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  t:\n    agent:\n      prompt: \"go\"\n      schema:\n        type: objet\n",
        );
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(
            matches!(&errors[0], SchemaError::Validation { message, .. }
                if message.contains("not a valid JSON Schema")),
            "{errors:?}"
        );
    }

    #[test]
    fn valid_infer_schema_pushes_no_error() {
        // A compiling schema leaves `errors` empty — pins that the scan
        // reaches the action AND that a good schema passes (so the
        // bad-schema tests above prove the arm, not a blanket reject).
        let errors = errors_of(
            "nika: wftest\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  t:\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n",
        );
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn valid_schemas_compile() {
        for ok in [
            json!({"type": "object", "properties": {}}),
            json!({"type": "string", "maxLength": 5}),
            json!({"type": "array", "items": {"type": "number"}}),
            json!({"type": "object", "required": ["name"], "properties": {"name": {"type": "string"}}}),
        ] {
            assert!(schema_compiles(&ok).is_ok(), "{ok} should compile");
        }
    }

    #[test]
    fn invalid_type_is_a_clean_error() {
        // The 005-invalid-schema fixture shape: `type: objet` is not a type.
        let err = schema_compiles(&json!({"type": "objet", "properties": {}}))
            .expect_err("`objet` is not a JSON Schema type");
        assert!(!err.contains('\n'), "diagnostic is one clean line: {err}");
        assert!(!err.is_empty(), "diagnostic carries a reason");
    }

    #[test]
    fn unknown_schema_dialect_is_an_error() {
        // A bad `$schema` URI cannot be compiled (no sovereign retriever).
        let err = schema_compiles(&json!({"$schema": "invalid-uri", "type": "string"}))
            .expect_err("unknown dialect");
        assert!(!err.is_empty(), "{err}");
    }
}
