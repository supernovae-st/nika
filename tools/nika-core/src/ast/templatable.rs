// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Template-aware typed values for workflow fields.
//!
//! Fields like `temperature`, `max_tokens`, `timeout` etc. are typed (f64, u32, bool)
//! but may contain template expressions like `{{inputs.temperature}}` that are resolved
//! at runtime. `Templatable<T>` allows both forms through the AST pipeline.
//!
//! ```text
//! YAML → Schema (oneOf) → Parser (Templatable<T>) → Analyzer → Lower → Runtime (resolve)
//! ```

use std::fmt;

/// A value that is either a concrete literal or a template expression.
///
/// Template expressions (e.g. `{{inputs.temperature}}`) are preserved as strings
/// through the AST pipeline and resolved at runtime when the execution context
/// is available.
#[derive(Debug, Clone, PartialEq)]
pub enum Templatable<T> {
    /// A concrete, already-parsed value.
    Value(T),
    /// A template expression to be resolved at runtime (e.g. `"{{inputs.temperature}}"`).
    Template(String),
}

impl<T> Templatable<T> {
    /// Returns the concrete value, if this is not a template.
    #[inline]
    pub fn as_value(&self) -> Option<&T> {
        match self {
            Templatable::Value(v) => Some(v),
            Templatable::Template(_) => None,
        }
    }

    /// Returns the template string, if this is a template.
    #[inline]
    pub fn as_template(&self) -> Option<&str> {
        match self {
            Templatable::Value(_) => None,
            Templatable::Template(s) => Some(s),
        }
    }

    /// Returns `true` if this is a template expression.
    #[inline]
    pub fn is_template(&self) -> bool {
        matches!(self, Templatable::Template(_))
    }

    /// Returns `true` if this is a concrete value.
    #[inline]
    pub fn is_value(&self) -> bool {
        matches!(self, Templatable::Value(_))
    }

    /// Transform the inner value, preserving templates.
    ///
    /// Used for type conversions like `f64 → f32` during lowering.
    pub fn map_value<U>(self, f: impl FnOnce(T) -> U) -> Templatable<U> {
        match self {
            Templatable::Value(v) => Templatable::Value(f(v)),
            Templatable::Template(s) => Templatable::Template(s),
        }
    }

    /// Unwrap the concrete value, panicking if this is a template.
    ///
    /// # Panics
    ///
    /// Panics if called on a `Template` variant.
    #[inline]
    pub fn unwrap_value(self) -> T {
        match self {
            Templatable::Value(v) => v,
            Templatable::Template(s) => {
                panic!("called unwrap_value() on a Template: {s}")
            }
        }
    }
}

impl<T: Copy> Templatable<T> {
    /// Copy the concrete value out, if present.
    #[inline]
    pub fn value(&self) -> Option<T> {
        match self {
            Templatable::Value(v) => Some(*v),
            Templatable::Template(_) => None,
        }
    }
}

impl<T: Default> Default for Templatable<T> {
    fn default() -> Self {
        Templatable::Value(T::default())
    }
}

impl<T: fmt::Display> fmt::Display for Templatable<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Templatable::Value(v) => write!(f, "{v}"),
            Templatable::Template(s) => write!(f, "{s}"),
        }
    }
}

/// Check if a string looks like a template expression (contains `{{`).
#[inline]
pub fn is_template_string(s: &str) -> bool {
    s.contains("{{")
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_variant() {
        let t = Templatable::Value(0.7f64);
        assert!(t.is_value());
        assert!(!t.is_template());
        assert_eq!(t.as_value(), Some(&0.7));
        assert_eq!(t.as_template(), None);
        assert_eq!(t.value(), Some(0.7));
    }

    #[test]
    fn test_template_variant() {
        let t: Templatable<f64> = Templatable::Template("{{inputs.temperature}}".to_string());
        assert!(t.is_template());
        assert!(!t.is_value());
        assert_eq!(t.as_template(), Some("{{inputs.temperature}}"));
        assert_eq!(t.as_value(), None);
        assert_eq!(t.value(), None);
    }

    #[test]
    fn test_map_value_on_value() {
        let t = Templatable::Value(0.7f64);
        let mapped = t.map_value(|v| v as f32);
        assert_eq!(mapped, Templatable::Value(0.7f32));
    }

    #[test]
    fn test_map_value_on_template() {
        let t: Templatable<f64> = Templatable::Template("{{inputs.temp}}".to_string());
        let mapped: Templatable<f32> = t.map_value(|v| v as f32);
        assert_eq!(mapped, Templatable::Template("{{inputs.temp}}".to_string()));
    }

    #[test]
    fn test_unwrap_value() {
        let t = Templatable::Value(42u32);
        assert_eq!(t.unwrap_value(), 42);
    }

    #[test]
    #[should_panic(expected = "called unwrap_value() on a Template")]
    fn test_unwrap_value_panics_on_template() {
        let t: Templatable<u32> = Templatable::Template("{{inputs.count}}".to_string());
        t.unwrap_value();
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", Templatable::Value(0.7f64)), "0.7");
        assert_eq!(
            format!(
                "{}",
                Templatable::<f64>::Template("{{inputs.t}}".to_string())
            ),
            "{{inputs.t}}"
        );
    }

    #[test]
    fn test_is_template_string() {
        assert!(is_template_string("{{inputs.temperature}}"));
        assert!(is_template_string("prefix {{with.val}} suffix"));
        assert!(!is_template_string("0.7"));
        assert!(!is_template_string("true"));
        assert!(!is_template_string("{single_brace}"));
    }

    #[test]
    fn test_clone_and_eq() {
        let a = Templatable::Value(42u32);
        let b = a.clone();
        assert_eq!(a, b);

        let c: Templatable<u32> = Templatable::Template("{{x}}".to_string());
        let d = c.clone();
        assert_eq!(c, d);
        assert_ne!(a, c);
    }
}
