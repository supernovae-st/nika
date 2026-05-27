// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Task-list parsing — YAML `tasks:` sequence → `Vec<Spanned<RawTask>>`.
//!
//! Round 2e scope extends Round 2d with three optional task-level
//! fields expressed as plain strings or string lists:
//!
//! - `depends_on:` — list of task names this task waits on.
//! - `condition:` — template expression; the parser stores the raw
//!   string, the analyzer evaluates it at runtime.
//! - `for_each:` — iteration source (template expression; same
//!   deal — stored raw).
//!
//! Each task still MUST carry `name` + exactly one verb key (`infer`,
//! `exec`, `invoke`, `agent`) with its minimum required
//! field:
//!
//! | Verb    | Required field |
//! |---------|----------------|
//! | infer   | `prompt`       |
//! | exec    | `command`      |
//! | invoke  | `tool` **or** `resource` |
//! | agent   | `prompt`       |
//!
//! `fetch` is NOT a verb — it is the `nika:fetch` builtin reached via
//! `invoke:` (spec D-2026-05-22-N18 · 4 verbs absolute). A verb is a
//! distinct native execution model; calling a URL is calling a tool.
//!
//! Still deferred to later rounds: `max_retries` (u32, Round 2f),
//! verb-specific optional fields (system prompt, temperature,
//! headers, tools, …), and the complex sub-configs (`record`,
//! `decompose`, `budget`, `limits`, `completion`, `guardrails`).

use marked_yaml::types::MarkedMappingNode;

use crate::error::SchemaError;
use crate::raw::{
    RawAction, RawAgentAction, RawExecAction, RawInferAction, RawInvokeAction, RawTask,
};
use crate::source::{ByteOffset, FileId, Span, Spanned};

use super::{CharToByte, extract_scalar, yaml_span_to_span};

/// The four task verbs. Exhaustiveness of the parser's action
/// dispatcher is enforced at compile time: every [`Verb`] variant
/// has exactly one arm in [`build_action`], so adding a fifth verb
/// cannot silently break parsing.
#[derive(Clone, Copy, Debug)]
enum Verb {
    Infer,
    Exec,
    Invoke,
    Agent,
}

impl Verb {
    /// The YAML key that selects this verb (e.g. `Verb::Infer` → `"infer"`).
    const fn key(self) -> &'static str {
        match self {
            Self::Infer => "infer",
            Self::Exec => "exec",
            Self::Invoke => "invoke",
            Self::Agent => "agent",
        }
    }

    /// All verbs, in deterministic order. Used to scan a task
    /// mapping for whichever verb key is present.
    const fn all() -> &'static [Verb] {
        &[Self::Infer, Self::Exec, Self::Invoke, Self::Agent]
    }
}

/// Parse the top-level `tasks:` sequence into `Vec<Spanned<RawTask>>`.
///
/// Returns `Ok(vec![])` when the `tasks:` key is absent. Returns a
/// [`SchemaError::Validation`] if the key is present but the value is
/// not a YAML sequence, or if any element is not a mapping.
pub(super) fn parse_tasks(
    workflow: &MarkedMappingNode,
    file_id: FileId,
    char_to_byte: &CharToByte,
) -> Result<Vec<Spanned<RawTask>>, SchemaError> {
    let Some(node) = workflow.get_node("tasks") else {
        return Ok(Vec::new());
    };
    let Some(seq) = node.as_sequence() else {
        return Err(SchemaError::Validation {
            message: "`tasks` must be a YAML sequence".to_owned(),
            span: yaml_span_to_span(file_id, node.span(), char_to_byte),
        });
    };

    let mut tasks = Vec::with_capacity(seq.len());
    for item in seq.iter() {
        let Some(task_map) = item.as_mapping() else {
            return Err(SchemaError::Validation {
                message: "each entry in `tasks` must be a mapping".to_owned(),
                span: yaml_span_to_span(file_id, item.span(), char_to_byte),
            });
        };
        let task = parse_task(task_map, file_id, char_to_byte)?;
        let span = yaml_span_to_span(file_id, item.span(), char_to_byte)
            .unwrap_or_else(|| Span::point(file_id, ByteOffset::new(0)));
        tasks.push(Spanned::new(task, span));
    }
    Ok(tasks)
}

