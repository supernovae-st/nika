// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The pre-effect model resolver: a `model:` value as known before any
//! effect, from the operator's bindings, a declared default, a const or a
//! `with:` alias. Descended verbatim from `nika-runtime`'s `admit.rs`
//! (2026-09-28 · B9): it builds on [`static_literal_of`] and
//! [`bare_static_ref`] beside it, and the runtime's 15k wall needed the room
//! for the launch gates that share it. `nika-runtime` re-exports
//! [`resolve_model_expr`] at its historical path.

use std::collections::BTreeMap;

use nika_schema::raw::{ForEachValue, RawAction, RawTask, RawWorkflow};
use nika_schema::source::Spanned;
use nika_schema::types::VarDecl;
use serde_json::Value;

use crate::static_ref::{bare_static_ref, static_literal_of};

/// The workflow with a CLI `--model` swapped into the envelope default
/// (#342) — per-task `model:` keeps winning, mirroring the runtime's
/// precedence. The synthetic span is fine: the pricing surfaces never
/// render the envelope model's span. The ONE home for the swap (the
/// CLI's budget preflight AND the runtime's admission gate both price
/// the EFFECTIVE model — two surfaces, one constructor, no drift).
#[must_use]
pub fn with_model_override(wf: &RawWorkflow, model: &str) -> RawWorkflow {
    let mut wf = wf.clone();
    let span = wf
        .model
        .as_ref()
        .map_or_else(nika_schema::Span::default, |m| m.span);
    wf.model = Some(nika_schema::Spanned::new(model.to_owned(), span));
    wf
}

/// The workflow as this run seats it before any effect (B9): the operator's
/// `--model` in the envelope (a task's own `model:` keeps winning), then every
/// fan over an input the invocation binds iterating the bound value (B11 · a
/// bound value never falls back to the default), then every `model:` its
/// bindings, a declared default or a const decide, as literals. `None` when
/// that is the file itself. A seat only the run decides stays an expression,
/// judged at dispatch. Public so a host's budget warnings describe the same
/// workflow its floor prices.
#[must_use]
pub fn effective_workflow(
    wf: &RawWorkflow,
    model_override: Option<&str>,
    overrides: &BTreeMap<String, Value>,
) -> Option<RawWorkflow> {
    let seated = model_override.map(|m| with_model_override(wf, m));
    let bound = rendered_collections(seated.as_ref().unwrap_or(wf), overrides).or(seated);
    rendered_models(bound.as_ref().unwrap_or(wf), overrides).or(bound)
}

/// The workflow as the run would seat it before any effect: every envelope or
/// task `model:` expression that `overrides`, a declared default, a const or a
/// determinable `with:` alias decides becomes that literal (the resolver
/// below). A `model:` only the run decides (an upstream output, an answer, an
/// item) stays an expression: the check keeps it unbounded, and dispatch keeps
/// its authority. `None` when no `model:` changed. The operator's `--model` is
/// the caller's to apply first; a task's own `model:` keeps winning over it.
#[must_use]
pub fn rendered_models(
    wf: &RawWorkflow,
    overrides: &BTreeMap<String, Value>,
) -> Option<RawWorkflow> {
    let mut seated = wf.clone();
    let mut changed = false;
    let mut render = |model: &mut Spanned<String>, task: Option<&RawTask>| {
        if model.value.contains("${{")
            && let Some(literal) = resolve_model_expr(&model.value, wf, overrides, task)
        {
            model.value = literal;
            changed = true;
        }
    };
    if let Some(model) = seated.model.as_mut() {
        render(model, None);
    }
    for (task, declared) in seated.tasks.iter_mut().zip(&wf.tasks) {
        let model = match &mut task.value.action {
            RawAction::Infer(action) => action.model.as_mut(),
            RawAction::Agent(action) => action.model.as_mut(),
            _ => None,
        };
        if let Some(model) = model {
            render(model, Some(&declared.value));
        }
    }
    changed.then_some(seated)
}

