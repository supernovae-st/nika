// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! YAML Bomb Protection via serde-saphyr Budget System
//!
//! This module provides protection against YAML bombs (deeply nested structures,
//! alias expansion attacks, and oversized inputs) using serde-saphyr's Budget system.
//!
//! # Security
//!
//! YAML bombs are a class of denial-of-service attacks that exploit:
//! - **Deep nesting**: Excessive recursion that can exhaust stack space
//! - **Alias expansion**: Exponential expansion of anchors/aliases (the "billion laughs" attack)
//! - **Large scalars**: Oversized string values that consume memory
//!
//! # Default Budget
//!
//! The default budget for Nika workflows is more restrictive than serde-saphyr's defaults:
//!
//! | Limit | Nika Default | serde-saphyr Default | Rationale |
//! |-------|--------------|----------------------|-----------|
//! | `max_depth` | 100 | 2,000 | Workflows rarely need >50 levels |
//! | `max_anchors` | 200 | 50,000 | Workflows should use few anchors |
//! | `max_aliases` | 500 | 50,000 | Limited alias usage expected |
//! | `max_nodes` | 50,000 | 250,000 | Workflows are typically small |
//! | `max_total_scalar_bytes` | 1 MiB | 64 MiB | Prevent memory exhaustion |
//! | `max_events` | 100,000 | 1,000,000 | Limit parser event count |
//!
//! # Usage
//!
//! ```rust,ignore
//! use nika::ast::budget::from_str_with_budget;
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct Workflow {
//!     schema: String,
//!     workflow: String,
//! }
//!
//! let yaml = r#"
//! schema: nika/workflow@0.12
//! workflow: example
//! "#;
//!
//! let workflow: Workflow = from_str_with_budget(yaml)?;
//! ```

use serde::de::{DeserializeOwned, Error as DeError};

// Re-export Budget from serde_saphyr for external use
pub use crate::serde_yaml::budget::{Budget, BudgetBreach, BudgetReport};
pub use crate::serde_yaml::options::{AliasLimits, Options};

/// Default budget for Nika workflows.
///
/// This budget is more restrictive than serde-saphyr defaults because
/// Nika workflows are expected to be relatively small, well-structured
/// YAML files rather than arbitrary user-generated content.
pub fn default_budget() -> Budget {
    Budget {
        max_depth: 100,                          // Workflows rarely need > 50 levels
        max_anchors: 200,                        // Few anchors expected in workflows
        max_aliases: 500,                        // Limited alias usage
        max_nodes: 50_000,                       // Workflows are typically small
        max_total_scalar_bytes: 1_048_576,       // 1 MiB - prevent memory exhaustion
        max_events: 100_000,                     // Limit parser event count
        max_documents: 10,                       // Few multi-doc workflows
        max_merge_keys: 100,                     // Limited merge key usage
        max_reader_input_bytes: Some(2_097_152), // 2 MiB input cap
        enforce_alias_anchor_ratio: true,        // Detect alias bombs
        alias_anchor_min_aliases: 50,            // Lower threshold for detection
        alias_anchor_ratio_multiplier: 5,        // Stricter ratio
    }
}

/// Default alias limits for Nika workflows.
///
/// These limits restrict alias replay to prevent exponential expansion attacks.
pub fn default_alias_limits() -> AliasLimits {
    AliasLimits {
        max_total_replayed_events: 100_000, // Total replayed events from all aliases
        max_replay_stack_depth: 32,         // Nested alias depth
        max_alias_expansions_per_anchor: 100, // How many times one anchor can be expanded
    }
}

/// Default options combining budget and alias limits for Nika.
pub fn default_options() -> Options {
    Options {
        budget: Some(default_budget()),
        alias_limits: default_alias_limits(),
        ..Options::default()
    }
}

/// Parse YAML with Nika's default budget protection.
///
/// This function enforces:
/// - Maximum nesting depth of 100
/// - Maximum 200 anchors
/// - Maximum 500 aliases
/// - Maximum 1 MiB of scalar content
/// - Alias/anchor ratio heuristic to detect alias bombs
///
/// # Errors
///
/// Returns `serde_saphyr::Error` if:
/// - The YAML is malformed
/// - Any budget limit is exceeded
/// - Deserialization fails
///
/// # Example
///
/// ```rust,ignore
/// use nika::ast::budget::from_str_with_budget;
///
/// let yaml = "name: test\nversion: 1";
/// let value: serde_json::Value = from_str_with_budget(yaml)?;
/// ```
pub fn from_str_with_budget<T: DeserializeOwned>(s: &str) -> Result<T, crate::serde_yaml::Error> {
    crate::serde_yaml::from_str_with_options(s, default_options())
}

/// Parse YAML with custom budget settings.
///
/// Use this when the default budget is too restrictive or permissive
/// for your specific use case.
///
/// # Example
///
/// ```rust,ignore
/// use nika::ast::budget::{from_str_with_custom_budget, default_budget};
///
/// let mut budget = default_budget();
/// budget.max_depth = 200;  // Allow deeper nesting
///
/// let yaml = "deeply: { nested: { content: true } }";
/// let value: serde_json::Value = from_str_with_custom_budget(yaml, budget)?;
/// ```
pub fn from_str_with_custom_budget<T: DeserializeOwned>(
    s: &str,
    budget: Budget,
) -> Result<T, crate::serde_yaml::Error> {
    let options = Options {
        budget: Some(budget),
        alias_limits: default_alias_limits(),
        ..Options::default()
    };
    crate::serde_yaml::from_str_with_options(s, options)
}

/// Check if YAML content exceeds budget limits without deserializing.
///
/// This is useful for pre-validation when you want to check budget
/// compliance before attempting deserialization.
///
/// # Returns
///
/// - `Ok(report)` with `report.breached.is_none()` if within budget
/// - `Ok(report)` with `report.breached.is_some()` if budget exceeded
/// - `Err(error)` if YAML is malformed
///
/// # Example
///
/// ```rust,ignore
/// use nika::ast::budget::check_budget;
///
/// let yaml = "key: value";
/// let report = check_budget(yaml)?;
/// if report.breached.is_none() {
///     println!("YAML is within budget limits");
/// }
/// ```
pub fn check_budget(input: &str) -> Result<BudgetReport, crate::serde_yaml::Error> {
    crate::serde_yaml::budget::check_yaml_budget(
        input,
        default_budget(),
        crate::serde_yaml::budget::EnforcingPolicy::AllContent,
    )
    .map_err(|e| <crate::serde_yaml::Error as DeError>::custom(e.to_string()))
}

/// Check if YAML content exceeds custom budget limits.
pub fn check_budget_with_custom(
    input: &str,
    budget: Budget,
) -> Result<BudgetReport, crate::serde_yaml::Error> {
    crate::serde_yaml::budget::check_yaml_budget(
        input,
        budget,
        crate::serde_yaml::budget::EnforcingPolicy::AllContent,
    )
    .map_err(|e| <crate::serde_yaml::Error as DeError>::custom(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct SimpleConfig {
        name: String,
        enabled: bool,
    }

    #[test]
    fn test_normal_yaml_parses_successfully() {
        let yaml = r#"
name: test-workflow
enabled: true
"#;

        let config: SimpleConfig = from_str_with_budget(yaml).unwrap();
        assert_eq!(config.name, "test-workflow");
        assert!(config.enabled);
    }

    #[test]
    fn test_nested_yaml_within_limits() {
        let yaml = r#"
level1:
  level2:
    level3:
      level4:
        value: "deep but acceptable"
"#;

        let result: serde_json::Value = from_str_with_budget(yaml).unwrap();
        let value = &result["level1"]["level2"]["level3"]["level4"]["value"];
        assert_eq!(value.as_str(), Some("deep but acceptable"));
    }

    #[test]
    fn test_deep_nesting_rejected() {
        // Generate YAML with nesting depth > 100
        let mut yaml = String::new();
        for _ in 0..110 {
            yaml.push('[');
        }
        for _ in 0..110 {
            yaml.push(']');
        }

        let result: Result<serde_json::Value, _> = from_str_with_budget(&yaml);
        assert!(result.is_err(), "Deeply nested YAML should be rejected");

        let err = result.unwrap_err();
        let err_str = err.to_string();
        // Should contain budget breach indication
        assert!(
            err_str.contains("depth") || err_str.contains("budget") || err_str.contains("Depth"),
            "Error should mention depth limit: {err_str}"
        );
    }

    #[test]
    fn test_many_anchors_rejected() {
        // Generate YAML with > 200 anchors
        let mut yaml = String::new();
        for i in 0..210 {
            yaml.push_str(&format!("anchor_{i}: &a{i} value{i}\n"));
        }

        let result: Result<serde_json::Value, _> = from_str_with_budget(&yaml);
        assert!(result.is_err(), "Too many anchors should be rejected");

        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("anchor") || err_str.contains("budget") || err_str.contains("Anchor"),
            "Error should mention anchor limit: {err_str}"
        );
    }

    #[test]
    fn test_large_scalar_rejected() {
        // Generate YAML with scalar content > 1 MiB
        let large_value = "x".repeat(1_100_000); // 1.1 MiB
        let yaml = format!("data: \"{large_value}\"");

        let result: Result<serde_json::Value, _> = from_str_with_budget(&yaml);
        assert!(result.is_err(), "Large scalar should be rejected");

        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("scalar")
                || err_str.contains("budget")
                || err_str.contains("Scalar")
                || err_str.contains("bytes"),
            "Error should mention scalar bytes limit: {err_str}"
        );
    }

    #[test]
    fn test_alias_bomb_rejected() {
        // Classic "billion laughs" style alias bomb (simplified)
        // Each level doubles the expansion
        let yaml = r#"
a: &a ["lol"]
b: &b [*a, *a]
c: &c [*b, *b]
d: &d [*c, *c]
e: &e [*d, *d]
f: &f [*e, *e]
g: &g [*f, *f]
h: &h [*g, *g]
i: &i [*h, *h]
j: &j [*i, *i]
k: &k [*j, *j]
l: &l [*k, *k]
m: &m [*l, *l]
n: &n [*m, *m]
o: &o [*n, *n]
p: &p [*o, *o]
q: &q [*p, *p]
r: &r [*q, *q]
s: &s [*r, *r]
t: &t [*s, *s]
result: *t
"#;

        let result: Result<serde_json::Value, _> = from_str_with_budget(yaml);
        // This should fail due to alias limits or alias/anchor ratio
        assert!(result.is_err(), "Alias bomb should be rejected");
    }

    #[test]
    fn test_check_budget_valid_yaml() {
        let yaml = "key: value\nlist:\n  - item1\n  - item2";
        let report = check_budget(yaml).unwrap();
        assert!(
            report.breached.is_none(),
            "Valid YAML should pass budget check"
        );
    }

    #[test]
    fn test_check_budget_deep_nesting() {
        let mut yaml = String::new();
        for _ in 0..110 {
            yaml.push('[');
        }
        for _ in 0..110 {
            yaml.push(']');
        }

        let report = check_budget(&yaml).unwrap();
        assert!(
            report.breached.is_some(),
            "Deep nesting should breach budget"
        );
        if let Some(BudgetBreach::Depth { depth }) = report.breached {
            assert!(depth > 100, "Should report depth > 100");
        }
    }

    #[test]
    fn test_custom_budget_allows_deeper_nesting() {
        // 60 levels of nesting - exceeds default (100) but we'll allow more
        let mut yaml = String::new();
        for _ in 0..60 {
            yaml.push('[');
        }
        for _ in 0..60 {
            yaml.push(']');
        }

        // Default budget should pass (60 < 100)
        let result: Result<serde_json::Value, _> = from_str_with_budget(&yaml);
        assert!(result.is_ok(), "60 levels should pass default budget");

        // Custom budget with lower depth should fail
        let mut strict_budget = default_budget();
        strict_budget.max_depth = 50;

        let result: Result<serde_json::Value, _> =
            from_str_with_custom_budget(&yaml, strict_budget);
        assert!(result.is_err(), "60 levels should fail with max_depth=50");
    }

    #[test]
    fn test_budget_report_statistics() {
        let yaml = r#"
root: &root
  key1: value1
  key2: value2
ref1: *root
ref2: *root
"#;

        let report = check_budget(yaml).unwrap();
        assert!(report.breached.is_none());
        assert!(report.nodes > 0, "Should count nodes");
        assert_eq!(report.anchors, 1, "Should count one anchor");
        assert_eq!(report.aliases, 2, "Should count two aliases");
    }

    #[test]
    fn test_default_options_has_correct_values() {
        let options = default_options();
        let budget = options.budget.unwrap();

        assert_eq!(budget.max_depth, 100);
        assert_eq!(budget.max_anchors, 200);
        assert_eq!(budget.max_aliases, 500);
        assert_eq!(budget.max_nodes, 50_000);
        assert_eq!(budget.max_total_scalar_bytes, 1_048_576);
        assert_eq!(budget.max_events, 100_000);
        assert!(budget.enforce_alias_anchor_ratio);

        let alias_limits = options.alias_limits;
        assert_eq!(alias_limits.max_total_replayed_events, 100_000);
        assert_eq!(alias_limits.max_replay_stack_depth, 32);
        assert_eq!(alias_limits.max_alias_expansions_per_anchor, 100);
    }

    #[test]
    fn test_empty_yaml_parses() {
        // Empty documents should work
        let yaml = "";
        let result: Result<Option<serde_json::Value>, _> = from_str_with_budget(yaml);
        // Empty YAML is valid and deserializes to None/null
        assert!(
            result.is_ok(),
            "Empty YAML should parse successfully: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_typical_workflow_size() {
        // A realistic Nika workflow should always pass
        let yaml = r#"
schema: nika/workflow@0.12
workflow: test-workflow

context:
  files:
    brand: ./context/brand.md
    persona: ./context/persona.json

tasks:
  - id: step1
    infer:
      prompt: "Generate a headline for our product"
      model: claude-sonnet-4-6
      temperature: 0.7

  - id: step2
    with:
      headline: step1
    infer:
      prompt: "Expand on this headline: {{with.headline}}"

  - id: step3
    exec:
      command: "echo 'Done processing'"

"#;

        let result: serde_json::Value = from_str_with_budget(yaml).unwrap();
        assert_eq!(result["schema"], "nika/workflow@0.12");
        assert_eq!(result["workflow"], "test-workflow");
    }
}
