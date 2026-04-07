-- Filetype detection for Nika workflow files (*.nika.yaml)
-- Sets filetype to "yaml.nika" so YAML base support is preserved
-- while Nika-specific ftplugin and LSP attach on the compound filetype.

vim.filetype.add({
  pattern = {
    ['.*%.nika%.yaml'] = 'yaml.nika',
    ['.*%.nika%.yml'] = 'yaml.nika',
  },
})
