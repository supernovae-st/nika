// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Template-aware typed values for workflow fields.
//!
//! Fields like `temperature`, `max_tokens`, `timeout` are typed (`f64`, `u32`,
//! `bool`) but may contain template expressions like `{{inputs.temperature}}`
//! resolved at runtime. `Templatable<T>` preserves both forms through the AST.
//!
//! ```text
//! YAML → Parser (Templatable<T>) → Analyzer → Runtime (resolve)
//! ```
//!
//! Not serde-derived: the parser manually constructs `Value` or `Template`
//! variants by inspecting whether a YAML scalar contains `{{`.

use std::fmt;

/// A value that is either a concrete literal or a template expression.
///
/// Template expressions (e.g. `{{inputs.temperature}}`) are preserved as
/// strings through the AST pipeline and resolved at runtime.
///
/// **Exhaustive by design** (FCI-002 carve-out) · this is a closed 2-variant
/// sum — a value IS either concrete or a template, there is no third state.
/// Downstream exhaustive matching on `Value`/`Template` is the intended API;
/// `#[non_exhaustive]` would force wildcard arms for a variant that cannot
/// exist. Recorded 2026-06-10 (architecture review).
#[derive(Debug, Clone, PartialEq)]
pub enum Templatable<T> {
    /// A concrete, already-parsed value.
    Value(T),
    /// A template expression to be resolved at runtime.
    Template(String),
}

impl<T> Templatable<T> {
    /// Returns the concrete value, if not a template.
    #[inline]
    #[must_use]
    pub fn as_value(&self) -> Option<&T> {
        match self {
            Self::Value(v) => Some(v),
            Self::Template(_) => None,
        }
    }

    /// Returns the template string, if this is a template.
    #[inline]
    #[must_use]
    pub fn as_template(&self) -> Option<&str> {
        match self {
            Self::Value(_) => None,
            Self::Template(s) => Some(s),
        }
    }

    /// Returns `true` if this is a template expression.
    #[inline]
    #[must_use]
    pub fn is_template(&self) -> bool {
        matches!(self, Self::Template(_))
    }

    /// Returns `true` if this is a concrete value.
    #[inline]
    #[must_use]
    pub fn is_value(&self) -> bool {
        matches!(self, Self::Value(_))
    }

    /// Transform the inner value, preserving templates.
    #[must_use]
    pub fn map_value<U>(self, f: impl FnOnce(T) -> U) -> Templatable<U> {
        match self {
            Self::Value(v) => Templatable::Value(f(v)),
            Self::Template(s) => Templatable::Template(s),
        }
    }

    /// Consume and return the concrete value, or `None` if template.
    #[must_use]
    pub fn into_value(self) -> Option<T> {
        match self {
            Self::Value(v) => Some(v),
            Self::Template(_) => None,
        }
    }
}

impl<T: Copy> Templatable<T> {
    /// Copy the concrete value out, if present.
    #[inline]
    #[must_use]
    pub fn value(&self) -> Option<T> {
        match self {
            Self::Value(v) => Some(*v),
            Self::Template(_) => None,
        }
    }
}

impl<T: Default> Default for Templatable<T> {
    fn default() -> Self {
        Self::Value(T::default())
    }
}

impl<T: fmt::Display> fmt::Display for Templatable<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Value(v) => write!(f, "{v}"),
            Self::Template(s) => write!(f, "{s}"),
        }
    }
}

/// Check if a string looks like a template expression (contains `{{`).
#[inline]
#[must_use]
pub fn is_template_string(s: &str) -> bool {
    s.contains("{{")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_variant() {
        let t = Templatable::Value(0.7_f64);
        assert!(t.is_value());
        assert!(!t.is_template());
        assert_eq!(t.as_value(), Some(&0.7));
        assert_eq!(t.as_template(), None);
        assert_eq!(t.value(), Some(0.7));
    }

    #[test]
    fn template_variant() {
        let t: Templatable<f64> = Templatable::Template("{{inputs.temperature}}".into());
        assert!(t.is_template());
        assert!(!t.is_value());
        assert_eq!(t.as_template(), Some("{{inputs.temperature}}"));
        assert_eq!(t.as_value(), None);
        assert_eq!(t.value(), None);
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn map_value_on_value() {
        let t = Templatable::Value(0.7_f64);
        let mapped = t.map_value(|v| v as f32);
        assert_eq!(mapped, Templatable::Value(0.7_f32));
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn map_value_on_template() {
        let t: Templatable<f64> = Templatable::Template("{{inputs.temp}}".into());
        let mapped: Templatable<f32> = t.map_value(|v| v as f32);
        assert_eq!(mapped, Templatable::Template("{{inputs.temp}}".into()));
    }

    #[test]
    fn into_value_some() {
        let t = Templatable::Value(42_u32);
        assert_eq!(t.into_value(), Some(42));
    }

    #[test]
    fn into_value_none() {
        let t: Templatable<u32> = Templatable::Template("{{inputs.count}}".into());
        assert_eq!(t.into_value(), None);
    }

    #[test]
    fn default_is_value_default() {
        let t: Templatable<u32> = Templatable::default();
        assert_eq!(t, Templatable::Value(0));
    }

    #[test]
    fn display() {
        assert_eq!(Templatable::Value(0.7_f64).to_string(), "0.7");
        assert_eq!(
            Templatable::<f64>::Template("{{inputs.t}}".into()).to_string(),
            "{{inputs.t}}"
        );
    }

    #[test]
    fn is_template_string_detection() {
        assert!(is_template_string("{{inputs.temperature}}"));
        assert!(is_template_string("prefix {{with.val}} suffix"));
        assert!(!is_template_string("0.7"));
        assert!(!is_template_string("true"));
        assert!(!is_template_string("{single_brace}"));
    }

    #[test]
    fn clone_and_eq() {
        let a = Templatable::Value(42_u32);
        let b = a.clone();
        assert_eq!(a, b);

        let c: Templatable<u32> = Templatable::Template("{{x}}".into());
        assert_ne!(a, c);
    }
}
