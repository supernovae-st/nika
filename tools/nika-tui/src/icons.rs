// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! # Unified Icon System for Nika TUI 🦋
//!
//! Centralized icon definitions to ensure consistency and avoid conflicts.
//!
//! ## Design Principles
//!
//! 1. **No Conflicts**: Each icon is unique within its category
//! 2. **ASCII Fallback**: Every emoji has a terminal-safe ASCII alternative
//! 3. **Semantic Mapping**: Icons match their conceptual meaning
//! 4. **Variation Selectors**: Consistent emoji rendering with U+FE0F
//!
//! ## Categories
//!
//! - **Verbs**: 5 semantic verbs (infer, exec, fetch, invoke, agent)
//! - **Providers**: LLM providers (Claude, OpenAI, Mistral, etc.)
//! - **Status**: Task states (pending, running, success, failed)
//! - **UI**: Navigation, actions, indicators
//! - **Space**: Cosmic theme elements (🦋🌌)

// ═══════════════════════════════════════════════════════════════════════════════
// VERB ICONS (5 Semantic Verbs)
// ═══════════════════════════════════════════════════════════════════════════════

/// Verb icon definitions
///
/// Each verb has a distinct visual identity.
pub mod verb {
    /// Infer verb - LLM text generation (Violet ⚡)
    pub const INFER: &str = "⚡";
    pub const INFER_ASCII: &str = "[I]";

    /// Exec verb - Shell command execution (Amber 📟)
    pub const EXEC: &str = "📟";
    pub const EXEC_ASCII: &str = "[X]";

    /// Fetch verb - HTTP request (Cyan 🛰️)
    /// Note: Uses variation selector U+FE0F for consistent rendering
    pub const FETCH: &str = "🛰️";
    pub const FETCH_ASCII: &str = "[F]";

    /// Invoke verb - MCP tool call (Emerald 🔌)
    pub const INVOKE: &str = "🔌";
    pub const INVOKE_ASCII: &str = "[V]";

    /// Agent verb - Multi-turn agentic loop (Rose 🐔)
    pub const AGENT: &str = "🐔";
    pub const AGENT_ASCII: &str = "[A]";

    /// Subagent - Spawned via spawn_agent (🐤)
    pub const SUBAGENT: &str = "🐤";
    pub const SUBAGENT_ASCII: &str = "[a]";

    /// User - User message in chat (👤)
    pub const USER: &str = "👤";
    pub const USER_ASCII: &str = "[U]";

