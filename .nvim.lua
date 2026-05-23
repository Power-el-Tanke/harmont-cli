local ok, telescope = pcall(require, "telescope")
if not ok then
  return
end

local current = require("telescope.config").values.file_ignore_patterns or {}
table.insert(current, "^docs/plans/")

telescope.setup({
  defaults = {
    file_ignore_patterns = current,
  },
})
