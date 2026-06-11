// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Expression AST — the CEL v0.1 subset (spec `03-dag.md` §CEL-subset ·
//! grammar version `cel-subset/0.1`).

/// A parsed CEL v0.1-subset expression.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Expr {
    /// `a || b` — logical or (left-associative fold).
    Or(Box<Expr>, Box<Expr>),
    /// `a && b` — logical and (left-associative fold).
    And(Box<Expr>, Box<Expr>),
    /// `!a` — logical negation.
    Not(Box<Expr>),
    /// `a <op> b` — a single relation (« relations do not chain »).
    Relation {
        /// The relational operator.
        op: RelOp,
        /// Left operand.
        lhs: Box<Expr>,
        /// Right operand.
        rhs: Box<Expr>,
    },
    /// `cond ? a : b` — conditional value selection (right-associative ·
    /// loosest precedence · `cond` MUST be boolean-shaped). Value
    /// selection, not a relation — `then`/`else_` may be any value.
    Ternary {
        /// The boolean condition.
        cond: Box<Expr>,
        /// The value when `cond` is true.
        then: Box<Expr>,
        /// The value when `cond` is false.
        else_: Box<Expr>,
    },
    /// `size(x)` — free form · « the ONE v0.1 function ».
    SizeCall(Box<Expr>),
    /// `has(x)` — the presence macro · `true` iff `x` resolves to a
    /// defined, non-`null` value. Boolean-shaped.
    HasCall(Box<Expr>),
    /// `x.contains(s)` · `x.startsWith(s)` · `x.endsWith(s)` — the 1-arg
    /// string-predicate methods (case-sensitive · boolean-shaped).
    StringMethod {
        /// The string receiver.
        base: Box<Expr>,
        /// Which predicate.
        method: StringPredicate,
        /// The argument string expression.
        arg: Box<Expr>,
    },
    /// `x.size()` — method form · exactly 0 arguments.
    SizeMethod(Box<Expr>),
    /// `base.field` — member access.
    Member {
        /// The accessed base.
        base: Box<Expr>,
        /// The member name.
        field: String,
    },
    /// `base[index]` — index access (int or string index per grammar).
    Index {
        /// The accessed base.
        base: Box<Expr>,
        /// The index expression.
        index: Box<Expr>,
    },
    /// A root identifier (`vars` · `tasks` · `item` · …).
    Ident(String),
    /// `[a, b, c]` — list literal.
    List(Vec<Expr>),
    /// A scalar literal.
    Lit(Literal),
}

/// A scalar literal in the v0.1 subset.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Literal {
    /// `true` / `false`.
    Bool(bool),
    /// `42` / `-7`.
    Int(i64),
    /// `3.5` / `-0.25`.
    Float(f64),
    /// `'str'` / `"str"` (escapes resolved).
    Str(String),
    /// `null`.
    Null,
}

/// The 1-arg string predicates (`cel-subset/0.1` · boolean-valued).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StringPredicate {
    /// `x.contains(s)` — substring test.
    Contains,
    /// `x.startsWith(s)` — prefix test.
    StartsWith,
    /// `x.endsWith(s)` — suffix test.
    EndsWith,
}

impl StringPredicate {
    /// The method name in source.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::StartsWith => "startsWith",
            Self::EndsWith => "endsWith",
        }
    }
}

/// The relational operators (spec EBNF `relop`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RelOp {
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `in` — membership.
    In,
}

impl RelOp {
    /// The operator's source text.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Eq => "==",
            Self::Ne => "!=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::In => "in",
        }
    }
}

/// A classified root reference found in an expression (spec
/// `04-variables.md` §Resolution order · the 5 namespaces + the 2
/// `for_each` loop-locals).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NamespaceRef {
    /// `vars.<name>` — workflow inputs.
    Vars(String),
    /// `with.<name>` — task-scope injections.
    With(String),
    /// `tasks.<id>[.<field>]` — an upstream task's result record.
    Tasks {
        /// The referenced task id.
        id: String,
        /// The accessed result-record field (`output` · `status` · …)
        /// when one is named.
        field: Option<String>,
    },
    /// `env.<name>` — environment variables.
    Env(String),
    /// `secrets.<name>` — secret references.
    Secrets(String),
    /// `item` — the current `for_each` element (loop-local).
    Item,
    /// `index` — the current `for_each` position (loop-local).
    Index,
    /// Any other root identifier (unresolvable · `NIKA-VAR-001`).
    Unknown(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relop_as_str_round_trip() {
        for (op, s) in [
            (RelOp::Eq, "=="),
            (RelOp::Ne, "!="),
            (RelOp::Lt, "<"),
            (RelOp::Le, "<="),
            (RelOp::Gt, ">"),
            (RelOp::Ge, ">="),
            (RelOp::In, "in"),
        ] {
            assert_eq!(op.as_str(), s);
        }
    }

    #[test]
    fn expr_is_clone_and_debug() {
        let e = Expr::And(
            Box::new(Expr::Ident("a".into())),
            Box::new(Expr::Lit(Literal::Bool(true))),
        );
        let cloned = e.clone();
        assert_eq!(e, cloned);
        assert!(format!("{e:?}").contains("And"));
    }
}
