// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Runtime resolution for `Templatable<T>` fields.
//!
//! When a workflow field like `temperature` contains a template expression
//! (e.g. `{{inputs.temperature}}`), it is stored as `Templatable::Template(String)`
//! through the AST pipeline. This module resolves those templates at execution time
//! using the available `ResolvedBindings` and `RunContext`.
//!
//! ## Coverage
//!
//! - **Action fields**: `resolve_action_templates()` — verb-specific (infer, exec, fetch, invoke, agent)
//! - **Task-level fields**: `resolve_task_u32()`, `resolve_task_bool()` — concurrency, fail_fast, context_budget
//! - **Retry fields**: `resolve_retry()` — max_attempts, delay_ms, backoff
//! - **Standalone helpers**: public `resolve_*` functions for use in runner/task_dispatch

use nika_core::ast::analyzed::{AnalyzedRetry, AnalyzedTaskAction};
use nika_core::ast::templatable::Templatable;

use crate::binding::{template_resolve, ResolvedBindings};
use crate::error::NikaError;
use crate::store::RunContext;

/// Resolve all `Templatable::Template` fields in an `AnalyzedTaskAction`.
///
/// Returns a new action with all templates replaced by concrete values.
/// Templates are resolved using the same mechanism as prompt/system templates.
///
/// # Errors
///
/// Returns `NikaError::BindingTypeMismatch` (NIKA-043) if a template resolves to a value
/// that cannot be parsed as the expected type (e.g. "hello" for a temperature field).
pub fn resolve_action_templates(
    action: &AnalyzedTaskAction,
    bindings: &ResolvedBindings,
    ctx: &RunContext,
) -> Result<AnalyzedTaskAction, NikaError> {
    match action {
        AnalyzedTaskAction::Infer(infer) => {
            let mut resolved = infer.clone();
            resolved.temperature =
                resolve_opt_f64_range(&infer.temperature, bindings, ctx, "temperature", 0.0, 2.0)?;
            resolved.max_tokens =
                resolve_opt_u32_min(&infer.max_tokens, bindings, ctx, "max_tokens", 1)?;
            resolved.extended_thinking =
                resolve_opt_bool(&infer.extended_thinking, bindings, ctx, "extended_thinking")?;
            resolved.thinking_budget =
                resolve_opt_u32_min(&infer.thinking_budget, bindings, ctx, "thinking_budget", 1)?;
            Ok(AnalyzedTaskAction::Infer(resolved))
        }
        AnalyzedTaskAction::Exec(exec) => {
            let mut resolved = exec.clone();
            resolved.shell = resolve_bool(&exec.shell, bindings, ctx, "shell")?;
            resolved.timeout_ms =
                resolve_opt_u64_min(&exec.timeout_ms, bindings, ctx, "timeout", 1)?;
            resolved.max_stdout = resolve_opt_u64(&exec.max_stdout, bindings, ctx, "max_stdout")?;
            Ok(AnalyzedTaskAction::Exec(resolved))
        }
        AnalyzedTaskAction::Fetch(fetch) => {
            let mut resolved = fetch.clone();
            resolved.timeout_ms =
                resolve_opt_u64_min(&fetch.timeout_ms, bindings, ctx, "timeout", 1)?;
            resolved.follow_redirects =
                resolve_bool(&fetch.follow_redirects, bindings, ctx, "follow_redirects")?;
            resolved.session = resolve_bool(&fetch.session, bindings, ctx, "session")?;
            resolved.cache = resolve_bool(&fetch.cache, bindings, ctx, "cache")?;
            Ok(AnalyzedTaskAction::Fetch(resolved))
        }
        AnalyzedTaskAction::Invoke(invoke) => {
            let mut resolved = invoke.clone();
            resolved.timeout_ms =
                resolve_opt_u64_min(&invoke.timeout_ms, bindings, ctx, "timeout", 1)?;
            Ok(AnalyzedTaskAction::Invoke(resolved))
        }
        AnalyzedTaskAction::Agent(agent) => {
            let mut resolved = agent.clone();
            resolved.temperature =
                resolve_opt_f64_range(&agent.temperature, bindings, ctx, "temperature", 0.0, 2.0)?;
            resolved.max_tokens =
                resolve_opt_u32_min(&agent.max_tokens, bindings, ctx, "max_tokens", 1)?;
            resolved.max_turns =
                resolve_opt_u32_min(&agent.max_turns, bindings, ctx, "max_turns", 1)?;
            resolved.token_budget =
                resolve_opt_u32_min(&agent.token_budget, bindings, ctx, "token_budget", 1)?;
            resolved.extended_thinking =
                resolve_opt_bool(&agent.extended_thinking, bindings, ctx, "extended_thinking")?;
            resolved.thinking_budget =
                resolve_opt_u32_min(&agent.thinking_budget, bindings, ctx, "thinking_budget", 1)?;
            resolved.depth_limit =
                resolve_opt_u32_min(&agent.depth_limit, bindings, ctx, "depth_limit", 1)?;
            Ok(AnalyzedTaskAction::Agent(resolved))
        }
    }
}

