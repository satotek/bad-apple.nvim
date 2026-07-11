local Player = require("bad-apple.player")
local paths = require("bad-apple.paths")

local M = {}

local defaults = {
  engine_path = nil,
  movie_path = nil,
  audio_path = nil,
  release_base = "https://github.com/satotek/bad-apple.nvim/releases/latest/download",
}

local options = vim.deepcopy(defaults)
local player = nil

function M.setup(user_options)
  options = vim.tbl_deep_extend("force", defaults, user_options or {})
  vim.api.nvim_set_hl(0, "BadAppleOverlayLight", { fg = "#ffffff" })
  vim.api.nvim_set_hl(0, "BadAppleOverlayDark", { fg = "#111111" })
end

function M.overlay()
  if player then
    M.stop()
    return
  end
  if not paths.resolve_engine(options.engine_path)
    or not paths.resolve_movie(options.movie_path)
    or not paths.resolve_audio(options.audio_path)
  then
    M.install(false)
  end
  player = Player.new(options)
  player:start(nil, true)
end

function M.play(source)
  if not source
    and (not paths.resolve_engine(options.engine_path)
      or not paths.resolve_movie(options.movie_path)
      or not paths.resolve_audio(options.audio_path))
  then
    M.install(false)
  end
  if player then
    player:stop()
  end
  player = Player.new(options)
  player:start(source)
end

function M.stop()
  if player then
    player:stop()
    player = nil
  end
end

function M.toggle_pause()
  if player then
    player:toggle_pause()
  end
end

function M.install(force)
  require("bad-apple.install").install(force, options)
end

return M
