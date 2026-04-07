-- Buffer-local settings for Nika workflow files (yaml.nika filetype).
-- Only activates when the buffer name matches *.nika.yaml.

local fname = vim.fn.expand('%:t')
if not fname:match('%.nika%.yaml$') and not fname:match('%.nika%.yml$') then
  return
end

-- Indentation: YAML standard 2-space
vim.bo.tabstop = 2
vim.bo.shiftwidth = 2
vim.bo.softtabstop = 2
vim.bo.expandtab = true

-- Folding via tree-sitter (YAML parser handles block nodes)
vim.wo.foldmethod = 'expr'
vim.wo.foldexpr = 'v:lua.vim.treesitter.foldexpr()'
vim.wo.foldlevel = 99 -- start unfolded

-- Comment string
vim.bo.commentstring = '# %s'

-- Conceal level for cleaner template display
vim.wo.conceallevel = 0
