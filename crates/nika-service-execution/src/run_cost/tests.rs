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
        ONE.replace("    infer:", "    retry: { max_attempts: 2 }\n    infer:"),
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
            ONE.replace("    infer:", "    retry: { max_attempts: 2 }\n    infer:"),
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

fn run_time(source: &str, overrides: &[(&str, &str)]) -> Result<bool, RunTimeModelRefusal> {
    let overrides = overrides
        .iter()
        .map(|&(k, v)| (k.to_owned(), serde_json::Value::from(v)))
        .collect();
    run_time_models(
        &parsed(source),
        &nika_providers::resolve_execution_plan(&[], &[], None),
        &nika_providers::ProvidersConfig::new(),
        &overrides,
    )
}

const DYNAMIC: &str = "nika: dynamic\ninputs:\n  m: { type: string, required: true }\npermits: {}\ntasks:\n  first:\n    infer: { prompt: text, model: \"${{ inputs.m }}\", max_tokens: 64, vision: [{ source: file, path: './image.png' }] }\n";
const VISION: &str = ", vision: [{ source: file, path: './image.png' }]";

/// C4 · E13 F1: a `model:` its inputs decide is judged before any effect
/// exactly as that literal would be: the same shape and route classes.
#[test]
fn a_run_time_model_is_judged_at_the_value_its_inputs_decide() {
    let refusal = run_time(DYNAMIC, &[("m", FREE)]).expect_err("free vision");
    assert!(
        matches!(&refusal, RunTimeModelRefusal::FreeShape(s)
            if (s.task.as_str(), s.model.as_str(), s.shape) == ("first", FREE, "vision")),
        "{refusal:?}"
    );
    assert!(
        refusal.to_string().contains("declared-free route"),
        "{refusal}"
    );
    for model in [
        "deepseek/deepseek-v4-pro",
        "anthropic/claude-sonnet-4-5-20250929",
        "mock/echo",
        "ollama/llama3.2",
    ] {
        assert_eq!(
            run_time(DYNAMIC, &[("m", model)]),
            Ok(true),
            "{model} keeps its policy"
        );
    }
    for model in [
        "mistral/mistral-small-latest",
        "openrouter/google/gemma-4-26b-a4b-it:free",
        "openrouter/vendor/unseen:free",
    ] {
        assert_eq!(
            run_time(DYNAMIC, &[("m", model)]),
            Err(RunTimeModelRefusal::UnknownCost {
                task: "first".into(),
                model: model.into()
            }),
            "{model}: the review would not admit it as a literal"
        );
    }
    let text = DYNAMIC.replace(VISION, "");
    assert_eq!(
        run_time(&text, &[("m", FREE)]),
        Ok(true),
        "bounded text is observed"
    );
    assert_eq!(
        run_time(ONE, &[]),
        Ok(false),
        "literal models are the plan's"
    );
    let nested =
        "nika: root\npermits: {}\ntasks:\n  sub:\n    invoke: { workflow: ./child.nika }\n";
    assert_eq!(
        run_time(nested, &[]),
        Ok(true),
        "no root plan sees a child's routes: they are judged at its dispatch"
    );
}

