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

const FREE: &str = "openrouter/qwen/qwen3.8-27b:free";
const FREE_ONE: &str = "nika: free\nmodel: openrouter/qwen/qwen3.8-27b:free\npermits: {}\ntasks:\n  first:\n    infer: { prompt: text, max_tokens: 64 }\n";

/// An admitted API plan over the given (model, provider, key env) lanes.
fn lanes(needs: &[(&str, &str, &str)]) -> nika_providers::ExecutionAccessPlan {
    use nika_providers::probe::{ExecutionLocus, ProviderProbe, ProviderReadiness};
    let probes: Vec<ProviderProbe> = needs
        .iter()
        .map(|&(_, provider, env)| {
            ProviderProbe::new(
                provider,
                true,
                true,
                env,
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
                "https://fixture.invalid",
            )
        })
        .collect();
    let models: Vec<nika_providers::ModelNeed> = needs
        .iter()
        .map(|&(model, _, _)| nika_providers::ModelNeed::new(model, true, false))
        .collect();
    nika_providers::resolve_execution_plan(&models, &probes, Some("api"))
}

fn free_shape(source: &str, override_model: Option<&str>) -> Result<bool, FreeShapeRefusal> {
    let plan = lanes(&[
        (FREE, "openrouter", "OPENROUTER_API_KEY"),
        ("deepseek/deepseek-chat", "deepseek", "DEEPSEEK_API_KEY"),
    ]);
    declared_free_shape(
        &parsed(source),
        &plan,
        &nika_providers::ProvidersConfig::new(),
        override_model,
    )
}

/// C2 · a bounded text infer on an exact declared-free route is the one shape
/// its observation admits; Check and Run read the same `Ok(true)`.
#[test]
fn a_bounded_text_infer_on_a_declared_free_route_is_admitted() {
    assert_eq!(free_shape(FREE_ONE, None), Ok(true));
    let schema = FREE_ONE.replace("max_tokens: 64", "max_tokens: 64, schema: { type: object }");
    assert_eq!(
        free_shape(&schema, None),
        Ok(true),
        "structured text stays text"
    );
    let quiet_thinking = FREE_ONE.replace(
        "max_tokens: 64",
        "max_tokens: 64, thinking: { enabled: false, budget_tokens: 1024 }",
    );
    assert_eq!(
        free_shape(&quiet_thinking, None),
        Ok(true),
        "a disabled thinking config never reaches the wire"
    );
}

/// C2 · every shape the provider's bounded-text guard would refuse is named
/// before any effect: never a runtime surprise, never a known zero.
#[test]
fn unsupported_free_shapes_are_refused_by_task_and_shape() {
    let vision = FREE_ONE.replace(
        "max_tokens: 64",
        "max_tokens: 64, vision: [{ source: file, path: './image.png' }]",
    );
    let thinking = FREE_ONE.replace(
        "max_tokens: 64",
        "max_tokens: 64, thinking: { enabled: true, budget_tokens: 1024 }",
    );
    let unbounded = FREE_ONE.replace(", max_tokens: 64", "");
    let oversized = FREE_ONE.replace("max_tokens: 64", "max_tokens: 4000000");
    let agent = FREE_ONE
        .replace("    infer:", "    agent:")
        .replace("max_tokens:", "max_tokens_total:");
    let bound = "an output bound outside its tariff";
    for (source, shape) in [
        (vision, "vision"),
        (thinking, "thinking"),
        (unbounded, bound),
        (oversized, bound),
        (agent, "an agent loop"),
    ] {
        let refusal = free_shape(&source, None).expect_err(shape);
        assert_eq!((refusal.task.as_str(), refusal.shape), ("first", shape));
        assert_eq!(refusal.model, FREE);
        assert!(
            refusal.to_string().contains("declared-free route"),
            "{refusal}"
        );
    }
}

/// Other lanes keep their own policy: a paid route's vision is not this
/// guard's business, and a plan without a declared-free lane reads `false`.
#[test]
fn only_declared_free_lanes_are_judged() {
    let paid_vision = format!(
        "{FREE_ONE}  look:\n    infer: {{ prompt: text, model: deepseek/deepseek-chat, max_tokens: 64, vision: [{{ source: file, path: './image.png' }}] }}\n"
    );
    assert_eq!(free_shape(&paid_vision, None), Ok(true));
    let paid_only = lanes(&[("deepseek/deepseek-chat", "deepseek", "DEEPSEEK_API_KEY")]);
    let source = FREE_ONE.replace(FREE, "deepseek/deepseek-chat");
    assert_eq!(
        declared_free_shape(
            &parsed(&source),
            &paid_only,
            &nika_providers::ProvidersConfig::new(),
            None
        ),
        Ok(false)
    );
}

/// The Run's `--model` names the lane a model-less task rides.
#[test]
fn the_run_override_names_the_lane_a_modelless_task_rides() {
    let modelless = "nika: plain\npermits: {}\ntasks:\n  first:\n    infer: { prompt: text, vision: [{ source: file, path: './image.png' }], max_tokens: 64 }\n";
    let refusal = free_shape(modelless, Some(FREE)).expect_err("override rides the free lane");
    assert_eq!(refusal.shape, "vision");
    assert_eq!(
        free_shape(modelless, None),
        Ok(true),
        "no override, no free task"
    );
}

/// E6 S1 · a declared-free lane beside an unknown-cost lane never shares one
/// account: the unknown-cost review refuses the two-lane plan before any
/// question or effect.
#[test]
fn a_declared_free_lane_beside_an_unknown_cost_lane_is_refused_before_review() {
    let two = lanes(&[
        (FREE, "openrouter", "OPENROUTER_API_KEY"),
        (MODEL, "deepseek", "DEEPSEEK_API_KEY"),
    ]);
    let source =
        format!("{ONE}  free:\n    infer: {{ prompt: text, model: {FREE}, max_tokens: 32 }}\n");
    assert_eq!(
        request_bound(&parsed(&source), &two, 1),
        Err(RunShapeError::Route)
    );
}