/// Parse one task mapping.
fn parse_task(
    mapping: &MarkedMappingNode,
    file_id: FileId,
    char_to_byte: &CharToByte,
) -> Result<RawTask, SchemaError> {
    let name = extract_scalar(mapping, "name", file_id, char_to_byte)?.ok_or_else(|| {
        SchemaError::MissingField {
            field: "name".to_owned(),
            span: yaml_span_to_span(file_id, mapping.span(), char_to_byte),
        }
    })?;

    let action = parse_action(mapping, file_id, char_to_byte)?;
    let mut task = RawTask::new(name, action);
    task.depends_on = parse_string_list(mapping, "depends_on", file_id, char_to_byte)?;
    task.condition = extract_scalar(mapping, "condition", file_id, char_to_byte)?;
    task.for_each = extract_scalar(mapping, "for_each", file_id, char_to_byte)?;
    Ok(task)
}

/// Extract an optional list of string scalars under `key`.
///
/// Returns `Ok(vec![])` when the key is absent. Returns
/// [`SchemaError::Validation`] if the key is present but is not a
/// sequence, or if any element is not a scalar string.
fn parse_string_list(
    mapping: &MarkedMappingNode,
    key: &str,
    file_id: FileId,
    char_to_byte: &CharToByte,
) -> Result<Vec<Spanned<String>>, SchemaError> {
    let Some(node) = mapping.get_node(key) else {
        return Ok(Vec::new());
    };
    let Some(seq) = node.as_sequence() else {
        return Err(SchemaError::Validation {
            message: format!("`{key}` must be a YAML sequence of strings"),
            span: yaml_span_to_span(file_id, node.span(), char_to_byte),
        });
    };
    let mut out = Vec::with_capacity(seq.len());
    for item in seq.iter() {
        let Some(scalar) = item.as_scalar() else {
            return Err(SchemaError::Validation {
                message: format!("each entry in `{key}` must be a string"),
                span: yaml_span_to_span(file_id, item.span(), char_to_byte),
            });
        };
        let span = yaml_span_to_span(file_id, scalar.span(), char_to_byte)
            .unwrap_or_else(|| Span::point(file_id, ByteOffset::new(0)));
        out.push(Spanned::new(scalar.as_str().to_owned(), span));
    }
    Ok(out)
}

/// Detect which of the four verb keys is present and parse it.
///
/// Returns [`SchemaError::MissingField`] if no verb key is present,
/// [`SchemaError::Validation`] if more than one is present
/// (ambiguous), and variant-specific errors for malformed bodies.
fn parse_action(
    task: &MarkedMappingNode,
    file_id: FileId,
    char_to_byte: &CharToByte,
) -> Result<RawAction, SchemaError> {
    let present: Vec<Verb> = Verb::all()
        .iter()
        .copied()
        .filter(|v| task.get_node(v.key()).is_some())
        .collect();
    match present.as_slice() {
        [] => Err(SchemaError::MissingField {
            field: "action (one of: infer, exec, invoke, agent)".to_owned(),
            span: yaml_span_to_span(file_id, task.span(), char_to_byte),
        }),
        [verb] => build_action(*verb, task, file_id, char_to_byte),
        many => {
            let joined: Vec<&'static str> = many.iter().map(|v| v.key()).collect();
            Err(SchemaError::Validation {
                message: format!(
                    "task has multiple action verbs ({}); exactly one required",
                    joined.join(", ")
                ),
                span: yaml_span_to_span(file_id, task.span(), char_to_byte),
            })
        }
    }
}

