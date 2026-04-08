// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! LSP Server Capabilities
//!
//! Defines what features the Nika language server supports.

#[cfg(feature = "lsp")]
use tower_lsp_server::ls_types::*;

#[cfg(feature = "lsp")]
use super::handlers::semantic_tokens::semantic_token_legend;

/// Build the server capabilities advertised to the client
#[cfg(feature = "lsp")]
pub fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        // Incremental sync - client sends only changed portions
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::INCREMENTAL),
                will_save: Some(false),
                will_save_wait_until: Some(false),
                save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                    include_text: Some(true),
                })),
            },
        )),

        // Completion support with trigger characters
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![
                ":".to_string(), // After verb names
                ".".to_string(), // Binding paths (with.alias.field)
                "$".to_string(), // Implicit output shorthand
                "{".to_string(), // Template start {{
            ]),
            resolve_provider: Some(false), // No lazy resolution needed
            work_done_progress_options: WorkDoneProgressOptions {
                work_done_progress: Some(false),
            },
            all_commit_characters: None,
            completion_item: None,
        }),

        // Hover for documentation
        hover_provider: Some(HoverProviderCapability::Simple(true)),

        // Go to definition for task references
        definition_provider: Some(OneOf::Left(true)),

        // Code actions for quick fixes
        code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
            code_action_kinds: Some(vec![CodeActionKind::QUICKFIX, CodeActionKind::REFACTOR]),
            work_done_progress_options: WorkDoneProgressOptions {
                work_done_progress: Some(false),
            },
            resolve_provider: Some(false),
        })),

        // Document symbols for outline
        document_symbol_provider: Some(OneOf::Left(true)),

        // Workspace capabilities
        workspace: Some(WorkspaceServerCapabilities {
            workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                supported: Some(true),
                change_notifications: Some(OneOf::Left(true)),
            }),
            file_operations: None,
        }),

        // Features we don't support yet
        signature_help_provider: None,
        references_provider: Some(OneOf::Left(true)),
        document_highlight_provider: None,
        document_formatting_provider: None,
        document_range_formatting_provider: None,
        rename_provider: None,
        folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
        selection_range_provider: None,
        linked_editing_range_provider: None,
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                legend: semantic_token_legend(),
                full: Some(SemanticTokensFullOptions::Bool(true)),
                range: None,
                work_done_progress_options: WorkDoneProgressOptions {
                    work_done_progress: Some(false),
                },
            },
        )),
        moniker_provider: None,
        inlay_hint_provider: Some(OneOf::Left(true)),
        inline_value_provider: None,
        type_definition_provider: None,
        implementation_provider: None,
        color_provider: None,
        execute_command_provider: None,
        call_hierarchy_provider: None,
        code_lens_provider: Some(CodeLensOptions {
            resolve_provider: Some(false),
        }),
        document_link_provider: Some(DocumentLinkOptions {
            resolve_provider: Some(false),
            work_done_progress_options: WorkDoneProgressOptions {
                work_done_progress: Some(false),
            },
        }),
        document_on_type_formatting_provider: None,
        declaration_provider: None,
        workspace_symbol_provider: None,
        experimental: None,
        position_encoding: None,
        diagnostic_provider: None,
        inline_completion_provider: None,
        notebook_document_sync: None,
    }
}

/// Server capabilities when LSP feature is disabled (stub)
#[cfg(not(feature = "lsp"))]
pub fn server_capabilities() -> () {
    ()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "lsp")]
    fn test_capabilities_have_completion() {
        let caps = server_capabilities();
        assert!(caps.completion_provider.is_some());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_capabilities_have_hover() {
        let caps = server_capabilities();
        assert!(caps.hover_provider.is_some());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_capabilities_have_definition() {
        let caps = server_capabilities();
        assert!(caps.definition_provider.is_some());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_capabilities_have_semantic_tokens() {
        let caps = server_capabilities();
        assert!(
            caps.semantic_tokens_provider.is_some(),
            "Should advertise semantic tokens support"
        );
        if let Some(SemanticTokensServerCapabilities::SemanticTokensOptions(opts)) =
            caps.semantic_tokens_provider
        {
            assert_eq!(opts.legend.token_types.len(), 7);
            assert_eq!(opts.legend.token_modifiers.len(), 2);
            assert!(matches!(
                opts.full,
                Some(SemanticTokensFullOptions::Bool(true))
            ));
        } else {
            panic!("Expected SemanticTokensOptions");
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_trigger_characters() {
        let caps = server_capabilities();
        if let Some(CompletionOptions {
            trigger_characters: Some(chars),
            ..
        }) = caps.completion_provider
        {
            assert!(chars.contains(&":".to_string()));
            assert!(chars.contains(&".".to_string()));
            assert!(chars.contains(&"$".to_string()));
            assert!(chars.contains(&"{".to_string()));
        } else {
            panic!("Expected completion trigger characters");
        }
    }
}
