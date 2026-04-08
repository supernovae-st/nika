// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika bench` command handler — provider benchmarking.
//!
//! Runs a workflow N iterations across M providers, measuring TTFT, tok/s,
//! cost, and optional LLM-as-judge quality evaluation.

use std::path::Path;

use colored::Colorize;
use nika_engine::ast::parse_analyzed_with_includes;
use nika_engine::ast::schema_validator::WorkflowSchemaValidator;
use nika_engine::error::NikaError;
use nika_engine::runtime::Runner;

use crate::discover::resolve_workflow_path;

/// Run provider benchmarks on a workflow.
#[allow(clippy::too_many_arguments)]
pub async fn run_bench(
    file: &str,
    providers: &[String],
    iterations: usize,
    show_profile: bool,
    json_output: bool,
    eval: bool,
    eval_model: &str,
    _skip_confirm: bool,
    quiet: bool,
) -> Result<(), NikaError> {
    use nika_engine::display::bench_cache;
    use nika_engine::display::renderer::RunStats;
    use nika_engine::event::EventLog;
    use std::time::Instant;

    if iterations == 0 {
        return Err(NikaError::ValidationError {
            reason: "Iterations must be >= 1".to_string(),
        });
    }
    if providers.is_empty() {
        return Err(NikaError::ValidationError {
            reason: "At least one provider is required (--providers)".to_string(),
        });
    }

    // Bootstrap secrets
    let _ = nika_engine::secrets::load_from_daemon_or_fallback().await;

    let resolved_path = resolve_workflow_path(file).await?;
    let yaml = tokio::fs::read_to_string(&resolved_path).await?;

    let validator = WorkflowSchemaValidator::new()?;
    validator.validate_yaml(&yaml)?;

    let base_path = resolved_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));

    let base_workflow = parse_analyzed_with_includes(&yaml, base_path)?;
    let task_count = base_workflow.tasks.len();
    let wf_hash = bench_cache::workflow_hash(&yaml);

    // Print header
    if !quiet && !json_output {
        nika_engine::display::print_bench_header(
            resolved_path
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("workflow"),
            task_count,
            iterations,
            providers,
        );
    }

    let bench_start = Instant::now();
    let mut all_results: Vec<nika_engine::display::BenchProviderResult> = Vec::new();

    for provider_name in providers {
        let mut iteration_stats: Vec<RunStats> = Vec::new();
        let mut final_output: Option<String> = None;

        for i in 0..iterations {
            if !quiet && !json_output {
                eprint!(
                    "  {} {} [{}/{}]...",
                    nika_engine::display::icons::provider(),
                    provider_name,
                    i + 1,
                    iterations,
                );
            }

            let mut workflow = base_workflow.clone();
            workflow.provider = Some(provider_name.as_str().into());

            let event_log = EventLog::new();
            let mut runner = Runner::with_event_log(workflow, event_log.clone())?
                .with_base_path(base_path.to_path_buf())
                .quiet();

            let iter_start = Instant::now();
            match runner.run().await {
                Ok(output) => {
                    let duration_ms = iter_start.elapsed().as_millis() as u64;
                    let mut stats = RunStats::default();
                    event_log.with_events(|events| {
                        for event in events {
                            stats.apply_event(event);
                        }
                    });

                    if !quiet && !json_output {
                        eprintln!(
                            " {} {:.1}s",
                            "\u{2713}".green(),
                            duration_ms as f64 / 1000.0,
                        );
                    }

                    if !output.is_empty() {
                        final_output = Some(output);
                    }
                    iteration_stats.push(stats);
                }
                Err(e) => {
                    if !quiet && !json_output {
                        eprintln!(" {} {}", "\u{2717}".red(), e);
                    }
                    // Continue with remaining iterations
                }
            }
        }

        if iteration_stats.is_empty() {
            if !quiet && !json_output {
                eprintln!(
                    "  {} All iterations failed for {}",
                    "\u{26A0}".yellow(),
                    provider_name,
                );
            }
            continue;
        }

        // Aggregate across iterations
        let mut result = aggregate_bench_stats(provider_name, &iteration_stats, task_count);

        // Quality evaluation via LLM-as-judge
        if eval {
            if let Some(ref output_text) = final_output {
                if !quiet && !json_output {
                    eprint!("  {} evaluating quality...", "\u{2727}".cyan());
                }
                match evaluate_quality(output_text, eval_model).await {
                    Ok(scores) => {
                        let overall = scores.iter().map(|s| s.score).sum::<f64>()
                            / scores.len().max(1) as f64;
                        result.quality_scores = scores;
                        result.quality_overall = Some(overall);
                        if !quiet && !json_output {
                            eprintln!(" {} {:.0}%", "\u{2713}".green(), overall * 100.0);
                        }
                    }
                    Err(e) => {
                        if !quiet && !json_output {
                            eprintln!(" {} {}", "\u{26A0}".yellow(), e);
                        }
                    }
                }
            }
        }

        all_results.push(result);
    }

    let bench_duration = bench_start.elapsed();

    if json_output {
        // JSON export
        let json_results: Vec<serde_json::Value> = all_results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "provider": r.provider,
                    "model": r.model,
                    "ttft_p50_ms": r.ttft.p50,
                    "ttft_p90_ms": r.ttft.p90,
                    "ttft_p99_ms": r.ttft.p99,
                    "tokens_per_sec": r.tokens_per_sec,
                    "total_secs": r.total_secs,
                    "cost_per_run": r.cost_per_run,
                    "input_tokens": r.input_tokens,
                    "output_tokens": r.output_tokens,
                    "cache_tokens": r.cache_tokens,
                    "quality_overall": r.quality_overall,
                    "quality_scores": r.quality_scores.iter().map(|s| {
                        serde_json::json!({"criterion": s.criterion, "score": s.score})
                    }).collect::<Vec<_>>(),
                })
            })
            .collect();
        let output = serde_json::json!({
            "workflow": file,
            "workflow_hash": wf_hash,
            "iterations": iterations,
            "bench_duration_secs": bench_duration.as_secs_f64(),
            "results": json_results,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_default()
        );
    } else if !quiet {
        // Rich display
        nika_engine::display::print_speed_section(&all_results);
        nika_engine::display::print_cost_section(&all_results);

        if show_profile {
            nika_engine::display::print_profile_section(&all_results);
        }

        nika_engine::display::print_quality_section(&all_results);
        nika_engine::display::print_bench_summary(
            &all_results,
            bench_duration,
            task_count,
            iterations,
        );
    }

    // Persist to bench cache
    let cache_entry = bench_cache::BenchCacheEntry {
        workflow_hash: wf_hash.clone(),
        timestamp: {
            use std::time::SystemTime;
            let d = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default();
            format!("{}Z", d.as_secs())
        },
        iterations,
        results: all_results
            .iter()
            .map(|r| {
                (
                    r.provider.clone(),
                    bench_cache::CachedProviderResult {
                        model: r.model.clone(),
                        avg_duration_ms: (r.total_secs * 1000.0) as u64,
                        avg_cost_usd: r.cost_per_run,
                        avg_quality: r.quality_overall,
                        ttft_p50_ms: r.ttft.p50,
                        ttft_p90_ms: r.ttft.p90,
                        avg_input_tokens: r.input_tokens,
                        avg_output_tokens: r.output_tokens,
                        avg_tokens_per_sec: r.tokens_per_sec,
                    },
                )
            })
            .collect(),
    };
    if let Err(e) = bench_cache::write_cache(base_path, &cache_entry) {
        if !quiet {
            eprintln!("  {} Cache write failed: {}", "\u{26A0}".yellow(), e);
        }
    }

    Ok(())
}

