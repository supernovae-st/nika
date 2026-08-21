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
    PositionEncodingKind, RenameOptions, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind, WorkDoneProgressOptions,
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
        // W1: a task's identity is its map key — server-side rename moves
        // the key + every depends_on entry + every `tasks.<id>` island ref
        // atomically. prepareRename gates the gesture to identity sites.
        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: WorkDoneProgressOptions::default(),
        })),
        // v0.2 surface: quickfix-only — the `check --fix` rename engine
        // projected (one fix engine, every editor).
        code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
            code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]),
            ..CodeActionOptions::default()
        })),
        // Custom extensions, capability-gated the rust-analyzer way —
        // a client (or agent) reads this to know the oracle surface.
        // `graphFormat` IS the in-payload version of the projection the
        // document carries (spec 03 §graph-projection · `nika_graph::
        // GRAPH_FORMAT`) — derived, never retyped: a literal here said 2
        // while the served document said 3 (2026-08-18), and a client
        // that honours the advertisement decides adoption on it.
        experimental: Some(serde_json::json!({
            "nika": { "semanticDocument": { "graphFormat": nika_graph::GRAPH_FORMAT } }
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

    /// The experimental block advertises the oracle surface — a client
    /// discovers `nika/semanticDocument` (and its `graphFormat`) here
    /// instead of probing blind.
    #[test]
    fn experimental_advertises_the_semantic_document() {
        let caps = server_capabilities();
        let exp = caps.experimental.expect("experimental block");
        // The advertisement IS the projection's own number — pinned to the
        // constant AND to the value the spec names, so a bump of either
        // without the other fails here, not in a client.
        assert_eq!(
            exp["nika"]["semanticDocument"]["graphFormat"],
            nika_graph::GRAPH_FORMAT
        );
        assert_eq!(exp["nika"]["semanticDocument"]["graphFormat"], 3);
    }

    #[test]
    fn scope_pins_hold_v02_in_the_rest_out() {
        let caps = server_capabilities();
        // v0.2: quickfix-only code actions (the --fix engine projected).
        let Some(CodeActionProviderCapability::Options(opts)) = caps.code_action_provider else {
            panic!("code actions advertise as Options");
        };
        assert_eq!(opts.code_action_kinds, Some(vec![CodeActionKind::QUICKFIX]));
        // W1: rename with prepare support (the map key is the identity).
        let Some(OneOf::Right(rename)) = caps.rename_provider else {
            panic!("rename advertises as Options with prepare");
        };
        assert_eq!(rename.prepare_provider, Some(true));
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