/// Defaults, const and a `with:` alias decide as the runtime's walk does; a
/// value only the run decides stays dynamic for the dispatch observer.
#[test]
fn defaults_const_and_upstream_values_follow_the_runtime_walk() {
    let defaulted = DYNAMIC.replace("required: true", &format!("default: \"{FREE}\""));
    assert!(matches!(
        run_time(&defaulted, &[]),
        Err(RunTimeModelRefusal::FreeShape(_))
    ));
    assert_eq!(
        run_time(&defaulted, &[("m", "deepseek/deepseek-v4-pro")]),
        Ok(true),
        "the operator's value wins over the default"
    );
    let constant = DYNAMIC
        .replace(
            "inputs:\n  m: { type: string, required: true }\n",
            &format!("const:\n  m: \"{FREE}\"\n"),
        )
        .replace("inputs.m", "const.m");
    assert!(matches!(
        run_time(&constant, &[]),
        Err(RunTimeModelRefusal::FreeShape(_))
    ));
    let upstream = format!(
        "nika: upstream\npermits: {{}}\ntasks:\n  pick:\n    infer: {{ prompt: name one, model: mock/echo, max_tokens: 16 }}\n  first:\n    with: {{ m: \"${{{{ tasks.pick.output }}}}\" }}\n    infer: {{ prompt: text, model: \"${{{{ with.m }}}}\", max_tokens: 64{VISION} }}\n"
    );
    assert_eq!(
        run_time(&upstream, &[]),
        Ok(true),
        "undecidable before any effect"
    );
    let agent = DYNAMIC
        .replace("    infer:", "    agent:")
        .replace(&format!("max_tokens: 64{VISION}"), "max_tokens_total: 64");
    let refusal = run_time(&agent, &[("m", FREE)]).expect_err("agent loop");
    assert!(refusal.to_string().contains("an agent loop"), "{refusal}");
}

const LOCALE: &str = "nika: locale-report\ninputs:\n  locale: {type: string, required: true}\n  count: {type: integer, default: 3}\npermits:\n  tools: [nika:write]\n  fs: {write: [./locale.txt]}\ntasks:\n  save:\n    invoke:\n      tool: nika:write\n      args: { path: ./locale.txt, content: '${{ inputs.locale }}' }\n";

fn scheduled(source: &str, bindings: &[&str], ceiling: f64) -> ScheduledProgram {
    let wf = parsed(source);
    let report = nika_check::check(&wf);
    let bindings: Vec<String> = bindings.iter().map(|&b| b.to_owned()).collect();
    scheduled_program(
        &wf,
        &report,
        &nika_providers::ProvidersConfig::new(),
        &bindings,
        ceiling,
    )
}

fn kinds(program: &ScheduledProgram) -> Vec<(&str, Option<&str>, Option<&str>)> {
    program
        .blockers
        .iter()
        .map(|b| (b.kind, b.subject.as_deref(), b.reason))
        .collect()
}

/// C5 · R4 71: a required input with no source is unready under the
/// registered NIKA-1708; a literal or a declared default binds it; the
/// document names each source and never a value.
#[test]
fn a_scheduled_program_names_each_binding_source_and_never_a_value() {
    let missing = scheduled(LOCALE, &[], 0.05);
    assert!(!missing.required_inputs_ready);
    assert_eq!(kinds(&missing), [("input_unbound", Some("locale"), None)]);
    assert_eq!(missing.blockers[0].code, Some("NIKA-1708"));
    assert_eq!(
        missing.document["unbound_inputs"],
        serde_json::json!(["locale"])
    );
    let bound = scheduled(LOCALE, &["locale=fr-secret-literal"], 0.05);
    assert!(bound.required_inputs_ready && bound.blockers.is_empty());
    let required = &bound.document["required_inputs"][0];
    assert_eq!(required["binding_source"], "schedule_literal");
    assert_eq!(required["binding_status"], "bound");
    assert_eq!(
        bound.document["optional_inputs"][0]["binding_source"],
        "workflow_default"
    );
    assert_eq!(bound.document["unbound_inputs"], serde_json::json!([]));
    assert!(!bound.document.to_string().contains("fr-secret-literal"));
    assert_eq!(bound.model_cost_ready, Some(true), "no model route");
    assert_eq!(
        bound.document["model_summary"]["routes"],
        serde_json::json!([])
    );
}

