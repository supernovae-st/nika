// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The v0.1 [`ServerCapabilities`] the server advertises at initialize.
//!
//! Exactly the v0.1 scope (per the spec, LOCKED) ·
//! - **positionEncoding** UTF-16 (the LSP default — `LineIndex` matches).
//! - **textDocumentSync** FULL (single-file, full-reparse-on-change).
//! - **hoverProvider**, **completionProvider**, **definitionProvider**,
//!   **documentSymbolProvider** — the four read features.
//! - **publishDiagnostics** is a server→client notification (not a
//!   capability field) — it needs no advertisement.
//!
//! Everything OUT of v0.1 scope (code actions, inlay hints, semantic
//! tokens, …) is simply left `None`.

use lsp_types::{
    CompletionOptions, OneOf, PositionEncodingKind, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind,
};

/// The capabilities advertised in the initialize response.
#[must_use]
pub fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        position_encoding: Some(PositionEncodingKind::UTF16),
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        hover_provider: Some(true.into()),
        completion_provider: Some(CompletionOptions {
            // we complete on a leading `.` (for `tasks.`) and `/` (for
            // `provider/`) plus the normal identifier triggers
            trigger_characters: Some(vec![".".to_owned(), "/".to_owned()]),
            ..CompletionOptions::default()
        }),
        definition_provider: Some(OneOf::Left(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        ..ServerCapabilities::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advertises_utf16_and_full_sync() {
        let caps = server_capabilities();
        assert_eq!(
            caps.position_encoding,
            Some(PositionEncodingKind::UTF16),
            "UTF-16 default"
        );
        assert!(matches!(
            caps.text_document_sync,
            Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL))
        ));
    }

    #[test]
    fn advertises_the_four_read_features() {
        let caps = server_capabilities();
        assert!(caps.hover_provider.is_some());
        assert!(caps.completion_provider.is_some());
        assert!(matches!(caps.definition_provider, Some(OneOf::Left(true))));
        assert!(matches!(
            caps.document_symbol_provider,
            Some(OneOf::Left(true))
        ));
    }

    #[test]
    fn completion_advertises_the_dot_and_slash_trigger_characters() {
        // The completion provider must declare `.` and `/` as trigger
        // characters (for `tasks.` and `provider/`). Dropping the field
        // would silently disable trigger-character completion in clients.
        let caps = server_capabilities();
        let completion = caps.completion_provider.expect("completion provider");
        assert_eq!(
            completion.trigger_characters,
            Some(vec![".".to_owned(), "/".to_owned()]),
            "exactly `.` and `/` as triggers"
        );
    }

    #[test]
    fn out_of_scope_features_are_absent() {
        let caps = server_capabilities();
        assert!(caps.code_action_provider.is_none());
        assert!(caps.inlay_hint_provider.is_none());
        assert!(caps.semantic_tokens_provider.is_none());
    }

    #[test]
    fn capabilities_serialize_to_json() {
        // the initialize handshake passes these as a serde_json::Value
        let caps = server_capabilities();
        let value = serde_json::to_value(&caps).expect("serializes");
        assert_eq!(value["positionEncoding"], "utf-16");
        assert_eq!(value["textDocumentSync"], 1); // FULL
    }
}
