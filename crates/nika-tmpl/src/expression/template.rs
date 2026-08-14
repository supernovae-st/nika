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
use crate::scan_islands;

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
    // Lexing is delegated to `nika_tmpl` — the ONE island scanner the runtime
    // resolver ALSO consumes, so `check` ⇄ `run` agree on island bounds by
    // construction (parity-by-construction · the drift that shipped the
    // 2026-06-18 escape bug is now structurally impossible). The checker then
    // layers its static expression parse on top of the shared spans.
    scan_islands(s)
        .map_err(|e| ExprError::UnterminatedTemplate { offset: e.offset() })?
        .into_iter()
        .map(|isl| {
            let src = isl.body.trim().to_owned();
            let expr = parse_expression(&src)?;
            Ok(TemplateIsland::new(expr, src, isl.start, isl.end))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expression::ast::{Literal, RelOp};

    #[test]
    fn scan_single_island() {
        let islands = scan_templates("${{ inputs.topic }}").expect("scan");
        assert_eq!(islands.len(), 1);
        assert_eq!(islands[0].src, "inputs.topic");
        assert_eq!(islands[0].start, 0);
        assert_eq!(islands[0].end, 19);
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
        let s = "${{ inputs.a }} and ${{ inputs.b }}";
        let islands = scan_templates(s).expect("scan");
        assert_eq!(islands.len(), 2);
        assert_eq!(islands[0].src, "inputs.a");
        assert_eq!(islands[1].src, "inputs.b");
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
        let islands = scan_templates(r"\${{ literal }} but ${{ inputs.real }}").expect("scan");
        assert_eq!(islands.len(), 1);
        assert_eq!(islands[0].src, "inputs.real");
    }

    #[test]
    fn unterminated_island_errors() {
        // Conformance fixture variables/011-unclosed-expression.
        let err = scan_templates("${{ inputs.x ").expect_err("unterminated");
        assert_eq!(err, ExprError::UnterminatedTemplate { offset: 0 });
    }

    #[test]
    fn island_with_string_containing_close_braces() {
        // A `}}` inside a CEL string literal must not close the island.
        let islands = scan_templates("${{ inputs.x == '}}' }}").expect("scan");
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
        let s = "café ${{ inputs.x }}";
        let islands = scan_templates(s).expect("scan");
        assert_eq!(islands.len(), 1);
        assert_eq!(&s[islands[0].start..islands[0].end], "${{ inputs.x }}");
    }

    #[test]
    fn adjacent_islands() {
        let islands = scan_templates("${{ inputs.a }}${{ inputs.b }}").expect("scan");
        assert_eq!(islands.len(), 2);
    }

    // ── Escape-skip stride (`\${{` → `i += 3`) ──────────────────────

    #[test]
    fn escaped_opener_skip_lands_exactly_past_three_bytes() {
        // After an escaped `\${{`, the scan must advance by EXACTLY 3 bytes
        // (past `${{`), so a REAL island immediately following is still
        // found. A mutant stride (`i *= 3`) overshoots far past the real
        // island and silently drops it. The long prefix makes the `$`
        // offset large, so `i*3` jumps clean off the end → 0 islands.
        let s = "prefix-thirty-chars-padding!! \\${{esc}}${{ inputs.real }}";
        let islands = scan_templates(s).expect("scan");
        assert_eq!(islands.len(), 1, "the real island after `\\${{` must scan");
        assert_eq!(islands[0].src, "inputs.real");
    }

    // ── String-escape stride inside an island (`i += 2`) ─────────────

    #[test]
    fn backslash_escape_inside_island_string_does_not_break_close() {
        // Inside a CEL string, `\'` is an escaped quote — the close-finder
        // must step OVER both bytes (`i += 2`) so the escaped quote does
        // NOT prematurely end the string AND the two escape bytes are not
        // re-interpreted. The `}}` after the `\'` (still inside the string)
        // stays protected; the island closes on the FINAL `}}`.
        //
        // A forward over-stride (`i *= 2`) overshoots the real close →
        // UnterminatedTemplate (caught by `.expect`). A backward stride
        // (`i -= 2`) re-reads the two pre-`\` bytes `ab` (no quote there),
        // stays "in string", marches forward, re-hits the same `\` and
        // loops → the mutant never terminates (caught by timeout). Correct
        // code parses cleanly.
        let islands = scan_templates(r"${{ inputs.x == 'ab\'}}cd' }}").expect("scan");
        assert_eq!(islands.len(), 1);
        assert_eq!(islands[0].src, r"inputs.x == 'ab\'}}cd'");
        let Expr::Relation {
            op: RelOp::Eq, rhs, ..
        } = &islands[0].expr
        else {
            panic!("Eq relation, got {:?}", islands[0].expr);
        };
        // The string literal: `\'` → `'`, the `}}` are literal content.
        assert_eq!(**rhs, Expr::Lit(Literal::Str("ab'}}cd".into())));
    }

    // ── Quote-close detection (`b == q`) ─────────────────────────────

    #[test]
    fn close_braces_inside_quoted_string_do_not_close_island() {
        // A `}}` INSIDE a CEL string literal must not close the island —
        // the scanner stays "inside the quote" until the MATCHING close
        // quote (`b == q`). The `}}` here sits AFTER a leading char so the
        // quote is genuinely open across it. A mutant (`b != q`) clears the
        // quote state on the first non-quote byte (`a`), exposing the inner
        // `}}` as a false close → body `'a` → an unterminated-string error,
        // not the well-formed island correct code produces.
        let islands = scan_templates("${{ 'a}}b' == inputs.x }}").expect("scan");
        assert_eq!(islands.len(), 1);
        assert_eq!(islands[0].src, "'a}}b' == inputs.x");
        let Expr::Relation {
            op: RelOp::Eq, lhs, ..
        } = &islands[0].expr
        else {
            panic!("Eq relation, got {:?}", islands[0].expr);
        };
        assert_eq!(**lhs, Expr::Lit(Literal::Str("a}}b".into())));

        // Plus the canonical short form (a string that IS just `}}`): the
        // single leading char is the quote itself, so the close quote is
        // reached before any non-quote byte — guards the simple case too.
        let islands = scan_templates("${{ inputs.x == '}}' }}").expect("scan");
        assert_eq!(islands.len(), 1);
        assert_eq!(islands[0].src, "inputs.x == '}}'");
    }

    // ── `}}` lookahead (`bytes.get(i + 1)`) ──────────────────────────

    #[test]
    fn a_single_brace_does_not_close_the_island() {
        // The island closes only on a DOUBLE `}}` — the lookahead checks
        // `bytes[i + 1]`. A mutant collapsing the lookahead to `bytes[i]`
        // (`i * 1`) would treat a lone `}` as a close.
        //
        // Case A · a `}` that is genuinely lone (no second `}` anywhere
        // after) must NOT produce a false close → the island is reported
        // unterminated. The mutant would instead close AT the lone `}` and
        // report a spurious, well-formed island.
        let err = scan_templates("${{ inputs.x } ").expect_err("lone brace");
        assert_eq!(err, ExprError::UnterminatedTemplate { offset: 0 });

        // Case B · a stray `}` (followed by a space) sits mid-body; the
        // true close is the `}}` further right. Correct code scans through
        // the stray `}`, so the body is `a } b` → an inner parse error.
        // The mutant closes at the stray `}`, yielding a bogus `a` island.
        let err = scan_templates("${{ a } b }}").expect_err("stray brace mid-body");
        assert!(
            !matches!(err, ExprError::UnterminatedTemplate { .. }),
            "the `}}}}` does close it — the body just doesn't parse: {err:?}"
        );

        // …and the all-valid form still closes on the real `}}`.
        let islands = scan_templates("${{ inputs.x in [1, 2] }}").expect("scan");
        assert_eq!(islands.len(), 1);
        assert_eq!(islands[0].src, "inputs.x in [1, 2]");
    }

    // ── Outer-scan bound (`while i < bytes.len()`) ───────────────────

    #[test]
    fn island_at_the_very_end_of_input_scans() {
        // Pins the outer-scan loop bound: an island flush against the end
        // (no trailing bytes) must be found. The opener probe `bytes[i..]`
        // and the trailing-`}}` are all within bounds.
        let islands = scan_templates("end:${{ inputs.z }}").expect("scan");
        assert_eq!(islands.len(), 1);
        assert_eq!(islands[0].src, "inputs.z");
        assert_eq!(islands[0].end, "end:${{ inputs.z }}".len());
    }
}