/// The workflow as the run binds its fan-out collections before any effect
/// (B11): a `for_each` over a bare `${{ inputs.<name> }}` the invocation
/// binds iterates the bound value, exactly as the run's own binding does (a
/// bound value replaces the declared default and never falls back to it).
/// Each such input's declared literal becomes the bound value, so the static
/// readers count the operator's items: an array counts its length, anything
/// else stays an unknown count. A literal list, a const, and an input left
/// unbound keep their static count; a task output or a computed expression
/// stays unknown. `None` when no fan reads a bound input.
#[must_use]
pub fn rendered_collections(
    wf: &RawWorkflow,
    overrides: &BTreeMap<String, Value>,
) -> Option<RawWorkflow> {
    let bound: Vec<&str> = wf
        .tasks
        .iter()
        .filter_map(
            |task| match task.value.for_each.as_ref().map(|f| &f.value) {
                Some(ForEachValue::Expression(expr)) => bare_static_ref(expr),
                _ => None,
            },
        )
        .filter(|(authority, name)| *authority == "inputs." && overrides.contains_key(*name))
        .map(|(_, name)| name)
        .collect();
    if bound.is_empty() {
        return None;
    }
    let mut seated = wf.clone();
    for (key, decl) in &mut seated.inputs {
        let Some(value) = overrides
            .get(&key.value)
            .filter(|_| bound.contains(&key.value.as_str()))
        else {
            continue;
        };
        match decl {
            VarDecl::Untyped(literal) => literal.clone_from(value),
            VarDecl::Typed { default, .. } => *default = Some(value.clone()),
        }
    }
    Some(seated)
}

/// Every infer/agent seat as known before any effect: a task's own
/// `model:`, else the operator's `--model`, else the envelope's; a seat
/// only the run can decide is left out.
pub fn resolved_infer_models(
    wf: &RawWorkflow,
    model_override: Option<&str>,
    overrides: &BTreeMap<String, Value>,
) -> Vec<String> {
    let envelope = wf.model.as_ref().map(|m| m.value.as_str());
    let default = model_override
        .map(str::to_owned)
        .or_else(|| envelope.and_then(|expr| resolve_model_expr(expr, wf, overrides, None)));
    wf.tasks
        .iter()
        .filter_map(|task| {
            let declared = match &task.value.action {
                RawAction::Infer(action) => action.model.as_ref().map(|m| m.value.as_str()),
                RawAction::Agent(action) => action.model.as_ref().map(|m| m.value.as_str()),
                _ => return None,
            };
            match declared {
                Some(expr) => resolve_model_expr(expr, wf, overrides, Some(&task.value)),
                None => default.clone(),
            }
        })
        .collect()
}

/// A `model:` value as known before any effect (`--var`, a default, const, `with:`), else `None`.
pub fn resolve_model_expr(
    expr: &str,
    wf: &RawWorkflow,
    overrides: &BTreeMap<String, Value>,
    task: Option<&RawTask>,
) -> Option<String> {
    if !expr.contains("${{") {
        return Some(expr.to_owned());
    }
    if let Some(joined) = concat_model_expr(expr, wf, overrides, task) {
        return Some(joined);
    }
    if let Some((authority, name)) = bare_static_ref(expr)
        && authority == "inputs."
        && let Some(value) = overrides.get(name).and_then(Value::as_str)
    {
        return Some(value.to_owned());
    }
    if let Some(from_with) = with_alias(expr, wf, overrides, task) {
        return Some(from_with);
    }
    static_literal_of(wf, expr)?.as_str().map(str::to_owned)
}

