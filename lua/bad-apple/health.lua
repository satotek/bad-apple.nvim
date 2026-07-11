local paths = require("bad-apple.paths")

local M = {}

function M.check()
  vim.health.start("bad-apple.nvim")

  if vim.fn.has("nvim-0.11") == 1 then
    vim.health.ok("Neovim 0.11 or newer")
  else
    vim.health.error("Neovim 0.11 or newer is required")
  end

  local engine = paths.resolve_engine()
  if engine then
    vim.health.ok("bav-engine: " .. engine)
  else
    vim.health.error("bav-engine was not found", { "Run :BadAppleInstall" })
  end

  local movie = paths.resolve_movie()
  if movie then
    vim.health.ok("movie: " .. movie)
  else
    vim.health.error("movie.bav was not found", { "Run :BadAppleInstall" })
  end

  if vim.fn.executable("curl") == 1 then
    vim.health.ok("curl is available for release installation")
  else
    vim.health.warn("curl is unavailable", { "Install assets manually or configure engine_path and movie_path" })
  end
end

return M
