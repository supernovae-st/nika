// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Workflow value declarations — the typed entries of the `inputs:` ·
//! `config:` · `const:` authorities (spec `01-envelope.md`).
//!
//! Post-R3b (LAW-GRAMMAR-0211) the `type:` field speaks the **full
//! `TypeExpr`** of `09-types.md` — the flat 6-enum (`string` · `number` ·
//! `integer` · `boolean` · `array` · `object`) is dead and `bool` is the
//! one boolean spelling (no alias — the JSON-Schema `"boolean"` lowering
//! is a machine projection, never an authorable spelling). The
//! declaration therefore carries the RAW expression, shape-only (the
//! `types:` block precedent): the grammar judgment (`NIKA-TYPE-001/006`)
//! and the default-conformance judgment (`NIKA-DEFAULT-001`) are the
//! analyzer's, via the one type core (`nika_types::types`).

use nika_source::Spanned;

/// One `inputs:` / `config:` / `const:` entry — a bare literal constant
/// (untyped) or the typed declaration form.
#[derive(Debug, Clone, PartialEq)]
pub enum VarDecl {
    /// Untyped form · `greeting: "hello"` — a bare literal (legal in
    /// `const:` only · `inputs:`/`config:` entries are typed by law,
    /// the parser refuses otherwise).
    Untyped(serde_json::Value),
    /// Typed form · `topic: { type, required, default, description }`
    /// (inputs · config) or `pi: { type, value }` (the typed constant —
    /// its `value:` rides `default`).
    Typed {
        /// The declared type — the RAW `TypeExpr` of spec 09 (shape-only ·
        /// the grammar and conformance judgments are the analyzer's).
        r#type: Spanned<serde_json::Value>,
        /// Whether the caller must provide this input (default `false`).
        required: bool,
        /// Default used when the caller omits the input · the typed
        /// constant's `value:` for `const:` entries.
        default: Option<serde_json::Value>,
        /// Human-readable description (LSP hover · callable schema).
        description: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_source::Span;

    #[test]
    fn untyped_holds_value() {
        let v = VarDecl::Untyped(serde_json::json!("./output"));
        assert!(matches!(v, VarDecl::Untyped(ref val) if val == "./output"));
    }

    #[test]
    fn typed_carries_the_raw_type_expr() {
        let v = VarDecl::Typed {
            r#type: Spanned::new(
                serde_json::json!({ "enum": ["fast", "slow"] }),
                Span::default(),
            ),
            required: true,
            default: Some(serde_json::json!("fast")),
            description: Some("Subject to research".into()),
        };
        let VarDecl::Typed {
            r#type, required, ..
        } = &v
        else {
            panic!("expected Typed");
        };
        assert!(required);
        assert!(r#type.value.is_object(), "the raw composite form rides");
    }
}
