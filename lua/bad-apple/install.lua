local paths = require("bad-apple.paths")

local M = {}

local function engine_asset()
  local os = vim.uv.os_uname().sysname
  local architecture = vim.uv.os_uname().machine
  local targets = {
    Darwin = {
      arm64 = "aarch64-apple-darwin",
      x86_64 = "x86_64-apple-darwin",
    },
    Linux = {
      aarch64 = "aarch64-unknown-linux-gnu",
      arm64 = "aarch64-unknown-linux-gnu",
      x86_64 = "x86_64-unknown-linux-gnu",
    },
  }
  local target = targets[os] and targets[os][architecture]
  if not target then
    error(string.format("unsupported platform: %s/%s", os, architecture))
  end
  return "bav-engine-" .. target
end

local function download(url, destination)
  local temporary = destination .. ".download"
  local result = vim.system({ "curl", "-fL", "--retry", "3", "-o", temporary, url }, { text = true }):wait()
  if result.code ~= 0 then
    vim.uv.fs_unlink(temporary)
    error(vim.trim(result.stderr ~= "" and result.stderr or "download failed: " .. url))
  end
  assert(vim.uv.fs_rename(temporary, destination))
end

function M.install(force, options)
  if vim.fn.executable("curl") ~= 1 then
    error("curl is required by :BadAppleInstall")
  end

  local directory = paths.data_dir()
  vim.fn.mkdir(directory .. "/bin", "p")
  local engine = paths.installed_engine()
  local movie = paths.installed_movie()
  local audio = paths.installed_audio()
  local release_base = assert(options.release_base, "release_base is required"):gsub("/$", "")

  vim.notify("bad-apple.nvim: installing release assets...")
  if force or vim.fn.executable(engine) ~= 1 then
    download(release_base .. "/" .. engine_asset(), engine)
    assert(vim.uv.fs_chmod(engine, 493))
  end
  if force or vim.fn.filereadable(movie) ~= 1 then
    download(release_base .. "/movie.bav", movie)
  end
  if force or vim.fn.filereadable(audio) ~= 1 then
    download(release_base .. "/audio.mp3", audio)
  end
  vim.notify("bad-apple.nvim: installation complete", vim.log.levels.INFO)
end

return M
