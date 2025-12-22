//! # Function Registry Module (v4.7.1)
//!
//! Handles custom function execution for `function:` tasks.
//!
//! ## Reference Format
//!
//! ```yaml
//! function: module::name
//! ```
//!
//! Where:
//! - `module` = Module/namespace name
//! - `name` = Function name within that module
//!
//! ## Built-in Functions
//!
//! Nika provides several built-in functions:
//!
//! - `aggregate::collect` - Collect outputs from multiple tasks
//! - `aggregate::merge` - Merge JSON objects
//! - `transform::format` - Format data as string
//! - `transform::json` - Parse JSON string
//!
//! ## Custom Functions
//!
//! Users can register custom functions via external scripts or WASM modules
//! (future feature).

use anyhow::{bail, Result};
use std::collections::HashMap;

/// Parsed function reference (module::name)
#[derive(Debug, Clone)]
pub struct FunctionReference {
    pub module: String,
    pub name: String,
}

impl FunctionReference {
    /// Parse a reference string in the format "module::name"
    pub fn parse(reference: &str) -> Result<Self> {
        let parts: Vec<&str> = reference.split("::").collect();
        if parts.len() != 2 {
            bail!(
                "Invalid function reference '{}': expected 'module::name' format",
                reference
            );
        }

        let module = parts[0].trim();
        let name = parts[1].trim();

        if module.is_empty() {
            bail!(
                "Invalid function reference '{}': module name is empty",
                reference
            );
        }
        if name.is_empty() {
            bail!(
                "Invalid function reference '{}': function name is empty",
                reference
            );
        }

        Ok(Self {
            module: module.to_string(),
            name: name.to_string(),
        })
    }

    /// Get full reference string
    pub fn full_name(&self) -> String {
        format!("{}::{}", self.module, self.name)
    }
}

/// Function execution result
#[derive(Debug, Clone)]
pub struct FunctionResult {
    pub output: serde_json::Value,
    pub success: bool,
    pub message: Option<String>,
}

/// Type alias for function implementations
pub type FunctionImpl = fn(serde_json::Value) -> Result<FunctionResult>;

/// Function Registry - manages custom functions
pub struct FunctionRegistry {
    /// Registered functions: "module::name" -> implementation
    functions: HashMap<String, FunctionImpl>,
}