fn build_action(
    verb: Verb,
    task: &MarkedMappingNode,
    file_id: FileId,
    char_to_byte: &CharToByte,
) -> Result<RawAction, SchemaError> {
    let key = verb.key();
    let body = task
        .get_mapping(key)
        .ok_or_else(|| SchemaError::Validation {
            message: format!("`{key}` must be a mapping"),
            span: task
                .get_node(key)
                .and_then(|n| yaml_span_to_span(file_id, n.span(), char_to_byte)),
        })?;

    // Exhaustive match over Verb — the compiler refuses to let a
    // future variant sneak through without a dispatch arm.
    match verb {
        Verb::Infer => {
            let prompt = require_scalar(body, "prompt", file_id, char_to_byte)?;
            Ok(RawAction::Infer(RawInferAction::new(prompt)))
        }
        Verb::Exec => {
            let command = require_scalar(body, "command", file_id, char_to_byte)?;
            Ok(RawAction::Exec(RawExecAction::new(command)))
        }
        Verb::Invoke => {
            let tool = extract_scalar(body, "tool", file_id, char_to_byte)?;
            let resource = extract_scalar(body, "resource", file_id, char_to_byte)?;
            if tool.is_none() && resource.is_none() {
                return Err(SchemaError::MissingField {
                    field: "invoke.tool or invoke.resource".to_owned(),
                    span: yaml_span_to_span(file_id, body.span(), char_to_byte),
                });
            }
            Ok(RawAction::Invoke(RawInvokeAction::with_target(
                tool, resource,
            )))
        }
        Verb::Agent => {
            let prompt = require_scalar(body, "prompt", file_id, char_to_byte)?;
            Ok(RawAction::Agent(Box::new(RawAgentAction::new(prompt))))
        }
    }
}

/// Extract a required scalar: same as [`extract_scalar`] but produces
/// [`SchemaError::MissingField`] when the key is absent.
fn require_scalar(
    mapping: &MarkedMappingNode,
    key: &str,
    file_id: FileId,
    char_to_byte: &CharToByte,
) -> Result<Spanned<String>, SchemaError> {
    extract_scalar(mapping, key, file_id, char_to_byte)?.ok_or_else(|| SchemaError::MissingField {
        field: key.to_owned(),
        span: yaml_span_to_span(file_id, mapping.span(), char_to_byte),
    })
}

#[cfg(test)]
mod tests {
    use crate::error::SchemaError;
    use crate::parser::parse;
    use crate::raw::RawAction;
    use crate::source::FileId;

    fn fid() -> FileId {
        FileId::new(0)
    }

    #[test]
    fn parse_minimal_infer_task() {
        let yaml = "\
tasks:
  - name: greet
    infer:
      prompt: \"Say hello\"
";
        let wf = parse(yaml, fid()).expect("parse");
        assert_eq!(wf.tasks.len(), 1);
        let task = &wf.tasks[0].value;
        assert_eq!(task.name.value, "greet");
        let RawAction::Infer(ref action) = task.action else {
            panic!("expected Infer");
        };
        assert_eq!(action.prompt.value, "Say hello");
    }

    #[test]
    fn parse_minimal_exec_task() {
        let yaml = "\
tasks:
  - name: list
    exec:
      command: ls -la
";
        let wf = parse(yaml, fid()).expect("parse");
        let RawAction::Exec(ref action) = wf.tasks[0].value.action else {
            panic!("expected Exec");
        };
        assert_eq!(action.command.value, "ls -la");
    }

    // `fetch` is NOT a verb (spec D-2026-05-22-N18 · 4 verbs absolute) — it
    // is the `nika:fetch` builtin reached via `invoke:`. A top-level `fetch:`
    // key must therefore be rejected as "no verb present".
    #[test]
    fn fetch_key_is_not_a_verb() {
        let yaml = "\
tasks:
  - name: poll
    fetch:
      url: https://api.example.com/v1/status
";
        let err = parse(yaml, fid()).expect_err("fetch: is not a verb");
        let SchemaError::MissingField { field, .. } = err else {
            panic!("expected MissingField, got {err:?}");
        };
        assert!(
            field.contains("infer, exec, invoke, agent"),
            "error should list the 4 verbs without fetch, got: {field}"
        );
    }

    #[test]
    fn parse_minimal_invoke_task_with_tool() {
        let yaml = "\
tasks:
  - name: search
    invoke:
      tool: web_search
";
        let wf = parse(yaml, fid()).expect("parse");
        let RawAction::Invoke(ref action) = wf.tasks[0].value.action else {
            panic!("expected Invoke");
        };
        assert_eq!(
            action.tool.as_ref().map(|s| s.value.as_str()),
            Some("web_search")
        );
        assert!(action.resource.is_none());
    }