/// E16-4 · each failing binding is named with its reason; a correctly
/// bound required input is never blamed for a neighbour.
#[test]
fn a_refused_binding_is_attributed_only_to_itself() {
    let extra = scheduled(LOCALE, &["locale=fr", "extra=1"], 0.05);
    assert!(!extra.required_inputs_ready);
    assert_eq!(
        kinds(&extra),
        [("input_refused", Some("extra"), Some("unknown_input"))]
    );
    assert_eq!(
        extra.document["required_inputs"][0]["binding_status"],
        "bound"
    );
    let declared = LOCALE.replace("permits:\n", "permits:\n  env: [C5_SURELY_UNSET_VAR]\n");
    let unset = scheduled(&declared, &["locale=@env:C5_SURELY_UNSET_VAR"], 0.05);
    assert_eq!(
        kinds(&unset),
        [("input_refused", Some("locale"), Some("env_unset"))],
        "refused once, never also unbound"
    );
    // R4 71: required minus bound — a refused binding binds nothing.
    assert_eq!(
        unset.document["unbound_inputs"],
        serde_json::json!(["locale"])
    );
    assert_eq!(unset.document["required_inputs"][0]["reason"], "env_unset");
    let typed = scheduled(LOCALE, &["locale=fr", "count=not-a-number"], 0.05);
    assert_eq!(
        kinds(&typed),
        [("input_refused", Some("count"), Some("type_mismatch"))]
    );
    assert!(!typed.blockers[0].message.contains("not-a-number"));
}

/// The model and cost law of an unattended fire: values the inputs decide
/// are judged as literals; routes only a dispatch can judge stay unknown,
/// never ready; the plafond floors a priced route.
#[test]
fn model_and_cost_readiness_never_turns_unknown_into_ready() {
    let dynamic = "nika: d\ninputs:\n  m: {type: string, required: true}\npermits: {}\ntasks:\n  ask:\n    infer: { prompt: hi, model: \"${{ inputs.m }}\", max_tokens: 64 }\n";
    let unknown = scheduled(dynamic, &["m=mistral/mistral-small-latest"], 0.05);
    assert_eq!(unknown.model_cost_ready, Some(false));
    assert_eq!(
        kinds(&unknown),
        [(
            "unknown_cost_unreviewable",
            Some("mistral/mistral-small-latest"),
            None
        )]
    );
    let mock = scheduled(dynamic, &["m=mock/echo"], 0.05);
    assert_eq!(mock.model_cost_ready, Some(true));
    assert_eq!(mock.document["model_summary"]["routes"][0]["class"], "mock");
    let upstream = "nika: u\npermits: {}\ntasks:\n  pick:\n    infer: { prompt: name one, model: mock/echo, max_tokens: 16 }\n  ask:\n    with: { m: \"${{ tasks.pick.output }}\" }\n    infer: { prompt: hi, model: \"${{ with.m }}\", max_tokens: 64 }\n";
    let undecided = scheduled(upstream, &[], 0.05);
    assert_eq!(undecided.model_cost_ready, None, "unknown, never ready");
    assert!(undecided.unknowns[0].contains("task `ask`"));
    let nested = "nika: n\npermits: {}\ntasks:\n  sub:\n    invoke: { workflow: ./child.nika }\n";
    let child = scheduled(nested, &[], 0.05);
    assert_eq!(child.model_cost_ready, None);
    assert!(child.unknowns.iter().any(|u| u.contains("nested workflow")));
    let priced = "nika: p\nmodel: deepseek/deepseek-v4-pro\npermits: {}\ntasks:\n  ask:\n    infer: { prompt: hi, max_tokens: 4000 }\n";
    let floored = scheduled(priced, &[], 0.000_000_1);
    assert!(
        floored
            .blockers
            .iter()
            .any(|b| b.kind == "budget_floor" && b.code == Some("NIKA-1709")),
        "{:?}",
        floored.blockers
    );
    assert_eq!(floored.model_cost_ready, Some(false));
}

/// The authority an unattended fire needs is listed, never acquired.
#[test]
fn authority_requirements_are_listed_and_never_acquired() {
    let gated = "nika: g\nsecrets:\n  token: { source: env, key: C5_TOKEN }\npermits:\n  tools: [nika:prompt]\ntasks:\n  gate:\n    invoke: { tool: 'nika:prompt', args: { message: 'continue?' } }\n";
    let program = scheduled(gated, &[], 0.05);
    let authority = &program.document["authority_summary"];
    assert_eq!(authority["human_gates"], serde_json::json!(["gate"]));
    assert_eq!(authority["secrets"][0]["name"], "token");
    assert_eq!(authority["secrets"][0]["source"], "env");
    assert_eq!(authority["activation"], "not_acquired");
    assert_eq!(
        authority["permits"]["tools"],
        serde_json::json!(["nika:prompt"])
    );
}

