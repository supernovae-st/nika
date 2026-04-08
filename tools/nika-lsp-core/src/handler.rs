// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Unified handler trait for LSP delegation.
//!
//! Both `nika lsp` (embedded) and `nika-lsp` (standalone) delegate
//! ALL handler calls through this trait. The [`DefaultHandler`]
//! implementation wires the pure functions from the `handlers` module.
//!
//! ## Usage
//!
//! ```ignore
//! let handler = DefaultHandler::new();
//! let hover = handler.hover(text, offset, &context);
//! let items = handler.completion(text, offset, &context);
//! ```

use crate::analysis::context::CursorContext;
use crate::handlers::{
    code_action::CodeActionEntry, completion, definition::DefinitionResult, hover::HoverResult,
    semantic_tokens::RawToken, symbols::SymbolEntry,
};

/// Protocol-agnostic LSP handler trait.
///
/// All methods take text + offset/context and return result types
/// that the tower-lsp shim converts to `ls_types::*`.
///
/// This trait exists so both binaries share a single dispatch point.
/// Future implementations could add AST caching or model intelligence.
pub trait LspHandler: Send + Sync {
    /// Compute completions at the given offset.
    fn completion(
        &self,
        text: &str,
        offset: u32,
        context: &CursorContext,
    ) -> Vec<ls_types::CompletionItem>;

    /// Compute hover documentation at the given offset.
    fn hover(&self, text: &str, offset: u32, context: &CursorContext) -> Option<HoverResult>;

    /// Find the definition of the element at the given offset.
    fn definition(
        &self,
        text: &str,
        offset: u32,
        context: &CursorContext,
    ) -> Option<DefinitionResult>;

    /// Compute code actions for the given range.
    fn code_actions(&self, text: &str, start: u32, end: u32) -> Vec<CodeActionEntry>;

    /// Compute code actions including diagnostic-linked quick fixes.
    fn code_actions_with_diagnostics(
        &self,
        text: &str,
        start: u32,
        end: u32,
        diagnostics: &[crate::handlers::code_action::DiagnosticInfo],
    ) -> Vec<CodeActionEntry>;

    /// Compute semantic tokens for the full document.
    fn semantic_tokens(&self, text: &str) -> Vec<RawToken>;

    /// Compute document symbols (outline) for the document.
    fn symbols(&self, text: &str) -> Vec<SymbolEntry>;

    /// Compute code lenses for the document.
    fn code_lenses(&self, text: &str) -> Vec<crate::handlers::code_lens::CodeLensEntry>;

    /// Compute inlay hints for the given range.
    fn inlay_hints(
        &self,
        text: &str,
        start: u32,
        end: u32,
    ) -> Vec<crate::handlers::inlay_hints::InlayHintEntry>;

    /// Compute folding ranges for the document.
    fn folding_ranges(&self, text: &str) -> Vec<crate::handlers::folding_ranges::FoldEntry>;

    /// Find all document links (URLs, file paths, include references).
    fn document_links(&self, text: &str) -> Vec<crate::handlers::document_links::LinkEntry>;

    /// Find the task ID at the given byte offset in the document.
    fn find_task_at_offset(&self, text: &str, offset: u32) -> Option<String>;

    /// Find all references to a task ID in the document.
    fn references(
        &self,
        text: &str,
        task_id: &str,
    ) -> Vec<crate::handlers::references::ReferenceEntry>;
}

// ═══════════════════════════════════════════════════════════════════════════
// DefaultHandler — wires the pure functions from handlers::*
// ═══════════════════════════════════════════════════════════════════════════

/// Default implementation that delegates to the pure handler functions.
///
/// This is the standard handler used by both binaries. No AST cache,
/// no model intelligence — just the pure text-based handlers.
#[derive(Debug, Default)]
pub struct DefaultHandler;

impl DefaultHandler {
    pub fn new() -> Self {
        Self
    }
}

impl LspHandler for DefaultHandler {
    fn completion(
        &self,
        text: &str,
        offset: u32,
        context: &CursorContext,
    ) -> Vec<ls_types::CompletionItem> {
        completion::completions(text, offset, context, None)
    }

    fn hover(&self, text: &str, offset: u32, context: &CursorContext) -> Option<HoverResult> {
        crate::handlers::hover::hover(text, offset, context, None)
    }

    fn definition(
        &self,
        text: &str,
        offset: u32,
        context: &CursorContext,
    ) -> Option<DefinitionResult> {
        crate::handlers::definition::definition(text, offset, context)
    }

