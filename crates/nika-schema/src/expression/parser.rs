// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Recursive-descent parser for the CEL v0.1 subset.
//!
//! Implements EXACTLY the normative EBNF of spec `03-dag.md`
//! (`cel-subset/0.1`) · precedence tightest→loosest · postfix (`.` `[]`)
//! → `!` → relational → `&&` → `||`. Relations are non-associative
//! (« `a < b < c` is a parse error ») and `size` is the only callable.

use super::ast::{Expr, Literal, RelOp, StringPredicate};
use super::error::ExprError;
use super::lexer::{Token, TokenKind, tokenize};

/// The maximum AST nesting depth the checker's parser will build before
/// refusing. Mirrors the runtime engine's `nika_cel` cap (one shared limit,
/// one shared class). Two shapes overflow the native stack when a later
/// walker (`expr_refs` · the schema-typing pass) recurses the tree:
///
/// 1. deep grouping — `${{ ((((…)))) }}`; and
/// 2. a wide FLAT chain — `${{ a || a || … }}` (30k terms), which the
///    iterative `or`/`and` loops build as a 30k-deep LEFT-NESTED tree.
///
/// Both refuse at parse here, so `nika check`/`inspect`/`graph` never crash
/// and never hand a pathological AST to the analyzers. Mirrors the runtime
/// `nika_cel::parser::MAX_DEPTH` (the static-subset class · NIKA-VAR-005).
const MAX_DEPTH: usize = 128;

