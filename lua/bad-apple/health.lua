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
    vim.health.error("bav-engine was not found", { "Run :BadApple to install release assets" })
  end

  local movie = paths.resolve_movie()
  if movie then
    vim.health.ok("movie: " .. movie)
  else
    vim.health.error("movie.bav was not found", { "Run :BadApple to install release assets" })
  end

  local audio = paths.resolve_audio()
  if audio then
    vim.health.ok("audio: " .. audio)
  else
    vim.health.error("audio.mp3 was not found", { "Run :BadApple to install release assets" })
  end

  if vim.fn.executable("curl") == 1 then
    vim.health.ok("curl is available for first-run installation")
  else
    vim.health.warn("curl is unavailable", { "Install it to download the player and source PV" })
  end

  if vim.fn.executable("ffmpeg") == 1 then
    vim.health.ok("ffmpeg is available for local media generation")
  else
    vim.health.warn("ffmpeg is unavailable", { "Install it before the first :BadApple run" })
  end
end

return M