impl FunctionRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        let mut registry = Self {
            functions: HashMap::new(),
        };

        // Register built-in functions
        registry.register_builtins();

        registry
    }

    /// Register built-in functions
    fn register_builtins(&mut self) {
        // aggregate::collect - Collect values into an array
        self.register("aggregate", "collect", builtin_aggregate_collect);

        // aggregate::merge - Merge JSON objects
        self.register("aggregate", "merge", builtin_aggregate_merge);

        // transform::format - Format as string
        self.register("transform", "format", builtin_transform_format);

        // transform::json - Parse JSON
        self.register("transform", "json", builtin_transform_json);

        // transform::passthrough - Pass data unchanged
        self.register("transform", "passthrough", builtin_transform_passthrough);
    }

    /// Register a function
    pub fn register(&mut self, module: &str, name: &str, func: FunctionImpl) {
        let key = format!("{}::{}", module, name);
        self.functions.insert(key, func);
    }

    /// Check if a function is registered
    pub fn has_function(&self, reference: &str) -> bool {
        self.functions.contains_key(reference)
    }

    /// Call a function
    pub fn call(&self, reference: &str, args: serde_json::Value) -> Result<FunctionResult> {
        if let Some(func) = self.functions.get(reference) {
            func(args)
        } else {
            // Return a stub result for unregistered functions
            Ok(FunctionResult {
                output: serde_json::json!({
                    "stub": true,
                    "reference": reference,
                    "args": args
                }),
                success: true,
                message: Some(format!(
                    "Function '{}' not registered, using stub",
                    reference
                )),
            })
        }
    }

    /// Get list of registered functions
    pub fn list_functions(&self) -> Vec<&str> {
        self.functions.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for FunctionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// BUILT-IN FUNCTIONS
// ============================================================================

/// aggregate::collect - Collect values into an array
fn builtin_aggregate_collect(args: serde_json::Value) -> Result<FunctionResult> {
    // If args is already an array, return it
    // Otherwise, wrap single value in array
    let output = if args.is_array() {
        args
    } else if args.is_object() {
        // Convert object values to array
        let obj = args.as_object().unwrap();
        serde_json::Value::Array(obj.values().cloned().collect())
    } else {
        serde_json::Value::Array(vec![args])
    };

    Ok(FunctionResult {
        output,
        success: true,
        message: None,
    })
}

/// aggregate::merge - Merge JSON objects
fn builtin_aggregate_merge(args: serde_json::Value) -> Result<FunctionResult> {
    let mut result = serde_json::Map::new();

    if let Some(arr) = args.as_array() {
        for item in arr {
            if let Some(obj) = item.as_object() {
                for (k, v) in obj {
                    result.insert(k.clone(), v.clone());
                }
            }
        }
    } else if let Some(obj) = args.as_object() {
        result = obj.clone();
    }

    Ok(FunctionResult {
        output: serde_json::Value::Object(result),
        success: true,
        message: None,
    })
}

/// transform::format - Format as string
fn builtin_transform_format(args: serde_json::Value) -> Result<FunctionResult> {
    let output = match args {
        serde_json::Value::String(s) => serde_json::Value::String(s),
        other => serde_json::Value::String(serde_json::to_string_pretty(&other)?),
    };

    Ok(FunctionResult {
        output,
        success: true,
        message: None,
    })
}

/// transform::json - Parse JSON string
fn builtin_transform_json(args: serde_json::Value) -> Result<FunctionResult> {
    let output = if let serde_json::Value::String(s) = args {
        serde_json::from_str(&s)?
    } else {
        args
    };

    Ok(FunctionResult {
        output,
        success: true,
        message: None,
    })
}

/// transform::passthrough - Pass data unchanged (identity function)
fn builtin_transform_passthrough(args: serde_json::Value) -> Result<FunctionResult> {
    Ok(FunctionResult {
        output: args,
        success: true,
        message: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_function_reference() {
        let ref1 = FunctionReference::parse("aggregate::collect").unwrap();
        assert_eq!(ref1.module, "aggregate");
        assert_eq!(ref1.name, "collect");

        let ref2 = FunctionReference::parse("transform::format").unwrap();
        assert_eq!(ref2.module, "transform");
        assert_eq!(ref2.name, "format");
    }

    #[test]
    fn test_parse_function_reference_invalid() {
        // Missing ::
        assert!(FunctionReference::parse("aggregate").is_err());

        // Empty module
        assert!(FunctionReference::parse("::collect").is_err());

        // Empty name
        assert!(FunctionReference::parse("aggregate::").is_err());
    }

    #[test]
    fn test_builtin_aggregate_collect() {
        let registry = FunctionRegistry::new();

        // Single value
        let result = registry
            .call("aggregate::collect", serde_json::json!("hello"))
            .unwrap();
        assert_eq!(result.output, serde_json::json!(["hello"]));

        // Array value
        let result = registry
            .call("aggregate::collect", serde_json::json!([1, 2, 3]))
            .unwrap();
        assert_eq!(result.output, serde_json::json!([1, 2, 3]));
    }

    #[test]
    fn test_builtin_aggregate_merge() {
        let registry = FunctionRegistry::new();

        let args = serde_json::json!([
            {"a": 1, "b": 2},
            {"c": 3, "d": 4}
        ]);
        let result = registry.call("aggregate::merge", args).unwrap();
        assert_eq!(
            result.output,
            serde_json::json!({"a": 1, "b": 2, "c": 3, "d": 4})
        );
    }

    #[test]
    fn test_builtin_transform_format() {
        let registry = FunctionRegistry::new();

        let result = registry
            .call("transform::format", serde_json::json!({"key": "value"}))
            .unwrap();
        assert!(result.output.is_string());
    }

    #[test]
    fn test_builtin_transform_passthrough() {
        let registry = FunctionRegistry::new();

        let input = serde_json::json!({"unchanged": true});
        let result = registry
            .call("transform::passthrough", input.clone())
            .unwrap();
        assert_eq!(result.output, input);
    }

    #[test]
    fn test_unregistered_function_returns_stub() {
        let registry = FunctionRegistry::new();

        let result = registry
            .call("custom::myFunc", serde_json::json!({"arg": 1}))
            .unwrap();
        assert!(result.success);
        assert!(result.message.is_some());
        assert!(result.output.get("stub").unwrap().as_bool().unwrap());
    }

    #[test]
    fn test_registry_has_builtins() {
        let registry = FunctionRegistry::new();

        assert!(registry.has_function("aggregate::collect"));
        assert!(registry.has_function("aggregate::merge"));
        assert!(registry.has_function("transform::format"));
        assert!(registry.has_function("transform::json"));
        assert!(registry.has_function("transform::passthrough"));
    }
}
