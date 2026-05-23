(function()
  local ok, telescope = pcall(require, "telescope")
  if not ok then
    return
  end

  local ok_config, config = pcall(require, "telescope.config")
  local current = ok_config and config.values.file_ignore_patterns or {}
  table.insert(current, "^docs/plans/")

  telescope.setup({
    defaults = {
      file_ignore_patterns = current,
    },
  })
end)()