    #[test]
    fn parse_minimal_invoke_task_with_resource() {
        let yaml = "\
tasks:
  - name: read_doc
    invoke:
      resource: file:///tmp/readme.md
";
        let wf = parse(yaml, fid()).expect("parse");
        let RawAction::Invoke(ref action) = wf.tasks[0].value.action else {
            panic!("expected Invoke");
        };
        assert_eq!(
            action.resource.as_ref().map(|s| s.value.as_str()),
            Some("file:///tmp/readme.md")
        );
    }

    #[test]
    fn parse_minimal_agent_task() {
        let yaml = "\
tasks:
  - name: research
    agent:
      prompt: Investigate X
";
        let wf = parse(yaml, fid()).expect("parse");
        let RawAction::Agent(ref action) = wf.tasks[0].value.action else {
            panic!("expected Agent");
        };
        assert_eq!(action.prompt.value, "Investigate X");
    }

    #[test]
    fn parse_multiple_tasks() {
        let yaml = "\
tasks:
  - name: step1
    exec:
      command: echo one
  - name: step2
    infer:
      prompt: summarize
  - name: step3
    exec:
      command: echo three
";
        let wf = parse(yaml, fid()).expect("parse");
        assert_eq!(wf.tasks.len(), 3);
        assert_eq!(wf.tasks[0].value.name.value, "step1");
        assert_eq!(wf.tasks[1].value.name.value, "step2");
        assert_eq!(wf.tasks[2].value.name.value, "step3");
    }

    #[test]
    fn parse_task_without_name_errors() {
        let yaml = "\
tasks:
  - exec:
      command: ls
";
        let err = parse(yaml, fid()).expect_err("missing name");
        assert!(
            matches!(&err, SchemaError::MissingField { field, .. } if field == "name"),
            "expected MissingField(name), got {err:?}",
        );
    }

    #[test]
    fn parse_task_without_action_errors() {
        let yaml = "\
tasks:
  - name: just_a_name
";
        let err = parse(yaml, fid()).expect_err("missing action");
        assert!(
            matches!(&err, SchemaError::MissingField { field, .. } if field.contains("action")),
            "expected MissingField(action, ...), got {err:?}",
        );
    }

    #[test]
    fn parse_task_with_two_verbs_errors() {
        // A task cannot be both `infer` and `exec` — the analyzer
        // will never choose silently.
        let yaml = "\
tasks:
  - name: confused
    infer:
      prompt: hi
    exec:
      command: ls
";
        let err = parse(yaml, fid()).expect_err("ambiguous verbs");
        assert!(matches!(err, SchemaError::Validation { .. }));
    }

    #[test]
    fn parse_invoke_without_tool_or_resource_errors() {
        // Empty invoke body so this test cannot be accidentally
        // satisfied by Round 2e once optional fields (`mcp`,
        // `params`, `timeout_ms`) start being parsed. The point is
        // that at least one of `tool` / `resource` must be present.
        let yaml = "\
tasks:
  - name: empty_invoke
    invoke: {}
";
        let err = parse(yaml, fid()).expect_err("invoke needs tool or resource");
        assert!(matches!(&err, SchemaError::MissingField { .. }));
    }

    #[test]
    fn parse_infer_without_prompt_errors() {
        let yaml = "\
tasks:
  - name: no_prompt
    infer:
      model: gpt-4o
";
        let err = parse(yaml, fid()).expect_err("infer needs prompt");
        assert!(
            matches!(&err, SchemaError::MissingField { field, .. } if field == "prompt"),
            "expected MissingField(prompt), got {err:?}",
        );
    }

    #[test]
    fn parse_tasks_as_mapping_errors() {
        let yaml = "\
tasks:
  a: b
";
        let err = parse(yaml, fid()).expect_err("tasks must be a sequence");
        assert!(
            matches!(&err, SchemaError::Validation { message, .. } if message.contains("sequence")),
            "got {err:?}",
        );
    }

