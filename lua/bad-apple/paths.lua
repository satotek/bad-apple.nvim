local M = {}

function M.root()
  local source = vim.api.nvim_get_runtime_file("lua/bad-apple/paths.lua", false)[1]
  return source and vim.fs.dirname(vim.fs.dirname(vim.fs.dirname(source))) or nil
end

function M.data_dir()
  return vim.fn.stdpath("data") .. "/bad-apple.nvim"
end

function M.installed_engine()
  return M.data_dir() .. "/bin/bav-engine"
end

function M.installed_movie()
  return M.data_dir() .. "/movie.bav"
end

function M.installed_audio()
  return M.data_dir() .. "/audio.mp3"
end

function M.installed_version()
  return M.data_dir() .. "/asset-version"
end

function M.is_current(version)
  if vim.fn.filereadable(M.installed_version()) ~= 1 then
    return false
  end
  local lines = vim.fn.readfile(M.installed_version(), "", 1)
  return lines[1] == tostring(version)
end

function M.resolve_engine(configured)
  if configured and vim.fn.executable(configured) == 1 then
    return vim.fn.expand(configured)
  end
  if vim.fn.executable(M.installed_engine()) == 1 then
    return M.installed_engine()
  end
  local path_engine = vim.fn.exepath("bav-engine")
  if path_engine ~= "" then
    return path_engine
  end
  local root = M.root()
  if root then
    for _, profile in ipairs({ "release", "debug" }) do
      local candidate = root .. "/target/" .. profile .. "/bav-engine"
      if vim.fn.executable(candidate) == 1 then
        return candidate
      end
    end
  end
  return nil
end

function M.resolve_movie(configured)
  local candidate = configured and vim.fn.expand(configured) or M.installed_movie()
  return vim.fn.filereadable(candidate) == 1 and candidate or nil
end

function M.resolve_audio(configured)
  local candidate = configured and vim.fn.expand(configured) or M.installed_audio()
  return vim.fn.filereadable(candidate) == 1 and candidate or nil
end

return M