/// Evaluate output quality using an LLM-as-judge.
async fn evaluate_quality(
    output: &str,
    eval_model: &str,
) -> Result<Vec<nika_engine::display::QualityScore>, NikaError> {
    use nika_engine::ast::{InferParams, ResponseFormat, TaskAction};
    use nika_engine::binding::ResolvedBindings;
    use nika_engine::event::EventLog;
    use nika_engine::runtime::TaskExecutor;
    use nika_engine::store::RunContext;
    use std::sync::Arc;

    let detected = crate::verbs::detect_provider();
    let eval_provider = if eval_model.contains("claude") || eval_model.contains("haiku") {
        "anthropic"
    } else if eval_model.starts_with("gpt") || eval_model.starts_with("o1") {
        "openai"
    } else {
        detected.as_deref().unwrap_or("anthropic")
    };

    let truncated = &output[..output.len().min(4000)];
    let judge_prompt = format!(
        "You are a quality evaluator. Score the following AI-generated output on 3 criteria.\n\
         Each score is 0.0 to 1.0 (1.0 = perfect).\n\n\
         OUTPUT TO EVALUATE:\n---\n{truncated}\n---\n\n\
         Respond with ONLY valid JSON (no markdown, no explanation):\n\
         {{\"accuracy\": 0.0, \"relevance\": 0.0, \"completeness\": 0.0}}"
    );

    let infer = InferParams {
        prompt: judge_prompt,
        response_format: Some(ResponseFormat::Json),
        max_tokens: Some(200),
        temperature: Some(0.0),
        ..Default::default()
    };
    let action = TaskAction::Infer { infer };
    let task_id: Arc<str> = Arc::from("bench_eval");

    let event_log = EventLog::new();
    let executor = TaskExecutor::new(eval_provider, Some(eval_model), None, event_log)?;
    let bindings = ResolvedBindings::new();
    let datastore = RunContext::new(nika_engine::trust::InvocationSource::Cli);

    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await?;

    let parsed: serde_json::Value =
        serde_json::from_str(result.trim()).map_err(|e| NikaError::ParseError {
            details: format!("Judge response is not valid JSON: {e} — raw: {result}"),
        })?;

    let mut scores = Vec::new();
    for criterion in &["accuracy", "relevance", "completeness"] {
        let score = parsed
            .get(criterion)
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);
        scores.push(nika_engine::display::QualityScore {
            criterion: criterion[..1].to_uppercase() + &criterion[1..],
            score,
        });
    }

    Ok(scores)
}

