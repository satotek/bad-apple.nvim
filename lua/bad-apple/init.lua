local Player = require("bad-apple.player")
local paths = require("bad-apple.paths")

local M = {}

local defaults = {
  engine_path = nil,
  movie_path = nil,
  audio_path = nil,
  asset_version = 2,
  release_base = "https://github.com/satotek/bad-apple.nvim/releases/latest/download",
}

local options = vim.deepcopy(defaults)
local player = nil

function M.setup(user_options)
  options = vim.tbl_deep_extend("force", defaults, user_options or {})
end

function M.play(source)
  if not source and not options.engine_path and not options.movie_path and not options.audio_path then
    local stale = not paths.is_current(options.asset_version)
    local missing = not paths.resolve_engine() or not paths.resolve_movie() or not paths.resolve_audio()
    if stale or missing then
      M.install(stale)
    end
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
