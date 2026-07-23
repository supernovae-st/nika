// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `inert:` task-level key parser (NEP-0006 · the honest door of the
//! data-as-code sink) — split beside `declassify.rs` under the ADR-023
//! 1,500-LOC ceiling on `tasks.rs`.

use marked_yaml::types::MarkedMappingNode;

use crate::error::SchemaError;
use crate::source::Spanned;

use super::Cx;

/// `inert:` — the task-level declaration that this task's fetch reads a
/// code-bearing artifact it will never load or run (NEP-0006 law 2 · the
/// only door through the sink law). Shape-only here: ONE non-empty string
/// scalar — the string IS the justification (the because is the
/// substance, an empty door is a refusal, never a silent toggle). WHICH
/// fetches the door blesses is the check's judgment, not the parser's.
pub(super) fn parse_inert(
    cx: &Cx<'_>,
    mapping: &MarkedMappingNode,
    task_label: &str,
) -> Result<Option<Spanned<String>>, SchemaError> {
    let Some(node) = mapping.get_node("inert") else {
        return Ok(None);
    };
    let Some(scalar) = node.as_scalar() else {
        return Err(SchemaError::Validation {
            message: format!(
                "`inert` on {task_label} must be one non-empty string (the justification · \
                 NEP-0006)"
            ),
            span: cx.span(node.span()),
        });
    };
    let value = scalar.as_str().to_owned();
    if value.trim().is_empty() {
        return Err(SchemaError::Validation {
            message: format!(
                "`inert` on {task_label} is empty — the because IS the substance of the door \
                 (NEP-0006 · declare why this read never feeds an interpreter)"
            ),
            span: cx.span(scalar.span()),
        });
    }
    Ok(Some(Spanned::new(value, cx.span_or_zero(scalar.span()))))
}

#[cfg(test)]
mod tests {
    use crate::{FileId, ParseMode, parse};

    const BASE: &str = "nika: v1\nworkflow:\n  id: w\npermits:\n  net: { http: [\"data.example.com\"] }\n  tools: [\"nika:fetch\"]\n";

    #[test]
    fn inert_parses_a_non_empty_string() {
        let yaml = format!(
            "{BASE}tasks:\n  grab:\n    invoke:\n      tool: \"nika:fetch\"\n      args: {{ url: \"https://data.example.com/a.csv\" }}\n    inert: \"archived for provenance\"\n"
        );
        let wf = parse(&yaml, FileId::new(0), ParseMode::Strict).expect("parses");
        let task = &wf.tasks[0].value;
        assert_eq!(
            task.inert.as_ref().map(|s| s.value.as_str()),
            Some("archived for provenance")
        );
    }

    #[test]
    fn inert_whitespace_only_is_refused_too() {
        // O7-E · the door needs a real because, not spaces (red team
        // 2026-07-23 · `inert: " "` used to pass the non-empty check).
        let yaml = format!(
            "{BASE}tasks:\n  grab:\n    invoke:\n      tool: \"nika:fetch\"\n      args: {{ url: \"https://data.example.com/a.csv\" }}\n    inert: \"   \"\n"
        );
        let err = parse(&yaml, FileId::new(0), ParseMode::Strict).expect_err("refused");
        assert!(err.to_string().contains("inert"), "{err}");
    }

    #[test]
    fn inert_empty_or_non_scalar_is_refused() {
        for bad in ["inert: \"\"", "inert: [x]"] {
            let yaml = format!(
                "{BASE}tasks:\n  grab:\n    invoke:\n      tool: \"nika:fetch\"\n      args: {{ url: \"https://data.example.com/a.csv\" }}\n    {bad}\n"
            );
            let err = parse(&yaml, FileId::new(0), ParseMode::Strict).expect_err("refused");
            assert!(err.to_string().contains("inert"), "{bad}: {err}");
        }
    }
}