    /// Get icon for verb name
    pub fn icon(verb: &str) -> &'static str {
        match verb.to_lowercase().as_str() {
            "infer" => INFER,
            "exec" => EXEC,
            "fetch" => FETCH,
            "invoke" => INVOKE,
            "agent" => AGENT,
            "subagent" | "spawn" => SUBAGENT,
            _ => "•",
        }
    }

    /// Get ASCII icon for verb name
    pub fn icon_ascii(verb: &str) -> &'static str {
        match verb.to_lowercase().as_str() {
            "infer" => INFER_ASCII,
            "exec" => EXEC_ASCII,
            "fetch" => FETCH_ASCII,
            "invoke" => INVOKE_ASCII,
            "agent" => AGENT_ASCII,
            "subagent" | "spawn" => SUBAGENT_ASCII,
            _ => "[?]",
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// PROVIDER ICONS
// ═══════════════════════════════════════════════════════════════════════════════

/// LLM Provider icons
///
/// Each provider has a unique icon (no conflicts with verbs).
pub mod provider {
    /// Claude (Anthropic) - Brain 🧠
    pub const CLAUDE: &str = "🧠";
    pub const CLAUDE_ASCII: &str = "[C]";

    /// OpenAI - Robot 🤖
    pub const OPENAI: &str = "🤖";
    pub const OPENAI_ASCII: &str = "[O]";

    /// Mistral - Wind 💨
    pub const MISTRAL: &str = "💨";
    pub const MISTRAL_ASCII: &str = "[M]";

    /// Gemini - Gem 💎
    pub const GEMINI: &str = "💎";
    pub const GEMINI_ASCII: &str = "[Gm]";

    /// Native (Local) - Butterfly 🦋
    pub const NATIVE: &str = "🦋";
    pub const NATIVE_ASCII: &str = "[N]";

    /// Groq - Timer ⏱️ (Changed from ⚡ to avoid conflict with infer:)
    pub const GROQ: &str = "⏱️";
    pub const GROQ_ASCII: &str = "[G]";

    /// DeepSeek - Deep ocean 🌊
    pub const DEEPSEEK: &str = "🌊";
    pub const DEEPSEEK_ASCII: &str = "[D]";

    /// Mock provider (testing) - Test tube 🧪
    pub const MOCK: &str = "🧪";
    pub const MOCK_ASCII: &str = "[T]";

    /// Get icon for provider name
    pub fn icon(provider: &str) -> &'static str {
        match provider.to_lowercase().as_str() {
            "claude" | "anthropic" => CLAUDE,
            "openai" | "gpt" => OPENAI,
            "mistral" => MISTRAL,
            "gemini" => GEMINI,
            "native" => NATIVE,
            "groq" => GROQ,
            "deepseek" => DEEPSEEK,
            "mock" | "test" => MOCK,
            _ => "•",
        }
    }

    /// Get ASCII icon for provider name
    pub fn icon_ascii(provider: &str) -> &'static str {
        match provider.to_lowercase().as_str() {
            "claude" | "anthropic" => CLAUDE_ASCII,
            "openai" | "gpt" => OPENAI_ASCII,
            "mistral" => MISTRAL_ASCII,
            "gemini" => GEMINI_ASCII,
            "native" => NATIVE_ASCII,
            "groq" => GROQ_ASCII,
            "deepseek" => DEEPSEEK_ASCII,
            "mock" | "test" => MOCK_ASCII,
            _ => "[?]",
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// STATUS ICONS
// ═══════════════════════════════════════════════════════════════════════════════

/// Task status icons
pub mod status {
    /// Pending - Hourglass ⏳
    pub const PENDING: &str = "⏳";
    pub const PENDING_ASCII: &str = "[ ]";

    /// Running - Spinner ⟳ (or animated)
    pub const RUNNING: &str = "⟳";
    pub const RUNNING_ASCII: &str = "[~]";

    /// Success - Checkmark ✓
    pub const SUCCESS: &str = "✓";
    pub const SUCCESS_ASCII: &str = "[+]";

    /// Failed - Cross ✗
    pub const FAILED: &str = "✗";
    pub const FAILED_ASCII: &str = "[!]";

    /// Paused - Pause bars ⏸
    pub const PAUSED: &str = "⏸";
    pub const PAUSED_ASCII: &str = "[-]";

    /// Skipped - Skip arrow ⏭
    pub const SKIPPED: &str = "⏭";
    pub const SKIPPED_ASCII: &str = "[>]";

    /// Get icon for status name
    pub fn icon(status: &str) -> &'static str {
        match status.to_lowercase().as_str() {
            "pending" | "queued" | "waiting" => PENDING,
            "running" | "active" | "in_progress" => RUNNING,
            "success" | "completed" | "done" | "passed" => SUCCESS,
            "failed" | "error" | "crashed" => FAILED,
            "paused" | "suspended" => PAUSED,
            "skipped" | "cancelled" => SKIPPED,
            _ => "•",
        }
    }

    /// Get ASCII icon for status name
    pub fn icon_ascii(status: &str) -> &'static str {
        match status.to_lowercase().as_str() {
            "pending" | "queued" | "waiting" => PENDING_ASCII,
            "running" | "active" | "in_progress" => RUNNING_ASCII,
            "success" | "completed" | "done" | "passed" => SUCCESS_ASCII,
            "failed" | "error" | "crashed" => FAILED_ASCII,
            "paused" | "suspended" => PAUSED_ASCII,
            "skipped" | "cancelled" => SKIPPED_ASCII,
            _ => "[?]",
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// UI ICONS
// ═══════════════════════════════════════════════════════════════════════════════

/// UI element icons
pub mod ui {
    // Navigation
    pub const ARROW_UP: &str = "▲";
    pub const ARROW_DOWN: &str = "▼";
    pub const ARROW_LEFT: &str = "◀";
    pub const ARROW_RIGHT: &str = "▶";
    pub const CHEVRON_RIGHT: &str = "›";
    pub const CHEVRON_DOWN: &str = "˅";

    // Tree structure
    pub const TREE_BRANCH: &str = "├";
    pub const TREE_LAST: &str = "└";
    pub const TREE_PIPE: &str = "│";
    pub const TREE_SPACE: &str = " ";

    // Folders/Files
    pub const FOLDER_CLOSED: &str = "📁";
    pub const FOLDER_OPEN: &str = "📂";
    pub const FILE: &str = "📄";
    pub const FILE_YAML: &str = "📋";
    pub const FILE_CODE: &str = "📜";

    // Actions
    pub const EDIT: &str = "✎";
    pub const SAVE: &str = "💾";
    pub const DELETE: &str = "🗑";
    pub const REFRESH: &str = "↻";
    pub const SEARCH: &str = "🔍";
    pub const FILTER: &str = "⚙";
    pub const COPY: &str = "📋";
    pub const PASTE: &str = "📎";

    // Indicators
    pub const INFO: &str = "ℹ";
    pub const WARNING: &str = "⚠";
    pub const ERROR: &str = "✕";
    pub const HELP: &str = "?";
    pub const LOADING: &str = "◌";
    pub const DOT: &str = "•";
    pub const BULLET: &str = "◆";
    pub const STAR: &str = "★";
    pub const STAR_EMPTY: &str = "☆";

    // Separators
    pub const SEPARATOR_H: &str = "─";
    pub const SEPARATOR_V: &str = "│";
    pub const CORNER_TL: &str = "┌";
    pub const CORNER_TR: &str = "┐";
    pub const CORNER_BL: &str = "└";
    pub const CORNER_BR: &str = "┘";

    // MCP
    pub const MCP_CONNECTED: &str = "●";
    pub const MCP_DISCONNECTED: &str = "○";
    pub const MCP_SERVER: &str = "🔗";
    pub const MCP_TOOL: &str = "🔧";
}

// ═══════════════════════════════════════════════════════════════════════════════
// COSMIC THEME ICONS 🦋🌌
// ═══════════════════════════════════════════════════════════════════════════════

/// Cosmic theme decorative icons
pub mod cosmic {
    /// Nika brand - Butterfly 🦋
    pub const NIKA: &str = "🦋";

    /// Cosmic space - Galaxy 🌌
    pub const GALAXY: &str = "🌌";

    /// Stars
    pub const STAR: &str = "✦";
    pub const STAR_FILLED: &str = "★";
    pub const STAR_SPARKLE: &str = "✨";

    /// Space elements
    pub const ROCKET: &str = "🚀";
    pub const SATELLITE: &str = "🛰️";
    pub const MOON: &str = "🌙";
    pub const PLANET: &str = "🪐";
    pub const NEBULA: &str = "🌈";
    pub const COMET: &str = "☄️";

    /// Activity
    pub const PULSE: &str = "◉";
    pub const GLOW: &str = "◎";
    pub const ORBIT: &str = "◯";

    /// Cosmic spinners (for animations)
    pub const SPINNER_ROCKET: [&str; 4] = ["🚀", "✨", "💫", "⭐"];
    pub const SPINNER_STARS: [&str; 6] = ["✦", "✧", "★", "☆", "✵", "✶"];
    pub const SPINNER_ORBIT: [&str; 4] = ["◐", "◓", "◑", "◒"];
    pub const SPINNER_MOON: [&str; 8] = ["🌑", "🌒", "🌓", "🌔", "🌕", "🌖", "🌗", "🌘"];

    /// Get spinner frame
    pub fn spinner_frame(spinner: &[&'static str], frame: usize) -> &'static str {
        spinner[frame % spinner.len()]
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// ICON SET (Unified Access)
// ═══════════════════════════════════════════════════════════════════════════════

/// Icon mode for terminal compatibility
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconMode {
    /// Use emoji/unicode icons
    #[default]
    Unicode,
    /// Use ASCII-only fallbacks
    Ascii,
}

impl IconMode {
    /// Detect icon mode from terminal capabilities
    pub fn detect() -> Self {
        // Check for emoji support via TERM_PROGRAM or similar
        if let Ok(term) = std::env::var("TERM_PROGRAM") {
            if term.contains("iTerm")
                || term.contains("Hyper")
                || term.contains("vscode")
                || term.contains("Alacritty")
                || term.contains("kitty")
            {
                return Self::Unicode;
            }
        }

        // Check LANG for UTF-8 support
        if let Ok(lang) = std::env::var("LANG") {
            if lang.contains("UTF-8") || lang.contains("utf-8") {
                return Self::Unicode;
            }
        }

        // Fallback to ASCII for compatibility
        Self::Ascii
    }
}

/// Unified icon set that respects terminal capabilities
#[derive(Debug, Clone, Copy)]
pub struct IconSet {
    mode: IconMode,
}

impl Default for IconSet {
    fn default() -> Self {
        Self::new()
    }
}

impl IconSet {
    /// Create icon set with auto-detected mode
    pub fn new() -> Self {
        Self {
            mode: IconMode::detect(),
        }
    }

    /// Create icon set with specific mode
    pub fn with_mode(mode: IconMode) -> Self {
        Self { mode }
    }

    /// Get verb icon
    pub fn verb(&self, name: &str) -> &'static str {
        match self.mode {
            IconMode::Unicode => verb::icon(name),
            IconMode::Ascii => verb::icon_ascii(name),
        }
    }

    /// Get provider icon
    pub fn provider(&self, name: &str) -> &'static str {
        match self.mode {
            IconMode::Unicode => provider::icon(name),
            IconMode::Ascii => provider::icon_ascii(name),
        }
    }

    /// Get status icon
    pub fn status(&self, name: &str) -> &'static str {
        match self.mode {
            IconMode::Unicode => status::icon(name),
            IconMode::Ascii => status::icon_ascii(name),
        }
    }

    /// Get current mode
    pub fn mode(&self) -> IconMode {
        self.mode
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verb_icons_are_unique() {
        let icons = [
            verb::INFER,
            verb::EXEC,
            verb::FETCH,
            verb::INVOKE,
            verb::AGENT,
            verb::SUBAGENT,
        ];

        for (i, icon1) in icons.iter().enumerate() {
            for (j, icon2) in icons.iter().enumerate() {
                if i != j {
                    assert_ne!(icon1, icon2, "Verb icons must be unique");
                }
            }
        }
    }

    #[test]
    fn test_provider_icons_no_conflict_with_verbs() {
        let verb_icons = [
            verb::INFER,
            verb::EXEC,
            verb::FETCH,
            verb::INVOKE,
            verb::AGENT,
        ];
        let provider_icons = [
            provider::CLAUDE,
            provider::OPENAI,
            provider::MISTRAL,
            provider::GEMINI,
            provider::NATIVE,
            provider::GROQ,
            provider::DEEPSEEK,
        ];

        for verb in &verb_icons {
            for prov in &provider_icons {
                assert_ne!(verb, prov, "Provider icon conflicts with verb icon");
            }
        }
    }

    #[test]
    fn test_groq_icon_changed() {
        // Groq should use timer, not lightning (which is infer)
        assert_eq!(provider::GROQ, "⏱️");
        assert_ne!(provider::GROQ, verb::INFER);
    }

    #[test]
    fn test_icon_set_unicode() {
        let icons = IconSet::with_mode(IconMode::Unicode);
        assert_eq!(icons.verb("infer"), "⚡");
        assert_eq!(icons.provider("claude"), "🧠");
        assert_eq!(icons.status("success"), "✓");
    }

    #[test]
    fn test_icon_set_ascii() {
        let icons = IconSet::with_mode(IconMode::Ascii);
        assert_eq!(icons.verb("infer"), "[I]");
        assert_eq!(icons.provider("claude"), "[C]");
        assert_eq!(icons.status("success"), "[+]");
    }

    #[test]
    fn test_cosmic_spinner() {
        assert_eq!(cosmic::spinner_frame(&cosmic::SPINNER_ORBIT, 0), "◐");
        assert_eq!(cosmic::spinner_frame(&cosmic::SPINNER_ORBIT, 1), "◓");
        assert_eq!(cosmic::spinner_frame(&cosmic::SPINNER_ORBIT, 4), "◐"); // wraps
    }

    #[test]
    fn test_verb_lookup_case_insensitive() {
        assert_eq!(verb::icon("INFER"), verb::INFER);
        assert_eq!(verb::icon("Exec"), verb::EXEC);
        assert_eq!(verb::icon("fetch"), verb::FETCH);
    }
}
