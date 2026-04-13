# TUI Guide -- Nika Terminal UI

Nika includes a full-featured terminal user interface (TUI) built with ratatui. The TUI provides real-time workflow execution monitoring, a file browser with syntax highlighting, an AI chat interface, and configuration management -- all from your terminal.

## Launching the TUI

```bash
# Default (Studio view)
nika ui

# With a specific view
nika ui --view=command
nika ui --view=control

# With a workflow preloaded
nika ui workflow.nika.yaml

# Shortcuts
nika studio                            # Opens Studio view
nika studio workflow.nika.yaml         # Studio with file loaded
```

## The Three Views

The TUI has three primary views, each accessible by pressing the corresponding key:

| Key | View | Purpose |
|-----|------|---------|
| `1` or `s` | Studio | File browser + YAML editor + DAG preview |
| `2` or `c` | Command | Real-time execution monitoring |
| `3` or `x` | Control | Provider config, theme, preferences |

### Studio View (1/s)

The Studio is your primary workspace for browsing, editing, and understanding workflows.

**Layout:**
- **Left panel** -- File browser showing `.nika.yaml` files in the project
- **Center panel** -- YAML editor with syntax highlighting
- **Right panel** -- DAG visualization showing task dependencies

**Key features:**
- Browse project files with arrow keys
- View DAG structure as an ASCII diagram
- See task count, provider, and schema at a glance
- Run the selected workflow directly from the editor

**Keyboard shortcuts in Studio:**

| Key | Action |
|-----|--------|
| `Enter` | Open selected file |
| `r` | Run current workflow |
| `c` | Validate (check) current workflow |
| `Tab` | Switch focus between panels |
| `q` | Quit |

### Command View (2/c)

The Command view shows real-time workflow execution with live progress.

**Layout:**
- **Top** -- Workflow header (name, schema, provider)
- **Center** -- Task execution list with status, duration, and output
- **Bottom** -- Summary statistics

**Task status indicators:**
- `[~]` -- Running (animated)
- `[+]` -- Completed successfully
- `[x]` -- Failed
- `[o]` -- Skipped (dependency failed)
- `[>]` -- Retrying

**Key features:**
- Live streaming output as tasks execute
- Real-time DAG progress visualization
- Token count and cost tracking
- Error details with NIKA-XXX codes

### Control View (3/x)

The Control view provides configuration management and provider status.

**Key features:**
- Provider status display
- API key management
- Theme selection (dark/light)
- Editor preferences
- Trace configuration

**Keyboard shortcuts in Control:**

| Key | Action |
|-----|--------|
| `Up/Down` | Navigate settings |
| `Enter` | Toggle/select option |
| `Esc` | Close view |

## Global Keyboard Shortcuts

These shortcuts work in any view:

| Key | Action |
|-----|--------|
| `1` or `s` | Switch to Studio |
| `2` or `c` | Switch to Command |
| `3` or `x` | Switch to Control |
| `?` | Show help overlay |
| `q` | Quit (with confirmation if running) |
| `Ctrl+C` | Force quit |

## Running Workflows from the TUI

1. Open the TUI: `nika ui`
2. Navigate to a `.nika.yaml` file in the Studio file browser
3. Press `Enter` to load it
4. Press `r` to run
5. The view switches to Command automatically

Or run directly:

```bash
nika ui workflow.nika.yaml
```

## TUI vs Headless

| Feature | TUI (`nika ui`) | Headless (`nika run`) |
|---------|:---------------:|:---------------------:|
| Real-time progress | Yes | Basic (text output) |
| File browser | Yes | No |
| DAG visualization | Yes | No |
| Multiple workflows | Yes (switch files) | One per invocation |
| CI/CD friendly | No | Yes |
| Scriptable output | No | Yes (`--detail json`) |

**Recommendation:**
- Use **TUI** for development, exploration, and interactive work
- Use **headless** for automation, CI/CD, and scripting

## Configuration

### Theme

The TUI supports cosmic themes. Configure via Settings view or:

```bash
nika config set editor.theme dark
```

### Default View

Set which view opens by default:

```bash
nika config set tui.default_view studio
```

## Common TUI Workflows

### Workflow Development Cycle

The most common TUI workflow is the edit-check-run cycle:

1. Launch Studio: `nika studio workflow.nika.yaml`
2. Browse and review the DAG visualization (right panel)
3. Press `c` to validate the workflow
4. Press `r` to execute
5. View results in the Command view
6. Press `1` or `s` to return to Studio for edits
7. Repeat

### Monitoring Long-Running Workflows

For workflows with many tasks or long-running agent loops:

1. Launch: `nika ui workflow.nika.yaml`
2. Press `r` to start execution
3. The Command view shows live progress
4. Scroll through completed tasks to review output
5. Watch token counts and timing in real-time
6. If a task fails, error details appear immediately

### Provider Configuration

From the Control view (`3` or `x`):

1. View all configured providers and their status
2. See which API keys are detected
3. Switch the default provider for the session
4. Toggle theme between dark and light

## TUI Architecture

The TUI is built with [ratatui](https://github.com/ratatui/ratatui), a Rust terminal rendering library. It provides:

- **60fps rendering** for smooth animations and live updates
- **Unicode support** for box-drawing characters and status icons
- **True color** support on modern terminals
- **Responsive layout** that adapts to terminal size
- **Mouse support** for scrolling and panel interaction (terminal-dependent)

### Terminal Requirements

The TUI works best with:
- A modern terminal emulator (iTerm2, Alacritty, Kitty, WezTerm, Windows Terminal)
- Minimum 80x24 character size (120x40 recommended)
- True color support (most modern terminals)
- A monospace font with Unicode support

### Fallback to Headless

If the TUI is not compiled in (e.g., minimal build without the `tui` feature), Nika falls back to headless mode automatically. The CLI commands `nika run` and `nika check` always work regardless of TUI availability.

## Tips

1. **Quick workflow testing** -- Use `nika studio file.yaml` to open directly in the editor, then press `r` to run
2. **Chat for prototyping** -- Use `nika chat` to quickly test prompts before putting them in a workflow
3. **Provider switching** -- Change providers in Settings without restarting
4. **Watch mode** -- The file browser auto-refreshes when files change on disk
5. **Large output** -- The Runner view scrolls automatically. Use arrow keys to review past output
6. **Terminal size** -- Resize your terminal for better layout. The TUI adapts automatically
7. **Copy output** -- Most terminals let you select and copy text from the TUI output
8. **Side-by-side** -- Run `nika ui` in one terminal pane and your editor in another for the best development experience
