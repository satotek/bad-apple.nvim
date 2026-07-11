local paths = require("bad-apple.paths")

local M = {}

local function target()
  local os = vim.uv.os_uname().sysname
  local architecture = vim.uv.os_uname().machine
  local targets = {
    Darwin = {
      arm64 = "aarch64-apple-darwin",
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
  return target
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
    error("curl is required to download the player and source PV")
  end
  if vim.fn.executable("ffmpeg") ~= 1 then
    error("ffmpeg is required for the first local media conversion")
  end

  local directory = paths.data_dir()
  vim.fn.mkdir(directory .. "/bin", "p")
  local engine = paths.installed_engine()
  local encoder = paths.installed_encoder()
  local source = paths.source_video()
  local movie = paths.installed_movie()
  local audio = paths.installed_audio()
  local release_base = assert(options.release_base, "release_base is required"):gsub("/$", "")

  local release_target = target()
  vim.notify("bad-apple.nvim: installing player binaries...")
  if force or vim.fn.executable(engine) ~= 1 then
    download(release_base .. "/bav-engine-" .. release_target, engine)
    assert(vim.uv.fs_chmod(engine, 493))
  end
  if force or vim.fn.executable(encoder) ~= 1 then
    download(release_base .. "/bav-encode-" .. release_target, encoder)
    assert(vim.uv.fs_chmod(encoder, 493))
  end
  if vim.fn.filereadable(source) ~= 1 then
    vim.notify("bad-apple.nvim: downloading the source PV for local conversion...")
    download(assert(options.source_url, "source_url is required"), source)
  end
  if force or vim.fn.filereadable(movie) ~= 1 or vim.fn.filereadable(audio) ~= 1 then
    vim.notify("bad-apple.nvim: generating local movie and audio assets...")
    local result = vim.system({ encoder, "--video", source, movie, audio }, { text = true }):wait()
    if result.code ~= 0 then
      error(vim.trim(result.stderr ~= "" and result.stderr or "local media conversion failed"))
    end
  end
  vim.fn.writefile({ tostring(options.asset_version) }, paths.installed_version())
  vim.notify("bad-apple.nvim: local installation complete", vim.log.levels.INFO)
end

return M
