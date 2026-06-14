// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `cel-subset/0.1` recursive-descent parser — tokens → AST.
//!
//! One function per EBNF production (03-dag §formal-grammar), in
//! precedence order (loosest→tightest as we descend): `ternary` → `or`
//! → `and` → `rel` → `unary` → `postfix` → `primary`. The relation rung
//! is NON-associative (at most one `relop` · `a < b < c` is
//! NIKA-VAR-005). Every grammar violation is NIKA-VAR-005.

use serde_json::Value;

use crate::ast::{Expr, Node, RelOp, Step};
use crate::error::CelError;
use crate::lexer::{Tok, Token, lex};

/// The maximum AST nesting depth the parser will build before refusing with
/// NIKA-VAR-005. Two pathological shapes overflow the native stack (a
/// non-catchable process abort, not an `Err`) when the COMPUTER or a host
/// AST walker later recurses the tree:
///
/// 1. deep grouping — `${{ ((((…)))) }}` (the `ternary`/`unary` rungs); and
/// 2. a wide FLAT chain — `${{ a || a || … || a }}` (30k terms), which the
///    iterative `or`/`and` loops would build as a 30k-deep LEFT-NESTED tree.
///
/// Both are capped here at parse time (reached by BOTH `check` and `run`),
/// so the AST handed downstream is never deeper than this. The cap is far
/// below any real workflow expression and far below the stack budget. It
/// aligns with the runtime `when:`-nesting limit (the static-subset class ·
/// NIKA-VAR-005). The evaluator additionally bounds its own recursion as a
/// defense-in-depth backstop (see `compute::MAX_EVAL_DEPTH`).
const MAX_DEPTH: usize = 128;

