// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The verb-field TYPE sweep — the false-green ratchet for the
//! plain-scalar coercion class (`infer.prompt: 123` audited ✔ while the
//! spec types `prompt` as a string).
//!
//! The verb key sets are CLOSED ([`INFER_KEYS`] · [`EXEC_KEYS`] ·
//! [`INVOKE_KEYS`] · [`AGENT_KEYS`]), and every key in them carries a
//! spec type (`02-verbs.md` field tables). This suite walks the key sets
//! against the checked-in field→type table below and asserts, per row:
//!
//! - the wrong-typed fixture EXISTS and REFUSES (never silently coerces);
//! - the legal twin parses clean (the control pair — a refusal that ate
//!   the legal form would be the same defect, mirrored).
//!
//! A verb field added without a row fails the sweep; a row naming a
//! retired field fails it too. The table is the coverage claim — the
//! sweep is what keeps the claim true.

use super::verbs::{AGENT_KEYS, EXEC_KEYS, INFER_KEYS, INVOKE_KEYS};
use super::{ParseMode, parse};
use crate::source::FileId;

/// The spec type of one verb field (`02-verbs.md` field tables).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldType {
    /// A string scalar — the coercion-guard class. A plain `123` / `0.5`
    /// / `true` used to restringify silently through `as_str()`; the
    /// guard refuses it and the QUOTED form stays legal.
    Str,
    /// A string sequence — every element is a string (agent `tools` /
    /// `skills`).
    StrList,
    /// A string map — every value is a string (exec `env`).
    StrMap,
    /// The exec argv — every element is a string.
    Argv,
    /// A number — the `as_f64`/`as_u32`/`as_u64` paths refuse a
    /// non-numeric scalar (no coercion seam; pinned here against
    /// regression).
    Num,
    /// A mapping (`schema` · `args` · `thinking`).
    Map,
    /// A sequence of mappings (`vision`).
    SeqMap,
    /// A closed enum string (`capture` · `decode`) — an unknown value is
    /// refused by the enum's own lookup.
    Enum,
}

/// One row of the checked-in field→type table: the field, its spec type,
/// the wrong-typed value that MUST refuse, and the legal twin that MUST
/// parse.
struct Row {
    verb: &'static str,
    field: &'static str,
    ty: FieldType,
    bad: &'static str,
    good: &'static str,
}

const fn row(
    verb: &'static str,
    field: &'static str,
    ty: FieldType,
    bad: &'static str,
    good: &'static str,
) -> Row {
    Row {
        verb,
        field,
        ty,
        bad,
        good,
    }
}

/// The field→type table — one row per key of the four closed verb key
/// sets (the sweep asserts BOTH directions: every key has exactly one
/// row, and every row names a live key).
const TABLE: &[Row] = &[
    // ── infer (spec 02 §infer) ──────────────────────────────────────
    row("infer", "prompt", FieldType::Str, "123", "\"123\""),
    row("infer", "system", FieldType::Str, "123", "\"123\""),
    row("infer", "model", FieldType::Str, "123", "\"123\""),
    row("infer", "temperature", FieldType::Num, "soon", "0.2"),
    row("infer", "max_tokens", FieldType::Num, "soon", "500"),
    row("infer", "schema", FieldType::Map, "123", "{ type: object }"),
    row(
        "infer",
        "thinking",
        FieldType::Map,
        "123",
        "{ enabled: true }",
    ),
    row(
        "infer",
        "vision",
        FieldType::SeqMap,
        "123",
        "[{ source: file, path: \"./scan.png\" }]",
    ),
    // ── exec (spec 02 §exec + 09 §decode) ───────────────────────────
    row(
        "exec",
        "command",
        FieldType::Argv,
        "[\"echo\", 123]",
        "[\"echo\", \"123\"]",
    ),
    row("exec", "shell", FieldType::Str, "123", "\"echo ok\""),
    row("exec", "cwd", FieldType::Str, "123", "\"123\""),
    row(
        "exec",
        "env",
        FieldType::StrMap,
        "{ KEY: 123 }",
        "{ KEY: \"123\" }",
    ),
    row("exec", "stdin", FieldType::Str, "123", "\"123\""),
    row("exec", "capture", FieldType::Enum, "123", "stdout"),
    row("exec", "decode", FieldType::Enum, "123", "text"),
    // ── invoke (spec 02 §invoke + 14 §the form) ─────────────────────
    row("invoke", "tool", FieldType::Str, "123", "\"nika:uuid\""),
    row(
        "invoke",
        "workflow",
        FieldType::Str,
        "123",
        "\"./child.nika.yaml\"",
    ),
    row("invoke", "args", FieldType::Map, "123", "{ x: 1 }"),
    // ── agent (spec 02 §agent) ──────────────────────────────────────
    row("agent", "prompt", FieldType::Str, "123", "\"123\""),
    row("agent", "system", FieldType::Str, "123", "\"123\""),
    row("agent", "model", FieldType::Str, "123", "\"123\""),
    row(
        "agent",
        "tools",
        FieldType::StrList,
        "[123]",
        "[\"nika:done\"]",
    ),
    row(
        "agent",
        "skills",
        FieldType::StrList,
        "[123]",
        "[\"./SKILL.md\"]",
    ),
    row("agent", "max_turns", FieldType::Num, "soon", "3"),
    row("agent", "max_tokens_total", FieldType::Num, "soon", "5000"),
    row("agent", "temperature", FieldType::Num, "soon", "0.2"),
    row("agent", "schema", FieldType::Map, "123", "{ type: object }"),
];