/// A decided route is judged as the literal it renders to: a reasoning
/// seat's `max_tokens` floor refuses it exactly as it refuses a literal
/// `model:`, and the route row names the task that decided it.
#[test]
fn a_decided_route_is_judged_as_its_literal() {
    let dynamic = "nika: d\ninputs:\n  m: {type: string, required: true}\npermits: {}\ntasks:\n  ask:\n    infer: { prompt: hi, model: \"${{ inputs.m }}\", max_tokens: 64 }\n";
    let tight = scheduled(dynamic, &["m=deepseek/deepseek-v4-pro"], 0.05);
    assert_eq!(tight.model_cost_ready, Some(false));
    assert!(
        tight
            .blockers
            .iter()
            .any(|b| b.kind == "model_admission_refused" && b.message.contains("max_tokens")),
        "{:?}",
        tight.blockers
    );
    let roomy = dynamic.replace("max_tokens: 64", "max_tokens: 4000");
    let judged = scheduled(&roomy, &["m=deepseek/deepseek-v4-pro"], 0.05);
    assert!(
        !judged
            .blockers
            .iter()
            .any(|b| b.kind == "model_admission_refused"),
        "{:?}",
        judged.blockers
    );
    // Key presence is this process's environment, so the keyless case is
    // proven on the frozen binary under a controlled environment instead.
    let route = &judged.document["model_summary"]["routes"][0];
    assert_eq!(route["source"], "run_time");
    assert_eq!(route["task"], "ask");
}

/// B12 · the typed dispatch law over the workflow as the run binds it.
fn dispatch(source: &str, bindings: &[(&str, serde_json::Value)]) -> Result<DispatchBound, E> {
    let bound: std::collections::BTreeMap<String, serde_json::Value> = bindings
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.clone()))
        .collect();
    dispatch_bound(&parsed(source), &plan(), 1, &bound)
}
use RunShapeError as E;
const FAN: &str = "nika: fan\nmodel: deepseek/unpriced-shape-fixture\ntasks:\n  review:\n    for_each: { items: [a, b, c], max_parallel: 2 }\n    retry: { max_attempts: 2 }\n    infer: { prompt: 'x ${{ item }}', max_tokens: 32 }\n";
const INPUT_FAN: &str = "nika: fan\nmodel: deepseek/unpriced-shape-fixture\ninputs:\n  items: { type: { array: string }, default: [a] }\ntasks:\n  review:\n    for_each: { items: '${{ inputs.items }}' }\n    infer: { prompt: 'x ${{ item }}', max_tokens: 32 }\n";

#[test]
fn a_sequential_run_keeps_its_request_bound_and_one_request_in_flight() {
    let structured = ONE.replace("prompt: text", "prompt: text, schema: { type: string }");
    let two = format!(
        "{ONE}  second:\n    after: {{ first: success }}\n    infer: {{ prompt: text, max_tokens: 16 }}\n"
    );
    for source in [ONE.to_owned(), structured, two] {
        let bound = dispatch(&source, &[]).unwrap();
        let historical = request_bound(&parsed(&source), &plan(), 1).unwrap();
        assert_eq!((bound.requests, bound.max_in_flight), (historical, 1));
        assert!(!bound.multiplied() && !bound.authored_retry(), "{source}");
        assert!(bound.lines().is_empty());
        assert!(
            bound
                .tasks
                .iter()
                .all(|t| t.items.is_none() && t.attempts == 1)
        );
    }
}

