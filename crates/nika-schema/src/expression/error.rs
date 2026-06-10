// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Expression-layer errors — lexing · parsing · template scanning.
//!
//! Offsets are byte offsets **relative to the expression / scanned
//! string start** — callers (analyzer) translate to absolute file
//! spans and wrap into [`crate::error::SchemaError::TemplateSyntax`].

/// An error from the CEL v0.1-subset expression layer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ExprError {
    /// A character outside the grammar's alphabet.
    #[error("unexpected character `{ch}` at offset {offset}")]
    UnexpectedChar {
        /// The offending character.
        ch: char,
        /// Byte offset relative to the expression start.
        offset: usize,
    },

    /// A token where another was required.
    #[error("expected {expected}, found {found} at offset {offset}")]
    UnexpectedToken {
        /// What the parser found.
        found: String,
        /// What the grammar required.
        expected: String,
        /// Byte offset relative to the expression start.
        offset: usize,
    },

    /// An unterminated `'…'` / `"…"` string literal.
    #[error("unterminated string literal starting at offset {offset}")]
    UnterminatedString {
        /// Byte offset of the opening quote.
        offset: usize,
    },

    /// `a < b < c` — relations are non-associative (spec side
    /// constraint 3 · « Relations do not chain »).
    #[error("chained relation at offset {offset} — relations do not chain (use parentheses)")]
    ChainedRelation {
        /// Byte offset of the second relational operator.
        offset: usize,
    },

    /// A call to anything but `size` (spec side constraint 1 · « The
    /// only callable is `size` »).
    #[error("unknown function `{name}` at offset {offset} — `size` is the only v0.1 callable")]
    UnknownFunction {
        /// The attempted function name.
        name: String,
        /// Byte offset of the name.
        offset: usize,
    },

    /// Leftover input after a complete expression.
    #[error("trailing input at offset {offset} after a complete expression")]
    TrailingInput {
        /// Byte offset where the leftover begins.
        offset: usize,
    },

    /// An empty (or whitespace-only) expression.
    #[error("empty expression")]
    EmptyExpression,

    /// A `${{` island with no closing `}}` (spec `04-variables.md`).
    #[error("unterminated `${{{{` template island starting at offset {offset}")]
    UnterminatedTemplate {
        /// Byte offset of the `${{` opener.
        offset: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_carries_offsets() {
        let err = ExprError::UnexpectedChar { ch: '@', offset: 3 };
        assert!(err.to_string().contains('@'));
        assert!(err.to_string().contains('3'));
    }

    #[test]
    fn unterminated_template_display_renders_opener() {
        let err = ExprError::UnterminatedTemplate { offset: 7 };
        assert!(err.to_string().contains("${{"), "{err}");
        assert!(err.to_string().contains('7'));
    }
}
