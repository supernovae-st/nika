// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used)]

//! A mistyped reference must never become a successful literal business value.

use nika_schema::{FileId, ParseMode, SchemaError, parse};

fn analyze(source: &str) -> Result<(), Vec<SchemaError>> {
    let workflow = parse(source, FileId::new(0), ParseMode::Strict).expect("valid YAML");
    nika_check::analyzer::analyze(&workflow).map(|_| ())
}

#[test]
fn single_brace_references_are_refused_on_each_template_surface() {
    for body in [
        "tasks:\n  t:\n    infer: { prompt: hi }\noutputs:\n  value: '${ const.seed }'\n",
        "tasks:\n  t:\n    with: { value: '${ const.seed }' }\n    infer: { prompt: '${{ with.value }}' }\n",
        "tasks:\n  t:\n    invoke: { tool: 'nika:hash', args: { content: '${ const.seed }' } }\n",
        "tasks:\n  t:\n    infer: { prompt: 'value: ${ const.seed }' }\n",
        "tasks:\n  t:\n    infer: { prompt: hi }\noutputs:\n  value: '${ tasks.t.output }'\n",
        "tasks:\n  t:\n    infer: { prompt: 'é ${const.seed' }\n",
    ] {
        let source = format!("nika: typo\nconst: {{ seed: hello }}\n{body}");
        let errors = analyze(&source).expect_err("single-brace reference must refuse");
        assert!(
            errors
                .iter()
                .any(|error| matches!(error, SchemaError::ExpressionViolation { .. })),
            "missing VAR-005 in {errors:?}: {source}"
        );
    }
}

#[test]
fn ordinary_dollars_shell_expansions_and_real_islands_stay_valid() {
    for text in [
        "price is $5",
        "${HOME}",
        "${name:-default}",
        "${items[0]}",
        "${{ const.seed }}",
        "${{ '${ const.seed }' }}",
        r"\${{ const.seed }}",
        "é $5 and ${{ const.seed }}",
    ] {
        let encoded = serde_json::to_string(text).expect("JSON string is valid YAML");
        let source = format!(
            "nika: literals\nconst: {{ seed: hello }}\ntasks:\n  t:\n    infer: {{ prompt: {encoded} }}\n"
        );
        assert!(
            analyze(&source).is_ok(),
            "legitimate literal or island refused: {source}"
        );
    }
}

#[test]
fn constrained_expression_surfaces_emit_one_specific_opener_diagnostic() {
    for field in [
        "when: '${ const.items }'",
        "for_each: { items: '${ const.items }' }",
    ] {
        let source = format!(
            "nika: typo\nconst: {{ items: [] }}\ntasks:\n  t:\n    {field}\n    infer: {{ prompt: hi }}\n"
        );
        let errors = analyze(&source).expect_err("mistyped opener");
        assert_eq!(errors.len(), 1, "one voice on {field}: {errors:?}");
        let message = errors[0].to_string();
        assert!(
            message.contains("for a reference") && message.contains("literal text"),
            "{message}"
        );
        assert!(message.contains("${{ '${ item.name }' }}"), "{message}");
    }
}
