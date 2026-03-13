//! Editor Definitions
//!
//! Defines supported editors and their configuration paths.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Supported editor types for MCP configuration sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EditorKind {
    /// Claude Code (Anthropic's official CLI)
    ClaudeCode,
    /// Cursor AI editor
    Cursor,
    /// Windsurf AI editor
    Windsurf,
    /// VS Code with MCP extension
    VsCode,
}

impl EditorKind {
    /// Get all supported editor kinds.
    pub fn all() -> &'static [EditorKind] {
        &[
            EditorKind::ClaudeCode,
            EditorKind::Cursor,
            EditorKind::Windsurf,
            EditorKind::VsCode,
        ]
    }

    /// Get the display name for this editor.
    pub fn display_name(&self) -> &'static str {
        match self {
            EditorKind::ClaudeCode => "Claude Code",
            EditorKind::Cursor => "Cursor",
            EditorKind::Windsurf => "Windsurf",
            EditorKind::VsCode => "VS Code",
        }
    }

    /// Get the CLI identifier for this editor.
    pub fn id(&self) -> &'static str {
        match self {
            EditorKind::ClaudeCode => "claude-code",
            EditorKind::Cursor => "cursor",
            EditorKind::Windsurf => "windsurf",
            EditorKind::VsCode => "vscode",
        }
    }

    /// Parse from a string identifier.
    pub fn from_id(id: &str) -> Option<EditorKind> {
        match id.to_lowercase().as_str() {
            "claude-code" | "claude" | "cc" => Some(EditorKind::ClaudeCode),
            "cursor" => Some(EditorKind::Cursor),
            "windsurf" => Some(EditorKind::Windsurf),
            "vscode" | "vs-code" | "code" => Some(EditorKind::VsCode),
            _ => None,
        }
    }

    /// Get the config directory name for this editor.
    fn config_dir_name(&self) -> &'static str {
        match self {
            EditorKind::ClaudeCode => ".claude",
            EditorKind::Cursor => ".cursor",
            EditorKind::Windsurf => ".windsurf",
            EditorKind::VsCode => ".vscode",
        }
    }

    /// Get the config filename for MCP servers.
    fn mcp_config_filename(&self) -> &'static str {
        match self {
            // Claude Code uses settings.json with mcp.servers key
            EditorKind::ClaudeCode => "settings.json",
            // Others use dedicated mcp.json
            EditorKind::Cursor => "mcp.json",
            EditorKind::Windsurf => "mcp.json",
            EditorKind::VsCode => "mcp.json",
        }
    }
}

/// Editor configuration with resolved paths.
#[derive(Debug, Clone)]
pub struct Editor {
    /// The kind of editor.
    pub kind: EditorKind,
    /// Whether the editor is installed (config dir exists).
    pub installed: bool,
    /// Path to the editor's config directory.
    pub config_dir: PathBuf,
    /// Path to the MCP config file.
    pub mcp_config_path: PathBuf,
}

impl Editor {
    /// Create an Editor instance by detecting installation.
    ///
    /// If the home directory cannot be determined (e.g., in containers),
    /// the editor is marked as not installed with empty paths.
    pub fn detect(kind: EditorKind) -> Self {
        match Self::resolve_config_dir(kind) {
            Some(config_dir) => {
                let installed = config_dir.exists();
                let mcp_config_path = config_dir.join(kind.mcp_config_filename());
                Self {
                    kind,
                    installed,
                    config_dir,
                    mcp_config_path,
                }
            }
            None => {
                // Graceful degradation: mark as not installed if home dir unavailable
                Self {
                    kind,
                    installed: false,
                    config_dir: PathBuf::new(),
                    mcp_config_path: PathBuf::new(),
                }
            }
        }
    }

    /// Detect all supported editors.
    pub fn detect_all() -> Vec<Editor> {
        EditorKind::all()
            .iter()
            .map(|&k| Editor::detect(k))
            .collect()
    }

    /// Detect only installed editors.
    pub fn detect_installed() -> Vec<Editor> {
        Self::detect_all()
            .into_iter()
            .filter(|e| e.installed)
            .collect()
    }

    /// Resolve the config directory for an editor.
    ///
    /// Returns None if the home directory cannot be determined (e.g., in containers).
    fn resolve_config_dir(kind: EditorKind) -> Option<PathBuf> {
        dirs::home_dir().map(|home| home.join(kind.config_dir_name()))
    }

    /// Check if MCP config file exists.
    pub fn has_mcp_config(&self) -> bool {
        self.mcp_config_path.exists()
    }