/// Aggregate RunStats from multiple iterations into a single BenchProviderResult.
pub fn aggregate_bench_stats(
    provider_name: &str,
    iteration_stats: &[nika_engine::display::renderer::RunStats],
    _task_count: usize,
) -> nika_engine::display::BenchProviderResult {
    let n = iteration_stats.len() as f64;

    // Aggregate TTFT values across all iterations
    let mut all_ttft: Vec<u64> = iteration_stats
        .iter()
        .flat_map(|s| s.ttft_values.iter().copied())
        .collect();
    all_ttft.sort_unstable();

    let ttft = nika_engine::display::Percentiles {
        p50: percentile(&all_ttft, 50),
        p90: percentile(&all_ttft, 90),
        p99: percentile(&all_ttft, 99),
    };

    // Average tokens
    let avg_input: u64 = (iteration_stats
        .iter()
        .map(|s| s.total_input_tokens)
        .sum::<u64>() as f64
        / n) as u64;
    let avg_output: u64 = (iteration_stats
        .iter()
        .map(|s| s.total_output_tokens)
        .sum::<u64>() as f64
        / n) as u64;
    let avg_cache: u64 = (iteration_stats
        .iter()
        .map(|s| s.total_cache_tokens)
        .sum::<u64>() as f64
        / n) as u64;

    // Average cost
    let avg_cost: f64 = iteration_stats.iter().map(|s| s.total_cost).sum::<f64>() / n;

    // Total duration from task_timeline (sum of all task durations)
    let avg_duration_ms: f64 = iteration_stats
        .iter()
        .map(|s| {
            s.task_timeline
                .iter()
                .map(|(_, _, start, dur)| start + dur)
                .max()
                .unwrap_or(0)
        })
        .sum::<u64>() as f64
        / n;
    let total_secs = avg_duration_ms / 1000.0;

    // Tokens per second
    let tokens_per_sec = if total_secs > 0.0 {
        avg_output as f64 / total_secs
    } else {
        0.0
    };

    // Model name from the first iteration's provider_calls
    let model = iteration_stats
        .iter()
        .flat_map(|s| s.provider_calls.first())
        .map(|pc| pc.model.clone())
        .next()
        .unwrap_or_else(|| "unknown".to_string());

    // Task timeline from the median iteration (by total duration)
    let mut durations: Vec<(usize, u64)> = iteration_stats
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let d = s
                .task_timeline
                .iter()
                .map(|(_, _, start, dur)| start + dur)
                .max()
                .unwrap_or(0);
            (i, d)
        })
        .collect();
    durations.sort_by_key(|(_, d)| *d);
    let median_idx = durations
        .get(durations.len() / 2)
        .map(|(i, _)| *i)
        .unwrap_or(0);
    let median_stats = &iteration_stats[median_idx];

    let task_timeline: Vec<nika_engine::display::BenchTaskTiming> = {
        let max_dur = median_stats
            .task_timeline
            .iter()
            .map(|(_, _, _, dur)| *dur)
            .max()
            .unwrap_or(1);
        median_stats
            .task_timeline
            .iter()
            .map(
                |(task_id, verb, start, dur)| nika_engine::display::BenchTaskTiming {
                    task_id: task_id.clone(),
                    verb: verb.clone(),
                    start_ms: *start,
                    duration_ms: *dur,
                    is_bottleneck: *dur == max_dur,
                },
            )
            .collect()
    };

    nika_engine::display::BenchProviderResult {
        provider: provider_name.to_string(),
        model,
        ttft,
        tokens_per_sec,
        total_secs,
        cost_per_run: avg_cost,
        input_tokens: avg_input,
        output_tokens: avg_output,
        cache_tokens: avg_cache,
        task_timeline,
        quality_scores: vec![],
        quality_overall: None,
    }
}

/// Compute the p-th percentile from a sorted slice.
pub fn percentile(sorted: &[u64], p: u32) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((p as f64 / 100.0) * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_empty_is_zero() {
        assert_eq!(percentile(&[], 50), 0);
    }

    #[test]
    fn percentile_single_element() {
        assert_eq!(percentile(&[42], 50), 42);
        assert_eq!(percentile(&[42], 99), 42);
    }

    #[test]
    fn percentile_p50_of_sorted() {
        let sorted: Vec<u64> = (1..=10).collect();
        // p50 of [1..=10]: idx = round(0.5 * 9) = 5 → sorted[5] = 6
        assert_eq!(percentile(&sorted, 50), 6);
    }

    #[test]
    fn percentile_p99_of_sorted() {
        let sorted: Vec<u64> = (1..=100).collect();
        assert_eq!(percentile(&sorted, 99), 99);
    }

    #[test]
    fn percentile_p0_is_first() {
        assert_eq!(percentile(&[10, 20, 30], 0), 10);
    }

    #[test]
    fn percentile_p100_is_last() {
        assert_eq!(percentile(&[10, 20, 30], 100), 30);
    }
}
