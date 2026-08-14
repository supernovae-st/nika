// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `lift:` task-level key parser — the SINGLE construct through which
//! an author opens a named law (spec `10-authority.md` §the authored
//! doors). Split out of `tasks.rs` under the ADR-023 1,500-LOC ceiling,
//! which `tasks.rs` sits one line under.
//!
//! It replaces two predecessors that did the same job in two spellings —
//! a task-level `declassify:` list and a task-level `inert:` string. The
//! spec's argument for the merge is measured, not aesthetic: a door per
//! law grows the language linearly in laws, and each language feature an
//! author uses carries **+18.9% odds of workflow failure** (regressed
//! over 13,915 workflows). The law is a parameter of `lift:`, the way the
//! provider is a parameter of `infer:` — not a field of its own.
//!
//! Shape-only here, like every other parser module: `from:`'s own
//! grammar (which root may be raised) and rule 6 — a lift that lifts
//! NOTHING is an error (`NIKA-AUTH-011`) — are the check's judgment,
//! because both need to know what the task actually does.

use marked_yaml::types::MarkedMappingNode;

use crate::error::SchemaError;
use crate::raw::{LiftEntry, LiftLaw};
use crate::source::Spanned;

use super::Cx;

/// The closed `law:` enum (spec 10 rule 1 · v1 knows exactly two doors ·
/// 24 error-bearing laws exist and 2 have one).
const LAWS: &[&str] = &["taint", "data-as-code"];

/// The accepted keys of one entry — `from:` is law-specific (rule 5).
const ENTRY_KEYS: &[&str] = &["law", "from", "because"];

/// `lift:` — the authored doors. A sequence of `{law, from?, because}`
/// entries, each lifting exactly ONE named law with a mandatory reason.
///
/// # Errors
///
/// Returns [`SchemaError::Validation`] when the value is not a sequence,
/// an entry is not a mapping, `law:` is outside the closed enum,
/// `because:` is absent or blank, or `from:` is missing on `taint` /
/// present on any other law (rule 5 · the schema discriminates).
pub(super) fn parse_lift(
    cx: &Cx<'_>,
    mapping: &MarkedMappingNode,
    task_label: &str,
) -> Result<Vec<LiftEntry>, SchemaError> {
    let Some(node) = mapping.get_node("lift") else {
        return Ok(Vec::new());
    };
    let Some(seq) = node.as_sequence() else {
        return Err(SchemaError::Validation {
            message: format!(
                "`lift` on {task_label} must be a YAML sequence of \
                 `{{law, from?, because}}` entries (spec 10 §the authored doors)"
            ),
            span: cx.span(node.span()),
        });
    };
    if seq.is_empty() {
        return Err(SchemaError::Validation {
            message: format!(
                "`lift` on {task_label} is empty — a door that opens nothing is not a door"
            ),
            span: cx.span(node.span()),
        });
    }
    let mut out = Vec::with_capacity(seq.len());
    for item in seq.iter() {
        let Some(entry) = item.as_mapping() else {
            return Err(SchemaError::Validation {
                message: format!(
                    "each `lift` entry of {task_label} must be a mapping `{{law, from?, because}}`"
                ),
                span: cx.span(item.span()),
            });
        };
        cx.check_unknown_keys(
            entry,
            ENTRY_KEYS,
            &format!("a `lift` entry of {task_label}"),
        )?;
        let law = parse_law(cx, entry, item, task_label)?;
        let because = parse_because(cx, entry, item, task_label)?;
        let from = parse_from(cx, entry, &law, task_label)?;
        out.push(LiftEntry { law, from, because });
    }
    Ok(out)
}

