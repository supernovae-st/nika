use zed_extension_api::{self as zed, Command, ContextServerId, Extension, LanguageServerId, Result, Worktree};

/// Nika extension for Zed.
///
/// Provides:
/// - **LSP**: diagnostics, completions, hover, go-to-definition, semantic tokens
/// - **MCP Context Server**: AI agent can validate, run, and scaffold Nika workflows
#[derive(Default)]
struct NikaExtension {
    /// Cached binary path after first successful PATH discovery.
    cached_binary_path: Option<String>,
}

impl NikaExtension {
    /// Find the `nika` binary on PATH (cached after first lookup).
    fn find_nika_binary(&mut self, worktree: &Worktree) -> Result<String> {
        if let Some(path) = &self.cached_binary_path {
            return Ok(path.clone());
        }

        // Prefer dedicated nika-lsp, fall back to main nika binary.
        let path = worktree
            .which("nika-lsp")
            .or_else(|| worktree.which("nika"))
            .ok_or_else(|| {
                "Could not find nika or nika-lsp on PATH. \
                 Install via: brew install supernovae-st/tap/nika"
                    .to_string()
            })?;

        self.cached_binary_path = Some(path.clone());
        Ok(path)
    }

    /// Check if the cached binary is the dedicated `nika-lsp` (no subcommand needed).
    fn is_dedicated_lsp(&self) -> bool {
        self.cached_binary_path
            .as_ref()
            .is_some_and(|p| p.ends_with("nika-lsp"))
    }
}

impl Extension for NikaExtension {
    fn new() -> Self {
        Self::default()
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command> {
        let env = worktree.shell_env();
        let binary = self.find_nika_binary(worktree)?;

        let args = if self.is_dedicated_lsp() {
            vec![]
        } else {
            vec!["lsp".into(), "--stdio".into()]
        };

        Ok(Command {
            command: binary,
            args,
            env,
        })
    }

    fn context_server_command(
        &mut self,
        _context_server_id: &ContextServerId,
        worktree: &Worktree,
    ) -> Result<Command> {
        let env = worktree.shell_env();

        // MCP server always uses the main `nika` binary (not nika-lsp).
        let binary = worktree.which("nika").ok_or_else(|| {
            "Could not find nika on PATH for MCP server. \
             Install via: brew install supernovae-st/tap/nika"
                .to_string()
        })?;

        Ok(Command {
            command: binary,
            args: vec!["mcp".into()],
            env,
        })
    }

    // No language_server_initialization_options override needed.
    // The Nika LSP server uses sensible defaults when no init options are provided:
    //   validation.enabled = true, completion.providers = true,
    //   completion.mcpTools = true, diagnostics.delay = 300ms.
}

zed::register_extension!(NikaExtension);