/// Parse a CEL v0.1-subset expression (the inside of a `${{ }}` island).
///
/// # Errors
///
/// Returns [`ExprError`] on any deviation from the `cel-subset/0.1`
/// grammar — unknown characters · unterminated strings · chained
/// relations · non-`size` calls · trailing input · OR an expression that
/// nests beyond the depth cap ([`ExprError::TooDeep`] · the
/// stack-overflow-class refusal, mirroring the runtime).
pub fn parse_expression(src: &str) -> Result<Expr, ExprError> {
    let tokens = tokenize(src)?;
    let mut parser = Parser {
        tokens,
        pos: 0,
        depth: 0,
    };
    if parser.peek_kind() == &TokenKind::Eof {
        return Err(ExprError::EmptyExpression);
    }
    let expr = parser.ternary_expr()?;
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
    /// Current recursive-descent / chain nesting depth (capped at
    /// [`MAX_DEPTH`] so a crafted deep expression refuses instead of
    /// overflowing the native stack downstream).
    depth: usize,
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

    fn expect_token(&mut self, kind: &TokenKind, expected: &str) -> Result<(), ExprError> {
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

    /// Increment the nesting depth, refusing past [`MAX_DEPTH`]. The ONE
    /// guard the recursive rungs + the chain loops share.
    fn enter(&mut self) -> Result<(), ExprError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(ExprError::TooDeep {
                offset: self.peek_offset(),
                limit: MAX_DEPTH,
            });
        }
        Ok(())
    }

    /// `ternary = or [ "?" expr ":" ternary ]` — right-associative ·
    /// loosest precedence (spec `cel-subset/0.1`). Value selection.
    fn ternary_expr(&mut self) -> Result<Expr, ExprError> {
        self.enter()?;
        let cond = self.or_expr()?;
        let out = if self.peek_kind() == &TokenKind::Question {
            self.advance(); // ?
            let then = self.ternary_expr()?;
            self.expect_token(&TokenKind::Colon, "`:` — the ternary else-branch")?;
            let else_ = self.ternary_expr()?;
            Expr::Ternary {
                cond: Box::new(cond),
                then: Box::new(then),
                else_: Box::new(else_),
            }
        } else {
            cond
        };
        self.depth -= 1;
        Ok(out)
    }

    /// `or = and { "||" and }` — left-associative. The loop is ITERATIVE
    /// (no parse-stack growth), but each `||` adds one `Expr::Or` nesting
    /// level a later walker recurses — so a wide flat chain is depth-capped
    /// here, never building a stack-overflowing tree.
    fn or_expr(&mut self) -> Result<Expr, ExprError> {
        let entry = self.depth;
        let mut lhs = self.and_expr()?;
        while self.peek_kind() == &TokenKind::OrOr {
            self.enter()?;
            self.advance();
            let rhs = self.and_expr()?;
            lhs = Expr::Or(Box::new(lhs), Box::new(rhs));
        }
        self.depth = entry; // the chain is one returned subtree · restore
        Ok(lhs)
    }

    /// `and = rel { "&&" rel }` — left-associative. Same flat-chain depth
    /// cap as [`Self::or_expr`] (a wide `&&` chain is the dual `DoS`).
    fn and_expr(&mut self) -> Result<Expr, ExprError> {
        let entry = self.depth;
        let mut lhs = self.rel_expr()?;
        while self.peek_kind() == &TokenKind::AndAnd {
            self.enter()?;
            self.advance();
            let rhs = self.rel_expr()?;
            lhs = Expr::And(Box::new(lhs), Box::new(rhs));
        }
        self.depth = entry; // restore — siblings get a fresh budget
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

    /// `unary = { "!" } postfix`. A `!` chain (`!!!…x`) nests `Expr::Not`,
    /// which a walker recurses — so the rung is depth-capped too.
    fn unary_expr(&mut self) -> Result<Expr, ExprError> {
        if self.peek_kind() == &TokenKind::Bang {
            self.enter()?;
            self.advance();
            let inner = self.unary_expr()?;
            self.depth -= 1;
            return Ok(Expr::Not(Box::new(inner)));
        }
        self.postfix_expr()
    }

    /// `postfix = primary { "." IDENT [ "(" ")" ] | "[" expr "]" }`.
    ///
    /// The only method form is `.size()` (0 args) — any other `.name()`
    /// is [`ExprError::UnknownFunction`].
    fn postfix_expr(&mut self) -> Result<Expr, ExprError> {
        // Each `.field` / `[idx]` segment builds one `Expr::Member`/`Expr::Index`
        // nesting that a later walker (`analyzer::schema_paths` · `check::reach` ·
        // the `Box<Expr>` Drop) recurses ONE frame deep. Cap the chain length
        // with the shared `enter()` guard — the postfix sibling of the or/and/
        // ternary rungs — so an unbounded `a.a.a.…` (or `a['a']['a']…`) returns
        // `TooDeep` instead of overflowing the stack. Untrusted callers reach
        // this via `nika-mcp` `nika_check`; the cap is the trust boundary.
        let entry = self.depth;
        let mut base = self.primary_expr()?;
        loop {
            match self.peek_kind() {
                TokenKind::Dot => {
                    self.enter()?;
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
                        base = self.parse_method_call(base, &name, name_token.offset)?;
                    } else {
                        base = Expr::Member {
                            base: Box::new(base),
                            field: name,
                        };
                    }
                }
                TokenKind::LBracket => {
                    self.enter()?;
                    self.advance();
                    let index = self.ternary_expr()?;
                    self.expect_token(&TokenKind::RBracket, "`]`")?;
                    base = Expr::Index {
                        base: Box::new(base),
                        index: Box::new(index),
                    };
                }
                // The chain is ONE returned subtree from the caller's view ·
                // restore the depth budget (like `or_expr`).
                _ => {
                    self.depth = entry;
                    return Ok(base);
                }
            }
        }
    }

    /// A method-call form after `base.<name>` · the closing 0-arg
    /// `.size()` or a 1-arg string predicate (`contains`/`startsWith`/
    /// `endsWith`). Any other name is [`ExprError::UnknownFunction`]
    /// (`matches` stays reserved · `ReDoS`).
    fn parse_method_call(
        &mut self,
        base: Expr,
        name: &str,
        offset: usize,
    ) -> Result<Expr, ExprError> {
        self.advance(); // (
        if name == "size" {
            self.expect_token(&TokenKind::RParen, "`)` — `x.size()` takes no arguments")?;
            return Ok(Expr::SizeMethod(Box::new(base)));
        }
        let method = match name {
            "contains" => StringPredicate::Contains,
            "startsWith" => StringPredicate::StartsWith,
            "endsWith" => StringPredicate::EndsWith,
            _ => {
                return Err(ExprError::UnknownFunction {
                    name: name.to_owned(),
                    offset,
                });
            }
        };
        let arg = self.ternary_expr()?;
        self.expect_token(
            &TokenKind::RParen,
            "`)` — the string predicate takes exactly 1 argument",
        )?;
        Ok(Expr::StringMethod {
            base: Box::new(base),
            method,
            arg: Box::new(arg),
        })
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
                    // Free-call form · `size(expr)` and `has(expr)`.
                    let ctor: fn(Box<Expr>) -> Expr = match name.as_str() {
                        "size" => Expr::SizeCall,
                        "has" => Expr::HasCall,
                        _ => {
                            return Err(ExprError::UnknownFunction {
                                name,
                                offset: token.offset,
                            });
                        }
                    };
                    self.advance(); // (
                    let arg = self.ternary_expr()?;
                    self.expect_token(
                        &TokenKind::RParen,
                        "`)` — the free call takes exactly 1 argument",
                    )?;
                    Ok(ctor(Box::new(arg)))
                } else {
                    Ok(Expr::Ident(name))
                }
            }
            TokenKind::LParen => {
                let inner = self.ternary_expr()?;
                self.expect_token(&TokenKind::RParen, "`)`")?;
                Ok(inner)
            }
            TokenKind::LBracket => {
                let mut items = Vec::new();
                if self.peek_kind() != &TokenKind::RBracket {
                    loop {
                        // `list = "[" [ expr {"," expr} ] "]"` · expr = ternary
                        // — a ternary element is grammar-valid, the cascade
                        // must reach list elements too (not stop at or_expr).
                        items.push(self.ternary_expr()?);
                        if self.peek_kind() == &TokenKind::Comma {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                }
                self.expect_token(&TokenKind::RBracket, "`]`")?;
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
    fn reserved_calls_are_still_errors() {
        // cel-subset/0.1 closed-call set · `all`/`exists`/`matches` stay
        // reserved (ReDoS for matches) — still UnknownFunction.
        for src in ["vars.items.all()", "matches(s)", "s.matches('x')", "foo(1)"] {
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

    #[test]
    fn deeply_nested_input_is_refused_not_a_crash() {
        // BUG#1a: the checker's parser had NO depth bound — deep `((((…))))`
        // crashed `nika check`/`inspect`/`graph` (stack overflow, exit 134).
        // It now REFUSES with TooDeep (a clean finding), never overflows.
        let deep = format!("{}true{}", "(".repeat(6500), ")".repeat(6500));
        assert!(
            matches!(parse_expression(&deep), Err(ExprError::TooDeep { .. })),
            "deep grouping must refuse, not crash"
        );
        // A `!` chain is bounded the same way.
        let bangs = format!("{}vars.a", "!".repeat(6500));
        assert!(
            matches!(parse_expression(&bangs), Err(ExprError::TooDeep { .. })),
            "deep ! chain must refuse"
        );
        // …ordinary shallow nesting still parses.
        assert!(parse_expression("((vars.a == 'x'))").is_ok());
    }

    #[test]
    fn a_wide_flat_chain_is_refused_not_a_crash() {
        // BUG#1b: a wide flat `a || a || …` chain parses iteratively but
        // builds a deep LEFT-NESTED tree the analyzers (`expr_refs`,
        // schema-typing) recurse — overflowing the stack during `check`.
        // The chain depth is now capped → a clean TooDeep refusal.
        let wide = std::iter::repeat_n("vars.a", 30_000)
            .collect::<Vec<_>>()
            .join(" || ");
        assert!(
            matches!(parse_expression(&wide), Err(ExprError::TooDeep { .. })),
            "wide || chain must refuse, not crash a downstream walker"
        );
        let wide_and = std::iter::repeat_n("vars.a", 30_000)
            .collect::<Vec<_>>()
            .join(" && ");
        assert!(
            matches!(parse_expression(&wide_and), Err(ExprError::TooDeep { .. })),
            "wide && chain refuses too"
        );
        // …a normal-width chain (a real workflow's `when:`) still parses.
        let ok = std::iter::repeat_n("vars.a == 'x'", 12)
            .collect::<Vec<_>>()
            .join(" || ");
        assert!(parse_expression(&ok).is_ok(), "a 12-term chain is fine");
    }

    #[test]
    fn a_long_postfix_chain_is_refused_not_a_crash() {
        // BUG#1c (security swarm 2026-06-16): postfix `.field`/`[idx]` chains
        // had NO depth bound — `vars.a.a.a.…` built a deep left-nested
        // Member/Index tree the walkers (schema_paths · reach · Box<Expr> Drop)
        // recurse, overflowing the stack. nika-mcp `nika_check` newly exposes
        // this to UNTRUSTED clients. Now capped → a clean TooDeep refusal.
        let member = format!("vars{}", ".a".repeat(30_000));
        assert!(
            matches!(parse_expression(&member), Err(ExprError::TooDeep { .. })),
            "a long .field chain must refuse, not crash a downstream walker"
        );
        let index = format!("vars{}", "[0]".repeat(30_000));
        assert!(
            matches!(parse_expression(&index), Err(ExprError::TooDeep { .. })),
            "a long [idx] chain refuses too"
        );
        // …a real workflow's path depth still parses.
        assert!(parse_expression("tasks.fetch.output.users").is_ok());
    }

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

#[cfg(test)]
mod cel_v01_additions {
    use super::*;
    use crate::expression::ast::{Expr, StringPredicate};

    #[test]
    fn ternary_parses_right_associative() {
        let e = parse_expression("vars.env == 'prod' ? 'a' : 'b'").expect("ternary");
        assert!(matches!(e, Expr::Ternary { .. }));
        // chained else: a ? b : c ? d : e  ==  a ? b : (c ? d : e)
        let e = parse_expression("a ? b : c ? d : e").expect("chained");
        let Expr::Ternary { else_, .. } = e else {
            panic!("expected ternary")
        };
        assert!(matches!(*else_, Expr::Ternary { .. }), "right-associative");
    }

    #[test]
    fn ternary_is_loosest_precedence() {
        // `x || y ? a : b` parses as `(x || y) ? a : b`
        let e = parse_expression("x || y ? a : b").expect("prec");
        let Expr::Ternary { cond, .. } = e else {
            panic!("ternary at top")
        };
        assert!(matches!(*cond, Expr::Or(_, _)), "|| binds tighter than ?:");
    }

    #[test]
    fn has_is_a_free_call() {
        let e = parse_expression("has(vars.style)").expect("has");
        assert!(matches!(e, Expr::HasCall(_)));
    }

    #[test]
    fn string_predicates_parse() {
        for (src, want) in [
            ("s.contains('x')", StringPredicate::Contains),
            ("s.startsWith('x')", StringPredicate::StartsWith),
            ("s.endsWith('x')", StringPredicate::EndsWith),
        ] {
            let e = parse_expression(src).expect(src);
            let Expr::StringMethod { method, .. } = e else {
                panic!("{src} → not a string method")
            };
            assert_eq!(method, want, "{src}");
        }
    }

    #[test]
    fn string_predicate_needs_one_arg() {
        assert!(parse_expression("s.contains()").is_err(), "0 args rejected");
        assert!(
            parse_expression("s.contains('a', 'b')").is_err(),
            "2 args rejected"
        );
    }

    #[test]
    fn ternary_inside_list_literal() {
        // `[cond ? a : b, c]` — a ternary element is grammar-valid
        // (regression: the cascade once stopped at or_expr for list elems).
        let e = parse_expression("[vars.x == 1 ? 'a' : 'b', 'c']").expect("list ternary");
        assert!(matches!(e, Expr::List(_)));
    }

    #[test]
    fn double_colon_is_not_chained() {
        // a missing else-branch is a parse error, never silently accepted
        assert!(parse_expression("a ? b").is_err(), "incomplete ternary");
    }
}