/// `law:` — the closed enum (rule 1). An unknown law is refused by name,
/// never accepted-and-ignored: a door the engine does not know is a door
/// the author believes in and the engine will not honour.
fn parse_law(
    cx: &Cx<'_>,
    entry: &MarkedMappingNode,
    item: &marked_yaml::types::Node,
    task_label: &str,
) -> Result<Spanned<LiftLaw>, SchemaError> {
    let Some(node) = entry.get_node("law") else {
        return Err(SchemaError::Validation {
            message: format!(
                "a `lift` entry of {task_label} must name `law:` — one of {}",
                LAWS.join(" · ")
            ),
            span: cx.span(item.span()),
        });
    };
    let Some(scalar) = node.as_scalar() else {
        return Err(SchemaError::Validation {
            message: "`lift.law` must be a string".to_owned(),
            span: cx.span(node.span()),
        });
    };
    let law = match scalar.as_str() {
        "taint" => LiftLaw::Taint,
        "data-as-code" => LiftLaw::DataAsCode,
        other => {
            return Err(SchemaError::Validation {
                message: format!(
                    "`lift.law` `{other}` is not a door — the set is closed: {} \
                     (a law with no door cannot be lifted at all)",
                    LAWS.join(" · ")
                ),
                span: cx.span(node.span()),
            });
        }
    };
    Ok(Spanned::new(law, cx.span_or_zero(node.span())))
}

/// `because:` — mandatory and non-empty (rule 2). The reason is what
/// review reads; a lift with no reason is a parse error, not a warning.
fn parse_because(
    cx: &Cx<'_>,
    entry: &MarkedMappingNode,
    item: &marked_yaml::types::Node,
    task_label: &str,
) -> Result<Spanned<String>, SchemaError> {
    let Some(node) = entry.get_node("because") else {
        return Err(SchemaError::Validation {
            message: format!(
                "a `lift` entry of {task_label} must name `because:` — the reason is what \
                 review reads, and it lands in the run receipt"
            ),
            span: cx.span(item.span()),
        });
    };
    let Some(scalar) = node.as_scalar() else {
        return Err(SchemaError::Validation {
            message: "`lift.because` must be a string".to_owned(),
            span: cx.span(node.span()),
        });
    };
    if scalar.as_str().trim().is_empty() {
        return Err(SchemaError::Validation {
            message: format!(
                "`lift.because` on {task_label} is empty — the reason IS the substance \
                 of the door (spec 10 rule 2)"
            ),
            span: cx.span(node.span()),
        });
    }
    Ok(Spanned::new(
        scalar.as_str().to_owned(),
        cx.span_or_zero(node.span()),
    ))
}