/// Resolve an `AnalyzedRetry` config (max_attempts, delay_ms, backoff).
pub fn resolve_retry(
    retry: &AnalyzedRetry,
    bindings: &ResolvedBindings,
    ctx: &RunContext,
) -> Result<AnalyzedRetry, NikaError> {
    Ok(AnalyzedRetry {
        max_attempts: resolve_templatable_u32(
            &retry.max_attempts,
            bindings,
            ctx,
            "retry.max_attempts",
        )?,
        delay_ms: resolve_templatable_u64(&retry.delay_ms, bindings, ctx, "retry.delay_ms")?,
        backoff: resolve_opt_f64(&retry.backoff, bindings, ctx, "retry.backoff")?,
        span: retry.span,
    })
}

// ============================================================================
// Public standalone helpers (for task_dispatch / runner)
// ============================================================================

/// Resolve a standalone `Option<Templatable<u32>>` (e.g. concurrency, context_budget).
pub fn resolve_task_u32(
    field: &Option<Templatable<u32>>,
    bindings: &ResolvedBindings,
    ctx: &RunContext,
    name: &str,
) -> Result<Option<u32>, NikaError> {
    match field {
        None => Ok(None),
        Some(Templatable::Value(v)) => Ok(Some(*v)),
        Some(Templatable::Template(tpl)) => {
            let resolved = resolve_template_str(tpl, bindings, ctx)?;
            let val = parse_u32(&resolved, name)?;
            Ok(Some(val))
        }
    }
}

/// Resolve a standalone `Option<Templatable<bool>>` (e.g. fail_fast).
pub fn resolve_task_bool(
    field: &Option<Templatable<bool>>,
    bindings: &ResolvedBindings,
    ctx: &RunContext,
    name: &str,
) -> Result<Option<bool>, NikaError> {
    match field {
        None => Ok(None),
        Some(Templatable::Value(v)) => Ok(Some(*v)),
        Some(Templatable::Template(tpl)) => {
            let resolved = resolve_template_str(tpl, bindings, ctx)?;
            let val = parse_bool_value(resolved.trim()).ok_or_else(|| {
                NikaError::BindingTypeMismatch {
                    path: name.to_string(),
                    expected: "boolean".to_string(),
                    actual: resolved.clone(),
                }
            })?;
            Ok(Some(val))
        }
    }
}

/// Resolve a non-optional `Templatable<bool>` (e.g. for_each.fail_fast).
pub fn resolve_required_bool(
    field: &Templatable<bool>,
    bindings: &ResolvedBindings,
    ctx: &RunContext,
    name: &str,
    default: bool,
) -> Result<bool, NikaError> {
    match field {
        Templatable::Value(v) => Ok(*v),
        Templatable::Template(tpl) => {
            let resolved = resolve_template_str(tpl, bindings, ctx)?;
            let trimmed = resolved.trim();
            if trimmed.is_empty() {
                return Ok(default);
            }
            parse_bool_value(trimmed).ok_or_else(|| NikaError::BindingTypeMismatch {
                path: name.to_string(),
                expected: "boolean".to_string(),
                actual: resolved.clone(),
            })
        }
    }
}

