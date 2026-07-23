// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `declassify:` task-level key parser (NEP-0004 law 5 · the only
//! door through the permit re-gate) — split out of `tasks.rs` under the
//! ADR-023 1,500-LOC ceiling.

use marked_yaml::types::MarkedMappingNode;

use crate::error::SchemaError;
use crate::raw::DeclassifyEntry;
use crate::source::Spanned;

use super::Cx;

/// `declassify:` — the task-level taint-lift declarations (NEP-0004 law
/// 5 · the ONLY door through the re-gate). Shape-only here: a sequence
/// of `{from, to, because}` mappings, all three required string scalars,
/// `to: trusted` the one raise v1 knows, `because:` non-empty (the
/// receipt's justification). The `from:` binding's OWN grammar (which
/// root may be raised) is the check's — the parser stays shape-only.
pub(super) fn parse_declassify(
    cx: &Cx<'_>,
    mapping: &MarkedMappingNode,
    task_label: &str,
) -> Result<Vec<DeclassifyEntry>, SchemaError> {
    const KEYS: &[&str] = &["from", "to", "because"];
    let Some(node) = mapping.get_node("declassify") else {
        return Ok(Vec::new());
    };
    let Some(seq) = node.as_sequence() else {
        return Err(SchemaError::Validation {
            message: "`declassify` must be a YAML sequence of `{from, to, because}` entries"
                .to_owned(),
            span: cx.span(node.span()),
        });
    };
    let mut out = Vec::with_capacity(seq.len());
    for item in seq.iter() {
        let Some(entry_map) = item.as_mapping() else {
            return Err(SchemaError::Validation {
                message: "each `declassify` entry must be a mapping `{from, to, because}`"
                    .to_owned(),
                span: cx.span(item.span()),
            });
        };
        cx.check_unknown_keys(
            entry_map,
            KEYS,
            &format!("declassify of task `{task_label}`"),
        )?;
        let scalar = |key: &str| -> Result<Spanned<String>, SchemaError> {
            let Some(value) = entry_map.get_node(key) else {
                return Err(SchemaError::Validation {
                    message: format!(
                        "a `declassify` entry of task `{task_label}` must name `{key}` (NEP-0004 law 5)"
                    ),
                    span: cx.span(item.span()),
                });
            };
            let Some(s) = value.as_scalar() else {
                return Err(SchemaError::Validation {
                    message: format!("`declassify.{key}` must be a string"),
                    span: cx.span(value.span()),
                });
            };
            Ok(Spanned::new(
                s.as_str().to_owned(),
                cx.span_or_zero(value.span()),
            ))
        };
        let from = scalar("from")?;
        let to = scalar("to")?;
        let because = scalar("because")?;
        if from.value.is_empty() {
            return Err(SchemaError::Validation {
                message: "`declassify.from` must name the binding it raises (e.g. `inputs.p`)"
                    .to_owned(),
                span: Some(from.span),
            });
        }
        if to.value != "trusted" {
            return Err(SchemaError::Validation {
                message: format!(
                    "`declassify.to` knows exactly one raise in v1: `trusted` (got `{}` · NEP-0004 law 5)",
                    to.value
                ),
                span: Some(to.span),
            });
        }
        if because.value.trim().is_empty() {
            return Err(SchemaError::Validation {
                message: "`declassify.because` must be a non-empty justification — it lands in the run receipt"
                    .to_owned(),
                span: Some(because.span),
            });
        }
        out.push(DeclassifyEntry { from, to, because });
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::super::tasks::tests::{one_task, parse_strict};
    use crate::error::SchemaError;

    #[test]
    fn declassify_parses_the_nep_0004_shape() {
        // NEP-0004 law 5 (conformance fixture core/authority/011) — the
        // one grammar addition: a task-level sequence of
        // `{from, to: trusted, because}` entries.
        let yaml = "\
tasks:
  load:
    invoke:
      tool: nika:read
      args: { path: \"${{ inputs.p }}\" }
    declassify:
      - from: inputs.p
        to: trusted
        because: \"vendor inventory path, reviewed at release time\"
";
        let task = one_task(yaml);
        assert_eq!(task.declassify.len(), 1);
        let entry = &task.declassify[0];
        assert_eq!(entry.from.value, "inputs.p");
        assert_eq!(entry.to.value, "trusted");
        assert!(entry.because.value.contains("vendor inventory path"));
        // …and a task without the key carries the empty declaration.
        let plain = one_task("tasks:\n  t:\n    exec: { command: [\"true\"] }\n");
        assert!(plain.declassify.is_empty());
    }

    #[test]
    fn declassify_shape_refusals_are_validation_errors() {
        // `to:` knows exactly one raise in v1 (NEP-0004 law 5).
        let bad_to = parse_strict(
            "tasks:\n  t:\n    exec: { command: [\"true\"] }\n    declassify:\n      - { from: inputs.p, to: public, because: x }\n",
        )
        .expect_err("to != trusted");
        assert!(
            matches!(bad_to, SchemaError::Validation { .. }),
            "{bad_to:?}"
        );
        assert!(bad_to.to_string().contains("trusted"), "{bad_to}");
        // `because:` is the receipt's justification — never empty.
        let empty_because = parse_strict(
            "tasks:\n  t:\n    exec: { command: [\"true\"] }\n    declassify:\n      - { from: inputs.p, to: trusted, because: \"\" }\n",
        )
        .expect_err("empty because");
        assert!(
            matches!(empty_because, SchemaError::Validation { .. }),
            "{empty_because:?}"
        );
        // all three keys are required.
        let missing = parse_strict(
            "tasks:\n  t:\n    exec: { command: [\"true\"] }\n    declassify:\n      - { from: inputs.p, to: trusted }\n",
        )
        .expect_err("missing because");
        assert!(
            matches!(missing, SchemaError::Validation { .. }),
            "{missing:?}"
        );
        // an unknown fourth key is the closed-set refusal.
        let extra = parse_strict(
            "tasks:\n  t:\n    exec: { command: [\"true\"] }\n    declassify:\n      - { from: inputs.p, to: trusted, because: x, scope: global }\n",
        )
        .expect_err("unknown key");
        assert!(
            matches!(extra, SchemaError::UnknownField { .. }),
            "{extra:?}"
        );
    }
}
