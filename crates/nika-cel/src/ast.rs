// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `cel-subset/0.1` AST — one node enum per the EBNF productions.
//!
//! Every node carries the byte span of the source it came from (for the
//! host's diagnostics). The tree is the shared contract: the checker
//! walks it ([`Expr::roots`] · [`Expr::is_boolean_shaped`]) and the
//! computer evaluates it (the `compute` module).

use serde_json::Value;

/// A relational operator (the closed set · grammar `relop`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RelOp {
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
    /// `in` (membership)
    In,
}

/// A postfix navigation step (`.field` · `[index]` · a method call).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Step {
    /// `.field` — object member access.
    Field(String),
    /// `[expr]` — index/key access (the index is itself an expression).
    Index(Box<Node>),
    /// `.method(args)` — the closed method set (`size` · `contains` ·
    /// `startsWith` · `endsWith`).
    Method { name: String, args: Vec<Node> },
}

/// One AST node (an expression).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Node {
    /// A literal value (`42` · `'s'` · `true` · `null` · a list).
    Lit(Value),
    /// A list literal `[a, b, c]` (its elements are expressions).
    List(Vec<Node>),
    /// A namespace root identifier (`vars` · `tasks` · `item` · …).
    Root { name: String, span: (usize, usize) },
    /// A postfix chain over a base (`base . field [i] . method(...)`).
    Postfix { base: Box<Node>, steps: Vec<Step> },
    /// A free-function call (`size(x)` · `has(x)`).
    Call {
        name: String,
        arg: Box<Node>,
        span: (usize, usize),
    },
    /// Logical NOT (`!x`).
    Not(Box<Node>),
    /// A relation (`a <relop> b` · at most one per grammar).
    Rel {
        op: RelOp,
        lhs: Box<Node>,
        rhs: Box<Node>,
        span: (usize, usize),
    },
    /// Short-circuit AND (`a && b`).
    And(Box<Node>, Box<Node>),
    /// Short-circuit OR (`a || b`).
    Or(Box<Node>, Box<Node>),
    /// Conditional value selection (`cond ? a : b`).
    Ternary {
        cond: Box<Node>,
        then: Box<Node>,
        otherwise: Box<Node>,
    },
}

/// A parsed `cel-subset/0.1` expression (the AST root + the source span
/// it spans · the public handle).
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub(crate) root: Node,
}

impl Expr {
    pub(crate) fn new(root: Node) -> Self {
        Self { root }
    }

    /// The internal root node (for the computer).
    pub(crate) fn node(&self) -> &Node {
        &self.root
    }

    /// Static `when:`-shape (spec §when · side-constraint 5): an
    /// expression is boolean-SHAPED iff its top form is a relation, a
    /// boolean operator (`&&`/`||`/`!`), a `has()` call, a `bool`
    /// literal, or a ternary whose branches are themselves boolean.
    /// A bare reference, a bare non-bool literal, `size()`, or a list
    /// is NOT boolean-shaped → the checker raises NIKA-VAR-005.
    #[must_use]
    pub fn is_boolean_shaped(&self) -> bool {
        Self::node_is_boolean_shaped(&self.root)
    }

    fn node_is_boolean_shaped(node: &Node) -> bool {
        match node {
            Node::Rel { .. }
            | Node::And(..)
            | Node::Or(..)
            | Node::Not(_)
            | Node::Lit(Value::Bool(_)) => true,
            Node::Call { name, .. } => name == "has",
            Node::Ternary {
                then, otherwise, ..
            } => Self::node_is_boolean_shaped(then) && Self::node_is_boolean_shaped(otherwise),
            _ => false,
        }
    }

    /// Every namespace ROOT this expression references, in first-seen
    /// order, deduplicated (the checker validates `tasks.<id>` ids +
    /// deep paths against the declared shapes).
    #[must_use]
    pub fn roots(&self) -> Vec<String> {
        let mut acc = Vec::new();
        Self::collect_roots(&self.root, &mut acc);
        acc
    }

    fn collect_roots(node: &Node, acc: &mut Vec<String>) {
        match node {
            Node::Root { name, .. } => {
                if !acc.iter().any(|r| r == name) {
                    acc.push(name.clone());
                }
            }
            Node::Lit(_) => {}
            Node::List(items) => {
                for n in items {
                    Self::collect_roots(n, acc);
                }
            }
            Node::Postfix { base, steps } => {
                Self::collect_roots(base, acc);
                for step in steps {
                    match step {
                        Step::Index(n) => Self::collect_roots(n, acc),
                        Step::Method { args, .. } => {
                            for a in args {
                                Self::collect_roots(a, acc);
                            }
                        }
                        Step::Field(_) => {}
                    }
                }
            }
            Node::Call { arg, .. } => Self::collect_roots(arg, acc),
            Node::Not(n) => Self::collect_roots(n, acc),
            Node::Rel { lhs, rhs, .. } => {
                Self::collect_roots(lhs, acc);
                Self::collect_roots(rhs, acc);
            }
            Node::And(a, b) | Node::Or(a, b) => {
                Self::collect_roots(a, acc);
                Self::collect_roots(b, acc);
            }
            Node::Ternary {
                cond,
                then,
                otherwise,
            } => {
                Self::collect_roots(cond, acc);
                Self::collect_roots(then, acc);
                Self::collect_roots(otherwise, acc);
            }
        }
    }
}
