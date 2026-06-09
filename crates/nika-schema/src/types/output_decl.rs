// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Workflow return declarations — the envelope `outputs:` block.
//!
//! Per spec `01-envelope.md` §outputs · « `outputs:` declares **what the
//! workflow returns** — the symmetric twin of `vars:` … in the **untyped
//! form** (bare reference) or the **typed form**
//! (`{ value, type, description }`) ».
//!
//! `outputs:` (envelope · plural) ≠ `output:` (task · singular jq bindings).

use crate::source::Spanned;

use super::var_decl::VarType;

/// One envelope `outputs:` entry.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum OutputDecl {
    /// Untyped form · `summary: ${{ tasks.synthesize.output }}` — a bare
    /// `${{ }}` reference.
    Untyped(Spanned<String>),
    /// Typed form · `report: { value, type, description }` — powers the
    /// output half of the callable-workflow schema.
    Typed {
        /// The `${{ }}` reference (required in the typed form).
        value: Spanned<String>,
        /// Declared return type.
        r#type: Option<VarType>,
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
    use crate::source::Span;

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
            r#type: Some(VarType::String),
            description: Some("The final markdown brief".into()),
        };
        assert_eq!(d.value().value, "${{ tasks.report.output }}");
    }
}
