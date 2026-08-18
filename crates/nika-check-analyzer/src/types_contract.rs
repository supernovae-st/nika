// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The type-core contract layer (spec `09-types.md` · W3) — the engine
//! twin of `conformance/type_core.py::type_core_errors` (one-vérité:
//! every judgment mirrors the reference evaluator) ·
//!
//! - `NIKA-TYPE-001` / `NIKA-TYPE-006` · each `returns:` parses against
//!   the closed v1 grammar — the finding carries the type core's own
//!   teaching detail verbatim. A `PascalCase` word in type position is
//!   `NIKA-TYPE-001` too: the name environment is EMPTY by construction
//!   now, so the refusal teaches the inline form.
//! - `NIKA-TYPE-003` · `returns:` and a verb-level `schema:` on one
//!   task — two spellings of one contract.
//! - `NIKA-TYPE-004` · a `returns:` type unreachable from the declared
//!   `decode:` (mirror of `type_core.py::_decodable`).
//! - `NIKA-PARSE-025` · `decode:` with `capture: structured` — that
//!   capture already IS an object.
//!
//! This module ALSO owns the shared projections every downstream
//! consumer reads (never re-derived): [`returns_type`] (one task's
//! parsed contract) ·
//! [`lowered_returns`] (task → `lower(returns)` · the schema the static
//! walks and the structured-output lane consume).

use std::collections::{BTreeMap, BTreeSet};

use nika_types::types::{NikaType, Primitive, fits, lower, parse_type};

use nika_schema::error::SchemaError;
use nika_schema::raw::{RawAction, RawTask, RawWorkflow};
use nika_schema::source::Spanned;
use nika_schema::types::{
    CaptureMode, DecodeMode, OutputDecl, VarDecl, default_not_conforming_teaching,
};

/// The wire number a [`nika_types::types::ParseTypeError`] rides —
/// `NIKA-TYPE-001` (grammar) or `NIKA-TYPE-006` (regex dialect).
fn wire_num(code: &str) -> u16 {
    if code == "NIKA-TYPE-006" { 6 } else { 1 }
}

/// The parsed `returns:` contract of one task — `None` when absent OR
/// when the expression does not parse (the finding is the check's).
#[must_use]
pub fn returns_type(task: &RawTask, wf: &RawWorkflow) -> Option<NikaType> {
    let _ = wf;
    let ret = task.returns.as_ref()?;
    parse_type(
        &ret.value,
        &BTreeSet::new(),
        &format!("tasks.{}.returns", task.id.value),
    )
    .ok()
}

/// `task id → lower(returns)` — the JSON-Schema projection of every
/// parsing `returns:` contract (spec 09 §lowering · one direction).
/// The static walks and the structured-output lane consume THIS —
/// never a second lowering.
#[must_use]
pub fn lowered_returns(wf: &RawWorkflow) -> BTreeMap<String, serde_json::Value> {
    let names = BTreeSet::new();
    let env = BTreeMap::new();
    let mut out = BTreeMap::new();
    for task in &wf.tasks {
        let t = &task.value;
        let Some(ret) = t.returns.as_ref() else {
            continue;
        };
        let where_ = format!("tasks.{}.returns", t.id.value);
        if let Ok(ty) = parse_type(&ret.value, &names, &where_) {
            out.insert(t.id.value.clone(), lower(&ty, &env));
        }
    }
    out
}

/// The NIKA-TYPE static layer + NIKA-PARSE-025 (spec 09 · the engine
/// twin of `type_core.py::type_core_errors`).
pub(super) fn check_types_contract(wf: &RawWorkflow, errors: &mut Vec<SchemaError>) {
    // The name environment is EMPTY, permanently. There is no `types:`
    // block to fill it and no import to widen it, so every judgment
    // below terminates by construction rather than behind a guard.
    let names = BTreeSet::new();
    for task in &wf.tasks {
        check_task_contract(&task.value, &names, errors);
    }
}

