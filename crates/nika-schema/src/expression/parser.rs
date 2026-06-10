// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Recursive-descent parser for the CEL v0.1 subset.
//!
//! Implements EXACTLY the normative EBNF of spec `03-dag.md`
//! (`cel-subset/0.1`) · precedence tightest→loosest · postfix (`.` `[]`)
//! → `!` → relational → `&&` → `||`. Relations are non-associative
//! (« `a < b < c` is a parse error ») and `size` is the only callable.

use super::ast::{Expr, Literal, RelOp};
use super::error::ExprError;
use super::lexer::{Token, TokenKind, tokenize};

/// Parse a CEL v0.1-subset expression (the inside of a `${{ }}` island).
///
/// # Errors
///
/// Returns [`ExprError`] on any deviation from the `cel-subset/0.1`
/// grammar — unknown characters · unterminated strings · chained
/// relations · non-`size` calls · trailing input.
pub fn parse_expression(src: &str) -> Result<Expr, ExprError> {
    let tokens = tokenize(src)?;
    let mut parser = Parser { tokens, pos: 0 };
    if parser.peek_kind() == &TokenKind::Eof {
        return Err(ExprError::EmptyExpression);
    }
    let expr = parser.or_expr()?;
    match parser.peek_kind() {
        TokenKind::Eof => Ok(expr),
        _ => Err(ExprError::TrailingInput {
            offset: parser.peek_offset(),
        }),
    }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> &Token {
        // The token stream always ends with Eof; clamp defensively.
        self.tokens
            .get(self.pos)
            .unwrap_or_else(|| &self.tokens[self.tokens.len() - 1])
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    fn peek_offset(&self) -> usize {
        self.peek().offset
    }

    fn advance(&mut self) -> Token {
        let token = self.peek().clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        token
    }

    fn expect(&mut self, kind: &TokenKind, expected: &str) -> Result<(), ExprError> {
        if self.peek_kind() == kind {
            self.advance();
            Ok(())
        } else {
            Err(ExprError::UnexpectedToken {
                found: self.peek_kind().describe(),
                expected: expected.to_owned(),
                offset: self.peek_offset(),
            })
        }
    }

    /// `or = and { "||" and }` — left-associative.
    fn or_expr(&mut self) -> Result<Expr, ExprError> {
        let mut lhs = self.and_expr()?;
        while self.peek_kind() == &TokenKind::OrOr {
            self.advance();
            let rhs = self.and_expr()?;
            lhs = Expr::Or(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    /// `and = rel { "&&" rel }` — left-associative.
    fn and_expr(&mut self) -> Result<Expr, ExprError> {
        let mut lhs = self.rel_expr()?;
        while self.peek_kind() == &TokenKind::AndAnd {
            self.advance();
            let rhs = self.rel_expr()?;
            lhs = Expr::And(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    /// `rel = unary [ relop unary ]` — AT MOST one relation (spec side
    /// constraint 3 · « Relations do not chain »).
    fn rel_expr(&mut self) -> Result<Expr, ExprError> {
        let lhs = self.unary_expr()?;
        let Some(op) = rel_op(self.peek_kind()) else {
            return Ok(lhs);
        };
        self.advance();
        let rhs = self.unary_expr()?;
        // A SECOND relational operator here is the chained-relation
        // parse error (`a < b < c` · `a == b == c`).
        if rel_op(self.peek_kind()).is_some() {
            return Err(ExprError::ChainedRelation {
                offset: self.peek_offset(),
            });
        }
        Ok(Expr::Relation {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        })
    }

    /// `unary = { "!" } postfix`.
    fn unary_expr(&mut self) -> Result<Expr, ExprError> {
        if self.peek_kind() == &TokenKind::Bang {
            self.advance();
            let inner = self.unary_expr()?;
            return Ok(Expr::Not(Box::new(inner)));
        }
        self.postfix_expr()
    }

    /// `postfix = primary { "." IDENT [ "(" ")" ] | "[" expr "]" }`.
    ///
    /// The only method form is `.size()` (0 args) — any other `.name()`
    /// is [`ExprError::UnknownFunction`].
    fn postfix_expr(&mut self) -> Result<Expr, ExprError> {
        let mut base = self.primary_expr()?;
        loop {
            match self.peek_kind() {
                TokenKind::Dot => {
                    self.advance();
                    let name_token = self.advance();
                    let TokenKind::Ident(name) = name_token.kind else {
                        return Err(ExprError::UnexpectedToken {
                            found: name_token.kind.describe(),
                            expected: "member name".to_owned(),
                            offset: name_token.offset,
                        });
                    };
                    if self.peek_kind() == &TokenKind::LParen {
                        // Method-call form · only `size()` exists.
                        if name != "size" {
                            return Err(ExprError::UnknownFunction {
                                name,
                                offset: name_token.offset,
                            });
                        }
                        self.advance(); // (
                        self.expect(&TokenKind::RParen, "`)` — `x.size()` takes no arguments")?;
                        base = Expr::SizeMethod(Box::new(base));
                    } else {
                        base = Expr::Member {
                            base: Box::new(base),
                            field: name,
                        };
                    }
                }
                TokenKind::LBracket => {
                    self.advance();
                    let index = self.or_expr()?;
                    self.expect(&TokenKind::RBracket, "`]`")?;
                    base = Expr::Index {
                        base: Box::new(base),
                        index: Box::new(index),
                    };
                }
                _ => return Ok(base),
            }
        }
    }

    /// `primary = literal | list | call | IDENT | "(" expr ")"`.
    fn primary_expr(&mut self) -> Result<Expr, ExprError> {
        let token = self.advance();
        match token.kind {
            TokenKind::True => Ok(Expr::Lit(Literal::Bool(true))),
            TokenKind::False => Ok(Expr::Lit(Literal::Bool(false))),
            TokenKind::Null => Ok(Expr::Lit(Literal::Null)),
            TokenKind::Int(v) => Ok(Expr::Lit(Literal::Int(v))),
            TokenKind::Float(v) => Ok(Expr::Lit(Literal::Float(v))),
            TokenKind::Str(s) => Ok(Expr::Lit(Literal::Str(s))),
            TokenKind::Ident(name) => {
                if self.peek_kind() == &TokenKind::LParen {
                    // Free-call form · `size(expr)` is the only callable.
                    if name != "size" {
                        return Err(ExprError::UnknownFunction {
                            name,
                            offset: token.offset,
                        });
                    }
                    self.advance(); // (
                    let arg = self.or_expr()?;
                    self.expect(
                        &TokenKind::RParen,
                        "`)` — `size(x)` takes exactly 1 argument",
                    )?;
                    Ok(Expr::SizeCall(Box::new(arg)))
                } else {
                    Ok(Expr::Ident(name))
                }
            }
            TokenKind::LParen => {
                let inner = self.or_expr()?;
                self.expect(&TokenKind::RParen, "`)`")?;
                Ok(inner)
            }
            TokenKind::LBracket => {
                let mut items = Vec::new();
                if self.peek_kind() != &TokenKind::RBracket {
                    loop {
                        items.push(self.or_expr()?);
                        if self.peek_kind() == &TokenKind::Comma {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                }
                self.expect(&TokenKind::RBracket, "`]`")?;
                Ok(Expr::List(items))
            }
            other => Err(ExprError::UnexpectedToken {
                found: other.describe(),
                expected: "an expression".to_owned(),
                offset: token.offset,
            }),
        }
    }
}

/// Map a relational token to its operator.
fn rel_op(kind: &TokenKind) -> Option<RelOp> {
    match kind {
        TokenKind::EqEq => Some(RelOp::Eq),
        TokenKind::NotEq => Some(RelOp::Ne),
        TokenKind::Lt => Some(RelOp::Lt),
        TokenKind::Le => Some(RelOp::Le),
        TokenKind::Gt => Some(RelOp::Gt),
        TokenKind::Ge => Some(RelOp::Ge),
        TokenKind::In => Some(RelOp::In),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // ── Spec §when valid examples (03-dag.md · verbatim) ────────────

    #[test]
    fn spec_when_valid_examples_parse() {
        for src in [
            "tasks.build.status == 'success'",
            "tasks.test.output.coverage > 80",
            "vars.env == 'production'",
            "tasks.a.status == 'success' && tasks.b.status == 'success'",
            "tasks.deploy.status in ['success', 'skipped']",
            "!(tasks.test.status == 'failure')",
            "vars.env == \"production\"",
            "tasks.upstream.status == \"success\"",
            "tasks.scan.alerts.size() > 0",
            "vars.dry_run == false && tasks.check.passed",
            "tasks.X.output != null",
            "vars.threshold > 0",
            "vars.message != \"\"",
            "size(vars.items) > 0",
            "tasks.check.alert_count > 0",
        ] {
            assert!(parse_expression(src).is_ok(), "should parse: {src}");
        }
    }

    #[test]
    fn spec_subset_feature_examples_parse() {
        // 03-dag.md §CEL-subset feature list.
        for src in [
            "vars.topic",
            "tasks.build.status",
            "with.content",
            "tasks.list.output[0]",
            "obj['key-with-dash']",
            "status in ['success','skipped']",
            "size(coll)",
            "coll.size()",
            "true",
            "false",
            "42",
            "2.5",
            "'str'",
            "\"str\"",
            "null",
            "(vars.a)",
        ] {
            assert!(parse_expression(src).is_ok(), "should parse: {src}");
        }
    }

    // ── Structure assertions ────────────────────────────────────────

    #[test]
    fn relation_structure() {
        let e = parse_expression("tasks.build.status == 'success'").expect("parse");
        let Expr::Relation { op, lhs, rhs } = e else {
            panic!("expected Relation");
        };
        assert_eq!(op, RelOp::Eq);
        assert_eq!(
            *lhs,
            Expr::Member {
                base: Box::new(Expr::Member {
                    base: Box::new(Expr::Ident("tasks".into())),
                    field: "build".into(),
                }),
                field: "status".into(),
            }
        );
        assert_eq!(*rhs, Expr::Lit(Literal::Str("success".into())));
    }

    #[test]
    fn precedence_or_loosest() {
        // a && b || c && d  ⇒  (a && b) || (c && d)
        let e = parse_expression("a && b || c && d").expect("parse");
        let Expr::Or(lhs, rhs) = e else {
            panic!("root must be Or");
        };
        assert!(matches!(*lhs, Expr::And(_, _)));
        assert!(matches!(*rhs, Expr::And(_, _)));
    }

    #[test]
    fn precedence_not_tighter_than_relation() {
        // !a == b  ⇒  (!a) == b   (unary binds tighter than relop)
        let e = parse_expression("!a == b").expect("parse");
        let Expr::Relation {
            op: RelOp::Eq, lhs, ..
        } = e
        else {
            panic!("root must be Eq relation");
        };
        assert!(matches!(*lhs, Expr::Not(_)));
    }

    #[test]
    fn parens_override_precedence() {
        // a && (b || c)  keeps Or inside And.
        let e = parse_expression("a && (b || c)").expect("parse");
        let Expr::And(_, rhs) = e else {
            panic!("root must be And");
        };
        assert!(matches!(*rhs, Expr::Or(_, _)));
    }

    #[test]
    fn double_negation_nests() {
        let e = parse_expression("!!a").expect("parse");
        let Expr::Not(inner) = e else {
            panic!("root Not");
        };
        assert!(matches!(*inner, Expr::Not(_)));
    }

    #[test]
    fn index_access_int_and_string() {
        let e = parse_expression("tasks.list.output[0]").expect("parse");
        assert!(matches!(e, Expr::Index { .. }));
        let e = parse_expression("obj['key-with-dash']").expect("parse");
        let Expr::Index { index, .. } = e else {
            panic!("Index");
        };
        assert_eq!(*index, Expr::Lit(Literal::Str("key-with-dash".into())));
    }

    #[test]
    fn size_method_zero_args() {
        let e = parse_expression("tasks.scan.alerts.size()").expect("parse");
        assert!(matches!(e, Expr::SizeMethod(_)));
    }

    #[test]
    fn member_named_size_without_parens_is_field() {
        // `.size` (no parens) is plain member access — the grammar's
        // `[ "(" ")" ]` is optional.
        let e = parse_expression("vars.size").expect("parse");
        assert!(matches!(e, Expr::Member { .. }));
    }

    // ── Spec parse-error cases ──────────────────────────────────────

    #[test]
    fn chained_relation_is_parse_error() {
        // Side constraint 3 · « `a < b < c` is a parse error ».
        for src in ["a < b < c", "a == b == c", "1 <= x >= 2", "a in b in c"] {
            let err = parse_expression(src).expect_err("must reject chain");
            assert!(
                matches!(err, ExprError::ChainedRelation { .. }),
                "{src} → {err:?}"
            );
        }
    }

    #[test]
    fn non_size_calls_are_errors() {
        // Side constraint 1 · « The only callable is `size` ».
        for src in [
            "has(vars.x)",
            "vars.items.all()",
            "matches(s)",
            "x.startsWith()",
        ] {
            let err = parse_expression(src).expect_err("must reject call");
            assert!(
                matches!(err, ExprError::UnknownFunction { .. }),
                "{src} → {err:?}"
            );
        }
    }

    #[test]
    fn size_method_with_arg_is_error() {
        let err = parse_expression("x.size(1)").expect_err("size() takes 0 args");
        assert!(matches!(err, ExprError::UnexpectedToken { .. }), "{err:?}");
    }

    #[test]
    fn arithmetic_is_reserved() {
        // « Everything else is reserved · arithmetic … » — `+` is not
        // in the token alphabet (standalone `-` neither).
        assert!(parse_expression("a + b").is_err());
        assert!(parse_expression("a - b").is_err());
        assert!(parse_expression("a * b").is_err());
    }

    #[test]
    fn empty_expression_is_error() {
        assert_eq!(parse_expression(""), Err(ExprError::EmptyExpression));
        assert_eq!(parse_expression("   "), Err(ExprError::EmptyExpression));
    }

    #[test]
    fn trailing_input_is_error() {
        let err = parse_expression("a == b c").expect_err("trailing ident");
        assert!(matches!(err, ExprError::TrailingInput { .. }), "{err:?}");
    }

    #[test]
    fn unbalanced_parens_error() {
        assert!(parse_expression("(a == b").is_err());
        assert!(parse_expression("a == b)").is_err());
        assert!(parse_expression("[1, 2").is_err());
    }

    #[test]
    fn reserved_words_are_not_idents() {
        // `true.x` — literals take no member in practice but the
        // grammar's postfix applies to any primary; what must NOT
        // happen is `true` lexing as Ident.
        let e = parse_expression("true").expect("parse");
        assert_eq!(e, Expr::Lit(Literal::Bool(true)));
        // `in` alone is not an expression.
        assert!(parse_expression("in").is_err());
    }

    #[test]
    fn empty_list_parses() {
        let e = parse_expression("x in []").expect("parse");
        let Expr::Relation {
            op: RelOp::In, rhs, ..
        } = e
        else {
            panic!("In relation");
        };
        assert_eq!(*rhs, Expr::List(vec![]));
    }

    // ── Robustness ──────────────────────────────────────────────────

    proptest! {
        #[test]
        fn parse_never_panics(src in ".{0,64}") {
            // Any input · Ok or Err · never a panic.
            let _ = parse_expression(&src);
        }

        #[test]
        fn parse_never_panics_grammar_alphabet(src in "[a-z0-9_.\\[\\]()'\"!=<>&|, ]{0,48}") {
            let _ = parse_expression(&src);
        }
    }
}
