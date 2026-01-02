//! Use wiring types for explicit data wiring (v0.1)
//!
//! Three forms supported:
//! - Form 1: `alias: task.path` -> single value (shorthand)
//! - Form 2: `task.path: [field1, field2]` -> batch extraction
//! - Form 3: `alias: { from: task, path: field, default: value }` -> advanced

use serde::Deserialize;
use serde_json::Value;
use rustc_hash::FxHashMap;

/// Use wiring - map of alias/path to entry
pub type UseWiring = FxHashMap<String, UseEntry>;

/// Three forms of use entry (serde auto-detects via untagged)
///
/// Order matters for serde untagged:
/// 1. Path (String) - simplest, tried first
/// 2. Batch (Vec<String>) - array
/// 3. Advanced (UseAdvanced) - object with from/path/default
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum UseEntry {
    /// Form 1: "alias: task.path" -> String path
    Path(String),

    /// Form 2: "task.path: [field1, field2]" -> Batch extraction
    Batch(Vec<String>),

    /// Form 3: "alias: { from: task, path: x.y, default: v }" -> Advanced
    Advanced(UseAdvanced),
}

/// Advanced use entry with explicit from/path/default
#[derive(Debug, Clone, Deserialize)]
pub struct UseAdvanced {
    /// Source task ID (required)
    pub from: String,

    /// Optional path within the task's output (dot-separated)
    /// Example: "summary" or "data.items.0"
    #[serde(default)]
    pub path: Option<String>,

    /// Optional default value if result is null
    #[serde(default)]
    pub default: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_form1_path() {
        let yaml = "forecast: weather.summary";
        let wiring: UseWiring = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(wiring.get("forecast"), Some(UseEntry::Path(_))));
    }

    #[test]
    fn parse_form2_batch() {
        let yaml = r#""flights.cheapest": [price, airline]"#;
        let wiring: UseWiring = serde_yaml::from_str(yaml).unwrap();
        match wiring.get("flights.cheapest") {
            Some(UseEntry::Batch(fields)) => {
                assert_eq!(fields, &["price", "airline"]);
            }
            _ => panic!("Expected Batch"),
        }
    }

    #[test]
    fn parse_form3_advanced_minimal() {
        let yaml = r#"
weather:
  from: weather_task
"#;
        let wiring: UseWiring = serde_yaml::from_str(yaml).unwrap();
        match wiring.get("weather") {
            Some(UseEntry::Advanced(adv)) => {
                assert_eq!(adv.from, "weather_task");
                assert!(adv.path.is_none());
                assert!(adv.default.is_none());
            }
            _ => panic!("Expected Advanced"),
        }
    }

    #[test]
    fn parse_form3_advanced_with_path() {
        let yaml = r#"
summary:
  from: weather_task
  path: data.summary
"#;
        let wiring: UseWiring = serde_yaml::from_str(yaml).unwrap();
        match wiring.get("summary") {
            Some(UseEntry::Advanced(adv)) => {
                assert_eq!(adv.from, "weather_task");
                assert_eq!(adv.path.as_deref(), Some("data.summary"));
            }
            _ => panic!("Expected Advanced"),
        }
    }

    #[test]
    fn parse_form3_advanced_with_default() {
        let yaml = r#"
price:
  from: flight_task
  path: price
  default: 0
"#;
        let wiring: UseWiring = serde_yaml::from_str(yaml).unwrap();
        match wiring.get("price") {
            Some(UseEntry::Advanced(adv)) => {
                assert_eq!(adv.from, "flight_task");
                assert_eq!(adv.path.as_deref(), Some("price"));
                assert_eq!(adv.default, Some(json!(0)));
            }
            _ => panic!("Expected Advanced"),
        }
    }

    #[test]
    fn parse_form3_advanced_default_object() {
        let yaml = r#"
fallback:
  from: some_task
  default:
    status: unknown
    code: -1
"#;
        let wiring: UseWiring = serde_yaml::from_str(yaml).unwrap();
        match wiring.get("fallback") {
            Some(UseEntry::Advanced(adv)) => {
                assert_eq!(adv.default, Some(json!({"status": "unknown", "code": -1})));
            }
            _ => panic!("Expected Advanced"),
        }
    }
}
