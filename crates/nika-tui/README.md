# nika-tui

Terminal UI for Nika workflow engine.

## Overview

This crate provides the interactive terminal interface:

- **4-View Architecture** - Chat, Home, Studio, Monitor
- **VS Code-like Navigation** - Tab bar, Alt+arrows, Ctrl+P fuzzy search
- **Real-time Streaming** - Live LLM token display
- **YAML Editor** - Syntax highlighting, live validation

## Architecture

```
nika-tui/
├── lib.rs          # Entry points (run_tui_standalone, run_tui_chat, run_tui_studio)
├── app.rs          # Main App state machine
├── state.rs        # AppState, view management
├── mode.rs         # AppMode enum
├── layout.rs       # Terminal layout calculation
├── theme.rs        # Colors, styles
├── keybindings.rs  # Keyboard handling
├── views/          # 4 main views
│   ├── chat.rs     # Chat view (agent conversation)
│   ├── home.rs     # Home view (workflow browser)
│   ├── studio.rs   # Studio view (YAML editor)
│   └── monitor.rs  # Monitor view (execution observer)
├── panels/         # Reusable UI panels
├── widgets/        # Custom ratatui widgets
└── standalone.rs   # Standalone TUI runner
```

## Views

| Key | View | Description |
|-----|------|-------------|
| `a` | Chat | Conversational AI agent |
| `h` | Home | Browse and select workflows |
| `s` | Studio | Edit YAML with live validation |
| `m` | Monitor | Real-time execution observer |

## Keybindings

| Key | Action |
|-----|--------|
| `Tab` | Navigate between views |
| `Alt+←/→` | Navigate tabs |
| `Alt+W` | Close tab |
| `Ctrl+P` / `/` | Fuzzy file search |
| `?` | Show help |
| `q` | Quit |

## Usage

```rust
use nika_tui::{run_tui_standalone, run_tui_chat, run_tui_studio};

// Launch full TUI
run_tui_standalone().await?;

// Launch chat mode
run_tui_chat(Some("claude"), Some("claude-3-sonnet")).await?;

// Launch studio with workflow
run_tui_studio(Some(PathBuf::from("workflow.yaml"))).await?;
```

## License

MIT