/// `from:` — law-specific (rule 5): REQUIRED by `taint` (the one binding
/// it raises), FORBIDDEN elsewhere. A `from:` on the wrong law is a parse
/// error, not a silently-ignored field.
fn parse_from(
    cx: &Cx<'_>,
    entry: &MarkedMappingNode,
    law: &Spanned<LiftLaw>,
    task_label: &str,
) -> Result<Option<Spanned<String>>, SchemaError> {
    let node = entry.get_node("from");
    match (law.value, node) {
        (LiftLaw::Taint, None) => Err(SchemaError::Validation {
            message: format!(
                "`lift: law: taint` on {task_label} must name `from:` — the ONE binding \
                 it raises (e.g. `inputs.p`)"
            ),
            span: Some(law.span),
        }),
        (LiftLaw::Taint, Some(node)) => {
            let Some(scalar) = node.as_scalar() else {
                return Err(SchemaError::Validation {
                    message: "`lift.from` must be a string".to_owned(),
                    span: cx.span(node.span()),
                });
            };
            if scalar.as_str().trim().is_empty() {
                return Err(SchemaError::Validation {
                    message: format!(
                        "`lift.from` on {task_label} must name the binding it raises \
                         (e.g. `inputs.p`)"
                    ),
                    span: cx.span(node.span()),
                });
            }
            Ok(Some(Spanned::new(
                scalar.as_str().to_owned(),
                cx.span_or_zero(node.span()),
            )))
        }
        (_, Some(node)) => Err(SchemaError::Validation {
            message: format!(
                "`from:` is law-specific — `taint` requires it, `data-as-code` forbids it \
                 (spec 10 rule 5 · task {task_label})"
            ),
            span: cx.span(node.span()),
        }),
        (_, None) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::super::tasks::tests::{one_task, parse_strict};
    use crate::error::SchemaError;
    use crate::raw::LiftLaw;

    /// Conformance fixture `core/authority/011-declassify-declared-opens-the-door`.
    #[test]
    fn taint_door_parses_the_spec_shape() {
        let yaml = "\
tasks:
  load:
    invoke:
      tool: nika:read
      args: { path: \"${{ inputs.p }}\" }
    lift:
      - law: taint
        from: inputs.p
        because: \"vendor inventory path, reviewed at release time\"
";
        let task = one_task(yaml);
        assert_eq!(task.lift.len(), 1);
        assert_eq!(task.lift[0].law.value, LiftLaw::Taint);
        assert_eq!(task.lift[0].from.as_ref().expect("from").value, "inputs.p");
    }

    /// Conformance fixture `core/authority/021-inert-door-declared-passes`.
    #[test]
    fn data_as_code_door_parses_without_from() {
        let yaml = "\
tasks:
  archive:
    invoke:
      tool: nika:fetch
      args: { url: \"https://data.example.com/legacy.pkl\" }
    lift:
      - law: data-as-code
        because: \"archived for provenance · never loaded\"
";
        let task = one_task(yaml);
        assert_eq!(task.lift[0].law.value, LiftLaw::DataAsCode);
        assert!(task.lift[0].from.is_none());
    }

    /// The four shape refusals, ported one-for-one from the two
    /// predecessors so the guarantees survive the merge — a closed key
    /// set, a closed law set, a non-empty reason, and the law-specific
    /// `from:`. The whitespace-only case is the 2026-07-23 red-team
    /// regression (`inert: " "` used to pass); it is re-proven here on
    /// the surviving construct.
    #[test]
    fn the_shape_refusals_survive_the_merge() {
        let cases = [
            // an unknown law is refused BY NAME (rule 1)
            (
                "      - law: telemetry\n        because: \"x\"\n",
                "not a door",
            ),
            // the reason is mandatory (rule 2)
            ("      - law: data-as-code\n", "must name `because:`"),
            // …and non-empty
            (
                "      - law: data-as-code\n        because: \"\"\n",
                "is empty",
            ),
            // …whitespace is empty too (the red-team regression)
            (
                "      - law: data-as-code\n        because: \"   \"\n",
                "is empty",
            ),
            // taint REQUIRES from: (rule 5)
            (
                "      - law: taint\n        because: \"x\"\n",
                "must name `from:`",
            ),
            // …and every other law FORBIDS it (rule 5)
            (
                "      - law: data-as-code\n        from: inputs.p\n        because: \"x\"\n",
                "law-specific",
            ),
            // the entry key set is closed
            (
                "      - law: data-as-code\n        because: \"x\"\n        scope: all\n",
                "unknown field",
            ),
        ];
        for (entry, needle) in cases {
            let yaml =
                format!("tasks:\n  t:\n    exec: {{ command: [\"true\"] }}\n    lift:\n{entry}");
            let err = parse_strict(&yaml).expect_err("refused");
            assert!(
                err.to_string().contains(needle),
                "{entry:?} → {err} (wanted {needle:?})"
            );
        }
    }

    /// A door that opens nothing is not a door — the empty sequence is
    /// refused before any entry is judged (spec 10 `minItems: 1`).
    #[test]
    fn an_empty_lift_is_refused() {
        let yaml = "tasks:\n  t:\n    exec: { command: [\"true\"] }\n    lift: []\n";
        let err = parse_strict(yaml).expect_err("refused");
        assert!(matches!(err, SchemaError::Validation { .. }), "{err:?}");
        assert!(err.to_string().contains("opens nothing"), "{err}");
    }
}
