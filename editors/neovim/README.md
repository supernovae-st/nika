# nika.nvim

Neovim plugin for the [Nika](https://github.com/supernovae-st/nika) workflow engine.

- **LSP** -- completions, hover docs, diagnostics, go-to-definition, code actions, semantic tokens, inlay hints, code lens, rename
- **Syntax** -- Nika-specific Tree-sitter highlights on top of YAML
- **Commands** -- run, check, lint, test, explain, graph from inside Neovim
- **Keymaps** -- `<leader>n` prefix for all operations

## Requirements

- Neovim >= 0.10
- `nika` CLI in PATH (v0.63.0+, built with LSP feature)
- [nvim-lspconfig](https://github.com/neovim/nvim-lspconfig)
- Tree-sitter YAML parser (`:TSInstall yaml`)

## Installation

### lazy.nvim (recommended)

```lua
{
  'supernovae-st/nika',
  ft = { 'yaml.nika' },
  config = function()
    require('nika').setup()
  end,
  -- Plugin lives in editors/neovim/ inside the repo
  -- lazy.nvim needs to know the subdirectory:
  dir = nil,  -- remove this line if cloning from GitHub
}
```

Because the plugin is nested inside the Nika monorepo, point lazy.nvim at the
subdirectory:

```lua
{
  'supernovae-st/nika',
  name = 'nika.nvim',
  ft = { 'yaml.nika' },
  config = function()
    require('nika').setup()
  end,
  -- Tell lazy.nvim to use the neovim plugin subdirectory
  -- This requires lazy.nvim v11+ with subdir support
}
```

**Alternative -- symlink to a local path:**

```bash
# From your Neovim config directory
ln -s /path/to/nika/editors/neovim ~/.local/share/nvim/site/pack/nika/start/nika.nvim
```

Then in `init.lua`:

```lua
require('nika').setup()
```

### packer.nvim

```lua
use {
  '~/.local/share/nvim/site/pack/nika/start/nika.nvim',
  config = function()
    require('nika').setup()
  end,
  ft = { 'yaml.nika' },
}
```

### Manual

Clone or symlink the `editors/neovim/` directory into your Neovim runtime path:

```bash
mkdir -p ~/.local/share/nvim/site/pack/nika/start
ln -s /path/to/nika/editors/neovim ~/.local/share/nvim/site/pack/nika/start/nika.nvim
```

Add to your `init.lua`:

```lua
require('nika').setup()
```

## Configuration

All options and their defaults:

```lua
require('nika').setup({
  -- Enable LSP integration
  lsp = true,

  -- LSP server command
  lsp_cmd = { 'nika', 'lsp', '--stdio' },

  -- LSP settings (default: {} -- server uses sensible defaults for all features).
  -- Override only to disable features or change the diagnostics delay:
  -- lsp_settings = {
  --   nika = {
  --     validation = { enabled = false },
  --     diagnostics = { delay = 500 },
  --   },
  -- },
  lsp_settings = {},

  -- Custom on_attach (receives client, bufnr)
  on_attach = nil,

  -- Custom LSP capabilities (auto-detects nvim-cmp if nil)
  capabilities = nil,

  -- Enable default keymaps
  keymaps = true,

  -- Keymap prefix
  keymap_prefix = '<leader>n',
})
```

### Integrating with your existing on_attach

```lua
require('nika').setup({
  on_attach = function(client, bufnr)
    -- Your shared on_attach logic here
    -- e.g., format on save, set up additional keymaps
  end,
})
```

### Using with nvim-cmp

If [nvim-cmp](https://github.com/hrsh7th/nvim-cmp) and
[cmp-nvim-lsp](https://github.com/hrsh7th/cmp-nvim-lsp) are installed,
capabilities are automatically enhanced. No extra config needed.

## Keymaps

All keymaps are buffer-local, only active in `.nika.yaml` files.

| Keymap | Action | Command |
|--------|--------|---------|
| `<leader>nr` | Run workflow | `:NikaRun` |
| `<leader>nc` | Check (validate) | `:NikaCheck` |
| `<leader>nd` | Show DAG | `:NikaGraph` |
| `<leader>nl` | Lint | `:NikaLint` |
| `<leader>ne` | Explain | `:NikaExplain` |
| `<leader>nt` | Test (mock provider) | `:NikaTest` |

Change the prefix:

```lua
require('nika').setup({ keymap_prefix = '<leader>w' })
```

Or disable default keymaps and define your own:

```lua
require('nika').setup({ keymaps = false })

vim.api.nvim_create_autocmd('FileType', {
  pattern = 'yaml.nika',
  callback = function(ev)
    vim.keymap.set('n', '<leader>wr', ':NikaRun<CR>', { buffer = ev.buf })
  end,
})
```

## Commands

| Command | Description |
|---------|-------------|
| `:NikaRun` | Run the current workflow |
| `:NikaCheck` | Validate syntax + DAG |
| `:NikaGraph` | Visualize the task DAG |
| `:NikaLint` | Best-practice linting (10 rules) |
| `:NikaExplain` | Human-readable workflow summary |
| `:NikaTest` | Test with mock provider (no API calls) |
| `:NikaInfo` | Show nika version |

## LSP Features

The Nika LSP server (`nika lsp --stdio`) provides:

| Feature | Description |
|---------|-------------|
| **Completions** | 5 semantic verbs, task fields, template variables, MCP tools, provider names |
| **Hover** | Inline documentation for verbs, fields, and parameters |
| **Diagnostics** | Real-time validation with NIKA error codes and fix suggestions |
| **Go-to-definition** | Jump to task definitions from `$task_id` references |
| **Code actions** | Quick fixes for common errors |
| **Code lens** | Run/check actions inline above tasks |
| **Semantic tokens** | Rich syntax highlighting beyond Tree-sitter |
| **Inlay hints** | Type annotations for bindings and transforms |
| **Rename** | Refactor task IDs across the workflow |
| **References** | Find all usages of a task ID |

## Syntax Highlighting

The plugin extends the default YAML Tree-sitter highlights with Nika-specific
queries that colorize:

- Schema declarations
- The 5 semantic verbs (`infer`, `exec`, `fetch`, `invoke`, `agent`)
- Task IDs
- Top-level workflow keys
- Template expressions (`{{with.data | upper}}`)
- Dollar references (`$task_id`, `$env.API_KEY`)
- NIKA error codes in comments

## Health Check

```vim
:checkhealth nika
```

Verifies:
- `nika` binary in PATH
- Version >= 0.63.0
- LSP feature compiled in
- LSP server responds
- nvim-lspconfig installed
- Tree-sitter YAML parser available

## License

AGPL-3.0-or-later -- see [LICENSE](../../LICENSE).