    #[test]
    fn parse_task_entry_must_be_mapping() {
        let yaml = "\
tasks:
  - just_a_scalar
";
        let err = parse(yaml, fid()).expect_err("task must be a mapping");
        assert!(matches!(err, SchemaError::Validation { .. }));
    }

    #[test]
    fn parse_action_body_must_be_mapping() {
        // `infer: foo` — infer's body must be a mapping, not a scalar.
        let yaml = "\
tasks:
  - name: bad
    infer: foo
";
        let err = parse(yaml, fid()).expect_err("infer body must be mapping");
        assert!(matches!(&err, SchemaError::Validation { .. }));
    }

    #[test]
    fn parse_preserves_task_span() {
        // The outer task's Spanned wrapper must carry a non-blank span
        // so downstream analyzer diagnostics can underline the task.
        let yaml = "\
tasks:
  - name: t1
    exec:
      command: echo
";
        let wf = parse(yaml, FileId::new(9)).expect("parse");
        assert_eq!(wf.tasks[0].span.file, FileId::new(9));
    }

    // ─── Round 2e: optional task-level fields ──────────────────

    #[test]
    fn parse_depends_on_multiple_tasks() {
        let yaml = "\
tasks:
  - name: a
    exec:
      command: echo a
  - name: b
    exec:
      command: echo b
  - name: c
    depends_on:
      - a
      - b
    exec:
      command: echo c
";
        let wf = parse(yaml, fid()).expect("parse");
        let c = &wf.tasks[2].value;
        assert_eq!(c.depends_on.len(), 2);
        assert_eq!(c.depends_on[0].value, "a");
        assert_eq!(c.depends_on[1].value, "b");
    }

    #[test]
    fn parse_depends_on_absent_defaults_to_empty() {
        let yaml = "\
tasks:
  - name: lonely
    exec:
      command: ls
";
        let wf = parse(yaml, fid()).expect("parse");
        assert!(wf.tasks[0].value.depends_on.is_empty());
    }

    #[test]
    fn parse_depends_on_preserves_file_id_on_each_entry() {
        let yaml = "\
tasks:
  - name: first
    exec:
      command: echo
  - name: second
    depends_on:
      - first
    exec:
      command: echo
";
        let wf = parse(yaml, FileId::new(3)).expect("parse");
        let second = &wf.tasks[1].value;
        assert_eq!(second.depends_on[0].span.file, FileId::new(3));
    }

    #[test]
    fn parse_depends_on_as_scalar_errors() {
        let yaml = "\
tasks:
  - name: bad
    depends_on: some_task
    exec:
      command: ls
";
        let err = parse(yaml, fid()).expect_err("depends_on must be a sequence");
        assert!(
            matches!(&err, SchemaError::Validation { message, .. } if message.contains("sequence")),
            "got {err:?}",
        );
    }

    #[test]
    fn parse_depends_on_with_non_scalar_element_errors() {
        let yaml = "\
tasks:
  - name: bad
    depends_on:
      - name: oops
    exec:
      command: ls
";
        let err = parse(yaml, fid()).expect_err("element must be a string");
        assert!(matches!(err, SchemaError::Validation { .. }));
    }

    #[test]
    fn parse_condition_and_for_each() {
        let yaml = "\
tasks:
  - name: guarded
    condition: \"{{ ready }}\"
    for_each: \"{{ items }}\"
    exec:
      command: echo
";
        let wf = parse(yaml, fid()).expect("parse");
        let t = &wf.tasks[0].value;
        assert_eq!(t.condition.as_ref().unwrap().value, "{{ ready }}");
        assert_eq!(t.for_each.as_ref().unwrap().value, "{{ items }}");
    }

    #[test]
    fn parse_condition_as_mapping_errors() {
        // Templates are strings — a mapping here is a malformed YAML
        // expression. Must fail loud so the author sees the typo.
        let yaml = "\
tasks:
  - name: bad
    condition:
      key: value
    exec:
      command: ls
";
        let err = parse(yaml, fid()).expect_err("condition must be a scalar");
        assert!(matches!(err, SchemaError::Validation { .. }));
    }
}