/// Build a one-task workflow around `<verb>: { <companions> <field>: <value> }`
/// — the companions carry the verb's REQUIRED fields (minus the one under
/// test) so a refusal is attributable to the tested line alone.
fn workflow(verb: &str, field: &str, value: &str) -> String {
    let companions = match (verb, field) {
        ("infer" | "agent", "prompt")
        | ("exec", "command" | "shell")
        | ("invoke", "tool" | "workflow") => "",
        ("infer" | "agent", _) => "prompt: \"ok\", ",
        ("exec", _) => "command: [\"echo\", \"ok\"], ",
        ("invoke", _) => "tool: \"nika:uuid\", ",
        _ => unreachable!("table verb `{verb}` outside the closed 4"),
    };
    format!("tasks:\n  t:\n    {verb}: {{ {companions}{field}: {value} }}\n")
}

fn parse_strict(yaml: &str) -> Result<crate::raw::RawWorkflow, crate::error::SchemaError> {
    parse(yaml, FileId::new(0), ParseMode::Strict)
}

#[test]
fn the_table_and_the_closed_key_sets_cover_each_other_exactly() {
    let mut failures = Vec::new();
    for (verb, keys) in [
        ("infer", INFER_KEYS),
        ("exec", EXEC_KEYS),
        ("invoke", INVOKE_KEYS),
        ("agent", AGENT_KEYS),
    ] {
        for key in keys {
            let n = TABLE
                .iter()
                .filter(|r| r.verb == verb && r.field == *key)
                .count();
            if n != 1 {
                failures.push(format!(
                    "{verb}.{key} has {n} table rows — every verb field needs exactly one"
                ));
            }
        }
        for r in TABLE.iter().filter(|r| r.verb == verb) {
            if !keys.contains(&r.field) {
                failures.push(format!(
                    "{verb}.{} has a row but is gone from the verb key set — a stale claim",
                    r.field
                ));
            }
        }
    }
    let table_verbs: std::collections::BTreeSet<&str> = TABLE.iter().map(|r| r.verb).collect();
    assert_eq!(
        table_verbs,
        ["agent", "exec", "infer", "invoke"].into_iter().collect(),
        "a row on a verb outside the closed 4"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_typed_field_refuses_its_wrong_type_and_admits_its_legal_twin() {
    let mut failures = Vec::new();
    for r in TABLE {
        let bad = workflow(r.verb, r.field, r.bad);
        if let Ok(wf) = parse_strict(&bad) {
            failures.push(format!(
                "{}.{} ({:?}) ACCEPTED the wrong-typed `{}` — the silent coercion lives: {}",
                r.verb,
                r.field,
                r.ty,
                r.bad,
                bad.trim()
            ));
            drop(wf);
        }
        let good = workflow(r.verb, r.field, r.good);
        if let Err(err) = parse_strict(&good) {
            failures.push(format!(
                "{}.{} ({:?}) refused the LEGAL twin `{}`: {err}",
                r.verb, r.field, r.ty, r.good
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// ── The guard's own contract (the control pairs that name it) ────────

#[test]
fn the_reported_repro_refuses_with_the_quoting_teaching() {
    // The false green that opened the arc: `prompt: 123` audited clean
    // because `as_str()` restringified the plain int. Now the parse
    // refuses it and teaches the quoted form.
    let err = parse_strict("tasks:\n  t:\n    infer: { prompt: 123 }\n")
        .expect_err("a plain number where the spec says string");
    let msg = err.to_string();
    assert!(msg.contains("`prompt`"), "names the field: {msg}");
    assert!(msg.contains("number"), "names the YAML type read: {msg}");
    assert!(msg.contains("\"123\""), "teaches the quoted form: {msg}");

    // The quoted form IS the string — the legal twin stays clean.
    parse_strict("tasks:\n  t:\n    infer: { prompt: \"123\" }\n")
        .expect("a quoted number is a string");
}

#[test]
fn plain_booleans_and_floats_refuse_too_but_yaml11_aliases_stay_strings() {
    // YAML 1.2 core: only true/false are booleans. A plain `true` where
    // the spec says string is the same ambiguity as `123`; `yes`/`no`
    // are NOT booleans in this dialect (marked-yaml `as_bool` agrees) and
    // stay legal strings.
    for bad in ["true", "0.5", "-3"] {
        let yaml = format!("tasks:\n  t:\n    infer: {{ prompt: {bad} }}\n");
        assert!(
            parse_strict(&yaml).is_err(),
            "plain `{bad}` must refuse where the spec says string"
        );
    }
    parse_strict("tasks:\n  t:\n    infer: { prompt: yes }\n")
        .expect("`yes` is a string in the YAML 1.2 core dialect");
    parse_strict("tasks:\n  t:\n    infer: { prompt: \"true\" }\n")
        .expect("a quoted bool is a string");
}

#[test]
fn template_islands_are_untouched() {
    // `${{ }}` templates are plain scalars that never parse as a
    // number/bool — the guard must never see them as coercible values.
    let yaml = "inputs:\n  x: { type: string }\ntasks:\n  t:\n    infer:\n      prompt: \"${{ inputs.x }}\"\n      system: ${{ inputs.x }}\n";
    parse_strict(yaml).expect("a template island is not a coercion");
}

#[test]
fn nested_typed_fields_keep_their_refusals() {
    // `thinking.enabled` (bool) and the enum fields refused wrong types
    // BEFORE the guard — the sweep pins that those predicates still hold
    // (their mutation is a different seam than `extract_scalar`).
    let err =
        parse_strict("tasks:\n  t:\n    infer: { prompt: \"ok\", thinking: { enabled: soon } }\n")
            .expect_err("`enabled: soon` is not a boolean");
    assert!(
        err.to_string().contains("boolean"),
        "the nested refusal names the type: {err}"
    );
    parse_strict("tasks:\n  t:\n    infer: { prompt: \"ok\", thinking: { enabled: true } }\n")
        .expect("the legal twin");
}