#[test]
fn a_literal_fan_multiplies_items_attempts_and_schema_reasks() {
    let bound = dispatch(FAN, &[]).unwrap();
    assert_eq!((bound.requests, bound.max_in_flight), (6, 2));
    assert!(bound.multiplied() && bound.authored_retry());
    assert_eq!(
        bound.lines(),
        ["`review`: 3 items × 2 attempts × 1 call = 6 requests, at most 2 at once"]
    );
    let per_schema = 1 + u32::from(nika_verb_infer::DEFAULT_SCHEMA_RETRY_BUDGET);
    let structured = FAN.replace("max_tokens: 32", "max_tokens: 32, schema: { type: string }");
    let bound = dispatch(&structured, &[]).unwrap();
    assert_eq!(bound.requests, 3 * 2 * per_schema);
    assert_eq!(bound.tasks[0].calls_per_attempt, per_schema);
    // Every item may be in flight without a declared width, never more than the fan.
    let open = FAN.replace(", max_parallel: 2", "");
    assert_eq!(dispatch(&open, &[]).unwrap().max_in_flight, 3);
    let wide = FAN.replace("max_parallel: 2", "max_parallel: 10");
    assert_eq!(dispatch(&wide, &[]).unwrap().max_in_flight, 3);
    // The check's own cap law: a declared `max_items` never raises a count.
    let capped = FAN.replace("max_parallel: 2", "max_parallel: 2, max_items: 5");
    assert_eq!(dispatch(&capped, &[]).unwrap().requests, 6);
    let unretried = dispatch(&FAN.replace("    retry: { max_attempts: 2 }\n", ""), &[]).unwrap();
    assert_eq!(unretried.requests, 3);
    assert!(unretried.multiplied() && !unretried.authored_retry());
    let retried = ONE.replace("    infer:", "    retry: { max_attempts: 3 }\n    infer:");
    let retried = dispatch(&retried, &[]).unwrap();
    assert_eq!((retried.requests, retried.max_in_flight), (3, 1));
    assert_eq!(
        retried.lines(),
        ["`first`: 3 attempts × 1 call = 3 requests, at most 1 at once"]
    );
}

#[test]
fn an_operator_bound_collection_counts_before_its_default() {
    assert_eq!(dispatch(INPUT_FAN, &[]).unwrap().requests, 1, "the default");
    let four = serde_json::json!(["a", "b", "c", "d"]);
    let bound = dispatch(INPUT_FAN, &[("items", four)]).unwrap();
    assert_eq!((bound.requests, bound.max_in_flight), (4, 4));
    assert_eq!(bound.tasks[0].items, Some(4));
    let empty = dispatch(INPUT_FAN, &[("items", serde_json::json!([]))]).unwrap();
    assert_eq!(
        empty.requests, 0,
        "a zero total is a value, never an allowance"
    );
    // A bound value above a declared cap is refused before its first item:
    // the cap stays the ceiling, as in the check's own law.
    let capped = INPUT_FAN.replace("}' }", "}', max_items: 2 }");
    let four = serde_json::json!(["a", "b", "c", "d"]);
    assert_eq!(dispatch(&capped, &[("items", four)]).unwrap().requests, 2);
    let required = INPUT_FAN.replace(", default: [a]", ", required: true");
    assert_eq!(dispatch(&required, &[]), Err(E::Cardinality));
    let two = serde_json::json!(["a", "b"]);
    assert_eq!(dispatch(&required, &[("items", two)]).unwrap().requests, 2);
    let from_const = INPUT_FAN
        .replace(
            "inputs:\n  items: { type: { array: string }, default: [a] }",
            "const:\n  items: [a, b]",
        )
        .replace("inputs.items", "const.items");
    assert_eq!(dispatch(&from_const, &[]).unwrap().requests, 2);
}