    /// Get the display name.
    pub fn display_name(&self) -> &'static str {
        self.kind.display_name()
    }

    /// Get the CLI identifier.
    pub fn id(&self) -> &'static str {
        self.kind.id()
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_editor_kind_all() {
        let all = EditorKind::all();
        assert_eq!(all.len(), 4);
        assert!(all.contains(&EditorKind::ClaudeCode));
        assert!(all.contains(&EditorKind::Cursor));
        assert!(all.contains(&EditorKind::Windsurf));
        assert!(all.contains(&EditorKind::VsCode));
    }

    #[test]
    fn test_editor_kind_display_name() {
        assert_eq!(EditorKind::ClaudeCode.display_name(), "Claude Code");
        assert_eq!(EditorKind::Cursor.display_name(), "Cursor");
        assert_eq!(EditorKind::Windsurf.display_name(), "Windsurf");
        assert_eq!(EditorKind::VsCode.display_name(), "VS Code");
    }

    #[test]
    fn test_editor_kind_id() {
        assert_eq!(EditorKind::ClaudeCode.id(), "claude-code");
        assert_eq!(EditorKind::Cursor.id(), "cursor");
        assert_eq!(EditorKind::Windsurf.id(), "windsurf");
        assert_eq!(EditorKind::VsCode.id(), "vscode");
    }

    #[test]
    fn test_editor_kind_from_id() {
        // Exact matches
        assert_eq!(
            EditorKind::from_id("claude-code"),
            Some(EditorKind::ClaudeCode)
        );
        assert_eq!(EditorKind::from_id("cursor"), Some(EditorKind::Cursor));
        assert_eq!(EditorKind::from_id("windsurf"), Some(EditorKind::Windsurf));
        assert_eq!(EditorKind::from_id("vscode"), Some(EditorKind::VsCode));

        // Aliases
        assert_eq!(EditorKind::from_id("claude"), Some(EditorKind::ClaudeCode));
        assert_eq!(EditorKind::from_id("cc"), Some(EditorKind::ClaudeCode));
        assert_eq!(EditorKind::from_id("vs-code"), Some(EditorKind::VsCode));
        assert_eq!(EditorKind::from_id("code"), Some(EditorKind::VsCode));

        // Case insensitive
        assert_eq!(
            EditorKind::from_id("CLAUDE-CODE"),
            Some(EditorKind::ClaudeCode)
        );
        assert_eq!(EditorKind::from_id("CURSOR"), Some(EditorKind::Cursor));

        // Unknown
        assert_eq!(EditorKind::from_id("unknown"), None);
        assert_eq!(EditorKind::from_id(""), None);
    }

    #[test]
    fn test_editor_kind_config_dir_name() {
        assert_eq!(EditorKind::ClaudeCode.config_dir_name(), ".claude");
        assert_eq!(EditorKind::Cursor.config_dir_name(), ".cursor");
        assert_eq!(EditorKind::Windsurf.config_dir_name(), ".windsurf");
        assert_eq!(EditorKind::VsCode.config_dir_name(), ".vscode");
    }

    #[test]
    fn test_editor_kind_mcp_config_filename() {
        assert_eq!(
            EditorKind::ClaudeCode.mcp_config_filename(),
            "settings.json"
        );
        assert_eq!(EditorKind::Cursor.mcp_config_filename(), "mcp.json");
        assert_eq!(EditorKind::Windsurf.mcp_config_filename(), "mcp.json");
        assert_eq!(EditorKind::VsCode.mcp_config_filename(), "mcp.json");
    }

    #[test]
    fn test_editor_detect() {
        // Just verify it doesn't panic
        let editor = Editor::detect(EditorKind::ClaudeCode);
        assert_eq!(editor.kind, EditorKind::ClaudeCode);
        assert!(editor.config_dir.ends_with(".claude"));
        assert!(editor.mcp_config_path.ends_with("settings.json"));
    }

    #[test]
    fn test_editor_detect_all() {
        let editors = Editor::detect_all();
        assert_eq!(editors.len(), 4);
    }

    #[test]
    fn test_editor_serde_roundtrip() {
        let kind = EditorKind::ClaudeCode;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, "\"claude-code\"");

        let parsed: EditorKind = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, kind);
    }

    #[test]
    fn test_all_editor_kinds_serde() {
        for kind in EditorKind::all() {
            let json = serde_json::to_string(kind).unwrap();
            let parsed: EditorKind = serde_json::from_str(&json).unwrap();
            assert_eq!(&parsed, kind);
        }
    }
}
