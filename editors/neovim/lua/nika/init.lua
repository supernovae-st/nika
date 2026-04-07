-- nika.nvim — Neovim plugin for the Nika workflow engine.
--
-- Provides:
--   - LSP integration via `nika lsp --stdio`
--   - Filetype detection for *.nika.yaml
--   - Keymaps for run, check, and DAG visualization
--   - Health check (:checkhealth nika)

local M = {}

---@class NikaConfig
---@field lsp? boolean                Enable LSP (default: true)
---@field lsp_cmd? string[]           LSP command (default: {"nika", "lsp", "--stdio"})
---@field lsp_settings? table         LSP settings (default: {} — server uses sensible defaults)
---@field on_attach? function         Custom on_attach for LSP
---@field capabilities? table         Custom LSP capabilities
---@field keymaps? boolean            Enable default keymaps (default: true)
---@field keymap_prefix? string       Keymap prefix (default: "<leader>n")

---@type NikaConfig
local defaults = {
  lsp = true,
  lsp_cmd = { 'nika', 'lsp', '--stdio' },
  -- No lsp_settings needed: the Nika LSP server uses sensible defaults when
  -- no settings are provided (validation enabled, completions with providers
  -- and MCP tools, 300ms diagnostics delay). Override here only to disable
  -- features or change the delay.
  lsp_settings = {},
  on_attach = nil,
  capabilities = nil,
  keymaps = true,
  keymap_prefix = '<leader>n',
}

---@type NikaConfig
M.config = {}

-- ─── LSP Setup ─────────────────────────────────────────────────────────────

--- Register and configure the Nika LSP server via nvim-lspconfig.
---@param cfg NikaConfig
local function setup_lsp(cfg)
  local has_lspconfig, lspconfig = pcall(require, 'lspconfig')
  if not has_lspconfig then
    vim.notify(
      '[nika.nvim] nvim-lspconfig not found. LSP will not be configured.\n'
        .. 'Install: https://github.com/neovim/nvim-lspconfig',
      vim.log.levels.WARN
    )
    return
  end

  local configs = require('lspconfig.configs')

  -- Register the server definition if not already present.
  if not configs.nika_lsp then
    configs.nika_lsp = {
      default_config = {
        cmd = cfg.lsp_cmd,
        filetypes = { 'yaml.nika' },
        root_dir = function(fname)
          return lspconfig.util.root_pattern('nika.toml', '.nika', '.git')(fname)
            or lspconfig.util.find_git_ancestor(fname)
        end,
        single_file_support = true,
        settings = cfg.lsp_settings or {},
      },
    }
  end

  -- Build capabilities (merge with user-provided or cmp defaults).
  local capabilities = cfg.capabilities
  if not capabilities then
    local has_cmp, cmp_lsp = pcall(require, 'cmp_nvim_lsp')
    if has_cmp then
      capabilities = cmp_lsp.default_capabilities()
    else
      capabilities = vim.lsp.protocol.make_client_capabilities()
    end
  end

  -- Attach handler.
  local on_attach = function(client, bufnr)
    -- Nika-specific buffer keymaps (if enabled globally).
    if cfg.keymaps then
      M._set_lsp_keymaps(bufnr, cfg.keymap_prefix)
    end

    -- Call user on_attach if provided.
    if cfg.on_attach then
      cfg.on_attach(client, bufnr)
    end
  end

  lspconfig.nika_lsp.setup({
    capabilities = capabilities,
    on_attach = on_attach,
    settings = cfg.lsp_settings,
  })
end

-- ─── Keymaps ───────────────────────────────────────────────────────────────

--- Set buffer-local keymaps for Nika workflows.
---@param bufnr integer
---@param prefix string
function M._set_lsp_keymaps(bufnr, prefix)
  local opts = function(desc)
    return { buffer = bufnr, noremap = true, silent = true, desc = 'Nika: ' .. desc }
  end

  -- Run the current workflow
  vim.keymap.set('n', prefix .. 'r', function()
    local file = vim.api.nvim_buf_get_name(bufnr)
    if file == '' then
      vim.notify('[nika] Buffer has no file path', vim.log.levels.WARN)
      return
    end
    vim.cmd('botright split | terminal nika run ' .. vim.fn.shellescape(file))
  end, opts('Run workflow'))

  -- Check (validate) the current workflow
  vim.keymap.set('n', prefix .. 'c', function()
    local file = vim.api.nvim_buf_get_name(bufnr)
    if file == '' then
      vim.notify('[nika] Buffer has no file path', vim.log.levels.WARN)
      return
    end
    vim.cmd('botright split | terminal nika check ' .. vim.fn.shellescape(file))
  end, opts('Check workflow'))

  -- Show DAG visualization
  vim.keymap.set('n', prefix .. 'd', function()
    local file = vim.api.nvim_buf_get_name(bufnr)
    if file == '' then
      vim.notify('[nika] Buffer has no file path', vim.log.levels.WARN)
      return
    end
    vim.cmd('botright split | terminal nika graph ' .. vim.fn.shellescape(file))
  end, opts('Show DAG'))

  -- Lint the current workflow
  vim.keymap.set('n', prefix .. 'l', function()
    local file = vim.api.nvim_buf_get_name(bufnr)
    if file == '' then
      vim.notify('[nika] Buffer has no file path', vim.log.levels.WARN)
      return
    end
    vim.cmd('botright split | terminal nika lint ' .. vim.fn.shellescape(file))
  end, opts('Lint workflow'))

  -- Explain the current workflow
  vim.keymap.set('n', prefix .. 'e', function()
    local file = vim.api.nvim_buf_get_name(bufnr)
    if file == '' then
      vim.notify('[nika] Buffer has no file path', vim.log.levels.WARN)
      return
    end
    vim.cmd('botright split | terminal nika explain ' .. vim.fn.shellescape(file))
  end, opts('Explain workflow'))

  -- Test with mock provider
  vim.keymap.set('n', prefix .. 't', function()
    local file = vim.api.nvim_buf_get_name(bufnr)
    if file == '' then
      vim.notify('[nika] Buffer has no file path', vim.log.levels.WARN)
      return
    end
    vim.cmd('botright split | terminal nika test ' .. vim.fn.shellescape(file))
  end, opts('Test workflow (mock)'))
