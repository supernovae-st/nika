// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Workflow return declarations — the envelope `outputs:` block.
//!
//! Per spec `01-envelope.md` §outputs · « `outputs:` declares **what the
//! workflow returns** » — in the **untyped form** (a bare `${{ }}`
//! reference) or the **typed form** (`{ value, type, description }`).
//! Post-R3b (LAW-GRAMMAR-0211) the typed form's `type:` speaks the full
//! `TypeExpr` of `09-types.md`, exactly like the `inputs:` half — the
//! callable contract never speaks two type languages at once.
//!
//! `outputs:` (envelope · plural) ≠ `output:` (task · singular jq bindings).

use nika_source::Spanned;

/// One envelope `outputs:` entry.
#[derive(Debug, Clone, PartialEq)]
pub enum OutputDecl {
    /// Untyped form · `summary: ${{ tasks.synthesize.output }}` — a bare
    /// `${{ }}` reference.
    Untyped(Spanned<String>),
    /// Typed form · `report: { value, type, description }` — powers the
    /// output half of the callable-workflow schema.
    Typed {
        /// The `${{ }}` reference (required in the typed form).
        value: Spanned<String>,
        /// Declared return type — the RAW `TypeExpr` of spec 09
        /// (shape-only · the grammar judgment is the analyzer's).
        r#type: Option<Spanned<serde_json::Value>>,
        /// Human-readable description.
        description: Option<String>,
    },
}

impl OutputDecl {
    /// The `${{ }}` reference expression, whichever form carries it.
    #[must_use]
    pub fn value(&self) -> &Spanned<String> {
        match self {
            Self::Untyped(v) | Self::Typed { value: v, .. } => v,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_source::Span;

    fn spanned(s: &str) -> Spanned<String> {
        Spanned::new(s.to_owned(), Span::default())
    }

    #[test]
    fn untyped_value_accessor() {
        let d = OutputDecl::Untyped(spanned("${{ tasks.x.output }}"));
        assert_eq!(d.value().value, "${{ tasks.x.output }}");
    }

    #[test]
    fn typed_value_accessor() {
        let d = OutputDecl::Typed {
            value: spanned("${{ tasks.report.output }}"),
            r#type: Some(Spanned::new(serde_json::json!("string"), Span::default())),
            description: Some("The final markdown brief".into()),
        };
        assert_eq!(d.value().value, "${{ tasks.report.output }}");
    }
}