#[test]
fn a_count_only_the_run_decides_is_refused_as_its_own_reason() {
    let optional = INPUT_FAN.replace(", default: [a]", ", required: false");
    let capped = optional.replace("}' }", "}', max_items: 3 }");
    let navigated = INPUT_FAN.replace("inputs.items }}", "inputs.items.rest }}");
    let upstream = "nika: fan\nmodel: deepseek/unpriced-shape-fixture\npermits:\n  tools: ['nika:read']\n  fs: { read: ['./items.json'] }\ntasks:\n  load:\n    invoke: { tool: 'nika:read', args: { path: './items.json' } }\n  review:\n    with: { list: '${{ tasks.load.output }}' }\n    for_each: { items: '${{ with.list }}', max_items: 3 }\n    infer: { prompt: 'x ${{ item }}', max_tokens: 32 }\n";
    for source in [
        optional.as_str(),
        capped.as_str(),
        navigated.as_str(),
        upstream,
    ] {
        assert_eq!(dispatch(source, &[]), Err(E::Cardinality), "{source}");
    }
    assert_eq!(
        E::Cardinality.to_string(),
        "unknown-cost Run fans only over a literal list or an input/const array; a count only the run decides has no finite bound"
    );
}

#[test]
fn recovery_non_infer_multipliers_and_other_actions_stay_unsupported() {
    let read = "nika: fan\nmodel: deepseek/unpriced-shape-fixture\npermits:\n  tools: ['nika:read']\n  fs: { read: ['./a.txt'] }\ntasks:\n  load:\n    for_each: { items: [a] }\n    invoke: { tool: 'nika:read', args: { path: './a.txt' } }\n";
    let cases = [
        (
            FAN.replace("    retry:", "    on_error: { skip: true }\n    retry:"),
            E::Control,
        ),
        (
            format!("{read}  first:\n    infer: {{ prompt: text, max_tokens: 32 }}\n"),
            E::Control,
        ),
        (
            format!(
                "{ONE}  shape:\n    retry: {{ max_attempts: 2 }}\n    invoke: {{ tool: 'nika:jq', args: {{ data: 1, expr: '.' }} }}\n"
            ),
            E::Control,
        ),
        (
            format!("{ONE}  process:\n    exec: {{ command: ['echo', 'data'] }}\n"),
            E::Action,
        ),
        (
            ONE.replace("    infer:", "    agent:")
                .replace("max_tokens:", "max_tokens_total:"),
            E::Action,
        ),
        (
            format!("{ONE}  nested:\n    invoke: {{ workflow: child.nika }}\n"),
            E::Tool,
        ),
        (
            format!(
                "{FAN}  other:\n    for_each: {{ items: [x] }}\n    infer: {{ prompt: text, max_tokens: 32 }}\n"
            ),
            E::Parallel,
        ),
    ];
    for (source, reason) in cases {
        assert_eq!(dispatch(&source, &[]), Err(reason), "{source}");
    }
    let bound = std::collections::BTreeMap::new();
    assert_eq!(
        dispatch_bound(&parsed(FAN), &plan(), 2, &bound),
        Err(E::Route)
    );
    let only_local = "nika: local\nmodel: deepseek/unpriced-shape-fixture\ntasks:\n  admit:\n    invoke: { tool: 'nika:assert', args: { condition: true } }\n";
    assert_eq!(dispatch(only_local, &[]), Err(E::NoInfer));
}

#[test]
fn every_product_and_sum_is_checked() {
    let huge = FAN.replace("max_attempts: 2", "max_attempts: 4294967295");
    assert_eq!(dispatch(&huge, &[]), Err(E::Overflow));
    let half = ONE.replace(
        "    infer:",
        "    retry: { max_attempts: 2147483648 }\n    infer:",
    );
    assert_eq!(dispatch(&half, &[]).unwrap().requests, 2_147_483_648);
    let twice = format!(
        "{half}  second:\n    after: {{ first: terminal }}\n    retry: {{ max_attempts: 2147483648 }}\n    infer: {{ prompt: text, max_tokens: 16 }}\n"
    );
    assert_eq!(dispatch(&twice, &[]), Err(E::Overflow));
}