/// Parse the inside of one `${{ }}` island (trimmed) into an [`Expr`].
///
/// # Errors
///
/// [`CelError`] with `NIKA-VAR-005` on any grammar violation (bad token ·
/// chained relation · unknown call · trailing input · unclosed group).
pub fn parse(src: &str) -> Result<Expr, CelError> {
    let tokens = lex(src)?;
    let end = src.len();
    let mut p = Parser {
        tokens: &tokens,
        pos: 0,
        end,
        depth: 0,
    };
    let node = p.ternary()?;
    if p.pos < p.tokens.len() {
        let span = p.peek_span();
        return Err(CelError::static_err("unexpected trailing input", span));
    }
    Ok(Expr::new(node))
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    end: usize,
    /// Current recursive-descent nesting depth (capped at [`MAX_DEPTH`] to
    /// keep a crafted deep expression from overflowing the native stack).
    depth: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos).map(|t| &t.kind)
    }

    fn peek_span(&self) -> (usize, usize) {
        self.tokens
            .get(self.pos)
            .map_or((self.end, self.end), |t| t.span)
    }

    fn eat(&mut self, want: &Tok) -> bool {
        if self.peek() == Some(want) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn consume(&mut self, want: &Tok, what: &str) -> Result<(), CelError> {
        if self.eat(want) {
            Ok(())
        } else {
            Err(CelError::static_err(
                format!("expected {what}"),
                self.peek_span(),
            ))
        }
    }

    // ── ternary = or , [ "?" , expr , ":" , ternary ] ───────────────
    fn ternary(&mut self) -> Result<Node, CelError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(CelError::static_err(
                "expression nests too deeply",
                self.peek_span(),
            ));
        }
        let cond = self.or()?;
        let node = if self.eat(&Tok::Question) {
            let then = self.ternary()?; // the middle is a full expr (= ternary)
            self.consume(&Tok::Colon, "`:` in the conditional")?;
            let otherwise = self.ternary()?; // right-associative
            Node::Ternary {
                cond: Box::new(cond),
                then: Box::new(then),
                otherwise: Box::new(otherwise),
            }
        } else {
            cond
        };
        self.depth -= 1;
        Ok(node)
    }

    // ── or = and , { "||" , and } ───────────────────────────────────
    // The loop is ITERATIVE (no parse-stack growth), but each `||` adds one
    // `Node::Or` nesting level — a 30k-wide chain builds a 30k-deep
    // LEFT-NESTED tree the COMPUTER (and the host's AST walkers) then
    // recurse, overflowing the native stack. Cap the chain depth the same
    // way nesting is capped, so a crafted wide chain is NIKA-VAR-005 here
    // (reached by both `check` and `run`) and the AST never gets pathological.
    fn or(&mut self) -> Result<Node, CelError> {
        let entry = self.depth;
        let mut lhs = self.and()?;
        while self.eat(&Tok::OrOr) {
            self.depth += 1;
            if self.depth > MAX_DEPTH {
                return Err(CelError::static_err(
                    "expression nests too deeply",
                    self.peek_span(),
                ));
            }
            let rhs = self.and()?;
            lhs = Node::Or(Box::new(lhs), Box::new(rhs));
        }
        self.depth = entry; // the chain is one returned subtree · restore
        Ok(lhs)
    }

    // ── and = rel , { "&&" , rel } ──────────────────────────────────
    // Same flat-chain depth cap as `or` (a wide `&&` chain is the dual DoS).
    fn and(&mut self) -> Result<Node, CelError> {
        let entry = self.depth;
        let mut lhs = self.rel()?;
        while self.eat(&Tok::AndAnd) {
            self.depth += 1;
            if self.depth > MAX_DEPTH {
                return Err(CelError::static_err(
                    "expression nests too deeply",
                    self.peek_span(),
                ));
            }
            let rhs = self.rel()?;
            lhs = Node::And(Box::new(lhs), Box::new(rhs));
        }
        self.depth = entry; // restore — siblings get a fresh budget
        Ok(lhs)
    }

    // ── rel = unary , [ relop , unary ]  (NON-associative) ──────────
    fn rel(&mut self) -> Result<Node, CelError> {
        let lhs = self.unary()?;
        let Some(op) = self.peek_relop() else {
            return Ok(lhs);
        };
        let op_span = self.peek_span();
        self.pos += 1;
        let rhs = self.unary()?;
        // Non-associative: a second relop here is a chained relation.
        if self.peek_relop().is_some() {
            return Err(CelError::static_err(
                "chained relation — relations do not associate (wrap with parens)",
                self.peek_span(),
            ));
        }
        Ok(Node::Rel {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            span: op_span,
        })
    }

    fn peek_relop(&self) -> Option<RelOp> {
        Some(match self.peek()? {
            Tok::EqEq => RelOp::Eq,
            Tok::NotEq => RelOp::Ne,
            Tok::Lt => RelOp::Lt,
            Tok::Le => RelOp::Le,
            Tok::Gt => RelOp::Gt,
            Tok::Ge => RelOp::Ge,
            Tok::In => RelOp::In,
            _ => return None,
        })
    }

    // ── unary = { "!" } , postfix ───────────────────────────────────
    fn unary(&mut self) -> Result<Node, CelError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(CelError::static_err(
                "expression nests too deeply",
                self.peek_span(),
            ));
        }
        let node = if self.eat(&Tok::Bang) {
            Node::Not(Box::new(self.unary()?))
        } else {
            self.postfix()?
        };
        self.depth -= 1;
        Ok(node)
    }

    // ── postfix = primary , { "." IDENT [call] | "[" expr "]" } ─────
    fn postfix(&mut self) -> Result<Node, CelError> {
        let base = self.primary()?;
        let mut steps = Vec::new();
        loop {
            if self.eat(&Tok::Dot) {
                steps.push(self.field_or_method()?);
            } else if self.eat(&Tok::LBrack) {
                let idx = self.ternary()?;
                self.consume(&Tok::RBrack, "`]` to close the index")?;
                steps.push(Step::Index(Box::new(idx)));
            } else {
                break;
            }
        }
        if steps.is_empty() {
            Ok(base)
        } else {
            Ok(Node::Postfix {
                base: Box::new(base),
                steps,
            })
        }
    }

    /// After a `.`: an IDENT, optionally a method call `(args)`. The
    /// method set is closed — `size` (0-arg), `contains`/`startsWith`/
    /// `endsWith` (1-arg). Any other method is NIKA-VAR-005.
    fn field_or_method(&mut self) -> Result<Step, CelError> {
        let span = self.peek_span();
        let Some(Tok::Ident(name)) = self.peek().cloned() else {
            return Err(CelError::static_err(
                "expected a field name after `.`",
                span,
            ));
        };
        self.pos += 1;
        if !self.eat(&Tok::LParen) {
            return Ok(Step::Field(name));
        }
        // A method call.
        let arity = match name.as_str() {
            "size" => 0,
            "contains" | "startsWith" | "endsWith" => 1,
            _ => {
                return Err(CelError::static_err(
                    format!("unknown method `.{name}()` — outside cel-subset/0.1"),
                    span,
                ));
            }
        };
        let mut args = Vec::new();
        if arity == 1 {
            args.push(self.ternary()?);
        }
        self.consume(&Tok::RParen, "`)` to close the method call")?;
        Ok(Step::Method { name, args })
    }

    // ── primary = literal | list | call | IDENT | "(" expr ")" ──────
    fn primary(&mut self) -> Result<Node, CelError> {
        let span = self.peek_span();
        match self.peek().cloned() {
            Some(Tok::Int(n)) => {
                self.pos += 1;
                Ok(Node::Lit(Value::from(n)))
            }
            Some(Tok::Float(f)) => {
                self.pos += 1;
                Ok(Node::Lit(
                    serde_json::Number::from_f64(f).map_or(Value::Null, Value::Number),
                ))
            }
            Some(Tok::Str(s)) => {
                self.pos += 1;
                Ok(Node::Lit(Value::String(s)))
            }
            Some(Tok::True) => {
                self.pos += 1;
                Ok(Node::Lit(Value::Bool(true)))
            }
            Some(Tok::False) => {
                self.pos += 1;
                Ok(Node::Lit(Value::Bool(false)))
            }
            Some(Tok::Null) => {
                self.pos += 1;
                Ok(Node::Lit(Value::Null))
            }
            Some(Tok::LBrack) => self.list(),
            Some(Tok::LParen) => {
                self.pos += 1;
                let inner = self.ternary()?;
                self.consume(&Tok::RParen, "`)` to close the group")?;
                Ok(inner)
            }
            Some(Tok::Ident(name)) => self.ident_or_call(name, span),
            _ => Err(CelError::static_err("expected an expression", span)),
        }
    }

    /// An IDENT root, OR a free-function call `size(x)` / `has(x)`.
    fn ident_or_call(&mut self, name: String, span: (usize, usize)) -> Result<Node, CelError> {
        self.pos += 1;
        if self.eat(&Tok::LParen) {
            // Free calls are a CLOSED set.
            if name != "size" && name != "has" {
                return Err(CelError::static_err(
                    format!(
                        "unknown function `{name}()` — only `size` and `has` in cel-subset/0.1"
                    ),
                    span,
                ));
            }
            let arg = self.ternary()?;
            self.consume(&Tok::RParen, "`)` to close the call")?;
            return Ok(Node::Call {
                name,
                arg: Box::new(arg),
                span,
            });
        }
        Ok(Node::Root { name, span })
    }

    // ── list = "[" [ expr { "," expr } ] "]" ────────────────────────
    fn list(&mut self) -> Result<Node, CelError> {
        self.consume(&Tok::LBrack, "`[`")?;
        let mut items = Vec::new();
        if !self.eat(&Tok::RBrack) {
            loop {
                items.push(self.ternary()?);
                if self.eat(&Tok::RBrack) {
                    break;
                }
                self.consume(&Tok::Comma, "`,` or `]` in the list")?;
            }
        }
        Ok(Node::List(items))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_simple_relation() {
        let e = parse("vars.publish == 'yes'").unwrap();
        assert!(matches!(e.node(), Node::Rel { op: RelOp::Eq, .. }));
        assert!(e.is_boolean_shaped());
        assert_eq!(e.roots(), vec!["vars".to_owned()]);
    }

    #[test]
    fn precedence_or_binds_looser_than_and() {
        // a || b && c  ==  a || (b && c)
        let e = parse("vars.a == '1' || vars.b == '2' && vars.c == '3'").unwrap();
        assert!(matches!(e.node(), Node::Or(_, rhs) if matches!(**rhs, Node::And(..))));
    }

    #[test]
    fn ternary_is_right_associative() {
        // a ? b : c ? d : e  ==  a ? b : (c ? d : e)
        let e = parse("vars.x == '1' ? 'b' : vars.y == '2' ? 'd' : 'e'").unwrap();
        let Node::Ternary { otherwise, .. } = e.node() else {
            panic!("ternary");
        };
        assert!(matches!(**otherwise, Node::Ternary { .. }));
    }

    #[test]
    fn chained_relation_is_static_error() {
        let err = parse("vars.a < vars.b < vars.c").expect_err("no chains");
        assert_eq!(err.spec_code(), "NIKA-VAR-005");
        assert!(err.message().contains("chained"));
    }

    #[test]
    fn unknown_function_is_static() {
        let err = parse("frobnicate(vars.x)").expect_err("closed fn set");
        assert_eq!(err.spec_code(), "NIKA-VAR-005");
        assert!(err.message().contains("size") && err.message().contains("has"));
    }

    #[test]
    fn unknown_method_is_static() {
        let err = parse("vars.s.frobnicate('x')").expect_err("closed method set");
        assert_eq!(err.spec_code(), "NIKA-VAR-005");
    }

    #[test]
    fn the_callable_set_parses() {
        for src in [
            "size(vars.tags) > 0",
            "has(vars.optional)",
            "vars.s.size() > 0",
            "vars.s.contains('x')",
            "vars.s.startsWith('http')",
            "vars.s.endsWith('.md')",
        ] {
            assert!(parse(src).is_ok(), "should parse: {src}");
        }
    }

    #[test]
    fn membership_and_lists_and_index() {
        let e = parse("tasks.x.status in ['success', 'skipped']").unwrap();
        assert!(matches!(e.node(), Node::Rel { op: RelOp::In, .. }));
        assert!(parse("tasks.list.output[0] == 'a'").is_ok());
        assert!(parse("obj['key-with-dash'] == 1").is_ok());
    }

    #[test]
    fn bare_reference_is_not_boolean_shaped() {
        assert!(!parse("vars.topic").unwrap().is_boolean_shaped());
        assert!(!parse("'literal'").unwrap().is_boolean_shaped());
        assert!(!parse("size(vars.tags)").unwrap().is_boolean_shaped());
        // but a relation / boolean op / has() is.
        assert!(parse("size(vars.tags) > 0").unwrap().is_boolean_shaped());
        assert!(parse("has(vars.x)").unwrap().is_boolean_shaped());
        assert!(parse("!vars.flag").unwrap().is_boolean_shaped());
    }

    #[test]
    fn roots_dedup_in_first_seen_order() {
        let e = parse("vars.a == tasks.t.output && vars.a != 'x'").unwrap();
        assert_eq!(e.roots(), vec!["vars".to_owned(), "tasks".to_owned()]);
    }

    #[test]
    fn trailing_input_is_static() {
        assert_eq!(
            parse("vars.a == 'b' extra")
                .expect_err("trailing")
                .spec_code(),
            "NIKA-VAR-005"
        );
    }

    #[test]
    fn unclosed_group_is_static() {
        assert_eq!(
            parse("(vars.a == 'b'").expect_err("unclosed").spec_code(),
            "NIKA-VAR-005"
        );
    }

    #[test]
    fn bare_literals_parse_and_advance_the_cursor() {
        // true / false / null as the WHOLE expression — pins both the
        // primary() match arm (a deleted arm → "expected an expression")
        // AND the cursor advance (a non-advancing `pos` would leave the
        // token unconsumed → "unexpected trailing input").
        for (src, want) in [
            ("true", Value::Bool(true)),
            ("false", Value::Bool(false)),
            ("null", Value::Null),
        ] {
            let e = parse(src).unwrap_or_else(|err| panic!("`{src}` parses: {err}"));
            assert!(
                matches!(e.node(), Node::Lit(v) if *v == want),
                "`{src}` is the literal {want}"
            );
        }
    }

    #[test]
    fn error_spans_point_at_the_offending_token() {
        // peek_span feeds the error's byte anchor · assert the EXACT bytes
        // so a blanked `(0,0)` / `(end,end)` span can't pass.
        // Trailing input → the stray `extra` ident (bytes 14..19).
        let err = parse("vars.a == 'b' extra").expect_err("trailing");
        assert_eq!(err.span(), (14, 19));
        // Chained relation → the SECOND `<` operator (byte 16, one wide).
        let err = parse("vars.a < vars.b < vars.c").expect_err("chain");
        assert_eq!(err.span(), (16, 17));
    }

    #[test]
    fn ternary_is_boolean_shaped_iff_both_branches_are() {
        // Both branches boolean → the whole ternary is boolean-shaped.
        assert!(
            parse("vars.a == '1' ? vars.b == '2' : vars.c == '3'")
                .unwrap()
                .is_boolean_shaped()
        );
        // One branch a bare value → NOT boolean-shaped. This pins the
        // `&&` (both must hold), not a `||` (either), AND the existence of
        // the Ternary arm at all (a deleted arm → blanket `false`).
        assert!(
            !parse("vars.a == '1' ? vars.b == '2' : 'literal'")
                .unwrap()
                .is_boolean_shaped()
        );
        assert!(
            !parse("vars.a == '1' ? 'literal' : vars.c == '3'")
                .unwrap()
                .is_boolean_shaped()
        );
    }

    #[test]
    fn deeply_nested_input_is_refused_not_a_crash() {
        // A crafted deep `(((…)))` must REFUSE with VAR-005, never overflow
        // the native stack (a non-catchable process abort). 2000 levels is
        // far past MAX_DEPTH.
        let deep = format!("{}true{}", "(".repeat(2000), ")".repeat(2000));
        assert_eq!(
            parse(&deep).expect_err("deep group").spec_code(),
            "NIKA-VAR-005"
        );
        // The `!` chain is bounded the same way.
        let bangs = format!("{}vars.a", "!".repeat(2000));
        assert_eq!(
            parse(&bangs).expect_err("deep ! chain").spec_code(),
            "NIKA-VAR-005"
        );
        // …ordinary shallow nesting still parses.
        assert!(parse("((vars.a == 'x'))").is_ok());
    }

    #[test]
    fn a_wide_flat_chain_is_refused_not_a_crash() {
        // A WIDE flat boolean chain (`a || a || …`) is iterative to parse
        // but builds a LEFT-NESTED tree that the COMPUTER recurses — a 30k
        // chain overflowed the native stack at eval time. The parser now
        // caps the chain depth (BUG#1b), so it REFUSES with VAR-005 here,
        // reached by BOTH `check` and `run`, never handing eval a deep tree.
        let wide_or = std::iter::repeat_n("vars.a", 30_000)
            .collect::<Vec<_>>()
            .join(" || ");
        assert_eq!(
            parse(&wide_or).expect_err("wide || chain").spec_code(),
            "NIKA-VAR-005"
        );
        // `&&` is the dual DoS — capped identically.
        let wide_and = std::iter::repeat_n("vars.a", 30_000)
            .collect::<Vec<_>>()
            .join(" && ");
        assert_eq!(
            parse(&wide_and).expect_err("wide && chain").spec_code(),
            "NIKA-VAR-005"
        );
        // …a NORMAL-width chain (well within the cap) still parses fine.
        let ok = std::iter::repeat_n("vars.a == 'x'", 10)
            .collect::<Vec<_>>()
            .join(" || ");
        assert!(
            parse(&ok).is_ok(),
            "a 10-term chain is a fine real workflow"
        );
    }
}
