---
name: nika-setup
description: Guide the user through complete Nika setup on their machine. Install binary, configure editors, set up API keys, initialize project. Use for first-time setup or migration.
user-invocable: true
allowed-tools: Bash, Read, Write, Edit, Glob, Grep
argument-hint: "[install | editor | keys | project | all]"
---

# Nika Setup Guide

> Complete machine setup for Nika workflow engine.

## Process

### Step 1: Detect Current State

Run these checks to understand what is already set up:

```bash
# OS and architecture
uname -sm

# Package managers
which brew 2>/dev/null && echo "Homebrew: installed" || echo "Homebrew: not found"
which cargo 2>/dev/null && echo "Cargo: installed" || echo "Cargo: not found"

# Existing Nika installation
which nika 2>/dev/null && nika --version 2>/dev/null || echo "Nika: not installed"

# Editors
which code 2>/dev/null && echo "VS Code: installed" || echo "VS Code: not found"
which cursor 2>/dev/null && echo "Cursor: installed" || echo "Cursor: not found"
which zed 2>/dev/null && echo "Zed: installed" || echo "Zed: not found"
which nvim 2>/dev/null && echo "Neovim: installed" || echo "Neovim: not found"
which hx 2>/dev/null && echo "Helix: installed" || echo "Helix: not found"

# API keys (env only, no keychain)
for KEY in ANTHROPIC_API_KEY OPENAI_API_KEY MISTRAL_API_KEY GROQ_API_KEY DEEPSEEK_API_KEY GEMINI_API_KEY XAI_API_KEY; do
  eval "VAL=\$$KEY"
  [ -n "$VAL" ] && echo "$KEY: set" || echo "$KEY: not set"
done
```

### Step 2: Install Nika Binary

Based on the detected platform, guide through installation:

#### Option A: Homebrew (recommended for macOS)

```bash
brew tap supernovae-st/tap
brew install nika
```

#### Option B: Cargo (cross-platform)

```bash
cargo install nika
```

#### Option C: Pre-built Binary (GitHub Release)

```bash
# macOS (Apple Silicon)
curl -L https://github.com/supernovae-st/nika/releases/latest/download/nika-aarch64-apple-darwin.tar.gz | tar xz
sudo mv nika /usr/local/bin/

# macOS (Intel)
curl -L https://github.com/supernovae-st/nika/releases/latest/download/nika-x86_64-apple-darwin.tar.gz | tar xz
sudo mv nika /usr/local/bin/

# Linux (x86_64)
curl -L https://github.com/supernovae-st/nika/releases/latest/download/nika-x86_64-unknown-linux-gnu.tar.gz | tar xz
sudo mv nika /usr/local/bin/
```

Verify installation:

```bash
nika --version
nika features
```

### Step 3: Editor Integration

#### VS Code / Cursor

The Nika LSP provides diagnostics, completions, hover, and go-to-definition for `.nika.yaml` files.

Add to `.vscode/settings.json`:

```json
{
  "files.associations": {
    "*.nika.yaml": "yaml"
  },
  "yaml.schemas": {
    "https://raw.githubusercontent.com/supernovae-st/nika/main/schemas/workflow-0.12.json": "*.nika.yaml"
  }
}
```

For LSP integration (if compiled with `--features lsp`), add to `.vscode/settings.json`:

```json
{
  "[nika-yaml]": {
    "editor.formatOnSave": false
  }
}
```

#### Neovim (nvim-lspconfig)

Add to Neovim LSP configuration:

```lua
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')

if not configs.nika then
  configs.nika = {
    default_config = {
      cmd = { 'nika', 'lsp', '--mode', 'stdio' },
      filetypes = { 'yaml' },
      root_dir = lspconfig.util.root_pattern('.nika', '.nika.yaml'),
      settings = {},
    },
  }
end

lspconfig.nika.setup({
  on_attach = function(client, bufnr)
    -- Only activate for .nika.yaml files
    local filename = vim.api.nvim_buf_get_name(bufnr)
    if not filename:match('%.nika%.yaml$') then
      vim.lsp.buf_detach_client(bufnr, client.id)
    end
  end,
})
```

#### Helix

Add to `~/.config/helix/languages.toml`:

```toml
[[language]]
name = "nika-yaml"
scope = "source.nika"
injection-regex = "nika"
file-types = [{ glob = "*.nika.yaml" }]
language-servers = ["nika-lsp"]
grammar = "yaml"

[language-server.nika-lsp]
command = "nika"
args = ["lsp", "--mode", "stdio"]
```

#### Zed

Add to Zed settings:

```json
{
  "lsp": {
    "nika-lsp": {
      "binary": {
        "path": "nika",
        "arguments": ["lsp", "--mode", "stdio"]
      }
    }
  },
  "file_types": {
    "YAML": ["nika.yaml"]
  }
}
```

### Step 4: API Key Configuration

Guide through setting up at least one LLM provider:

```bash
# Recommended: set via environment variable in shell profile
echo 'export ANTHROPIC_API_KEY="sk-ant-..."' >> ~/.zshrc
source ~/.zshrc

# Alternative: use Nika's OS keychain storage
# WARNING: triggers macOS Keychain popup on first use
# nika keys set anthropic
```

### Step 5: Initialize Project

```bash
# Option 1: Minimal scaffold (5 workflows, 1 per verb)
nika init --minimal

# Option 2: Full interactive setup
nika init

# Option 3: Learning course (44 exercises across 12 levels)
nika init --course
```

### Step 6: Verify Setup

```bash
# Run doctor to confirm everything works
nika doctor --full

# Validate generated workflows
nika check .

# Run a test workflow (exec only, no API key needed)
nika run examples/hello.nika.yaml
```

## Troubleshooting

| Issue | Fix |
|-------|-----|
| `nika: command not found` | Add cargo bin to PATH: `export PATH="$HOME/.cargo/bin:$PATH"` |
| LSP not working | Confirm `nika lsp --help` works; recompile with `--features lsp` |
| Keychain popup | Use env vars instead: `export ANTHROPIC_API_KEY=...` |
| `.nika.yaml` not highlighted | Add file association in editor settings |

## Rules

- NEVER trigger macOS Keychain popups during setup
- ALWAYS verify installation with `nika --version` after install
- ALWAYS run `nika doctor` as final verification
- DETECT existing setup before suggesting changes
- ASK before modifying shell profile files (.zshrc, .bashrc)
