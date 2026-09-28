// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used)]
use super::*;
const MODEL: &str = "openai/gpt-oss-120b";
const SOURCE: &str = "nika: bounded\nmodel: openai/gpt-oss-120b\ntasks:\n  draft:\n    infer: { prompt: text, max_tokens: 32, schema: { type: string } }\n";

fn plan() -> ExecutionAccessPlan {
    use nika_providers::probe::{ExecutionLocus, ProviderProbe, ProviderReadiness};
    nika_providers::resolve_execution_plan(
        &[nika_providers::ModelNeed::new(MODEL, true, false)],
        &[ProviderProbe::new(
            "openai",
            true,
            true,
            "OPENAI_API_KEY",
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
            "https://api.scaleway.ai/example-project/v1",
        )],
        Some("api"),
    )
}

fn parsed(source: &str) -> RawWorkflow {
    nika_schema::parse(
        source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .unwrap()
}

#[test]
fn structured_unknown_route_is_clean_but_needs_run_choice() {
    let config = ProvidersConfig::new()
        .with_base_url("openai", "https://api.scaleway.ai/example-project/v1");
    let wf = parsed(SOURCE);
    assert!(nika_check::check(&wf).is_clean());
    let blocker = readiness_with_config(&wf, &plan(), &config).unwrap();
    let count = 3; // Production default: initial call plus two schema re-asks.
    assert!(
        blocker.contains(&format!("at most {count} requests")),
        "{blocker}"
    );
    assert!(blocker.contains("Check has not admitted spend or effects"));
    let layers = nika_display::check_render::VerdictLayers::new(
        true,
        Some(true),
        Vec::new(),
        true,
        vec![blocker],
    );
    assert_eq!(layers.run_ready(), Some(false));
}

#[test]
fn unbounded_unknown_and_wrong_endpoint_stay_unready() {
    let wf = parsed(&SOURCE.replace(", max_tokens: 32", ""));
    let config = ProvidersConfig::new()
        .with_base_url("openai", "https://api.scaleway.ai/example-project/v1");
    let blocker = readiness_with_config(&wf, &plan(), &config).unwrap();
    assert!(blocker.contains("cannot obtain a bounded choice"));
    let config = ProvidersConfig::new().with_base_url("openai", "http://localhost:12345/v1");
    assert!(readiness_with_config(&parsed(SOURCE), &plan(), &config).is_some());
}

#[test]
fn no_model_plan_has_no_monetary_blocker() {
    let plan = nika_providers::resolve_execution_plan(&[], &[], None);
    let wf = parsed(
        "nika: local\npermits: { tools: ['nika:assert'] }\ntasks:\n  ok:\n    invoke: { tool: 'nika:assert', args: { condition: true } }\n",
    );
    assert_eq!(
        readiness_with_config(&wf, &plan, &ProvidersConfig::new()),
        None
    );
}

/// C2 · Check mirrors the Run: bounded text on an exact declared-free route
/// is run ready; the same route with vision is not, and says which task.
#[test]
fn a_declared_free_route_is_ready_for_text_and_names_an_unsupported_shape() {
    use nika_providers::probe::{ExecutionLocus, ProviderProbe, ProviderReadiness};
    let free = "openrouter/qwen/qwen3.8-27b:free";
    let plan = nika_providers::resolve_execution_plan(
        &[nika_providers::ModelNeed::new(free, true, false)],
        &[ProviderProbe::new(
            "openrouter",
            true,
            true,
            "OPENROUTER_API_KEY",
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
            "https://openrouter.ai/api/v1/chat/completions",
        )],
        Some("api"),
    );
    let text = format!(
        "nika: free\nmodel: {free}\npermits: {{}}\ntasks:\n  draft:\n    infer: {{ prompt: text, max_tokens: 64 }}\n"
    );
    let config = ProvidersConfig::new();
    assert_eq!(readiness_with_config(&parsed(&text), &plan, &config), None);
    let vision = text.replace(
        "max_tokens: 64",
        "max_tokens: 64, vision: [{ source: file, path: './image.png' }]",
    );
    let blocker = readiness_with_config(&parsed(&vision), &plan, &config).unwrap();
    assert!(blocker.contains("task `draft`"), "{blocker}");
    assert!(blocker.contains("with vision"), "{blocker}");
}
