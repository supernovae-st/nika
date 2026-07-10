// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Expression-layer errors — lexing · parsing · template scanning.
//!
//! Offsets are byte offsets **relative to the expression / scanned
//! string start** — callers (the schema analyzer) translate to absolute
//! file spans and wrap into their own error types.
//!
//! Display + Error are hand-written (FCI-019 · the crate's zero-dep
//! law — same pattern as the scanner's own errors), byte-identical to
//! the `thiserror` renderings this file carried before descending.

use std::fmt;

/// An error from the CEL v0.1-subset expression layer.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExprError {
    /// A character outside the grammar's alphabet.
    UnexpectedChar {
        /// The offending character.
        ch: char,
        /// Byte offset relative to the expression start.
        offset: usize,
    },

    /// A token where another was required.
    UnexpectedToken {
        /// What the parser found.
        found: String,
        /// What the grammar required.
        expected: String,
        /// Byte offset relative to the expression start.
        offset: usize,
    },

    /// An unterminated `'…'` / `"…"` string literal.
    UnterminatedString {
        /// Byte offset of the opening quote.
        offset: usize,
    },

    /// `a < b < c` — relations are non-associative (spec side
    /// constraint 3 · « Relations do not chain »).
    ChainedRelation {
        /// Byte offset of the second relational operator.
        offset: usize,
    },

    /// A call to anything but `size` (spec side constraint 1 · « The
    /// only callable is `size` »).
    UnknownFunction {
        /// The attempted function name.
        name: String,
        /// Byte offset of the name.
        offset: usize,
    },

    /// Leftover input after a complete expression.
    TrailingInput {
        /// Byte offset where the leftover begins.
        offset: usize,
    },

    /// An empty (or whitespace-only) expression.
    EmptyExpression,

    /// A `${{` island with no closing `}}` (spec `04-variables.md`).
    UnterminatedTemplate {
        /// Byte offset of the `${{` opener.
        offset: usize,
    },

    /// An expression that nests (or chains) deeper than the parser admits —
    /// deep grouping `((((…))))` OR a wide flat `a || a || …` chain. Refused
    /// at parse so a later AST walker cannot overflow the native stack (the
    /// stack-overflow-`DoS` class · mirrors the runtime engine's cap).
    TooDeep {
        /// Byte offset where the depth limit was exceeded.
        offset: usize,
        /// The maximum nesting depth the parser admits.
        limit: usize,
    },

    /// A binary arithmetic operator (`+` · `-` · `*` · `/` · `%`) — the
    /// v0.1 CEL subset is a BOOLEAN guard grammar (comparisons · `&&`/`||`
    /// · `!` · `size` · member access), never a calculator (spec `03-dag.md`
    /// §CEL-subset). A distinct teaching variant so a new user who writes
    /// `${{ vars.a + vars.b > 5 }}` learns WHERE arithmetic belongs (a
    /// `nika:jq` task) instead of reading a raw `unexpected character`
    /// tokenizer error · mirrors [`Self::UnknownFunction`]'s « `size` is
    /// the only v0.1 callable » teaching.
    ArithmeticUnsupported {
        /// The offending operator.
        op: char,
        /// Byte offset relative to the expression start.
        offset: usize,
    },
}

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedChar { ch, offset } => {
                write!(f, "unexpected character `{ch}` at offset {offset}")
            }
            Self::UnexpectedToken {
                found,
                expected,
                offset,
            } => write!(f, "expected {expected}, found {found} at offset {offset}"),
            Self::UnterminatedString { offset } => {
                write!(f, "unterminated string literal starting at offset {offset}")
            }
            Self::ChainedRelation { offset } => write!(
                f,
                "chained relation at offset {offset} — relations do not chain (use parentheses)"
            ),
            Self::UnknownFunction { name, offset } => write!(
                f,
                "unknown function `{name}` at offset {offset} — `size` is the only v0.1 callable"
            ),
            Self::TrailingInput { offset } => write!(
                f,
                "trailing input at offset {offset} after a complete expression"
            ),
            Self::EmptyExpression => write!(f, "empty expression"),
            Self::UnterminatedTemplate { offset } => write!(
                f,
                "unterminated `${{{{` template island starting at offset {offset}"
            ),
            Self::TooDeep { offset, limit } => write!(
                f,
                "expression nests too deeply at offset {offset} (limit {limit})"
            ),
            Self::ArithmeticUnsupported { op, offset } => write!(
                f,
                "arithmetic operator `{op}` at offset {offset} — `${{{{ … }}}}` is a boolean \
                 guard, not a calculator (v0.1 CEL subset): compute the value in a `nika:jq` \
                 task and gate on `tasks.<id>.output`"
            ),
        }
    }
}

impl std::error::Error for ExprError {}

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

    /// The hand-written Display renders the EXACT strings the
    /// `thiserror` derive produced pre-descent — consumers match on
    /// message text (the checker's diagnostics), so the descent must
    /// be invisible in every rendering.
    #[test]
    fn display_is_byte_identical_to_the_pre_descent_renderings() {
        let cases: Vec<(ExprError, &str)> = vec![
            (
                ExprError::UnexpectedChar { ch: '@', offset: 3 },
                "unexpected character `@` at offset 3",
            ),
            (
                ExprError::UnexpectedToken {
                    found: "`)`".to_owned(),
                    expected: "an operand".to_owned(),
                    offset: 9,
                },
                "expected an operand, found `)` at offset 9",
            ),
            (
                ExprError::UnterminatedString { offset: 4 },
                "unterminated string literal starting at offset 4",
            ),
            (
                ExprError::ChainedRelation { offset: 6 },
                "chained relation at offset 6 — relations do not chain (use parentheses)",
            ),
            (
                ExprError::UnknownFunction {
                    name: "len".to_owned(),
                    offset: 2,
                },
                "unknown function `len` at offset 2 — `size` is the only v0.1 callable",
            ),
            (
                ExprError::TrailingInput { offset: 11 },
                "trailing input at offset 11 after a complete expression",
            ),
            (ExprError::EmptyExpression, "empty expression"),
            (
                ExprError::UnterminatedTemplate { offset: 7 },
                "unterminated `${{` template island starting at offset 7",
            ),
            (
                ExprError::TooDeep {
                    offset: 40,
                    limit: 32,
                },
                "expression nests too deeply at offset 40 (limit 32)",
            ),
        ];
        for (err, expected) in cases {
            assert_eq!(err.to_string(), expected);
        }
    }

    /// The teaching rendering is pinned byte-exact — the `${{{{`→`${{`
    /// format-escape is the classic silent break, and the message IS the
    /// product (it must name the jq route and the guard nature verbatim).
    #[test]
    fn arithmetic_display_teaches_the_jq_route() {
        let err = ExprError::ArithmeticUnsupported { op: '+', offset: 7 };
        assert_eq!(
            err.to_string(),
            "arithmetic operator `+` at offset 7 — `${{ … }}` is a boolean guard, \
             not a calculator (v0.1 CEL subset): compute the value in a `nika:jq` \
             task and gate on `tasks.<id>.output`"
        );
    }
}