/// One task's contract rules (spec 09 §returns · §decode).
fn check_task_contract(task: &RawTask, names: &BTreeSet<String>, errors: &mut Vec<SchemaError>) {
    let id = task.id.value.as_str();
    let verb = task.action.verb();

    if let Some(ret) = task.returns.as_ref() {
        // NIKA-TYPE-001/006 · the returns expression parses.
        let rt = match parse_type(&ret.value, names, &format!("tasks.{id}.returns")) {
            Ok(ty) => Some(ty),
            Err(e) => {
                errors.push(SchemaError::TypeExprInvalid {
                    num: wire_num(e.code),
                    detail: e.detail,
                    span: Some(ret.span),
                });
                None
            }
        };

        // NIKA-TYPE-003 · returns: + <verb>.schema: — one contract.
        let schema_present = match &task.action {
            RawAction::Infer(a) => a.schema.is_some(),
            RawAction::Agent(a) => a.schema.is_some(),
            RawAction::Exec(_) | RawAction::Invoke(_) => false,
            #[allow(
                clippy::unreachable,
                reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
            )]
            other => unreachable!("unknown action: {other:?}"),
        };
        if schema_present {
            errors.push(SchemaError::TypeContractDuplicated {
                task: id.to_owned(),
                verb,
                span: Some(ret.span),
            });
        }

        // NIKA-TYPE-004 · the contract must come out of the decode.
        if let (RawAction::Exec(exec), Some(ty)) = (&task.action, rt.as_ref()) {
            let capture = exec
                .capture
                .as_ref()
                .map_or(CaptureMode::Stdout, |c| c.value);
            let decode = exec.decode.as_ref().map_or(DecodeMode::Text, |d| d.value);
            if capture != CaptureMode::Structured && !decodable(ty, decode) {
                errors.push(SchemaError::TypeUndecodable {
                    task: id.to_owned(),
                    decode: decode.to_string(),
                    span: Some(ret.span),
                });
            }
        }
    }

    // NIKA-PARSE-025 · decode: with capture: structured.
    if let RawAction::Exec(exec) = &task.action
        && let Some(decode) = exec.decode.as_ref()
        && exec
            .capture
            .as_ref()
            .is_some_and(|c| c.value == CaptureMode::Structured)
    {
        errors.push(SchemaError::DecodeWithStructuredCapture {
            task: id.to_owned(),
            span: Some(decode.span),
        });
    }
}