/// Resolve a non-optional `Templatable<u64>` (e.g. max_duration_secs).
pub fn resolve_required_u64(
    field: &Templatable<u64>,
    bindings: &ResolvedBindings,
    ctx: &RunContext,
    name: &str,
    default: u64,
) -> Result<u64, NikaError> {
    match field {
        Templatable::Value(v) => Ok(*v),
        Templatable::Template(tpl) => {
            let resolved = resolve_template_str(tpl, bindings, ctx)?;
            let trimmed = resolved.trim();
            if trimmed.is_empty() {
                return Ok(default);
            }
            trimmed
                .parse::<u64>()
                .map_err(|_| NikaError::BindingTypeMismatch {
                    path: name.to_string(),
                    expected: "positive integer".to_string(),
                    actual: resolved.clone(),
                })
        }
    }
}

// ============================================================================
// Internal helpers
// ============================================================================

fn resolve_template_str(
    template: &str,
    bindings: &ResolvedBindings,
    ctx: &RunContext,
) -> Result<String, NikaError> {
    let resolved = template_resolve(template, bindings, ctx)?;
    Ok(resolved.into_owned())
}

/// Resolve + range-validate an f64 field.
fn resolve_opt_f64_range(
    field: &Option<Templatable<f64>>,
    bindings: &ResolvedBindings,
    ctx: &RunContext,
    name: &str,
    min: f64,
    max: f64,
) -> Result<Option<Templatable<f64>>, NikaError> {
    match field {
        None => Ok(None),
        Some(Templatable::Value(v)) => Ok(Some(Templatable::Value(*v))),
        Some(Templatable::Template(tpl)) => {
            let resolved = resolve_template_str(tpl, bindings, ctx)?;
            if resolved.trim().is_empty() {
                return Ok(None); // missing input = field absent, use provider default
            }
            let val = parse_f64(&resolved, name)?;
            if val < min || val > max {
                return Err(NikaError::BindingTypeMismatch {
                    path: name.to_string(),
                    expected: format!("number in [{min}, {max}]"),
                    actual: format!("{val}"),
                });
            }
            Ok(Some(Templatable::Value(val)))
        }
    }
}

fn resolve_opt_f64(
    field: &Option<Templatable<f64>>,
    bindings: &ResolvedBindings,
    ctx: &RunContext,
    name: &str,
) -> Result<Option<Templatable<f64>>, NikaError> {
    match field {
        None => Ok(None),
        Some(Templatable::Value(v)) => Ok(Some(Templatable::Value(*v))),
        Some(Templatable::Template(tpl)) => {
            let resolved = resolve_template_str(tpl, bindings, ctx)?;
            if resolved.trim().is_empty() {
                return Ok(None);
            }
            let val = parse_f64(&resolved, name)?;
            Ok(Some(Templatable::Value(val)))
        }
    }
}

fn resolve_opt_u32_min(
    field: &Option<Templatable<u32>>,
    bindings: &ResolvedBindings,
    ctx: &RunContext,
    name: &str,
    min: u32,
) -> Result<Option<Templatable<u32>>, NikaError> {
    match field {
        None => Ok(None),
        Some(Templatable::Value(v)) => Ok(Some(Templatable::Value(*v))),
        Some(Templatable::Template(tpl)) => {
            let resolved = resolve_template_str(tpl, bindings, ctx)?;
            let val = parse_u32(&resolved, name)?;
            if val < min {
                return Err(NikaError::BindingTypeMismatch {
                    path: name.to_string(),
                    expected: format!("integer >= {min}"),
                    actual: format!("{val}"),
                });
            }
            Ok(Some(Templatable::Value(val)))
        }
    }
}

fn resolve_opt_u64_min(
    field: &Option<Templatable<u64>>,
    bindings: &ResolvedBindings,
    ctx: &RunContext,
    name: &str,
    min: u64,
) -> Result<Option<Templatable<u64>>, NikaError> {
    match field {
        None => Ok(None),
        Some(Templatable::Value(v)) => Ok(Some(Templatable::Value(*v))),
        Some(Templatable::Template(tpl)) => {
            let resolved = resolve_template_str(tpl, bindings, ctx)?;
            let val = parse_u64(&resolved, name)?;
            if val < min {
                return Err(NikaError::BindingTypeMismatch {
                    path: name.to_string(),
                    expected: format!("integer >= {min}"),
                    actual: format!("{val}"),
                });
            }
            Ok(Some(Templatable::Value(val)))
        }
    }
}