/// `${{ inputs.provider }}/${{ inputs.name }}` — both sides resolve, the
/// slash is the catalog seat spelling (N01 / issue 1319).
fn concat_model_expr(
    expr: &str,
    wf: &RawWorkflow,
    overrides: &BTreeMap<String, Value>,
    task: Option<&RawTask>,
) -> Option<String> {
    let (left, right) = expr.split_once('/')?;
    if !left.contains("${{") || !right.contains("${{") {
        return None;
    }
    let left = resolve_model_expr(left, wf, overrides, task)?;
    let right = resolve_model_expr(right, wf, overrides, task)?;
    if left.contains("${{") || right.contains("${{") {
        return None;
    }
    Some(format!("{left}/{right}"))
}

/// `${{ with.model }}` follows the task's `with:` alias (N01).
fn with_alias(
    expr: &str,
    wf: &RawWorkflow,
    overrides: &BTreeMap<String, Value>,
    task: Option<&RawTask>,
) -> Option<String> {
    let inner = expr.trim().strip_prefix("${{")?.strip_suffix("}}")?.trim();
    let name = inner.strip_prefix("with.")?;
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return None;
    }
    let task = task?;
    let (_, bound) = task.with.iter().find(|(k, _)| k.value == name)?;
    let next = bound.value.as_str()?;
    resolve_model_expr(next, wf, overrides, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    const PAID: &str = "deepseek/deepseek-v4-pro";

    fn wf(yaml: &str) -> RawWorkflow {
        parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses")
    }

    fn seats(wf: &RawWorkflow) -> Vec<Option<String>> {
        let mut out = vec![wf.model.as_ref().map(|m| m.value.clone())];
        for task in &wf.tasks {
            if let RawAction::Infer(action) = &task.value.action {
                out.push(action.model.as_ref().map(|m| m.value.clone()));
            }
        }
        out
    }

    fn bind(value: &str) -> BTreeMap<String, Value> {
        [("m".to_owned(), Value::from(value))].into()
    }

    /// Every pre-effect source becomes its literal: an operator binding over
    /// the default, the declared default itself, a const, a `with:` alias.
    #[test]
    fn every_pre_effect_source_becomes_its_literal() {
        let head = "nika: w\ninputs:\n  m: { type: string, required: false, default: \"mock/echo\" }\nconst:\n  c: \"deepseek/deepseek-v4-pro\"\ntasks:\n";
        let infer = |model: &str| {
            format!("    infer: {{ prompt: hi, model: \"{model}\", max_tokens: 9 }}\n")
        };
        let flow = wf(&format!(
            "{head}  a:\n{}  b:\n{}  c:\n    with: {{ m: \"${{{{ inputs.m }}}}\" }}\n{}",
            infer("${{ inputs.m }}"),
            infer("${{ const.c }}"),
            infer("${{ with.m }}"),
        ));
        let bound = rendered_models(&flow, &bind(PAID)).expect("rendered");
        assert_eq!(
            seats(&bound),
            [
                None,
                Some(PAID.into()),
                Some(PAID.into()),
                Some(PAID.into())
            ]
        );
        let defaulted = rendered_models(&flow, &BTreeMap::new()).expect("rendered");
        let echo = Some("mock/echo".to_owned());
        assert_eq!(
            seats(&defaulted),
            [None, echo.clone(), Some(PAID.into()), echo]
        );
    }

    /// A seat only the run decides stays an expression; a literal is left
    /// alone; nothing changed is `None`. The envelope renders too.
    #[test]
    fn a_run_decided_seat_stays_an_expression() {
        let flow = wf(
            "nika: w\ntasks:\n  pick:\n    exec: { command: [\"true\"] }\n  ask:\n    with: { m: \"${{ tasks.pick.output }}\" }\n    infer: { prompt: hi, model: \"${{ with.m }}\", max_tokens: 9 }\n  lit:\n    infer: { prompt: hi, model: \"mock/echo\", max_tokens: 9 }\n",
        );
        assert!(
            rendered_models(&flow, &bind(PAID)).is_none(),
            "nothing decided"
        );
        let envelope = wf(
            "nika: w\nmodel: \"${{ inputs.m }}\"\ninputs:\n  m: { type: string, required: true }\ntasks:\n  ask:\n    infer: { prompt: hi, max_tokens: 9 }\n",
        );
        let bound = rendered_models(&envelope, &bind(PAID)).expect("rendered");
        assert_eq!(seats(&bound), [Some(PAID.into()), None]);
        assert!(
            rendered_models(&envelope, &BTreeMap::new()).is_none(),
            "no value yet"
        );
    }

    /// A fan over `inputs.xs` (declared default `["a"]`), a const fan, a
    /// literal fan and a task-output fan.
    fn fans() -> RawWorkflow {
        wf(
            "nika: w\ninputs:\n  xs: { type: { array: string }, required: false, default: [\"a\"] }\n  m: { type: string, required: false, default: \"mock/echo\" }\nconst:\n  cs: [\"k\"]\ntasks:\n  pick:\n    exec: { command: [\"true\"] }\n  by_input:\n    for_each: { items: \"${{ inputs.xs }}\" }\n    infer: { prompt: hi, model: \"${{ inputs.m }}\", max_tokens: 9 }\n  by_const:\n    for_each: { items: \"${{ const.cs }}\" }\n    infer: { prompt: hi, max_tokens: 9 }\n  by_list:\n    for_each: { items: [1, 2] }\n    infer: { prompt: hi, max_tokens: 9 }\n  by_output:\n    with: { found: \"${{ tasks.pick.output }}\" }\n    for_each: { items: \"${{ with.found }}\" }\n    infer: { prompt: hi, max_tokens: 9 }\n",
        )
    }

    /// The value the static readers count for `inputs.xs`.
    fn counted(flow: &RawWorkflow) -> Option<&Value> {
        static_literal_of(flow, "${{ inputs.xs }}")
    }

    /// The operator's items replace the declared default for the fan that
    /// reads them, whatever their count; the other fans are untouched.
    #[test]
    fn a_bound_input_fan_counts_the_operators_items() {
        let flow = fans();
        let five = serde_json::json!(["a", "b", "c", "d", "e"]);
        let bound = rendered_collections(&flow, &[("xs".to_owned(), five.clone())].into())
            .expect("a fan reads a bound input");
        assert_eq!(counted(&bound), Some(&five));
        let one = serde_json::json!(["z"]);
        let smaller = rendered_collections(&flow, &[("xs".to_owned(), one.clone())].into())
            .expect("a smaller value binds too");
        assert_eq!(counted(&smaller), Some(&one));
        assert_eq!(
            static_literal_of(&bound, "${{ const.cs }}"),
            static_literal_of(&flow, "${{ const.cs }}"),
            "a const fan keeps its literal"
        );
    }

    /// A bound value that is not an array never falls back to the default:
    /// the fan's count becomes unknown.
    #[test]
    fn a_bound_non_array_is_an_unknown_count_never_the_default() {
        let flow = fans();
        let bound = rendered_collections(&flow, &bind_xs(serde_json::json!("a")))
            .expect("the fan reads a bound input");
        assert_eq!(counted(&bound), Some(&serde_json::json!("a")));
        assert!(counted(&bound).and_then(Value::as_array).is_none());
    }

    /// Nothing changes when no fan reads a bound input: an unbound input
    /// keeps its default, and a binding only a `model:` reads is the model
    /// resolver's, not a collection's.
    #[test]
    fn no_bound_fan_changes_nothing() {
        let flow = fans();
        assert!(rendered_collections(&flow, &BTreeMap::new()).is_none());
        assert!(rendered_collections(&flow, &bind(PAID)).is_none());
        assert_eq!(counted(&flow), Some(&serde_json::json!(["a"])));
    }

    fn bind_xs(value: Value) -> BTreeMap<String, Value> {
        [("xs".to_owned(), value)].into()
    }
}
