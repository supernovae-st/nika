// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Token budget enforcement — bridges nika-core estimation with engine types.
//!
//! Pure estimation and truncation functions live in `nika_core::binding::token_budget`.
//! This module provides `enforce_budget()` which needs `ResolvedBindings` + `EventKind`.

pub use nika_core::binding::token_budget::{
    estimate_bindings_tokens, estimate_tokens_str, estimate_tokens_value, truncate_to_tokens,
};

use serde_json::Value;

/// Minimum tokens preserved per binding during truncation.
const MIN_TOKENS_PER_BINDING: u64 = 50;

/// Enforce a token budget on resolved bindings.
///
/// If total estimated tokens exceed `budget`, truncates the largest string
/// bindings proportionally. Non-string values are left untouched.
///
/// Returns the EventKind to emit (BudgetOk or BudgetExceeded).
pub fn enforce_budget(
    bindings: &mut super::ResolvedBindings,
    budget: u32,
    task_id: &std::sync::Arc<str>,
) -> nika_event::EventKind {
    let budget_u64 = budget as u64;

    // Collect (alias, token_count) for all resolved bindings
    let binding_sizes: Vec<(String, u64)> = bindings
        .iter()
        .map(|(alias, value)| {
            let tokens = estimate_tokens_value(value);
            (alias.to_string(), tokens)
        })
        .collect();

    let actual: u64 = binding_sizes.iter().map(|(_, t)| *t).sum();

    if actual <= budget_u64 {
        return nika_event::EventKind::BudgetOk {
            task_id: std::sync::Arc::clone(task_id),
            budget,
            actual: actual as u32,
        };
    }

    // Need to truncate. Sort by size descending to truncate largest first.
    let mut sorted: Vec<(String, u64)> = binding_sizes;
    sorted.sort_by(|a, b| b.1.cmp(&a.1));

    let excess = actual - budget_u64;
    let mut remaining_excess = excess;
    let mut truncated_fields = Vec::new();

    for (alias, tokens) in &sorted {
        if remaining_excess == 0 {
            break;
        }

        // Only truncate string values
        let value = match bindings.get(alias) {
            Some(v) => v.clone(),
            None => continue,
        };

        if let Value::String(ref text) = value {
            // How much can we take from this binding?
            let max_reduction = tokens.saturating_sub(MIN_TOKENS_PER_BINDING);
            let reduction = remaining_excess.min(max_reduction);

            if reduction > 0 {
                let target_tokens = tokens - reduction;
                let truncated = truncate_to_tokens(text, target_tokens);
                bindings.set(alias.clone(), Value::String(truncated.to_string()));
                remaining_excess = remaining_excess.saturating_sub(reduction);
                truncated_fields.push(alias.clone());
            }
        }
    }

    nika_event::EventKind::BudgetExceeded {
        task_id: std::sync::Arc::clone(task_id),
        budget,
        actual: actual as u32,
        truncated_fields,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_enforce_budget_under_budget() {
        let mut bindings = super::super::ResolvedBindings::new();
        bindings.set("data", json!("short text"));
        let task_id = std::sync::Arc::from("test");
        let event = enforce_budget(&mut bindings, 1000, &task_id);
        assert!(
            matches!(event, nika_event::EventKind::BudgetOk { .. }),
            "Should be under budget"
        );
    }

    #[test]
    fn test_enforce_budget_over_budget_truncates() {
        let mut bindings = super::super::ResolvedBindings::new();
        // Long text: ~500 chars => ~125 tokens
        let long_text = "word ".repeat(100);
        bindings.set("data", json!(long_text));
        let task_id = std::sync::Arc::from("test");
        let event = enforce_budget(&mut bindings, 20, &task_id);
        match event {
            nika_event::EventKind::BudgetExceeded {
                truncated_fields, ..
            } => {
                assert!(
                    truncated_fields.contains(&"data".to_string()),
                    "Should truncate 'data' field: {truncated_fields:?}"
                );
                // Verify binding was actually truncated
                let val = bindings.get("data").unwrap();
                let text = val.as_str().unwrap();
                let tokens = estimate_tokens_str(text);
                assert!(
                    tokens <= 80,
                    "Should be truncated: {tokens} tokens, len={}",
                    text.len()
                );
            }
            _ => panic!("Should exceed budget"),
        }
    }

    #[test]
    fn test_enforce_budget_preserves_minimum() {
        let mut bindings = super::super::ResolvedBindings::new();
        // Even with tiny budget, each binding keeps MIN_TOKENS_PER_BINDING
        let text = "a ".repeat(200); // ~100 tokens
        bindings.set("data", json!(text));
        let task_id = std::sync::Arc::from("test");
        let event = enforce_budget(&mut bindings, 1, &task_id);
        assert!(
            matches!(event, nika_event::EventKind::BudgetExceeded { .. }),
            "Should exceed budget"
        );
        // Binding should still have some content
        let val = bindings.get("data").unwrap();
        let text = val.as_str().unwrap();
        assert!(!text.is_empty(), "Should preserve minimum content");
    }

    #[test]
    fn test_enforce_budget_no_context_budget() {
        let mut bindings = super::super::ResolvedBindings::new();
        bindings.set("x", json!("hello"));
        let task_id = std::sync::Arc::from("test");
        let event = enforce_budget(&mut bindings, 100, &task_id);
        assert!(matches!(event, nika_event::EventKind::BudgetOk { .. }));
    }

    #[test]
    fn test_enforce_budget_multiple_bindings() {
        let mut bindings = super::super::ResolvedBindings::new();
        bindings.set("big", json!("x ".repeat(200))); // ~100 tokens
        bindings.set("small", json!("tiny")); // ~2 tokens
        let task_id = std::sync::Arc::from("test");
        let event = enforce_budget(&mut bindings, 30, &task_id);
        match event {
            nika_event::EventKind::BudgetExceeded {
                truncated_fields, ..
            } => {
                assert!(
                    truncated_fields.contains(&"big".to_string()),
                    "Should truncate 'big': {truncated_fields:?}"
                );
            }
            _ => panic!("Should exceed budget"),
        }
    }

    #[test]
    fn test_enforce_budget_json_value_not_truncated() {
        let mut bindings = super::super::ResolvedBindings::new();
        bindings.set("config", json!({"key": "value", "nested": {"deep": true}}));
        let task_id = std::sync::Arc::from("test");
        let event = enforce_budget(&mut bindings, 2, &task_id);
        assert!(matches!(
            event,
            nika_event::EventKind::BudgetExceeded { .. }
        ));
        let val = bindings.get("config").unwrap();
        assert!(val.is_object(), "JSON object should be preserved");
    }
}
