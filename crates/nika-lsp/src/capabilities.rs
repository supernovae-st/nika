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
    CodeActionKind, CodeActionOptions, CodeActionProviderCapability, CompletionOptions, OneOf,
    PositionEncodingKind, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
};

/// The capabilities advertised in the initialize response.
#[must_use]
pub fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        position_encoding: Some(PositionEncodingKind::UTF16),
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        hover_provider: Some(true.into()),
        completion_provider: Some(CompletionOptions {
            // auto-trigger on `.` (for `tasks.`), `/` (for `provider/`),
            // `[` (for `depends_on: [`) and ` ` — the pause after a value
            // colon (`tool: ` · `model: ` · `capture: `) is the exact
            // moment an author asks "what goes here?", and clients only
            // re-pop on word characters, so without the space trigger
            // every value lane waits for a manual ctrl+space. Non-value
            // spaces answer with an empty list (pinned cheap by test).
            trigger_characters: Some(vec![
                ".".to_owned(),
                "/".to_owned(),
                "[".to_owned(),
                " ".to_owned(),
            ]),
            ..CompletionOptions::default()
        }),
        definition_provider: Some(OneOf::Left(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        // v0.2 surface: quickfix-only — the `check --fix` rename engine
        // projected (one fix engine, every editor).
        code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
            code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]),
            ..CodeActionOptions::default()
        })),
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
    fn completion_advertises_the_trigger_characters() {
        // `.` (tasks.), `/` (provider/), `[` (depends_on: [) and ` ` —
        // the pause after a value colon, where every value lane lives.
        // Dropping the field would silently disable trigger-character
        // completion in clients.
        let caps = server_capabilities();
        let completion = caps.completion_provider.expect("completion provider");
        assert_eq!(
            completion.trigger_characters,
            Some(vec![
                ".".to_owned(),
                "/".to_owned(),
                "[".to_owned(),
                " ".to_owned(),
            ]),
            "exactly `.`, `/`, `[` and ` ` as triggers"
        );
    }

    #[test]
    fn scope_pins_hold_v02_in_the_rest_out() {
        let caps = server_capabilities();
        // v0.2: quickfix-only code actions (the --fix engine projected).
        let Some(CodeActionProviderCapability::Options(opts)) = caps.code_action_provider else {
            panic!("code actions advertise as Options");
        };
        assert_eq!(opts.code_action_kinds, Some(vec![CodeActionKind::QUICKFIX]));
        // still out of scope:
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