#[test]
fn readiness_names_the_same_typed_bound_and_zero_needs_no_choice() {
    let config = nika_providers::ProvidersConfig::new();
    let fan = readiness(&parsed(FAN), &plan(), &config).unwrap();
    assert_eq!(
        fan,
        "USD cost is unknown: Run requires a fresh finite-call choice for at most 6 requests, at most 2 at once, including schema re-asks and authored retries; Check has not admitted spend or effects"
    );
    let one = readiness(&parsed(ONE), &plan(), &config).unwrap();
    assert_eq!(
        one,
        "USD cost is unknown: Run requires a fresh finite-call choice for at most 1 requests, including schema re-asks; Check has not admitted spend or effects"
    );
    let empty = FAN.replace("[a, b, c]", "[]");
    assert_eq!(dispatch(&empty, &[]).unwrap().requests, 0);
    assert_eq!(readiness(&parsed(&empty), &plan(), &config), None);
}

/// B12 r5 · the bound's owner configures a fresh review with its whole law:
/// the question and the confirmed choice carry its total, width, breakdown
/// and retry law; a sequential bound keeps the historical review, and a zero
/// total configures nothing.
#[test]
fn the_bound_configures_a_review_with_its_whole_law() {
    use nika_providers::admission::{CostHostEvidence, CostReview, CostRoute};
    let route = CostRoute::observe(MODEL, nika_providers::ProvidersConfig::new()).unwrap();
    let fresh = || {
        let local = CostHostEvidence::unmanaged_interactive_local();
        CostReview::new(
            "candidate".into(),
            "run".into(),
            route.clone(),
            local,
            None,
            None,
        )
        .unwrap()
    };
    let bound = dispatch(FAN, &[]).unwrap();
    let review = bound.review(fresh()).unwrap();
    assert_eq!((review.max_requests(), review.max_in_flight()), (6, 2));
    let question = review.question();
    for line in bound.lines() {
        assert!(question.contains(&line), "{question}");
    }
    assert!(
        question.contains("Task retries authored in the workflow"),
        "{question}"
    );
    let account = review.confirm("candidate", &route).unwrap();
    let choice = serde_json::to_value(account.snapshot().unwrap().unknown_cost).unwrap();
    assert_eq!(
        (
            &choice["max_requests"],
            &choice["max_in_flight"],
            &choice["authored_retry"]
        ),
        (
            &serde_json::json!(6),
            &serde_json::json!(2),
            &serde_json::json!(true)
        )
    );
    let one = dispatch(ONE, &[]).unwrap().review(fresh()).unwrap();
    assert_eq!(one.question(), fresh().for_run(1).unwrap().question());
    let empty = dispatch(&FAN.replace("[a, b, c]", "[]"), &[]).unwrap();
    assert!(
        empty.review(fresh()).is_err(),
        "zero work buys no allowance"
    );
}

/// B12 r5 · one bound law: `request_bound` is `dispatch_bound` at declared
/// defaults, refused as `Control` exactly when the bound multiplies. A single
/// authored attempt is the sequential Run of one request with no retry law,
/// and a fan whose count only the run decides keeps its own reason.
#[test]
fn request_bound_is_the_sequential_projection_of_the_one_law() {
    let once = ONE.replace("    infer:", "    retry: { max_attempts: 1 }\n    infer:");
    let bound = dispatch(&once, &[]).unwrap();
    assert!(!bound.multiplied() && !bound.authored_retry());
    assert_eq!(
        request_bound(&parsed(&once), &plan(), 1),
        Ok(bound.requests())
    );
    for multiplied in [
        FAN.to_owned(),
        FAN.replace("[a, b, c]", "[]"),
        FAN.replace("    retry: { max_attempts: 2 }\n", ""),
        ONE.replace("    infer:", "    retry: { max_attempts: 2 }\n    infer:"),
    ] {
        assert!(dispatch(&multiplied, &[]).unwrap().multiplied());
        assert_eq!(
            request_bound(&parsed(&multiplied), &plan(), 1),
            Err(E::Control),
            "{multiplied}"
        );
    }
    let required = INPUT_FAN.replace(", default: [a]", ", required: true");
    assert_eq!(
        request_bound(&parsed(&required), &plan(), 1),
        Err(E::Cardinality)
    );
}
