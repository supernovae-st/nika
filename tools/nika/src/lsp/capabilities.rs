//! LSP Server Capabilities
//!
//! Defines what features the Nika language server supports.

#[cfg(feature = "lsp")]
use tower_lsp::lsp_types::*;

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
                ".".to_string(), // Binding paths (use.alias.field)
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
        references_provider: None,
        document_highlight_provider: None,
        document_formatting_provider: None,
        document_range_formatting_provider: None,
        rename_provider: None,
        folding_range_provider: None,
        selection_range_provider: None,
        linked_editing_range_provider: None,
        semantic_tokens_provider: None,
        moniker_provider: None,
        inlay_hint_provider: None,
        inline_value_provider: None,
        type_definition_provider: None,
        implementation_provider: None,
        color_provider: None,
        execute_command_provider: None,
        call_hierarchy_provider: None,
        code_lens_provider: None,
        document_link_provider: None,
        document_on_type_formatting_provider: None,
        declaration_provider: None,
        workspace_symbol_provider: None,
        experimental: None,
        position_encoding: None,
        diagnostic_provider: None,
        // Note: inline_completion_provider and notebook_document_sync removed
        // (not available in lsp-types 0.94.x used by tower-lsp 0.20)
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