end

-- ─── Commands ──────────────────────────────────────────────────────────────

--- Create user commands for Nika operations.
local function setup_commands()
  vim.api.nvim_create_user_command('NikaRun', function()
    local file = vim.api.nvim_buf_get_name(0)
    if file == '' or not file:match('%.nika%.ya?ml$') then
      vim.notify('[nika] Not a .nika.yaml file', vim.log.levels.WARN)
      return
    end
    vim.cmd('botright split | terminal nika run ' .. vim.fn.shellescape(file))
  end, { desc = 'Run current Nika workflow' })

  vim.api.nvim_create_user_command('NikaCheck', function()
    local file = vim.api.nvim_buf_get_name(0)
    if file == '' or not file:match('%.nika%.ya?ml$') then
      vim.notify('[nika] Not a .nika.yaml file', vim.log.levels.WARN)
      return
    end
    vim.cmd('botright split | terminal nika check ' .. vim.fn.shellescape(file))
  end, { desc = 'Check current Nika workflow' })

  vim.api.nvim_create_user_command('NikaGraph', function()
    local file = vim.api.nvim_buf_get_name(0)
    if file == '' or not file:match('%.nika%.ya?ml$') then
      vim.notify('[nika] Not a .nika.yaml file', vim.log.levels.WARN)
      return
    end
    vim.cmd('botright split | terminal nika graph ' .. vim.fn.shellescape(file))
  end, { desc = 'Show DAG for current Nika workflow' })

  vim.api.nvim_create_user_command('NikaLint', function()
    local file = vim.api.nvim_buf_get_name(0)
    if file == '' or not file:match('%.nika%.ya?ml$') then
      vim.notify('[nika] Not a .nika.yaml file', vim.log.levels.WARN)
      return
    end
    vim.cmd('botright split | terminal nika lint ' .. vim.fn.shellescape(file))
  end, { desc = 'Lint current Nika workflow' })

  vim.api.nvim_create_user_command('NikaExplain', function()
    local file = vim.api.nvim_buf_get_name(0)
    if file == '' or not file:match('%.nika%.ya?ml$') then
      vim.notify('[nika] Not a .nika.yaml file', vim.log.levels.WARN)
      return
    end
    vim.cmd('botright split | terminal nika explain ' .. vim.fn.shellescape(file))
  end, { desc = 'Explain current Nika workflow' })

  vim.api.nvim_create_user_command('NikaTest', function()
    local file = vim.api.nvim_buf_get_name(0)
    if file == '' or not file:match('%.nika%.ya?ml$') then
      vim.notify('[nika] Not a .nika.yaml file', vim.log.levels.WARN)
      return
    end
    vim.cmd('botright split | terminal nika test ' .. vim.fn.shellescape(file))
  end, { desc = 'Test current Nika workflow with mock provider' })

  vim.api.nvim_create_user_command('NikaInfo', function()
    local output = vim.fn.system({ 'nika', 'version' })
    vim.notify(vim.trim(output), vim.log.levels.INFO)
  end, { desc = 'Show Nika version info' })
end

-- ─── Setup ─────────────────────────────────────────────────────────────────

--- Initialize the Nika plugin.
---@param opts? NikaConfig
function M.setup(opts)
  M.config = vim.tbl_deep_extend('force', defaults, opts or {})

  -- Register filetype (idempotent; also in ftdetect/ for non-lazy scenarios).
  vim.filetype.add({
    pattern = {
      ['.*%.nika%.yaml'] = 'yaml.nika',
      ['.*%.nika%.yml'] = 'yaml.nika',
    },
  })

  -- Create user commands.
  setup_commands()

  -- Setup LSP if enabled.
  if M.config.lsp then
    setup_lsp(M.config)
  end
end

return M