fn resolve_opt_u64(
    field: &Option<Templatable<u64>>,
    bindings: &ResolvedBindings,
    ctx: &RunContext,
    name: &str,
) -> Result<Option<Templatable<u64>>, NikaError> {
    match field {
        None => Ok(None),
        Some(Templatable::Value(v)) => Ok(Some(Templatable::Value(*v))),
        Some(Templatable::Template(tpl)) => {
            let resolved = resolve_template_str(tpl, bindings, ctx)?;
            let val = parse_u64(&resolved, name)?;
            Ok(Some(Templatable::Value(val)))
        }
    }
}

fn resolve_opt_bool(
    field: &Option<Templatable<bool>>,
    bindings: &ResolvedBindings,
    ctx: &RunContext,
    name: &str,
) -> Result<Option<Templatable<bool>>, NikaError> {
    match field {
        None => Ok(None),
        Some(Templatable::Value(v)) => Ok(Some(Templatable::Value(*v))),
        Some(Templatable::Template(tpl)) => {
            let resolved = resolve_template_str(tpl, bindings, ctx)?;
            let val = parse_bool_value(resolved.trim()).ok_or_else(|| {
                NikaError::BindingTypeMismatch {
                    path: name.to_string(),
                    expected: "boolean".to_string(),
                    actual: resolved.clone(),
                }
            })?;
            Ok(Some(Templatable::Value(val)))
        }
    }
}

fn resolve_bool(
    field: &Templatable<bool>,
    bindings: &ResolvedBindings,
    ctx: &RunContext,
    name: &str,
) -> Result<Templatable<bool>, NikaError> {
    match field {
        Templatable::Value(v) => Ok(Templatable::Value(*v)),
        Templatable::Template(tpl) => {
            let resolved = resolve_template_str(tpl, bindings, ctx)?;
            let val = parse_bool_value(resolved.trim()).ok_or_else(|| {
                NikaError::BindingTypeMismatch {
                    path: name.to_string(),
                    expected: "boolean".to_string(),
                    actual: resolved.clone(),
                }
            })?;
            Ok(Templatable::Value(val))
        }
    }
}

fn resolve_templatable_u32(
    field: &Templatable<u32>,
    bindings: &ResolvedBindings,
    ctx: &RunContext,
    name: &str,
) -> Result<Templatable<u32>, NikaError> {
    match field {
        Templatable::Value(v) => Ok(Templatable::Value(*v)),
        Templatable::Template(tpl) => {
            let resolved = resolve_template_str(tpl, bindings, ctx)?;
            let val = parse_u32(&resolved, name)?;
            Ok(Templatable::Value(val))
        }
    }
}

fn resolve_templatable_u64(
    field: &Templatable<u64>,
    bindings: &ResolvedBindings,
    ctx: &RunContext,
    name: &str,
) -> Result<Templatable<u64>, NikaError> {
    match field {
        Templatable::Value(v) => Ok(Templatable::Value(*v)),
        Templatable::Template(tpl) => {
            let resolved = resolve_template_str(tpl, bindings, ctx)?;
            let val = parse_u64(&resolved, name)?;
            Ok(Templatable::Value(val))
        }
    }
}

// ============================================================================
// Parse helpers
// ============================================================================

fn parse_f64(s: &str, name: &str) -> Result<f64, NikaError> {
    let trimmed = s.trim();
    let val = trimmed
        .parse::<f64>()
        .map_err(|_| NikaError::BindingTypeMismatch {
            path: name.to_string(),
            expected: "number".to_string(),
            actual: trimmed.to_string(),
        })?;
    if !val.is_finite() {
        return Err(NikaError::BindingTypeMismatch {
            path: name.to_string(),
            expected: "finite number".to_string(),
            actual: trimmed.to_string(),
        });
    }
    Ok(val)
}

fn parse_u32(s: &str, name: &str) -> Result<u32, NikaError> {
    // Also accept float-like "3.0" → 3 (common when values come from JSON)
    let trimmed = s.trim();
    trimmed
        .parse::<u32>()
        .or_else(|_| {
            trimmed
                .parse::<f64>()
                .ok()
                .and_then(|f| {
                    if f.fract() == 0.0 && f >= 0.0 && f <= u32::MAX as f64 {
                        Some(f as u32)
                    } else {
                        None
                    }
                })
                .ok_or(())
        })
        .map_err(|_| NikaError::BindingTypeMismatch {
            path: name.to_string(),
            expected: "positive integer".to_string(),
            actual: trimmed.to_string(),
        })
}

