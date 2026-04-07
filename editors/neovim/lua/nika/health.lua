-- Health check for the Nika Neovim plugin.
-- Run with :checkhealth nika

local M = {}

--- Parse a semver string into {major, minor, patch} or nil.
---@param version_str string
---@return table|nil
local function parse_version(version_str)
  local major, minor, patch = version_str:match('(%d+)%.(%d+)%.(%d+)')
  if major then
    return {
      major = tonumber(major),
      minor = tonumber(minor),
      patch = tonumber(patch),
    }
  end
  return nil
end

--- Compare two version tables. Returns -1, 0, or 1.
---@param a table
---@param b table
---@return integer
local function compare_versions(a, b)
  if a.major ~= b.major then return a.major < b.major and -1 or 1 end
  if a.minor ~= b.minor then return a.minor < b.minor and -1 or 1 end
  if a.patch ~= b.patch then return a.patch < b.patch and -1 or 1 end
  return 0
end

function M.check()
  vim.health.start('Nika workflow engine')

  -- 1. Check nika binary
  local nika_path = vim.fn.exepath('nika')
  if nika_path == '' then
    vim.health.error(
      '`nika` binary not found in PATH',
      {
        'Install via Homebrew: brew install supernovae-st/tap/nika',
        'Install via cargo: cargo install nika',
        'Or download from https://github.com/supernovae-st/nika/releases',
      }
    )
    return
  end
  vim.health.ok('`nika` found at ' .. nika_path)

  -- 2. Check version
  local version_output = vim.fn.system({ 'nika', 'version' })
  local version_str = version_output:match('[Vv]?(%d+%.%d+%.%d+)')
  if version_str then
    local version = parse_version(version_str)
    local min_version = parse_version('0.63.0')
    if version and min_version and compare_versions(version, min_version) < 0 then
      vim.health.warn(
        'nika v' .. version_str .. ' found (v0.63.0+ recommended for full LSP support)',
        { 'Upgrade: brew upgrade nika  OR  cargo install nika' }
      )
    else
      vim.health.ok('nika v' .. version_str)
    end
  else
    vim.health.warn(
      'Could not parse nika version from output',
      { 'Output was: ' .. vim.trim(version_output) }
    )
  end

  -- 3. Check LSP feature
  local features_output = vim.fn.system({ 'nika', 'features' })
  if vim.v.shell_error ~= 0 then
    vim.health.warn(
      '`nika features` command not available',
      { 'LSP feature check skipped. Ensure nika was built with the lsp feature.' }
    )
  else
    if features_output:match('lsp') then
      vim.health.ok('LSP feature compiled in')
    else
      vim.health.error(
        'nika binary was built without LSP support',
        {
          'Reinstall with: cargo install nika --features lsp',
          'Or install via Homebrew (includes LSP by default): brew install supernovae-st/tap/nika',
        }
      )
    end
  end

  -- 4. Test LSP startup
  local lsp_help = vim.fn.system({ 'nika', 'lsp', '--help' })
  if vim.v.shell_error == 0 then
    vim.health.ok('`nika lsp --help` responds (LSP server functional)')
  else
    vim.health.error(
      '`nika lsp --help` failed',
      {
        'Ensure nika was compiled with --features lsp',
        'Error: ' .. vim.trim(lsp_help),
      }
    )
  end

  -- 5. Check nvim-lspconfig availability
  local has_lspconfig = pcall(require, 'lspconfig')
  if has_lspconfig then
    vim.health.ok('nvim-lspconfig available')
  else
    vim.health.warn(
      'nvim-lspconfig not installed',
      {
        'LSP will not auto-attach without nvim-lspconfig.',
        'Install: https://github.com/neovim/nvim-lspconfig',
      }
    )
  end

  -- 6. Check tree-sitter YAML parser
  local has_ts = pcall(require, 'nvim-treesitter')
  if has_ts then
    local parsers = require('nvim-treesitter.parsers')
    if parsers.has_parser and parsers.has_parser('yaml') then
      vim.health.ok('Tree-sitter YAML parser installed')
    else
      vim.health.warn(
        'Tree-sitter YAML parser not installed',
        { 'Install: :TSInstall yaml' }
      )
    end
  else
    -- Neovim 0.10+ ships with built-in tree-sitter; check directly
    local ok = pcall(vim.treesitter.language.inspect, 'yaml')
    if ok then
      vim.health.ok('Tree-sitter YAML parser available')
    else
      vim.health.info(
        'Tree-sitter YAML parser not detected (syntax highlighting will fall back to regex)'
      )
    end
  end
end

return M