/// The io-declaration contract (R3b · LAW-GRAMMAR-0211 + LAW-TYPE-0211 —
/// the engine twin of `values_core.py::_default_errors`), ONE walk per
/// declaration emitting both arms: `NIKA-TYPE-001/006` for every `type:`
/// of `inputs:`/`config:`/`const:`/`outputs:` outside the FULL `TypeExpr`
/// (the flat 6-enum is dead · `bool` is the one boolean spelling), and
/// `NIKA-DEFAULT-001` for a declared `default:` / typed `const:` `value:`
/// misfit (an unparseable type skips the fit · « reported elsewhere »).
pub(super) fn check_io_declarations(wf: &RawWorkflow, errors: &mut Vec<SchemaError>) {
    let type_names = BTreeSet::new();
    let named = BTreeMap::new();
    let authorities = [("inputs", &wf.inputs), ("const", &wf.consts)];
    for (authority, block) in authorities {
        for (name, decl) in block {
            let VarDecl::Typed {
                r#type, default, ..
            } = decl
            else {
                continue;
            };
            let where_ = format!("{authority}.{}", name.value);
            let Some(ty) = io_type(&type_names, &where_, r#type, errors) else {
                continue;
            };
            let Some(value) = default else { continue };
            if !fits(value, &ty, &named) {
                // The typed constant's value rides `default` in the AST —
                // the wire place names the YAML key the author wrote.
                let slot = if authority == "const" {
                    "value"
                } else {
                    "default"
                };
                let place = format!("{where_}.{slot}");
                errors.push(SchemaError::DefaultNotConforming {
                    message: default_not_conforming_teaching(&place, value, &r#type.value),
                    where_: place,
                    span: Some(r#type.span),
                });
            }
        }
    }
    for (name, decl) in &wf.outputs {
        if let OutputDecl::Typed {
            r#type: Some(t), ..
        } = decl
        {
            // Outputs carry no default — the grammar arm only.
            io_type(&type_names, &format!("outputs.{}", name.value), t, errors);
        }
    }
}

/// The grammar arm of one io declaration (`NIKA-TYPE-001/006`) — `Some`
/// when the expression parses; the refusal is pushed, `None` returned
/// otherwise (the conformance arm then skips).
fn io_type(
    names: &BTreeSet<String>,
    where_: &str,
    type_expr: &Spanned<serde_json::Value>,
    errors: &mut Vec<SchemaError>,
) -> Option<NikaType> {
    match parse_type(&type_expr.value, names, where_) {
        Ok(ty) => Some(ty),
        Err(e) => {
            errors.push(SchemaError::TypeExprInvalid {
                num: wire_num(e.code),
                detail: e.detail,
                span: Some(type_expr.span),
            });
            None
        }
    }
}

/// Can a value of type `t` come out of `decode`? — the EXACT mirror of
/// `type_core.py::_decodable` (one-vérité: same clause order) ·
/// unions: any member · a ref: always (nominal here) · bytes: only the
/// `bytes` primitive · json/jsonl: anything · text: strings, the
/// string newtypes, enums and refined strings.
fn decodable(t: &NikaType, decode: DecodeMode) -> bool {
    match t {
        NikaType::Union(ms) => ms.iter().any(|m| decodable(m, decode)),
        NikaType::Ref(_) => true,
        other => match decode {
            DecodeMode::Bytes => matches!(other, NikaType::Prim(Primitive::Bytes)),
            DecodeMode::Json | DecodeMode::Jsonl => true,
            DecodeMode::Text => match other {
                NikaType::Prim(p) => *p == Primitive::String || p.narrows_string(),
                NikaType::Enum(_) | NikaType::RefinedStr(_) => true,
                _ => false,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn wf(yaml: &str) -> RawWorkflow {
        parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse")
    }

    fn errors_of(yaml: &str) -> Vec<SchemaError> {
        let wf = wf(yaml);
        let mut errors = Vec::new();
        check_types_contract(&wf, &mut errors);
        errors
    }

    fn codes_of(errors: &[SchemaError]) -> Vec<String> {
        errors.iter().map(|e| e.spec_code().to_string()).collect()
    }

    /// The contract is written where it is USED. There is no `types:`
    /// block to hoist it into any more, and none was ever wanted: the
    /// spec measured 14 `returns:` across 815 files, so no expression
    /// was ever written twice.
    const HAPPY: &str = "\
nika: t
tasks:
  stats:
    exec:
      command: [\"jq\", \"-c\", \".stats\", \"report.json\"]
      decode: json
    returns:
      object:
        title: string
        bullets: { array: string }
        score: { integer: { min: 0, max: 100 } }
  summarize:
    with: { article: \"${{ tasks.stats.output }}\" }
    infer:
      prompt: \"Summarize\"
    returns:
      object:
        title: string
        bullets: { array: string }
        score: { integer: { min: 0, max: 100 } }
";

    #[test]
    fn happy_path_zero_findings() {
        // types: + returns: + decode: all valid → not one finding.
        let errors = errors_of(HAPPY);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn a_pascal_name_in_type_position_is_type_001() {
        // The name environment is empty by construction, so a
        // PascalCase word resolves to nothing — and the refusal is what
        // teaches the inline form.
        let errors = errors_of(
            "nika: t
tasks:
  a:
    infer: { prompt: hi }
    returns: Summary
",
        );
        assert_eq!(codes_of(&errors), ["NIKA-TYPE-001"]);
        let msg = errors[0].to_string();
        assert!(msg.contains("tasks.a.returns"), "the place is named: {msg}");
        assert!(errors[0].span().is_some(), "span lands on the returns");
    }

    #[test]
    fn returns_plus_schema_is_type_003() {
        let errors = errors_of(
            "nika: t\ntasks:\n  a:\n    infer:\n      prompt: hi\n      schema: { type: object }\n    returns: string\n",
        );
        assert_eq!(codes_of(&errors), ["NIKA-TYPE-003"]);
        let msg = errors[0].to_string();
        assert!(
            msg.contains("infer.schema:") && msg.contains("keep returns: (the typed door)"),
            "teaches the one-obvious-way: {msg}"
        );
    }

    #[test]
    fn object_over_text_decode_is_type_004() {
        // No decode: (default text) + an object contract → unreachable.
        let errors = errors_of(
            "nika: t\ntasks:\n  a:\n    exec:\n      command: [\"cat\", \"x\"]\n    returns: { object: { n: integer } }\n",
        );
        assert_eq!(codes_of(&errors), ["NIKA-TYPE-004"]);
        let msg = errors[0].to_string();
        assert!(
            msg.contains("decode: text") && msg.contains("json or jsonl"),
            "names the default decode + the fix: {msg}"
        );
    }

    #[test]
    fn decodable_mirrors_the_reference_evaluator() {
        use nika_types::types::{Field, StrBounds};
        let s = NikaType::Prim(Primitive::String);
        let b = NikaType::Prim(Primitive::Bytes);
        let obj = NikaType::Object {
            fields: std::iter::once((
                "n".to_owned(),
                Field::new(NikaType::Prim(Primitive::Integer), false),
            ))
            .collect(),
            additional: false,
        };
        // text · strings + newtypes + enum + refined-string only
        assert!(decodable(&s, DecodeMode::Text));
        assert!(decodable(&NikaType::Prim(Primitive::Uri), DecodeMode::Text));
        assert!(decodable(
            &NikaType::Enum(vec!["a".to_owned()]),
            DecodeMode::Text
        ));
        assert!(decodable(
            &NikaType::RefinedStr(StrBounds::new(None, Some(1), None)),
            DecodeMode::Text
        ));
        assert!(!decodable(&obj, DecodeMode::Text));
        assert!(!decodable(
            &NikaType::Prim(Primitive::Integer),
            DecodeMode::Text
        ));
        // bytes · ONLY the bytes primitive
        assert!(decodable(&b, DecodeMode::Bytes));
        assert!(!decodable(&s, DecodeMode::Bytes));
        // json/jsonl · anything
        assert!(decodable(&obj, DecodeMode::Json));
        assert!(decodable(&obj, DecodeMode::Jsonl));
        // union · any member suffices
        assert!(decodable(
            &NikaType::Union(vec![obj.clone(), s.clone()]),
            DecodeMode::Text
        ));
        // ref · nominal — always decodable here
        assert!(decodable(&NikaType::Ref("X".to_owned()), DecodeMode::Bytes));
    }

    #[test]
    fn decode_with_structured_capture_is_parse_025() {
        let errors = errors_of(
            "nika: t\ntasks:\n  a:\n    exec:\n      command: [\"true\"]\n      capture: structured\n      decode: json\n",
        );
        assert_eq!(codes_of(&errors), ["NIKA-PARSE-025"]);
        let msg = errors[0].to_string();
        assert!(
            msg.contains("already IS an object") && msg.contains("type it with returns:"),
            "teaches the fix: {msg}"
        );
        assert!(errors[0].span().is_some(), "span lands on the decode");
    }

    #[test]
    fn structured_capture_with_returns_and_no_decode_is_clean() {
        // returns: types the structured object directly — legal.
        let errors = errors_of(
            "nika: t\ntasks:\n  a:\n    exec:\n      command: [\"true\"]\n      capture: structured\n    returns: { object: { stdout: string, stderr: string, exit_code: integer } }\n",
        );
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn out_of_dialect_regex_is_type_006() {
        let errors = errors_of(
            "nika: t\ntasks:\n  a:\n    infer: { prompt: hi }\n    returns: { string: { pattern: \"(?=x)\" } }\n",
        );
        assert_eq!(codes_of(&errors), ["NIKA-TYPE-006"]);
    }

    #[test]
    fn reserved_constructor_is_type_001_naming_the_wave() {
        let errors =
            errors_of("nika: t\ntasks:\n  a:\n    infer: { prompt: hi }\n    returns: money\n");
        assert_eq!(codes_of(&errors), ["NIKA-TYPE-001"]);
        assert!(errors[0].to_string().contains("reserved"), "{}", errors[0]);
    }

    #[test]
    fn lowered_returns_projects_json_schema_2020_12() {
        let wf = wf(HAPPY);
        let lowered = lowered_returns(&wf);
        let schema = lowered.get("summarize").expect("summarize lowered");
        assert_eq!(schema["type"], "object", "{schema}");
        assert_eq!(
            schema["properties"]["score"]["maximum"], 100,
            "the named reference INLINES at its use site: {schema}"
        );
        assert_eq!(
            schema["additionalProperties"],
            serde_json::json!(false),
            "closed by default"
        );
        // the exec task's contract lowers too (its walk is as sharp)
        assert!(lowered.contains_key("stats"));
    }

    #[test]
    fn returns_type_parses_the_inline_expression() {
        let wf = wf(HAPPY);
        let task = &wf.tasks[1].value;
        assert_eq!(task.id.value, "summarize");
        let ty = returns_type(task, &wf).expect("the contract parses");
        assert!(
            matches!(ty, NikaType::Object { .. }),
            "the expression is the contract, self-contained: {ty:?}"
        );
    }

    // ── R3b · the io-declaration contract (LAW-GRAMMAR-0211 +
    // LAW-TYPE-0211 · the engine twin of values_core.py::_default_errors) ──

    fn io_errors_of(yaml: &str) -> Vec<SchemaError> {
        let wf = wf(yaml);
        let mut errors = Vec::new();
        check_io_declarations(&wf, &mut errors);
        errors
    }

    const TASKS_TAIL: &str = "tasks:\n  a:\n    infer: { prompt: hi }\n";

    #[test]
    fn conforming_defaults_and_typed_consts_are_clean() {
        // The valid conformance fixture's shape (values/valid/
        // default-conforms-to-type): primitives · an enum composite · a
        // deployment-supplied input · a typed constant — every value
        // conforms, zero findings.
        let yaml = format!(
            "nika: t\n\
             inputs:\n  count: {{ type: integer, required: false, default: 5 }}\n  mode: {{ type: {{ enum: [\"fast\", \"slow\"] }}, default: \"fast\" }}\n  timeout_s: {{ type: number, required: false, default: 30 }}\n\
             const:\n  label: {{ type: string, value: \"prod\" }}\n{TASKS_TAIL}"
        );
        assert!(io_errors_of(&yaml).is_empty());
    }

    #[test]
    fn the_flat_6_enum_spellings_die_with_type_001() {
        // LAW-GRAMMAR-0211 · `boolean` (no alias — `bool` is the one
        // spelling) and the bare `array`/`object` constructor names are
        // OUT of the grammar; NIKA-PARSE-015 is never reused.
        for dead in ["boolean", "array", "object"] {
            let yaml = format!("nika: t\ninputs:\n  x: {{ type: {dead} }}\n{TASKS_TAIL}");
            let errors = io_errors_of(&yaml);
            assert_eq!(codes_of(&errors), ["NIKA-TYPE-001"], "`{dead}`");
            let msg = errors[0].to_string();
            assert!(msg.contains("inputs.x"), "the place is named: {msg}");
            assert!(errors[0].span().is_some(), "span lands on the type:");
        }
        // …and the live spellings parse clean.
        for live in [
            "bool",
            "string",
            "{ array: string }",
            "{ object: { x: string } }",
        ] {
            let yaml = format!("nika: t\ninputs:\n  x: {{ type: {live} }}\n{TASKS_TAIL}");
            assert!(io_errors_of(&yaml).is_empty(), "`{live}`");
        }
    }

    #[test]
    fn outputs_type_also_speaks_the_full_typeexpr() {
        // LAW-GRAMMAR-0211 names inputs AND outputs — the callable
        // contract never speaks two type languages at once.
        let ok = format!(
            "nika: t\noutputs:\n  report: {{ value: \"${{ tasks.a.output }}\", type: {{ enum: [\"md\", \"html\"] }} }}\n{TASKS_TAIL}"
        );
        assert!(io_errors_of(&ok).is_empty());
        let bad = format!(
            "nika: t\noutputs:\n  report: {{ value: \"${{ tasks.a.output }}\", type: boolean }}\n{TASKS_TAIL}"
        );
        assert_eq!(codes_of(&io_errors_of(&bad)), ["NIKA-TYPE-001"]);
    }

    #[test]
    fn default_mismatch_is_default_001_with_the_teaching() {
        // The P0 witness of the ruling — `{ type: integer, required: false, default:
        // "abc" }` passed check AND run before (values/invalid/
        // default-type-mismatch).
        let yaml = format!(
            "nika: t\ninputs:\n  count: {{ type: integer, required: false, default: \"abc\" }}\n{TASKS_TAIL}"
        );
        let errors = io_errors_of(&yaml);
        assert_eq!(codes_of(&errors), ["NIKA-DEFAULT-001"]);
        let msg = errors[0].to_string();
        for needle in [
            "inputs.count.default",
            "\"abc\"",
            "`integer`",
            "P0 soundness hole",
            "LAW-TYPE-0211",
        ] {
            assert!(msg.contains(needle), "missing {needle}: {msg}");
        }
        let code = errors[0].spec_code();
        assert_eq!(code.namespace, "DEFAULT");
        assert_eq!(code.category.as_str(), "validation_error");
    }

    #[test]
    fn inputs_and_const_ride_the_same_code() {
        // An input's default: and the typed constant's value: are the SAME
        // class — no second code minted for the const variant (the law).
        // The fixture rode `config:` until the 9-key envelope killed it;
        // the deployment-supplied role is an `inputs:` entry now.
        let yaml = format!(
            "nika: t\n\
             inputs:\n  timeout_s: {{ type: number, required: false, default: \"soon\" }}\n\
             const:\n  retries: {{ type: integer, value: \"many\" }}\n{TASKS_TAIL}"
        );
        let errors = io_errors_of(&yaml);
        assert_eq!(codes_of(&errors), ["NIKA-DEFAULT-001", "NIKA-DEFAULT-001"]);
        let rendered: Vec<String> = errors.iter().map(ToString::to_string).collect();
        assert!(
            rendered[0].contains("inputs.timeout_s.default"),
            "{rendered:?}"
        );
        assert!(rendered[1].contains("const.retries.value"), "{rendered:?}");
    }

    #[test]
    fn a_broken_declared_type_skips_the_conformance_arm() {
        // The oracle's « reported elsewhere »: an out-of-grammar type
        // refuses ONCE (the grammar arm) — never a doubled DEFAULT-001.
        let yaml = format!(
            "nika: t\ninputs:\n  x: {{ type: frobnicate, required: false, default: 5 }}\n{TASKS_TAIL}"
        );
        assert_eq!(codes_of(&io_errors_of(&yaml)), ["NIKA-TYPE-001"]);
    }

    #[test]
    fn inline_enum_defaults_fit_against_their_own_expression() {
        let ok = format!(
            "nika: t\ninputs:\n  mode: {{ type: {{ enum: [\"fast\", \"slow\"] }}, \
             default: \"fast\" }}\n{TASKS_TAIL}"
        );
        assert!(io_errors_of(&ok).is_empty());
        let bad = format!(
            "nika: t\ninputs:\n  mode: {{ type: {{ enum: [\"fast\", \"slow\"] }}, \
             default: \"ludicrous\" }}\n{TASKS_TAIL}"
        );
        assert_eq!(codes_of(&io_errors_of(&bad)), ["NIKA-DEFAULT-001"]);
    }
}