fn parse_u64(s: &str, name: &str) -> Result<u64, NikaError> {
    let trimmed = s.trim();
    trimmed
        .parse::<u64>()
        .or_else(|_| {
            trimmed
                .parse::<f64>()
                .ok()
                .and_then(|f| {
                    if f.fract() == 0.0 && f >= 0.0 && f <= u64::MAX as f64 {
                        Some(f as u64)
                    } else {
                        None
                    }
                })
                .ok_or(())
        })
        .map_err(|_| NikaError::BindingTypeMismatch {
            path: name.to_string(),
            expected: "positive integer".to_string(),
            actual: trimmed.to_string(),
        })
}

fn parse_bool_value(s: &str) -> Option<bool> {
    match s.to_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bool_value() {
        assert_eq!(parse_bool_value("true"), Some(true));
        assert_eq!(parse_bool_value("false"), Some(false));
        assert_eq!(parse_bool_value("yes"), Some(true));
        assert_eq!(parse_bool_value("no"), Some(false));
        assert_eq!(parse_bool_value("1"), Some(true));
        assert_eq!(parse_bool_value("0"), Some(false));
        assert_eq!(parse_bool_value("TRUE"), Some(true));
        assert_eq!(parse_bool_value("potato"), None);
    }

    #[test]
    fn test_resolve_opt_f64_value_passthrough() {
        let field = Some(Templatable::Value(0.7));
        let bindings = ResolvedBindings::new();
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        let result = resolve_opt_f64(&field, &bindings, &ctx, "temp").unwrap();
        assert_eq!(result, Some(Templatable::Value(0.7)));
    }

    #[test]
    fn test_resolve_opt_f64_none() {
        let bindings = ResolvedBindings::new();
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        let result = resolve_opt_f64(&None, &bindings, &ctx, "temp").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_u32_from_float() {
        // JSON often serializes integers as "3.0"
        assert_eq!(parse_u32("3.0", "test").unwrap(), 3);
        assert_eq!(parse_u32("100", "test").unwrap(), 100);
        assert!(parse_u32("3.5", "test").is_err());
        assert!(parse_u32("potato", "test").is_err());
        assert!(parse_u32("-1", "test").is_err());
    }

    #[test]
    fn test_parse_f64() {
        assert_eq!(parse_f64("0.7", "test").unwrap(), 0.7);
        assert_eq!(parse_f64(" 1.5 ", "test").unwrap(), 1.5);
        assert!(parse_f64("potato", "test").is_err());
    }

    #[test]
    fn test_range_validation_temperature() {
        let bindings = ResolvedBindings::new();
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        // In-range: no error
        let field = Some(Templatable::Value(1.5));
        let r = resolve_opt_f64_range(&field, &bindings, &ctx, "temperature", 0.0, 2.0);
        assert!(r.is_ok());
    }

    #[test]
    fn test_resolve_task_u32_none() {
        let bindings = ResolvedBindings::new();
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        assert_eq!(resolve_task_u32(&None, &bindings, &ctx, "x").unwrap(), None);
    }

    #[test]
    fn test_resolve_task_u32_value() {
        let bindings = ResolvedBindings::new();
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        let field = Some(Templatable::Value(5u32));
        assert_eq!(
            resolve_task_u32(&field, &bindings, &ctx, "x").unwrap(),
            Some(5)
        );
    }

    #[test]
    fn test_resolve_task_bool_value() {
        let bindings = ResolvedBindings::new();
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        let field = Some(Templatable::Value(true));
        assert_eq!(
            resolve_task_bool(&field, &bindings, &ctx, "x").unwrap(),
            Some(true)
        );
    }

    #[test]
    fn test_resolve_required_bool_value() {
        let bindings = ResolvedBindings::new();
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        assert!(
            !resolve_required_bool(&Templatable::Value(false), &bindings, &ctx, "x", true).unwrap()
        );
    }

    #[test]
    fn test_resolve_required_u64_value() {
        let bindings = ResolvedBindings::new();
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        assert_eq!(
            resolve_required_u64(&Templatable::Value(3600), &bindings, &ctx, "x", 0).unwrap(),
            3600
        );
    }
}
