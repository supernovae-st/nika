// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `${{ … }}` template-island scanning (spec `04-variables.md`).
//!
//! « One syntax everywhere » · `${{ <CEL expr> }}` islands appear in any
//! string position. `\${{` is the literal escape (« the engine MUST
//! honor this »). An unterminated `${{` is a template syntax error.

use super::ast::Expr;
use super::error::ExprError;
use super::parser::parse_expression;

/// One `${{ … }}` island found in a string.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct TemplateIsland {
    /// The parsed inner expression.
    pub expr: Expr,
    /// The inner expression source (trimmed).
    pub src: String,
    /// Byte offset of the `${{` opener within the scanned string.
    pub start: usize,
    /// Byte offset just past the closing `}}`.
    pub end: usize,
}

impl TemplateIsland {
    /// Create an island record.
    #[must_use]
    pub fn new(expr: Expr, src: String, start: usize, end: usize) -> Self {
        Self {
            expr,
            src,
            start,
            end,
        }
    }
}

/// Scan a string for `${{ … }}` islands and parse each inner expression.
///
/// - `\${{` escapes produce NO island (the engine renders a literal
///   `${{` · spec `04-variables.md` §escaping).
/// - The island body is lexed quote-aware · a `}}` inside a CEL string
///   literal does not close the island.
///
/// # Errors
///
/// Returns [`ExprError::UnterminatedTemplate`] when a `${{` has no
/// closing `}}`, or the inner expression's parse error.
pub fn scan_templates(s: &str) -> Result<Vec<TemplateIsland>, ExprError> {
    let bytes = s.as_bytes();
    let mut islands = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        // Byte-level probe — `i` walks bytes and may sit inside a
        // multi-byte char; `${{` is pure ASCII so this is exact.
        if !bytes[i..].starts_with(b"${{") {
            i += 1;
            continue;
        }
        // `\${{` — escaped literal · not an island.
        if i > 0 && bytes[i - 1] == b'\\' {
            i += 3;
            continue;
        }
        let start = i;
        let body_start = i + 3;
        let body_end = find_island_close(s, body_start)
            .ok_or(ExprError::UnterminatedTemplate { offset: start })?;
        let src = s[body_start..body_end].trim().to_owned();
        let expr = parse_expression(&src)?;
        let end = body_end + 2;
        islands.push(TemplateIsland::new(expr, src, start, end));
        i = end;
    }
    Ok(islands)
}

/// Find the offset of the closing `}}`, skipping CEL string literals
/// (a `}}` inside `'…'` / `"…"` does not close the island).
fn find_island_close(s: &str, from: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = from;
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) => {
                if b == b'\\' {
                    i += 2;
                    continue;
                }
                if b == q {
                    quote = None;
                }
                i += 1;
            }
            None => {
                if b == b'\'' || b == b'"' {
                    quote = Some(b);
                    i += 1;
                } else if b == b'}' && bytes.get(i + 1) == Some(&b'}') {
                    return Some(i);
                } else {
                    i += 1;
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expression::ast::{Literal, RelOp};

    #[test]
    fn scan_single_island() {
        let islands = scan_templates("${{ vars.topic }}").expect("scan");
        assert_eq!(islands.len(), 1);
        assert_eq!(islands[0].src, "vars.topic");
        assert_eq!(islands[0].start, 0);
        assert_eq!(islands[0].end, 17);
    }

    #[test]
    fn scan_island_in_prose() {
        let s = "Summarize ${{ tasks.research.output }} in 3 bullets";
        let islands = scan_templates(s).expect("scan");
        assert_eq!(islands.len(), 1);
        assert_eq!(islands[0].start, 10);
        assert_eq!(
            &s[islands[0].start..islands[0].end],
            "${{ tasks.research.output }}"
        );
    }

    #[test]
    fn scan_multiple_islands() {
        let s = "${{ vars.a }} and ${{ vars.b }}";
        let islands = scan_templates(s).expect("scan");
        assert_eq!(islands.len(), 2);
        assert_eq!(islands[0].src, "vars.a");
        assert_eq!(islands[1].src, "vars.b");
    }

    #[test]
    fn scan_no_islands() {
        assert!(scan_templates("plain prose").expect("scan").is_empty());
        assert!(scan_templates("").expect("scan").is_empty());
    }

    #[test]
    fn escaped_island_is_literal() {
        // Spec 04 §escaping · « To embed a literal `${{` in a string ·
        // use `\${{` ».
        let islands = scan_templates(r"The syntax \${{ var.x }} is how you reference variables")
            .expect("scan");
        assert!(islands.is_empty(), "escaped opener must not scan");
    }

    #[test]
    fn escaped_and_real_island_mix() {
        let islands = scan_templates(r"\${{ literal }} but ${{ vars.real }}").expect("scan");
        assert_eq!(islands.len(), 1);
        assert_eq!(islands[0].src, "vars.real");
    }

    #[test]
    fn unterminated_island_errors() {
        // Conformance fixture variables/011-unclosed-expression.
        let err = scan_templates("${{ vars.x ").expect_err("unterminated");
        assert_eq!(err, ExprError::UnterminatedTemplate { offset: 0 });
    }

    #[test]
    fn island_with_string_containing_close_braces() {
        // A `}}` inside a CEL string literal must not close the island.
        let islands = scan_templates("${{ vars.x == '}}' }}").expect("scan");
        assert_eq!(islands.len(), 1);
        let Expr::Relation {
            op: RelOp::Eq, rhs, ..
        } = &islands[0].expr
        else {
            panic!("Eq relation");
        };
        assert_eq!(**rhs, Expr::Lit(Literal::Str("}}".into())));
    }

    #[test]
    fn inner_parse_error_propagates() {
        let err = scan_templates("${{ a < b < c }}").expect_err("chained relation");
        assert!(matches!(err, ExprError::ChainedRelation { .. }), "{err:?}");
    }

    #[test]
    fn island_offsets_are_byte_accurate() {
        let s = "café ${{ vars.x }}";
        let islands = scan_templates(s).expect("scan");
        assert_eq!(islands.len(), 1);
        assert_eq!(&s[islands[0].start..islands[0].end], "${{ vars.x }}");
    }

    #[test]
    fn adjacent_islands() {
        let islands = scan_templates("${{ vars.a }}${{ vars.b }}").expect("scan");
        assert_eq!(islands.len(), 2);
    }
}
