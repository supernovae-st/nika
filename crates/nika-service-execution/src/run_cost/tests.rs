// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;
const MODEL: &str = "deepseek/unpriced-shape-fixture";
const ONE: &str = "nika: bounded\nmodel: deepseek/unpriced-shape-fixture\ntasks:\n  first:\n    infer: { prompt: text, max_tokens: 32 }\n";
fn plan() -> nika_providers::ExecutionAccessPlan {
    use nika_providers::probe::{ExecutionLocus, ProviderProbe, ProviderReadiness};
    let probe = ProviderProbe::new(
        "deepseek",
        true,
        true,
        "DEEPSEEK_API_KEY",
        false,
        ProviderReadiness::new(
            true,
            true,
            None,
            None,
            true,
            ExecutionLocus::Cloud,
            nika_types::access::AccessClass::Api,
        ),
        "https://api.deepseek.com",
    );
    nika_providers::resolve_execution_plan(
        &[nika_providers::ModelNeed::new(MODEL, true, false)],
        &[probe],
        Some("api"),
    )
}
fn parsed(source: &str) -> RawWorkflow {
    nika_schema::parse(
        source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("test must reach structural review")
}
#[test]
fn count_comes_from_checked_waves_and_skips_are_upper_bounds() {
    for gate in ["true", "false"] {
        let source = format!(
            "{ONE}  second:\n    after: {{ first: terminal }}\n    when: {gate}\n    infer: {{ prompt: text, max_tokens: 16 }}\n"
        );
        assert_eq!(request_bound(&parsed(&source), &plan(), 1).unwrap(), 2);
    }
    assert_eq!(request_bound(&parsed(ONE), &plan(), 1).unwrap(), 1);
}
#[test]
fn parallel_dynamic_mixed_unbounded_and_hidden_inference_are_refused() {
    let variants = [
        format!("{ONE}  parallel:\n    infer: {{ prompt: text, max_tokens: 32 }}\n"),
        ONE.replace("prompt: text", "prompt: text, model: openai/gpt-4o-mini"),
        ONE.replace(
            "model: deepseek/unpriced-shape-fixture",
            "model: '${{ inputs.model }}'",
        ),
        ONE.replace(", max_tokens: 32", ""),
        ONE.replace("max_tokens: 32", "max_tokens: 8193"),
        ONE.replace("prompt: text", "prompt: text, thinking: { enabled: true }"),
        ONE.replace(
            "prompt: text",
            "prompt: text, vision: [{ source: file, path: './image.png' }]",
        ),
        ONE.replace("    infer:", "    retry: { max_attempts: 1 }\n    infer:"),
        ONE.replace("    infer:", "    for_each: { items: [a] }\n    infer:"),
        ONE.replace("    infer:", "    on_error: { skip: true }\n    infer:"),
        ONE.replace(
            "tasks:",
            "secrets:\n  token: { source: env, key: NEVER_READ, egress: [{ to: exec }] }\ntasks:",
        ),
        ONE.replace("    infer:", "    agent:")
            .replace("max_tokens:", "max_tokens_total:"),
        format!("{ONE}  nested:\n    invoke: {{ workflow: child.nika }}\n"),
        format!("{ONE}  process:\n    exec: {{ command: ['echo', 'data'] }}\n"),
        format!(
            "{ONE}  extra:\n    invoke: {{ tool: 'nika:fetch', args: {{ url: 'https://example.com/' }} }}\n"
        ),
    ];
    for source in variants {
        assert!(
            request_bound(&parsed(&source), &plan(), 1).is_err(),
            "{source}"
        );
    }
    assert!(request_bound(&parsed(ONE), &plan(), 2).is_err());
}

#[test]
fn structured_tasks_count_every_schema_reask_not_just_dag_nodes() {
    let structured = ONE.replace("prompt: text", "prompt: text, schema: { type: string }");
    let per_schema = 1 + u32::from(nika_verb_infer::DEFAULT_SCHEMA_RETRY_BUDGET);
    assert_eq!(
        request_bound(&parsed(&structured), &plan(), 1).unwrap(),
        per_schema
    );
    let plain_then_schema = format!(
        "{ONE}  second:\n    after: {{ first: success }}\n    infer: {{ prompt: text, max_tokens: 32, schema: {{ type: string }} }}\n"
    );
    assert_eq!(
        request_bound(&parsed(&plain_then_schema), &plan(), 1).unwrap(),
        1 + per_schema
    );
    let two_schemas = plain_then_schema.replace(
        "prompt: text, max_tokens: 32 }",
        "prompt: text, max_tokens: 32, schema: { type: string } }",
    );
    assert_eq!(
        request_bound(&parsed(&two_schemas), &plan(), 1).unwrap(),
        2 * per_schema
    );
}

#[test]
fn immutable_const_paths_and_assert_keep_the_existing_effect_gate() {
    let source = ONE.replace("tasks:", "const:\n  path: './input.txt'\npermits:\n  tools: ['nika:read', 'nika:assert']\n  fs: { read: ['./input.txt'] }\ntasks:");
    let source = format!(
        "{source}  read:\n    invoke: {{ tool: 'nika:read', args: {{ path: '${{{{ const.path }}}}' }} }}\n  admit:\n    after: {{ first: success }}\n    invoke: {{ tool: 'nika:assert', args: {{ condition: true }} }}\n"
    );
    assert_eq!(request_bound(&parsed(&source), &plan(), 1).unwrap(), 1);
    // Pure assertion needs no tool grant; effectful reads still do.
    let without_assert_grant = source.replace("['nika:read', 'nika:assert']", "['nika:read']");
    assert_eq!(
        request_bound(&parsed(&without_assert_grant), &plan(), 1).unwrap(),
        1
    );
    for invalid in [
        source.replace("['nika:read', 'nika:assert']", "['nika:assert']"),
        source.replace("'./input.txt'\npermits:", "'../escape.txt'\npermits:"),
        source.replace("${{ const.path }}", "${{ inputs.path }}"),
        source.replace("${{ const.path }}", "prefix/${{ const.path }}"),
    ] {
        assert!(
            request_bound(&parsed(&invalid), &plan(), 1).is_err(),
            "{invalid}"
        );
    }
}
#[test]
fn local_steps_are_narrow_and_check_permits_are_an_independent_gate() {
    let source = "nika: local\nmodel: deepseek/unpriced-shape-fixture\npermits:\n  tools: ['nika:read', 'nika:write', 'nika:jq']\n  fs: { read: ['./input.txt'], write: ['./output.txt'] }\ntasks:\n  read:\n    invoke: { tool: 'nika:read', args: { path: './input.txt' } }\n  first:\n    with: { data: '${{ tasks.read.output }}' }\n    infer: { prompt: '${{ with.data }}', max_tokens: 32 }\n  write:\n    with: { data: '${{ tasks.first.output }}' }\n    invoke: { tool: 'nika:write', args: { path: './output.txt', content: '${{ with.data }}' } }\n";
    assert_eq!(request_bound(&parsed(source), &plan(), 1).unwrap(), 1);
    let no_permit = source.replace("write: ['./output.txt']", "write: []");
    assert!(request_bound(&parsed(&no_permit), &plan(), 1).is_err());
    for path in ["/outside.txt", "../escape.txt", "${{ const.path }}"] {
        let changed = source.replace("path: './input.txt'", &format!("path: '{path}'"));
        assert!(request_bound(&parsed(&changed), &plan(), 1).is_err());
    }
}

#[test]
fn each_refusal_is_a_typed_reason_that_keeps_its_words() {
    use RunShapeError as E;
    let cases = [
        (
            format!("{ONE}  parallel:\n    infer: {{ prompt: text, max_tokens: 32 }}\n"),
            E::Parallel,
        ),
        (
            ONE.replace("prompt: text", "prompt: text, model: openai/gpt-4o-mini"),
            E::OtherModel,
        ),
        (ONE.replace(", max_tokens: 32", ""), E::InferShape),
        (
            ONE.replace("    infer:", "    retry: { max_attempts: 1 }\n    infer:"),
            E::Control,
        ),
        (
            ONE.replace(
                "tasks:",
                "secrets:\n  token: { source: env, key: NEVER_READ, egress: [{ to: exec }] }\ntasks:",
            ),
            E::Secrets,
        ),
        (
            format!("{ONE}  process:\n    exec: {{ command: ['echo', 'data'] }}\n"),
            E::Action,
        ),
        (
            format!(
                "{ONE}  extra:\n    invoke: {{ tool: 'nika:fetch', args: {{ url: 'https://example.com/' }} }}\n"
            ),
            E::Tool,
        ),
    ];
    for (source, reason) in cases {
        assert_eq!(
            request_bound(&parsed(&source), &plan(), 1),
            Err(reason),
            "{source}"
        );
    }
    assert_eq!(request_bound(&parsed(ONE), &plan(), 2), Err(E::Route));
    assert_eq!(add_requests(u32::MAX - 1, 1), Ok(u32::MAX));
    assert_eq!(add_requests(u32::MAX, 1), Err(E::Overflow));
    for (reason, words) in [
        (
            E::Route,
            "unknown-cost Run requires one exact admitted API route",
        ),
        (E::Overflow, "request bound overflow"),
        (
            E::Tool,
            "unknown-cost Run supports only direct infer and local read/write/jq/assert; no nested workflow or other tools",
        ),
        (
            E::DynamicPath,
            "local file step requires a literal or immutable const path",
        ),
        (
            E::UnconfinedPath,
            "unknown-cost Run file paths must be static files confined to the project",
        ),
    ] {
        assert_eq!(reason.to_string(), words);
    }
}

/// The path of a workflow's one `nika:read` step, through the public resolver.
fn read_path(source: &str) -> Result<PathBuf, RunShapeError> {
    let wf = parsed(source);
    let action = wf
        .tasks
        .iter()
        .find_map(|task| match &task.value.action {
            RawAction::Invoke(action) if action.tool().is_some_and(|t| t.value == "nika:read") => {
                Some(action)
            }
            _ => None,
        })
        .expect("a read step");
    project_file_path(&ConstStrings::of(&wf), action)
}

#[test]
fn project_paths_are_typed_as_dynamic_or_unconfined() {
    let source = "nika: local\nmodel: deepseek/unpriced-shape-fixture\npermits:\n  tools: ['nika:read']\n  fs: { read: ['./input.txt'] }\ntasks:\n  read:\n    invoke: { tool: 'nika:read', args: { path: './input.txt' } }\n";
    assert_eq!(read_path(source), Ok(PathBuf::from("input.txt")));
    for path in ["/outside.txt", "../escape.txt"] {
        let changed = source.replace("path: './input.txt'", &format!("path: '{path}'"));
        assert_eq!(
            read_path(&changed),
            Err(RunShapeError::UnconfinedPath),
            "{path}"
        );
    }
    let dynamic = source.replace("path: './input.txt'", "path: '${{ inputs.path }}'");
    assert_eq!(read_path(&dynamic), Err(RunShapeError::DynamicPath));
}