    fn code_actions(&self, text: &str, start: u32, end: u32) -> Vec<CodeActionEntry> {
        crate::handlers::code_action::code_actions(text, start, end)
    }

    fn code_actions_with_diagnostics(
        &self,
        text: &str,
        start: u32,
        end: u32,
        diagnostics: &[crate::handlers::code_action::DiagnosticInfo],
    ) -> Vec<CodeActionEntry> {
        crate::handlers::code_action::code_actions_with_diagnostics(text, start, end, diagnostics)
    }

    fn semantic_tokens(&self, text: &str) -> Vec<RawToken> {
        crate::handlers::semantic_tokens::semantic_tokens(text)
    }

    fn symbols(&self, text: &str) -> Vec<SymbolEntry> {
        crate::handlers::symbols::document_symbols(text)
    }

    fn code_lenses(&self, text: &str) -> Vec<crate::handlers::code_lens::CodeLensEntry> {
        crate::handlers::code_lens::code_lenses(text, None)
    }

    fn inlay_hints(
        &self,
        text: &str,
        start: u32,
        end: u32,
    ) -> Vec<crate::handlers::inlay_hints::InlayHintEntry> {
        crate::handlers::inlay_hints::inlay_hints(text, start, end)
    }

    fn folding_ranges(&self, text: &str) -> Vec<crate::handlers::folding_ranges::FoldEntry> {
        crate::handlers::folding_ranges::folding_ranges(text)
    }

    fn document_links(&self, text: &str) -> Vec<crate::handlers::document_links::LinkEntry> {
        crate::handlers::document_links::document_links(text)
    }

    fn find_task_at_offset(&self, text: &str, offset: u32) -> Option<String> {
        crate::handlers::references::find_task_at_offset(text, offset)
    }

    fn references(
        &self,
        text: &str,
        task_id: &str,
    ) -> Vec<crate::handlers::references::ReferenceEntry> {
        crate::handlers::references::find_task_references(text, task_id)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
schema: \"@0.12\"
workflow: test
provider: anthropic

tasks:
  - id: step1
    infer: \"Generate a title\"

  - id: step2
    with:
      data: $step1
    exec: \"echo {{with.data}}\"
    depends_on: [step1]
";

    #[test]
    fn default_handler_constructs() {
        let _h = DefaultHandler::new();
    }

    #[test]
    fn default_handler_hover_verb() {
        let h = DefaultHandler::new();
        let ctx = CursorContext::VerbBlock {
            task_id: None,
            verb: "infer".to_string(),
            existing_subfields: vec![],
            prefix: String::new(),
        };
        let r = h.hover(SAMPLE, 0, &ctx);
        assert!(r.is_some());
        assert!(r.unwrap().contents.contains("LLM Generation"));
    }

    #[test]
    fn default_handler_definition() {
        let h = DefaultHandler::new();
        let ctx = CursorContext::DependsOn {
            task_id: None,
            existing_deps: vec![],
            prefix: "step1".into(),
        };
        let r = h.definition(SAMPLE, 0, &ctx);
        assert!(r.is_some());
    }

    #[test]
    fn default_handler_code_actions_schema() {
        let h = DefaultHandler::new();
        let actions = h.code_actions("workflow: test\n", 0, 0);
        assert!(actions.iter().any(|a| a.title.contains("schema")));
    }

    #[test]
    fn default_handler_semantic_tokens() {
        let h = DefaultHandler::new();
        let tokens = h.semantic_tokens(SAMPLE);
        assert!(!tokens.is_empty());
        // Should find at least: schema, workflow, tasks keywords + infer, exec verbs
        assert!(tokens.len() >= 5);
    }

    #[test]
    fn default_handler_symbols() {
        let h = DefaultHandler::new();
        let syms = h.symbols(SAMPLE);
        // Should have schema, workflow, provider, tasks
        assert!(syms.len() >= 3);
        let tasks = syms.iter().find(|s| s.name.contains("tasks")).unwrap();
        assert_eq!(tasks.children.len(), 2); // step1, step2
    }

    #[test]
    fn default_handler_completion() {
        let h = DefaultHandler::new();
        let ctx = CursorContext::WorkflowRoot {
            prefix: "sc".into(),
        };
        let items = h.completion(SAMPLE, 0, &ctx);
        // Should suggest root-level keys
        assert!(!items.is_empty());
    }

    #[test]
    fn trait_is_object_safe() {
        // Verify LspHandler can be used as dyn trait object
        fn _accepts_handler(_h: &dyn LspHandler) {}
        let h = DefaultHandler::new();
        _accepts_handler(&h);
    }
}
