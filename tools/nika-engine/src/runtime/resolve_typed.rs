//! Runtime resolution for `Templatable<T>` fields.
//!
//! When a workflow field like `temperature` contains a template expression
//! (e.g. `{{inputs.temperature}}`), it is stored as `Templatable::Template(String)`
//! through the AST pipeline. This module resolves those templates at execution time
//! using the available `ResolvedBindings` and `RunContext`.
//!
//! Resolution happens in `task_dispatch.rs` just before `lower_action()`.

use nika_core::ast::analyzed::AnalyzedTaskAction;
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
/// Returns `NikaError::BindingError` (NIKA-043) if a template resolves to a value
/// that cannot be parsed as the expected type (e.g. "hello" for a temperature field).
pub fn resolve_action_templates(
  action: &AnalyzedTaskAction,
  bindings: &ResolvedBindings,
  ctx: &RunContext,
) -> Result<AnalyzedTaskAction, NikaError> {
  match action {
    AnalyzedTaskAction::Infer(infer) => {
      let mut resolved = infer.clone();
      resolved.temperature = resolve_opt_f64(&infer.temperature, bindings, ctx, "temperature")?;
      resolved.max_tokens = resolve_opt_u32(&infer.max_tokens, bindings, ctx, "max_tokens")?;
      resolved.extended_thinking =
        resolve_opt_bool(&infer.extended_thinking, bindings, ctx, "extended_thinking")?;
      resolved.thinking_budget =
        resolve_opt_u32(&infer.thinking_budget, bindings, ctx, "thinking_budget")?;
      Ok(AnalyzedTaskAction::Infer(resolved))
    }
    AnalyzedTaskAction::Exec(exec) => {
      let mut resolved = exec.clone();
      resolved.shell = resolve_bool(&exec.shell, bindings, ctx, "shell")?;
      resolved.timeout_ms = resolve_opt_u64(&exec.timeout_ms, bindings, ctx, "timeout")?;
      resolved.max_stdout = resolve_opt_u64(&exec.max_stdout, bindings, ctx, "max_stdout")?;
      Ok(AnalyzedTaskAction::Exec(resolved))
    }
    AnalyzedTaskAction::Fetch(fetch) => {
      let mut resolved = fetch.clone();
      resolved.timeout_ms = resolve_opt_u64(&fetch.timeout_ms, bindings, ctx, "timeout")?;
      resolved.follow_redirects =
        resolve_bool(&fetch.follow_redirects, bindings, ctx, "follow_redirects")?;
      resolved.session = resolve_bool(&fetch.session, bindings, ctx, "session")?;
      resolved.cache = resolve_bool(&fetch.cache, bindings, ctx, "cache")?;
      Ok(AnalyzedTaskAction::Fetch(resolved))
    }
    AnalyzedTaskAction::Invoke(invoke) => {
      let mut resolved = invoke.clone();
      resolved.timeout_ms = resolve_opt_u64(&invoke.timeout_ms, bindings, ctx, "timeout")?;
      Ok(AnalyzedTaskAction::Invoke(resolved))
    }
    AnalyzedTaskAction::Agent(agent) => {
      let mut resolved = agent.clone();
      resolved.temperature =
        resolve_opt_f64(&agent.temperature, bindings, ctx, "temperature")?;
      resolved.max_tokens = resolve_opt_u32(&agent.max_tokens, bindings, ctx, "max_tokens")?;
      resolved.max_turns = resolve_opt_u32(&agent.max_turns, bindings, ctx, "max_turns")?;
      resolved.token_budget =
        resolve_opt_u32(&agent.token_budget, bindings, ctx, "token_budget")?;
      resolved.extended_thinking =
        resolve_opt_bool(&agent.extended_thinking, bindings, ctx, "extended_thinking")?;
      resolved.thinking_budget =
        resolve_opt_u32(&agent.thinking_budget, bindings, ctx, "thinking_budget")?;
      resolved.depth_limit = resolve_opt_u32(&agent.depth_limit, bindings, ctx, "depth_limit")?;
      Ok(AnalyzedTaskAction::Agent(resolved))
    }
  }
}

// ============================================================================
// Typed resolution helpers
// ============================================================================

fn resolve_template_str(
  template: &str,
  bindings: &ResolvedBindings,
  ctx: &RunContext,
) -> Result<String, NikaError> {
  let resolved = template_resolve(template, bindings, ctx)?;
  Ok(resolved.into_owned())
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
      let val: f64 = resolved.trim().parse().map_err(|_| {
        NikaError::BindingTypeMismatch {
          path: name.to_string(),
          expected: "number".to_string(),
          actual: resolved.clone(),
        }
      })?;
      Ok(Some(Templatable::Value(val)))
    }
  }
}

fn resolve_opt_u32(
  field: &Option<Templatable<u32>>,
  bindings: &ResolvedBindings,
  ctx: &RunContext,
  name: &str,
) -> Result<Option<Templatable<u32>>, NikaError> {
  match field {
    None => Ok(None),
    Some(Templatable::Value(v)) => Ok(Some(Templatable::Value(*v))),
    Some(Templatable::Template(tpl)) => {
      let resolved = resolve_template_str(tpl, bindings, ctx)?;
      let val: u32 = resolved.trim().parse().map_err(|_| {
        NikaError::BindingTypeMismatch {
          path: name.to_string(),
          expected: "positive integer".to_string(),
          actual: resolved.clone(),
        }
      })?;
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
      let val: u64 = resolved.trim().parse().map_err(|_| {
        NikaError::BindingTypeMismatch {
          path: name.to_string(),
          expected: "positive integer".to_string(),
          actual: resolved.clone(),
        }
      })?;
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
    let ctx = RunContext::default();
    let result = resolve_opt_f64(&field, &bindings, &ctx, "temp").unwrap();
    assert_eq!(result, Some(Templatable::Value(0.7)));
  }

  #[test]
  fn test_resolve_opt_f64_none() {
    let bindings = ResolvedBindings::new();
    let ctx = RunContext::default();
    let result = resolve_opt_f64(&None, &bindings, &ctx, "temp").unwrap();
    assert_eq!(result, None);
  }
}
