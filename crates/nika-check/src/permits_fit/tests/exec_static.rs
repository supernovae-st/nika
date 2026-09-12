// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use nika_schema::parser::{ParseMode, parse};
use nika_schema::source::FileId;

fn exec_escapes(declarations: &str, program: &str, grant: &str) -> Vec<CapabilityEscape> {
    let argv = serde_json::json!([program, "${{ inputs.arg }}"]);
    let yaml = format!(
        "nika: static-exec\n{declarations}\npermits:\n  exec: {grant}\ntasks:\n  leg:\n    exec:\n      command: {argv}\n"
    );
    let wf = parse(&yaml, FileId::new(0), ParseMode::Strict).expect("parse fixture");
    scan_escapes(&wf)
        .into_iter()
        .filter(|e| e.category == "exec")
        .collect()
}

#[test]
fn immutable_const_program_is_judged_even_with_dynamic_later_argv() {
    for declaration in [
        "const: {cmd: rm}",
        "const: {cmd: {type: string, value: rm}}",
    ] {
        for program in [
            "${{ const.cmd }}",
            "${{ const['cmd'] }}",
            "${{ const[\"cmd\"] }}",
        ] {
            let escapes = exec_escapes(declaration, program, "[echo]");
            assert_eq!(escapes.len(), 1, "{declaration} / {program}: {escapes:?}");
            assert!(escapes[0].detail.contains("program `rm`"));
            assert!(exec_escapes(declaration, program, "[rm]").is_empty());
        }
    }
}

#[test]
fn input_defaults_remain_runtime_authority_and_cannot_alias_consts() {
    for declaration in [
        "inputs: {cmd: {type: string, default: rm}}",
        "inputs: {cmd: {type: string}}",
        "const: {cmd: rm}\ninputs: {cmd: {type: string, default: echo}}",
    ] {
        for program in [
            "${{ inputs.cmd }}",
            "${{ inputs['cmd'] }}",
            "${{ inputs[\"cmd\"] }}",
        ] {
            assert!(exec_escapes(declaration, program, "[echo]").is_empty());
            assert_eq!(exec_escapes(declaration, program, "false").len(), 1);
        }
    }
    assert_eq!(
        exec_escapes(
            "const: {cmd: rm}\ninputs: {cmd: {type: string, default: echo}}",
            "${{ const.cmd }}",
            "[echo]"
        )
        .len(),
        1
    );
}

#[test]
fn composed_program_expressions_are_not_mistaken_for_bare_consts() {
    for program in [
        "prefix${{ const.cmd }}",
        "${{ const.cmd + 'x' }}",
        "${{ const.cmd.field }}",
        " ${{ const.cmd }}",
        "${{ const.cmd }} ",
        "\t${{ const.cmd }}",
    ] {
        assert!(exec_escapes("const: {cmd: rm}", program, "[echo]").is_empty());
    }
    assert_eq!(exec_escapes("", "rm", "[echo]").len(), 1);
    assert!(exec_escapes("", "echo", "[echo]").is_empty());
}
